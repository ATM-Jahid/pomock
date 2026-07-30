use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::{
    app::{Action, Direction, EditMode, SettingsMode, UiFocus},
    config::{ConfigKey, KeysConfig},
};

/// Maps a physical key to a semantic action for the current application context.
pub fn map_key(
    key: KeyCode,
    edit_mode: EditMode,
    focus: UiFocus,
    confirmation_open: bool,
    settings_mode: SettingsMode,
    keys: &KeysConfig,
) -> Option<Action> {
    map_physical_key(
        PhysicalKey {
            code: key,
            control: false,
            alt: false,
        },
        edit_mode,
        focus,
        confirmation_open,
        settings_mode,
        keys,
    )
}

/// Maps a complete terminal key event, including Control and Alt modifiers.
pub fn map_key_event(
    key: KeyEvent,
    edit_mode: EditMode,
    focus: UiFocus,
    confirmation_open: bool,
    settings_mode: SettingsMode,
    keys: &KeysConfig,
) -> Option<Action> {
    map_physical_key(
        PhysicalKey {
            code: key.code,
            control: key.modifiers.contains(KeyModifiers::CONTROL),
            alt: key.modifiers.contains(KeyModifiers::ALT),
        },
        edit_mode,
        focus,
        confirmation_open,
        settings_mode,
        keys,
    )
}

#[derive(Clone, Copy)]
struct PhysicalKey {
    code: KeyCode,
    control: bool,
    alt: bool,
}

impl PhysicalKey {
    const fn is_unmodified(self) -> bool {
        !self.control && !self.alt
    }
}

fn map_physical_key(
    key: PhysicalKey,
    edit_mode: EditMode,
    focus: UiFocus,
    confirmation_open: bool,
    settings_mode: SettingsMode,
    keys: &KeysConfig,
) -> Option<Action> {
    if confirmation_open {
        if !key.is_unmodified() {
            return None;
        }
        return match key.code {
            KeyCode::Char('y') | KeyCode::Enter => Some(Action::ConfirmPendingAction),
            KeyCode::Char('n') | KeyCode::Esc => Some(Action::CancelPendingAction),
            _ => None,
        };
    }

    if edit_mode != EditMode::Normal {
        if !key.is_unmodified() {
            return None;
        }
        return match key.code {
            KeyCode::Enter => Some(Action::SubmitEdit),
            KeyCode::Esc => Some(Action::CancelEdit),
            KeyCode::Backspace => Some(Action::PopInput),
            KeyCode::Char(character) => Some(Action::PushInput(character)),
            _ => None,
        };
    }

    match settings_mode {
        SettingsMode::EditingValue => {
            if !key.is_unmodified() {
                return None;
            }
            return match key.code {
                KeyCode::Enter => Some(Action::SettingsSubmitInput),
                KeyCode::Esc => Some(Action::SettingsCancel),
                KeyCode::Backspace => Some(Action::SettingsPopInput),
                KeyCode::Char(character) => Some(Action::SettingsPushInput(character)),
                _ => None,
            };
        }
        SettingsMode::CapturingKey => {
            return match key.code {
                KeyCode::Esc if key.is_unmodified() => Some(Action::SettingsCancel),
                _ => config_key(key).map(Action::SettingsCaptureKey),
            };
        }
        SettingsMode::Navigating => {
            return match key.code {
                KeyCode::Esc if key.is_unmodified() => Some(Action::SettingsClose),
                _ if key_matches_any(key, keys.settings()) => Some(Action::SettingsClose),
                KeyCode::Enter | KeyCode::Char(' ') if key.is_unmodified() => {
                    Some(Action::SettingsActivate)
                }
                KeyCode::Up | KeyCode::Char('k') if key.is_unmodified() => {
                    Some(Action::SettingsMove(false))
                }
                KeyCode::Down | KeyCode::Char('j') if key.is_unmodified() => {
                    Some(Action::SettingsMove(true))
                }
                KeyCode::Left | KeyCode::Char('h') if key.is_unmodified() => {
                    Some(Action::SettingsAdjust(false))
                }
                KeyCode::Right | KeyCode::Char('l') if key.is_unmodified() => {
                    Some(Action::SettingsAdjust(true))
                }
                _ => None,
            };
        }
        SettingsMode::Closed => {}
    }

    if key.code == KeyCode::Esc && key.is_unmodified() {
        return Some(Action::CancelPendingAction);
    }

    if key_matches_any(key, keys.settings()) {
        return Some(Action::OpenSettings);
    }

    if let Some(direction) = focus_direction(key, keys) {
        return Some(Action::NavigateFocus(direction));
    }

    if key_matches_any(key, keys.quit()) {
        return Some(Action::Quit);
    }

    match focus {
        UiFocus::Clock if key_matches_any(key, keys.clock_primary()) => Some(Action::PrimaryAction),
        UiFocus::Clock if key_matches_any(key, keys.cycle_session()) => Some(Action::CycleSession),
        UiFocus::Clock if key_matches_any(key, keys.reset_session()) => Some(Action::ResetSession),
        UiFocus::Todo | UiFocus::Done if key_matches_any(key, keys.add_task()) => {
            Some(Action::BeginAdd)
        }
        UiFocus::Todo | UiFocus::Done if key_matches_any(key, keys.edit_task()) => {
            Some(Action::EditSelected)
        }
        UiFocus::Todo | UiFocus::Done if key_matches_any(key, keys.delete_task()) => {
            Some(Action::DeleteSelected)
        }
        UiFocus::Todo | UiFocus::Done if key_matches_any(key, keys.task_primary()) => {
            Some(Action::PrimaryAction)
        }
        UiFocus::Todo | UiFocus::Done if key_matches_any(key, keys.move_task_up()) => {
            Some(Action::MoveSelectedTask(Direction::Up))
        }
        UiFocus::Todo | UiFocus::Done if key_matches_any(key, keys.move_task_down()) => {
            Some(Action::MoveSelectedTask(Direction::Down))
        }
        UiFocus::Todo | UiFocus::Done => row_direction(key, keys).map(Action::MoveSelection),
        _ => None,
    }
}

fn config_key(key: PhysicalKey) -> Option<ConfigKey> {
    let key_code = match key.code {
        KeyCode::Char(' ') => Some(ConfigKey::Space),
        KeyCode::Char(character) => Some(ConfigKey::Character(character)),
        KeyCode::Enter => Some(ConfigKey::Enter),
        KeyCode::Esc => Some(ConfigKey::Escape),
        KeyCode::Backspace => Some(ConfigKey::Backspace),
        KeyCode::Up => Some(ConfigKey::Up),
        KeyCode::Down => Some(ConfigKey::Down),
        KeyCode::Left => Some(ConfigKey::Left),
        KeyCode::Right => Some(ConfigKey::Right),
        _ => None,
    }?;
    Some(key_code.with_modifiers(key.control, key.alt))
}

fn focus_direction(key: PhysicalKey, keys: &KeysConfig) -> Option<Direction> {
    for (binding, direction) in [
        (keys.focus_left(), Direction::Left),
        (keys.focus_down(), Direction::Down),
        (keys.focus_up(), Direction::Up),
        (keys.focus_right(), Direction::Right),
    ] {
        if key_matches_any(key, binding) {
            return Some(direction);
        }
    }
    None
}

fn row_direction(key: PhysicalKey, keys: &KeysConfig) -> Option<Direction> {
    if key_matches_any(key, keys.list_down()) {
        Some(Direction::Down)
    } else if key_matches_any(key, keys.list_up()) {
        Some(Direction::Up)
    } else {
        None
    }
}

fn key_matches_any(key: PhysicalKey, configured: &[ConfigKey]) -> bool {
    configured
        .iter()
        .any(|configured| key_matches(key, *configured))
}

fn key_matches(key: PhysicalKey, configured: ConfigKey) -> bool {
    config_key(key) == Some(configured)
}

#[cfg(test)]
#[path = "input/tests.rs"]
mod tests;
