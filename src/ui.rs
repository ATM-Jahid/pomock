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
    ui_layout::{C_H_SUG, ClockFace, FooterHeights, LayoutRequest, resolve},
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
    let footer_text = footer_text(app, keys);
    let workspace_width = inner_width(area);
    let footer = stable_footer_metrics(keys, workspace_width);
    let footer_text = wrap_help(&footer_text, workspace_width);
    let layout = resolve(LayoutRequest {
        area,
        footer_heights: footer.heights,
        footer_cutoff: footer.cutoff,
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

    let footer = Paragraph::new(footer_text).alignment(Alignment::Center);

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
    if layout.footer().width > 0 && layout.footer().height > 0 {
        frame.render_widget(footer, layout.footer());
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FooterMetrics {
    heights: FooterHeights,
    item_width: u16,
    height_width: u16,
    cutoff: u16,
}

fn stable_footer_metrics(keys: &KeysConfig, width: u16) -> FooterMetrics {
    let texts = [
        normal_help_text(keys, UiFocus::Clock),
        normal_help_text(keys, UiFocus::Todo),
        normal_help_text(keys, UiFocus::Done),
    ];

    let heights_at = |candidate_width| FooterHeights {
        clock: viable_help_height(&texts[0], candidate_width),
        todo: viable_help_height(&texts[1], candidate_width),
        done: viable_help_height(&texts[2], candidate_width),
    };

    let item_width = texts
        .iter()
        .map(|text| help_item_width(text))
        .max()
        .unwrap_or(0);

    let height_width = (item_width.max(1)..=u16::MAX)
        .find(|&candidate_width| {
            heights_at(candidate_width)
                .reserve()
                .is_some_and(|height| height <= C_H_SUG)
        })
        .unwrap_or(u16::MAX);

    FooterMetrics {
        heights: heights_at(width),
        item_width,
        height_width,
        cutoff: item_width.max(height_width),
    }
}

fn help_items(text: &str) -> impl Iterator<Item = &str> {
    text.lines()
        .flat_map(|line| line.split("  "))
        .filter(|item| !item.is_empty())
}

fn help_item_width(text: &str) -> u16 {
    help_items(text)
        .map(|item| u16::try_from(Line::from(item).width()).unwrap_or(u16::MAX))
        .max()
        .unwrap_or(0)
}

fn viable_help_height(text: &str, width: u16) -> Option<u16> {
    if width == 0 || help_item_width(text) > width {
        return None;
    }

    Some(text_height(&wrap_help(text, width)))
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
        format!("{}. {description}", index + 1)
    } else {
        description.to_string()
    }
}

fn footer_text(app: &App, keys: &KeysConfig) -> String {
    footer_text_for_focus(app, keys, app.ui_focus())
}

fn footer_text_for_focus(app: &App, keys: &KeysConfig, focus: UiFocus) -> String {
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
        EditMode::Normal => normal_help_text(keys, focus),
    }
}

fn normal_help_text(keys: &KeysConfig, focus: UiFocus) -> String {
    let focus_navigation = key_labels(&[
        first_key(keys.focus_left()),
        first_key(keys.focus_down()),
        first_key(keys.focus_up()),
        first_key(keys.focus_right()),
    ]);
    let list_navigation = key_labels(&[first_key(keys.list_down()), first_key(keys.list_up())]);
    let item_movement = key_labels(&[
        first_key(keys.move_task_up()),
        first_key(keys.move_task_down()),
    ]);
    let quit = format_key(first_key(keys.quit()));
    let settings = format_key(first_key(keys.settings()));
    match focus {
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
#[path = "ui/tests.rs"]
mod tests;
