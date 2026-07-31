use ratatui::layout::{Position, Rect};

use crate::{
    app::{Action, App, ClickTarget, ScrollTarget, UiFocus},
    timer::SessionKind,
};

use super::{
    FrameGeometry,
    settings::{settings_footer, settings_parts, settings_row_at},
    task_lists::task_row_at,
};

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

fn session_control_at(controls: [Rect; 3], point: Position) -> Option<SessionKind> {
    [
        SessionKind::Focus,
        SessionKind::ShortBreak,
        SessionKind::LongBreak,
    ]
    .into_iter()
    .zip(controls)
    .find_map(|(session, area)| area.contains(point).then_some(session))
}
