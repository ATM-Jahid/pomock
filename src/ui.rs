use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction as LayoutDirection, Layout, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
};

use crate::{
    app::{App, ClickTarget, EditMode, UiFocus},
    display::{format_big_duration, format_state},
};

#[derive(Debug, Clone, Copy)]
struct UiLayout {
    clock: Rect,
    todo: Rect,
    done: Rect,
    controls: Rect,
}

#[derive(Debug, Clone, Copy)]
struct Theme {
    focused_border: Color,
    unfocused_border: Color,
    todo_highlight: Color,
    done_highlight: Color,
    completed_sessions: Color,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            focused_border: Color::Yellow,
            unfocused_border: Color::DarkGray,
            todo_highlight: Color::Yellow,
            done_highlight: Color::Green,
            completed_sessions: Color::Green,
        }
    }
}

/// Renders the complete application UI and synchronizes list scroll offsets.
pub fn draw(frame: &mut Frame, app: &mut App) {
    let theme = Theme::default();
    let area = frame.area();
    let layout = ui_layout(area);

    let outer_block = Block::default().title("pomock").borders(Borders::ALL);
    frame.render_widget(outer_block, area);

    let clock_block = focused_block("Clock", app.ui_focus() == UiFocus::Clock, theme);
    let clock_area = clock_block.inner(layout.clock);
    frame.render_widget(clock_block, layout.clock);

    let clock_chunks = Layout::default()
        .direction(LayoutDirection::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .split(clock_area);

    let state = Paragraph::new(format_state(app.timer().state())).alignment(Alignment::Center);
    let remaining = Paragraph::new(format_big_duration(app.timer().remaining()))
        .alignment(Alignment::Center)
        .style(Style::default().add_modifier(Modifier::BOLD));
    let completed_sessions = Paragraph::new(format!(
        "Focus sessions completed: {}",
        app.timer().completed_focus_sessions()
    ))
    .alignment(Alignment::Center)
    .style(Style::default().fg(theme.completed_sessions));

    let controls = Paragraph::new(controls_text(app)).alignment(Alignment::Center);

    let todo_items: Vec<ListItem> = app
        .tasks()
        .pending()
        .map(|task| ListItem::new(format!("  {}", task.description())))
        .collect();
    let todo_is_empty = todo_items.is_empty();
    let todo = if todo_is_empty {
        List::new(vec![ListItem::new("No tasks yet")])
    } else {
        List::new(todo_items)
    }
    .block(focused_block(
        "To-do",
        app.ui_focus() == UiFocus::Todo,
        theme,
    ))
    .highlight_style(
        Style::default()
            .fg(theme.todo_highlight)
            .add_modifier(Modifier::BOLD),
    )
    .highlight_symbol("> ");

    let mut todo_state = ListState::default();
    if !todo_is_empty {
        todo_state.select(Some(app.todo_selection()));
        *todo_state.offset_mut() = app.todo_offset();
    }

    let done_items: Vec<ListItem> = app
        .tasks()
        .completed()
        .map(|task| ListItem::new(format!("  {}", task.description())))
        .collect();
    let done_is_empty = done_items.is_empty();
    let done = if done_is_empty {
        List::new(vec![ListItem::new("No completed tasks")])
    } else {
        List::new(done_items)
    }
    .block(focused_block(
        "Done",
        app.ui_focus() == UiFocus::Done,
        theme,
    ))
    .highlight_style(
        Style::default()
            .fg(theme.done_highlight)
            .add_modifier(Modifier::BOLD),
    )
    .highlight_symbol("> ");

    let mut done_state = ListState::default();
    if !done_is_empty {
        done_state.select(Some(app.done_selection()));
        *done_state.offset_mut() = app.done_offset();
    }

    frame.render_widget(state, clock_chunks[0]);
    frame.render_widget(remaining, clock_chunks[1]);
    frame.render_widget(completed_sessions, clock_chunks[2]);
    frame.render_stateful_widget(todo, layout.todo, &mut todo_state);
    frame.render_stateful_widget(done, layout.done, &mut done_state);
    frame.render_widget(controls, layout.controls);
    app.set_offsets(todo_state.offset(), done_state.offset());
}

/// Translates terminal coordinates into a semantic application click target.
pub fn click_target(area: Rect, position: (u16, u16), app: &App) -> ClickTarget {
    let layout = ui_layout(area);
    let point = position.into();

    if layout.clock.contains(point) {
        ClickTarget::Clock
    } else if layout.todo.contains(point) {
        task_row_at(
            position,
            layout.todo,
            app.todo_offset(),
            app.tasks().pending().count(),
        )
        .map_or(ClickTarget::Todo, ClickTarget::TodoTask)
    } else if layout.done.contains(point) {
        task_row_at(
            position,
            layout.done,
            app.done_offset(),
            app.tasks().completed().count(),
        )
        .map_or(ClickTarget::Done, ClickTarget::DoneTask)
    } else {
        ClickTarget::Outside
    }
}

fn ui_layout(area: Rect) -> UiLayout {
    let inner_area = Block::default().borders(Borders::ALL).inner(area);
    let chunks = Layout::default()
        .direction(LayoutDirection::Vertical)
        .constraints([
            Constraint::Percentage(55),
            Constraint::Percentage(45),
            Constraint::Length(1),
        ])
        .split(inner_area);
    let task_chunks = Layout::default()
        .direction(LayoutDirection::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(chunks[1]);

    UiLayout {
        clock: chunks[0],
        todo: task_chunks[0],
        done: task_chunks[1],
        controls: chunks[2],
    }
}

fn task_row_at(position: (u16, u16), area: Rect, offset: usize, len: usize) -> Option<usize> {
    let inner = Block::default().borders(Borders::ALL).inner(area);
    let point = position.into();
    if !inner.contains(point) {
        return None;
    }

    let index = offset + usize::from(position.1 - inner.y);
    (index < len).then_some(index)
}

fn controls_text(app: &App) -> String {
    match app.edit_mode() {
        EditMode::Adding => format!("Add task: {}_", app.input()),
        EditMode::Editing { .. } => format!("Edit task: {}_", app.input()),
        EditMode::Normal => match app.ui_focus() {
            UiFocus::Clock => {
                "[HJKL] box nav [space] start/pause [f] next [r] reset [q] quit"
            }
            UiFocus::Todo => {
                "[HJKL] box nav [jk/↓↑] list nav [a] add [e] edit [x] delete [space] complete [q] quit"
            }
            UiFocus::Done => {
                "[HJKL] box nav [jk/↓↑] list nav [e] edit [x] delete [space] return [q] quit"
            }
        }
        .to_string(),
    }
}

fn focused_block(title: &str, focused: bool, theme: Theme) -> Block<'_> {
    let border_color = if focused {
        theme.focused_border
    } else {
        theme.unfocused_border
    };

    Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
}

#[cfg(test)]
mod tests {
    use crate::app::{Action, Direction};

    use super::*;

    fn add_task(app: &mut App, description: &str) {
        let _ = app.dispatch(Action::NavigateFocus(Direction::Down));
        let _ = app.dispatch(Action::BeginAdd);
        for character in description.chars() {
            let _ = app.dispatch(Action::PushInput(character));
        }
        let _ = app.dispatch(Action::SubmitEdit);
        let _ = app.dispatch(Action::NavigateFocus(Direction::Up));
    }

    #[test]
    fn task_hit_testing_ignores_borders_empty_space_and_empty_lists() {
        let area = Rect::new(10, 5, 12, 5);

        assert_eq!(task_row_at((10, 6), area, 0, 3), None);
        assert_eq!(task_row_at((11, 5), area, 0, 3), None);
        assert_eq!(task_row_at((11, 6), area, 0, 0), None);
        assert_eq!(task_row_at((11, 9), area, 0, 3), None);
    }

    #[test]
    fn task_hit_testing_maps_visible_rows_through_the_scroll_offset() {
        let area = Rect::new(10, 5, 12, 5);

        assert_eq!(task_row_at((11, 6), area, 4, 8), Some(4));
        assert_eq!(task_row_at((20, 8), area, 4, 8), Some(6));
    }

    #[test]
    fn click_translation_distinguishes_boxes_rows_and_outside() {
        let mut app = App::new();
        add_task(&mut app, "First");
        add_task(&mut app, "Second");
        let area = Rect::new(0, 0, 80, 24);
        let layout = ui_layout(area);

        assert_eq!(
            click_target(area, (layout.clock.x, layout.clock.y), &app),
            ClickTarget::Clock
        );
        assert_eq!(
            click_target(area, (layout.todo.x + 1, layout.todo.y + 2), &app),
            ClickTarget::TodoTask(1)
        );
        assert_eq!(
            click_target(area, (layout.todo.x, layout.todo.y), &app),
            ClickTarget::Todo
        );
        let _ = app.dispatch(Action::NavigateFocus(Direction::Down));
        let _ = app.dispatch(Action::PrimaryAction);
        assert_eq!(
            click_target(area, (layout.done.x + 1, layout.done.y + 1), &app),
            ClickTarget::DoneTask(0)
        );
        assert_eq!(click_target(area, (0, 0), &app), ClickTarget::Outside);
    }

    #[test]
    fn click_translation_uses_list_scroll_offsets() {
        let mut app = App::new();
        for index in 0..8 {
            add_task(&mut app, &format!("Task {index}"));
        }
        app.set_offsets(4, 0);
        let area = Rect::new(0, 0, 80, 24);
        let layout = ui_layout(area);

        assert_eq!(
            click_target(area, (layout.todo.x + 1, layout.todo.y + 1), &app),
            ClickTarget::TodoTask(4)
        );
    }
}
