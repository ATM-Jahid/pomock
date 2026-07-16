use serde::{Deserialize, Serialize};

/// A portable named terminal color accepted by the shared configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThemeColor {
    Black,
    Red,
    Green,
    Yellow,
    Blue,
    Magenta,
    Cyan,
    Gray,
    DarkGray,
    LightRed,
    LightGreen,
    LightYellow,
    LightBlue,
    LightMagenta,
    LightCyan,
    White,
}

/// Durable colors assigned to semantic presentation roles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ThemeConfig {
    focused_border: ThemeColor,
    unfocused_border: ThemeColor,
    todo_highlight: ThemeColor,
    done_highlight: ThemeColor,
    completed_sessions: ThemeColor,
}

impl ThemeConfig {
    pub fn new(
        focused_border: ThemeColor,
        unfocused_border: ThemeColor,
        todo_highlight: ThemeColor,
        done_highlight: ThemeColor,
        completed_sessions: ThemeColor,
    ) -> Self {
        Self {
            focused_border,
            unfocused_border,
            todo_highlight,
            done_highlight,
            completed_sessions,
        }
    }

    pub fn focused_border(&self) -> ThemeColor {
        self.focused_border
    }

    pub fn unfocused_border(&self) -> ThemeColor {
        self.unfocused_border
    }

    pub fn todo_highlight(&self) -> ThemeColor {
        self.todo_highlight
    }

    pub fn done_highlight(&self) -> ThemeColor {
        self.done_highlight
    }

    pub fn completed_sessions(&self) -> ThemeColor {
        self.completed_sessions
    }

    pub fn with_color(mut self, role: ThemeRole, color: ThemeColor) -> Self {
        match role {
            ThemeRole::FocusedBorder => self.focused_border = color,
            ThemeRole::UnfocusedBorder => self.unfocused_border = color,
            ThemeRole::TodoHighlight => self.todo_highlight = color,
            ThemeRole::DoneHighlight => self.done_highlight = color,
            ThemeRole::CompletedSessions => self.completed_sessions = color,
        }
        self
    }

    pub fn color(self, role: ThemeRole) -> ThemeColor {
        match role {
            ThemeRole::FocusedBorder => self.focused_border,
            ThemeRole::UnfocusedBorder => self.unfocused_border,
            ThemeRole::TodoHighlight => self.todo_highlight,
            ThemeRole::DoneHighlight => self.done_highlight,
            ThemeRole::CompletedSessions => self.completed_sessions,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeRole {
    FocusedBorder,
    UnfocusedBorder,
    TodoHighlight,
    DoneHighlight,
    CompletedSessions,
}

impl ThemeColor {
    pub const ALL: [Self; 16] = [
        Self::Black,
        Self::Red,
        Self::Green,
        Self::Yellow,
        Self::Blue,
        Self::Magenta,
        Self::Cyan,
        Self::Gray,
        Self::DarkGray,
        Self::LightRed,
        Self::LightGreen,
        Self::LightYellow,
        Self::LightBlue,
        Self::LightMagenta,
        Self::LightCyan,
        Self::White,
    ];

    pub fn cycle(self, forward: bool) -> Self {
        let index = Self::ALL
            .iter()
            .position(|color| *color == self)
            .unwrap_or(0);
        let next = if forward {
            (index + 1) % Self::ALL.len()
        } else {
            (index + Self::ALL.len() - 1) % Self::ALL.len()
        };
        Self::ALL[next]
    }
}

impl Default for ThemeConfig {
    fn default() -> Self {
        Self::new(
            ThemeColor::Yellow,
            ThemeColor::DarkGray,
            ThemeColor::Yellow,
            ThemeColor::Green,
            ThemeColor::Green,
        )
    }
}
