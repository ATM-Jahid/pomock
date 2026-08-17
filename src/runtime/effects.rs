use std::{error::Error, fmt, io};

use pomock::{
    app::{App, AppOutcome, FocusAudioAction, TaskState},
    config::{Config, ConfigError},
    notification::Notifier,
    persistence::{TaskPersistenceError, TaskStore},
    sound::SoundPlayer,
};

pub(crate) fn handle_outcome(
    outcome: AppOutcome,
    app: &mut App,
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
            if let Some(task_store) = task_store.as_ref()
                && let Err(error) = task_store.save(&app.task_state())
            {
                report_write_failure(app, &FileWriteError::Tasks(error));
            }
            Ok(false)
        }
        AppOutcome::SettingsChanged(updated) => {
            let focus_file_changed =
                config.sound().focus().playback_file() != updated.sound().focus().playback_file();
            let next_task_store = task_store_for_config(&updated, workspace_store);
            let errors = apply_settings_change(
                *updated,
                &app.task_state(),
                config,
                task_store,
                next_task_store,
                Config::save,
            );
            for error in &errors {
                report_write_failure(app, error);
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
            sound_player.stop_completion();
            Ok(true)
        }
    }
}

pub(crate) fn apply_settings_change(
    updated: Config,
    task_state: &TaskState,
    config: &mut Config,
    task_store: &mut Option<TaskStore>,
    next_task_store: Option<TaskStore>,
    config_writer: impl FnOnce(&Config) -> Result<(), ConfigError>,
) -> Vec<FileWriteError> {
    let mut errors = Vec::new();
    if let Err(error) = sync_task_persistence(config, &updated, task_state, &next_task_store) {
        errors.push(FileWriteError::Tasks(error));
    }
    if let Err(error) = save_config(&updated, config_writer) {
        errors.push(FileWriteError::Config(error));
    }
    install_runtime_settings(updated, config, task_store, next_task_store);
    errors
}

fn save_config(
    updated: &Config,
    writer: impl FnOnce(&Config) -> Result<(), ConfigError>,
) -> Result<(), ConfigError> {
    writer(updated)
}

fn sync_task_persistence(
    current: &Config,
    updated: &Config,
    task_state: &TaskState,
    next_task_store: &Option<TaskStore>,
) -> Result<(), TaskPersistenceError> {
    let enabling = !current.tasks().persist() && updated.tasks().persist();
    if enabling && let Some(store) = next_task_store {
        store.replace_with_backup(task_state)?;
    }
    Ok(())
}

fn install_runtime_settings(
    updated: Config,
    config: &mut Config,
    task_store: &mut Option<TaskStore>,
    next_task_store: Option<TaskStore>,
) {
    *config = updated;
    *task_store = next_task_store;
}

#[derive(Debug)]
pub(crate) enum FileWriteError {
    Tasks(TaskPersistenceError),
    Config(ConfigError),
}

fn report_write_failure(app: &mut App, failure: &FileWriteError) {
    let (name, error): (&str, &(dyn Error + 'static)) = match failure {
        FileWriteError::Tasks(error) => ("tasks.toml", error),
        FileWriteError::Config(error) => ("config.toml", error),
    };
    let reason =
        underlying_io_error(error).map_or("a file error occurred", |error| match error.kind() {
            io::ErrorKind::PermissionDenied => "permission denied",
            io::ErrorKind::StorageFull | io::ErrorKind::WriteZero => "storage is full",
            io::ErrorKind::ReadOnlyFilesystem => "the filesystem is read-only",
            io::ErrorKind::NotFound => "the save location was not found",
            _ => "a file error occurred",
        });
    let message = format!("Could not save {name}: {reason}. Changes remain active.");
    let diagnostic = format!("pomock: could not save {name}: {error}");
    match failure {
        FileWriteError::Tasks(_) => app.report_task_write_error(message, diagnostic),
        FileWriteError::Config(_) => app.report_config_write_error(message, diagnostic),
    }
}

fn underlying_io_error<'a>(mut error: &'a (dyn Error + 'static)) -> Option<&'a io::Error> {
    loop {
        if let Some(error) = error.downcast_ref::<io::Error>() {
            return Some(error);
        }
        error = error.source()?;
    }
}

pub(crate) fn task_store_for_config(
    config: &Config,
    workspace_store: &TaskStore,
) -> Option<TaskStore> {
    config.tasks().persist().then(|| workspace_store.clone())
}

#[derive(Debug)]
pub(crate) enum RunError {
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
