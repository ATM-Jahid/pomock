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

pub use keys::{ConfigKey, KeyAction, KeysConfig};
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
mod tests {
    use std::{
        fs,
        path::PathBuf,
        sync::atomic::{AtomicU64, Ordering},
    };

    use super::{
        CompletionSoundConfig, Config, ConfigError, ConfigKey, ConfigValidationError,
        FocusSoundConfig, KeyBindings, KeysConfig, NotificationConfig, SoundConfig, TasksConfig,
        ThemeColor, ThemeConfig, TimerConfig,
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
        assert!(!config.timer().autostart_breaks());
        assert!(!config.timer().autostart_focus());
        assert!(config.tasks().persist());
        assert!(config.tasks().show_numbers());
        assert_eq!(config.theme(), &ThemeConfig::default());
        assert_eq!(config.keys(), &KeysConfig::default());
        assert!(config.notification().enabled());
        assert_eq!(config.sound(), &SoundConfig::default());
        assert!(!config.sound().completion().enabled());
        assert!(config.sound().completion().file().is_none());
        assert!(!config.sound().focus().enabled());
        assert!(config.sound().focus().file().is_none());
        assert_eq!(
            config.keys().list_down(),
            [ConfigKey::Character('j'), ConfigKey::Down]
        );
        assert_eq!(
            config.keys().list_up(),
            [ConfigKey::Character('k'), ConfigKey::Up]
        );
        assert_eq!(config.keys().settings(), [ConfigKey::Character('s')]);
        assert_eq!(config.keys().move_task_up(), [ConfigKey::Character('u')]);
        assert_eq!(config.keys().move_task_down(), [ConfigKey::Character('d')]);
    }

    #[test]
    fn missing_file_uses_defaults() {
        let path = temp_path("missing.toml");

        assert_eq!(Config::load_from(path).unwrap(), Config::default());
    }

    #[test]
    fn saved_default_config_follows_the_documented_settings_order() {
        let path = temp_path("ordered/config.toml");
        Config::default().save_to(&path).unwrap();

        let contents = fs::read_to_string(&path).unwrap();
        assert_eq!(
            contents,
            concat!(
                "[timer]\n",
                "focus_duration = \"25:00\"\n",
                "short_break_duration = \"05:00\"\n",
                "long_break_duration = \"15:00\"\n",
                "long_break_interval = 4\n",
                "autostart_breaks = false\n",
                "autostart_focus = false\n",
                "\n",
                "[notification]\n",
                "enabled = true\n",
                "\n",
                "[sound.completion]\n",
                "enabled = false\n",
                "\n",
                "[sound.focus]\n",
                "enabled = false\n",
                "\n",
                "[tasks]\n",
                "persist = true\n",
                "show_numbers = true\n",
                "\n",
                "[keys]\n",
                "quit = \"q\"\n",
                "settings = \"s\"\n",
                "focus_left = \"H\"\n",
                "focus_down = \"J\"\n",
                "focus_up = \"K\"\n",
                "focus_right = \"L\"\n",
                "clock_primary = \"space\"\n",
                "cycle_session = \"c\"\n",
                "reset_session = \"r\"\n",
                "add_task = \"a\"\n",
                "edit_task = \"e\"\n",
                "delete_task = \"x\"\n",
                "task_primary = \"space\"\n",
                "list_down = [\n",
                "    \"j\",\n",
                "    \"down\",\n",
                "]\n",
                "list_up = [\n",
                "    \"k\",\n",
                "    \"up\",\n",
                "]\n",
                "move_task_up = \"u\"\n",
                "move_task_down = \"d\"\n",
                "\n",
                "[theme]\n",
                "focused_border = \"light_red\"\n",
                "unfocused_border = \"dark_gray\"\n",
                "focus = \"magenta\"\n",
                "short_break = \"cyan\"\n",
                "long_break = \"green\"\n",
                "todo_highlight = \"red\"\n",
                "done_highlight = \"green\"\n",
            )
        );
        fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn partial_nested_sound_sections_remain_disabled_without_enabled_flags() {
        let path = temp_path("partial-sound.toml");
        fs::write(
            &path,
            "[timer]\nfocus_duration = \"25:00\"\nshort_break_duration = \"05:00\"\nlong_break_duration = \"15:00\"\nlong_break_interval = 4\n\n[sound.completion]\nfile = \"~/complete.wav\"\n\n[sound.focus]\nfile = \"~/focus.wav\"\n",
        )
        .unwrap();

        let config = Config::load_from(&path).unwrap();

        assert!(!config.sound().completion().enabled());
        assert!(config.sound().completion().playback_file().is_none());
        assert!(!config.sound().focus().enabled());
        assert!(config.sound().focus().playback_file().is_none());
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn notification_and_sound_files_round_trip() {
        let path = temp_path("sound/config.toml");
        let sound_file = PathBuf::from("~/sounds/session-complete.mp3");
        let focus_file = PathBuf::from("~/sounds/focus.ogg");
        let config = Config::default()
            .with_notification(NotificationConfig::new(false))
            .with_sound(
                SoundConfig::default()
                    .with_completion(CompletionSoundConfig::new(true, Some(sound_file.clone())))
                    .with_focus(FocusSoundConfig::new(true, Some(focus_file.clone()))),
            )
            .unwrap();

        config.save_to(&path).unwrap();

        assert_eq!(Config::load_from(&path).unwrap(), config);
        assert_eq!(
            config.sound().completion().playback_file(),
            Some(
                directories::UserDirs::new()
                    .unwrap()
                    .home_dir()
                    .join("sounds/session-complete.mp3")
                    .as_path()
            )
        );
        assert_eq!(
            config.sound().focus().playback_file(),
            Some(
                directories::UserDirs::new()
                    .unwrap()
                    .home_dir()
                    .join("sounds/focus.ogg")
                    .as_path()
            )
        );
        let contents = fs::read_to_string(&path).unwrap();
        assert!(contents.contains("[notification]"));
        assert!(contents.contains("enabled = false"));
        assert!(contents.contains("[sound.completion]"));
        assert!(contents.contains("enabled = true"));
        assert!(contents.contains("file = \"~/sounds/session-complete.mp3\""));
        assert!(contents.contains("[sound.focus]"));
        assert!(contents.contains("file = \"~/sounds/focus.ogg\""));

        fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn absolute_sound_paths_pass_through_unchanged() {
        let sound_file = temp_path("absolute-sound.wav");
        let config = Config::default()
            .with_sound(
                SoundConfig::default()
                    .with_completion(CompletionSoundConfig::new(true, Some(sound_file.clone()))),
            )
            .unwrap();

        assert_eq!(
            config.sound().completion().file(),
            Some(sound_file.as_path())
        );
        assert_eq!(
            config.sound().completion().playback_file(),
            Some(sound_file.as_path())
        );
    }

    #[test]
    fn other_relative_sound_paths_are_rejected_with_the_config_path() {
        let path = temp_path("relative-sound.toml");
        fs::write(
            &path,
            "[timer]\nfocus_duration = \"25:00\"\nshort_break_duration = \"05:00\"\nlong_break_duration = \"15:00\"\nlong_break_interval = 4\n\n[sound.completion]\nenabled = true\nfile = \"sounds/done.wav\"\n",
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
    fn relative_focus_sound_paths_report_the_precise_field_and_config_path() {
        let path = temp_path("relative-focus-sound.toml");
        fs::write(
            &path,
            "[timer]\nfocus_duration = \"25:00\"\nshort_break_duration = \"05:00\"\nlong_break_duration = \"15:00\"\nlong_break_interval = 4\n\n[sound.focus]\nenabled = true\nfile = \"sounds/focus.wav\"\n",
        )
        .unwrap();

        let error = Config::load_from(&path).unwrap_err();

        assert!(error.to_string().contains(path.to_str().unwrap()));
        assert!(error.to_string().contains("sound.focus.file"));
        assert!(error.to_string().contains("sounds/focus.wav"));
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn saves_and_loads_a_valid_toml_round_trip() {
        let path = temp_path("round-trip/config.toml");
        let config = Config::with_settings(
            TimerConfig::from_seconds(50 * 60 + 30, 10 * 60 + 15, 30 * 60 + 45, 3)
                .unwrap()
                .with_autostart(true, false),
            TasksConfig::with_numbering(false, false),
            ThemeConfig::new(
                ThemeColor::LightBlue,
                ThemeColor::Black,
                ThemeColor::LightYellow,
                ThemeColor::LightGreen,
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
        assert!(contents.contains("focus_duration = \"50:30\""));
        assert!(contents.contains("autostart_breaks = true"));
        assert!(contents.contains("autostart_focus = false"));
        assert!(contents.contains("[tasks]"));
        assert!(contents.contains("persist = false"));
        assert!(contents.contains("show_numbers = false"));
        assert!(contents.contains("[theme]"));
        assert!(contents.contains("focused_border = \"light_blue\""));
        assert!(!contents.contains("completed_sessions"));
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
            "[timer]\nfocus_duration = \"25:00\"\nshort_break_duration = \"05:00\"\nlong_break_duration = \"15:00\"\nlong_break_interval = 4\n",
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
            "[timer]\nfocus_duration = \"25:00\"\nshort_break_duration = \"05:00\"\nlong_break_duration = \"15:00\"\nlong_break_interval = 4\n\n[keys]\ncycle_session = \"n\"\nclock_primary = \"enter\"\n",
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
            "[timer]\nfocus_duration = \"25:00\"\nshort_break_duration = \"05:00\"\nlong_break_duration = \"15:00\"\nlong_break_interval = 4\n\n[keys]\nlist_down = [\"j\", \"down\"]\nlist_up = [\"k\", \"up\"]\nquit = \"q\"\n",
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
            "[timer]\nfocus_duration = \"25:00\"\nshort_break_duration = \"05:00\"\nlong_break_duration = \"15:00\"\nlong_break_interval = 4\n\n[keys]\nquit = []\n",
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
            "[timer]\nfocus_duration = \"25:00\"\nshort_break_duration = \"05:00\"\nlong_break_duration = \"15:00\"\nlong_break_interval = 4\n\n[keys]\ncycle_session = [\"c\", \"q\"]\n",
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
            "[timer]\nfocus_duration = \"25:00\"\nshort_break_duration = \"05:00\"\nlong_break_duration = \"15:00\"\nlong_break_interval = 4\n\n[keys]\ncycle_session = \"esc\"\n",
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
            "[timer]\nfocus_duration = \"25:00\"\nshort_break_duration = \"05:00\"\nlong_break_duration = \"15:00\"\nlong_break_interval = 4\n\n[keys]\nsettings = \"enter\"\n",
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
            "[timer]\nfocus_duration = \"25:00\"\nshort_break_duration = \"05:00\"\nlong_break_duration = \"15:00\"\nlong_break_interval = 4\n\n[keys]\ncycle_session = \"page_down\"\n",
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
            "[timer]\nfocus_duration = \"25:00\"\nshort_break_duration = \"05:00\"\nlong_break_duration = \"15:00\"\nlong_break_interval = 4\n\n[keys]\ncycle_session = \"q\"\n",
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
    fn task_movement_keys_share_task_context_validation() {
        let path = temp_path("conflicting-task-movement-key.toml");
        fs::write(
            &path,
            "[timer]\nfocus_duration = \"25:00\"\nshort_break_duration = \"05:00\"\nlong_break_duration = \"15:00\"\nlong_break_interval = 4\n\n[keys]\nmove_task_up = \"a\"\nmove_task_down = \"d\"\n",
        )
        .unwrap();

        let error = Config::load_from(&path).unwrap_err();

        assert!(matches!(error, ConfigError::Validation { .. }));
        assert!(error.to_string().contains("keys.add_task"));
        assert!(error.to_string().contains("keys.move_task_up"));
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
            "[timer]\nfocus_duration = \"25:00\"\nshort_break_duration = \"05:00\"\nlong_break_duration = \"15:00\"\nlong_break_interval = 4\n\n[theme]\nfocused_border = \"light_cyan\"\n",
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
            "[timer]\nfocus_duration = \"25:00\"\nshort_break_duration = \"05:00\"\nlong_break_duration = \"15:00\"\nlong_break_interval = 4\n\n[theme]\nfocused_border = \"#5FD7fF\"\n",
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
            "[timer]\nfocus_duration = \"25:00\"\nshort_break_duration = \"05:00\"\nlong_break_duration = \"15:00\"\nlong_break_interval = 4\n\n[theme]\nfocused_border = \"orange\"\nunfocused_border = \"dark_gray\"\ntodo_highlight = \"yellow\"\ndone_highlight = \"green\"\n",
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
            "[timer]\nfocus_duration = \"25:00\"\nshort_break_duration = \"05:00\"\nlong_break_duration = \"15:00\"\nlong_break_interval = 4\n\n[tasks]\npersist = false\n",
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
        fs::write(&path, "[timer\nfocus_duration = \"25:00\"").unwrap();

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
                field: "focus_duration"
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
            "[timer]\nfocus_duration = \"25:00\"\nshort_break_duration = \"05:00\"\nlong_break_duration = \"15:00\"\nlong_break_interval = 0\n",
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
