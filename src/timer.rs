use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionKind {
    Focus,
    Break,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimerState {
    Ready(SessionKind),
    Focus,
    Break,
    Paused,
    Completed(SessionKind),
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
            state: TimerState::Ready(SessionKind::Focus),
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
    pub fn primary_action(&mut self) {
        match self.state {
            TimerState::Ready(SessionKind::Focus) => self.start_focus(),
            TimerState::Ready(SessionKind::Break) => self.start_break(),
            TimerState::Focus | TimerState::Break => self.pause(),
            TimerState::Paused => self.resume(),
            TimerState::Completed(SessionKind::Focus) => self.start_break(),
            TimerState::Completed(SessionKind::Break) => self.start_focus(),
        }
    }

    pub fn fast_forward(&mut self) {
        let next_session = match self.current_session() {
            Some(SessionKind::Break) => SessionKind::Focus,
            Some(SessionKind::Focus) | None => SessionKind::Break,
        };

        self.state = TimerState::Ready(next_session);
        self.remaining = self.duration_for(next_session);
        self.previous_state = None;
    }

    pub fn reset_session(&mut self) {
        let session = match self.state {
            TimerState::Focus => SessionKind::Focus,
            TimerState::Break => SessionKind::Break,
            TimerState::Paused => match self.previous_state {
                Some(TimerState::Focus) => SessionKind::Focus,
                Some(TimerState::Break) => SessionKind::Break,
                _ => return,
            },
            TimerState::Ready(_) | TimerState::Completed(_) => return,
        };

        self.state = TimerState::Ready(session);
        self.remaining = self.duration_for(session);
        self.previous_state = None;
    }

    fn duration_for(&self, session: SessionKind) -> Duration {
        match session {
            SessionKind::Focus => self.focus_duration,
            SessionKind::Break => self.break_duration,
        }
    }

    fn current_session(&self) -> Option<SessionKind> {
        match self.state {
            TimerState::Ready(session) => Some(session),
            TimerState::Focus => Some(SessionKind::Focus),
            TimerState::Break => Some(SessionKind::Break),
            TimerState::Paused => match self.previous_state {
                Some(TimerState::Focus) => Some(SessionKind::Focus),
                Some(TimerState::Break) => Some(SessionKind::Break),
                _ => None,
            },
            TimerState::Completed(session) => Some(session),
        }
    }

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
            TimerState::Ready(_) | TimerState::Paused | TimerState::Completed(_) => {}
        }
    }

    pub fn resume(&mut self) {
        if self.state == TimerState::Paused
            && let Some(previous_state) = self.previous_state
        {
            self.state = previous_state;
            self.previous_state = None;
        }
    }

    pub fn reset(&mut self) {
        self.state = TimerState::Ready(SessionKind::Focus);
        self.remaining = self.focus_duration;
        self.previous_state = None;
    }

    pub fn tick(&mut self, elapsed: Duration) {
        match self.state {
            TimerState::Focus | TimerState::Break => {
                if elapsed >= self.remaining {
                    self.remaining = Duration::ZERO;
                    let completed_session = match self.state {
                        TimerState::Focus => SessionKind::Focus,
                        TimerState::Break => SessionKind::Break,
                        _ => unreachable!("tick only completes a running session"),
                    };
                    self.state = TimerState::Completed(completed_session);
                    self.previous_state = None;
                } else {
                    self.remaining -= elapsed;
                }
            }
            TimerState::Ready(_) | TimerState::Paused | TimerState::Completed(_) => {}
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
    fn new_timer_starts_with_focus_ready() {
        let timer = timer();

        assert_eq!(timer.state(), TimerState::Ready(SessionKind::Focus));
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
    fn primary_action_starts_focus_when_ready() {
        let mut timer = timer();

        timer.primary_action();

        assert_eq!(timer.state(), TimerState::Focus);
        assert_eq!(timer.remaining(), Duration::from_secs(25 * 60));
    }

    #[test]
    fn primary_action_pauses_a_running_session() {
        let mut timer = timer();
        timer.start_focus();

        timer.primary_action();

        assert_eq!(timer.state(), TimerState::Paused);
    }

    #[test]
    fn primary_action_resumes_a_paused_session() {
        let mut timer = timer();
        timer.start_focus();
        timer.pause();

        timer.primary_action();

        assert_eq!(timer.state(), TimerState::Focus);
    }

    #[test]
    fn primary_action_starts_break_after_focus_completes() {
        let mut timer = timer();
        timer.start_focus();
        timer.tick(Duration::from_secs(25 * 60));

        timer.primary_action();

        assert_eq!(timer.state(), TimerState::Break);
        assert_eq!(timer.remaining(), Duration::from_secs(5 * 60));
    }

    #[test]
    fn primary_action_starts_focus_after_break_completes() {
        let mut timer = timer();
        timer.start_break();
        timer.tick(Duration::from_secs(5 * 60));

        timer.primary_action();

        assert_eq!(timer.state(), TimerState::Focus);
        assert_eq!(timer.remaining(), Duration::from_secs(25 * 60));
    }

    #[test]
    fn fast_forward_selects_break_from_initial_focus() {
        let mut timer = timer();

        timer.fast_forward();

        assert_eq!(timer.state(), TimerState::Ready(SessionKind::Break));
        assert_eq!(timer.remaining(), Duration::from_secs(5 * 60));
    }

    #[test]
    fn fast_forward_moves_between_session_types() {
        let mut timer = timer();
        timer.start_focus();

        timer.fast_forward();
        assert_eq!(timer.state(), TimerState::Ready(SessionKind::Break));
        assert_eq!(timer.remaining(), Duration::from_secs(5 * 60));

        timer.fast_forward();
        assert_eq!(timer.state(), TimerState::Ready(SessionKind::Focus));
        assert_eq!(timer.remaining(), Duration::from_secs(25 * 60));
    }

    #[test]
    fn fast_forward_uses_paused_session_type() {
        let mut timer = timer();
        timer.start_focus();
        timer.pause();

        timer.fast_forward();

        assert_eq!(timer.state(), TimerState::Ready(SessionKind::Break));
    }

    #[test]
    fn ready_session_starts_only_after_primary_action() {
        let mut timer = timer();
        timer.fast_forward();

        timer.tick(Duration::from_secs(60));
        assert_eq!(timer.state(), TimerState::Ready(SessionKind::Break));
        assert_eq!(timer.remaining(), Duration::from_secs(5 * 60));

        timer.primary_action();
        assert_eq!(timer.state(), TimerState::Break);
    }

    #[test]
    fn reset_session_returns_running_session_to_ready() {
        let mut timer = timer();
        timer.start_focus();
        timer.tick(Duration::from_secs(60));

        timer.reset_session();

        assert_eq!(timer.state(), TimerState::Ready(SessionKind::Focus));
        assert_eq!(timer.remaining(), Duration::from_secs(25 * 60));
    }

    #[test]
    fn reset_session_returns_paused_break_to_ready() {
        let mut timer = timer();
        timer.start_break();
        timer.tick(Duration::from_secs(60));
        timer.pause();

        timer.reset_session();

        assert_eq!(timer.state(), TimerState::Ready(SessionKind::Break));
        assert_eq!(timer.remaining(), Duration::from_secs(5 * 60));
    }

    #[test]
    fn reset_session_returns_paused_focus_to_ready() {
        let mut timer = timer();
        timer.start_focus();
        timer.tick(Duration::from_secs(60));
        timer.pause();

        timer.reset_session();

        assert_eq!(timer.state(), TimerState::Ready(SessionKind::Focus));
        assert_eq!(timer.remaining(), Duration::from_secs(25 * 60));
    }

    #[test]
    fn reset_session_does_nothing_after_completion() {
        let mut timer = timer();
        timer.start_break();
        timer.tick(Duration::from_secs(5 * 60));

        timer.reset_session();

        assert_eq!(timer.state(), TimerState::Completed(SessionKind::Break));
        assert_eq!(timer.remaining(), Duration::ZERO);
    }

    #[test]
    fn reset_session_does_nothing_when_ready() {
        let mut timer = timer();

        timer.reset_session();
        assert_eq!(timer.state(), TimerState::Ready(SessionKind::Focus));

        timer.fast_forward();
        timer.reset_session();
        assert_eq!(timer.state(), TimerState::Ready(SessionKind::Break));
        assert_eq!(timer.remaining(), Duration::from_secs(5 * 60));
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

        assert_eq!(timer.state(), TimerState::Completed(SessionKind::Focus));
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
    fn reset_returns_timer_to_focus_ready() {
        let mut timer = timer();

        timer.start_focus();
        timer.tick(Duration::from_secs(60));
        timer.reset();

        assert_eq!(timer.state(), TimerState::Ready(SessionKind::Focus));
        assert_eq!(timer.remaining(), Duration::from_secs(25 * 60));
    }
}
