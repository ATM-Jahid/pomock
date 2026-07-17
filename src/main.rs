use std::{
    error::Error,
    fmt,
    io::{self, Stdout},
};

use crossterm::{
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyEventKind, MouseButton,
        MouseEvent, MouseEventKind,
    },
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};

use ratatui::{Terminal, backend::CrosstermBackend, layout::Rect};

use std::time::{Duration, Instant};

use pomock::{
    app::{App, AppOutcome, EditMode, FocusAudioAction, TaskState},
    config::{Config, ConfigError},
    input::map_key,
    notification::{DesktopNotifier, Notifier},
    persistence::{TaskPersistenceError, TaskStore},
    sound::{FileSoundPlayer, SoundPlayer},
    ui::{Theme, click_target, draw},
};

fn handle_mouse(app: &mut App, mouse: MouseEvent, area: Rect, now: Instant) -> AppOutcome {
    if mouse.kind != MouseEventKind::Down(MouseButton::Left) {
        return AppOutcome::None;
    }

    let target = click_target(area, (mouse.column, mouse.row), app);
    app.handle_click_target(target, now)
}

fn handle_outcome(
    outcome: AppOutcome,
    app: &App,
    config: &mut Config,
    task_store: &mut Option<TaskStore>,
    notifier: &mut impl Notifier,
    sound_player: &mut impl SoundPlayer,
) -> Result<bool, RunError> {
    match outcome {
        AppOutcome::None => Ok(false),
        AppOutcome::FocusAudio(action) => {
            match action {
                FocusAudioAction::StartOrResume => {
                    if let Some(file) = config.sound().focus().playback_file() {
                        sound_player.start_or_resume_focus(file);
                    }
                }
                FocusAudioAction::Pause => sound_player.pause_focus(),
                FocusAudioAction::Stop => sound_player.stop_focus(),
            }
            Ok(false)
        }
        AppOutcome::SessionCompleted(session) => {
            if session == pomock::SessionKind::Focus {
                sound_player.stop_focus();
            }
            if config.notification().enabled() {
                notifier.session_completed(session);
            }
            if let Some(file) = config.sound().completion().playback_file() {
                sound_player.play_completion(file);
            }
            Ok(false)
        }
        AppOutcome::TasksChanged => {
            if let Some(task_store) = task_store.as_ref() {
                task_store.save(&app.task_state())?;
            }
            Ok(false)
        }
        AppOutcome::SettingsChanged(updated) => {
            let focus_file_changed =
                config.sound().focus().playback_file() != updated.sound().focus().playback_file();
            updated.save()?;
            *config = *updated;
            *task_store = task_store_for_config(config)?;
            if let Some(task_store) = task_store.as_ref() {
                task_store.save(&app.task_state())?;
            }
            if focus_file_changed {
                sound_player.stop_focus();
                if app.is_focus_running()
                    && let Some(file) = config.sound().focus().playback_file()
                {
                    sound_player.start_or_resume_focus(file);
                }
            }
            Ok(false)
        }
        AppOutcome::Quit => {
            sound_player.stop_focus();
            Ok(true)
        }
    }
}

fn task_store_for_config(config: &Config) -> Result<Option<TaskStore>, TaskPersistenceError> {
    config.tasks().persist().then(TaskStore::user).transpose()
}

fn load_task_state(task_store: Option<&TaskStore>) -> Result<TaskState, TaskPersistenceError> {
    task_store.map_or_else(|| Ok(TaskState::default()), TaskStore::load)
}

fn should_handle_key_event(kind: KeyEventKind) -> bool {
    kind != KeyEventKind::Release
}

fn advance_timer(app: &mut App, last_tick: &mut Instant, now: Instant) -> AppOutcome {
    let elapsed = now.duration_since(*last_tick);
    *last_tick = now;
    app.tick(elapsed)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = Config::load()?;
    let task_store = task_store_for_config(&config)?;
    let task_state = load_task_state(task_store.as_ref())?;
    let mut session = TerminalSession::start()?;
    let run_result = run_app(session.terminal_mut(), config, task_store, task_state);
    let restore_result = session.restore();

    Ok(combine_run_and_restore_results(run_result, restore_result)?)
}

#[derive(Debug)]
enum RunError {
    Terminal(io::Error),
    Config(ConfigError),
    TaskPersistence(TaskPersistenceError),
    TerminalRestoration { run: Box<Self>, restore: io::Error },
}

impl fmt::Display for RunError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Terminal(error) => error.fmt(formatter),
            Self::TaskPersistence(error) => error.fmt(formatter),
            Self::Config(error) => error.fmt(formatter),
            Self::TerminalRestoration { run, restore } => {
                write!(
                    formatter,
                    "{run}; terminal restoration also failed: {restore}"
                )
            }
        }
    }
}

impl Error for RunError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Terminal(error) => Some(error),
            Self::TaskPersistence(error) => Some(error),
            Self::Config(error) => Some(error),
            Self::TerminalRestoration { run, .. } => Some(run),
        }
    }
}

impl From<io::Error> for RunError {
    fn from(error: io::Error) -> Self {
        Self::Terminal(error)
    }
}

impl From<TaskPersistenceError> for RunError {
    fn from(error: TaskPersistenceError) -> Self {
        Self::TaskPersistence(error)
    }
}

impl From<ConfigError> for RunError {
    fn from(error: ConfigError) -> Self {
        Self::Config(error)
    }
}

struct TerminalSession {
    terminal: Terminal<CrosstermBackend<Stdout>>,
    restore_required: bool,
}

impl TerminalSession {
    fn start() -> io::Result<Self> {
        enable_raw_mode()?;
        let mut rollback = SetupRollback::new();
        let mut stdout = io::stdout();

        rollback.alternate_screen = true;
        execute!(stdout, EnterAlternateScreen)?;
        rollback.mouse_capture = true;
        execute!(stdout, EnableMouseCapture)?;

        let backend = CrosstermBackend::new(stdout);
        let terminal = Terminal::new(backend)?;
        rollback.disarm();

        Ok(Self {
            terminal,
            restore_required: true,
        })
    }

    fn terminal_mut(&mut self) -> &mut Terminal<CrosstermBackend<Stdout>> {
        &mut self.terminal
    }

    fn restore(&mut self) -> io::Result<()> {
        if !self.restore_required {
            return Ok(());
        }
        self.restore_required = false;

        let mut first_error = None;
        record_error(
            &mut first_error,
            execute!(self.terminal.backend_mut(), DisableMouseCapture),
        );
        record_error(
            &mut first_error,
            execute!(self.terminal.backend_mut(), LeaveAlternateScreen),
        );
        record_error(&mut first_error, self.terminal.show_cursor());
        record_error(&mut first_error, disable_raw_mode());

        first_error.map_or(Ok(()), Err)
    }
}

impl Drop for TerminalSession {
    fn drop(&mut self) {
        let _ = self.restore();
    }
}

struct SetupRollback {
    armed: bool,
    alternate_screen: bool,
    mouse_capture: bool,
}

impl SetupRollback {
    fn new() -> Self {
        Self {
            armed: true,
            alternate_screen: false,
            mouse_capture: false,
        }
    }

    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for SetupRollback {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }

        let mut stdout = io::stdout();
        if self.mouse_capture {
            let _ = execute!(stdout, DisableMouseCapture);
        }
        if self.alternate_screen {
            let _ = execute!(stdout, LeaveAlternateScreen);
        }
        let _ = disable_raw_mode();
    }
}

fn record_error(first_error: &mut Option<io::Error>, result: io::Result<()>) {
    if let Err(error) = result
        && first_error.is_none()
    {
        *first_error = Some(error);
    }
}

fn combine_run_and_restore_results(
    run_result: Result<(), RunError>,
    restore_result: io::Result<()>,
) -> Result<(), RunError> {
    match (run_result, restore_result) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(error), Ok(())) => Err(error),
        (Ok(()), Err(error)) => Err(RunError::Terminal(error)),
        (Err(run), Err(restore)) => Err(RunError::TerminalRestoration {
            run: Box::new(run),
            restore,
        }),
    }
}

fn run_app(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    mut config: Config,
    mut task_store: Option<TaskStore>,
    task_state: TaskState,
) -> Result<(), RunError> {
    let mut app = App::from_config_and_tasks(&config, task_state);
    let mut notifier = DesktopNotifier;
    let mut sound_player = FileSoundPlayer::default();

    let mut last_tick = Instant::now();

    loop {
        let now = Instant::now();
        let outcome = advance_timer(&mut app, &mut last_tick, now);
        if handle_outcome(
            outcome,
            &app,
            &mut config,
            &mut task_store,
            &mut notifier,
            &mut sound_player,
        )? {
            break;
        }

        terminal.draw(|frame| {
            draw(frame, &mut app, Theme::from(config.theme()), config.keys());
        })?;

        if event::poll(Duration::from_millis(100))? {
            let event = event::read()?;
            let now = Instant::now();
            let outcome = advance_timer(&mut app, &mut last_tick, now);
            if handle_outcome(
                outcome,
                &app,
                &mut config,
                &mut task_store,
                &mut notifier,
                &mut sound_player,
            )? {
                break;
            }

            match event {
                Event::Key(key) if should_handle_key_event(key.kind) => {
                    if let Some(action) = map_key(
                        key.code,
                        app.edit_mode(),
                        app.ui_focus(),
                        app.is_confirmation_open(),
                        app.settings_mode(),
                        app.input_keys(),
                    ) {
                        let outcome = app.dispatch(action);
                        if handle_outcome(
                            outcome,
                            &app,
                            &mut config,
                            &mut task_store,
                            &mut notifier,
                            &mut sound_player,
                        )? {
                            break;
                        }
                    }
                }
                Event::Mouse(mouse) if app.edit_mode() == EditMode::Normal => {
                    let outcome = handle_mouse(&mut app, mouse, terminal.size()?.into(), now);
                    if handle_outcome(
                        outcome,
                        &app,
                        &mut config,
                        &mut task_store,
                        &mut notifier,
                        &mut sound_player,
                    )? {
                        break;
                    }
                }
                _ => {}
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::PathBuf,
        sync::atomic::{AtomicU64, Ordering},
    };

    use super::*;
    use pomock::{
        app::{Action, Direction},
        config::{TasksConfig, TimerConfig},
    };

    static NEXT_TEMP_PATH: AtomicU64 = AtomicU64::new(0);

    #[derive(Default)]
    struct RecordingNotifier {
        completions: Vec<pomock::SessionKind>,
    }

    impl Notifier for RecordingNotifier {
        fn session_completed(&mut self, session: pomock::SessionKind) {
            self.completions.push(session);
        }
    }

    #[derive(Default)]
    struct RecordingSoundPlayer {
        files: Vec<PathBuf>,
        focus_actions: Vec<&'static str>,
        focus_files: Vec<PathBuf>,
    }

    impl SoundPlayer for RecordingSoundPlayer {
        fn play_completion(&mut self, file: &std::path::Path) {
            self.files.push(file.to_owned());
        }

        fn start_or_resume_focus(&mut self, file: &std::path::Path) {
            self.focus_actions.push("start");
            self.focus_files.push(file.to_owned());
        }

        fn pause_focus(&mut self) {
            self.focus_actions.push("pause");
        }

        fn stop_focus(&mut self) {
            self.focus_actions.push("stop");
        }
    }

    fn temp_path(name: &str) -> PathBuf {
        let unique = NEXT_TEMP_PATH.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "pomock-main-test-{}-{unique}-{name}",
            std::process::id()
        ))
    }

    #[test]
    fn key_releases_are_ignored_while_presses_and_repeats_are_handled() {
        assert!(should_handle_key_event(KeyEventKind::Press));
        assert!(should_handle_key_event(KeyEventKind::Repeat));
        assert!(!should_handle_key_event(KeyEventKind::Release));
    }

    #[test]
    fn ready_time_before_start_is_not_charged_to_the_running_session() {
        let mut app = App::new();
        let start = Instant::now();
        let mut last_tick = start;
        let key_time = start + Duration::from_millis(80);

        assert_eq!(
            advance_timer(&mut app, &mut last_tick, key_time),
            AppOutcome::None
        );
        assert_eq!(
            app.dispatch(Action::PrimaryAction),
            AppOutcome::FocusAudio(FocusAudioAction::StartOrResume)
        );

        assert_eq!(
            advance_timer(
                &mut app,
                &mut last_tick,
                key_time + Duration::from_secs(25 * 60) - Duration::from_millis(1),
            ),
            AppOutcome::None
        );
        assert_eq!(
            advance_timer(
                &mut app,
                &mut last_tick,
                key_time + Duration::from_secs(25 * 60),
            ),
            AppOutcome::SessionCompleted(pomock::SessionKind::Focus)
        );
    }

    #[test]
    fn task_change_outcomes_are_saved_at_the_boundary() {
        let path = temp_path("tasks.toml");
        let store = TaskStore::at(&path);
        let mut app = App::new();
        let _ = app.dispatch(Action::NavigateFocus(Direction::Down));
        let _ = app.dispatch(Action::BeginAdd);
        for character in "Persist me".chars() {
            let _ = app.dispatch(Action::PushInput(character));
        }
        let outcome = app.dispatch(Action::SubmitEdit);

        let mut config = Config::default();
        let mut task_store = Some(store);
        let mut notifier = RecordingNotifier::default();
        let mut sound_player = RecordingSoundPlayer::default();

        assert!(
            !handle_outcome(
                outcome,
                &app,
                &mut config,
                &mut task_store,
                &mut notifier,
                &mut sound_player,
            )
            .unwrap()
        );
        assert!(notifier.completions.is_empty());
        assert!(sound_player.files.is_empty());
        assert_eq!(
            task_store.as_ref().unwrap().load().unwrap(),
            app.task_state()
        );

        fs::remove_file(path).unwrap();
    }

    #[test]
    fn disabled_task_persistence_starts_empty_and_does_not_save_changes() {
        let path = temp_path("disabled-tasks.toml");
        let store = TaskStore::at(&path);
        let config = Config::with_tasks(TimerConfig::default(), TasksConfig::new(false)).unwrap();
        let mut disabled_store = task_store_for_config(&config).unwrap();
        assert!(disabled_store.is_none());

        let mut persisted_app = App::new();
        let _ = persisted_app.dispatch(Action::NavigateFocus(Direction::Down));
        let _ = persisted_app.dispatch(Action::BeginAdd);
        for character in "Keep on disk".chars() {
            let _ = persisted_app.dispatch(Action::PushInput(character));
        }
        let _ = persisted_app.dispatch(Action::SubmitEdit);
        let persisted = persisted_app.task_state();
        store.save(&persisted).unwrap();

        assert_eq!(
            load_task_state(disabled_store.as_ref()).unwrap(),
            TaskState::default()
        );

        let mut app = App::new();
        let _ = app.dispatch(Action::NavigateFocus(Direction::Down));
        let _ = app.dispatch(Action::BeginAdd);
        let _ = app.dispatch(Action::PushInput('x'));
        let outcome = app.dispatch(Action::SubmitEdit);

        let mut config = config;
        let mut notifier = RecordingNotifier::default();
        let mut sound_player = RecordingSoundPlayer::default();
        assert!(
            !handle_outcome(
                outcome,
                &app,
                &mut config,
                &mut disabled_store,
                &mut notifier,
                &mut sound_player,
            )
            .unwrap()
        );
        assert_eq!(store.load().unwrap(), persisted);

        fs::remove_file(path).unwrap();
    }

    #[test]
    fn completion_outcomes_notify_and_play_the_configured_sound_exactly_once() {
        let app = App::new();
        let sound_file = temp_path("custom-completion.mp3");
        let mut config = Config::default()
            .with_sound(pomock::config::SoundConfig::default().with_completion(
                pomock::config::CompletionSoundConfig::new(true, Some(sound_file.clone())),
            ))
            .unwrap();
        let mut task_store = None;
        let mut notifier = RecordingNotifier::default();
        let mut sound_player = RecordingSoundPlayer::default();

        assert!(
            !handle_outcome(
                AppOutcome::SessionCompleted(pomock::SessionKind::Focus),
                &app,
                &mut config,
                &mut task_store,
                &mut notifier,
                &mut sound_player,
            )
            .unwrap()
        );
        assert_eq!(notifier.completions, [pomock::SessionKind::Focus]);
        assert_eq!(sound_player.files, [sound_file]);
        assert_eq!(sound_player.focus_actions, ["stop"]);

        assert!(
            !handle_outcome(
                AppOutcome::None,
                &app,
                &mut config,
                &mut task_store,
                &mut notifier,
                &mut sound_player,
            )
            .unwrap()
        );
        assert_eq!(notifier.completions, [pomock::SessionKind::Focus]);
        assert_eq!(sound_player.files.len(), 1);
        assert_eq!(sound_player.focus_actions, ["stop"]);
    }

    #[test]
    fn disabled_notifications_do_not_suppress_completion_audio() {
        let app = App::new();
        let sound_file = temp_path("completion.wav");
        let mut config = Config::default()
            .with_notification(pomock::config::NotificationConfig::new(false))
            .with_sound(pomock::config::SoundConfig::default().with_completion(
                pomock::config::CompletionSoundConfig::new(true, Some(sound_file.clone())),
            ))
            .unwrap();
        let mut task_store = None;
        let mut notifier = RecordingNotifier::default();
        let mut sound_player = RecordingSoundPlayer::default();

        handle_outcome(
            AppOutcome::SessionCompleted(pomock::SessionKind::ShortBreak),
            &app,
            &mut config,
            &mut task_store,
            &mut notifier,
            &mut sound_player,
        )
        .unwrap();

        assert!(notifier.completions.is_empty());
        assert_eq!(sound_player.files, [sound_file]);
        assert!(sound_player.focus_actions.is_empty());
    }

    #[test]
    fn focus_audio_outcomes_route_only_configured_starts_and_always_cleanup() {
        let app = App::new();
        let focus_file = temp_path("focus.ogg");
        let mut config = Config::default()
            .with_sound(pomock::config::SoundConfig::default().with_focus(
                pomock::config::FocusSoundConfig::new(true, Some(focus_file.clone())),
            ))
            .unwrap();
        let mut task_store = None;
        let mut notifier = RecordingNotifier::default();
        let mut sound_player = RecordingSoundPlayer::default();

        for outcome in [
            AppOutcome::FocusAudio(FocusAudioAction::StartOrResume),
            AppOutcome::FocusAudio(FocusAudioAction::Pause),
            AppOutcome::FocusAudio(FocusAudioAction::Stop),
        ] {
            handle_outcome(
                outcome,
                &app,
                &mut config,
                &mut task_store,
                &mut notifier,
                &mut sound_player,
            )
            .unwrap();
        }

        assert_eq!(sound_player.focus_actions, ["start", "pause", "stop"]);
        assert_eq!(sound_player.focus_files, [focus_file]);

        let mut disabled_config = Config::default();
        handle_outcome(
            AppOutcome::FocusAudio(FocusAudioAction::StartOrResume),
            &app,
            &mut disabled_config,
            &mut task_store,
            &mut notifier,
            &mut sound_player,
        )
        .unwrap();
        assert_eq!(sound_player.focus_actions, ["start", "pause", "stop"]);
    }

    #[test]
    fn disabled_sound_options_keep_configured_files_silent() {
        let app = App::new();
        let mut config = Config::default()
            .with_sound(
                pomock::config::SoundConfig::default()
                    .with_completion(pomock::config::CompletionSoundConfig::new(
                        false,
                        Some(temp_path("disabled-completion.wav")),
                    ))
                    .with_focus(pomock::config::FocusSoundConfig::new(
                        false,
                        Some(temp_path("disabled-focus.ogg")),
                    )),
            )
            .unwrap();
        let mut task_store = None;
        let mut notifier = RecordingNotifier::default();
        let mut sound_player = RecordingSoundPlayer::default();

        for outcome in [
            AppOutcome::FocusAudio(FocusAudioAction::StartOrResume),
            AppOutcome::SessionCompleted(pomock::SessionKind::ShortBreak),
        ] {
            handle_outcome(
                outcome,
                &app,
                &mut config,
                &mut task_store,
                &mut notifier,
                &mut sound_player,
            )
            .unwrap();
        }

        assert!(sound_player.files.is_empty());
        assert!(sound_player.focus_actions.is_empty());
    }

    #[test]
    fn run_error_is_preserved_when_restoration_succeeds() {
        let run_error = io::Error::new(io::ErrorKind::BrokenPipe, "run failed");

        let error = combine_run_and_restore_results(Err(RunError::Terminal(run_error)), Ok(()))
            .unwrap_err();

        assert!(matches!(
            error,
            RunError::Terminal(ref error) if error.kind() == io::ErrorKind::BrokenPipe
        ));
        assert_eq!(error.to_string(), "run failed");
    }

    #[test]
    fn restoration_error_is_reported_after_a_successful_run() {
        let restore_error = io::Error::other("restore failed");

        let error = combine_run_and_restore_results(Ok(()), Err(restore_error)).unwrap_err();

        assert!(matches!(error, RunError::Terminal(_)));
        assert_eq!(error.to_string(), "restore failed");
    }

    #[test]
    fn simultaneous_run_and_restoration_errors_are_both_reported() {
        let run_error = io::Error::new(io::ErrorKind::BrokenPipe, "run failed");
        let restore_error = io::Error::other("restore failed");

        let error =
            combine_run_and_restore_results(Err(RunError::Terminal(run_error)), Err(restore_error))
                .unwrap_err();

        assert!(matches!(error, RunError::TerminalRestoration { .. }));
        assert_eq!(
            error.to_string(),
            "run failed; terminal restoration also failed: restore failed"
        );
    }
}
