use std::{fmt, str::FromStr};

use serde::{Deserialize, Deserializer, Serialize, Serializer, de};

/// A portable named terminal color or an exact RGB color.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
    Rgb(u8, u8, u8),
}

/// Durable colors assigned to semantic presentation roles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ThemeConfig {
    focused_border: ThemeColor,
    unfocused_border: ThemeColor,
    todo_highlight: ThemeColor,
    done_highlight: ThemeColor,
    focus: ThemeColor,
    short_break: ThemeColor,
    long_break: ThemeColor,
}

impl ThemeConfig {
    pub fn new(
        focused_border: ThemeColor,
        unfocused_border: ThemeColor,
        todo_highlight: ThemeColor,
        done_highlight: ThemeColor,
    ) -> Self {
        Self {
            focused_border,
            unfocused_border,
            todo_highlight,
            done_highlight,
            focus: ThemeColor::Magenta,
            short_break: ThemeColor::Cyan,
            long_break: ThemeColor::Green,
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

    pub fn focus(&self) -> ThemeColor {
        self.focus
    }

    pub fn short_break(&self) -> ThemeColor {
        self.short_break
    }

    pub fn long_break(&self) -> ThemeColor {
        self.long_break
    }

    pub fn with_color(mut self, role: ThemeRole, color: ThemeColor) -> Self {
        match role {
            ThemeRole::FocusedBorder => self.focused_border = color,
            ThemeRole::UnfocusedBorder => self.unfocused_border = color,
            ThemeRole::TodoHighlight => self.todo_highlight = color,
            ThemeRole::DoneHighlight => self.done_highlight = color,
            ThemeRole::Focus => self.focus = color,
            ThemeRole::ShortBreak => self.short_break = color,
            ThemeRole::LongBreak => self.long_break = color,
        }
        self
    }

    pub fn color(self, role: ThemeRole) -> ThemeColor {
        match role {
            ThemeRole::FocusedBorder => self.focused_border,
            ThemeRole::UnfocusedBorder => self.unfocused_border,
            ThemeRole::TodoHighlight => self.todo_highlight,
            ThemeRole::DoneHighlight => self.done_highlight,
            ThemeRole::Focus => self.focus,
            ThemeRole::ShortBreak => self.short_break,
            ThemeRole::LongBreak => self.long_break,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeRole {
    FocusedBorder,
    UnfocusedBorder,
    TodoHighlight,
    DoneHighlight,
    Focus,
    ShortBreak,
    LongBreak,
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
        let index = Self::ALL.iter().position(|color| *color == self);
        match (index, forward) {
            (Some(index), true) => Self::ALL[(index + 1) % Self::ALL.len()],
            (Some(index), false) => Self::ALL[(index + Self::ALL.len() - 1) % Self::ALL.len()],
            (None, true) => Self::ALL[0],
            (None, false) => Self::ALL[Self::ALL.len() - 1],
        }
    }

    fn name(self) -> Option<&'static str> {
        match self {
            Self::Black => Some("black"),
            Self::Red => Some("red"),
            Self::Green => Some("green"),
            Self::Yellow => Some("yellow"),
            Self::Blue => Some("blue"),
            Self::Magenta => Some("magenta"),
            Self::Cyan => Some("cyan"),
            Self::Gray => Some("gray"),
            Self::DarkGray => Some("dark_gray"),
            Self::LightRed => Some("light_red"),
            Self::LightGreen => Some("light_green"),
            Self::LightYellow => Some("light_yellow"),
            Self::LightBlue => Some("light_blue"),
            Self::LightMagenta => Some("light_magenta"),
            Self::LightCyan => Some("light_cyan"),
            Self::White => Some("white"),
            Self::Rgb(_, _, _) => None,
        }
    }
}

impl fmt::Display for ThemeColor {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(name) = self.name() {
            formatter.write_str(name)
        } else if let Self::Rgb(red, green, blue) = self {
            write!(formatter, "#{red:02x}{green:02x}{blue:02x}")
        } else {
            unreachable!("all named colors have a stored name")
        }
    }
}

impl FromStr for ThemeColor {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let named = Self::ALL
            .into_iter()
            .find(|color| color.name() == Some(value));
        if let Some(color) = named {
            return Ok(color);
        }

        if let Some(hex) = value.strip_prefix('#')
            && hex.len() == 6
            && let Ok(rgb) = u32::from_str_radix(hex, 16)
        {
            return Ok(Self::Rgb(
                ((rgb >> 16) & 0xff) as u8,
                ((rgb >> 8) & 0xff) as u8,
                (rgb & 0xff) as u8,
            ));
        }

        Err(format!(
            "color must be a supported preset name or #RRGGBB; found {value:?}"
        ))
    }
}

impl Serialize for ThemeColor {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for ThemeColor {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        String::deserialize(deserializer)?
            .parse()
            .map_err(de::Error::custom)
    }
}

impl Default for ThemeConfig {
    fn default() -> Self {
        Self::new(
            ThemeColor::LightRed,
            ThemeColor::DarkGray,
            ThemeColor::Red,
            ThemeColor::Green,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_session_colors_are_distinct_semantic_roles() {
        let theme = ThemeConfig::default();

        assert_eq!(theme.focus(), ThemeColor::LightRed);
        assert_eq!(theme.short_break(), ThemeColor::Cyan);
        assert_eq!(theme.long_break(), ThemeColor::Green);
    }

    #[test]
    fn named_and_hex_colors_round_trip_through_toml() {
        for color in [ThemeColor::LightCyan, ThemeColor::Rgb(0x5f, 0xd7, 0xff)] {
            let config = ThemeConfig::default().with_color(ThemeRole::FocusedBorder, color);
            let stored = toml::to_string(&config).unwrap();
            assert_eq!(
                toml::from_str::<ThemeConfig>(&stored)
                    .unwrap()
                    .focused_border(),
                color
            );
        }
    }

    #[test]
    fn session_colors_round_trip_through_toml() {
        let theme = ThemeConfig::default()
            .with_color(ThemeRole::Focus, ThemeColor::Magenta)
            .with_color(ThemeRole::ShortBreak, ThemeColor::LightBlue)
            .with_color(ThemeRole::LongBreak, ThemeColor::Rgb(1, 2, 3));

        let stored = toml::to_string(&theme).unwrap();
        let loaded: ThemeConfig = toml::from_str(&stored).unwrap();

        assert_eq!(loaded, theme);
    }

    #[test]
    fn hex_colors_accept_either_case_and_display_canonically() {
        assert_eq!("#5FD7fF".parse(), Ok(ThemeColor::Rgb(0x5f, 0xd7, 0xff)));
        assert_eq!(ThemeColor::Rgb(0x5f, 0xd7, 0xff).to_string(), "#5fd7ff");
    }

    #[test]
    fn cycling_from_custom_colors_enters_the_named_preset_ring() {
        let custom = ThemeColor::Rgb(1, 2, 3);
        assert_eq!(custom.cycle(true), ThemeColor::Black);
        assert_eq!(custom.cycle(false), ThemeColor::White);
    }
}
