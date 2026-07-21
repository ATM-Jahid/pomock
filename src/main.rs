use std::{
    env,
    error::Error,
    ffi::OsString,
    fmt, fs,
    io::{self, BufRead, Stdout, Write},
    path::{Path, PathBuf},
};

use crossterm::{
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyEventKind, MouseButton,
        MouseEvent, MouseEventKind,
    },
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};

use ratatui::{Terminal, backend::CrosstermBackend};

use std::time::{Duration, Instant};

use pomock::{
    app::{Action, App, AppOutcome, Direction, EditMode, FocusAudioAction, TaskState},
    config::{Config, ConfigError},
    input::map_key,
    notification::{DesktopNotifier, Notifier},
    persistence::{TaskPersistenceError, TaskStore},
    sound::{FileSoundPlayer, SoundPlayer},
    ui::{FrameGeometry, Theme, action_target_visible, click_target, draw, scroll_target},
};

fn handle_mouse(
    app: &mut App,
    mouse: MouseEvent,
    geometry: &FrameGeometry,
    now: Instant,
) -> AppOutcome {
    match mouse.kind {
        MouseEventKind::Down(MouseButton::Left) => {
            let target = click_target(geometry, (mouse.column, mouse.row), app);
            app.handle_click_target(target, now)
        }
        MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => {
            let Some(target) = scroll_target(geometry, (mouse.column, mouse.row), app) else {
                return AppOutcome::None;
            };
            let direction = if mouse.kind == MouseEventKind::ScrollUp {
                Direction::Up
            } else {
                Direction::Down
            };
            app.dispatch(Action::Scroll(target, direction))
        }
        _ => AppOutcome::None,
    }
}

fn handle_outcome(
    outcome: AppOutcome,
    app: &App,
    config: &mut Config,
    task_store: &mut Option<TaskStore>,
    workspace_store: &TaskStore,
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
        AppOutcome::TimerEffects {
            focus_audio,
            stop_completion_audio,
        } => {
            if stop_completion_audio {
                sound_player.stop_completion();
            }
            if let Some(action) = focus_audio {
                match action {
                    FocusAudioAction::StartOrResume => {
                        if let Some(file) = config.sound().focus().playback_file() {
                            sound_player.start_or_resume_focus(file);
                        }
                    }
                    FocusAudioAction::Pause => sound_player.pause_focus(),
                    FocusAudioAction::Stop => sound_player.stop_focus(),
                }
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
            let next_task_store = task_store_for_config(&updated, workspace_store);
            commit_settings_change(
                *updated,
                &app.task_state(),
                config,
                task_store,
                next_task_store,
                Config::save,
            )?;
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
            sound_player.stop_completion();
            Ok(true)
        }
    }
}

fn commit_settings_change(
    updated: Config,
    task_state: &TaskState,
    config: &mut Config,
    task_store: &mut Option<TaskStore>,
    next_task_store: Option<TaskStore>,
    save_config: impl FnOnce(&Config) -> Result<(), ConfigError>,
) -> Result<(), RunError> {
    let enabling_task_persistence = !config.tasks().persist() && updated.tasks().persist();

    if enabling_task_persistence && let Some(store) = next_task_store.as_ref() {
        store.save(task_state)?;
    }

    save_config(&updated)?;
    *config = updated;
    *task_store = next_task_store;
    Ok(())
}

fn task_store_for_config(config: &Config, workspace_store: &TaskStore) -> Option<TaskStore> {
    config.tasks().persist().then(|| workspace_store.clone())
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
    let mut stdin = io::stdin().lock();
    let mut stdout = io::stdout();
    let command = CliCommand::parse(env::args_os().skip(1))?;
    let CliCommand::Run { workspace } = command else {
        write_help(&mut stdout)?;
        return Ok(());
    };
    let workspace_store = TaskStore::user_in_workspace(workspace.as_deref())?;
    let workspace_instance = workspace_store.register_instance()?;
    if workspace_instance.already_open()
        && !confirm_shared_workspace(workspace.as_deref(), &mut stdin, &mut stdout)?
    {
        return Ok(());
    }
    let Some(config) = load_config_for_startup(&mut stdin, &mut stdout)? else {
        return Ok(());
    };
    let task_store = task_store_for_config(&config, &workspace_store);
    let Some(task_state) = load_tasks_for_startup(task_store.as_ref(), &mut stdin, &mut stdout)?
    else {
        return Ok(());
    };
    let mut session = TerminalSession::start()?;
    let run_result = run_app(
        session.terminal_mut(),
        config,
        task_store,
        task_state,
        workspace_store,
    );
    let restore_result = session.restore();

    Ok(combine_run_and_restore_results(run_result, restore_result)?)
}

#[derive(Debug, PartialEq, Eq)]
enum CliCommand {
    Run { workspace: Option<String> },
    Help,
}

impl CliCommand {
    fn parse(arguments: impl IntoIterator<Item = OsString>) -> Result<Self, CliError> {
        let mut arguments = arguments.into_iter();
        let mut workspace = None;

        while let Some(argument) = arguments.next() {
            let argument = argument
                .into_string()
                .map_err(|_| CliError::NonUnicodeArgument)?;
            match argument.as_str() {
                "-h" | "--help" => return Ok(Self::Help),
                "--wspace" => {
                    if workspace.is_some() {
                        return Err(CliError::DuplicateWorkspace);
                    }
                    let name = arguments.next().ok_or(CliError::MissingWorkspaceName)?;
                    let name = name
                        .into_string()
                        .map_err(|_| CliError::NonUnicodeArgument)?;
                    validate_workspace_name(&name)?;
                    workspace = Some(name);
                }
                _ if argument.starts_with("--wspace=") => {
                    if workspace.is_some() {
                        return Err(CliError::DuplicateWorkspace);
                    }
                    let name = argument.trim_start_matches("--wspace=");
                    validate_workspace_name(name)?;
                    workspace = Some(name.to_owned());
                }
                _ => return Err(CliError::UnexpectedArgument(argument)),
            }
        }

        Ok(Self::Run { workspace })
    }
}

#[derive(Debug, PartialEq, Eq)]
enum CliError {
    MissingWorkspaceName,
    DuplicateWorkspace,
    InvalidWorkspaceName(String),
    UnexpectedArgument(String),
    NonUnicodeArgument,
}

impl fmt::Display for CliError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingWorkspaceName => formatter.write_str("--wspace requires a workspace name"),
            Self::DuplicateWorkspace => formatter.write_str("--wspace may only be specified once"),
            Self::InvalidWorkspaceName(name) => write!(
                formatter,
                "invalid workspace name {name:?}; use letters, numbers, '.', '-', or '_'"
            ),
            Self::UnexpectedArgument(argument) => write!(
                formatter,
                "unexpected argument {argument:?}; run `pomock --help` for usage"
            ),
            Self::NonUnicodeArgument => formatter.write_str("arguments must be valid Unicode"),
        }
    }
}

impl Error for CliError {}

fn validate_workspace_name(name: &str) -> Result<(), CliError> {
    let valid = !name.is_empty()
        && name != "."
        && name != ".."
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'));
    valid
        .then_some(())
        .ok_or_else(|| CliError::InvalidWorkspaceName(name.to_owned()))
}

fn write_help(writer: &mut impl Write) -> io::Result<()> {
    writeln!(
        writer,
        "pomock - a Pomodoro timer and task workspace\n\nUsage: pomock [--wspace NAME]\n\nOptions:\n  --wspace NAME  Use or create a named task workspace\n  -h, --help     Show this help"
    )
}

fn confirm_shared_workspace(
    workspace: Option<&str>,
    reader: &mut impl BufRead,
    writer: &mut impl Write,
) -> io::Result<bool> {
    let label = workspace.unwrap_or("default");
    writeln!(
        writer,
        "Warning: workspace {label:?} is already open. Multiple instances can overwrite each other's task changes."
    )?;

    loop {
        write!(writer, "Open it anyway? [y/N]: ")?;
        writer.flush()?;
        let mut choice = String::new();
        if reader.read_line(&mut choice)? == 0 {
            return Ok(false);
        }
        match choice.trim().to_ascii_lowercase().as_str() {
            "y" | "yes" => return Ok(true),
            "" | "n" | "no" => return Ok(false),
            _ => writeln!(writer, "Enter y to continue or n to quit.")?,
        }
    }
}

fn load_config_for_startup(
    reader: &mut impl BufRead,
    writer: &mut impl Write,
) -> Result<Option<Config>, StartupError> {
    let path = Config::path()?;
    load_config_path_for_startup(&path, reader, writer)
}

fn load_config_path_for_startup(
    path: &Path,
    reader: &mut impl BufRead,
    writer: &mut impl Write,
) -> Result<Option<Config>, StartupError> {
    match Config::load_from(path) {
        Ok(config) => Ok(Some(config)),
        Err(error) if is_invalid_config(&error) => {
            let recovered = recover_invalid_file(reader, writer, "configuration", path, &error)?;
            Ok(recovered.then_some(Config::default()))
        }
        Err(error) => Err(error.into()),
    }
}

fn load_tasks_for_startup(
    task_store: Option<&TaskStore>,
    reader: &mut impl BufRead,
    writer: &mut impl Write,
) -> Result<Option<TaskState>, StartupError> {
    let Some(task_store) = task_store else {
        return Ok(Some(TaskState::default()));
    };

    match task_store.load() {
        Ok(state) => Ok(Some(state)),
        Err(error) if is_invalid_task_file(&error) => {
            let recovered =
                recover_invalid_file(reader, writer, "task data", task_store.path(), &error)?;
            Ok(recovered.then_some(TaskState::default()))
        }
        Err(error) => Err(error.into()),
    }
}

fn is_invalid_config(error: &ConfigError) -> bool {
    matches!(
        error,
        ConfigError::Parse { .. } | ConfigError::Validation { .. }
    )
}

fn is_invalid_task_file(error: &TaskPersistenceError) -> bool {
    matches!(
        error,
        TaskPersistenceError::Parse { .. }
            | TaskPersistenceError::Validation { .. }
            | TaskPersistenceError::UnsupportedVersion { .. }
    )
}

fn recover_invalid_file(
    reader: &mut impl BufRead,
    writer: &mut impl Write,
    description: &str,
    path: &Path,
    error: &impl fmt::Display,
) -> Result<bool, StartupError> {
    writeln!(
        writer,
        "pomock could not load the {description} file at {}:\n{error}",
        path.display()
    )?;

    loop {
        write!(
            writer,
            "\n[d] Delete the invalid file and continue\n[q] Quit\nChoice: "
        )?;
        writer.flush()?;

        let mut choice = String::new();
        if reader.read_line(&mut choice)? == 0 {
            return Ok(false);
        }

        match choice.trim().to_ascii_lowercase().as_str() {
            "d" | "delete" => {
                match fs::remove_file(path) {
                    Ok(()) => {}
                    Err(source) if source.kind() == io::ErrorKind::NotFound => {}
                    Err(source) => {
                        return Err(StartupError::DeleteInvalidFile {
                            path: path.to_owned(),
                            source,
                        });
                    }
                }
                writeln!(writer, "Deleted {}.", path.display())?;
                return Ok(true);
            }
            "q" | "quit" => return Ok(false),
            _ => writeln!(writer, "Enter d to delete the file or q to quit.")?,
        }
    }
}

#[derive(Debug)]
enum StartupError {
    Config(ConfigError),
    TaskPersistence(TaskPersistenceError),
    Io(io::Error),
    DeleteInvalidFile { path: PathBuf, source: io::Error },
}

impl fmt::Display for StartupError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Config(error) => error.fmt(formatter),
            Self::TaskPersistence(error) => error.fmt(formatter),
            Self::Io(error) => error.fmt(formatter),
            Self::DeleteInvalidFile { path, source } => write!(
                formatter,
                "could not delete invalid file {}: {source}",
                path.display()
            ),
        }
    }
}

impl Error for StartupError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Config(error) => Some(error),
            Self::TaskPersistence(error) => Some(error),
            Self::Io(error) | Self::DeleteInvalidFile { source: error, .. } => Some(error),
        }
    }
}

impl From<ConfigError> for StartupError {
    fn from(error: ConfigError) -> Self {
        Self::Config(error)
    }
}

impl From<TaskPersistenceError> for StartupError {
    fn from(error: TaskPersistenceError) -> Self {
        Self::TaskPersistence(error)
    }
}

impl From<io::Error> for StartupError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
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
    workspace_store: TaskStore,
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
            &workspace_store,
            &mut notifier,
            &mut sound_player,
        )? {
            break;
        }

        let mut frame_geometry = None;
        terminal.draw(|frame| {
            frame_geometry = Some(draw(
                frame,
                &mut app,
                Theme::from(config.theme()),
                config.keys(),
            ));
        })?;
        let frame_geometry = frame_geometry.expect("terminal draw must resolve frame geometry");

        if event::poll(Duration::from_millis(100))? {
            let event = event::read()?;
            let now = Instant::now();
            let outcome = advance_timer(&mut app, &mut last_tick, now);
            if handle_outcome(
                outcome,
                &app,
                &mut config,
                &mut task_store,
                &workspace_store,
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
                    ) && action_target_visible(&frame_geometry, app.ui_focus(), &action)
                    {
                        let outcome = app.dispatch(action);
                        if handle_outcome(
                            outcome,
                            &app,
                            &mut config,
                            &mut task_store,
                            &workspace_store,
                            &mut notifier,
                            &mut sound_player,
                        )? {
                            break;
                        }
                    }
                }
                Event::Mouse(mouse) if app.edit_mode() == EditMode::Normal => {
                    let outcome = handle_mouse(&mut app, mouse, &frame_geometry, now);
                    if handle_outcome(
                        outcome,
                        &app,
                        &mut config,
                        &mut task_store,
                        &workspace_store,
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
        cell::Cell,
        fs,
        io::Cursor,
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

        fn stop_completion(&mut self) {
            self.focus_actions.push("stop_completion");
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

    fn task_state(description: &str) -> TaskState {
        let mut app = App::new();
        let _ = app.dispatch(Action::NavigateFocus(Direction::Down));
        let _ = app.dispatch(Action::BeginAdd);
        for character in description.chars() {
            let _ = app.dispatch(Action::PushInput(character));
        }
        let _ = app.dispatch(Action::SubmitEdit);
        app.task_state()
    }

    #[test]
    fn key_releases_are_ignored_while_presses_and_repeats_are_handled() {
        assert!(should_handle_key_event(KeyEventKind::Press));
        assert!(should_handle_key_event(KeyEventKind::Repeat));
        assert!(!should_handle_key_event(KeyEventKind::Release));
    }

    #[test]
    fn workspace_argument_accepts_separate_and_equals_forms() {
        assert_eq!(
            CliCommand::parse([OsString::from("--wspace"), OsString::from("client-one")]).unwrap(),
            CliCommand::Run {
                workspace: Some("client-one".to_owned())
            }
        );
        assert_eq!(
            CliCommand::parse([OsString::from("--wspace=personal.2026")]).unwrap(),
            CliCommand::Run {
                workspace: Some("personal.2026".to_owned())
            }
        );
        assert_eq!(
            CliCommand::parse(Vec::<OsString>::new()).unwrap(),
            CliCommand::Run { workspace: None }
        );
    }

    #[test]
    fn workspace_argument_rejects_missing_unsafe_and_duplicate_names() {
        assert_eq!(
            CliCommand::parse([OsString::from("--wspace")]).unwrap_err(),
            CliError::MissingWorkspaceName
        );
        assert!(matches!(
            CliCommand::parse([OsString::from("--wspace=../shared")]).unwrap_err(),
            CliError::InvalidWorkspaceName(_)
        ));
        assert_eq!(
            CliCommand::parse([
                OsString::from("--wspace=one"),
                OsString::from("--wspace=two")
            ])
            .unwrap_err(),
            CliError::DuplicateWorkspace
        );
    }

    #[test]
    fn shared_workspace_warning_requires_explicit_acceptance() {
        let mut accepted_output = Vec::new();
        assert!(
            confirm_shared_workspace(
                Some("client"),
                &mut Cursor::new(b"maybe\nyes\n"),
                &mut accepted_output,
            )
            .unwrap()
        );
        let accepted_output = String::from_utf8(accepted_output).unwrap();
        assert!(accepted_output.contains("workspace \"client\" is already open"));
        assert!(accepted_output.contains("Enter y to continue or n to quit."));

        assert!(
            !confirm_shared_workspace(None, &mut Cursor::new(b"\n"), &mut Vec::new(),).unwrap()
        );
    }

    #[test]
    fn invalid_config_can_be_deleted_and_replaced_with_defaults() {
        let path = temp_path("invalid-config.toml");
        fs::write(&path, "not valid toml =").unwrap();
        let mut input = Cursor::new(b"invalid\ndelete\n");
        let mut output = Vec::new();

        let config = load_config_path_for_startup(&path, &mut input, &mut output)
            .unwrap()
            .unwrap();

        assert_eq!(config, Config::default());
        assert!(!path.exists());
        let output = String::from_utf8(output).unwrap();
        assert!(output.contains("could not load the configuration file"));
        assert!(output.contains("Enter d to delete the file or q to quit."));
        assert!(output.contains("Deleted"));
    }

    #[test]
    fn invalid_config_can_be_left_in_place_when_quitting() {
        let path = temp_path("invalid-config-quit.toml");
        let contents = "not valid toml =";
        fs::write(&path, contents).unwrap();
        let mut input = Cursor::new(b"q\n");
        let mut output = Vec::new();

        let config = load_config_path_for_startup(&path, &mut input, &mut output).unwrap();

        assert!(config.is_none());
        assert_eq!(fs::read_to_string(&path).unwrap(), contents);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn invalid_task_file_can_be_deleted_and_started_empty() {
        let path = temp_path("invalid-tasks.toml");
        fs::write(&path, "version = 2\ntodo = []\ndone = []\n").unwrap();
        let store = TaskStore::at(&path);
        let mut input = Cursor::new(b"d\n");
        let mut output = Vec::new();

        let state = load_tasks_for_startup(Some(&store), &mut input, &mut output)
            .unwrap()
            .unwrap();

        assert_eq!(state, TaskState::default());
        assert!(!path.exists());
        assert!(
            String::from_utf8(output)
                .unwrap()
                .contains("could not load the task data file")
        );
    }

    #[test]
    fn task_read_errors_do_not_offer_to_delete_the_path() {
        let path = temp_path("task-read-error");
        fs::create_dir(&path).unwrap();
        let store = TaskStore::at(&path);
        let mut input = Cursor::new(b"d\n");
        let mut output = Vec::new();

        let error = load_tasks_for_startup(Some(&store), &mut input, &mut output).unwrap_err();

        assert!(matches!(error, StartupError::TaskPersistence(_)));
        assert!(output.is_empty());
        assert!(path.is_dir());
        fs::remove_dir(path).unwrap();
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
        let mut task_store = Some(store.clone());
        let mut notifier = RecordingNotifier::default();
        let mut sound_player = RecordingSoundPlayer::default();

        assert!(
            !handle_outcome(
                outcome,
                &app,
                &mut config,
                &mut task_store,
                &store,
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
        let mut disabled_store = task_store_for_config(&config, &store);
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
            load_tasks_for_startup(
                disabled_store.as_ref(),
                &mut Cursor::new(Vec::new()),
                &mut Vec::new(),
            )
            .unwrap()
            .unwrap(),
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
                &store,
                &mut notifier,
                &mut sound_player,
            )
            .unwrap()
        );
        assert_eq!(store.load().unwrap(), persisted);

        fs::remove_file(path).unwrap();
    }

    #[test]
    fn enabling_persistence_saves_tasks_before_config() {
        let path = temp_path("enable-persistence/tasks.toml");
        let next_store = TaskStore::at(&path);
        let state = task_state("Current task");
        let mut config =
            Config::with_tasks(TimerConfig::default(), TasksConfig::new(false)).unwrap();
        let updated = Config::default();
        let mut task_store = None;

        commit_settings_change(
            updated.clone(),
            &state,
            &mut config,
            &mut task_store,
            Some(next_store),
            |_| {
                assert_eq!(TaskStore::at(&path).load().unwrap(), state);
                Ok(())
            },
        )
        .unwrap();

        assert_eq!(config, updated);
        assert_eq!(task_store.unwrap().load().unwrap(), state);
        fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn failed_task_snapshot_does_not_commit_persistence_setting() {
        let parent = temp_path("enable-persistence-parent-is-file");
        fs::write(&parent, "not a directory").unwrap();
        let next_store = TaskStore::at(parent.join("tasks.toml"));
        let mut config =
            Config::with_tasks(TimerConfig::default(), TasksConfig::new(false)).unwrap();
        let original = config.clone();
        let mut task_store = None;
        let config_saved = Cell::new(false);

        let result = commit_settings_change(
            Config::default(),
            &task_state("Unsaved"),
            &mut config,
            &mut task_store,
            Some(next_store),
            |_| {
                config_saved.set(true);
                Ok(())
            },
        );

        assert!(matches!(result, Err(RunError::TaskPersistence(_))));
        assert!(!config_saved.get());
        assert_eq!(config, original);
        assert!(task_store.is_none());
        fs::remove_file(parent).unwrap();
    }

    #[test]
    fn unrelated_settings_changes_do_not_rewrite_tasks() {
        let path = temp_path("unchanged-persistence/tasks.toml");
        let next_store = TaskStore::at(&path);
        let mut config = Config::default();
        let updated = config
            .clone()
            .with_notification(pomock::config::NotificationConfig::new(false));
        let mut task_store = Some(next_store.clone());

        commit_settings_change(
            updated.clone(),
            &task_state("In memory"),
            &mut config,
            &mut task_store,
            Some(next_store),
            |_| Ok(()),
        )
        .unwrap();

        assert_eq!(config, updated);
        assert!(!path.exists());
    }

    #[test]
    fn completion_outcome_routes_notification_and_audio_effects() {
        let app = App::new();
        let sound_file = temp_path("custom-completion.mp3");
        let mut config = Config::default()
            .with_sound(pomock::config::SoundConfig::default().with_completion(
                pomock::config::CompletionSoundConfig::new(true, Some(sound_file.clone())),
            ))
            .unwrap();
        let mut task_store = None;
        let workspace_store = TaskStore::at(temp_path("completion-workspace/tasks.toml"));
        let mut notifier = RecordingNotifier::default();
        let mut sound_player = RecordingSoundPlayer::default();

        assert!(
            !handle_outcome(
                AppOutcome::SessionCompleted(pomock::SessionKind::Focus),
                &app,
                &mut config,
                &mut task_store,
                &workspace_store,
                &mut notifier,
                &mut sound_player,
            )
            .unwrap()
        );
        assert_eq!(notifier.completions, [pomock::SessionKind::Focus]);
        assert_eq!(sound_player.files, [sound_file]);
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
        let workspace_store = TaskStore::at(temp_path("notification-workspace/tasks.toml"));
        let mut notifier = RecordingNotifier::default();
        let mut sound_player = RecordingSoundPlayer::default();

        handle_outcome(
            AppOutcome::SessionCompleted(pomock::SessionKind::ShortBreak),
            &app,
            &mut config,
            &mut task_store,
            &workspace_store,
            &mut notifier,
            &mut sound_player,
        )
        .unwrap();

        assert!(notifier.completions.is_empty());
        assert_eq!(sound_player.files, [sound_file]);
        assert!(sound_player.focus_actions.is_empty());
    }

    #[test]
    fn combined_timer_effect_stops_completion_before_starting_focus_audio() {
        let focus_file = temp_path("focus-loop.wav");
        let mut config = Config::default()
            .with_sound(pomock::config::SoundConfig::default().with_focus(
                pomock::config::FocusSoundConfig::new(true, Some(focus_file.clone())),
            ))
            .unwrap();
        let app = App::from_config(&config);
        let mut task_store = None;
        let workspace_store = TaskStore::at(temp_path("timer-effects-workspace/tasks.toml"));
        let mut notifier = RecordingNotifier::default();
        let mut sound = RecordingSoundPlayer::default();

        handle_outcome(
            AppOutcome::TimerEffects {
                focus_audio: Some(FocusAudioAction::StartOrResume),
                stop_completion_audio: true,
            },
            &app,
            &mut config,
            &mut task_store,
            &workspace_store,
            &mut notifier,
            &mut sound,
        )
        .unwrap();

        assert_eq!(sound.focus_actions, ["stop_completion", "start"]);
        assert_eq!(sound.focus_files, [focus_file]);
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
        let workspace_store = TaskStore::at(temp_path("focus-audio-workspace/tasks.toml"));
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
                &workspace_store,
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
            &workspace_store,
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
        let workspace_store = TaskStore::at(temp_path("disabled-sound-workspace/tasks.toml"));
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
                &workspace_store,
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
