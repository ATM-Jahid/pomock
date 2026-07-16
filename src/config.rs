use std::{
    error::Error,
    fmt, fs, io,
    num::NonZeroU32,
    path::{Path, PathBuf},
    time::Duration,
};

use directories::ProjectDirs;
use serde::{Deserialize, Deserializer, Serialize, Serializer, de};

const CONFIG_FILE_NAME: &str = "config.toml";
const SECONDS_PER_MINUTE: u64 = 60;

/// Durable user settings shared by the application and future settings UI.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Config {
    timer: TimerConfig,
    tasks: TasksConfig,
    theme: ThemeConfig,
    keys: KeysConfig,
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
        timer.validate()?;
        keys.validate()?;
        Ok(Self {
            timer,
            tasks,
            theme,
            keys,
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

/// Durable task behavior and presentation settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TasksConfig {
    persist: bool,
    show_numbers: bool,
}

impl TasksConfig {
    pub fn new(persist: bool) -> Self {
        Self::with_numbering(persist, true)
    }

    pub fn with_numbering(persist: bool, show_numbers: bool) -> Self {
        Self {
            persist,
            show_numbers,
        }
    }

    pub fn persist(&self) -> bool {
        self.persist
    }

    pub fn show_numbers(&self) -> bool {
        self.show_numbers
    }
}

impl Default for TasksConfig {
    fn default() -> Self {
        Self::with_numbering(true, true)
    }
}

/// A portable named terminal color accepted by the shared configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThemeColor {
    Black,
    Red,
    Green,
    Yellow,
    Blue,
    Magenta,
    Cyan,
    Gray,
    DarkGray,
    LightRed,
    LightGreen,
    LightYellow,
    LightBlue,
    LightMagenta,
    LightCyan,
    White,
}

/// Durable colors assigned to semantic presentation roles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ThemeConfig {
    focused_border: ThemeColor,
    unfocused_border: ThemeColor,
    todo_highlight: ThemeColor,
    done_highlight: ThemeColor,
    completed_sessions: ThemeColor,
}

impl ThemeConfig {
    pub fn new(
        focused_border: ThemeColor,
        unfocused_border: ThemeColor,
        todo_highlight: ThemeColor,
        done_highlight: ThemeColor,
        completed_sessions: ThemeColor,
    ) -> Self {
        Self {
            focused_border,
            unfocused_border,
            todo_highlight,
            done_highlight,
            completed_sessions,
        }
    }

    pub fn focused_border(&self) -> ThemeColor {
        self.focused_border
    }

    pub fn unfocused_border(&self) -> ThemeColor {
        self.unfocused_border
    }

    pub fn todo_highlight(&self) -> ThemeColor {
        self.todo_highlight
    }

    pub fn done_highlight(&self) -> ThemeColor {
        self.done_highlight
    }

    pub fn completed_sessions(&self) -> ThemeColor {
        self.completed_sessions
    }
}

impl Default for ThemeConfig {
    fn default() -> Self {
        Self::new(
            ThemeColor::Yellow,
            ThemeColor::DarkGray,
            ThemeColor::Yellow,
            ThemeColor::Green,
            ThemeColor::Green,
        )
    }
}

/// A terminal-independent physical key accepted by configurable commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigKey {
    Character(char),
    Space,
    Enter,
    Escape,
    Backspace,
    Up,
    Down,
    Left,
    Right,
}

impl ConfigKey {
    fn stored_name(self) -> String {
        match self {
            Self::Character(character) => character.to_string(),
            Self::Space => "space".to_string(),
            Self::Enter => "enter".to_string(),
            Self::Escape => "esc".to_string(),
            Self::Backspace => "backspace".to_string(),
            Self::Up => "up".to_string(),
            Self::Down => "down".to_string(),
            Self::Left => "left".to_string(),
            Self::Right => "right".to_string(),
        }
    }

    fn from_stored_name(value: &str) -> Result<Self, String> {
        match value {
            "space" => Ok(Self::Space),
            "enter" => Ok(Self::Enter),
            "esc" => Ok(Self::Escape),
            "backspace" => Ok(Self::Backspace),
            "up" => Ok(Self::Up),
            "down" => Ok(Self::Down),
            "left" => Ok(Self::Left),
            "right" => Ok(Self::Right),
            _ => {
                let mut characters = value.chars();
                match (characters.next(), characters.next()) {
                    (Some(character), None) if !character.is_control() && character != ' ' => {
                        Ok(Self::Character(character))
                    }
                    _ => Err(format!(
                        "key must be one printable character or one of: space, enter, esc, backspace, up, down, left, right; found {value:?}"
                    )),
                }
            }
        }
    }
}

impl Serialize for ConfigKey {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.stored_name())
    }
}

impl<'de> Deserialize<'de> for ConfigKey {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::from_stored_name(&value).map_err(de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct KeyBindings(Vec<ConfigKey>);

impl KeyBindings {
    fn one(key: ConfigKey) -> Self {
        Self(vec![key])
    }

    fn as_slice(&self) -> &[ConfigKey] {
        &self.0
    }
}

impl Serialize for KeyBindings {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        if let [key] = self.0.as_slice() {
            key.serialize(serializer)
        } else {
            self.0.serialize(serializer)
        }
    }
}

impl<'de> Deserialize<'de> for KeyBindings {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct KeyBindingsVisitor;

        impl<'de> de::Visitor<'de> for KeyBindingsVisitor {
            type Value = KeyBindings;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a key name or a list of key names")
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                ConfigKey::from_stored_name(value)
                    .map(KeyBindings::one)
                    .map_err(E::custom)
            }

            fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
            where
                A: de::SeqAccess<'de>,
            {
                let mut keys = Vec::with_capacity(sequence.size_hint().unwrap_or(0));
                while let Some(key) = sequence.next_element()? {
                    keys.push(key);
                }
                Ok(KeyBindings(keys))
            }
        }

        deserializer.deserialize_any(KeyBindingsVisitor)
    }
}

/// Durable normal-mode key bindings. Editing and confirmation keys are fixed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct KeysConfig {
    focus_left: KeyBindings,
    focus_down: KeyBindings,
    focus_up: KeyBindings,
    focus_right: KeyBindings,
    list_down: KeyBindings,
    list_up: KeyBindings,
    quit: KeyBindings,
    clock_primary: KeyBindings,
    cycle_session: KeyBindings,
    reset_session: KeyBindings,
    add_task: KeyBindings,
    edit_task: KeyBindings,
    delete_task: KeyBindings,
    task_primary: KeyBindings,
}

impl KeysConfig {
    pub fn focus_left(&self) -> &[ConfigKey] {
        self.focus_left.as_slice()
    }
    pub fn focus_down(&self) -> &[ConfigKey] {
        self.focus_down.as_slice()
    }
    pub fn focus_up(&self) -> &[ConfigKey] {
        self.focus_up.as_slice()
    }
    pub fn focus_right(&self) -> &[ConfigKey] {
        self.focus_right.as_slice()
    }
    pub fn list_down(&self) -> &[ConfigKey] {
        self.list_down.as_slice()
    }
    pub fn list_up(&self) -> &[ConfigKey] {
        self.list_up.as_slice()
    }
    pub fn quit(&self) -> &[ConfigKey] {
        self.quit.as_slice()
    }
    pub fn clock_primary(&self) -> &[ConfigKey] {
        self.clock_primary.as_slice()
    }
    pub fn cycle_session(&self) -> &[ConfigKey] {
        self.cycle_session.as_slice()
    }
    pub fn reset_session(&self) -> &[ConfigKey] {
        self.reset_session.as_slice()
    }
    pub fn add_task(&self) -> &[ConfigKey] {
        self.add_task.as_slice()
    }
    pub fn edit_task(&self) -> &[ConfigKey] {
        self.edit_task.as_slice()
    }
    pub fn delete_task(&self) -> &[ConfigKey] {
        self.delete_task.as_slice()
    }
    pub fn task_primary(&self) -> &[ConfigKey] {
        self.task_primary.as_slice()
    }

    fn validate(&self) -> Result<(), ConfigValidationError> {
        let bindings = [
            ("focus_left", self.focus_left()),
            ("focus_down", self.focus_down()),
            ("focus_up", self.focus_up()),
            ("focus_right", self.focus_right()),
            ("quit", self.quit()),
            ("clock_primary", self.clock_primary()),
            ("cycle_session", self.cycle_session()),
            ("reset_session", self.reset_session()),
            ("list_down", self.list_down()),
            ("list_up", self.list_up()),
            ("add_task", self.add_task()),
            ("edit_task", self.edit_task()),
            ("delete_task", self.delete_task()),
            ("task_primary", self.task_primary()),
        ];
        for (field, keys) in bindings {
            if keys.is_empty() {
                return Err(ConfigValidationError::EmptyKeyBindings { field });
            }
        }

        let global = binding_entries(&bindings[..5]);
        validate_unique_bindings(&global)?;

        let clock = binding_entries(&bindings[5..8]);
        validate_context_bindings(&global, &clock)?;

        let tasks = binding_entries(&bindings[8..]);
        validate_context_bindings(&global, &tasks)
    }
}

impl Default for KeysConfig {
    fn default() -> Self {
        Self {
            focus_left: KeyBindings::one(ConfigKey::Character('H')),
            focus_down: KeyBindings::one(ConfigKey::Character('J')),
            focus_up: KeyBindings::one(ConfigKey::Character('K')),
            focus_right: KeyBindings::one(ConfigKey::Character('L')),
            list_down: KeyBindings(vec![ConfigKey::Character('j'), ConfigKey::Down]),
            list_up: KeyBindings(vec![ConfigKey::Character('k'), ConfigKey::Up]),
            quit: KeyBindings::one(ConfigKey::Character('q')),
            clock_primary: KeyBindings::one(ConfigKey::Space),
            cycle_session: KeyBindings::one(ConfigKey::Character('c')),
            reset_session: KeyBindings::one(ConfigKey::Character('r')),
            add_task: KeyBindings::one(ConfigKey::Character('a')),
            edit_task: KeyBindings::one(ConfigKey::Character('e')),
            delete_task: KeyBindings::one(ConfigKey::Character('x')),
            task_primary: KeyBindings::one(ConfigKey::Space),
        }
    }
}

type BindingEntry = (&'static str, ConfigKey, &'static str);

fn binding_entries(bindings: &[(&'static str, &[ConfigKey])]) -> Vec<BindingEntry> {
    bindings
        .iter()
        .flat_map(|(field, keys)| keys.iter().map(|key| (*field, *key, *field)))
        .collect()
}

fn validate_context_bindings(
    global: &[BindingEntry],
    contextual: &[BindingEntry],
) -> Result<(), ConfigValidationError> {
    let combined = global.iter().chain(contextual).copied().collect::<Vec<_>>();
    validate_unique_bindings(&combined)
}

fn validate_unique_bindings(bindings: &[BindingEntry]) -> Result<(), ConfigValidationError> {
    for (index, (first_field, first_key, first_action)) in bindings.iter().enumerate() {
        for (second_field, second_key, second_action) in &bindings[index + 1..] {
            if first_key == second_key && first_action != second_action {
                return Err(ConfigValidationError::ConflictingKeys {
                    first: first_field,
                    second: second_field,
                });
            }
        }
    }
    Ok(())
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
        Self::with_settings(
            TimerConfig::new(
                stored.timer.focus_minutes,
                stored.timer.short_break_minutes,
                stored.timer.long_break_minutes,
                stored.timer.long_break_interval,
            )?,
            TasksConfig::with_numbering(stored.tasks.persist, stored.tasks.show_numbers),
            stored.theme,
            stored.keys,
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
        TasksConfig, ThemeColor, ThemeConfig, TimerConfig,
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
        assert_eq!(
            config.keys().list_down(),
            [ConfigKey::Character('j'), ConfigKey::Down]
        );
        assert_eq!(
            config.keys().list_up(),
            [ConfigKey::Character('k'), ConfigKey::Up]
        );
    }

    #[test]
    fn missing_file_uses_defaults() {
        let path = temp_path("missing.toml");

        assert_eq!(Config::load_from(path).unwrap(), Config::default());
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
