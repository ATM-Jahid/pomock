use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer, de};

use super::ConfigValidationError;

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
pub(super) struct KeyBindings(Vec<ConfigKey>);

impl KeyBindings {
    pub(super) fn one(key: ConfigKey) -> Self {
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
    pub(super) focus_left: KeyBindings,
    pub(super) focus_down: KeyBindings,
    pub(super) focus_up: KeyBindings,
    pub(super) focus_right: KeyBindings,
    pub(super) list_down: KeyBindings,
    pub(super) list_up: KeyBindings,
    pub(super) quit: KeyBindings,
    pub(super) clock_primary: KeyBindings,
    pub(super) cycle_session: KeyBindings,
    pub(super) reset_session: KeyBindings,
    pub(super) add_task: KeyBindings,
    pub(super) edit_task: KeyBindings,
    pub(super) delete_task: KeyBindings,
    pub(super) task_primary: KeyBindings,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyAction {
    FocusLeft,
    FocusDown,
    FocusUp,
    FocusRight,
    ListDown,
    ListUp,
    Quit,
    ClockPrimary,
    CycleSession,
    ResetSession,
    AddTask,
    EditTask,
    DeleteTask,
    TaskPrimary,
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

    pub fn binding(&self, action: KeyAction) -> &[ConfigKey] {
        match action {
            KeyAction::FocusLeft => self.focus_left(),
            KeyAction::FocusDown => self.focus_down(),
            KeyAction::FocusUp => self.focus_up(),
            KeyAction::FocusRight => self.focus_right(),
            KeyAction::ListDown => self.list_down(),
            KeyAction::ListUp => self.list_up(),
            KeyAction::Quit => self.quit(),
            KeyAction::ClockPrimary => self.clock_primary(),
            KeyAction::CycleSession => self.cycle_session(),
            KeyAction::ResetSession => self.reset_session(),
            KeyAction::AddTask => self.add_task(),
            KeyAction::EditTask => self.edit_task(),
            KeyAction::DeleteTask => self.delete_task(),
            KeyAction::TaskPrimary => self.task_primary(),
        }
    }

    pub fn with_binding(mut self, action: KeyAction, key: ConfigKey) -> Self {
        let binding = KeyBindings::one(key);
        match action {
            KeyAction::FocusLeft => self.focus_left = binding,
            KeyAction::FocusDown => self.focus_down = binding,
            KeyAction::FocusUp => self.focus_up = binding,
            KeyAction::FocusRight => self.focus_right = binding,
            KeyAction::ListDown => self.list_down = binding,
            KeyAction::ListUp => self.list_up = binding,
            KeyAction::Quit => self.quit = binding,
            KeyAction::ClockPrimary => self.clock_primary = binding,
            KeyAction::CycleSession => self.cycle_session = binding,
            KeyAction::ResetSession => self.reset_session = binding,
            KeyAction::AddTask => self.add_task = binding,
            KeyAction::EditTask => self.edit_task = binding,
            KeyAction::DeleteTask => self.delete_task = binding,
            KeyAction::TaskPrimary => self.task_primary = binding,
        }
        self
    }

    pub(super) fn validate(&self) -> Result<(), ConfigValidationError> {
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
