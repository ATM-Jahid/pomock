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
mod tests {
    use super::*;

    fn modified_event(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, modifiers)
    }

    fn map_default(
        key: KeyCode,
        edit_mode: EditMode,
        focus: UiFocus,
        confirmation_open: bool,
    ) -> Option<Action> {
        map_key(
            key,
            edit_mode,
            focus,
            confirmation_open,
            SettingsMode::Closed,
            &KeysConfig::default(),
        )
    }

    #[test]
    fn maps_global_normal_mode_actions() {
        assert_eq!(
            map_default(KeyCode::Char('J'), EditMode::Normal, UiFocus::Clock, false),
            Some(Action::NavigateFocus(Direction::Down))
        );
        assert_eq!(
            map_default(KeyCode::Char('q'), EditMode::Normal, UiFocus::Done, false),
            Some(Action::Quit)
        );
    }

    #[test]
    fn maps_control_and_alt_bindings_without_matching_plain_keys() {
        let keys: KeysConfig =
            toml::from_str("cycle_session = \"ctrl+c\"\nlist_down = \"alt+down\"\n").unwrap();

        assert_eq!(
            map_key_event(
                modified_event(KeyCode::Char('c'), KeyModifiers::CONTROL),
                EditMode::Normal,
                UiFocus::Clock,
                false,
                SettingsMode::Closed,
                &keys,
            ),
            Some(Action::CycleSession)
        );
        assert_eq!(
            map_key(
                KeyCode::Char('c'),
                EditMode::Normal,
                UiFocus::Clock,
                false,
                SettingsMode::Closed,
                &keys,
            ),
            None
        );
        assert_eq!(
            map_key_event(
                modified_event(KeyCode::Down, KeyModifiers::ALT),
                EditMode::Normal,
                UiFocus::Todo,
                false,
                SettingsMode::Closed,
                &keys,
            ),
            Some(Action::MoveSelection(Direction::Down))
        );
    }

    #[test]
    fn key_capture_preserves_control_and_alt_modifiers() {
        let keys = KeysConfig::default();
        let event = modified_event(
            KeyCode::Char('q'),
            KeyModifiers::CONTROL | KeyModifiers::ALT,
        );

        assert_eq!(
            map_key_event(
                event,
                EditMode::Normal,
                UiFocus::Clock,
                false,
                SettingsMode::CapturingKey,
                &keys,
            ),
            Some(Action::SettingsCaptureKey(
                ConfigKey::Character('q').with_modifiers(true, true)
            ))
        );
    }

    #[test]
    fn modified_keys_do_not_trigger_fixed_modal_controls() {
        let keys = KeysConfig::default();
        assert_eq!(
            map_key_event(
                modified_event(KeyCode::Char('y'), KeyModifiers::CONTROL),
                EditMode::Normal,
                UiFocus::Clock,
                true,
                SettingsMode::Closed,
                &keys,
            ),
            None
        );
        assert_eq!(
            map_key_event(
                modified_event(KeyCode::Char('q'), KeyModifiers::ALT),
                EditMode::Adding,
                UiFocus::Todo,
                false,
                SettingsMode::Closed,
                &keys,
            ),
            None
        );
    }

    #[test]
    fn maps_keys_by_focused_area() {
        assert_eq!(
            map_default(KeyCode::Char(' '), EditMode::Normal, UiFocus::Clock, false),
            Some(Action::PrimaryAction)
        );
        assert_eq!(
            map_default(KeyCode::Char('a'), EditMode::Normal, UiFocus::Todo, false),
            Some(Action::BeginAdd)
        );
        assert_eq!(
            map_default(KeyCode::Down, EditMode::Normal, UiFocus::Done, false),
            Some(Action::MoveSelection(Direction::Down))
        );
        assert_eq!(
            map_default(KeyCode::Char('a'), EditMode::Normal, UiFocus::Done, false),
            Some(Action::BeginAdd)
        );
        assert_eq!(
            map_default(KeyCode::Down, EditMode::Normal, UiFocus::Clock, false),
            None
        );
        assert_eq!(
            map_default(KeyCode::Char('u'), EditMode::Normal, UiFocus::Todo, false),
            Some(Action::MoveSelectedTask(Direction::Up))
        );
        assert_eq!(
            map_default(KeyCode::Char('d'), EditMode::Normal, UiFocus::Done, false),
            Some(Action::MoveSelectedTask(Direction::Down))
        );
        assert_eq!(
            map_default(KeyCode::Char('c'), EditMode::Normal, UiFocus::Clock, false),
            Some(Action::CycleSession)
        );
        assert_eq!(
            map_default(KeyCode::Char('c'), EditMode::Normal, UiFocus::Todo, false),
            None
        );
    }

    #[test]
    fn edit_mode_takes_precedence_over_normal_commands() {
        assert_eq!(
            map_default(KeyCode::Char('q'), EditMode::Adding, UiFocus::Todo, false),
            Some(Action::PushInput('q'))
        );
        assert_eq!(
            map_default(
                KeyCode::Char('J'),
                EditMode::Editing { task_index: 0 },
                UiFocus::Todo,
                false,
            ),
            Some(Action::PushInput('J'))
        );
        assert_eq!(
            map_default(KeyCode::Enter, EditMode::Adding, UiFocus::Todo, false),
            Some(Action::SubmitEdit)
        );
        assert_eq!(
            map_default(KeyCode::Left, EditMode::Adding, UiFocus::Todo, false),
            None
        );
    }

    #[test]
    fn normal_mode_ignores_unmapped_keys() {
        assert_eq!(
            map_default(KeyCode::Enter, EditMode::Normal, UiFocus::Todo, false),
            None
        );
        assert_eq!(
            map_default(KeyCode::Char('h'), EditMode::Normal, UiFocus::Todo, false),
            None
        );
    }

    #[test]
    fn confirmation_keys_take_precedence_over_every_other_context() {
        for (key, expected) in [
            (KeyCode::Char('y'), Some(Action::ConfirmPendingAction)),
            (KeyCode::Enter, Some(Action::ConfirmPendingAction)),
            (KeyCode::Char('n'), Some(Action::CancelPendingAction)),
            (KeyCode::Esc, Some(Action::CancelPendingAction)),
            (KeyCode::Char('q'), None),
            (KeyCode::Char('H'), None),
        ] {
            assert_eq!(
                map_default(key, EditMode::Adding, UiFocus::Todo, true),
                expected
            );
        }
    }

    #[test]
    fn configured_keys_replace_defaults_in_their_context() {
        let keys: KeysConfig = toml::from_str(
            "focus_left = \"left\"\nclock_primary = \"backspace\"\ncycle_session = \"n\"\n",
        )
        .unwrap();

        assert_eq!(
            map_key(
                KeyCode::Left,
                EditMode::Normal,
                UiFocus::Clock,
                false,
                SettingsMode::Closed,
                &keys
            ),
            Some(Action::NavigateFocus(Direction::Left))
        );
        assert_eq!(
            map_key(
                KeyCode::Backspace,
                EditMode::Normal,
                UiFocus::Clock,
                false,
                SettingsMode::Closed,
                &keys
            ),
            Some(Action::PrimaryAction)
        );
        assert_eq!(
            map_key(
                KeyCode::Char('n'),
                EditMode::Normal,
                UiFocus::Clock,
                false,
                SettingsMode::Closed,
                &keys
            ),
            Some(Action::CycleSession)
        );
        assert_eq!(
            map_key(
                KeyCode::Char('c'),
                EditMode::Normal,
                UiFocus::Clock,
                false,
                SettingsMode::Closed,
                &keys
            ),
            None
        );
    }

    #[test]
    fn configured_list_keys_do_not_keep_default_aliases() {
        let keys: KeysConfig = toml::from_str("list_down = \"n\"\nlist_up = \"p\"\n").unwrap();

        assert_eq!(
            map_key(
                KeyCode::Char('n'),
                EditMode::Normal,
                UiFocus::Todo,
                false,
                SettingsMode::Closed,
                &keys
            ),
            Some(Action::MoveSelection(Direction::Down))
        );
        for key in [KeyCode::Char('j'), KeyCode::Down] {
            assert_eq!(
                map_key(
                    key,
                    EditMode::Normal,
                    UiFocus::Todo,
                    false,
                    SettingsMode::Closed,
                    &keys
                ),
                None
            );
        }
        assert_eq!(
            map_key(
                KeyCode::Char('p'),
                EditMode::Normal,
                UiFocus::Done,
                false,
                SettingsMode::Closed,
                &keys
            ),
            Some(Action::MoveSelection(Direction::Up))
        );
        for key in [KeyCode::Char('k'), KeyCode::Up] {
            assert_eq!(
                map_key(
                    key,
                    EditMode::Normal,
                    UiFocus::Done,
                    false,
                    SettingsMode::Closed,
                    &keys
                ),
                None
            );
        }
    }

    #[test]
    fn configured_task_movement_keys_replace_the_defaults() {
        let keys: KeysConfig =
            toml::from_str("move_task_up = \"w\"\nmove_task_down = \"z\"\n").unwrap();

        assert_eq!(
            map_key(
                KeyCode::Char('w'),
                EditMode::Normal,
                UiFocus::Todo,
                false,
                SettingsMode::Closed,
                &keys
            ),
            Some(Action::MoveSelectedTask(Direction::Up))
        );
        assert_eq!(
            map_key(
                KeyCode::Char('z'),
                EditMode::Normal,
                UiFocus::Done,
                false,
                SettingsMode::Closed,
                &keys
            ),
            Some(Action::MoveSelectedTask(Direction::Down))
        );
        for key in [KeyCode::Char('u'), KeyCode::Char('d')] {
            assert_eq!(
                map_key(
                    key,
                    EditMode::Normal,
                    UiFocus::Todo,
                    false,
                    SettingsMode::Closed,
                    &keys
                ),
                None
            );
        }
    }

    #[test]
    fn every_configured_key_for_an_action_is_mapped() {
        let keys: KeysConfig =
            toml::from_str("cycle_session = [\"c\", \"n\"]\nquit = [\"q\", \"z\"]\n").unwrap();

        for key in [KeyCode::Char('c'), KeyCode::Char('n')] {
            assert_eq!(
                map_key(
                    key,
                    EditMode::Normal,
                    UiFocus::Clock,
                    false,
                    SettingsMode::Closed,
                    &keys
                ),
                Some(Action::CycleSession)
            );
        }
        assert_eq!(
            map_key(
                KeyCode::Char('z'),
                EditMode::Normal,
                UiFocus::Done,
                false,
                SettingsMode::Closed,
                &keys
            ),
            Some(Action::Quit)
        );
    }

    #[test]
    fn editing_and_confirmation_override_configured_normal_keys() {
        let keys: KeysConfig =
            toml::from_str("clock_primary = \"backspace\"\ncycle_session = \"n\"\n").unwrap();

        assert_eq!(
            map_key(
                KeyCode::Backspace,
                EditMode::Adding,
                UiFocus::Todo,
                false,
                SettingsMode::Closed,
                &keys
            ),
            Some(Action::PopInput)
        );
        assert_eq!(
            map_key(
                KeyCode::Char('n'),
                EditMode::Normal,
                UiFocus::Clock,
                true,
                SettingsMode::Closed,
                &keys
            ),
            Some(Action::CancelPendingAction)
        );
    }

    #[test]
    fn settings_context_has_fixed_navigation_and_nested_editing_precedence() {
        let keys = KeysConfig::default();
        assert_eq!(
            map_key(
                KeyCode::Char('s'),
                EditMode::Normal,
                UiFocus::Clock,
                false,
                SettingsMode::Closed,
                &keys
            ),
            Some(Action::OpenSettings)
        );
        assert_eq!(
            map_key(
                KeyCode::Esc,
                EditMode::Normal,
                UiFocus::Clock,
                false,
                SettingsMode::Closed,
                &keys
            ),
            Some(Action::CancelPendingAction)
        );
        assert_eq!(
            map_key(
                KeyCode::Char('s'),
                EditMode::Normal,
                UiFocus::Clock,
                false,
                SettingsMode::Navigating,
                &keys
            ),
            Some(Action::SettingsClose)
        );
        assert_eq!(
            map_key(
                KeyCode::Esc,
                EditMode::Normal,
                UiFocus::Clock,
                false,
                SettingsMode::Navigating,
                &keys
            ),
            Some(Action::SettingsClose)
        );
        assert_eq!(
            map_key(
                KeyCode::Char('l'),
                EditMode::Normal,
                UiFocus::Clock,
                false,
                SettingsMode::Navigating,
                &keys
            ),
            Some(Action::SettingsAdjust(true))
        );
        assert_eq!(
            map_key(
                KeyCode::Char('7'),
                EditMode::Normal,
                UiFocus::Clock,
                false,
                SettingsMode::EditingValue,
                &keys
            ),
            Some(Action::SettingsPushInput('7'))
        );
        assert_eq!(
            map_key(
                KeyCode::Char('q'),
                EditMode::Normal,
                UiFocus::Clock,
                false,
                SettingsMode::CapturingKey,
                &keys
            ),
            Some(Action::SettingsCaptureKey(ConfigKey::Character('q')))
        );
        assert_eq!(
            map_key(
                KeyCode::Esc,
                EditMode::Normal,
                UiFocus::Clock,
                false,
                SettingsMode::CapturingKey,
                &keys
            ),
            Some(Action::SettingsCancel)
        );

        let keys: KeysConfig = toml::from_str("settings = \"t\"\ncycle_session = \"s\"\n").unwrap();
        assert_eq!(
            map_key(
                KeyCode::Char('t'),
                EditMode::Normal,
                UiFocus::Clock,
                false,
                SettingsMode::Closed,
                &keys
            ),
            Some(Action::OpenSettings)
        );
        assert_eq!(
            map_key(
                KeyCode::Char('t'),
                EditMode::Normal,
                UiFocus::Clock,
                false,
                SettingsMode::Navigating,
                &keys
            ),
            Some(Action::SettingsClose)
        );
        assert_eq!(
            map_key(
                KeyCode::Char('t'),
                EditMode::Adding,
                UiFocus::Todo,
                false,
                SettingsMode::Closed,
                &keys
            ),
            Some(Action::PushInput('t'))
        );
        assert_eq!(
            map_key(
                KeyCode::Char('t'),
                EditMode::Normal,
                UiFocus::Clock,
                false,
                SettingsMode::EditingValue,
                &keys
            ),
            Some(Action::SettingsPushInput('t'))
        );
    }
}
