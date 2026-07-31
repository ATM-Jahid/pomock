use std::time::Duration;

use super::{
    App, AppOutcome, ConfirmationOperation, FocusAudioAction, PendingAutostart,
    PendingConfirmation, PriorActivity, TimerChange,
};
use crate::{SessionKind, timer::TimerState};

const PROGRESS_CONFIRMATION_THRESHOLD: Duration = Duration::from_secs(10);
const AUTOSTART_DELAY: Duration = Duration::from_secs(5);

impl App {
    pub fn tick(&mut self, elapsed: Duration) -> AppOutcome {
        if let Some(pending) = &mut self.pending_autostart {
            if elapsed < pending.remaining {
                pending.remaining -= elapsed;
                return AppOutcome::None;
            }
            self.pending_autostart = None;
            self.completion_audio_active = false;
            let before = self.timer.state();
            self.timer.primary_action();
            return Self::autostart_transition_outcome(before, self.timer.state());
        }

        let Some(completed) = self.timer.tick(elapsed) else {
            return AppOutcome::None;
        };
        let recommended = match self.timer.state() {
            TimerState::Ready(session) => session,
            TimerState::Running(_) | TimerState::Paused(_) => {
                unreachable!("completion installs a ready recommendation")
            }
        };
        let enabled = match recommended {
            SessionKind::Focus => self.config.timer().autostart_focus(),
            SessionKind::ShortBreak | SessionKind::LongBreak => {
                self.config.timer().autostart_breaks()
            }
        };
        if enabled {
            self.pending_autostart = Some(PendingAutostart {
                session: recommended,
                remaining: AUTOSTART_DELAY,
            });
        }
        self.completion_audio_active = true;
        AppOutcome::SessionCompleted(completed)
    }

    pub(super) fn autostart_transition_outcome(
        before: TimerState,
        after: TimerState,
    ) -> AppOutcome {
        let focus_audio = match Self::timer_transition_outcome(before, after, AppOutcome::None) {
            AppOutcome::FocusAudio(action) => Some(action),
            AppOutcome::None => None,
            _ => unreachable!("timer transition only reports Focus audio"),
        };
        AppOutcome::TimerEffects {
            focus_audio,
            stop_completion_audio: true,
        }
    }

    pub(super) fn with_completion_stop(outcome: AppOutcome) -> AppOutcome {
        let focus_audio = match outcome {
            AppOutcome::FocusAudio(action) => Some(action),
            AppOutcome::None => None,
            _ => return outcome,
        };
        AppOutcome::TimerEffects {
            focus_audio,
            stop_completion_audio: true,
        }
    }

    pub(super) fn timer_transition_outcome(
        before: TimerState,
        after: TimerState,
        outcome: AppOutcome,
    ) -> AppOutcome {
        if outcome != AppOutcome::None {
            return outcome;
        }
        let action = match (before, after) {
            (
                TimerState::Ready(_)
                | TimerState::Running(SessionKind::ShortBreak | SessionKind::LongBreak)
                | TimerState::Paused(_),
                TimerState::Running(SessionKind::Focus),
            ) => Some(FocusAudioAction::StartOrResume),
            (TimerState::Running(SessionKind::Focus), TimerState::Paused(SessionKind::Focus)) => {
                Some(FocusAudioAction::Pause)
            }
            (
                TimerState::Running(SessionKind::Focus) | TimerState::Paused(SessionKind::Focus),
                TimerState::Ready(_)
                | TimerState::Running(SessionKind::ShortBreak | SessionKind::LongBreak)
                | TimerState::Paused(SessionKind::ShortBreak | SessionKind::LongBreak),
            ) => Some(FocusAudioAction::Stop),
            _ => None,
        };
        action.map_or(AppOutcome::None, AppOutcome::FocusAudio)
    }

    pub(super) fn clock_primary_action(&mut self) {
        self.timer.primary_action();
    }

    pub(super) fn cycle_session(&mut self) {
        self.request_timer_change(TimerChange::Cycle);
    }

    pub(super) fn reset_session(&mut self) {
        self.request_timer_change(TimerChange::Reset);
    }

    pub(super) fn request_quit(&mut self) -> AppOutcome {
        let prior_activity = match self.timer.state() {
            TimerState::Running(_) => PriorActivity::Running,
            TimerState::Paused(_) => PriorActivity::Paused,
            TimerState::Ready(_) => return AppOutcome::Quit,
        };

        if self.timer.progress() < PROGRESS_CONFIRMATION_THRESHOLD {
            return AppOutcome::Quit;
        }

        self.timer.pause();
        self.pending_confirmation = Some(PendingConfirmation {
            operation: ConfirmationOperation::Quit,
            prior_activity,
        });
        self.clear_pending_click();
        AppOutcome::None
    }

    pub(super) fn request_timer_change(&mut self, change: TimerChange) {
        let prior_activity = match self.timer.state() {
            TimerState::Running(_) => PriorActivity::Running,
            TimerState::Paused(_) => PriorActivity::Paused,
            TimerState::Ready(_) => {
                match change {
                    TimerChange::Reset => {}
                    TimerChange::Cycle => self.timer.cycle_ready_session(),
                    TimerChange::SelectSession(session) => self.timer.select_session(session),
                    TimerChange::StartSession(session) => self.timer.start_session(session),
                }
                return;
            }
        };

        if self.timer.progress() < PROGRESS_CONFIRMATION_THRESHOLD {
            self.apply_timer_change(change);
            return;
        }

        self.timer.pause();
        self.pending_confirmation = Some(PendingConfirmation {
            operation: ConfirmationOperation::TimerChange(change),
            prior_activity,
        });
        self.clear_pending_click();
    }

    fn apply_timer_change(&mut self, change: TimerChange) {
        self.timer.reset_session();
        match change {
            TimerChange::Reset => {}
            TimerChange::Cycle => self.timer.cycle_ready_session(),
            TimerChange::SelectSession(session) => self.timer.select_session(session),
            TimerChange::StartSession(session) => self.timer.start_session(session),
        }
    }

    pub(super) fn confirm_pending_action(&mut self) -> AppOutcome {
        let outcome = match self.pending_confirmation.take() {
            Some(PendingConfirmation {
                operation: ConfirmationOperation::Quit,
                ..
            }) => AppOutcome::Quit,
            Some(PendingConfirmation {
                operation: ConfirmationOperation::TimerChange(change),
                ..
            }) => {
                self.apply_timer_change(change);
                AppOutcome::None
            }
            None => AppOutcome::None,
        };
        self.clear_pending_click();
        outcome
    }

    pub(super) fn cancel_pending_action(&mut self) {
        let resume = self
            .pending_confirmation
            .take()
            .is_some_and(|pending| pending.prior_activity == PriorActivity::Running);
        if resume {
            self.timer.resume();
        }
        self.clear_pending_click();
    }
}
