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
    input::map_key_event,
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
                    if let Some(action) = map_key_event(
                        key,
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
#[path = "main/tests.rs"]
mod tests;
