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
