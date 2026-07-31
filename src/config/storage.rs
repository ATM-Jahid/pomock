use std::{
    fs, io,
    path::{Path, PathBuf},
};

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

        let original: toml::Value =
            toml::from_str(&contents).map_err(|source| ConfigError::Parse {
                path: path.to_owned(),
                source,
            })?;
        let defaults = toml::Value::try_from(StoredConfig::from(&Self::default()))
            .expect("the default configuration is serializable");
        let merged = merge_with_defaults(&original, &defaults);
        let stored: StoredConfig = merged.try_into().map_err(|source| ConfigError::Parse {
            path: path.to_owned(),
            source,
        })?;

        stored.try_into().map_err(|source| ConfigError::Validation {
            path: path.to_owned(),
            source,
        })
    }

    /// Creates a default configuration if `path` does not currently exist.
    ///
    /// Returns whether this call created the file.
    pub fn create_default_file(path: impl AsRef<Path>) -> Result<bool, ConfigError> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|source| ConfigError::CreateDirectory {
                path: parent.to_owned(),
                source,
            })?;
        }
        let contents = toml::to_string_pretty(&StoredConfig::from(&Self::default()))
            .map_err(ConfigError::Serialize)?;
        atomic_write::write_new(path, contents.as_bytes()).map_err(|source| ConfigError::Write {
            path: path.to_owned(),
            source,
        })
    }

    /// Replaces an invalid configuration only if it still has the expected contents.
    ///
    /// Returns the backup path, or `None` when the file changed before replacement.
    pub fn replace_with_default_if_unchanged(
        path: impl AsRef<Path>,
        expected: &[u8],
    ) -> Result<Option<PathBuf>, ConfigError> {
        let path = path.as_ref();
        if fs::read(path).map_err(|source| ConfigError::Read {
            path: path.to_owned(),
            source,
        })? != expected
        {
            return Ok(None);
        }

        let backup =
            atomic_write::backup(path, expected).map_err(|source| ConfigError::Backup {
                path: path.to_owned(),
                source,
            })?;
        if fs::read(path).map_err(|source| ConfigError::Read {
            path: path.to_owned(),
            source,
        })? != expected
        {
            return Ok(None);
        }

        Self::default().save_to(path)?;
        Ok(Some(backup))
    }

    /// Creates a timestamped recovery copy beside an existing configuration.
    pub fn backup_file(path: impl AsRef<Path>) -> Result<PathBuf, ConfigError> {
        let path = path.as_ref();
        let contents = fs::read(path).map_err(|source| ConfigError::Read {
            path: path.to_owned(),
            source,
        })?;
        atomic_write::backup(path, &contents).map_err(|source| ConfigError::Backup {
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

fn merge_with_defaults(existing: &toml::Value, defaults: &toml::Value) -> toml::Value {
    let (Some(existing), Some(defaults)) = (existing.as_table(), defaults.as_table()) else {
        return existing.clone();
    };
    let mut merged = defaults.clone();
    for (key, value) in existing {
        let value = defaults.get(key).map_or_else(
            || value.clone(),
            |default| merge_with_defaults(value, default),
        );
        merged.insert(key.clone(), value);
    }
    toml::Value::Table(merged)
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredConfig {
    timer: StoredTimerConfig,
    notification: NotificationConfig,
    sound: SoundConfig,
    tasks: StoredTasksConfig,
    keys: KeysConfig,
    theme: ThemeConfig,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredTimerConfig {
    focus_duration: String,
    short_break_duration: String,
    long_break_duration: String,
    long_break_interval: u32,
    autostart_breaks: bool,
    autostart_focus: bool,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredTasksConfig {
    persist: bool,
    show_numbers: bool,
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
