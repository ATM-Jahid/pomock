use std::time::{Duration, Instant};

use super::{Action, App, AppOutcome, ConfirmationOperation, EditMode, TimerChange, UiFocus};
use crate::{SessionKind, timer::TimerState};

const DOUBLE_CLICK_WINDOW: Duration = Duration::from_millis(500);

/// A semantic mouse target produced by UI coordinate hit testing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClickTarget {
    Clock,
    SessionControl(SessionKind),
    Todo,
    TodoTask(usize),
    Done,
    DoneTask(usize),
    SettingsRow(usize),
    Outside,
}

impl App {
    /// Applies a semantic click after the UI boundary performs hit testing.
    pub fn handle_click_target(&mut self, target: ClickTarget, now: Instant) -> AppOutcome {
        let prior_timer_state = self.timer.state();
        if self.edit_mode != EditMode::Normal {
            return AppOutcome::None;
        }

        if self.pending_confirmation.is_some() {
            self.upgrade_pending_session_click(target, now);
            return AppOutcome::None;
        }

        if self.settings.is_some() {
            let ClickTarget::SettingsRow(selection) = target else {
                return AppOutcome::None;
            };
            let is_double = self.is_double_click(target, now);
            if let Some(settings) = &mut self.settings {
                settings.select(selection);
            }
            if is_double {
                self.last_click = None;
                return self.dispatch(Action::SettingsActivate);
            }
            self.last_click = Some((target, now));
            return AppOutcome::None;
        }

        if self.pending_autostart.is_some() && matches!(target, ClickTarget::SessionControl(_)) {
            self.pending_autostart = None;
            self.completion_audio_active = false;
            if let ClickTarget::SessionControl(session) = target {
                self.focus(UiFocus::Clock);
                self.timer.select_session(session);
                self.last_click = Some((target, now));
            }
            return Self::autostart_transition_outcome(prior_timer_state, self.timer.state());
        }

        if self.pending_autostart.is_some()
            && target == ClickTarget::Clock
            && self.is_double_click(target, now)
        {
            self.pending_autostart = None;
            self.completion_audio_active = false;
            self.focus(UiFocus::Clock);
            self.timer.primary_action();
            self.clear_pending_click();
            return Self::autostart_transition_outcome(prior_timer_state, self.timer.state());
        }

        let stop_completion_audio = self.completion_audio_active
            && (matches!(target, ClickTarget::SessionControl(_))
                || target == ClickTarget::Clock && self.is_double_click(target, now));
        if stop_completion_audio {
            self.completion_audio_active = false;
        }

        let tasks_changed = match target {
            ClickTarget::Clock => {
                self.focus(UiFocus::Clock);
                self.handle_actionable_click(target, now);
                false
            }
            ClickTarget::SessionControl(session) => {
                self.focus(UiFocus::Clock);
                match self.timer.state() {
                    TimerState::Ready(_) => {
                        if self.is_double_click(target, now) {
                            self.timer.start_session(session);
                            self.clear_pending_click();
                        } else {
                            self.timer.select_session(session);
                            self.last_click = Some((target, now));
                        }
                    }
                    TimerState::Running(active_session) | TimerState::Paused(active_session) => {
                        if session == active_session {
                            if self.is_double_click(target, now) {
                                self.clock_primary_action();
                                self.clear_pending_click();
                            } else {
                                self.last_click = Some((target, now));
                            }
                        } else {
                            self.request_timer_change(TimerChange::SelectSession(session));
                            self.last_click = Some((target, now));
                        }
                    }
                }
                false
            }
            ClickTarget::Todo => {
                self.focus(UiFocus::Todo);
                self.clear_pending_click();
                false
            }
            ClickTarget::TodoTask(selection) => {
                self.focus(UiFocus::Todo);
                self.select_todo(selection);
                self.handle_actionable_click(target, now)
            }
            ClickTarget::Done => {
                self.focus(UiFocus::Done);
                self.clear_pending_click();
                false
            }
            ClickTarget::DoneTask(selection) => {
                self.focus(UiFocus::Done);
                self.select_done(selection);
                self.handle_actionable_click(target, now)
            }
            ClickTarget::Outside => {
                self.clear_pending_click();
                false
            }
            ClickTarget::SettingsRow(_) => false,
        };

        if tasks_changed {
            AppOutcome::TasksChanged
        } else {
            let outcome = Self::timer_transition_outcome(
                prior_timer_state,
                self.timer.state(),
                AppOutcome::None,
            );
            if stop_completion_audio {
                Self::with_completion_stop(outcome)
            } else {
                outcome
            }
        }
    }

    pub(super) fn clear_pending_click(&mut self) {
        self.last_click = None;
    }

    fn upgrade_pending_session_click(&mut self, target: ClickTarget, now: Instant) {
        let ClickTarget::SessionControl(session) = target else {
            return;
        };
        let should_upgrade = self.pending_confirmation.as_ref().is_some_and(|pending| {
            pending.operation
                == ConfirmationOperation::TimerChange(TimerChange::SelectSession(session))
                && self.is_double_click(target, now)
        });

        if should_upgrade {
            if let Some(pending) = &mut self.pending_confirmation {
                pending.operation =
                    ConfirmationOperation::TimerChange(TimerChange::StartSession(session));
            }
            self.clear_pending_click();
        }
    }

    fn handle_actionable_click(&mut self, target: ClickTarget, now: Instant) -> bool {
        let is_double_click = self.is_double_click(target, now);

        if is_double_click {
            let tasks_changed = match target {
                ClickTarget::Clock => {
                    self.clock_primary_action();
                    false
                }
                ClickTarget::TodoTask(_) => self.complete_selected_todo(),
                ClickTarget::DoneTask(_) => self.return_selected_done(),
                ClickTarget::SessionControl(_)
                | ClickTarget::Todo
                | ClickTarget::Done
                | ClickTarget::SettingsRow(_)
                | ClickTarget::Outside => {
                    unreachable!("only actionable targets are recorded")
                }
            };
            self.last_click = None;
            tasks_changed
        } else {
            self.last_click = Some((target, now));
            false
        }
    }

    fn is_double_click(&self, target: ClickTarget, now: Instant) -> bool {
        self.last_click.is_some_and(|(last_target, last_time)| {
            last_target == target
                && now
                    .checked_duration_since(last_time)
                    .is_some_and(|elapsed| elapsed <= DOUBLE_CLICK_WINDOW)
        })
    }
}
