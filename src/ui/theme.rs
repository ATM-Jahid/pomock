use ratatui::{
    style::{Color, Modifier, Style},
    widgets::{Block, Borders},
};

use crate::{
    config::{ThemeColor, ThemeConfig},
    timer::SessionKind,
};

#[derive(Debug, Clone, Copy)]
pub struct Theme {
    pub(super) focused_border: Color,
    pub(super) unfocused_border: Color,
    pub(super) todo_highlight: Color,
    pub(super) done_highlight: Color,
    pub(super) focus: Color,
    pub(super) short_break: Color,
    pub(super) long_break: Color,
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
    pub(super) fn session(self, session: SessionKind) -> Color {
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

pub(super) fn session_button_style(
    session: SessionKind,
    current: SessionKind,
    theme: Theme,
) -> Style {
    if session == current {
        Style::default()
            .fg(theme.session(session))
            .add_modifier(Modifier::REVERSED)
    } else {
        Style::default()
    }
}

pub(super) fn focused_block(title: &str, focused: bool, theme: Theme) -> Block<'_> {
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
