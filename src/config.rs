use std::{
    error::Error,
    fmt, fs, io,
    num::NonZeroU32,
    path::{Path, PathBuf},
    time::Duration,
};

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

const CONFIG_FILE_NAME: &str = "config.toml";
const SECONDS_PER_MINUTE: u64 = 60;

/// Durable user settings shared by the application and future settings UI.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Config {
    timer: TimerConfig,
}

impl Config {
    /// Creates and validates configuration expressed in whole minutes.
    pub fn new(timer: TimerConfig) -> Result<Self, ConfigValidationError> {
        timer.validate()?;
        Ok(Self { timer })
    }

    /// Returns the validated timer settings.
    pub fn timer(&self) -> &TimerConfig {
        &self.timer
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
        fs::write(path, contents).map_err(|source| ConfigError::Write {
            path: path.to_owned(),
            source,
        })
    }
}

/// Timer values as presented in the user configuration file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimerConfig {
    focus_minutes: u64,
    short_break_minutes: u64,
    long_break_minutes: u64,
    long_break_interval: u32,
}

impl TimerConfig {
    pub fn new(
        focus_minutes: u64,
        short_break_minutes: u64,
        long_break_minutes: u64,
        long_break_interval: u32,
    ) -> Result<Self, ConfigValidationError> {
        let timer = Self {
            focus_minutes,
            short_break_minutes,
            long_break_minutes,
            long_break_interval,
        };
        timer.validate()?;
        Ok(timer)
    }

    pub fn focus_duration(&self) -> Duration {
        Duration::from_secs(self.focus_minutes * SECONDS_PER_MINUTE)
    }

    pub fn short_break_duration(&self) -> Duration {
        Duration::from_secs(self.short_break_minutes * SECONDS_PER_MINUTE)
    }

    pub fn long_break_duration(&self) -> Duration {
        Duration::from_secs(self.long_break_minutes * SECONDS_PER_MINUTE)
    }

    pub fn long_break_interval(&self) -> NonZeroU32 {
        NonZeroU32::new(self.long_break_interval)
            .expect("validated timer configuration has a positive long-break interval")
    }

    fn validate(&self) -> Result<(), ConfigValidationError> {
        for (field, minutes) in [
            ("focus_minutes", self.focus_minutes),
            ("short_break_minutes", self.short_break_minutes),
            ("long_break_minutes", self.long_break_minutes),
        ] {
            if minutes == 0 {
                return Err(ConfigValidationError::ZeroDuration { field });
            }
            if minutes.checked_mul(SECONDS_PER_MINUTE).is_none() {
                return Err(ConfigValidationError::DurationOverflow { field });
            }
        }

        if self.long_break_interval == 0 {
            return Err(ConfigValidationError::ZeroLongBreakInterval);
        }

        Ok(())
    }
}

impl Default for TimerConfig {
    fn default() -> Self {
        Self {
            focus_minutes: 25,
            short_break_minutes: 5,
            long_break_minutes: 15,
            long_break_interval: 4,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigValidationError {
    ZeroDuration { field: &'static str },
    DurationOverflow { field: &'static str },
    ZeroLongBreakInterval,
}

impl fmt::Display for ConfigValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroDuration { field } => write!(formatter, "{field} must be greater than zero"),
            Self::DurationOverflow { field } => write!(formatter, "{field} is too large"),
            Self::ZeroLongBreakInterval => {
                formatter.write_str("long_break_interval must be greater than zero")
            }
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
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredTimerConfig {
    focus_minutes: u64,
    short_break_minutes: u64,
    long_break_minutes: u64,
    long_break_interval: u32,
}

impl TryFrom<StoredConfig> for Config {
    type Error = ConfigValidationError;

    fn try_from(stored: StoredConfig) -> Result<Self, Self::Error> {
        Self::new(TimerConfig::new(
            stored.timer.focus_minutes,
            stored.timer.short_break_minutes,
            stored.timer.long_break_minutes,
            stored.timer.long_break_interval,
        )?)
    }
}

impl From<&Config> for StoredConfig {
    fn from(config: &Config) -> Self {
        let timer = config.timer();
        Self {
            timer: StoredTimerConfig {
                focus_minutes: timer.focus_minutes,
                short_break_minutes: timer.short_break_minutes,
                long_break_minutes: timer.long_break_minutes,
                long_break_interval: timer.long_break_interval,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::PathBuf,
        sync::atomic::{AtomicU64, Ordering},
    };

    use super::{Config, ConfigError, ConfigValidationError, TimerConfig};

    static NEXT_TEMP_PATH: AtomicU64 = AtomicU64::new(0);

    fn temp_path(name: &str) -> PathBuf {
        let unique = NEXT_TEMP_PATH.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "pomock-config-test-{}-{unique}-{name}",
            std::process::id()
        ))
    }

    #[test]
    fn defaults_match_the_product_contract() {
        let config = Config::default();

        assert_eq!(config.timer().focus_duration().as_secs(), 25 * 60);
        assert_eq!(config.timer().short_break_duration().as_secs(), 5 * 60);
        assert_eq!(config.timer().long_break_duration().as_secs(), 15 * 60);
        assert_eq!(config.timer().long_break_interval().get(), 4);
    }

    #[test]
    fn missing_file_uses_defaults() {
        let path = temp_path("missing.toml");

        assert_eq!(Config::load_from(path).unwrap(), Config::default());
    }

    #[test]
    fn saves_and_loads_a_valid_toml_round_trip() {
        let path = temp_path("round-trip/config.toml");
        let config = Config::new(TimerConfig::new(50, 10, 30, 3).unwrap()).unwrap();

        config.save_to(&path).unwrap();
        assert_eq!(Config::load_from(&path).unwrap(), config);

        let contents = fs::read_to_string(&path).unwrap();
        assert!(contents.contains("[timer]"));
        assert!(contents.contains("focus_minutes = 50"));
        fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn malformed_toml_reports_its_path_and_parse_error() {
        let path = temp_path("malformed.toml");
        fs::write(&path, "[timer\nfocus_minutes = 25").unwrap();

        let error = Config::load_from(&path).unwrap_err();

        assert!(matches!(error, ConfigError::Parse { .. }));
        assert!(error.to_string().contains(path.to_str().unwrap()));
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn zero_duration_is_rejected_with_the_field_name() {
        let error = TimerConfig::new(0, 5, 15, 4).unwrap_err();

        assert_eq!(
            error,
            ConfigValidationError::ZeroDuration {
                field: "focus_minutes"
            }
        );
    }

    #[test]
    fn zero_long_break_interval_is_rejected() {
        let error = TimerConfig::new(25, 5, 15, 0).unwrap_err();

        assert_eq!(error, ConfigValidationError::ZeroLongBreakInterval);
    }

    #[test]
    fn invalid_file_values_include_the_file_path() {
        let path = temp_path("invalid.toml");
        fs::write(
            &path,
            "[timer]\nfocus_minutes = 25\nshort_break_minutes = 5\nlong_break_minutes = 15\nlong_break_interval = 0\n",
        )
        .unwrap();

        let error = Config::load_from(&path).unwrap_err();

        assert!(matches!(error, ConfigError::Validation { .. }));
        assert!(error.to_string().contains(path.to_str().unwrap()));
        assert!(error.to_string().contains("long_break_interval"));
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn read_errors_include_the_file_path() {
        let path = temp_path("directory-instead-of-file");
        fs::create_dir(&path).unwrap();

        let error = Config::load_from(&path).unwrap_err();

        assert!(matches!(error, ConfigError::Read { .. }));
        assert!(error.to_string().contains(path.to_str().unwrap()));
        fs::remove_dir(path).unwrap();
    }

    #[test]
    fn save_errors_include_the_failed_directory() {
        let parent = temp_path("parent-is-file");
        let path = parent.join("config.toml");
        fs::write(&parent, "not a directory").unwrap();

        let error = Config::default().save_to(path).unwrap_err();

        assert!(matches!(error, ConfigError::CreateDirectory { .. }));
        assert!(error.to_string().contains(parent.to_str().unwrap()));
        fs::remove_file(parent).unwrap();
    }
}
