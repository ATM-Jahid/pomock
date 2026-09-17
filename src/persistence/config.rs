use std::{
    error::Error,
    fmt, fs, io,
    path::{Path, PathBuf},
};

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

use super::DEFAULT_WORKSPACE;
use crate::atomic_write;
use crate::config::{
    Config, ConfigValidationError, KeysConfig, NotificationConfig, SoundConfig, TasksConfig,
    ThemeConfig, TimerConfig, format_duration, parse_duration,
};

const CONFIG_FILE_NAME: &str = "config.toml";

/// Filesystem boundary for durable configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigStore {
    path: PathBuf,
}

impl ConfigStore {
    /// Uses the platform-appropriate main profile configuration path.
    pub fn user() -> Result<Self, ConfigError> {
        Self::user_in_workspace(None)
    }

    /// Uses the per-user configuration file for a workspace, defaulting to `main`.
    pub fn user_in_workspace(workspace: Option<&str>) -> Result<Self, ConfigError> {
        let path = ProjectDirs::from("", "", "pomock")
            .map(|dirs| {
                dirs.config_dir()
                    .join(workspace.unwrap_or(DEFAULT_WORKSPACE))
                    .join(CONFIG_FILE_NAME)
            })
            .ok_or(ConfigError::DirectoryUnavailable)?;
        Ok(Self { path })
    }

    /// Uses an explicit file path, primarily for embedding and tests.
    pub fn at(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// Returns the backing path used by this store.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Loads configuration, using defaults when the file is absent.
    pub fn load(&self) -> Result<Config, ConfigError> {
        let path = self.path();
        let contents = match fs::read_to_string(path) {
            Ok(contents) => contents,
            Err(source) if source.kind() == io::ErrorKind::NotFound => return Ok(Config::default()),
            Err(source) => {
                return Err(ConfigError::Read {
                    path: path.to_owned(),
                    source,
                });
            }
        };
        Self::parse_contents(path, &contents)
    }

    fn parse_contents(path: &Path, contents: &str) -> Result<Config, ConfigError> {
        let original: toml::Value =
            toml::from_str(contents).map_err(|source| ConfigError::Parse {
                path: path.to_owned(),
                source,
            })?;
        let defaults = toml::Value::try_from(StoredConfig::from(&Config::default()))
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

    /// Initializes a missing workspace config from a valid main profile file.
    /// Missing or invalid main profile contents produce defaults instead.
    /// Existing workspace files are never overwritten.
    pub fn create_workspace_file(&self, main_config_store: &Self) -> Result<bool, ConfigError> {
        let path = self.path();
        if path.try_exists().map_err(|source| ConfigError::Read {
            path: path.to_owned(),
            source,
        })? {
            return Ok(false);
        }
        let main_config_path = main_config_store.path();
        let contents = match fs::read_to_string(main_config_path) {
            Ok(contents) if Self::parse_contents(main_config_path, &contents).is_ok() => contents,
            Ok(_) => return self.create_default_file(),
            Err(source)
                if matches!(
                    source.kind(),
                    io::ErrorKind::NotFound | io::ErrorKind::InvalidData
                ) =>
            {
                return self.create_default_file();
            }
            Err(source) => {
                return Err(ConfigError::Read {
                    path: main_config_path.to_owned(),
                    source,
                });
            }
        };
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|source| ConfigError::CreateDirectory {
                path: parent.to_owned(),
                source,
            })?;
        }
        atomic_write::write_new(path, contents.as_bytes()).map_err(|source| ConfigError::Write {
            path: path.to_owned(),
            source,
        })
    }

    /// Creates a default configuration if this store's path does not currently exist.
    ///
    /// Returns whether this call created the file.
    pub fn create_default_file(&self) -> Result<bool, ConfigError> {
        let path = self.path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|source| ConfigError::CreateDirectory {
                path: parent.to_owned(),
                source,
            })?;
        }
        let contents = toml::to_string_pretty(&StoredConfig::from(&Config::default()))
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
        &self,
        expected: &[u8],
    ) -> Result<Option<PathBuf>, ConfigError> {
        let path = self.path();
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

        self.save(&Config::default())?;
        Ok(Some(backup))
    }

    /// Creates a timestamped recovery copy beside an existing configuration.
    pub fn backup_file(&self) -> Result<PathBuf, ConfigError> {
        let path = self.path();
        let contents = fs::read(path).map_err(|source| ConfigError::Read {
            path: path.to_owned(),
            source,
        })?;
        atomic_write::backup(path, &contents).map_err(|source| ConfigError::Backup {
            path: path.to_owned(),
            source,
        })
    }

    /// Saves configuration to this store, creating its parent directory.
    pub fn save(&self, config: &Config) -> Result<(), ConfigError> {
        let path = self.path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|source| ConfigError::CreateDirectory {
                path: parent.to_owned(),
                source,
            })?;
        }

        let contents =
            toml::to_string_pretty(&StoredConfig::from(config)).map_err(ConfigError::Serialize)?;
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
                long_break_interval: timer.long_break_interval().get(),
                autostart_breaks: timer.autostart_breaks(),
                autostart_focus: timer.autostart_focus(),
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
    Backup {
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
            Self::Backup { path, source } => write!(
                formatter,
                "could not back up configuration file {}: {source}",
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
            | Self::Backup { source, .. }
            | Self::Write { source, .. } => Some(source),
            Self::Parse { source, .. } => Some(source),
            Self::Validation { source, .. } => Some(source),
            Self::Serialize(source) => Some(source),
        }
    }
}

#[cfg(test)]
#[path = "config/tests.rs"]
mod tests;
