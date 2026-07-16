use crossterm::event::KeyCode;

use crate::{
    app::{Action, Direction, EditMode, UiFocus},
    config::{ConfigKey, KeysConfig},
};

/// Maps a physical key to a semantic action for the current application context.
pub fn map_key(
    key: KeyCode,
    edit_mode: EditMode,
    focus: UiFocus,
    confirmation_open: bool,
    keys: &KeysConfig,
) -> Option<Action> {
    if confirmation_open {
        return match key {
            KeyCode::Char('y') | KeyCode::Enter => Some(Action::ConfirmPendingAction),
            KeyCode::Char('n') | KeyCode::Esc => Some(Action::CancelPendingAction),
            _ => None,
        };
    }

    if edit_mode != EditMode::Normal {
        return match key {
            KeyCode::Enter => Some(Action::SubmitEdit),
            KeyCode::Esc => Some(Action::CancelEdit),
            KeyCode::Backspace => Some(Action::PopInput),
            KeyCode::Char(character) => Some(Action::PushInput(character)),
            _ => None,
        };
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
        UiFocus::Todo if key_matches_any(key, keys.add_task()) => Some(Action::BeginAdd),
        UiFocus::Todo | UiFocus::Done if key_matches_any(key, keys.edit_task()) => {
            Some(Action::EditSelected)
        }
        UiFocus::Todo | UiFocus::Done if key_matches_any(key, keys.delete_task()) => {
            Some(Action::DeleteSelected)
        }
        UiFocus::Todo | UiFocus::Done if key_matches_any(key, keys.task_primary()) => {
            Some(Action::PrimaryAction)
        }
        UiFocus::Todo | UiFocus::Done => row_direction(key, keys).map(Action::MoveSelection),
        _ => None,
    }
}

fn focus_direction(key: KeyCode, keys: &KeysConfig) -> Option<Direction> {
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

fn row_direction(key: KeyCode, keys: &KeysConfig) -> Option<Direction> {
    if key_matches_any(key, keys.list_down()) {
        Some(Direction::Down)
    } else if key_matches_any(key, keys.list_up()) {
        Some(Direction::Up)
    } else {
        None
    }
}

fn key_matches_any(key: KeyCode, configured: &[ConfigKey]) -> bool {
    configured
        .iter()
        .any(|configured| key_matches(key, *configured))
}

fn key_matches(key: KeyCode, configured: ConfigKey) -> bool {
    match configured {
        ConfigKey::Character(character) => key == KeyCode::Char(character),
        ConfigKey::Space => key == KeyCode::Char(' '),
        ConfigKey::Enter => key == KeyCode::Enter,
        ConfigKey::Escape => key == KeyCode::Esc,
        ConfigKey::Backspace => key == KeyCode::Backspace,
        ConfigKey::Up => key == KeyCode::Up,
        ConfigKey::Down => key == KeyCode::Down,
        ConfigKey::Left => key == KeyCode::Left,
        ConfigKey::Right => key == KeyCode::Right,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
            None
        );
        assert_eq!(
            map_default(KeyCode::Down, EditMode::Normal, UiFocus::Clock, false),
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
    fn maps_cycle_session_to_c_only_in_clock_context() {
        assert_eq!(
            map_default(KeyCode::Char('c'), EditMode::Normal, UiFocus::Clock, false),
            Some(Action::CycleSession)
        );
        assert_eq!(
            map_default(KeyCode::Char('n'), EditMode::Normal, UiFocus::Clock, false),
            None
        );
    }

    #[test]
    fn configured_keys_replace_defaults_in_their_context() {
        let keys: KeysConfig = toml::from_str(
            "focus_left = \"left\"\nclock_primary = \"enter\"\ncycle_session = \"n\"\n",
        )
        .unwrap();

        assert_eq!(
            map_key(
                KeyCode::Left,
                EditMode::Normal,
                UiFocus::Clock,
                false,
                &keys
            ),
            Some(Action::NavigateFocus(Direction::Left))
        );
        assert_eq!(
            map_key(
                KeyCode::Enter,
                EditMode::Normal,
                UiFocus::Clock,
                false,
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
                &keys
            ),
            Some(Action::MoveSelection(Direction::Down))
        );
        for key in [KeyCode::Char('j'), KeyCode::Down] {
            assert_eq!(
                map_key(key, EditMode::Normal, UiFocus::Todo, false, &keys),
                None
            );
        }
        assert_eq!(
            map_key(
                KeyCode::Char('p'),
                EditMode::Normal,
                UiFocus::Done,
                false,
                &keys
            ),
            Some(Action::MoveSelection(Direction::Up))
        );
        for key in [KeyCode::Char('k'), KeyCode::Up] {
            assert_eq!(
                map_key(key, EditMode::Normal, UiFocus::Done, false, &keys),
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
                map_key(key, EditMode::Normal, UiFocus::Clock, false, &keys),
                Some(Action::CycleSession)
            );
        }
        assert_eq!(
            map_key(
                KeyCode::Char('z'),
                EditMode::Normal,
                UiFocus::Done,
                false,
                &keys
            ),
            Some(Action::Quit)
        );
    }

    #[test]
    fn editing_and_confirmation_override_configured_normal_keys() {
        let keys: KeysConfig =
            toml::from_str("clock_primary = \"enter\"\ncycle_session = \"n\"\n").unwrap();

        assert_eq!(
            map_key(
                KeyCode::Enter,
                EditMode::Adding,
                UiFocus::Todo,
                false,
                &keys
            ),
            Some(Action::SubmitEdit)
        );
        assert_eq!(
            map_key(
                KeyCode::Char('n'),
                EditMode::Normal,
                UiFocus::Clock,
                true,
                &keys
            ),
            Some(Action::CancelPendingAction)
        );
    }
}
