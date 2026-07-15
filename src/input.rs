use crossterm::event::KeyCode;

use crate::app::{Action, Direction, EditMode, UiFocus};

/// Maps a physical key to a semantic action for the current application context.
pub fn map_key(
    key: KeyCode,
    edit_mode: EditMode,
    focus: UiFocus,
    confirmation_open: bool,
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

    if let Some(direction) = focus_direction(key) {
        return Some(Action::NavigateFocus(direction));
    }

    match (focus, key) {
        (_, KeyCode::Char('q')) => Some(Action::Quit),
        (UiFocus::Clock, KeyCode::Char(' ')) => Some(Action::PrimaryAction),
        (UiFocus::Clock, KeyCode::Char('c')) => Some(Action::CycleSession),
        (UiFocus::Clock, KeyCode::Char('r')) => Some(Action::ResetSession),
        (UiFocus::Todo, KeyCode::Char('a')) => Some(Action::BeginAdd),
        (UiFocus::Todo | UiFocus::Done, KeyCode::Char('e')) => Some(Action::EditSelected),
        (UiFocus::Todo | UiFocus::Done, KeyCode::Char('x')) => Some(Action::DeleteSelected),
        (UiFocus::Todo | UiFocus::Done, KeyCode::Char(' ')) => Some(Action::PrimaryAction),
        (UiFocus::Todo | UiFocus::Done, key) => row_direction(key).map(Action::MoveSelection),
        _ => None,
    }
}

fn focus_direction(key: KeyCode) -> Option<Direction> {
    match key {
        KeyCode::Char('H') => Some(Direction::Left),
        KeyCode::Char('J') => Some(Direction::Down),
        KeyCode::Char('K') => Some(Direction::Up),
        KeyCode::Char('L') => Some(Direction::Right),
        _ => None,
    }
}

fn row_direction(key: KeyCode) -> Option<Direction> {
    match key {
        KeyCode::Char('j') | KeyCode::Down => Some(Direction::Down),
        KeyCode::Char('k') | KeyCode::Up => Some(Direction::Up),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_global_normal_mode_actions() {
        assert_eq!(
            map_key(KeyCode::Char('J'), EditMode::Normal, UiFocus::Clock, false),
            Some(Action::NavigateFocus(Direction::Down))
        );
        assert_eq!(
            map_key(KeyCode::Char('q'), EditMode::Normal, UiFocus::Done, false),
            Some(Action::Quit)
        );
    }

    #[test]
    fn maps_keys_by_focused_area() {
        assert_eq!(
            map_key(KeyCode::Char(' '), EditMode::Normal, UiFocus::Clock, false),
            Some(Action::PrimaryAction)
        );
        assert_eq!(
            map_key(KeyCode::Char('a'), EditMode::Normal, UiFocus::Todo, false),
            Some(Action::BeginAdd)
        );
        assert_eq!(
            map_key(KeyCode::Down, EditMode::Normal, UiFocus::Done, false),
            Some(Action::MoveSelection(Direction::Down))
        );
        assert_eq!(
            map_key(KeyCode::Char('a'), EditMode::Normal, UiFocus::Done, false),
            None
        );
        assert_eq!(
            map_key(KeyCode::Down, EditMode::Normal, UiFocus::Clock, false),
            None
        );
    }

    #[test]
    fn edit_mode_takes_precedence_over_normal_commands() {
        assert_eq!(
            map_key(KeyCode::Char('q'), EditMode::Adding, UiFocus::Todo, false),
            Some(Action::PushInput('q'))
        );
        assert_eq!(
            map_key(
                KeyCode::Char('J'),
                EditMode::Editing { task_index: 0 },
                UiFocus::Todo,
                false,
            ),
            Some(Action::PushInput('J'))
        );
        assert_eq!(
            map_key(KeyCode::Enter, EditMode::Adding, UiFocus::Todo, false),
            Some(Action::SubmitEdit)
        );
        assert_eq!(
            map_key(KeyCode::Left, EditMode::Adding, UiFocus::Todo, false),
            None
        );
    }

    #[test]
    fn normal_mode_ignores_unmapped_keys() {
        assert_eq!(
            map_key(KeyCode::Enter, EditMode::Normal, UiFocus::Todo, false),
            None
        );
        assert_eq!(
            map_key(KeyCode::Char('h'), EditMode::Normal, UiFocus::Todo, false),
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
                map_key(key, EditMode::Adding, UiFocus::Todo, true),
                expected
            );
        }
    }

    #[test]
    fn maps_cycle_session_to_c_only_in_clock_context() {
        assert_eq!(
            map_key(KeyCode::Char('c'), EditMode::Normal, UiFocus::Clock, false),
            Some(Action::CycleSession)
        );
        assert_eq!(
            map_key(KeyCode::Char('n'), EditMode::Normal, UiFocus::Clock, false),
            None
        );
    }
}
