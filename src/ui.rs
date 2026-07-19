use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction as LayoutDirection, Layout, Rect},
    style::{Color, Modifier, Style},
    text::Line,
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph},
};

use crate::{
    app::{
        Action, App, ClickTarget, ConfirmationOperation, EditMode, ScrollTarget, TimerChange,
        UiFocus,
    },
    config::{ConfigKey, KeyAction, KeysConfig, ThemeColor, ThemeConfig, ThemeRole},
    display::{format_big_duration_at_scale, format_duration, format_key, format_state},
    settings::SettingField,
    timer::{SessionKind, TimerState},
    ui_layout::{ClockFace, LayoutRequest, resolve},
};

pub use crate::ui_layout::FrameGeometry;

#[cfg(test)]
use crate::ui_layout::{WorkspaceMode, clock_geometry};

#[derive(Debug, Clone, Copy)]
pub struct Theme {
    focused_border: Color,
    unfocused_border: Color,
    todo_highlight: Color,
    done_highlight: Color,
    focus: Color,
    short_break: Color,
    long_break: Color,
}

impl From<&ThemeConfig> for Theme {
    fn from(config: &ThemeConfig) -> Self {
        Self {
            focused_border: theme_color(config.focused_border()),
            unfocused_border: theme_color(config.unfocused_border()),
            todo_highlight: theme_color(config.todo_highlight()),
            done_highlight: theme_color(config.done_highlight()),
            focus: theme_color(config.focus()),
            short_break: theme_color(config.short_break()),
            long_break: theme_color(config.long_break()),
        }
    }
}

impl Theme {
    fn session(self, session: SessionKind) -> Color {
        match session {
            SessionKind::Focus => self.focus,
            SessionKind::ShortBreak => self.short_break,
            SessionKind::LongBreak => self.long_break,
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
pub fn draw(frame: &mut Frame, app: &mut App, theme: Theme, keys: &KeysConfig) -> FrameGeometry {
    let theme = app
        .settings()
        .map_or(theme, |settings| Theme::from(settings.config().theme()));
    let area = frame.area();
    let controls_text = controls_text(app, keys);
    let controls_text = wrap_help(&controls_text, inner_width(area));
    let layout = resolve(LayoutRequest {
        area,
        controls_height: text_height(&controls_text),
        focus: app.ui_focus(),
        last_task_focus: app.last_task_focus(),
        duration: app.timer().remaining(),
    });

    let outer_block = Block::default().title("pomock").borders(Borders::ALL);
    frame.render_widget(outer_block, area);

    let remaining_duration = app.timer().remaining();

    let state_text = app.pending_autostart().map_or_else(
        || format_state(app.timer().state()).to_string(),
        |(session, seconds)| format!("Next: {} (autostart in {seconds}s)", session_label(session)),
    );
    let current_session = current_session(app.timer().state());
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

    if let Some(clock_layout) = layout.clock() {
        let clock_area = clock_layout.area;
        let clock_block = focused_block("Clock", app.ui_focus() == UiFocus::Clock, theme);
        frame.render_widget(clock_block, clock_area);
        let state = Paragraph::new(clock_status_text(
            &state_text,
            app.timer().state(),
            clock_layout.state.width,
        ))
        .alignment(Alignment::Center);
        let remaining_text = match clock_layout.face {
            ClockFace::Text => format_duration(remaining_duration),
            ClockFace::Glyphs { scale } => format_big_duration_at_scale(remaining_duration, scale),
        };
        let remaining = Paragraph::new(remaining_text)
            .alignment(Alignment::Center)
            .style(
                Style::default()
                    .fg(theme.session(current_session))
                    .add_modifier(Modifier::BOLD),
            );
        frame.render_widget(state, clock_layout.state);
        frame.render_widget(remaining, clock_layout.remaining);
        let completed = clock_completed_text(
            app.timer().completed_focus_sessions(),
            clock_layout.completed_sessions.width,
        );
        frame.render_widget(
            Paragraph::new(completed).alignment(Alignment::Center),
            clock_layout.completed_sessions,
        );
        for ((session, label), area) in session_controls
            .into_iter()
            .zip(clock_layout.session_controls)
        {
            let style = session_button_style(session, current_session, theme);
            frame.render_widget(
                Paragraph::new(session_control_label(session, label, area.width))
                    .alignment(Alignment::Center)
                    .style(style),
                area,
            );
        }
    }
    if let Some(todo_area) = layout.todo() {
        frame.render_stateful_widget(todo, todo_area, &mut todo_state);
    }
    if let Some(done_area) = layout.done() {
        frame.render_stateful_widget(done, done_area, &mut done_state);
    }
    if layout.controls().width > 0 && layout.controls().height > 0 {
        frame.render_widget(controls, layout.controls());
    }
    app.set_offsets(todo_state.offset(), done_state.offset());

    if app.is_settings_open() {
        draw_settings(frame, app, theme);
    }

    layout
}

fn current_session(state: TimerState) -> SessionKind {
    match state {
        TimerState::Ready(session) | TimerState::Running(session) | TimerState::Paused(session) => {
            session
        }
    }
}

fn session_button_style(session: SessionKind, current: SessionKind, theme: Theme) -> Style {
    if session == current {
        Style::default()
            .fg(theme.session(session))
            .add_modifier(Modifier::REVERSED)
    } else {
        Style::default()
    }
}

fn clock_status_text(full: &str, state: TimerState, width: u16) -> String {
    if full.len() <= usize::from(width) {
        return full.to_string();
    }

    let (session, activity) = match state {
        TimerState::Ready(session) => (session, "Ready"),
        TimerState::Running(session) => (session, "Running"),
        TimerState::Paused(session) => (session, "Paused"),
    };
    let session = session_label(session);
    [activity, session]
        .into_iter()
        .find(|candidate| candidate.len() <= usize::from(width))
        .unwrap_or("")
        .to_string()
}

fn clock_completed_text(completed: u32, width: u16) -> String {
    let full = format!("Focus sessions completed: {completed}");
    if full.len() <= usize::from(width) {
        full
    } else {
        let count = completed.to_string();
        if count.len() <= usize::from(width) {
            count
        } else {
            String::new()
        }
    }
}

fn session_control_label(session: SessionKind, full: &str, width: u16) -> String {
    let full = format!("[ {full} ]");
    if full.len() <= usize::from(width) {
        return full;
    }

    let initial = match session {
        SessionKind::Focus => 'F',
        SessionKind::ShortBreak => 'S',
        SessionKind::LongBreak => 'L',
    };
    let bracketed = format!("[{initial}]");
    if bracketed.len() <= usize::from(width) {
        bracketed
    } else if width > 0 {
        initial.to_string()
    } else {
        String::new()
    }
}

/// Translates terminal coordinates into a semantic application click target.
pub fn click_target(layout: &FrameGeometry, position: (u16, u16), app: &App) -> ClickTarget {
    if let Some(settings) = app.settings() {
        return settings_row_at(layout.area(), position, settings)
            .map_or(ClickTarget::Outside, ClickTarget::SettingsRow);
    }
    let point = position.into();

    if let Some(session) = layout
        .clock()
        .and_then(|clock| session_control_at(clock.session_controls, point))
    {
        ClickTarget::SessionControl(session)
    } else if layout
        .clock()
        .is_some_and(|clock| clock.area.contains(point))
    {
        ClickTarget::Clock
    } else if let Some(area) = layout.todo().filter(|area| area.contains(point)) {
        task_row_at(
            position,
            area,
            app.todo_offset(),
            app.tasks().pending().count(),
        )
        .map_or(ClickTarget::Todo, ClickTarget::TodoTask)
    } else if let Some(area) = layout.done().filter(|area| area.contains(point)) {
        task_row_at(
            position,
            area,
            app.done_offset(),
            app.tasks().completed().count(),
        )
        .map_or(ClickTarget::Done, ClickTarget::DoneTask)
    } else {
        ClickTarget::Outside
    }
}

/// Identifies the list under a mouse-wheel/touchpad scroll event.
pub fn scroll_target(
    layout: &FrameGeometry,
    position: (u16, u16),
    app: &App,
) -> Option<ScrollTarget> {
    let point = position.into();
    if let Some(settings) = app.settings() {
        let footer = settings_footer(settings);
        let (list, _) = settings_parts(layout.area(), &footer);
        return list.contains(point).then_some(ScrollTarget::Settings);
    }

    if layout.todo().is_some_and(|area| area.contains(point)) {
        Some(ScrollTarget::Todo)
    } else if layout.done().is_some_and(|area| area.contains(point)) {
        Some(ScrollTarget::Done)
    } else {
        None
    }
}

/// Returns whether the panel targeted by an action exists in the rendered frame.
///
/// Focus navigation and global actions remain available so a hidden semantic panel can be
/// brought into view without losing selection or editing state.
pub fn action_target_visible(layout: &FrameGeometry, focus: UiFocus, action: &Action) -> bool {
    let targets_current_panel = matches!(
        action,
        Action::BeginAdd
            | Action::EditSelected
            | Action::DeleteSelected
            | Action::PrimaryAction
            | Action::MoveSelectedTask(_)
            | Action::MoveSelection(_)
    );
    if !targets_current_panel {
        return true;
    }

    match focus {
        UiFocus::Clock => layout.clock().is_some(),
        UiFocus::Todo => layout.todo().is_some(),
        UiFocus::Done => layout.done().is_some(),
    }
}

fn session_control_at(
    controls: [Rect; 3],
    point: ratatui::layout::Position,
) -> Option<SessionKind> {
    [
        SessionKind::Focus,
        SessionKind::ShortBreak,
        SessionKind::LongBreak,
    ]
    .into_iter()
    .zip(controls)
    .find_map(|(session, area)| area.contains(point).then_some(session))
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

    if let Some((session, seconds)) = app.pending_autostart() {
        return format!(
            "Next: {} in {seconds}s  [{}] start now  [{}] cycle/cancel  [Esc] cancel",
            session_label(session),
            format_key(first_key(keys.clock_primary())),
            format_key(first_key(keys.cycle_session())),
        );
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
            let item_movement = key_labels(&[
                first_key(keys.move_task_up()),
                first_key(keys.move_task_down()),
            ]);
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
                    "[{focus_navigation}] box nav  [{list_navigation}] list nav  [{item_movement}] move list item  [{}] add  [{}] edit  [{}] delete  [{}] complete  [{settings}] settings  [{quit}] quit",
                    format_key(first_key(keys.add_task())),
                    format_key(first_key(keys.edit_task())),
                    format_key(first_key(keys.delete_task())),
                    format_key(first_key(keys.task_primary())),
                ),
                UiFocus::Done => format!(
                    "[{focus_navigation}] box nav  [{list_navigation}] list nav  [{item_movement}] move list item  [{}] add  [{}] edit  [{}] delete  [{}] return  [{settings}] settings  [{quit}] quit",
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

fn settings_group_start(group_index: usize) -> usize {
    SettingField::GROUPS[..group_index]
        .iter()
        .map(|(_, fields)| fields.len())
        .sum()
}

fn settings_visual_row(selection: usize) -> usize {
    selection
        + SettingField::GROUPS
            .iter()
            .enumerate()
            .filter(|(group_index, _)| selection >= settings_group_start(*group_index))
            .count()
}

fn settings_scroll_anchor(selection: usize) -> usize {
    SettingField::GROUPS
        .iter()
        .enumerate()
        .find_map(|(group_index, _)| {
            let first_field = settings_group_start(group_index);
            (first_field == selection).then_some(first_field + group_index)
        })
        .unwrap_or_else(|| settings_visual_row(selection))
}

fn settings_field_row(visual_row: usize) -> Option<usize> {
    let mut headings_before = 0;
    for (group_index, _) in SettingField::GROUPS.iter().enumerate() {
        let first_field = settings_group_start(group_index);
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

    let mut items = Vec::with_capacity(SettingField::ALL.len() + SettingField::GROUPS.len());
    for (index, field) in SettingField::ALL.iter().enumerate() {
        if let Some((_, (heading, _))) = SettingField::GROUPS
            .iter()
            .enumerate()
            .find(|(group_index, _)| settings_group_start(*group_index) == index)
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
            SettingField::FocusDuration
            | SettingField::ShortBreakDuration
            | SettingField::LongBreakDuration => "Type a duration as MM:SS (max 9999:59)",
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
        SettingField::FocusDuration => (
            "  Focus duration",
            crate::config::format_duration(config.timer().focus_duration()),
        ),
        SettingField::ShortBreakDuration => (
            "  Short break duration",
            crate::config::format_duration(config.timer().short_break_duration()),
        ),
        SettingField::LongBreakDuration => (
            "  Long break duration",
            crate::config::format_duration(config.timer().long_break_duration()),
        ),
        SettingField::LongBreakInterval => (
            "  Long break interval",
            config.timer().long_break_interval().to_string(),
        ),
        SettingField::AutostartBreaks => (
            "  Autostart breaks",
            on_off(config.timer().autostart_breaks()).to_string(),
        ),
        SettingField::AutostartFocus => (
            "  Autostart Focus",
            on_off(config.timer().autostart_focus()).to_string(),
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
        ThemeRole::Focus => "  Focus session",
        ThemeRole::ShortBreak => "  Short break session",
        ThemeRole::LongBreak => "  Long break session",
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
        KeyAction::MoveTaskUp => "  Move task up",
        KeyAction::MoveTaskDown => "  Move task down",
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

    use ratatui::{Terminal, backend::TestBackend};

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

    fn app_layout(area: Rect, app: &App) -> FrameGeometry {
        let help = wrap_help(&controls_text(app, app.input_keys()), inner_width(area));
        resolve(LayoutRequest {
            area,
            controls_height: text_height(&help),
            focus: app.ui_focus(),
            last_task_focus: app.last_task_focus(),
            duration: app.timer().remaining(),
        })
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
        assert!(help.contains("[u/d] move list item"));
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
    fn responsive_layout_selects_each_space_class() {
        let app = App::new();
        for (area, expected) in [
            (Rect::new(0, 0, 80, 24), WorkspaceMode::Full),
            (Rect::new(0, 0, 80, 10), WorkspaceMode::Short),
            (Rect::new(0, 0, 40, 24), WorkspaceMode::Narrow),
            (Rect::new(0, 0, 40, 10), WorkspaceMode::Compact),
            (Rect::new(0, 0, 20, 9), WorkspaceMode::Compact),
        ] {
            assert_eq!(app_layout(area, &app).mode(), expected, "area: {area:?}");
        }
    }

    #[test]
    fn responsive_mode_is_stable_when_focus_changes_the_help_height() {
        for (area, expected) in [
            (Rect::new(0, 0, 80, 24), WorkspaceMode::Full),
            (Rect::new(0, 0, 80, 10), WorkspaceMode::Short),
            (Rect::new(0, 0, 40, 24), WorkspaceMode::Narrow),
            (Rect::new(0, 0, 40, 10), WorkspaceMode::Compact),
            (Rect::new(0, 0, 20, 9), WorkspaceMode::Compact),
        ] {
            let mut app = App::new();
            assert_eq!(app_layout(area, &app).mode(), expected);

            let _ = app.dispatch(Action::NavigateFocus(Direction::Down));
            assert_eq!(app_layout(area, &app).mode(), expected);

            let _ = app.dispatch(Action::NavigateFocus(Direction::Right));
            assert_eq!(app_layout(area, &app).mode(), expected);

            let _ = app.dispatch(Action::NavigateFocus(Direction::Up));
            assert_eq!(app_layout(area, &app).mode(), expected);
        }
    }

    #[test]
    fn short_layout_switches_between_clock_and_the_task_split() {
        let area = Rect::new(0, 0, 80, 10);
        let mut app = App::new();

        let clock = app_layout(area, &app);
        assert_eq!(clock.mode(), WorkspaceMode::Short);
        assert!(clock.clock().is_some());
        assert!(clock.todo().is_none());
        assert!(clock.done().is_none());

        let _ = app.dispatch(Action::NavigateFocus(Direction::Down));
        let tasks = app_layout(area, &app);
        assert_eq!(tasks.mode(), WorkspaceMode::Short);
        assert!(tasks.clock().is_none());
        assert!(tasks.todo().is_some());
        assert!(tasks.done().is_some());
    }

    #[test]
    fn hit_testing_uses_the_layout_of_the_last_rendered_frame() {
        let area = Rect::new(0, 0, 80, 10);
        let mut app = App::new();
        let rendered = app_layout(area, &app);
        let clock = rendered.clock().unwrap().area;

        let _ = app.dispatch(Action::NavigateFocus(Direction::Down));
        assert!(app_layout(area, &app).clock().is_none());
        assert_eq!(
            click_target(&rendered, (clock.x + 1, clock.y + 1), &app),
            ClickTarget::Clock
        );
    }

    #[test]
    fn narrow_layout_retains_the_last_task_panel_while_clock_is_focused() {
        let area = Rect::new(0, 0, 40, 24);
        let mut app = App::new();
        let _ = app.dispatch(Action::NavigateFocus(Direction::Down));
        let _ = app.dispatch(Action::NavigateFocus(Direction::Right));

        let done = app_layout(area, &app);
        assert_eq!(done.mode(), WorkspaceMode::Narrow);
        assert!(done.todo().is_none());
        assert!(done.done().is_some());

        let _ = app.dispatch(Action::NavigateFocus(Direction::Up));
        let clock = app_layout(area, &app);
        assert!(clock.clock().is_some());
        assert!(clock.todo().is_none());
        assert!(clock.done().is_some());

        let _ = app.dispatch(Action::NavigateFocus(Direction::Down));
        let todo = app_layout(area, &app);
        assert_eq!(app.ui_focus(), UiFocus::Todo);
        assert!(todo.todo().is_some());
        assert!(todo.done().is_none());
    }

    #[test]
    fn compact_layout_shows_only_the_focused_panel() {
        let area = Rect::new(0, 0, 40, 10);
        let mut app = App::new();

        let clock = app_layout(area, &app);
        assert_eq!(clock.mode(), WorkspaceMode::Compact);
        assert!(clock.clock().is_some());
        assert!(clock.todo().is_none());

        let _ = app.dispatch(Action::NavigateFocus(Direction::Down));
        let todo = app_layout(area, &app);
        assert!(todo.clock().is_none());
        assert!(todo.todo().is_some());
        assert!(todo.done().is_none());

        let _ = app.dispatch(Action::NavigateFocus(Direction::Right));
        let done = app_layout(area, &app);
        assert!(done.todo().is_none());
        assert!(done.done().is_some());
    }

    #[test]
    fn smallest_compact_clock_exposes_only_its_boxed_hit_target() {
        let app = App::new();
        let area = Rect::new(0, 0, 20, 9);
        let layout = app_layout(area, &app);

        assert_eq!(layout.mode(), WorkspaceMode::Compact);
        assert!(layout.clock().is_some());
        assert_eq!(click_target(&layout, (10, 5), &app), ClickTarget::Clock);
        assert_eq!(scroll_target(&layout, (10, 5), &app), None);
    }

    #[test]
    fn short_text_clock_shows_complete_help_when_it_fits() {
        let mut app = App::new();
        let area = Rect::new(0, 0, 80, 10);
        let help = wrap_help(&controls_text(&app, app.input_keys()), inner_width(area));
        let layout = app_layout(area, &app);

        assert_eq!(layout.mode(), WorkspaceMode::Short);
        assert_eq!(layout.controls().height, text_height(&help));
        assert!(layout.controls().height > 0);
        assert!(layout.clock().unwrap().remaining.height > 0);

        let theme = Theme::from(&ThemeConfig::default());
        let keys = KeysConfig::default();
        let mut terminal = Terminal::new(TestBackend::new(area.width, area.height)).unwrap();
        terminal
            .draw(|frame| {
                draw(frame, &mut app, theme, &keys);
            })
            .unwrap();
        let rendered = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(rendered.contains("box nav"));
    }

    #[test]
    fn one_row_compact_clock_omits_help_instead_of_displacing_the_timer() {
        let app = App::new();
        let area = Rect::new(0, 0, 30, 5);
        let layout = app_layout(area, &app);

        assert_eq!(layout.mode(), WorkspaceMode::Compact);
        assert_eq!(layout.controls().height, 0);
        assert_eq!(layout.clock().unwrap().remaining.height, 1);
    }

    #[test]
    fn compact_text_clock_omits_help_instead_of_cutting_it_down() {
        let app = App::new();
        let area = Rect::new(0, 0, 20, 10);
        let help = wrap_help(&controls_text(&app, app.input_keys()), inner_width(area));
        let layout = app_layout(area, &app);

        assert_eq!(layout.mode(), WorkspaceMode::Compact);
        assert_eq!(layout.clock().unwrap().face, ClockFace::Text);
        assert!(text_height(&help) > 2);
        assert_eq!(layout.controls().height, 0);
    }

    #[test]
    fn smallest_compact_clock_renders_text_duration_instead_of_big_glyphs() {
        let mut app = App::new();
        let area = Rect::new(0, 0, 20, 9);
        let theme = Theme::from(&ThemeConfig::default());
        let keys = KeysConfig::default();
        let mut terminal = Terminal::new(TestBackend::new(area.width, area.height)).unwrap();

        terminal
            .draw(|frame| {
                draw(frame, &mut app, theme, &keys);
            })
            .unwrap();

        let rendered = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(rendered.contains("25:00"));
        assert!(rendered.contains("Clock"));
        assert!(!rendered.contains('█'));

        let clock = app_layout(area, &app).clock().unwrap();
        let buffer = terminal.backend().buffer();
        assert_eq!(buffer[(clock.area.x, clock.area.y)].symbol(), "┌");
        let duration_x = clock.remaining.x + (clock.remaining.width - 5) / 2;
        assert_eq!(buffer[(duration_x, clock.remaining.y)].fg, theme.focus);
    }

    #[test]
    fn full_clock_falls_back_to_text_without_removing_other_rows() {
        let app = App::new();
        let layout = app_layout(Rect::new(0, 0, 9, 10), &app);
        let clock = layout.clock().unwrap();

        assert_eq!(layout.mode(), WorkspaceMode::Compact);
        assert_eq!(clock.face, ClockFace::Text);
        assert_eq!(clock.state.height, 1);
        assert_eq!(clock.remaining.height, 1);
        assert_eq!(clock.completed_sessions.height, 1);
        assert!(clock.session_controls.iter().all(|area| area.height == 1));
    }

    #[test]
    fn compact_actions_are_available_only_for_the_rendered_panel() {
        let mut app = App::new();
        let _ = app.dispatch(Action::NavigateFocus(Direction::Down));
        let compact = app_layout(Rect::new(0, 0, 20, 9), &app);

        for action in [
            Action::BeginAdd,
            Action::EditSelected,
            Action::DeleteSelected,
            Action::PrimaryAction,
            Action::MoveSelection(Direction::Down),
            Action::MoveSelectedTask(Direction::Up),
        ] {
            assert!(action_target_visible(&compact, UiFocus::Todo, &action));
        }
        assert!(action_target_visible(
            &compact,
            UiFocus::Todo,
            &Action::NavigateFocus(Direction::Up)
        ));
        assert!(!action_target_visible(
            &compact,
            UiFocus::Clock,
            &Action::PrimaryAction
        ));
    }

    #[test]
    fn clock_content_is_centered_with_equal_internal_gaps() {
        let layout = clock_geometry(Rect::new(0, 0, 80, 18), Duration::from_secs(25 * 60));

        assert_eq!(layout.remaining.height, 5);
        assert_eq!(layout.remaining.y, layout.state.y + layout.state.height + 1);
        assert_eq!(
            layout.completed_sessions.y,
            layout.remaining.y + layout.remaining.height + 1
        );

        let top_padding = layout.state.y - 1;
        let bottom_padding = layout.session_controls[0].y
            - (layout.completed_sessions.y + layout.completed_sessions.height);
        assert_eq!(top_padding, bottom_padding);
    }

    #[test]
    fn compact_clock_removes_internal_gaps_before_squeezing_content() {
        let layout = clock_geometry(Rect::new(0, 0, 80, 10), Duration::from_secs(25 * 60));

        assert_eq!(layout.remaining.height, 5);
        assert_eq!(layout.remaining.y, layout.state.y + layout.state.height);
        assert_eq!(
            layout.completed_sessions.y,
            layout.remaining.y + layout.remaining.height
        );
        assert_eq!(layout.session_controls[0].height, 1);
        assert_eq!(
            layout.session_controls[0].y,
            layout.completed_sessions.y + layout.completed_sessions.height
        );
    }

    #[test]
    fn roomy_clock_scales_glyphs_to_available_width_and_height() {
        let layout = clock_geometry(Rect::new(0, 0, 80, 19), Duration::from_secs(25 * 60));

        assert_eq!(layout.face, ClockFace::Glyphs { scale: 2 });
        assert_eq!(layout.remaining.height, 10);
    }

    #[test]
    fn clock_scaling_accounts_for_additional_minute_glyphs() {
        let layout = clock_geometry(Rect::new(0, 0, 80, 19), Duration::from_secs(9999 * 60 + 59));

        assert_eq!(layout.face, ClockFace::Glyphs { scale: 1 });
        assert_eq!(layout.remaining.height, 5);
    }

    #[test]
    fn clock_does_not_scale_when_only_one_dimension_has_room() {
        let duration = Duration::from_secs(25 * 60);
        let wide_but_short = clock_geometry(Rect::new(0, 0, 100, 12), duration);
        let tall_but_narrow = clock_geometry(Rect::new(0, 0, 50, 24), duration);

        assert_eq!(wide_but_short.face, ClockFace::Glyphs { scale: 1 });
        assert_eq!(tall_but_narrow.face, ClockFace::Glyphs { scale: 1 });
    }

    #[test]
    fn completed_focus_count_uses_normal_terminal_text_color() {
        let mut app = App::new();
        let keys = KeysConfig::default();
        let theme = Theme::from(&ThemeConfig::default());
        let area = Rect::new(0, 0, 80, 24);
        let completed_area = app_layout(area, &app).clock().unwrap().completed_sessions;
        let text = "Focus sessions completed: 0";
        let text_x = completed_area.x + (completed_area.width - text.len() as u16) / 2;
        let mut terminal = Terminal::new(TestBackend::new(area.width, area.height)).unwrap();

        terminal
            .draw(|frame| {
                draw(frame, &mut app, theme, &keys);
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        for x in text_x..text_x + text.len() as u16 {
            assert_eq!(buffer[(x, completed_area.y)].fg, Color::Reset);
        }
    }

    #[test]
    fn configured_colors_map_to_their_semantic_theme_roles() {
        let config = ThemeConfig::new(
            ThemeColor::LightBlue,
            ThemeColor::Black,
            ThemeColor::LightYellow,
            ThemeColor::LightGreen,
        )
        .with_color(ThemeRole::Focus, ThemeColor::Red)
        .with_color(ThemeRole::ShortBreak, ThemeColor::Blue)
        .with_color(ThemeRole::LongBreak, ThemeColor::Magenta);

        let theme = Theme::from(&config);

        assert_eq!(theme.focused_border, Color::LightBlue);
        assert_eq!(theme.unfocused_border, Color::Black);
        assert_eq!(theme.todo_highlight, Color::LightYellow);
        assert_eq!(theme.done_highlight, Color::LightGreen);
        assert_eq!(theme.session(SessionKind::Focus), Color::Red);
        assert_eq!(theme.session(SessionKind::ShortBreak), Color::Blue);
        assert_eq!(theme.session(SessionKind::LongBreak), Color::Magenta);
    }

    #[test]
    fn only_the_current_session_button_uses_its_session_color() {
        let config = ThemeConfig::default().with_color(ThemeRole::Focus, ThemeColor::Red);
        let theme = Theme::from(&config);

        let current = session_button_style(SessionKind::Focus, SessionKind::Focus, theme);
        let inactive = session_button_style(SessionKind::ShortBreak, SessionKind::Focus, theme);

        assert_eq!(current.fg, Some(Color::Red));
        assert!(current.add_modifier.contains(Modifier::REVERSED));
        assert_eq!(inactive, Style::default());
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
    fn task_help_uses_configured_item_movement_keys() {
        let mut app = App::new();
        let _ = app.dispatch(Action::NavigateFocus(Direction::Down));
        let keys: KeysConfig =
            toml::from_str("move_task_up = \"w\"\nmove_task_down = \"z\"\n").unwrap();

        let help = controls_text(&app, &keys);

        assert!(help.contains("[w/z] move list item"));
        assert!(!help.contains("[u/d] move list item"));
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
        let layout = app_layout(area, &app);
        let clock = layout.clock().unwrap().area;
        let todo = layout.todo().unwrap();
        let done = layout.done().unwrap();

        assert_eq!(
            click_target(&layout, (clock.x, clock.y), &app),
            ClickTarget::Clock
        );
        assert_eq!(
            click_target(&layout, (todo.x + 1, todo.y + 2), &app),
            ClickTarget::TodoTask(1)
        );
        assert_eq!(
            click_target(&layout, (todo.x, todo.y), &app),
            ClickTarget::Todo
        );
        let _ = app.dispatch(Action::NavigateFocus(Direction::Down));
        let _ = app.dispatch(Action::PrimaryAction);
        assert_eq!(
            click_target(&layout, (done.x + 1, done.y + 1), &app),
            ClickTarget::DoneTask(0)
        );
        assert_eq!(click_target(&layout, (0, 0), &app), ClickTarget::Outside);
    }

    #[test]
    fn click_translation_uses_list_scroll_offsets() {
        let mut app = App::new();
        for index in 0..8 {
            add_task(&mut app, &format!("Task {index}"));
        }
        app.set_offsets(4, 0);
        let area = Rect::new(0, 0, 80, 24);
        let layout = app_layout(area, &app);
        let todo = layout.todo().unwrap();

        assert_eq!(
            click_target(&layout, (todo.x + 1, todo.y + 1), &app),
            ClickTarget::TodoTask(4)
        );
    }

    #[test]
    fn scroll_hit_testing_uses_task_boxes_and_settings_list() {
        let mut app = App::new();
        add_task(&mut app, "First");
        let area = Rect::new(0, 0, 80, 24);
        let layout = app_layout(area, &app);
        let clock = layout.clock().unwrap().area;
        let todo = layout.todo().unwrap();
        let done = layout.done().unwrap();

        assert_eq!(
            scroll_target(&layout, (todo.x, todo.y), &app),
            Some(ScrollTarget::Todo)
        );
        assert_eq!(
            scroll_target(&layout, (done.x, done.y), &app),
            Some(ScrollTarget::Done)
        );
        assert_eq!(scroll_target(&layout, (clock.x, clock.y), &app), None);

        let _ = app.dispatch(Action::OpenSettings);
        let settings = app.settings().unwrap();
        let footer = settings_footer(settings);
        let (list, footer_area) = settings_parts(area, &footer);
        assert_eq!(
            scroll_target(&layout, (list.x, list.y), &app),
            Some(ScrollTarget::Settings)
        );
        assert_eq!(
            scroll_target(&layout, (footer_area.x, footer_area.y), &app),
            None
        );
    }

    #[test]
    fn click_translation_maps_all_visible_session_controls() {
        let app = App::new();
        let area = Rect::new(0, 0, 80, 24);
        let layout = app_layout(area, &app);
        let controls = layout.clock().unwrap().session_controls;

        for (control, session) in controls.into_iter().zip([
            SessionKind::Focus,
            SessionKind::ShortBreak,
            SessionKind::LongBreak,
        ]) {
            assert_eq!(
                click_target(&layout, (control.x, control.y), &app),
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
        let selection = SettingField::ALL.len() - 1;
        let selected_row = settings_visual_row(selection);
        let first_visible = settings_offset(selected_row, usize::from(list.height));
        let row = (first_visible..)
            .find(|row| settings_field_row(*row).is_some())
            .unwrap();
        let expected = settings_field_row(row).unwrap();
        app.set_settings_offset(first_visible);
        let layout = app_layout(area, &app);

        assert_eq!(
            click_target(
                &layout,
                (list.x, list.y + u16::try_from(row - first_visible).unwrap()),
                &app
            ),
            ClickTarget::SettingsRow(expected)
        );
        assert_eq!(click_target(&layout, (0, 0), &app), ClickTarget::Outside);
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
        let layout = app_layout(area, &app);
        assert_eq!(
            click_target(&layout, (list.x, list.y + 2), &app),
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
        let notification = SettingField::ALL
            .iter()
            .position(|field| *field == SettingField::NotificationEnabled)
            .unwrap();
        let completion_sound = SettingField::ALL
            .iter()
            .position(|field| *field == SettingField::CompletionSoundEnabled)
            .unwrap();
        assert_eq!(
            settings_scroll_anchor(notification),
            settings_visual_row(notification) - 1
        );
        assert_eq!(
            settings_scroll_anchor(completion_sound),
            settings_visual_row(completion_sound) - 1
        );
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
        assert!(!setting_row(SettingField::FocusDuration, settings).contains("Timer /"));
        assert!(!setting_row(SettingField::PersistTasks, settings).contains("Tasks /"));
        assert!(!setting_row(SettingField::Key(KeyAction::FocusLeft), settings).contains("Keys /"));

        for (group_index, (_, fields)) in SettingField::GROUPS.iter().enumerate() {
            let first_field = settings_group_start(group_index);
            let heading_row = first_field + group_index;
            assert_eq!(settings_field_row(heading_row), None);
            assert_eq!(settings_field_row(heading_row + 1), Some(first_field));
            assert_eq!(SettingField::ALL[first_field], fields[0]);
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
