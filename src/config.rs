use std::{
    error::Error,
    fmt, fs, io,
    path::{Path, PathBuf},
};

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

use crate::atomic_write;

mod keys;
mod notification;
mod sound;
mod tasks;
mod theme;
mod timer;

pub use keys::{ConfigKey, ConfigKeyKind, KeyAction, KeysConfig};
pub use notification::NotificationConfig;
pub use sound::{CompletionSoundConfig, FocusSoundConfig, SoundConfig};
pub use tasks::TasksConfig;
pub use theme::{ThemeColor, ThemeConfig, ThemeRole};
pub use timer::TimerConfig;
pub(crate) use timer::{format_duration, parse_duration};

#[cfg(test)]
use keys::KeyBindings;

const CONFIG_FILE_NAME: &str = "config.toml";

/// Durable user settings shared by the application and future settings UI.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Config {
    timer: TimerConfig,
    tasks: TasksConfig,
    theme: ThemeConfig,
    keys: KeysConfig,
    notification: NotificationConfig,
    sound: SoundConfig,
}

impl Config {
    /// Creates and validates configuration with explicit timer settings.
    pub fn new(timer: TimerConfig) -> Result<Self, ConfigValidationError> {
        Self::with_tasks(timer, TasksConfig::default())
    }

    /// Creates and validates configuration with explicit task settings.
    pub fn with_tasks(
        timer: TimerConfig,
        tasks: TasksConfig,
    ) -> Result<Self, ConfigValidationError> {
        Self::with_tasks_and_theme(timer, tasks, ThemeConfig::default())
    }

    /// Creates and validates configuration with explicit task and theme settings.
    pub fn with_tasks_and_theme(
        timer: TimerConfig,
        tasks: TasksConfig,
        theme: ThemeConfig,
    ) -> Result<Self, ConfigValidationError> {
        Self::with_settings(timer, tasks, theme, KeysConfig::default())
    }

    /// Creates and validates all durable application settings.
    pub fn with_settings(
        timer: TimerConfig,
        tasks: TasksConfig,
        theme: ThemeConfig,
        keys: KeysConfig,
    ) -> Result<Self, ConfigValidationError> {
        Self::with_all_settings(
            timer,
            tasks,
            theme,
            keys,
            NotificationConfig::default(),
            SoundConfig::default(),
        )
    }

    pub(crate) fn with_all_settings(
        timer: TimerConfig,
        tasks: TasksConfig,
        theme: ThemeConfig,
        keys: KeysConfig,
        notification: NotificationConfig,
        mut sound: SoundConfig,
    ) -> Result<Self, ConfigValidationError> {
        timer.validate()?;
        keys.validate()?;
        sound.validate()?;
        Ok(Self {
            timer,
            tasks,
            theme,
            keys,
            notification,
            sound,
        })
    }

    /// Returns the validated timer settings.
    pub fn timer(&self) -> &TimerConfig {
        &self.timer
    }

    /// Returns the durable task settings.
    pub fn tasks(&self) -> &TasksConfig {
        &self.tasks
    }

    /// Returns the semantic UI theme settings.
    pub fn theme(&self) -> &ThemeConfig {
        &self.theme
    }

    /// Returns the contextual normal-mode key bindings.
    pub fn keys(&self) -> &KeysConfig {
        &self.keys
    }

    /// Returns native desktop-notification settings.
    pub const fn notification(&self) -> NotificationConfig {
        self.notification
    }

    /// Replaces native desktop-notification settings.
    pub fn with_notification(mut self, notification: NotificationConfig) -> Self {
        self.notification = notification;
        self
    }

    /// Returns the optional file-backed completion sound settings.
    pub fn sound(&self) -> &SoundConfig {
        &self.sound
    }

    /// Replaces the optional completion sound settings.
    pub fn with_sound(mut self, mut sound: SoundConfig) -> Result<Self, ConfigValidationError> {
        sound.validate()?;
        self.sound = sound;
        Ok(self)
    }

    /// Returns the platform-appropriate per-user configuration path.
    pub fn path() -> Result<PathBuf, ConfigError> {
        ProjectDirs::from("", "", "pomock")
            .map(|dirs| dirs.config_dir().join(CONFIG_FILE_NAME))
            .ok_or(ConfigError::DirectoryUnavailable)
    }

    /// Loads the user configuration, using defaults when the file is absent.
    pub fn load() -> Result<Self, ConfigError> {
        Self::load_from(Self::path()?)
    }

    /// Loads configuration from an explicit path.
    pub fn load_from(path: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let path = path.as_ref();
        let contents = match fs::read_to_string(path) {
            Ok(contents) => contents,
            Err(source) if source.kind() == io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(source) => {
                return Err(ConfigError::Read {
                    path: path.to_owned(),
                    source,
                });
            }
        };

        let stored: StoredConfig =
            toml::from_str(&contents).map_err(|source| ConfigError::Parse {
                path: path.to_owned(),
                source,
            })?;

        stored.try_into().map_err(|source| ConfigError::Validation {
            path: path.to_owned(),
            source,
        })
    }

    /// Saves the configuration to the platform-appropriate user path.
    pub fn save(&self) -> Result<(), ConfigError> {
        self.save_to(Self::path()?)
    }

    /// Saves configuration to an explicit path, creating its parent directory.
    pub fn save_to(&self, path: impl AsRef<Path>) -> Result<(), ConfigError> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|source| ConfigError::CreateDirectory {
                path: parent.to_owned(),
                source,
            })?;
        }

        let contents =
            toml::to_string_pretty(&StoredConfig::from(self)).map_err(ConfigError::Serialize)?;
        atomic_write::write(path, contents.as_bytes()).map_err(|source| ConfigError::Write {
            path: path.to_owned(),
            source,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigValidationError {
    ZeroDuration {
        field: &'static str,
    },
    DurationOverflow {
        field: &'static str,
    },
    InvalidDuration {
        field: &'static str,
    },
    ZeroLongBreakInterval,
    EmptyKeyBindings {
        field: &'static str,
    },
    ConflictingKeys {
        first: &'static str,
        second: &'static str,
    },
    ReservedKey {
        field: &'static str,
        key: ConfigKey,
    },
    SettingsOverlayKey {
        key: ConfigKey,
    },
    RelativeSoundPath {
        field: &'static str,
        path: PathBuf,
    },
    HomeDirectoryUnavailable {
        field: &'static str,
    },
}

impl fmt::Display for ConfigValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroDuration { field } => write!(formatter, "{field} must be greater than zero"),
            Self::DurationOverflow { field } => write!(formatter, "{field} is too large"),
            Self::InvalidDuration { field } => {
                write!(
                    formatter,
                    "{field} must use MM:SS with minutes from 00 to 9999 and seconds from 00 to 59"
                )
            }
            Self::ZeroLongBreakInterval => {
                formatter.write_str("long_break_interval must be greater than zero")
            }
            Self::EmptyKeyBindings { field } => {
                write!(formatter, "keys.{field} must contain at least one key")
            }
            Self::ConflictingKeys { first, second } => {
                write!(formatter, "keys.{first} conflicts with keys.{second}")
            }
            Self::ReservedKey { field, key } => {
                write!(formatter, "keys.{field} cannot use reserved key {key}")
            }
            Self::SettingsOverlayKey { key } => write!(
                formatter,
                "keys.settings cannot use fixed settings-overlay control {key}"
            ),
            Self::RelativeSoundPath { field, path } => write!(
                formatter,
                "{field} must be an absolute path or start with ~/; got {}",
                path.display()
            ),
            Self::HomeDirectoryUnavailable { field } => write!(
                formatter,
                "could not expand {field} because the home directory is unavailable"
            ),
        }
    }
}

impl Error for ConfigValidationError {}

#[derive(Debug)]
pub enum ConfigError {
    DirectoryUnavailable,
    Read {
        path: PathBuf,
        source: io::Error,
    },
    Parse {
        path: PathBuf,
        source: toml::de::Error,
    },
    Validation {
        path: PathBuf,
        source: ConfigValidationError,
    },
    CreateDirectory {
        path: PathBuf,
        source: io::Error,
    },
    Serialize(toml::ser::Error),
    Write {
        path: PathBuf,
        source: io::Error,
    },
}

impl fmt::Display for ConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DirectoryUnavailable => {
                formatter.write_str("could not determine the user configuration directory")
            }
            Self::Read { path, source } => {
                write!(formatter, "could not read {}: {source}", path.display())
            }
            Self::Parse { path, source } => {
                write!(formatter, "could not parse {}: {source}", path.display())
            }
            Self::Validation { path, source } => {
                write!(
                    formatter,
                    "invalid configuration in {}: {source}",
                    path.display()
                )
            }
            Self::CreateDirectory { path, source } => write!(
                formatter,
                "could not create configuration directory {}: {source}",
                path.display()
            ),
            Self::Serialize(source) => {
                write!(formatter, "could not serialize configuration: {source}")
            }
            Self::Write { path, source } => {
                write!(formatter, "could not write {}: {source}", path.display())
            }
        }
    }
}

impl Error for ConfigError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::DirectoryUnavailable => None,
            Self::Read { source, .. }
            | Self::CreateDirectory { source, .. }
            | Self::Write { source, .. } => Some(source),
            Self::Parse { source, .. } => Some(source),
            Self::Validation { source, .. } => Some(source),
            Self::Serialize(source) => Some(source),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredConfig {
    timer: StoredTimerConfig,
    #[serde(default)]
    notification: NotificationConfig,
    #[serde(default)]
    sound: SoundConfig,
    #[serde(default)]
    tasks: StoredTasksConfig,
    #[serde(default)]
    keys: KeysConfig,
    #[serde(default)]
    theme: ThemeConfig,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredTimerConfig {
    focus_duration: String,
    short_break_duration: String,
    long_break_duration: String,
    long_break_interval: u32,
    #[serde(default)]
    autostart_breaks: bool,
    #[serde(default)]
    autostart_focus: bool,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredTasksConfig {
    persist: bool,
    #[serde(default = "enabled")]
    show_numbers: bool,
}

impl Default for StoredTasksConfig {
    fn default() -> Self {
        Self {
            persist: true,
            show_numbers: true,
        }
    }
}

fn enabled() -> bool {
    true
}

impl TryFrom<StoredConfig> for Config {
    type Error = ConfigValidationError;

    fn try_from(stored: StoredConfig) -> Result<Self, Self::Error> {
        Self::with_all_settings(
            TimerConfig::from_seconds(
                parse_duration(&stored.timer.focus_duration, "focus_duration")?,
                parse_duration(&stored.timer.short_break_duration, "short_break_duration")?,
                parse_duration(&stored.timer.long_break_duration, "long_break_duration")?,
                stored.timer.long_break_interval,
            )?
            .with_autostart(stored.timer.autostart_breaks, stored.timer.autostart_focus),
            TasksConfig::with_numbering(stored.tasks.persist, stored.tasks.show_numbers),
            stored.theme,
            stored.keys,
            stored.notification,
            stored.sound,
        )
    }
}

impl From<&Config> for StoredConfig {
    fn from(config: &Config) -> Self {
        let timer = config.timer();
        Self {
            timer: StoredTimerConfig {
                focus_duration: format_duration(timer.focus_duration()),
                short_break_duration: format_duration(timer.short_break_duration()),
                long_break_duration: format_duration(timer.long_break_duration()),
                long_break_interval: timer.long_break_interval,
                autostart_breaks: timer.autostart_breaks,
                autostart_focus: timer.autostart_focus,
            },
            notification: config.notification(),
            sound: config.sound().clone(),
            tasks: StoredTasksConfig {
                persist: config.tasks().persist(),
                show_numbers: config.tasks().show_numbers(),
            },
            keys: config.keys().clone(),
            theme: *config.theme(),
        }
    }
}

#[cfg(test)]
#[path = "config/tests.rs"]
mod tests;
