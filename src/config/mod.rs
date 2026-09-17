use std::{error::Error, fmt, path::PathBuf};

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

/// Validated user settings shared by the application and settings UI.
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
    InvalidPositiveInteger {
        field: &'static str,
    },
    IntegerOverflow {
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
            Self::InvalidPositiveInteger { field } => {
                write!(formatter, "{field} must be a positive integer")
            }
            Self::IntegerOverflow { field } => write!(formatter, "{field} is too large"),
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

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
