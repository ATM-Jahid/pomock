use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction as LayoutDirection, Layout, Rect},
    style::{Color, Modifier, Style},
    text::Line,
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph},
};

use crate::{
    app::{App, ClickTarget, ConfirmationOperation, EditMode, ScrollTarget, TimerChange, UiFocus},
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
        ThemeColor::Rgb(red, green, blue) => Color::Rgb(red, green, blue),
    }
}

/// Renders the complete application UI and synchronizes list scroll offsets.
pub fn draw(frame: &mut Frame, app: &mut App, theme: Theme, keys: &KeysConfig) {
    let theme = app
        .settings()
        .map_or(theme, |settings| Theme::from(settings.config().theme()));
    let area = frame.area();
    let controls_text = controls_text(app, keys);
    let controls_text = wrap_help(&controls_text, inner_width(area));
    let layout = ui_layout(area, text_height(&controls_text));

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

    let controls = Paragraph::new(controls_text).alignment(Alignment::Center);

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
        return settings_row_at(area, position, settings)
            .map_or(ClickTarget::Outside, ClickTarget::SettingsRow);
    }
    let controls_text = controls_text(app, app.input_keys());
    let controls_text = wrap_help(&controls_text, inner_width(area));
    let layout = ui_layout(area, text_height(&controls_text));
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

/// Identifies the list under a mouse-wheel/touchpad scroll event.
pub fn scroll_target(area: Rect, position: (u16, u16), app: &App) -> Option<ScrollTarget> {
    let point = position.into();
    if let Some(settings) = app.settings() {
        let footer = settings_footer(settings);
        let (list, _) = settings_parts(area, &footer);
        return list.contains(point).then_some(ScrollTarget::Settings);
    }

    let controls_text = controls_text(app, app.input_keys());
    let controls_text = wrap_help(&controls_text, inner_width(area));
    let layout = ui_layout(area, text_height(&controls_text));
    if layout.todo.contains(point) {
        Some(ScrollTarget::Todo)
    } else if layout.done.contains(point) {
        Some(ScrollTarget::Done)
    } else {
        None
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

fn ui_layout(area: Rect, controls_height: u16) -> UiLayout {
    let inner_area = Block::default().borders(Borders::ALL).inner(area);
    let chunks = Layout::default()
        .direction(LayoutDirection::Vertical)
        .constraints([
            Constraint::Percentage(55),
            Constraint::Percentage(45),
            Constraint::Length(controls_height.min(inner_area.height)),
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

fn inner_width(area: Rect) -> u16 {
    Block::default().borders(Borders::ALL).inner(area).width
}

fn text_height(text: &str) -> u16 {
    u16::try_from(text.lines().count()).unwrap_or(u16::MAX)
}

fn wrap_help(text: &str, width: u16) -> String {
    if width == 0 {
        return String::new();
    }

    let mut lines = Vec::new();
    for source_line in text.lines() {
        let mut current = String::new();
        for item in source_line.split("  ").filter(|item| !item.is_empty()) {
            let candidate = if current.is_empty() {
                item.to_string()
            } else {
                format!("{current}  {item}")
            };
            if Line::from(candidate.as_str()).width() <= usize::from(width) {
                current = candidate;
            } else {
                if !current.is_empty() {
                    lines.push(std::mem::take(&mut current));
                }
                push_item(item, width, &mut current, &mut lines);
            }
        }
        lines.push(current);
    }
    lines.join("\n")
}

fn push_item(item: &str, width: u16, current: &mut String, lines: &mut Vec<String>) {
    for word in item.split_whitespace() {
        let candidate = if current.is_empty() {
            word.to_string()
        } else {
            format!("{current} {word}")
        };
        if Line::from(candidate.as_str()).width() <= usize::from(width) {
            *current = candidate;
        } else {
            if !current.is_empty() {
                lines.push(std::mem::take(current));
            }
            push_word(word, width, current, lines);
        }
    }
}

fn push_word(word: &str, width: u16, current: &mut String, lines: &mut Vec<String>) {
    let mut rest = word;
    while Line::from(rest).width() > usize::from(width) {
        let mut split = rest.len();
        for (index, character) in rest.char_indices() {
            let end = index + character.len_utf8();
            if Line::from(&rest[..end]).width() > usize::from(width) {
                split = if index == 0 {
                    character.len_utf8()
                } else {
                    index
                };
                break;
            }
        }
        lines.push(rest[..split].to_string());
        rest = &rest[split..];
    }
    current.push_str(rest);
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
        return format!("{prompt}  [y/Enter] confirm  [n/Esc] cancel");
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
            let settings = format_key(first_key(keys.settings()));
            match app.ui_focus() {
                UiFocus::Clock => format!(
                    "[{focus_navigation}] box nav  [{}] start/pause  [{}] cycle session  [{}] reset  [{settings}] settings  [{quit}] quit",
                    format_key(first_key(keys.clock_primary())),
                    format_key(first_key(keys.cycle_session())),
                    format_key(first_key(keys.reset_session())),
                ),
                UiFocus::Todo => format!(
                    "[{focus_navigation}] box nav  [{list_navigation}] list nav  [{}] add  [{}] edit  [{}] delete  [{}] complete  [{settings}] settings  [{quit}] quit",
                    format_key(first_key(keys.add_task())),
                    format_key(first_key(keys.edit_task())),
                    format_key(first_key(keys.delete_task())),
                    format_key(first_key(keys.task_primary())),
                ),
                UiFocus::Done => format!(
                    "[{focus_navigation}] box nav  [{list_navigation}] list nav  [{}] add  [{}] edit  [{}] delete  [{}] return  [{settings}] settings  [{quit}] quit",
                    format_key(first_key(keys.add_task())),
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

fn settings_parts(area: Rect, footer_text: &str) -> (Rect, Rect) {
    let inner = Block::default()
        .borders(Borders::ALL)
        .inner(settings_area(area));
    let footer_height = text_height(&wrap_help(footer_text, inner.width)).min(inner.height);
    let chunks = Layout::default()
        .direction(LayoutDirection::Vertical)
        .constraints([
            Constraint::Min(1),
            Constraint::Length(1),
            Constraint::Length(footer_height),
        ])
        .split(inner);
    (chunks[0], chunks[2])
}

#[cfg(test)]
fn settings_offset(selection: usize, visible_rows: usize) -> usize {
    if visible_rows == 0 {
        0
    } else {
        selection.saturating_add(1).saturating_sub(visible_rows)
    }
}

const SETTINGS_GROUPS: [(usize, &str); 6] = [
    (0, "Timer"),
    (4, "Notification"),
    (5, "Sound"),
    (9, "Tasks"),
    (11, "Keys"),
    (26, "Theme"),
];

fn settings_visual_row(selection: usize) -> usize {
    selection
        + SETTINGS_GROUPS
            .iter()
            .filter(|(first_field, _)| selection >= *first_field)
            .count()
}

fn settings_scroll_anchor(selection: usize) -> usize {
    SETTINGS_GROUPS
        .iter()
        .enumerate()
        .find_map(|(group_index, (first_field, _))| {
            (*first_field == selection).then_some(first_field + group_index)
        })
        .unwrap_or_else(|| settings_visual_row(selection))
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

fn settings_row_at(
    area: Rect,
    position: (u16, u16),
    settings: &crate::settings::SettingsOverlay,
) -> Option<usize> {
    let footer = settings_footer(settings);
    let (list, _) = settings_parts(area, &footer);
    let point = position.into();
    if !list.contains(point) {
        return None;
    }
    let row = settings.offset() + usize::from(position.1 - list.y);
    settings_field_row(row)
}

fn draw_settings(frame: &mut Frame, app: &mut App, theme: Theme) {
    let settings = app.settings().expect("settings overlay is open");
    let area = settings_area(frame.area());
    let footer = settings_footer(settings);
    let (list_area, footer_area) = settings_parts(frame.area(), &footer);
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
    *state.offset_mut() = settings
        .offset()
        .min(settings_scroll_anchor(settings.selection()));
    frame.render_stateful_widget(
        List::new(items).highlight_symbol("> ").highlight_style(
            Style::default()
                .fg(theme.todo_highlight)
                .add_modifier(Modifier::BOLD),
        ),
        list_area,
        &mut state,
    );
    app.set_settings_offset(state.offset());

    let footer = wrap_help(&footer, footer_area.width);
    frame.render_widget(
        Paragraph::new(footer).alignment(Alignment::Center),
        footer_area,
    );
}

fn settings_footer(settings: &crate::settings::SettingsOverlay) -> String {
    let close = format_key(first_key(settings.config().keys().settings()));
    if let Some(error) = settings.error() {
        format!("{error}\n[Esc] back")
    } else if settings.input().is_some() {
        let prompt = match settings.field() {
            SettingField::Theme(_) => "Type a preset or #RRGGBB",
            SettingField::CompletionSoundFile | SettingField::FocusSoundFile => {
                "Type an absolute or ~/ path; leave empty to disable"
            }
            _ => "Type a positive number",
        };
        format!("{prompt}  [Enter] apply  [Esc] cancel")
    } else if settings.is_capturing_key() {
        "Press a key  [Esc] cancel".to_string()
    } else {
        format!("[↑/↓ or j/k] select  [←/→ or h/l] change  [Enter/Space] edit  [{close}/Esc] close")
    }
}

fn setting_row(field: SettingField, settings: &crate::settings::SettingsOverlay) -> String {
    let config = settings.config();
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
        SettingField::NotificationEnabled => (
            "  Desktop notifications",
            on_off(config.notification().enabled()).to_string(),
        ),
        SettingField::CompletionSoundEnabled => (
            "  Completion enabled",
            on_off(config.sound().completion().enabled()).to_string(),
        ),
        SettingField::CompletionSoundFile => (
            "  Completion file",
            config
                .sound()
                .completion()
                .file()
                .map_or_else(|| "not set".to_string(), |path| path.display().to_string()),
        ),
        SettingField::FocusSoundEnabled => (
            "  Focus loop enabled",
            on_off(config.sound().focus().enabled()).to_string(),
        ),
        SettingField::FocusSoundFile => (
            "  Focus loop file",
            config
                .sound()
                .focus()
                .file()
                .map_or_else(|| "not set".to_string(), |path| path.display().to_string()),
        ),
        SettingField::PersistTasks => ("  Persist", on_off(config.tasks().persist()).to_string()),
        SettingField::ShowTaskNumbers => (
            "  Show numbers",
            on_off(config.tasks().show_numbers()).to_string(),
        ),
        SettingField::Theme(role) => (
            theme_role_label(role),
            config.theme().color(role).to_string(),
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
        KeyAction::Settings => "  Settings",
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
    fn help_wraps_to_the_available_width_without_losing_controls() {
        let app = App::new();
        let help = controls_text(&app, &KeysConfig::default());

        let wrapped = wrap_help(&help, 28);

        assert!(wrapped.lines().all(|line| Line::from(line).width() <= 28));
        assert_eq!(
            wrapped.split_whitespace().collect::<Vec<_>>(),
            help.split_whitespace().collect::<Vec<_>>()
        );
        assert!(text_height(&wrapped) > 1);
    }

    #[test]
    fn help_wraps_between_complete_key_action_items() {
        let app = App::new();
        let help = controls_text(&app, &KeysConfig::default());

        let wrapped = wrap_help(&help, 18);

        assert!(wrapped.lines().any(|line| line.contains("[q] quit")));
        assert!(wrapped.lines().any(|line| line.contains("[s] settings")));
        assert!(!wrapped.lines().any(|line| line.ends_with("[q]")));
    }

    #[test]
    fn narrow_layout_reserves_every_wrapped_control_row() {
        let app = App::new();
        let area = Rect::new(0, 0, 36, 18);
        let help = wrap_help(&controls_text(&app, app.input_keys()), inner_width(area));

        let layout = ui_layout(area, text_height(&help));

        assert_eq!(layout.controls.height, text_height(&help));
        assert!(layout.clock.height > 0);
        assert!(layout.todo.height > 0);
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
    fn rgb_colors_map_to_terminal_rgb_colors() {
        let config = ThemeConfig::default()
            .with_color(ThemeRole::FocusedBorder, ThemeColor::Rgb(0x5f, 0xd7, 0xff));

        assert_eq!(
            Theme::from(&config).focused_border,
            Color::Rgb(0x5f, 0xd7, 0xff)
        );
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
    fn normal_mode_help_uses_the_configured_settings_key() {
        let app = App::new();
        let keys: KeysConfig = toml::from_str("settings = \"t\"\n").unwrap();

        let help = controls_text(&app, &keys);

        assert!(help.contains("[t] settings"));
        assert!(!help.contains("[s] settings"));
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
            "Discard progress and cycle session?  [y/Enter] confirm  [n/Esc] cancel"
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
            "Quit and discard progress?  [y/Enter] confirm  [n/Esc] cancel"
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
        let help = controls_text(&app, app.input_keys());
        let layout = ui_layout(area, text_height(&wrap_help(&help, inner_width(area))));

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
        let help = controls_text(&app, app.input_keys());
        let layout = ui_layout(area, text_height(&wrap_help(&help, inner_width(area))));

        assert_eq!(
            click_target(area, (layout.todo.x + 1, layout.todo.y + 1), &app),
            ClickTarget::TodoTask(4)
        );
    }

    #[test]
    fn scroll_hit_testing_uses_task_boxes_and_settings_list() {
        let mut app = App::new();
        add_task(&mut app, "First");
        let area = Rect::new(0, 0, 80, 24);
        let help = controls_text(&app, app.input_keys());
        let layout = ui_layout(area, text_height(&wrap_help(&help, inner_width(area))));

        assert_eq!(
            scroll_target(area, (layout.todo.x, layout.todo.y), &app),
            Some(ScrollTarget::Todo)
        );
        assert_eq!(
            scroll_target(area, (layout.done.x, layout.done.y), &app),
            Some(ScrollTarget::Done)
        );
        assert_eq!(
            scroll_target(area, (layout.clock.x, layout.clock.y), &app),
            None
        );

        let _ = app.dispatch(Action::OpenSettings);
        let settings = app.settings().unwrap();
        let footer = settings_footer(settings);
        let (list, footer_area) = settings_parts(area, &footer);
        assert_eq!(
            scroll_target(area, (list.x, list.y), &app),
            Some(ScrollTarget::Settings)
        );
        assert_eq!(
            scroll_target(area, (footer_area.x, footer_area.y), &app),
            None
        );
    }

    #[test]
    fn click_translation_maps_all_visible_session_controls() {
        let app = App::new();
        let area = Rect::new(0, 0, 80, 24);
        let help = controls_text(&app, app.input_keys());
        let layout = ui_layout(area, text_height(&wrap_help(&help, inner_width(area))));
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
        let footer = settings_footer(app.settings().unwrap());
        let (list, _) = settings_parts(area, &footer);
        let selected_row = settings_visual_row(25);
        let first_visible = settings_offset(selected_row, usize::from(list.height));
        let expected = settings_field_row(first_visible).unwrap();
        app.set_settings_offset(first_visible);

        assert_eq!(
            click_target(area, (list.x, list.y), &app),
            ClickTarget::SettingsRow(expected)
        );
        assert_eq!(click_target(area, (0, 0), &app), ClickTarget::Outside);
    }

    #[test]
    fn settings_click_keeps_the_existing_viewport_when_row_is_already_visible() {
        let mut app = App::new();
        let _ = app.dispatch(Action::OpenSettings);
        let area = Rect::new(0, 0, 80, 24);
        let footer = settings_footer(app.settings().unwrap());
        let (list, _) = settings_parts(area, &footer);
        let offset = settings_visual_row(25).saturating_sub(usize::from(list.height)) + 1;
        app.set_settings_offset(offset);

        let clicked = settings_field_row(offset + 2).unwrap();
        assert_eq!(
            click_target(area, (list.x, list.y + 2), &app),
            ClickTarget::SettingsRow(clicked)
        );
        let _ =
            app.handle_click_target(ClickTarget::SettingsRow(clicked), std::time::Instant::now());
        assert_eq!(app.settings().unwrap().selection(), clicked);
        assert_eq!(app.settings().unwrap().offset(), offset);
    }

    #[test]
    fn settings_group_first_fields_scroll_with_their_heading() {
        assert_eq!(settings_scroll_anchor(0), 0);
        assert_eq!(settings_scroll_anchor(4), 5);
        assert_eq!(settings_scroll_anchor(5), 7);
        assert_eq!(settings_scroll_anchor(1), settings_visual_row(1));
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

        for (group_index, (first_field, _)) in SETTINGS_GROUPS.iter().enumerate() {
            let heading_row = first_field + group_index;
            assert_eq!(settings_field_row(heading_row), None);
            assert_eq!(settings_field_row(heading_row + 1), Some(*first_field));
        }
    }

    #[test]
    fn settings_help_shows_fixed_navigation_and_the_active_close_keys() {
        let mut app = App::new();
        let _ = app.dispatch(Action::OpenSettings);
        while app.settings().unwrap().field() != SettingField::Key(KeyAction::Settings) {
            let _ = app.dispatch(Action::SettingsMove(true));
        }
        let _ = app.dispatch(Action::SettingsActivate);
        let _ = app.dispatch(Action::SettingsCaptureKey(ConfigKey::Character('t')));

        let footer = settings_footer(app.settings().unwrap());

        assert!(footer.contains("[←/→ or h/l] change"));
        assert!(footer.contains("[Enter/Space] edit"));
        assert!(footer.contains("[t/Esc] close"));
        assert!(!footer.contains("[s/Esc] close"));
    }

    #[test]
    fn narrow_settings_overlay_reserves_every_wrapped_help_row() {
        let mut app = App::new();
        let _ = app.dispatch(Action::OpenSettings);
        let settings = app.settings().unwrap();
        let area = Rect::new(0, 0, 40, 18);
        let footer = settings_footer(settings);

        let (list, footer_area) = settings_parts(area, &footer);
        let wrapped = wrap_help(&footer, footer_area.width);

        assert!(footer_area.height > 2);
        assert_eq!(footer_area.height, text_height(&wrapped));
        assert!(list.height > 0);
        assert!(
            wrapped
                .lines()
                .all(|line| Line::from(line).width() <= usize::from(footer_area.width))
        );
    }

    #[test]
    fn settings_overlay_separates_list_from_help_footer() {
        let mut app = App::new();
        let _ = app.dispatch(Action::OpenSettings);
        let settings = app.settings().unwrap();
        let area = Rect::new(0, 0, 80, 24);
        let footer = settings_footer(settings);

        let (list, footer_area) = settings_parts(area, &footer);

        assert_eq!(footer_area.y, list.bottom() + 1);
    }

    #[test]
    fn settings_help_wraps_between_complete_actions() {
        let mut app = App::new();
        let _ = app.dispatch(Action::OpenSettings);
        let footer = settings_footer(app.settings().unwrap());

        let wrapped = wrap_help(&footer, 24);

        assert!(
            wrapped
                .lines()
                .any(|line| line.contains("[Enter/Space] edit"))
        );
        assert!(wrapped.lines().any(|line| line.contains("[s/Esc] close")));
    }
}
