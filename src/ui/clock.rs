use std::time::Duration;

use ratatui::{
    Frame,
    style::{Modifier, Style},
    widgets::Paragraph,
};

use crate::{
    app::App,
    timer::{SessionKind, TimerState},
};

use super::{
    Theme,
    layout::{ClockFace, ClockGeometry},
    theme::session_button_style,
};
use crate::display::{format_big_duration_at_scale, format_duration};

pub(super) fn draw_clock(
    frame: &mut Frame,
    clock: ClockGeometry,
    app: &App,
    theme: Theme,
    state_text: &str,
    remaining_duration: Duration,
) {
    let current = current_session(app.timer().state());
    let state = Paragraph::new(clock_status_text(
        state_text,
        app.timer().state(),
        clock.state.width,
    ))
    .alignment(ratatui::layout::Alignment::Center);
    let remaining_text = match clock.face {
        ClockFace::Text => format_duration(remaining_duration),
        ClockFace::Glyphs { scale } => format_big_duration_at_scale(remaining_duration, scale),
    };
    let remaining = Paragraph::new(remaining_text)
        .alignment(ratatui::layout::Alignment::Center)
        .style(
            Style::default()
                .fg(theme.session(current))
                .add_modifier(Modifier::BOLD),
        );
    frame.render_widget(state, clock.state);
    frame.render_widget(remaining, clock.remaining);
    frame.render_widget(
        Paragraph::new(clock_completed_text(
            app.timer().completed_focus_sessions(),
            clock.completed_sessions.width,
        ))
        .alignment(ratatui::layout::Alignment::Center),
        clock.completed_sessions,
    );

    for ((session, label), area) in [
        (SessionKind::Focus, "Focus"),
        (SessionKind::ShortBreak, "Short break"),
        (SessionKind::LongBreak, "Long break"),
    ]
    .into_iter()
    .zip(clock.session_controls)
    {
        frame.render_widget(
            Paragraph::new(session_control_label(session, label, area.width))
                .alignment(ratatui::layout::Alignment::Center)
                .style(session_button_style(session, current, theme)),
            area,
        );
    }
}

pub(super) fn current_session(state: TimerState) -> SessionKind {
    match state {
        TimerState::Ready(session) | TimerState::Running(session) | TimerState::Paused(session) => {
            session
        }
    }
}

pub(super) fn clock_status_text(full: &str, state: TimerState, width: u16) -> String {
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

pub(super) fn clock_completed_text(completed: u32, width: u16) -> String {
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

pub(super) fn session_control_label(session: SessionKind, full: &str, width: u16) -> String {
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

pub(super) fn session_label(session: SessionKind) -> &'static str {
    match session {
        SessionKind::Focus => "Focus",
        SessionKind::ShortBreak => "Short break",
        SessionKind::LongBreak => "Long break",
    }
}
