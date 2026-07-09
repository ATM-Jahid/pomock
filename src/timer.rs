use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimerState {
    Idle,
    Focus,
    Break,
    Paused,
    Completed,
}

#[derive(Debug, Clone)]
pub struct PomodoroTimer {
    state: TimerState,
    focus_duration: Duration,
    break_duration: Duration,
    remaining: Duration,
    previous_state: Option<TimerState>,
}

impl PomodoroTimer {
    pub fn new(focus_duration: Duration, break_duration: Duration) -> Self {
        Self {
            state: TimerState::Idle,
            focus_duration,
            break_duration,
            remaining: focus_duration,
            previous_state: None,
        }
    }

    pub fn state(&self) -> TimerState {
        self.state
    }

    pub fn remaining(&self) -> Duration {
        self.remaining
    }
}

impl PomodoroTimer {
    pub fn start_focus(&mut self) {
        self.state = TimerState::Focus;
        self.remaining = self.focus_duration;
        self.previous_state = None;
    }

    pub fn start_break(&mut self) {
        self.state = TimerState::Break;
        self.remaining = self.break_duration;
        self.previous_state = None;
    }

    pub fn pause(&mut self) {
        match self.state {
            TimerState::Focus | TimerState::Break => {
                self.previous_state = Some(self.state);
                self.state = TimerState::Paused;
            }
            TimerState::Idle | TimerState::Paused | TimerState::Completed => {}
        }
    }

    pub fn resume(&mut self) {
        if self.state == TimerState::Paused {
            if let Some(previous_state) = self.previous_state {
                self.state = previous_state;
                self.previous_state = None;
            }
        }
    }

    pub fn reset(&mut self) {
        self.state = TimerState::Idle;
        self.remaining = self.focus_duration;
        self.previous_state = None;
    }

    pub fn tick(&mut self, elapsed: Duration) {
        match self.state {
            TimerState::Focus | TimerState::Break => {
                if elapsed >= self.remaining {
                    self.remaining = Duration::ZERO;
                    self.state = TimerState::Completed;
                    self.previous_state = None;
                } else {
                    self.remaining -= elapsed;
                }
            }
            TimerState::Idle | TimerState::Paused | TimerState::Completed => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn timer() -> PomodoroTimer {
        PomodoroTimer::new(Duration::from_secs(25 * 60), Duration::from_secs(5 * 60))
    }

    #[test]
    fn new_timer_starts_idle_with_focus_time_remaining() {
        let timer = timer();

        assert_eq!(timer.state(), TimerState::Idle);
        assert_eq!(timer.remaining(), Duration::from_secs(25 * 60));
    }

    #[test]
    fn start_focus_sets_focus_state_and_focus_duration() {
        let mut timer = timer();

        timer.start_focus();

        assert_eq!(timer.state(), TimerState::Focus);
        assert_eq!(timer.remaining(), Duration::from_secs(25 * 60));
    }

    #[test]
    fn tick_reduces_remaining_time_while_focus_is_running() {
        let mut timer = timer();

        timer.start_focus();
        timer.tick(Duration::from_secs(60));

        assert_eq!(timer.state(), TimerState::Focus);
        assert_eq!(timer.remaining(), Duration::from_secs(24 * 60));
    }

    #[test]
    fn tick_completes_timer_when_elapsed_reaches_remaining_time() {
        let mut timer = PomodoroTimer::new(Duration::from_secs(10), Duration::from_secs(5));

        timer.start_focus();
        timer.tick(Duration::from_secs(10));

        assert_eq!(timer.state(), TimerState::Completed);
        assert_eq!(timer.remaining(), Duration::ZERO);
    }

    #[test]
    fn pause_and_resume_return_to_focus() {
        let mut timer = timer();

        timer.start_focus();
        timer.tick(Duration::from_secs(60));
        timer.pause();

        assert_eq!(timer.state(), TimerState::Paused);
        assert_eq!(timer.remaining(), Duration::from_secs(24 * 60));

        timer.resume();

        assert_eq!(timer.state(), TimerState::Focus);
        assert_eq!(timer.remaining(), Duration::from_secs(24 * 60));
    }

    #[test]
    fn paused_timer_does_not_tick_down() {
        let mut timer = timer();

        timer.start_focus();
        timer.pause();
        timer.tick(Duration::from_secs(60));

        assert_eq!(timer.state(), TimerState::Paused);
        assert_eq!(timer.remaining(), Duration::from_secs(25 * 60));
    }

    #[test]
    fn start_break_sets_break_state_and_break_duration() {
        let mut timer = timer();

        timer.start_break();

        assert_eq!(timer.state(), TimerState::Break);
        assert_eq!(timer.remaining(), Duration::from_secs(5 * 60));
    }

    #[test]
    fn reset_returns_timer_to_idle_focus_duration() {
        let mut timer = timer();

        timer.start_focus();
        timer.tick(Duration::from_secs(60));
        timer.reset();

        assert_eq!(timer.state(), TimerState::Idle);
        assert_eq!(timer.remaining(), Duration::from_secs(25 * 60));
    }
}
