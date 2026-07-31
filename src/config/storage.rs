use std::{fs, io, path::Path};

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

use super::{
    Config, ConfigError, ConfigValidationError, KeysConfig, NotificationConfig, SoundConfig,
    TasksConfig, ThemeConfig, TimerConfig, format_duration, parse_duration,
};
use crate::atomic_write;

const CONFIG_FILE_NAME: &str = "config.toml";

impl Config {
    /// Returns the platform-appropriate per-user configuration path.
    pub fn path() -> Result<std::path::PathBuf, ConfigError> {
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
