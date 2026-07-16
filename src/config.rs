use std::{
    error::Error,
    fmt, fs, io,
    path::{Path, PathBuf},
};

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

mod keys;
mod sound;
mod tasks;
mod theme;
mod timer;

pub use keys::{ConfigKey, KeyAction, KeysConfig};
pub use sound::SoundConfig;
pub use tasks::TasksConfig;
pub use theme::{ThemeColor, ThemeConfig, ThemeRole};
pub use timer::TimerConfig;

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
    sound: SoundConfig,
}

impl Config {
    /// Creates and validates configuration expressed in whole minutes.
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
        Self::with_all_settings(timer, tasks, theme, keys, SoundConfig::default())
    }

    pub(crate) fn with_all_settings(
        timer: TimerConfig,
        tasks: TasksConfig,
        theme: ThemeConfig,
        keys: KeysConfig,
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
        fs::write(path, contents).map_err(|source| ConfigError::Write {
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
        path: PathBuf,
    },
    HomeDirectoryUnavailable,
}

impl fmt::Display for ConfigValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroDuration { field } => write!(formatter, "{field} must be greater than zero"),
            Self::DurationOverflow { field } => write!(formatter, "{field} is too large"),
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
            Self::RelativeSoundPath { path } => write!(
                formatter,
                "sound.file must be an absolute path or start with ~/; got {}",
                path.display()
            ),
            Self::HomeDirectoryUnavailable => formatter
                .write_str("could not expand sound.file because the home directory is unavailable"),
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
    tasks: StoredTasksConfig,
    #[serde(default)]
    theme: ThemeConfig,
    #[serde(default)]
    keys: KeysConfig,
    #[serde(default)]
    sound: SoundConfig,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredTimerConfig {
    focus_minutes: u64,
    short_break_minutes: u64,
    long_break_minutes: u64,
    long_break_interval: u32,
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
            TimerConfig::new(
                stored.timer.focus_minutes,
                stored.timer.short_break_minutes,
                stored.timer.long_break_minutes,
                stored.timer.long_break_interval,
            )?,
            TasksConfig::with_numbering(stored.tasks.persist, stored.tasks.show_numbers),
            stored.theme,
            stored.keys,
            stored.sound,
        )
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
            tasks: StoredTasksConfig {
                persist: config.tasks().persist(),
                show_numbers: config.tasks().show_numbers(),
            },
            theme: *config.theme(),
            keys: config.keys().clone(),
            sound: config.sound().clone(),
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

    use super::{
        Config, ConfigError, ConfigKey, ConfigValidationError, KeyBindings, KeysConfig,
        SoundConfig, TasksConfig, ThemeColor, ThemeConfig, TimerConfig,
    };

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
        assert!(config.tasks().persist());
        assert!(config.tasks().show_numbers());
        assert_eq!(config.theme(), &ThemeConfig::default());
        assert_eq!(config.keys(), &KeysConfig::default());
        assert_eq!(config.sound(), &SoundConfig::default());
        assert_eq!(
            config.keys().list_down(),
            [ConfigKey::Character('j'), ConfigKey::Down]
        );
        assert_eq!(
            config.keys().list_up(),
            [ConfigKey::Character('k'), ConfigKey::Up]
        );
        assert_eq!(config.keys().settings(), [ConfigKey::Character('s')]);
    }

    #[test]
    fn missing_file_uses_defaults() {
        let path = temp_path("missing.toml");

        assert_eq!(Config::load_from(path).unwrap(), Config::default());
    }

    #[test]
    fn sound_file_round_trips_and_is_disabled_when_omitted() {
        let path = temp_path("sound/config.toml");
        let sound_file = PathBuf::from("~/sounds/session-complete.mp3");
        let config = Config::default()
            .with_sound(SoundConfig::new(&sound_file))
            .unwrap();

        config.save_to(&path).unwrap();

        assert_eq!(Config::load_from(&path).unwrap(), config);
        assert_eq!(
            config.sound().resolved_file(),
            Some(
                directories::UserDirs::new()
                    .unwrap()
                    .home_dir()
                    .join("sounds/session-complete.mp3")
                    .as_path()
            )
        );
        let contents = fs::read_to_string(&path).unwrap();
        assert!(contents.contains("[sound]"));
        assert!(contents.contains("file = \"~/sounds/session-complete.mp3\""));

        fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn absolute_sound_paths_pass_through_unchanged() {
        let sound_file = temp_path("absolute-sound.wav");
        let config = Config::default()
            .with_sound(SoundConfig::new(&sound_file))
            .unwrap();

        assert_eq!(config.sound().file(), Some(sound_file.as_path()));
        assert_eq!(config.sound().resolved_file(), Some(sound_file.as_path()));
    }

    #[test]
    fn other_relative_sound_paths_are_rejected_with_the_config_path() {
        let path = temp_path("relative-sound.toml");
        fs::write(
            &path,
            "[timer]\nfocus_minutes = 25\nshort_break_minutes = 5\nlong_break_minutes = 15\nlong_break_interval = 4\n\n[sound]\nfile = \"sounds/done.wav\"\n",
        )
        .unwrap();

        let error = Config::load_from(&path).unwrap_err();

        assert!(matches!(error, ConfigError::Validation { .. }));
        assert!(error.to_string().contains(path.to_str().unwrap()));
        assert!(error.to_string().contains("absolute path or start with ~/"));
        assert!(error.to_string().contains("sounds/done.wav"));
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn saves_and_loads_a_valid_toml_round_trip() {
        let path = temp_path("round-trip/config.toml");
        let config = Config::with_settings(
            TimerConfig::new(50, 10, 30, 3).unwrap(),
            TasksConfig::with_numbering(false, false),
            ThemeConfig::new(
                ThemeColor::LightBlue,
                ThemeColor::Black,
                ThemeColor::LightYellow,
                ThemeColor::LightGreen,
                ThemeColor::Cyan,
            ),
            KeysConfig {
                clock_primary: KeyBindings::one(ConfigKey::Enter),
                cycle_session: KeyBindings::one(ConfigKey::Character('n')),
                ..KeysConfig::default()
            },
        )
        .unwrap();

        config.save_to(&path).unwrap();
        assert_eq!(Config::load_from(&path).unwrap(), config);

        let contents = fs::read_to_string(&path).unwrap();
        assert!(contents.contains("[timer]"));
        assert!(contents.contains("focus_minutes = 50"));
        assert!(contents.contains("[tasks]"));
        assert!(contents.contains("persist = false"));
        assert!(contents.contains("show_numbers = false"));
        assert!(contents.contains("[theme]"));
        assert!(contents.contains("focused_border = \"light_blue\""));
        assert!(contents.contains("[keys]"));
        assert!(contents.contains("clock_primary = \"enter\""));
        assert!(contents.contains("cycle_session = \"n\""));
        fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn existing_config_without_tasks_section_keeps_persistence_enabled() {
        let path = temp_path("legacy.toml");
        fs::write(
            &path,
            "[timer]\nfocus_minutes = 25\nshort_break_minutes = 5\nlong_break_minutes = 15\nlong_break_interval = 4\n",
        )
        .unwrap();

        let config = Config::load_from(&path).unwrap();

        assert!(config.tasks().persist());
        assert!(config.tasks().show_numbers());
        assert_eq!(config.theme(), &ThemeConfig::default());
        assert_eq!(config.keys(), &KeysConfig::default());
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn partial_keys_use_defaults_for_unspecified_commands() {
        let path = temp_path("partial-keys.toml");
        fs::write(
            &path,
            "[timer]\nfocus_minutes = 25\nshort_break_minutes = 5\nlong_break_minutes = 15\nlong_break_interval = 4\n\n[keys]\ncycle_session = \"n\"\nclock_primary = \"enter\"\n",
        )
        .unwrap();

        let config = Config::load_from(&path).unwrap();

        assert_eq!(config.keys().cycle_session(), [ConfigKey::Character('n')]);
        assert_eq!(config.keys().clock_primary(), [ConfigKey::Enter]);
        assert_eq!(config.keys().quit(), [ConfigKey::Character('q')]);

        fs::remove_file(path).unwrap();
    }

    #[test]
    fn keys_accept_ordered_lists_and_single_values() {
        let path = temp_path("multiple-keys.toml");
        fs::write(
            &path,
            "[timer]\nfocus_minutes = 25\nshort_break_minutes = 5\nlong_break_minutes = 15\nlong_break_interval = 4\n\n[keys]\nlist_down = [\"j\", \"down\"]\nlist_up = [\"k\", \"up\"]\nquit = \"q\"\n",
        )
        .unwrap();

        let config = Config::load_from(&path).unwrap();

        assert_eq!(
            config.keys().list_down(),
            [ConfigKey::Character('j'), ConfigKey::Down]
        );
        assert_eq!(
            config.keys().list_up(),
            [ConfigKey::Character('k'), ConfigKey::Up]
        );
        assert_eq!(config.keys().quit(), [ConfigKey::Character('q')]);

        config.save_to(&path).unwrap();
        let contents = fs::read_to_string(&path).unwrap();
        assert!(contents.contains("list_down = [\n    \"j\",\n    \"down\",\n]"));
        assert!(contents.contains("quit = \"q\""));
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn empty_key_lists_are_rejected_with_the_field_path() {
        let path = temp_path("empty-keys.toml");
        fs::write(
            &path,
            "[timer]\nfocus_minutes = 25\nshort_break_minutes = 5\nlong_break_minutes = 15\nlong_break_interval = 4\n\n[keys]\nquit = []\n",
        )
        .unwrap();

        let error = Config::load_from(&path).unwrap_err();

        assert!(matches!(error, ConfigError::Validation { .. }));
        assert!(error.to_string().contains("keys.quit"));
        assert!(error.to_string().contains("at least one key"));
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn conflicts_are_detected_in_secondary_keys() {
        let path = temp_path("conflicting-secondary-key.toml");
        fs::write(
            &path,
            "[timer]\nfocus_minutes = 25\nshort_break_minutes = 5\nlong_break_minutes = 15\nlong_break_interval = 4\n\n[keys]\ncycle_session = [\"c\", \"q\"]\n",
        )
        .unwrap();

        let error = Config::load_from(&path).unwrap_err();

        assert!(matches!(error, ConfigError::Validation { .. }));
        assert!(error.to_string().contains("keys.quit"));
        assert!(error.to_string().contains("keys.cycle_session"));
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn escape_is_reserved_for_modal_cancellation() {
        let path = temp_path("reserved-escape.toml");
        fs::write(
            &path,
            "[timer]\nfocus_minutes = 25\nshort_break_minutes = 5\nlong_break_minutes = 15\nlong_break_interval = 4\n\n[keys]\ncycle_session = \"esc\"\n",
        )
        .unwrap();

        let error = Config::load_from(&path).unwrap_err();

        assert!(matches!(error, ConfigError::Validation { .. }));
        assert!(error.to_string().contains(path.to_str().unwrap()));
        assert!(error.to_string().contains("keys.cycle_session"));
        assert!(error.to_string().contains("reserved key esc"));
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn settings_rejects_fixed_overlay_controls() {
        let path = temp_path("settings-overlay-key.toml");
        fs::write(
            &path,
            "[timer]\nfocus_minutes = 25\nshort_break_minutes = 5\nlong_break_minutes = 15\nlong_break_interval = 4\n\n[keys]\nsettings = \"enter\"\n",
        )
        .unwrap();

        let error = Config::load_from(&path).unwrap_err();

        assert!(matches!(error, ConfigError::Validation { .. }));
        assert!(error.to_string().contains(path.to_str().unwrap()));
        assert!(error.to_string().contains("keys.settings"));
        assert!(
            error
                .to_string()
                .contains("fixed settings-overlay control enter")
        );
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn lowercase_s_can_be_reused_after_settings_is_rebound() {
        let keys = KeysConfig {
            settings: KeyBindings::one(ConfigKey::Character('t')),
            cycle_session: KeyBindings::one(ConfigKey::Character('s')),
            ..KeysConfig::default()
        };

        let config = Config::with_settings(
            TimerConfig::default(),
            TasksConfig::default(),
            ThemeConfig::default(),
            keys,
        )
        .unwrap();

        assert_eq!(config.keys().settings(), [ConfigKey::Character('t')]);
        assert_eq!(config.keys().cycle_session(), [ConfigKey::Character('s')]);
    }

    #[test]
    fn settings_is_validated_as_a_global_action() {
        let keys = KeysConfig {
            settings: KeyBindings::one(ConfigKey::Character('q')),
            ..KeysConfig::default()
        };

        let error = Config::with_settings(
            TimerConfig::default(),
            TasksConfig::default(),
            ThemeConfig::default(),
            keys,
        )
        .unwrap_err();

        assert!(matches!(
            error,
            ConfigValidationError::ConflictingKeys { .. }
        ));
        assert!(error.to_string().contains("keys.quit"));
        assert!(error.to_string().contains("keys.settings"));
    }

    #[test]
    fn invalid_key_name_reports_its_path_and_supported_forms() {
        let path = temp_path("invalid-key.toml");
        fs::write(
            &path,
            "[timer]\nfocus_minutes = 25\nshort_break_minutes = 5\nlong_break_minutes = 15\nlong_break_interval = 4\n\n[keys]\ncycle_session = \"page_down\"\n",
        )
        .unwrap();

        let error = Config::load_from(&path).unwrap_err();

        assert!(matches!(error, ConfigError::Parse { .. }));
        assert!(error.to_string().contains(path.to_str().unwrap()));
        assert!(error.to_string().contains("page_down"));
        assert!(error.to_string().contains("one printable character"));
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn conflicting_contextual_keys_report_both_fields_and_path() {
        let path = temp_path("conflicting-keys.toml");
        fs::write(
            &path,
            "[timer]\nfocus_minutes = 25\nshort_break_minutes = 5\nlong_break_minutes = 15\nlong_break_interval = 4\n\n[keys]\ncycle_session = \"q\"\n",
        )
        .unwrap();

        let error = Config::load_from(&path).unwrap_err();

        assert!(matches!(error, ConfigError::Validation { .. }));
        assert!(error.to_string().contains(path.to_str().unwrap()));
        assert!(error.to_string().contains("keys.quit"));
        assert!(error.to_string().contains("keys.cycle_session"));
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn same_key_is_allowed_for_commands_in_disjoint_contexts() {
        let keys = KeysConfig {
            cycle_session: KeyBindings::one(ConfigKey::Character('a')),
            ..KeysConfig::default()
        };

        assert!(
            Config::with_settings(
                TimerConfig::default(),
                TasksConfig::default(),
                ThemeConfig::default(),
                keys,
            )
            .is_ok()
        );
    }

    #[test]
    fn partial_theme_uses_defaults_for_unspecified_roles() {
        let path = temp_path("partial-theme.toml");
        fs::write(
            &path,
            "[timer]\nfocus_minutes = 25\nshort_break_minutes = 5\nlong_break_minutes = 15\nlong_break_interval = 4\n\n[theme]\nfocused_border = \"light_cyan\"\n",
        )
        .unwrap();

        let config = Config::load_from(&path).unwrap();

        assert_eq!(config.theme().focused_border(), ThemeColor::LightCyan);
        assert_eq!(config.theme().unfocused_border(), ThemeColor::DarkGray);
        assert_eq!(config.theme().done_highlight(), ThemeColor::Green);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn hex_theme_colors_load_and_save_canonically() {
        let path = temp_path("hex-theme.toml");
        fs::write(
            &path,
            "[timer]\nfocus_minutes = 25\nshort_break_minutes = 5\nlong_break_minutes = 15\nlong_break_interval = 4\n\n[theme]\nfocused_border = \"#5FD7fF\"\n",
        )
        .unwrap();

        let config = Config::load_from(&path).unwrap();
        assert_eq!(
            config.theme().focused_border(),
            ThemeColor::Rgb(0x5f, 0xd7, 0xff)
        );

        config.save_to(&path).unwrap();
        assert!(
            fs::read_to_string(&path)
                .unwrap()
                .contains("focused_border = \"#5fd7ff\"")
        );
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn unsupported_theme_color_reports_its_path_and_allowed_values() {
        let path = temp_path("invalid-theme.toml");
        fs::write(
            &path,
            "[timer]\nfocus_minutes = 25\nshort_break_minutes = 5\nlong_break_minutes = 15\nlong_break_interval = 4\n\n[theme]\nfocused_border = \"orange\"\nunfocused_border = \"dark_gray\"\ntodo_highlight = \"yellow\"\ndone_highlight = \"green\"\ncompleted_sessions = \"green\"\n",
        )
        .unwrap();

        let error = Config::load_from(&path).unwrap_err();

        assert!(matches!(error, ConfigError::Parse { .. }));
        assert!(error.to_string().contains(path.to_str().unwrap()));
        assert!(error.to_string().contains("orange"));
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn existing_tasks_section_without_numbering_keeps_numbers_enabled() {
        let path = temp_path("tasks-without-numbering.toml");
        fs::write(
            &path,
            "[timer]\nfocus_minutes = 25\nshort_break_minutes = 5\nlong_break_minutes = 15\nlong_break_interval = 4\n\n[tasks]\npersist = false\n",
        )
        .unwrap();

        let config = Config::load_from(&path).unwrap();

        assert!(!config.tasks().persist());
        assert!(config.tasks().show_numbers());
        fs::remove_file(path).unwrap();
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
