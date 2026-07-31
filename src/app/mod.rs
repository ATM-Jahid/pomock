use std::time::{Duration, Instant};

use crate::{
    SessionKind,
    config::Config,
    settings::SettingsOverlay,
    tasks::TaskList,
    timer::{PomodoroTimer, TimerState},
};

mod action;
mod pointer;
mod settings_flow;
mod task_flow;
mod timer_flow;

pub use action::{
    Action, AppOutcome, Direction, FocusAudioAction, ScrollTarget, SettingsAdjustmentDirection,
    SettingsMoveDirection,
};
pub use pointer::ClickTarget;

/// The application area that currently receives contextual commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiFocus {
    Clock,
    Todo,
    Done,
}

/// An opaque snapshot of durable task data for persistence adapters.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TaskState {
    pub(crate) todo: Vec<String>,
    pub(crate) done: Vec<String>,
}

impl TaskState {
    pub(crate) fn from_lists(todo: Vec<String>, done: Vec<String>) -> Self {
        Self { todo, done }
    }

    pub(crate) fn todo(&self) -> impl Iterator<Item = &str> {
        self.todo.iter().map(String::as_str)
    }

    pub(crate) fn done(&self) -> impl Iterator<Item = &str> {
        self.done.iter().map(String::as_str)
    }
}

/// The current task-entry context.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditMode {
    Normal,
    Adding,
    Editing { task_index: usize },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsMode {
    Closed,
    Navigating,
    EditingValue,
    CapturingKey,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TimerChange {
    Reset,
    Cycle,
    SelectSession(SessionKind),
    StartSession(SessionKind),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ConfirmationOperation {
    Quit,
    TimerChange(TimerChange),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PriorActivity {
    Running,
    Paused,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PendingConfirmation {
    operation: ConfirmationOperation,
    prior_activity: PriorActivity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PendingAutostart {
    session: SessionKind,
    remaining: Duration,
}

/// Runtime application state and terminal-independent state transitions.
#[derive(Debug)]
pub struct App {
    config: Config,
    timer: PomodoroTimer,
    tasks: TaskList,
    ui_focus: UiFocus,
    last_task_focus: UiFocus,
    todo_selection: usize,
    done_selection: usize,
    todo_offset: usize,
    done_offset: usize,
    edit_mode: EditMode,
    input: String,
    last_click: Option<(ClickTarget, Instant)>,
    pending_confirmation: Option<PendingConfirmation>,
    pending_autostart: Option<PendingAutostart>,
    completion_audio_active: bool,
    show_task_numbers: bool,
    settings: Option<SettingsOverlay>,
}

impl App {
    /// Creates an application with the current default durations and no tasks.
    pub fn new() -> Self {
        Self::from_config(&Config::default())
    }

    /// Creates an application using validated durable configuration.
    pub fn from_config(config: &Config) -> Self {
        Self::from_config_and_tasks(config, TaskState::default())
    }

    /// Creates an application using validated configuration and durable tasks.
    pub fn from_config_and_tasks(config: &Config, task_state: TaskState) -> Self {
        let timer = config.timer();
        Self {
            config: config.clone(),
            timer: PomodoroTimer::new(
                timer.focus_duration(),
                timer.short_break_duration(),
                timer.long_break_duration(),
                timer.long_break_interval(),
            ),
            tasks: TaskList::from_descriptions(task_state.todo, task_state.done),
            ui_focus: UiFocus::Clock,
            last_task_focus: UiFocus::Todo,
            todo_selection: 0,
            done_selection: 0,
            todo_offset: 0,
            done_offset: 0,
            edit_mode: EditMode::Normal,
            input: String::new(),
            last_click: None,
            pending_confirmation: None,
            pending_autostart: None,
            completion_audio_active: false,
            show_task_numbers: config.tasks().show_numbers(),
            settings: None,
        }
    }

    pub(crate) fn timer(&self) -> &PomodoroTimer {
        &self.timer
    }

    pub(crate) fn tasks(&self) -> &TaskList {
        &self.tasks
    }

    /// Captures the independently ordered to-do and done lists for persistence.
    pub fn task_state(&self) -> TaskState {
        TaskState::from_lists(
            self.tasks
                .pending()
                .map(|task| task.description().to_owned())
                .collect(),
            self.tasks
                .completed()
                .map(|task| task.description().to_owned())
                .collect(),
        )
    }

    /// Applies a semantic action without depending on its physical key mapping.
    pub fn dispatch(&mut self, action: Action) -> AppOutcome {
        let prior_timer_state = self.timer.state();
        if self.pending_confirmation.is_some() {
            let outcome = match action {
                Action::ConfirmPendingAction => self.confirm_pending_action(),
                Action::CancelPendingAction => {
                    self.cancel_pending_action();
                    AppOutcome::None
                }
                _ => AppOutcome::None,
            };
            return Self::timer_transition_outcome(prior_timer_state, self.timer.state(), outcome);
        }

        if self.settings.is_some() {
            return self.dispatch_settings(action);
        }

        if self.pending_autostart.is_some() {
            match action {
                Action::PrimaryAction if self.ui_focus == UiFocus::Clock => {
                    self.pending_autostart = None;
                    self.completion_audio_active = false;
                    self.timer.primary_action();
                    return Self::autostart_transition_outcome(
                        prior_timer_state,
                        self.timer.state(),
                    );
                }
                Action::CycleSession => {
                    self.pending_autostart = None;
                    self.completion_audio_active = false;
                    self.timer.cycle_ready_session();
                    return Self::autostart_transition_outcome(
                        prior_timer_state,
                        self.timer.state(),
                    );
                }
                Action::CancelPendingAction => {
                    self.pending_autostart = None;
                    self.completion_audio_active = false;
                    return Self::autostart_transition_outcome(
                        prior_timer_state,
                        self.timer.state(),
                    );
                }
                _ => {}
            }
        }

        let stop_completion_audio = self.completion_audio_active
            && (action == Action::CycleSession
                || action == Action::PrimaryAction && self.ui_focus == UiFocus::Clock);
        if stop_completion_audio {
            self.completion_audio_active = false;
        }

        match action {
            Action::Quit => {
                let outcome = self.request_quit();
                return Self::timer_transition_outcome(
                    prior_timer_state,
                    self.timer.state(),
                    outcome,
                );
            }
            Action::NavigateFocus(direction) => self.navigate_focus(direction),
            Action::Scroll(target, direction) => match target {
                ScrollTarget::Todo => {
                    self.focus(UiFocus::Todo);
                    self.move_todo_selection(direction);
                }
                ScrollTarget::Done => {
                    self.focus(UiFocus::Done);
                    self.move_done_selection(direction);
                }
                ScrollTarget::Settings => {}
            },
            Action::MoveSelection(direction) => match self.ui_focus {
                UiFocus::Clock => {}
                UiFocus::Todo => self.move_todo_selection(direction),
                UiFocus::Done => self.move_done_selection(direction),
            },
            Action::MoveSelectedTask(direction) => {
                if self.move_selected_task(direction) {
                    return AppOutcome::TasksChanged;
                }
            }
            Action::PrimaryAction => match self.ui_focus {
                UiFocus::Clock => self.clock_primary_action(),
                UiFocus::Todo => {
                    if self.complete_selected_todo() {
                        return AppOutcome::TasksChanged;
                    }
                }
                UiFocus::Done => {
                    if self.return_selected_done() {
                        return AppOutcome::TasksChanged;
                    }
                }
            },
            Action::CycleSession => self.cycle_session(),
            Action::ResetSession => self.reset_session(),
            Action::ConfirmPendingAction | Action::CancelPendingAction => {}
            Action::BeginAdd => self.begin_add(),
            Action::EditSelected => match self.ui_focus {
                UiFocus::Clock => {}
                UiFocus::Todo => self.edit_selected_todo(),
                UiFocus::Done => self.edit_selected_done(),
            },
            Action::DeleteSelected => match self.ui_focus {
                UiFocus::Clock => {}
                UiFocus::Todo => {
                    if self.delete_selected_todo() {
                        return AppOutcome::TasksChanged;
                    }
                }
                UiFocus::Done => {
                    if self.delete_selected_done() {
                        return AppOutcome::TasksChanged;
                    }
                }
            },
            Action::SubmitEdit => {
                if self.submit_edit() {
                    return AppOutcome::TasksChanged;
                }
            }
            Action::CancelEdit => self.cancel_edit(),
            Action::PushInput(character) => self.push_input(character),
            Action::PopInput => self.pop_input(),
            Action::OpenSettings => self.open_settings(),
            Action::SettingsMove(_)
            | Action::SettingsAdjust(_)
            | Action::SettingsActivate
            | Action::SettingsClose
            | Action::SettingsCancel
            | Action::SettingsPushInput(_)
            | Action::SettingsPopInput
            | Action::SettingsSubmitInput
            | Action::SettingsCaptureKey(_) => {}
        }

        let outcome =
            Self::timer_transition_outcome(prior_timer_state, self.timer.state(), AppOutcome::None);
        if stop_completion_audio {
            Self::with_completion_stop(outcome)
        } else {
            outcome
        }
    }

    /// Returns the area that receives contextual semantic actions.
    pub fn ui_focus(&self) -> UiFocus {
        self.ui_focus
    }

    /// Returns the task panel most recently focused by the user.
    pub(crate) fn last_task_focus(&self) -> UiFocus {
        self.last_task_focus
    }

    /// Returns the current text-entry context.
    pub fn edit_mode(&self) -> EditMode {
        self.edit_mode
    }

    /// Reports whether a confirmation owns keyboard and mouse input.
    pub fn is_confirmation_open(&self) -> bool {
        self.pending_confirmation.is_some()
    }

    pub fn is_settings_open(&self) -> bool {
        self.settings.is_some()
    }

    /// Returns the recommended session and displayed countdown while autostart is pending.
    pub fn pending_autostart(&self) -> Option<(SessionKind, u64)> {
        self.pending_autostart.map(|pending| {
            let seconds = pending.remaining.as_secs();
            let rounded_up = seconds + u64::from(pending.remaining.subsec_nanos() > 0);
            (pending.session, rounded_up)
        })
    }

    /// Reports whether Focus is actively counting down.
    pub fn is_focus_running(&self) -> bool {
        self.timer.state() == TimerState::Running(SessionKind::Focus)
    }

    pub fn settings_mode(&self) -> SettingsMode {
        match self.settings.as_ref() {
            None => SettingsMode::Closed,
            Some(settings) if settings.input().is_some() => SettingsMode::EditingValue,
            Some(settings) if settings.is_capturing_key() => SettingsMode::CapturingKey,
            Some(_) => SettingsMode::Navigating,
        }
    }

    pub(crate) fn settings(&self) -> Option<&SettingsOverlay> {
        self.settings.as_ref()
    }

    pub(crate) fn set_settings_offset(&mut self, offset: usize) {
        if let Some(settings) = &mut self.settings {
            settings.set_offset(offset);
        }
    }

    /// Returns the active keys, including changes accepted in the settings overlay.
    pub fn input_keys(&self) -> &crate::config::KeysConfig {
        self.settings
            .as_ref()
            .map_or(self.config.keys(), |settings| settings.config().keys())
    }

    pub(crate) fn pending_confirmation(&self) -> Option<ConfirmationOperation> {
        self.pending_confirmation
            .as_ref()
            .map(|pending| pending.operation.clone())
    }

    pub(crate) fn input(&self) -> &str {
        &self.input
    }

    pub(crate) fn todo_selection(&self) -> usize {
        self.todo_selection
    }

    pub(crate) fn done_selection(&self) -> usize {
        self.done_selection
    }

    pub(crate) fn todo_offset(&self) -> usize {
        self.todo_offset
    }

    pub(crate) fn done_offset(&self) -> usize {
        self.done_offset
    }

    pub(crate) fn set_offsets(&mut self, todo_offset: usize, done_offset: usize) {
        self.todo_offset = todo_offset;
        self.done_offset = done_offset;
    }

    pub(crate) fn show_task_numbers(&self) -> bool {
        self.show_task_numbers
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
