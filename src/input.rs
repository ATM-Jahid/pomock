use crossterm::event::KeyCode;

use crate::app::{Action, Direction, EditMode, UiFocus};

/// Maps a physical key to a semantic action for the current application context.
pub fn map_key(key: KeyCode, edit_mode: EditMode, focus: UiFocus) -> Option<Action> {
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
        (UiFocus::Clock, KeyCode::Char('f')) => Some(Action::SelectNextSession),
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
            map_key(KeyCode::Char('J'), EditMode::Normal, UiFocus::Clock),
            Some(Action::NavigateFocus(Direction::Down))
        );
        assert_eq!(
            map_key(KeyCode::Char('q'), EditMode::Normal, UiFocus::Done),
            Some(Action::Quit)
        );
    }

    #[test]
    fn maps_keys_by_focused_area() {
        assert_eq!(
            map_key(KeyCode::Char(' '), EditMode::Normal, UiFocus::Clock),
            Some(Action::PrimaryAction)
        );
        assert_eq!(
            map_key(KeyCode::Char('a'), EditMode::Normal, UiFocus::Todo),
            Some(Action::BeginAdd)
        );
        assert_eq!(
            map_key(KeyCode::Down, EditMode::Normal, UiFocus::Done),
            Some(Action::MoveSelection(Direction::Down))
        );
        assert_eq!(
            map_key(KeyCode::Char('a'), EditMode::Normal, UiFocus::Done),
            None
        );
        assert_eq!(
            map_key(KeyCode::Down, EditMode::Normal, UiFocus::Clock),
            None
        );
    }

    #[test]
    fn edit_mode_takes_precedence_over_normal_commands() {
        assert_eq!(
            map_key(KeyCode::Char('q'), EditMode::Adding, UiFocus::Todo),
            Some(Action::PushInput('q'))
        );
        assert_eq!(
            map_key(
                KeyCode::Char('J'),
                EditMode::Editing { task_index: 0 },
                UiFocus::Todo
            ),
            Some(Action::PushInput('J'))
        );
        assert_eq!(
            map_key(KeyCode::Enter, EditMode::Adding, UiFocus::Todo),
            Some(Action::SubmitEdit)
        );
        assert_eq!(
            map_key(KeyCode::Left, EditMode::Adding, UiFocus::Todo),
            None
        );
    }

    #[test]
    fn normal_mode_ignores_unmapped_keys() {
        assert_eq!(
            map_key(KeyCode::Enter, EditMode::Normal, UiFocus::Todo),
            None
        );
        assert_eq!(
            map_key(KeyCode::Char('h'), EditMode::Normal, UiFocus::Todo),
            None
        );
    }
}
