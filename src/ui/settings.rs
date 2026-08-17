use ratatui::{
    Frame,
    layout::{Constraint, Direction as LayoutDirection, Layout, Rect},
    style::{Modifier, Style},
    widgets::{Block, Clear, List, ListItem, ListState, Paragraph},
};

use crate::{
    app::App,
    config::{KeyAction, ThemeRole},
    display::format_key,
    settings::SettingField,
};

use super::{
    Theme,
    footer::{first_key, text_height, wrap_help},
};

pub(super) fn settings_area(area: Rect) -> Rect {
    let width = area.width.saturating_sub(4).min(72);
    let height = area.height.saturating_sub(2).min(30);
    Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    )
}

pub(super) fn settings_parts(area: Rect, footer_text: &str) -> (Rect, Rect) {
    let inner = Block::default()
        .borders(ratatui::widgets::Borders::ALL)
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

pub(super) fn settings_group_start(group_index: usize) -> usize {
    SettingField::GROUPS[..group_index]
        .iter()
        .map(|(_, fields)| fields.len())
        .sum()
}

pub(super) fn settings_visual_row(selection: usize) -> usize {
    selection
        + SettingField::GROUPS
            .iter()
            .enumerate()
            .filter(|(group_index, _)| selection >= settings_group_start(*group_index))
            .count()
}

pub(super) fn settings_scroll_anchor(selection: usize) -> usize {
    SettingField::GROUPS
        .iter()
        .enumerate()
        .find_map(|(group_index, _)| {
            let first_field = settings_group_start(group_index);
            (first_field == selection).then_some(first_field + group_index)
        })
        .unwrap_or_else(|| settings_visual_row(selection))
}

pub(super) fn settings_field_row(visual_row: usize) -> Option<usize> {
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

pub(super) fn settings_row_at(
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

pub(super) fn draw_settings(frame: &mut Frame, app: &mut App, theme: Theme) {
    let settings = app.settings().expect("settings overlay is open");
    let area = settings_area(frame.area());
    let footer = settings_footer(settings);
    let (list_area, footer_area) = settings_parts(frame.area(), &footer);
    frame.render_widget(Clear, area);
    frame.render_widget(
        Block::default()
            .title("Settings")
            .borders(ratatui::widgets::Borders::ALL)
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
        Paragraph::new(footer).alignment(ratatui::layout::Alignment::Center),
        footer_area,
    );
}

pub(super) fn settings_footer(settings: &crate::settings::SettingsOverlay) -> String {
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
    } else if let Some(error) = settings.write_error() {
        error.to_string()
    } else {
        format!("[↑/↓ or j/k] select  [←/→ or h/l] change  [Enter/Space] edit  [{close}/Esc] close")
    }
}

pub(super) fn setting_row(
    field: SettingField,
    settings: &crate::settings::SettingsOverlay,
) -> String {
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

pub(super) fn theme_role_label(role: ThemeRole) -> &'static str {
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
