use std::{num::NonZeroU32, time::Duration};

use super::ConfigValidationError;

const SECONDS_PER_MINUTE: u64 = 60;

/// Timer values as presented in the user configuration file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimerConfig {
    pub(super) focus_minutes: u64,
    pub(super) short_break_minutes: u64,
    pub(super) long_break_minutes: u64,
    pub(super) long_break_interval: u32,
}

impl TimerConfig {
    pub fn new(
        focus_minutes: u64,
        short_break_minutes: u64,
        long_break_minutes: u64,
        long_break_interval: u32,
    ) -> Result<Self, ConfigValidationError> {
        let timer = Self {
            focus_minutes,
            short_break_minutes,
            long_break_minutes,
            long_break_interval,
        };
        timer.validate()?;
        Ok(timer)
    }

    pub fn focus_duration(&self) -> Duration {
        Duration::from_secs(self.focus_minutes * SECONDS_PER_MINUTE)
    }

    pub fn short_break_duration(&self) -> Duration {
        Duration::from_secs(self.short_break_minutes * SECONDS_PER_MINUTE)
    }

    pub fn long_break_duration(&self) -> Duration {
        Duration::from_secs(self.long_break_minutes * SECONDS_PER_MINUTE)
    }

    pub fn long_break_interval(&self) -> NonZeroU32 {
        NonZeroU32::new(self.long_break_interval)
            .expect("validated timer configuration has a positive long-break interval")
    }

    pub(super) fn validate(&self) -> Result<(), ConfigValidationError> {
        for (field, minutes) in [
            ("focus_minutes", self.focus_minutes),
            ("short_break_minutes", self.short_break_minutes),
            ("long_break_minutes", self.long_break_minutes),
        ] {
            if minutes == 0 {
                return Err(ConfigValidationError::ZeroDuration { field });
            }
            if minutes.checked_mul(SECONDS_PER_MINUTE).is_none() {
                return Err(ConfigValidationError::DurationOverflow { field });
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
            focus_minutes: 25,
            short_break_minutes: 5,
            long_break_minutes: 15,
            long_break_interval: 4,
        }
    }
}
