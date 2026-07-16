use std::{num::NonZeroU32, time::Duration};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionKind {
    Focus,
    ShortBreak,
    LongBreak,
}

impl SessionKind {
    fn next_in_cycle(self) -> Self {
        match self {
            Self::Focus => Self::ShortBreak,
            Self::ShortBreak => Self::LongBreak,
            Self::LongBreak => Self::Focus,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimerState {
    Ready(SessionKind),
    Running(SessionKind),
    Paused(SessionKind),
}

#[derive(Debug, Clone)]
pub struct PomodoroTimer {
    state: TimerState,
    focus_duration: Duration,
    short_break_duration: Duration,
    long_break_duration: Duration,
    long_break_interval: NonZeroU32,
    installed_duration: Duration,
    remaining: Duration,
    completed_focus_sessions: u32,
}

impl PomodoroTimer {
    pub fn new(
        focus_duration: Duration,
        short_break_duration: Duration,
        long_break_duration: Duration,
        long_break_interval: NonZeroU32,
    ) -> Self {
        Self {
            state: TimerState::Ready(SessionKind::Focus),
            focus_duration,
            short_break_duration,
            long_break_duration,
            long_break_interval,
            installed_duration: focus_duration,
            remaining: focus_duration,
            completed_focus_sessions: 0,
        }
    }

    pub fn state(&self) -> TimerState {
        self.state
    }

    pub fn remaining(&self) -> Duration {
        self.remaining
    }

    pub fn completed_focus_sessions(&self) -> u32 {
        self.completed_focus_sessions
    }

    pub fn progress(&self) -> Duration {
        self.installed_duration.saturating_sub(self.remaining)
    }

    /// Replaces duration presets without disturbing an active session.
    pub fn reconfigure(
        &mut self,
        focus_duration: Duration,
        short_break_duration: Duration,
        long_break_duration: Duration,
        long_break_interval: NonZeroU32,
    ) {
        self.focus_duration = focus_duration;
        self.short_break_duration = short_break_duration;
        self.long_break_duration = long_break_duration;
        self.long_break_interval = long_break_interval;

        if let TimerState::Ready(session) = self.state {
            self.install(session);
        }
    }

    pub fn primary_action(&mut self) {
        match self.state {
            TimerState::Ready(session) => self.state = TimerState::Running(session),
            TimerState::Running(_) => self.pause(),
            TimerState::Paused(_) => self.resume(),
        }
    }

    pub fn cycle_ready_session(&mut self) {
        let TimerState::Ready(session) = self.state else {
            return;
        };

        self.select_session(session.next_in_cycle());
    }

    pub fn select_session(&mut self, session: SessionKind) {
        if !matches!(self.state, TimerState::Ready(_)) {
            return;
        }

        self.state = TimerState::Ready(session);
        self.install(session);
    }

    pub fn start_session(&mut self, session: SessionKind) {
        if !matches!(self.state, TimerState::Ready(_)) {
            return;
        }

        self.state = TimerState::Running(session);
        self.install(session);
    }

    pub fn pause(&mut self) {
        if let TimerState::Running(session) = self.state {
            self.state = TimerState::Paused(session);
        }
    }

    pub fn resume(&mut self) {
        if let TimerState::Paused(session) = self.state {
            self.state = TimerState::Running(session);
        }
    }

    pub fn reset_session(&mut self) {
        let session = match self.state {
            TimerState::Running(session) | TimerState::Paused(session) => session,
            TimerState::Ready(_) => return,
        };

        self.state = TimerState::Ready(session);
        self.install(session);
    }

    /// Advances a running session and returns its completion event exactly once.
    pub fn tick(&mut self, elapsed: Duration) -> Option<SessionKind> {
        let TimerState::Running(completed_session) = self.state else {
            return None;
        };

        if elapsed < self.remaining {
            self.remaining -= elapsed;
            return None;
        }

        let next_session = match completed_session {
            SessionKind::Focus => {
                self.completed_focus_sessions = self.completed_focus_sessions.saturating_add(1);
                if self.completed_focus_sessions % self.long_break_interval.get() == 0 {
                    SessionKind::LongBreak
                } else {
                    SessionKind::ShortBreak
                }
            }
            SessionKind::ShortBreak | SessionKind::LongBreak => SessionKind::Focus,
        };

        self.state = TimerState::Ready(next_session);
        self.install(next_session);
        Some(completed_session)
    }

    fn duration_for(&self, session: SessionKind) -> Duration {
        match session {
            SessionKind::Focus => self.focus_duration,
            SessionKind::ShortBreak => self.short_break_duration,
            SessionKind::LongBreak => self.long_break_duration,
        }
    }

    fn install(&mut self, session: SessionKind) {
        self.installed_duration = self.duration_for(session);
        self.remaining = self.installed_duration;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FOCUS: Duration = Duration::from_secs(25 * 60);
    const SHORT_BREAK: Duration = Duration::from_secs(5 * 60);
    const LONG_BREAK: Duration = Duration::from_secs(15 * 60);

    fn timer() -> PomodoroTimer {
        PomodoroTimer::new(FOCUS, SHORT_BREAK, LONG_BREAK, NonZeroU32::new(4).unwrap())
    }

    fn complete(timer: &mut PomodoroTimer, session: SessionKind) {
        timer.start_session(session);
        let duration = timer.remaining();
        assert_eq!(timer.tick(duration), Some(session));
    }

    #[test]
    fn new_timer_starts_with_focus_ready() {
        let timer = timer();

        assert_eq!(timer.state(), TimerState::Ready(SessionKind::Focus));
        assert_eq!(timer.remaining(), FOCUS);
        assert_eq!(timer.completed_focus_sessions(), 0);
    }

    #[test]
    fn primary_action_transitions_ready_running_paused_and_running_for_every_kind() {
        for (session, selections, duration) in [
            (SessionKind::Focus, 0, FOCUS),
            (SessionKind::ShortBreak, 1, SHORT_BREAK),
            (SessionKind::LongBreak, 2, LONG_BREAK),
        ] {
            let mut timer = timer();
            for _ in 0..selections {
                timer.cycle_ready_session();
            }

            timer.primary_action();
            assert_eq!(timer.state(), TimerState::Running(session));
            timer.tick(Duration::from_secs(1));

            timer.primary_action();
            assert_eq!(timer.state(), TimerState::Paused(session));
            assert_eq!(timer.remaining(), duration - Duration::from_secs(1));

            timer.primary_action();
            assert_eq!(timer.state(), TimerState::Running(session));
            assert_eq!(timer.remaining(), duration - Duration::from_secs(1));
        }
    }

    #[test]
    fn ready_selection_cycles_all_session_kinds_without_starting() {
        let mut timer = timer();

        timer.cycle_ready_session();
        assert_eq!(timer.state(), TimerState::Ready(SessionKind::ShortBreak));
        assert_eq!(timer.remaining(), SHORT_BREAK);
        timer.cycle_ready_session();
        assert_eq!(timer.state(), TimerState::Ready(SessionKind::LongBreak));
        assert_eq!(timer.remaining(), LONG_BREAK);
        timer.cycle_ready_session();
        assert_eq!(timer.state(), TimerState::Ready(SessionKind::Focus));
        assert_eq!(timer.remaining(), FOCUS);
    }

    #[test]
    fn ready_cycle_does_nothing_while_running_or_paused() {
        let mut timer = timer();
        timer.primary_action();

        timer.cycle_ready_session();
        assert_eq!(timer.state(), TimerState::Running(SessionKind::Focus));
        timer.primary_action();

        timer.cycle_ready_session();
        assert_eq!(timer.state(), TimerState::Paused(SessionKind::Focus));
    }

    #[test]
    fn explicit_session_controls_start_only_from_ready() {
        let mut timer = timer();

        timer.start_session(SessionKind::LongBreak);
        assert_eq!(timer.state(), TimerState::Running(SessionKind::LongBreak));
        assert_eq!(timer.remaining(), LONG_BREAK);

        timer.start_session(SessionKind::ShortBreak);
        assert_eq!(timer.state(), TimerState::Running(SessionKind::LongBreak));
        timer.primary_action();
        timer.start_session(SessionKind::Focus);
        assert_eq!(timer.state(), TimerState::Paused(SessionKind::LongBreak));
    }

    #[test]
    fn explicit_session_selection_changes_only_a_ready_timer() {
        let mut timer = timer();

        timer.select_session(SessionKind::LongBreak);
        assert_eq!(timer.state(), TimerState::Ready(SessionKind::LongBreak));
        assert_eq!(timer.remaining(), LONG_BREAK);

        timer.primary_action();
        timer.select_session(SessionKind::ShortBreak);
        assert_eq!(timer.state(), TimerState::Running(SessionKind::LongBreak));

        timer.primary_action();
        timer.select_session(SessionKind::Focus);
        assert_eq!(timer.state(), TimerState::Paused(SessionKind::LongBreak));
    }

    #[test]
    fn tick_only_reduces_running_sessions() {
        let mut timer = timer();

        assert_eq!(timer.tick(Duration::from_secs(60)), None);
        assert_eq!(timer.remaining(), FOCUS);
        timer.primary_action();
        assert_eq!(timer.tick(Duration::from_secs(60)), None);
        assert_eq!(timer.remaining(), Duration::from_secs(24 * 60));
        timer.primary_action();
        assert_eq!(timer.tick(Duration::from_secs(60)), None);
        assert_eq!(timer.remaining(), Duration::from_secs(24 * 60));
    }

    #[test]
    fn completion_is_an_event_and_immediately_prepares_the_next_session() {
        let mut timer = timer();
        timer.primary_action();

        assert_eq!(timer.tick(FOCUS), Some(SessionKind::Focus));
        assert_eq!(timer.state(), TimerState::Ready(SessionKind::ShortBreak));
        assert_eq!(timer.remaining(), SHORT_BREAK);
        assert_eq!(timer.tick(Duration::from_secs(1)), None);
        assert_eq!(timer.completed_focus_sessions(), 1);
    }

    #[test]
    fn focus_recommendations_cover_before_at_and_after_modulo_boundary() {
        let mut timer = timer();

        for completed in 1..=5 {
            complete(&mut timer, SessionKind::Focus);
            let recommendation = if completed == 4 {
                SessionKind::LongBreak
            } else {
                SessionKind::ShortBreak
            };
            assert_eq!(timer.state(), TimerState::Ready(recommendation));
            assert_eq!(timer.completed_focus_sessions(), completed);
        }
    }

    #[test]
    fn manual_overrides_do_not_shift_focus_recommendations() {
        let mut timer = timer();

        for _ in 0..3 {
            complete(&mut timer, SessionKind::Focus);
            timer.cycle_ready_session();
            timer.cycle_ready_session();
        }
        assert_eq!(timer.completed_focus_sessions(), 3);

        complete(&mut timer, SessionKind::Focus);
        assert_eq!(timer.state(), TimerState::Ready(SessionKind::LongBreak));
    }

    #[test]
    fn both_break_kinds_complete_back_to_focus_without_changing_count() {
        for session in [SessionKind::ShortBreak, SessionKind::LongBreak] {
            let mut timer = timer();

            complete(&mut timer, session);

            assert_eq!(timer.state(), TimerState::Ready(SessionKind::Focus));
            assert_eq!(timer.remaining(), FOCUS);
            assert_eq!(timer.completed_focus_sessions(), 0);
        }
    }

    #[test]
    fn reset_restores_full_duration_for_running_and_paused_sessions() {
        for pause_first in [false, true] {
            let mut timer = timer();
            timer.start_session(SessionKind::LongBreak);
            timer.tick(Duration::from_secs(60));
            if pause_first {
                timer.pause();
            }

            timer.reset_session();

            assert_eq!(timer.state(), TimerState::Ready(SessionKind::LongBreak));
            assert_eq!(timer.remaining(), LONG_BREAK);
            assert_eq!(timer.progress(), Duration::ZERO);
        }
    }

    #[test]
    fn reconfigure_preserves_an_active_sessions_installed_duration_and_progress() {
        let mut timer = timer();
        timer.primary_action();
        assert_eq!(timer.tick(Duration::from_secs(60)), None);
        timer.pause();

        timer.reconfigure(
            Duration::from_secs(30 * 60),
            Duration::from_secs(7 * 60),
            Duration::from_secs(20 * 60),
            NonZeroU32::new(3).unwrap(),
        );

        assert_eq!(timer.state(), TimerState::Paused(SessionKind::Focus));
        assert_eq!(timer.progress(), Duration::from_secs(60));
        assert_eq!(timer.remaining(), FOCUS - Duration::from_secs(60));

        timer.resume();
        assert_eq!(timer.tick(timer.remaining()), Some(SessionKind::Focus));
        assert_eq!(timer.state(), TimerState::Ready(SessionKind::ShortBreak));
        assert_eq!(timer.remaining(), Duration::from_secs(7 * 60));
    }

    #[test]
    fn reconfigure_updates_a_ready_sessions_installed_duration() {
        let mut timer = timer();

        timer.reconfigure(
            Duration::from_secs(30 * 60),
            SHORT_BREAK,
            LONG_BREAK,
            NonZeroU32::new(4).unwrap(),
        );

        assert_eq!(timer.state(), TimerState::Ready(SessionKind::Focus));
        assert_eq!(timer.remaining(), Duration::from_secs(30 * 60));
        assert_eq!(timer.progress(), Duration::ZERO);
    }

    #[test]
    fn reset_and_ready_cycle_do_not_change_completed_focus_count() {
        let mut timer = timer();
        timer.primary_action();
        timer.tick(Duration::from_secs(10));
        timer.reset_session();
        timer.cycle_ready_session();

        assert_eq!(timer.completed_focus_sessions(), 0);
    }

    #[test]
    fn reset_does_nothing_to_a_ready_session() {
        let mut timer = timer();
        timer.cycle_ready_session();

        timer.reset_session();

        assert_eq!(timer.state(), TimerState::Ready(SessionKind::ShortBreak));
        assert_eq!(timer.remaining(), SHORT_BREAK);
    }
}
