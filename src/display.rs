use std::time::Duration;

use crate::timer::TimerState;

pub fn format_duration(duration: Duration) -> String {
    let total_seconds = duration.as_secs();
    let minutes = total_seconds / 60;
    let seconds = total_seconds % 60;

    format!("{minutes:02}:{seconds:02}")
}

pub fn format_state(state: TimerState) -> &'static str {
    match state {
        TimerState::Idle => "Idle",
        TimerState::Focus => "Focus",
        TimerState::Break => "Break",
        TimerState::Paused => "Paused",
        TimerState::Completed => "Completed",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::timer::TimerState;
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
    fn formats_timer_state_labels() {
        assert_eq!(format_state(TimerState::Idle), "Idle");
        assert_eq!(format_state(TimerState::Focus), "Focus");
        assert_eq!(format_state(TimerState::Break), "Break");
        assert_eq!(format_state(TimerState::Paused), "Paused");
        assert_eq!(format_state(TimerState::Completed), "Completed");
    }
}
