use std::{num::NonZeroU32, time::Duration};

use super::ConfigValidationError;

const SECONDS_PER_MINUTE: u64 = 60;

pub(crate) fn format_duration(duration: Duration) -> String {
    let seconds = duration.as_secs();
    format!(
        "{:02}:{:02}",
        seconds / SECONDS_PER_MINUTE,
        seconds % SECONDS_PER_MINUTE
    )
}

pub(crate) fn parse_duration(
    value: &str,
    field: &'static str,
) -> Result<u64, ConfigValidationError> {
    let (minutes, seconds) = value
        .split_once(':')
        .ok_or(ConfigValidationError::InvalidDuration { field })?;
    if minutes.len() < 2
        || seconds.len() != 2
        || !value.chars().all(|c| c.is_ascii_digit() || c == ':')
    {
        return Err(ConfigValidationError::InvalidDuration { field });
    }
    let minutes = minutes
        .parse::<u64>()
        .map_err(|_| ConfigValidationError::DurationOverflow { field })?;
    let seconds = seconds
        .parse::<u64>()
        .map_err(|_| ConfigValidationError::InvalidDuration { field })?;
    if seconds >= SECONDS_PER_MINUTE {
        return Err(ConfigValidationError::InvalidDuration { field });
    }
    minutes
        .checked_mul(SECONDS_PER_MINUTE)
        .and_then(|total| total.checked_add(seconds))
        .ok_or(ConfigValidationError::DurationOverflow { field })
}

/// Validated timer presets used by the application.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimerConfig {
    pub(super) focus_seconds: u64,
    pub(super) short_break_seconds: u64,
    pub(super) long_break_seconds: u64,
    pub(super) long_break_interval: u32,
    pub(super) autostart_breaks: bool,
    pub(super) autostart_focus: bool,
}

impl TimerConfig {
    pub fn new(
        focus_minutes: u64,
        short_break_minutes: u64,
        long_break_minutes: u64,
        long_break_interval: u32,
    ) -> Result<Self, ConfigValidationError> {
        let focus_seconds = focus_minutes.checked_mul(SECONDS_PER_MINUTE).ok_or(
            ConfigValidationError::DurationOverflow {
                field: "focus_duration",
            },
        )?;
        let short_break_seconds = short_break_minutes.checked_mul(SECONDS_PER_MINUTE).ok_or(
            ConfigValidationError::DurationOverflow {
                field: "short_break_duration",
            },
        )?;
        let long_break_seconds = long_break_minutes.checked_mul(SECONDS_PER_MINUTE).ok_or(
            ConfigValidationError::DurationOverflow {
                field: "long_break_duration",
            },
        )?;
        Self::from_seconds(
            focus_seconds,
            short_break_seconds,
            long_break_seconds,
            long_break_interval,
        )
    }

    pub fn from_seconds(
        focus_seconds: u64,
        short_break_seconds: u64,
        long_break_seconds: u64,
        long_break_interval: u32,
    ) -> Result<Self, ConfigValidationError> {
        let timer = Self {
            focus_seconds,
            short_break_seconds,
            long_break_seconds,
            long_break_interval,
            autostart_breaks: false,
            autostart_focus: false,
        };
        timer.validate()?;
        Ok(timer)
    }

    pub fn with_autostart(mut self, breaks: bool, focus: bool) -> Self {
        self.autostart_breaks = breaks;
        self.autostart_focus = focus;
        self
    }

    pub fn autostart_breaks(&self) -> bool {
        self.autostart_breaks
    }

    pub fn autostart_focus(&self) -> bool {
        self.autostart_focus
    }

    pub fn focus_duration(&self) -> Duration {
        Duration::from_secs(self.focus_seconds)
    }

    pub fn short_break_duration(&self) -> Duration {
        Duration::from_secs(self.short_break_seconds)
    }

    pub fn long_break_duration(&self) -> Duration {
        Duration::from_secs(self.long_break_seconds)
    }

    pub fn long_break_interval(&self) -> NonZeroU32 {
        NonZeroU32::new(self.long_break_interval)
            .expect("validated timer configuration has a positive long-break interval")
    }

    pub(super) fn validate(&self) -> Result<(), ConfigValidationError> {
        for (field, seconds) in [
            ("focus_duration", self.focus_seconds),
            ("short_break_duration", self.short_break_seconds),
            ("long_break_duration", self.long_break_seconds),
        ] {
            if seconds == 0 {
                return Err(ConfigValidationError::ZeroDuration { field });
            }
        }

        if self.long_break_interval == 0 {
            return Err(ConfigValidationError::ZeroLongBreakInterval);
        }

        Ok(())
    }
}

impl Default for TimerConfig {
    fn default() -> Self {
        Self {
            focus_seconds: 25 * SECONDS_PER_MINUTE,
            short_break_seconds: 5 * SECONDS_PER_MINUTE,
            long_break_seconds: 15 * SECONDS_PER_MINUTE,
            long_break_interval: 4,
            autostart_breaks: false,
            autostart_focus: false,
        }
    }
}
