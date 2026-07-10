use std::time::Duration;

use crate::timer::{SessionKind, TimerState};

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
        TimerState::Ready(SessionKind::Break) => "Break ready",
        TimerState::Focus => "Focus",
        TimerState::Break => "Break",
        TimerState::Paused => "Paused",
        TimerState::Completed(_) => "Completed",
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
        '0' => big_glyph(["1111", "1001", "1001", "1001", "1111"]),
        '1' => big_glyph(["0010", "0010", "0010", "0010", "0010"]),
        '2' => big_glyph(["1111", "0001", "1111", "1000", "1111"]),
        '3' => big_glyph(["1111", "0001", "1111", "0001", "1111"]),
        '4' => big_glyph(["1001", "1001", "1111", "0001", "0001"]),
        '5' => big_glyph(["1111", "1000", "1111", "0001", "1111"]),
        '6' => big_glyph(["1111", "1000", "1111", "1001", "1111"]),
        '7' => big_glyph(["1111", "0001", "0001", "0001", "0001"]),
        '8' => big_glyph(["1111", "1001", "1111", "1001", "1111"]),
        '9' => big_glyph(["1111", "1001", "1111", "0001", "1111"]),
        ':' => big_glyph(["0000", "0110", "0000", "0110", "0000"]),
        _ => big_glyph(["0000", "0000", "0000", "0000", "0000"]),
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
    }

    #[test]
    fn formats_big_duration_for_minutes_and_seconds() {
        assert_eq!(
            format_big_duration(Duration::from_secs(65)),
            "████████     ██            ████████ ████████\n\
             ██    ██     ██     ████   ██    ██ ██      \n\
             ██    ██     ██            ██    ██ ████████\n\
             ██    ██     ██     ████   ██    ██       ██\n\
             ████████     ██            ████████ ████████"
        );
    }

    #[test]
    fn formats_timer_state_labels() {
        assert_eq!(
            format_state(TimerState::Ready(SessionKind::Focus)),
            "Focus ready"
        );
        assert_eq!(
            format_state(TimerState::Ready(SessionKind::Break)),
            "Break ready"
        );
        assert_eq!(format_state(TimerState::Focus), "Focus");
        assert_eq!(format_state(TimerState::Break), "Break");
        assert_eq!(format_state(TimerState::Paused), "Paused");
        assert_eq!(
            format_state(TimerState::Completed(SessionKind::Focus)),
            "Completed"
        );
    }
}
