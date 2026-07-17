use std::time::Duration;

use crate::{
    config::ConfigKey,
    timer::{SessionKind, TimerState},
};

const BIG_GLYPH_HEIGHT: usize = 5;
const BIG_ON: &str = "██";
const BIG_OFF: &str = "  ";

pub fn format_duration(duration: Duration) -> String {
    let total_seconds = duration.as_secs();
    let minutes = total_seconds / 60;
    let seconds = total_seconds % 60;

    format!("{minutes:02}:{seconds:02}")
}

pub fn format_big_duration(duration: Duration) -> String {
    render_big_text(&format_duration(duration))
}

pub fn format_state(state: TimerState) -> &'static str {
    match state {
        TimerState::Ready(SessionKind::Focus) => "Focus ready",
        TimerState::Ready(SessionKind::ShortBreak) => "Short break ready",
        TimerState::Ready(SessionKind::LongBreak) => "Long break ready",
        TimerState::Running(SessionKind::Focus) => "Focus",
        TimerState::Running(SessionKind::ShortBreak) => "Short break",
        TimerState::Running(SessionKind::LongBreak) => "Long break",
        TimerState::Paused(SessionKind::Focus) => "Focus paused",
        TimerState::Paused(SessionKind::ShortBreak) => "Short break paused",
        TimerState::Paused(SessionKind::LongBreak) => "Long break paused",
    }
}

pub fn format_key(key: ConfigKey) -> String {
    match key {
        ConfigKey::Character(character) => character.to_string(),
        ConfigKey::Space => "space".to_string(),
        ConfigKey::Enter => "Enter".to_string(),
        ConfigKey::Escape => "Esc".to_string(),
        ConfigKey::Backspace => "Backspace".to_string(),
        ConfigKey::Up => "↑".to_string(),
        ConfigKey::Down => "↓".to_string(),
        ConfigKey::Left => "←".to_string(),
        ConfigKey::Right => "→".to_string(),
    }
}

fn render_big_text(text: &str) -> String {
    let glyphs: Vec<[String; BIG_GLYPH_HEIGHT]> = text.chars().map(glyph).collect();
    let mut rows = Vec::with_capacity(BIG_GLYPH_HEIGHT);

    for row in 0..BIG_GLYPH_HEIGHT {
        let line = glyphs
            .iter()
            .map(|glyph| glyph[row].as_str())
            .collect::<Vec<_>>()
            .join(" ");

        rows.push(line);
    }

    rows.join("\n")
}

fn glyph(character: char) -> [String; BIG_GLYPH_HEIGHT] {
    match character {
        '0' => big_glyph(["111", "101", "101", "101", "111"]),
        '1' => big_glyph(["001", "001", "001", "001", "001"]),
        '2' => big_glyph(["111", "001", "111", "100", "111"]),
        '3' => big_glyph(["111", "001", "111", "001", "111"]),
        '4' => big_glyph(["101", "101", "111", "001", "001"]),
        '5' => big_glyph(["111", "100", "111", "001", "111"]),
        '6' => big_glyph(["111", "100", "111", "101", "111"]),
        '7' => big_glyph(["111", "001", "001", "001", "001"]),
        '8' => big_glyph(["111", "101", "111", "101", "111"]),
        '9' => big_glyph(["111", "101", "111", "001", "111"]),
        ':' => big_glyph(["0", "1", "0", "1", "0"]),
        _ => big_glyph(["000", "000", "000", "000", "000"]),
    }
}

fn big_glyph(pattern: [&str; BIG_GLYPH_HEIGHT]) -> [String; BIG_GLYPH_HEIGHT] {
    pattern.map(|row| {
        row.chars()
            .map(|cell| if cell == '1' { BIG_ON } else { BIG_OFF })
            .collect()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn formats_zero_seconds() {
        assert_eq!(format_duration(Duration::ZERO), "00:00");
    }

    #[test]
    fn formats_seconds_with_leading_zero() {
        assert_eq!(format_duration(Duration::from_secs(9)), "00:09");
    }

    #[test]
    fn formats_minutes_and_seconds() {
        assert_eq!(format_duration(Duration::from_secs(65)), "01:05");
    }

    #[test]
    fn formats_big_duration_as_five_lines() {
        let output = format_big_duration(Duration::ZERO);

        assert_eq!(output.lines().count(), 5);
    }

    #[test]
    fn formats_big_duration_with_equal_width_lines() {
        let output = format_big_duration(Duration::from_secs(65));
        let mut lines = output.lines();
        let first_line_width = lines.next().unwrap().chars().count();

        assert!(lines.all(|line| line.chars().count() == first_line_width));
        assert_eq!(first_line_width, 30);
    }

    #[test]
    fn formats_big_duration_for_minutes_and_seconds() {
        assert_eq!(
            format_big_duration(Duration::from_secs(65)),
            "██████     ██    ██████ ██████\n\
             ██  ██     ██ ██ ██  ██ ██    \n\
             ██  ██     ██    ██  ██ ██████\n\
             ██  ██     ██ ██ ██  ██     ██\n\
             ██████     ██    ██████ ██████"
        );
    }

    #[test]
    fn formats_timer_state_labels() {
        assert_eq!(
            format_state(TimerState::Ready(SessionKind::Focus)),
            "Focus ready"
        );
        assert_eq!(
            format_state(TimerState::Ready(SessionKind::ShortBreak)),
            "Short break ready"
        );
        assert_eq!(
            format_state(TimerState::Ready(SessionKind::LongBreak)),
            "Long break ready"
        );
        assert_eq!(
            format_state(TimerState::Running(SessionKind::Focus)),
            "Focus"
        );
        assert_eq!(
            format_state(TimerState::Paused(SessionKind::LongBreak)),
            "Long break paused"
        );
    }

    #[test]
    fn formats_configurable_key_labels_for_help_text() {
        assert_eq!(format_key(ConfigKey::Character('n')), "n");
        assert_eq!(format_key(ConfigKey::Space), "space");
        assert_eq!(format_key(ConfigKey::Enter), "Enter");
        assert_eq!(format_key(ConfigKey::Escape), "Esc");
        assert_eq!(format_key(ConfigKey::Down), "↓");
    }
}
