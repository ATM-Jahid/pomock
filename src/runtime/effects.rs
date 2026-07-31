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

pub(crate) fn commit_settings_change(
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
