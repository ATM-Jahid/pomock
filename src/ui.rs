use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction as LayoutDirection, Layout, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph},
};

use crate::{
    app::{App, ClickTarget, ConfirmationOperation, EditMode, TimerChange, UiFocus},
    config::{ConfigKey, KeyAction, KeysConfig, ThemeColor, ThemeConfig, ThemeRole},
    display::{format_big_duration, format_key, format_state},
    settings::SettingField,
    timer::{SessionKind, TimerState},
};

#[derive(Debug, Clone, Copy)]
struct UiLayout {
    clock: Rect,
    todo: Rect,
    done: Rect,
    controls: Rect,
}

#[derive(Debug, Clone, Copy)]
struct ClockLayout {
    state: Rect,
    remaining: Rect,
    completed_sessions: Rect,
    session_controls: [Rect; 3],
}

#[derive(Debug, Clone, Copy)]
pub struct Theme {
    focused_border: Color,
    unfocused_border: Color,
    todo_highlight: Color,
    done_highlight: Color,
    completed_sessions: Color,
}

impl From<&ThemeConfig> for Theme {
    fn from(config: &ThemeConfig) -> Self {
        Self {
            focused_border: theme_color(config.focused_border()),
            unfocused_border: theme_color(config.unfocused_border()),
            todo_highlight: theme_color(config.todo_highlight()),
            done_highlight: theme_color(config.done_highlight()),
            completed_sessions: theme_color(config.completed_sessions()),
        }
    }
}

fn theme_color(color: ThemeColor) -> Color {
    match color {
        ThemeColor::Black => Color::Black,
        ThemeColor::Red => Color::Red,
        ThemeColor::Green => Color::Green,
        ThemeColor::Yellow => Color::Yellow,
        ThemeColor::Blue => Color::Blue,
        ThemeColor::Magenta => Color::Magenta,
        ThemeColor::Cyan => Color::Cyan,
        ThemeColor::Gray => Color::Gray,
        ThemeColor::DarkGray => Color::DarkGray,
        ThemeColor::LightRed => Color::LightRed,
        ThemeColor::LightGreen => Color::LightGreen,
        ThemeColor::LightYellow => Color::LightYellow,
        ThemeColor::LightBlue => Color::LightBlue,
        ThemeColor::LightMagenta => Color::LightMagenta,
        ThemeColor::LightCyan => Color::LightCyan,
        ThemeColor::White => Color::White,
    }
}

/// Renders the complete application UI and synchronizes list scroll offsets.
pub fn draw(frame: &mut Frame, app: &mut App, theme: Theme, keys: &KeysConfig) {
    let theme = app
        .settings()
        .map_or(theme, |settings| Theme::from(settings.draft().theme()));
    let area = frame.area();
    let layout = ui_layout(area);

    let outer_block = Block::default().title("pomock").borders(Borders::ALL);
    frame.render_widget(outer_block, area);

    let clock_block = focused_block("Clock", app.ui_focus() == UiFocus::Clock, theme);
    frame.render_widget(clock_block, layout.clock);
    let clock_layout = clock_layout(layout.clock);

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
    let current_session = match app.timer().state() {
        TimerState::Ready(session) | TimerState::Running(session) | TimerState::Paused(session) => {
            session
        }
    };
    let session_controls = [
        (SessionKind::Focus, "Focus"),
        (SessionKind::ShortBreak, "Short break"),
        (SessionKind::LongBreak, "Long break"),
    ];

    let controls = Paragraph::new(controls_text(app, keys)).alignment(Alignment::Center);

    let todo_items: Vec<ListItem> = app
        .tasks()
        .pending()
        .enumerate()
        .map(|(index, task)| {
            ListItem::new(task_label(
                index,
                task.description(),
                app.show_task_numbers(),
            ))
        })
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
        .enumerate()
        .map(|(index, task)| {
            ListItem::new(task_label(
                index,
                task.description(),
                app.show_task_numbers(),
            ))
        })
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

    frame.render_widget(state, clock_layout.state);
    frame.render_widget(remaining, clock_layout.remaining);
    frame.render_widget(completed_sessions, clock_layout.completed_sessions);
    for ((session, label), area) in session_controls
        .into_iter()
        .zip(clock_layout.session_controls)
    {
        let style = if session == current_session {
            Style::default().add_modifier(Modifier::REVERSED)
        } else {
            Style::default()
        };
        frame.render_widget(
            Paragraph::new(format!("[ {label} ]"))
                .alignment(Alignment::Center)
                .style(style),
            area,
        );
    }
    frame.render_stateful_widget(todo, layout.todo, &mut todo_state);
    frame.render_stateful_widget(done, layout.done, &mut done_state);
    frame.render_widget(controls, layout.controls);
    app.set_offsets(todo_state.offset(), done_state.offset());

    if app.is_settings_open() {
        draw_settings(frame, app, theme);
    }
}

/// Translates terminal coordinates into a semantic application click target.
pub fn click_target(area: Rect, position: (u16, u16), app: &App) -> ClickTarget {
    if let Some(settings) = app.settings() {
        return settings_row_at(area, position, settings.selection())
            .map_or(ClickTarget::Outside, ClickTarget::SettingsRow);
    }
    let layout = ui_layout(area);
    let point = position.into();

    if let Some(session) = session_control_at(layout.clock, point) {
        ClickTarget::SessionControl(session)
    } else if layout.clock.contains(point) {
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

fn clock_layout(area: Rect) -> ClockLayout {
    let inner = Block::default().borders(Borders::ALL).inner(area);
    let chunks = Layout::default()
        .direction(LayoutDirection::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(inner);
    let controls = Layout::default()
        .direction(LayoutDirection::Horizontal)
        .constraints([
            Constraint::Ratio(1, 3),
            Constraint::Ratio(1, 3),
            Constraint::Ratio(1, 3),
        ])
        .split(chunks[3]);

    ClockLayout {
        state: chunks[0],
        remaining: chunks[1],
        completed_sessions: chunks[2],
        session_controls: [controls[0], controls[1], controls[2]],
    }
}

fn session_control_at(area: Rect, point: ratatui::layout::Position) -> Option<SessionKind> {
    let controls = clock_layout(area).session_controls;
    [
        SessionKind::Focus,
        SessionKind::ShortBreak,
        SessionKind::LongBreak,
    ]
    .into_iter()
    .zip(controls)
    .find_map(|(session, area)| area.contains(point).then_some(session))
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

fn task_label(index: usize, description: &str, show_numbers: bool) -> String {
    if show_numbers {
        format!("  {}. {description}", index + 1)
    } else {
        format!("  {description}")
    }
}

fn controls_text(app: &App, keys: &KeysConfig) -> String {
    if let Some(operation) = app.pending_confirmation() {
        let prompt = confirmation_prompt(operation);
        return format!("{prompt} [y/Enter] confirm [n/Esc] cancel");
    }

    match app.edit_mode() {
        EditMode::Adding => format!("Add task: {}_", app.input()),
        EditMode::Editing { .. } => format!("Edit task: {}_", app.input()),
        EditMode::Normal => {
            let focus_navigation = key_labels(&[
                first_key(keys.focus_left()),
                first_key(keys.focus_down()),
                first_key(keys.focus_up()),
                first_key(keys.focus_right()),
            ]);
            let list_navigation =
                key_labels(&[first_key(keys.list_down()), first_key(keys.list_up())]);
            let quit = format_key(first_key(keys.quit()));
            match app.ui_focus() {
                UiFocus::Clock => format!(
                    "[{focus_navigation}] box nav [{}] start/pause [{}] cycle session [{}] reset [s] settings [{quit}] quit",
                    format_key(first_key(keys.clock_primary())),
                    format_key(first_key(keys.cycle_session())),
                    format_key(first_key(keys.reset_session())),
                ),
                UiFocus::Todo => format!(
                    "[{focus_navigation}] box nav [{list_navigation}] list nav [{}] add [{}] edit [{}] delete [{}] complete [s] settings [{quit}] quit",
                    format_key(first_key(keys.add_task())),
                    format_key(first_key(keys.edit_task())),
                    format_key(first_key(keys.delete_task())),
                    format_key(first_key(keys.task_primary())),
                ),
                UiFocus::Done => format!(
                    "[{focus_navigation}] box nav [{list_navigation}] list nav [{}] edit [{}] delete [{}] return [s] settings [{quit}] quit",
                    format_key(first_key(keys.edit_task())),
                    format_key(first_key(keys.delete_task())),
                    format_key(first_key(keys.task_primary())),
                ),
            }
        }
    }
}

fn first_key(keys: &[ConfigKey]) -> ConfigKey {
    keys[0]
}

fn key_labels(keys: &[ConfigKey]) -> String {
    keys.iter()
        .map(|key| format_key(*key))
        .collect::<Vec<_>>()
        .join("/")
}

fn confirmation_prompt(operation: ConfirmationOperation) -> String {
    match operation {
        ConfirmationOperation::Quit => "Quit and discard progress?".to_string(),
        ConfirmationOperation::TimerChange(change) => match change {
            TimerChange::Reset => "Reset session?".to_string(),
            TimerChange::Cycle => "Discard progress and cycle session?".to_string(),
            TimerChange::SelectSession(session) => {
                format!("Discard progress and change to {}?", session_label(session))
            }
            TimerChange::StartSession(session) => format!(
                "Discard progress, change to {}, and start it?",
                session_label(session)
            ),
        },
        ConfirmationOperation::ApplySettings(_) => {
            "Apply timer settings and discard progress?".to_string()
        }
    }
}

fn settings_area(area: Rect) -> Rect {
    let width = area.width.saturating_sub(4).min(72);
    let height = area.height.saturating_sub(2).min(30);
    Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    )
}

fn settings_parts(area: Rect) -> (Rect, Rect) {
    let inner = Block::default()
        .borders(Borders::ALL)
        .inner(settings_area(area));
    let chunks = Layout::default()
        .direction(LayoutDirection::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(2)])
        .split(inner);
    (chunks[0], chunks[1])
}

fn settings_offset(selection: usize, visible_rows: usize) -> usize {
    if visible_rows == 0 {
        0
    } else {
        selection.saturating_add(1).saturating_sub(visible_rows)
    }
}

const SETTINGS_GROUPS: [(usize, &str); 4] =
    [(0, "Timer"), (4, "Tasks"), (6, "Theme"), (11, "Keys")];

fn settings_visual_row(selection: usize) -> usize {
    selection
        + SETTINGS_GROUPS
            .iter()
            .filter(|(first_field, _)| selection >= *first_field)
            .count()
}

fn settings_field_row(visual_row: usize) -> Option<usize> {
    let mut headings_before = 0;
    for (group_index, (first_field, _)) in SETTINGS_GROUPS.iter().enumerate() {
        let heading_row = first_field + group_index;
        if visual_row == heading_row {
            return None;
        }
        if visual_row > heading_row {
            headings_before += 1;
        }
    }
    let row = visual_row.saturating_sub(headings_before);
    (row < SettingField::ALL.len()).then_some(row)
}

fn settings_row_at(area: Rect, position: (u16, u16), selection: usize) -> Option<usize> {
    let (list, _) = settings_parts(area);
    let point = position.into();
    if !list.contains(point) {
        return None;
    }
    let visible = usize::from(list.height);
    let selected_row = settings_visual_row(selection);
    let row = settings_offset(selected_row, visible) + usize::from(position.1 - list.y);
    settings_field_row(row)
}

fn draw_settings(frame: &mut Frame, app: &App, theme: Theme) {
    let settings = app.settings().expect("settings overlay is open");
    let area = settings_area(frame.area());
    let (list_area, footer_area) = settings_parts(frame.area());
    frame.render_widget(Clear, area);
    frame.render_widget(
        Block::default()
            .title("Settings")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.focused_border)),
        area,
    );

    let mut items = Vec::with_capacity(SettingField::ALL.len() + SETTINGS_GROUPS.len());
    for (index, field) in SettingField::ALL.iter().enumerate() {
        if let Some((_, heading)) = SETTINGS_GROUPS
            .iter()
            .find(|(first_field, _)| *first_field == index)
        {
            items
                .push(ListItem::new(*heading).style(Style::default().add_modifier(Modifier::BOLD)));
        }
        items.push(ListItem::new(setting_row(*field, settings)));
    }
    let selected_row = settings_visual_row(settings.selection());
    let mut state = ListState::default().with_selected(Some(selected_row));
    *state.offset_mut() = settings_offset(selected_row, usize::from(list_area.height));
    frame.render_stateful_widget(
        List::new(items).highlight_symbol("> ").highlight_style(
            Style::default()
                .fg(theme.todo_highlight)
                .add_modifier(Modifier::BOLD),
        ),
        list_area,
        &mut state,
    );

    let footer = if let Some(error) = settings.error() {
        format!("{error}\n[Esc] back")
    } else if settings.input().is_some() {
        "Type a positive number  [Enter] apply  [s] save  [Esc] cancel".to_string()
    } else if settings.is_capturing_key() {
        "Press a key  [s] save  [Esc] cancel".to_string()
    } else {
        "[↑/↓ or j/k] select  [←/→] change  [Enter] edit  [s] save  [Esc] close".to_string()
    };
    frame.render_widget(
        Paragraph::new(footer).alignment(Alignment::Center),
        footer_area,
    );
}

fn setting_row(field: SettingField, settings: &crate::settings::SettingsOverlay) -> String {
    let config = settings.draft();
    let (label, value) = match field {
        SettingField::FocusMinutes => (
            "  Focus minutes",
            config.timer().focus_minutes().to_string(),
        ),
        SettingField::ShortBreakMinutes => (
            "  Short break minutes",
            config.timer().short_break_minutes().to_string(),
        ),
        SettingField::LongBreakMinutes => (
            "  Long break minutes",
            config.timer().long_break_minutes().to_string(),
        ),
        SettingField::LongBreakInterval => (
            "  Long break interval",
            config.timer().long_break_interval().to_string(),
        ),
        SettingField::PersistTasks => ("  Persist", on_off(config.tasks().persist()).to_string()),
        SettingField::ShowTaskNumbers => (
            "  Show numbers",
            on_off(config.tasks().show_numbers()).to_string(),
        ),
        SettingField::Theme(role) => (
            theme_role_label(role),
            format!("{:?}", config.theme().color(role)),
        ),
        SettingField::Key(action) => (
            key_action_label(action),
            config
                .keys()
                .binding(action)
                .iter()
                .map(|key| format_key(*key))
                .collect::<Vec<_>>()
                .join("/"),
        ),
    };
    let value = if settings.field() == field {
        settings.input().map_or(value, |input| format!("{input}_"))
    } else {
        value
    };
    if value.is_empty() {
        label.to_string()
    } else {
        format!("{label}: {value}")
    }
}

fn on_off(value: bool) -> &'static str {
    if value { "on" } else { "off" }
}

fn theme_role_label(role: ThemeRole) -> &'static str {
    match role {
        ThemeRole::FocusedBorder => "  Focused border",
        ThemeRole::UnfocusedBorder => "  Unfocused border",
        ThemeRole::TodoHighlight => "  To-do highlight",
        ThemeRole::DoneHighlight => "  Done highlight",
        ThemeRole::CompletedSessions => "  Completed sessions",
    }
}

fn key_action_label(action: KeyAction) -> &'static str {
    match action {
        KeyAction::FocusLeft => "  Focus left",
        KeyAction::FocusDown => "  Focus down",
        KeyAction::FocusUp => "  Focus up",
        KeyAction::FocusRight => "  Focus right",
        KeyAction::ListDown => "  List down",
        KeyAction::ListUp => "  List up",
        KeyAction::Quit => "  Quit",
        KeyAction::ClockPrimary => "  Clock primary",
        KeyAction::CycleSession => "  Cycle session",
        KeyAction::ResetSession => "  Reset session",
        KeyAction::AddTask => "  Add task",
        KeyAction::EditTask => "  Edit task",
        KeyAction::DeleteTask => "  Delete task",
        KeyAction::TaskPrimary => "  Task primary",
    }
}

fn session_label(session: SessionKind) -> &'static str {
    match session {
        SessionKind::Focus => "Focus",
        SessionKind::ShortBreak => "Short break",
        SessionKind::LongBreak => "Long break",
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
    use std::time::Duration;

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
    fn clock_legend_describes_cycle_session_control() {
        let app = App::new();
        let keys = KeysConfig::default();

        let help = controls_text(&app, &keys);

        assert!(help.contains("[H/J/K/L] box nav"));
        assert!(help.contains("[c] cycle session"));
        assert!(help.contains("[s] settings"));
        assert!(!help.contains("F2"));
        assert!(!help.contains("[n] next"));
    }

    #[test]
    fn task_legend_shows_only_the_first_default_list_keys() {
        let mut app = App::new();
        let _ = app.dispatch(Action::NavigateFocus(Direction::Down));

        let help = controls_text(&app, &KeysConfig::default());

        assert!(help.contains("[j/k] list nav"));
        assert!(!help.contains('↓'));
        assert!(!help.contains('↑'));
    }

    #[test]
    fn configured_colors_map_to_their_semantic_theme_roles() {
        let config = ThemeConfig::new(
            ThemeColor::LightBlue,
            ThemeColor::Black,
            ThemeColor::LightYellow,
            ThemeColor::LightGreen,
            ThemeColor::Cyan,
        );

        let theme = Theme::from(&config);

        assert_eq!(theme.focused_border, Color::LightBlue);
        assert_eq!(theme.unfocused_border, Color::Black);
        assert_eq!(theme.todo_highlight, Color::LightYellow);
        assert_eq!(theme.done_highlight, Color::LightGreen);
        assert_eq!(theme.completed_sessions, Color::Cyan);
    }

    #[test]
    fn normal_mode_help_uses_configured_key_labels() {
        let app = App::new();
        let keys: KeysConfig = toml::from_str(
            "focus_left = \"left\"\nclock_primary = \"enter\"\ncycle_session = \"n\"\n",
        )
        .unwrap();

        let help = controls_text(&app, &keys);

        assert!(help.contains("[←/J/K/L] box nav"));
        assert!(help.contains("[Enter] start/pause"));
        assert!(help.contains("[n] cycle session"));
        assert!(!help.contains("[c] cycle session"));
    }

    #[test]
    fn normal_mode_help_uses_only_the_first_key_for_each_action() {
        let app = App::new();
        let keys: KeysConfig = toml::from_str(
            "clock_primary = [\"enter\", \"space\"]\ncycle_session = [\"n\", \"c\"]\n",
        )
        .unwrap();

        let help = controls_text(&app, &keys);

        assert!(help.contains("[Enter] start/pause"));
        assert!(help.contains("[n] cycle session"));
        assert!(!help.contains("space"));
        assert!(!help.contains("[c] cycle session"));
    }

    #[test]
    fn cycle_confirmation_describes_progress_loss() {
        let mut app = App::new();
        let _ = app.dispatch(Action::PrimaryAction);
        let _ = app.tick(Duration::from_secs(10));
        let _ = app.dispatch(Action::CycleSession);

        assert_eq!(
            controls_text(&app, &KeysConfig::default()),
            "Discard progress and cycle session? [y/Enter] confirm [n/Esc] cancel"
        );
    }

    #[test]
    fn quit_confirmation_describes_progress_loss() {
        let mut app = App::new();
        let _ = app.dispatch(Action::PrimaryAction);
        let _ = app.tick(Duration::from_secs(10));
        let _ = app.dispatch(Action::Quit);

        assert_eq!(
            controls_text(&app, &KeysConfig::default()),
            "Quit and discard progress? [y/Enter] confirm [n/Esc] cancel"
        );
    }

    #[test]
    fn session_change_confirmation_distinguishes_select_from_start() {
        assert_eq!(
            confirmation_prompt(ConfirmationOperation::TimerChange(
                TimerChange::SelectSession(SessionKind::ShortBreak),
            )),
            "Discard progress and change to Short break?"
        );
        assert_eq!(
            confirmation_prompt(ConfirmationOperation::TimerChange(
                TimerChange::StartSession(SessionKind::ShortBreak),
            )),
            "Discard progress, change to Short break, and start it?"
        );
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
    fn task_labels_can_show_or_hide_one_based_numbers() {
        assert_eq!(task_label(0, "First", true), "  1. First");
        assert_eq!(task_label(11, "Twelfth", true), "  12. Twelfth");
        assert_eq!(task_label(0, "First", false), "  First");
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

    #[test]
    fn click_translation_maps_all_visible_session_controls() {
        let app = App::new();
        let area = Rect::new(0, 0, 80, 24);
        let layout = ui_layout(area);
        let controls = clock_layout(layout.clock).session_controls;

        for (control, session) in controls.into_iter().zip([
            SessionKind::Focus,
            SessionKind::ShortBreak,
            SessionKind::LongBreak,
        ]) {
            assert_eq!(
                click_target(area, (control.x, control.y), &app),
                ClickTarget::SessionControl(session)
            );
        }
    }

    #[test]
    fn settings_hit_testing_uses_the_visible_scrolled_rows() {
        let mut app = App::new();
        let _ = app.dispatch(Action::OpenSettings);
        for _ in 0..25 {
            let _ = app.dispatch(Action::SettingsMove(true));
        }
        let area = Rect::new(0, 0, 80, 24);
        let (list, _) = settings_parts(area);
        let selected_row = settings_visual_row(24);
        let first_visible = settings_offset(selected_row, usize::from(list.height));
        let expected = settings_field_row(first_visible).unwrap();

        assert_eq!(
            click_target(area, (list.x, list.y), &app),
            ClickTarget::SettingsRow(expected)
        );
        assert_eq!(click_target(area, (0, 0), &app), ClickTarget::Outside);
    }

    #[test]
    fn settings_groups_have_one_heading_and_indented_option_rows() {
        let mut app = App::new();
        let _ = app.dispatch(Action::OpenSettings);
        let settings = app.settings().unwrap();

        assert_eq!(
            theme_role_label(ThemeRole::FocusedBorder),
            "  Focused border"
        );
        assert!(
            !setting_row(SettingField::Theme(ThemeRole::FocusedBorder), settings)
                .contains("Theme /")
        );
        assert!(!setting_row(SettingField::FocusMinutes, settings).contains("Timer /"));
        assert!(!setting_row(SettingField::PersistTasks, settings).contains("Tasks /"));
        assert!(!setting_row(SettingField::Key(KeyAction::FocusLeft), settings).contains("Keys /"));

        let area = Rect::new(0, 0, 80, 24);
        let (list, _) = settings_parts(area);
        for (group_index, (first_field, _)) in SETTINGS_GROUPS.iter().enumerate() {
            let heading_row = first_field + group_index;
            assert_eq!(
                click_target(area, (list.x, list.y + heading_row as u16), &app),
                ClickTarget::Outside
            );
            assert_eq!(
                click_target(area, (list.x, list.y + heading_row as u16 + 1), &app),
                ClickTarget::SettingsRow(*first_field)
            );
        }
    }
}
