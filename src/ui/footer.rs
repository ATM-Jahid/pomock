use ratatui::text::Line;

use crate::{
    app::{App, ConfirmationOperation, EditMode, TimerChange, UiFocus},
    config::{ConfigKey, KeysConfig},
    display::format_key,
};

use super::{
    clock::session_label,
    layout::{C_H_SUG, FooterHeights},
};

pub(super) fn text_height(text: &str) -> u16 {
    u16::try_from(text.lines().count()).unwrap_or(u16::MAX)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct FooterMetrics {
    pub(super) heights: FooterHeights,
    pub(super) item_width: u16,
    pub(super) height_width: u16,
    pub(super) cutoff: u16,
}

pub(super) fn stable_footer_metrics(keys: &KeysConfig, width: u16) -> FooterMetrics {
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

pub(super) fn help_item_width(text: &str) -> u16 {
    help_items(text)
        .map(|item| u16::try_from(Line::from(item).width()).unwrap_or(u16::MAX))
        .max()
        .unwrap_or(0)
}

pub(super) fn viable_help_height(text: &str, width: u16) -> Option<u16> {
    if width == 0 || help_item_width(text) > width {
        return None;
    }

    Some(text_height(&wrap_help(text, width)))
}

pub(super) fn wrap_help(text: &str, width: u16) -> String {
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

pub(super) fn footer_text(app: &App, keys: &KeysConfig) -> String {
    footer_text_for_focus(app, keys, app.ui_focus())
}

pub(super) fn footer_text_for_focus(app: &App, keys: &KeysConfig, focus: UiFocus) -> String {
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

pub(super) fn first_key(keys: &[ConfigKey]) -> ConfigKey {
    keys[0]
}

fn key_labels(keys: &[ConfigKey]) -> String {
    keys.iter()
        .map(|key| format_key(*key))
        .collect::<Vec<_>>()
        .join("/")
}

pub(super) fn confirmation_prompt(operation: ConfirmationOperation) -> String {
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
