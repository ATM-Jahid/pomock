use std::time::{Duration, Instant};

use crate::{
    SessionKind,
    config::{Config, ConfigKey},
    settings::SettingsOverlay,
    tasks::TaskList,
    timer::{PomodoroTimer, TimerState},
};

const DOUBLE_CLICK_WINDOW: Duration = Duration::from_millis(500);
const PROGRESS_CONFIRMATION_THRESHOLD: Duration = Duration::from_secs(10);
const AUTOSTART_DELAY: Duration = Duration::from_secs(5);

/// The application area that currently receives contextual commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiFocus {
    Clock,
    Todo,
    Done,
}

/// A semantic navigation direction, independent of its physical key mapping.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Left,
    Down,
    Up,
    Right,
}

/// A user intention after terminal input has been translated from a physical key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Quit,
    NavigateFocus(Direction),
    MoveSelection(Direction),
    MoveSelectedTask(Direction),
    PrimaryAction,
    CycleSession,
    ResetSession,
    ConfirmPendingAction,
    CancelPendingAction,
    BeginAdd,
    EditSelected,
    DeleteSelected,
    SubmitEdit,
    CancelEdit,
    PushInput(char),
    PopInput,
    OpenSettings,
    SettingsMove(bool),
    SettingsAdjust(bool),
    SettingsActivate,
    SettingsClose,
    SettingsCancel,
    SettingsPushInput(char),
    SettingsPopInput,
    SettingsSubmitInput,
    SettingsCaptureKey(ConfigKey),
    Scroll(ScrollTarget, Direction),
}

/// A list that can be scrolled by a pointing device.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollTarget {
    Todo,
    Done,
    Settings,
}

/// A boundary-relevant result of applying an application transition.
#[must_use]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppOutcome {
    None,
    Quit,
    FocusAudio(FocusAudioAction),
    TimerEffects {
        focus_audio: Option<FocusAudioAction>,
        stop_completion_audio: bool,
    },
    SessionCompleted(SessionKind),
    TasksChanged,
    SettingsChanged(Box<Config>),
}

/// A lifecycle operation for the optional looping Focus audio.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusAudioAction {
    StartOrResume,
    Pause,
    Stop,
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

impl UiFocus {
    fn navigate(self, direction: Direction) -> Self {
        match (self, direction) {
            (Self::Clock, Direction::Down) => Self::Todo,
            (Self::Todo | Self::Done, Direction::Up) => Self::Clock,
            (Self::Todo, Direction::Right) => Self::Done,
            (Self::Done, Direction::Left) => Self::Todo,
            _ => self,
        }
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

    /// Advances monotonic application time and reports a completed session.
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

    fn autostart_transition_outcome(before: TimerState, after: TimerState) -> AppOutcome {
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

    fn with_completion_stop(outcome: AppOutcome) -> AppOutcome {
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

    fn timer_transition_outcome(
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

    /// Returns the area that receives contextual semantic actions.
    pub fn ui_focus(&self) -> UiFocus {
        self.ui_focus
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

    fn focus(&mut self, focus: UiFocus) {
        self.ui_focus = focus;
    }

    fn open_settings(&mut self) {
        self.settings = Some(SettingsOverlay::new(&self.config));
        self.clear_pending_click();
    }

    fn dispatch_settings(&mut self, action: Action) -> AppOutcome {
        let mut close = false;
        {
            let settings = self.settings.as_mut().expect("settings overlay is open");
            match action {
                Action::SettingsMove(down) => settings.move_selection(down),
                Action::Scroll(ScrollTarget::Settings, Direction::Down) => {
                    settings.move_selection(true)
                }
                Action::Scroll(ScrollTarget::Settings, Direction::Up) => {
                    settings.move_selection(false)
                }
                Action::SettingsAdjust(forward) => settings.adjust(forward),
                Action::SettingsActivate => settings.activate(),
                Action::SettingsClose => {
                    close = settings.input().is_none() && !settings.is_capturing_key();
                }
                Action::SettingsPushInput(character) => settings.push_input(character),
                Action::SettingsPopInput => settings.pop_input(),
                Action::SettingsSubmitInput => settings.submit_input(),
                Action::SettingsCaptureKey(key) => settings.capture_key(key),
                Action::SettingsCancel => {
                    settings.cancel_nested();
                }
                _ => {}
            }
        }

        if close {
            self.settings = None;
            self.clear_pending_click();
            return AppOutcome::None;
        }

        let updated = self
            .settings
            .as_ref()
            .filter(|settings| settings.config() != &self.config)
            .map(|settings| settings.config().clone());
        updated.map_or(AppOutcome::None, |config| self.apply_settings(config))
    }

    fn apply_settings(&mut self, config: Config) -> AppOutcome {
        if config.timer() != self.config.timer() {
            let timer = config.timer();
            self.timer.reconfigure(
                timer.focus_duration(),
                timer.short_break_duration(),
                timer.long_break_duration(),
                timer.long_break_interval(),
            );
        }
        self.show_task_numbers = config.tasks().show_numbers();
        self.config = config.clone();
        AppOutcome::SettingsChanged(Box::new(config))
    }

    fn navigate_focus(&mut self, direction: Direction) {
        self.ui_focus = self.ui_focus.navigate(direction);
    }

    fn select_todo(&mut self, selection: usize) {
        self.todo_selection = selection;
    }

    fn select_done(&mut self, selection: usize) {
        self.done_selection = selection;
    }

    fn begin_add(&mut self) {
        if !matches!(self.ui_focus, UiFocus::Todo | UiFocus::Done) {
            return;
        }

        self.input.clear();
        self.edit_mode = EditMode::Adding;
    }

    fn cancel_edit(&mut self) {
        self.input.clear();
        self.edit_mode = EditMode::Normal;
    }

    fn submit_edit(&mut self) -> bool {
        let description = std::mem::take(&mut self.input);

        let changed = match self.edit_mode {
            EditMode::Adding if !description.trim().is_empty() => {
                if self.ui_focus == UiFocus::Done {
                    self.tasks.add_completed(description);
                } else {
                    self.tasks.add(description);
                }
                true
            }
            EditMode::Editing { task_index } => match self.ui_focus {
                UiFocus::Todo => self.tasks.edit_pending(task_index, description),
                UiFocus::Done => self.tasks.edit_completed(task_index, description),
                UiFocus::Clock => false,
            },
            EditMode::Adding | EditMode::Normal => false,
        };

        self.edit_mode = EditMode::Normal;
        self.clamp_selections();
        changed
    }

    fn push_input(&mut self, character: char) {
        self.input.push(character);
    }

    fn pop_input(&mut self) {
        self.input.pop();
    }

    fn clock_primary_action(&mut self) {
        self.timer.primary_action();
    }

    fn cycle_session(&mut self) {
        self.request_timer_change(TimerChange::Cycle);
    }

    fn reset_session(&mut self) {
        self.request_timer_change(TimerChange::Reset);
    }

    fn request_quit(&mut self) -> AppOutcome {
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

    fn request_timer_change(&mut self, change: TimerChange) {
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

    fn confirm_pending_action(&mut self) -> AppOutcome {
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

    fn cancel_pending_action(&mut self) {
        let resume = self
            .pending_confirmation
            .take()
            .is_some_and(|pending| pending.prior_activity == PriorActivity::Running);
        if resume {
            self.timer.resume();
        }
        self.clear_pending_click();
    }

    fn move_todo_selection(&mut self, direction: Direction) {
        let len = self.tasks.pending().count();
        Self::move_selection(&mut self.todo_selection, len, direction);
    }

    fn move_done_selection(&mut self, direction: Direction) {
        let len = self.tasks.completed().count();
        Self::move_selection(&mut self.done_selection, len, direction);
    }

    fn move_selected_task(&mut self, direction: Direction) -> bool {
        match (self.ui_focus, direction) {
            (UiFocus::Todo, Direction::Up) => {
                let changed = self.tasks.move_pending_up(self.todo_selection);
                if changed {
                    self.todo_selection -= 1;
                    self.todo_offset = self.todo_offset.min(self.todo_selection);
                }
                changed
            }
            (UiFocus::Todo, Direction::Down) => {
                let changed = self.tasks.move_pending_down(self.todo_selection);
                if changed {
                    self.todo_selection += 1;
                }
                changed
            }
            (UiFocus::Done, Direction::Up) => {
                let changed = self.tasks.move_completed_up(self.done_selection);
                if changed {
                    self.done_selection -= 1;
                    self.done_offset = self.done_offset.min(self.done_selection);
                }
                changed
            }
            (UiFocus::Done, Direction::Down) => {
                let changed = self.tasks.move_completed_down(self.done_selection);
                if changed {
                    self.done_selection += 1;
                }
                changed
            }
            (UiFocus::Clock, _) | (_, Direction::Left | Direction::Right) => false,
        }
    }

    fn edit_selected_todo(&mut self) {
        let description = self
            .tasks
            .pending()
            .nth(self.todo_selection)
            .map(|task| task.description().to_string());
        if let Some(description) = description {
            self.begin_edit(self.todo_selection, description);
        }
    }

    fn edit_selected_done(&mut self) {
        let description = self
            .tasks
            .completed()
            .nth(self.done_selection)
            .map(|task| task.description().to_string());
        if let Some(description) = description {
            self.begin_edit(self.done_selection, description);
        }
    }

    fn delete_selected_todo(&mut self) -> bool {
        if self.tasks.pending().nth(self.todo_selection).is_some() {
            let changed = self.tasks.delete_pending(self.todo_selection);
            self.clamp_selections();
            return changed;
        }
        false
    }

    fn delete_selected_done(&mut self) -> bool {
        if self.tasks.completed().nth(self.done_selection).is_some() {
            let changed = self.tasks.delete_completed(self.done_selection);
            self.clamp_selections();
            return changed;
        }
        false
    }

    fn complete_selected_todo(&mut self) -> bool {
        if self.tasks.pending().nth(self.todo_selection).is_some() {
            let changed = self.tasks.complete(self.todo_selection);
            self.clamp_selections();
            return changed;
        }
        false
    }

    fn return_selected_done(&mut self) -> bool {
        if self.tasks.completed().nth(self.done_selection).is_some() {
            let changed = self.tasks.uncomplete(self.done_selection);
            self.clamp_selections();
            return changed;
        }
        false
    }

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

    fn clear_pending_click(&mut self) {
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

    fn move_selection(selection: &mut usize, len: usize, direction: Direction) {
        if len == 0 {
            *selection = 0;
            return;
        }

        match direction {
            Direction::Left | Direction::Up => {
                *selection = selection.saturating_sub(1);
            }
            Direction::Down | Direction::Right => {
                *selection = (*selection + 1).min(len - 1);
            }
        }
    }

    fn clamp_selections(&mut self) {
        let pending_len = self.tasks.pending().count();
        let completed_len = self.tasks.completed().count();
        self.todo_selection = self.todo_selection.min(pending_len.saturating_sub(1));
        self.done_selection = self.done_selection.min(completed_len.saturating_sub(1));
        self.todo_offset = self.todo_offset.min(self.todo_selection);
        self.done_offset = self.done_offset.min(self.done_selection);
    }

    fn begin_edit(&mut self, task_index: usize, description: String) {
        self.input = description;
        self.edit_mode = EditMode::Editing { task_index };
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use crate::{
        config::{Config, ConfigKey, KeyAction, TasksConfig, TimerConfig},
        settings::SettingField,
        timer::{SessionKind, TimerState},
    };

    use super::{
        Action, App, AppOutcome, ClickTarget, ConfirmationOperation, Direction, EditMode,
        FocusAudioAction, ScrollTarget, SettingsMode, TaskState, TimerChange, UiFocus,
    };

    fn add_task(app: &mut App, description: &str) {
        let _ = app.dispatch(Action::NavigateFocus(Direction::Down));
        let _ = app.dispatch(Action::BeginAdd);
        for character in description.chars() {
            let _ = app.dispatch(Action::PushInput(character));
        }
        let _ = app.dispatch(Action::SubmitEdit);
        let _ = app.dispatch(Action::NavigateFocus(Direction::Up));
    }

    fn move_settings_to(app: &mut App, field: SettingField) {
        while app.settings().unwrap().field() != field {
            let _ = app.dispatch(Action::SettingsMove(true));
        }
    }

    fn double_click_session(app: &mut App, session: SessionKind, first_click: Instant) {
        let target = ClickTarget::SessionControl(session);
        let _ = app.handle_click_target(target, first_click);
        let _ = app.handle_click_target(target, first_click + Duration::from_millis(100));
    }

    fn active_focus(progress: Duration, pause: bool) -> App {
        let mut app = App::new();
        let _ = app.dispatch(Action::PrimaryAction);
        let _ = app.tick(progress);
        if pause {
            let _ = app.dispatch(Action::PrimaryAction);
        }
        app
    }

    #[test]
    fn configured_durations_and_interval_drive_the_timer() {
        let config = Config::new(TimerConfig::new(2, 1, 3, 2).unwrap()).unwrap();
        let mut app = App::from_config(&config);

        assert_eq!(app.timer().remaining(), Duration::from_secs(2 * 60));

        for expected_next in [SessionKind::ShortBreak, SessionKind::LongBreak] {
            let _ = app.dispatch(Action::PrimaryAction);
            assert_eq!(
                app.tick(Duration::from_secs(2 * 60)),
                AppOutcome::SessionCompleted(SessionKind::Focus)
            );
            assert_eq!(app.timer().state(), TimerState::Ready(expected_next));
            let _ = app.dispatch(Action::CycleSession);
            if expected_next == SessionKind::ShortBreak {
                let _ = app.dispatch(Action::CycleSession);
            }
        }
    }

    #[test]
    fn configured_task_numbering_is_available_to_the_ui() {
        assert!(App::new().show_task_numbers());

        let config = Config::with_tasks(
            TimerConfig::default(),
            TasksConfig::with_numbering(true, false),
        )
        .unwrap();

        assert!(!App::from_config(&config).show_task_numbers());
    }

    #[test]
    fn durable_task_state_restores_order_and_completion() {
        let state = TaskState::from_lists(
            vec!["Pending first".to_owned(), "Pending second".to_owned()],
            vec!["Completed first".to_owned()],
        );

        let app = App::from_config_and_tasks(&Config::default(), state.clone());

        assert_eq!(app.task_state(), state);
        assert_eq!(
            app.tasks().completed().next().unwrap().description(),
            "Completed first"
        );
        assert_eq!(
            app.tasks().pending().next().unwrap().description(),
            "Pending first"
        );
    }

    #[test]
    fn navigates_between_adjacent_areas() {
        assert_eq!(UiFocus::Clock.navigate(Direction::Down), UiFocus::Todo);
        assert_eq!(UiFocus::Todo.navigate(Direction::Up), UiFocus::Clock);
        assert_eq!(UiFocus::Todo.navigate(Direction::Right), UiFocus::Done);
        assert_eq!(UiFocus::Done.navigate(Direction::Left), UiFocus::Todo);
        assert_eq!(UiFocus::Done.navigate(Direction::Up), UiFocus::Clock);
    }

    #[test]
    fn ignores_directions_without_an_adjacent_area() {
        assert_eq!(UiFocus::Clock.navigate(Direction::Left), UiFocus::Clock);
        assert_eq!(UiFocus::Clock.navigate(Direction::Up), UiFocus::Clock);
        assert_eq!(UiFocus::Clock.navigate(Direction::Right), UiFocus::Clock);
        assert_eq!(UiFocus::Todo.navigate(Direction::Left), UiFocus::Todo);
        assert_eq!(UiFocus::Todo.navigate(Direction::Down), UiFocus::Todo);
        assert_eq!(UiFocus::Done.navigate(Direction::Down), UiFocus::Done);
        assert_eq!(UiFocus::Done.navigate(Direction::Right), UiFocus::Done);
    }

    #[test]
    fn dispatches_focus_and_contextual_selection_actions() {
        let mut app = App::new();
        add_task(&mut app, "First");
        add_task(&mut app, "Second");

        let _ = app.dispatch(Action::NavigateFocus(Direction::Down));
        let _ = app.dispatch(Action::MoveSelection(Direction::Down));
        assert_eq!(app.ui_focus(), UiFocus::Todo);
        assert_eq!(app.todo_selection(), 1);

        assert_eq!(
            app.dispatch(Action::PrimaryAction),
            AppOutcome::TasksChanged
        );
        assert_eq!(app.tasks().pending().count(), 1);
        assert_eq!(app.tasks().completed().count(), 1);
    }

    #[test]
    fn dispatch_reports_only_boundary_relevant_outcomes() {
        let mut app = App::new();

        assert_eq!(
            app.dispatch(Action::NavigateFocus(Direction::Down)),
            AppOutcome::None
        );
        assert_eq!(app.dispatch(Action::Quit), AppOutcome::Quit);
    }

    #[test]
    fn focus_audio_lifecycle_follows_timer_transitions_and_confirmations() {
        let mut app = App::new();

        assert_eq!(
            app.dispatch(Action::PrimaryAction),
            AppOutcome::FocusAudio(FocusAudioAction::StartOrResume)
        );
        assert_eq!(
            app.dispatch(Action::PrimaryAction),
            AppOutcome::FocusAudio(FocusAudioAction::Pause)
        );
        assert_eq!(
            app.dispatch(Action::PrimaryAction),
            AppOutcome::FocusAudio(FocusAudioAction::StartOrResume)
        );
        let _ = app.tick(Duration::from_secs(10));
        assert_eq!(
            app.dispatch(Action::CycleSession),
            AppOutcome::FocusAudio(FocusAudioAction::Pause)
        );
        assert_eq!(
            app.dispatch(Action::CancelPendingAction),
            AppOutcome::FocusAudio(FocusAudioAction::StartOrResume)
        );
        assert_eq!(
            app.dispatch(Action::CycleSession),
            AppOutcome::FocusAudio(FocusAudioAction::Pause)
        );
        assert_eq!(
            app.dispatch(Action::ConfirmPendingAction),
            AppOutcome::FocusAudio(FocusAudioAction::Stop)
        );
    }

    #[test]
    fn focus_audio_lifecycle_is_emitted_for_mouse_timer_controls() {
        let mut app = App::new();
        let now = Instant::now();

        assert_eq!(
            app.handle_click_target(ClickTarget::Clock, now),
            AppOutcome::None
        );
        assert_eq!(
            app.handle_click_target(ClickTarget::Clock, now + Duration::from_millis(100)),
            AppOutcome::FocusAudio(FocusAudioAction::StartOrResume)
        );
        let target = ClickTarget::SessionControl(SessionKind::Focus);
        assert_eq!(
            app.handle_click_target(target, now + Duration::from_millis(200)),
            AppOutcome::None
        );
        assert_eq!(
            app.handle_click_target(target, now + Duration::from_millis(300)),
            AppOutcome::FocusAudio(FocusAudioAction::Pause)
        );
    }

    #[test]
    fn quit_below_ten_seconds_is_immediate_for_running_and_paused_sessions() {
        for initially_paused in [false, true] {
            let mut app = active_focus(Duration::from_secs(9), initially_paused);

            assert_eq!(app.dispatch(Action::Quit), AppOutcome::Quit);
            assert!(!app.is_confirmation_open());
        }
    }

    #[test]
    fn quit_at_ten_seconds_pauses_and_requests_confirmation() {
        for initially_paused in [false, true] {
            let mut app = active_focus(Duration::from_secs(10), initially_paused);

            let expected = if initially_paused {
                AppOutcome::None
            } else {
                AppOutcome::FocusAudio(FocusAudioAction::Pause)
            };
            assert_eq!(app.dispatch(Action::Quit), expected);

            assert!(app.is_confirmation_open());
            assert_eq!(
                app.pending_confirmation(),
                Some(ConfirmationOperation::Quit)
            );
            assert_eq!(app.timer().state(), TimerState::Paused(SessionKind::Focus));
            assert_eq!(app.timer().remaining(), Duration::from_secs(25 * 60 - 10));
        }
    }

    #[test]
    fn confirming_quit_emits_quit_outcome() {
        let mut app = active_focus(Duration::from_secs(10), false);
        assert_eq!(
            app.dispatch(Action::Quit),
            AppOutcome::FocusAudio(FocusAudioAction::Pause)
        );

        assert_eq!(app.dispatch(Action::ConfirmPendingAction), AppOutcome::Quit);
        assert!(!app.is_confirmation_open());
    }

    #[test]
    fn cancelling_quit_restores_running_but_preserves_paused() {
        for initially_paused in [false, true] {
            let mut app = active_focus(Duration::from_secs(10), initially_paused);
            let _ = app.dispatch(Action::Quit);

            let expected_outcome = if initially_paused {
                AppOutcome::None
            } else {
                AppOutcome::FocusAudio(FocusAudioAction::StartOrResume)
            };
            assert_eq!(app.dispatch(Action::CancelPendingAction), expected_outcome);

            let expected = if initially_paused {
                TimerState::Paused(SessionKind::Focus)
            } else {
                TimerState::Running(SessionKind::Focus)
            };
            assert_eq!(app.timer().state(), expected);
            assert_eq!(app.timer().remaining(), Duration::from_secs(25 * 60 - 10));
        }
    }

    #[test]
    fn tick_reports_each_session_completion_exactly_once() {
        for (session, duration) in [
            (SessionKind::Focus, Duration::from_secs(25 * 60)),
            (SessionKind::ShortBreak, Duration::from_secs(5 * 60)),
            (SessionKind::LongBreak, Duration::from_secs(15 * 60)),
        ] {
            let mut app = App::new();
            if session == SessionKind::Focus {
                let _ = app.dispatch(Action::PrimaryAction);
            } else {
                double_click_session(&mut app, session, Instant::now());
            }

            assert_eq!(app.tick(duration), AppOutcome::SessionCompleted(session));
            assert_eq!(app.tick(Duration::from_secs(1)), AppOutcome::None);
        }
    }

    fn autostart_app(breaks: bool, focus: bool) -> App {
        let timer = TimerConfig::default().with_autostart(breaks, focus);
        App::from_config(&Config::new(timer).unwrap())
    }

    #[test]
    fn configured_break_autostart_counts_down_and_starts_the_recommendation() {
        let mut app = autostart_app(true, false);
        let _ = app.dispatch(Action::PrimaryAction);

        assert_eq!(
            app.tick(Duration::from_secs(25 * 60)),
            AppOutcome::SessionCompleted(SessionKind::Focus)
        );
        assert_eq!(app.pending_autostart(), Some((SessionKind::ShortBreak, 5)));
        assert_eq!(app.tick(Duration::from_millis(4_999)), AppOutcome::None);
        assert_eq!(app.pending_autostart(), Some((SessionKind::ShortBreak, 1)));
        assert_eq!(
            app.tick(Duration::from_millis(1)),
            AppOutcome::TimerEffects {
                focus_audio: None,
                stop_completion_audio: true,
            }
        );
        assert_eq!(
            app.timer().state(),
            TimerState::Running(SessionKind::ShortBreak)
        );
    }

    #[test]
    fn focus_autostart_is_independent_from_break_autostart() {
        let mut app = autostart_app(false, true);
        double_click_session(&mut app, SessionKind::ShortBreak, Instant::now());

        assert_eq!(
            app.tick(Duration::from_secs(5 * 60)),
            AppOutcome::SessionCompleted(SessionKind::ShortBreak)
        );
        assert_eq!(app.pending_autostart(), Some((SessionKind::Focus, 5)));
        assert_eq!(
            app.tick(Duration::from_secs(5)),
            AppOutcome::TimerEffects {
                focus_audio: Some(FocusAudioAction::StartOrResume),
                stop_completion_audio: true,
            }
        );
        assert_eq!(app.timer().state(), TimerState::Running(SessionKind::Focus));
    }

    #[test]
    fn primary_starts_pending_session_while_escape_and_cycle_cancel_it() {
        let mut immediate = autostart_app(true, false);
        let _ = immediate.dispatch(Action::PrimaryAction);
        let _ = immediate.tick(Duration::from_secs(25 * 60));
        assert_eq!(
            immediate.dispatch(Action::PrimaryAction),
            AppOutcome::TimerEffects {
                focus_audio: None,
                stop_completion_audio: true,
            }
        );
        assert_eq!(
            immediate.timer().state(),
            TimerState::Running(SessionKind::ShortBreak)
        );

        let mut cancelled = autostart_app(true, false);
        let _ = cancelled.dispatch(Action::PrimaryAction);
        let _ = cancelled.tick(Duration::from_secs(25 * 60));
        let _ = cancelled.dispatch(Action::CancelPendingAction);
        assert_eq!(cancelled.pending_autostart(), None);
        assert_eq!(
            cancelled.timer().state(),
            TimerState::Ready(SessionKind::ShortBreak)
        );

        let mut cycled = autostart_app(true, false);
        let _ = cycled.dispatch(Action::PrimaryAction);
        let _ = cycled.tick(Duration::from_secs(25 * 60));
        let _ = cycled.dispatch(Action::CycleSession);
        assert_eq!(cycled.pending_autostart(), None);
        assert_eq!(
            cycled.timer().state(),
            TimerState::Ready(SessionKind::LongBreak)
        );
    }

    #[test]
    fn manual_start_stops_completion_audio_without_autostart() {
        let mut app = App::new();
        let _ = app.dispatch(Action::PrimaryAction);
        let _ = app.tick(Duration::from_secs(25 * 60));

        assert_eq!(
            app.dispatch(Action::PrimaryAction),
            AppOutcome::TimerEffects {
                focus_audio: None,
                stop_completion_audio: true,
            }
        );
    }

    #[test]
    fn selecting_another_session_cancels_autostart_and_leaves_it_ready() {
        let mut app = autostart_app(true, false);
        let _ = app.dispatch(Action::PrimaryAction);
        let _ = app.tick(Duration::from_secs(25 * 60));

        assert_eq!(
            app.handle_click_target(
                ClickTarget::SessionControl(SessionKind::LongBreak),
                Instant::now()
            ),
            AppOutcome::TimerEffects {
                focus_audio: None,
                stop_completion_audio: true,
            }
        );
        assert_eq!(app.pending_autostart(), None);
        assert_eq!(
            app.timer().state(),
            TimerState::Ready(SessionKind::LongBreak)
        );
    }

    #[test]
    fn double_clicking_clock_starts_pending_session_immediately() {
        let mut app = autostart_app(true, false);
        let _ = app.dispatch(Action::PrimaryAction);
        let _ = app.tick(Duration::from_secs(25 * 60));
        let now = Instant::now();

        assert_eq!(
            app.handle_click_target(ClickTarget::Clock, now),
            AppOutcome::None
        );
        assert_eq!(app.pending_autostart(), Some((SessionKind::ShortBreak, 5)));
        assert_eq!(
            app.handle_click_target(ClickTarget::Clock, now + Duration::from_millis(100)),
            AppOutcome::TimerEffects {
                focus_audio: None,
                stop_completion_audio: true,
            }
        );
        assert_eq!(app.pending_autostart(), None);
        assert_eq!(
            app.timer().state(),
            TimerState::Running(SessionKind::ShortBreak)
        );
    }

    #[test]
    fn dispatches_editing_actions_without_physical_key_codes() {
        let mut app = App::new();
        let _ = app.dispatch(Action::NavigateFocus(Direction::Down));

        let _ = app.dispatch(Action::BeginAdd);
        let _ = app.dispatch(Action::PushInput('a'));
        let _ = app.dispatch(Action::PushInput('b'));
        let _ = app.dispatch(Action::PopInput);
        assert_eq!(app.dispatch(Action::SubmitEdit), AppOutcome::TasksChanged);

        assert_eq!(app.tasks().pending().next().unwrap().description(), "a");
        assert_eq!(app.edit_mode(), EditMode::Normal);
    }

    #[test]
    fn begin_add_action_works_from_task_list_focus() {
        let mut app = App::new();
        let _ = app.dispatch(Action::BeginAdd);
        assert_eq!(app.edit_mode(), EditMode::Normal);

        let _ = app.dispatch(Action::NavigateFocus(Direction::Down));
        let _ = app.dispatch(Action::BeginAdd);
        assert_eq!(app.edit_mode(), EditMode::Adding);

        let _ = app.dispatch(Action::CancelEdit);
        let _ = app.dispatch(Action::NavigateFocus(Direction::Right));
        let _ = app.dispatch(Action::BeginAdd);
        assert_eq!(app.edit_mode(), EditMode::Adding);
    }

    #[test]
    fn adding_from_done_focus_creates_a_completed_task() {
        let mut app = App::new();
        let _ = app.dispatch(Action::NavigateFocus(Direction::Down));
        let _ = app.dispatch(Action::NavigateFocus(Direction::Right));
        let _ = app.dispatch(Action::BeginAdd);
        for character in "New task".chars() {
            let _ = app.dispatch(Action::PushInput(character));
        }

        assert_eq!(app.dispatch(Action::SubmitEdit), AppOutcome::TasksChanged);
        assert_eq!(app.tasks().pending().count(), 0);
        assert_eq!(
            app.tasks().completed().next().unwrap().description(),
            "New task"
        );
    }

    #[test]
    fn submitting_and_cancelling_add_update_state() {
        let mut app = App::new();
        let _ = app.dispatch(Action::NavigateFocus(Direction::Down));
        let _ = app.dispatch(Action::BeginAdd);
        for character in "Write tests".chars() {
            let _ = app.dispatch(Action::PushInput(character));
        }
        let _ = app.dispatch(Action::SubmitEdit);

        assert_eq!(app.edit_mode(), EditMode::Normal);
        assert_eq!(
            app.tasks().pending().next().unwrap().description(),
            "Write tests"
        );

        let _ = app.dispatch(Action::BeginAdd);
        let _ = app.dispatch(Action::PushInput('x'));
        let _ = app.dispatch(Action::CancelEdit);
        assert!(app.input().is_empty());
        assert_eq!(app.tasks().pending().count(), 1);
    }

    #[test]
    fn blank_or_contextless_submissions_do_not_report_task_changes() {
        let mut app = App::new();

        assert_eq!(app.dispatch(Action::SubmitEdit), AppOutcome::None);
        let _ = app.dispatch(Action::NavigateFocus(Direction::Down));
        let _ = app.dispatch(Action::BeginAdd);
        let _ = app.dispatch(Action::PushInput(' '));

        assert_eq!(app.dispatch(Action::SubmitEdit), AppOutcome::None);
        assert_eq!(app.task_state(), TaskState::default());
    }

    #[test]
    fn row_navigation_stays_within_tasks_and_handles_empty_lists() {
        let mut app = App::new();
        let _ = app.dispatch(Action::NavigateFocus(Direction::Down));
        let _ = app.dispatch(Action::MoveSelection(Direction::Down));
        assert_eq!(app.todo_selection(), 0);

        let _ = app.dispatch(Action::BeginAdd);
        let _ = app.dispatch(Action::PushInput('1'));
        assert_eq!(app.dispatch(Action::SubmitEdit), AppOutcome::TasksChanged);
        let _ = app.dispatch(Action::BeginAdd);
        let _ = app.dispatch(Action::PushInput('2'));
        let _ = app.dispatch(Action::SubmitEdit);
        let _ = app.dispatch(Action::MoveSelection(Direction::Down));
        let _ = app.dispatch(Action::MoveSelection(Direction::Down));
        assert_eq!(app.todo_selection(), 1);
        let _ = app.dispatch(Action::MoveSelection(Direction::Up));
        assert_eq!(app.todo_selection(), 0);
    }

    #[test]
    fn moving_selected_tasks_reorders_each_list_and_keeps_the_item_selected() {
        let state = TaskState::from_lists(
            vec!["Todo first".to_string(), "Todo selected".to_string()],
            vec!["Done selected".to_string(), "Done second".to_string()],
        );
        let mut app = App::from_config_and_tasks(&Config::default(), state);
        let _ = app.dispatch(Action::NavigateFocus(Direction::Down));
        let _ = app.dispatch(Action::MoveSelection(Direction::Down));

        assert_eq!(
            app.dispatch(Action::MoveSelectedTask(Direction::Up)),
            AppOutcome::TasksChanged
        );
        assert_eq!(app.todo_selection(), 0);
        assert_eq!(
            app.tasks()
                .pending()
                .map(|task| task.description())
                .collect::<Vec<_>>(),
            ["Todo selected", "Todo first"]
        );
        assert_eq!(
            app.dispatch(Action::MoveSelectedTask(Direction::Up)),
            AppOutcome::None
        );
        assert_eq!(app.todo_selection(), 0);

        let _ = app.dispatch(Action::NavigateFocus(Direction::Right));
        assert_eq!(
            app.dispatch(Action::MoveSelectedTask(Direction::Down)),
            AppOutcome::TasksChanged
        );
        assert_eq!(app.done_selection(), 1);
        assert_eq!(
            app.tasks()
                .completed()
                .map(|task| task.description())
                .collect::<Vec<_>>(),
            ["Done second", "Done selected"]
        );
        assert_eq!(
            app.dispatch(Action::MoveSelectedTask(Direction::Down)),
            AppOutcome::None
        );
        assert_eq!(app.done_selection(), 1);
    }

    #[test]
    fn scrolling_task_lists_focuses_the_target_and_moves_its_selection() {
        let mut app = App::new();
        add_task(&mut app, "First");
        add_task(&mut app, "Second");

        let _ = app.dispatch(Action::Scroll(ScrollTarget::Todo, Direction::Down));
        assert_eq!(app.ui_focus(), UiFocus::Todo);
        assert_eq!(app.todo_selection(), 1);

        let _ = app.dispatch(Action::Scroll(ScrollTarget::Todo, Direction::Up));
        assert_eq!(app.todo_selection(), 0);
        let _ = app.dispatch(Action::Scroll(ScrollTarget::Done, Direction::Down));
        assert_eq!(app.ui_focus(), UiFocus::Done);
        assert_eq!(app.done_selection(), 0);
    }

    #[test]
    fn scrolling_settings_moves_selection_but_is_locked_during_editing() {
        let mut app = App::new();
        let _ = app.dispatch(Action::OpenSettings);

        let _ = app.dispatch(Action::Scroll(ScrollTarget::Settings, Direction::Down));
        assert_eq!(app.settings().unwrap().selection(), 1);
        let _ = app.dispatch(Action::Scroll(ScrollTarget::Settings, Direction::Up));
        assert_eq!(app.settings().unwrap().selection(), 0);

        let _ = app.dispatch(Action::SettingsActivate);
        let _ = app.dispatch(Action::Scroll(ScrollTarget::Settings, Direction::Down));
        assert_eq!(app.settings().unwrap().selection(), 0);
    }

    #[test]
    fn editing_a_selected_todo_updates_that_list_entry() {
        let mut app = App::new();
        add_task(&mut app, "Done");
        let _ = app.dispatch(Action::NavigateFocus(Direction::Down));
        assert_eq!(
            app.dispatch(Action::PrimaryAction),
            AppOutcome::TasksChanged
        );
        let _ = app.dispatch(Action::NavigateFocus(Direction::Up));
        add_task(&mut app, "Edit me");
        let _ = app.dispatch(Action::NavigateFocus(Direction::Down));
        let _ = app.dispatch(Action::EditSelected);
        assert_eq!(app.edit_mode(), EditMode::Editing { task_index: 0 });

        while !app.input().is_empty() {
            let _ = app.dispatch(Action::PopInput);
        }
        for character in "Edited".chars() {
            let _ = app.dispatch(Action::PushInput(character));
        }
        let _ = app.dispatch(Action::SubmitEdit);

        assert_eq!(
            app.tasks().pending().next().unwrap().description(),
            "Edited"
        );
    }

    #[test]
    fn complete_return_and_delete_clamp_selections() {
        let mut app = App::new();
        add_task(&mut app, "First");
        add_task(&mut app, "Second");
        let _ = app.dispatch(Action::NavigateFocus(Direction::Down));
        let _ = app.dispatch(Action::MoveSelection(Direction::Down));
        assert_eq!(
            app.dispatch(Action::PrimaryAction),
            AppOutcome::TasksChanged
        );
        assert_eq!(app.todo_selection(), 0);
        assert_eq!(
            app.tasks().completed().next().unwrap().description(),
            "Second"
        );

        let _ = app.dispatch(Action::NavigateFocus(Direction::Right));
        let _ = app.dispatch(Action::PrimaryAction);
        assert_eq!(app.done_selection(), 0);
        assert_eq!(app.tasks().completed().count(), 0);

        let _ = app.dispatch(Action::NavigateFocus(Direction::Left));
        let _ = app.dispatch(Action::MoveSelection(Direction::Down));
        assert_eq!(
            app.dispatch(Action::DeleteSelected),
            AppOutcome::TasksChanged
        );
        assert_eq!(app.todo_selection(), 0);
        assert_eq!(app.tasks().pending().count(), 1);
    }

    #[test]
    fn moving_tasks_appends_them_to_the_destination_view() {
        let state = TaskState::from_lists(
            vec!["Todo first".to_string(), "Todo second".to_string()],
            vec!["Done first".to_string()],
        );
        let mut app = App::from_config_and_tasks(&Config::default(), state);
        let _ = app.dispatch(Action::NavigateFocus(Direction::Down));

        assert_eq!(
            app.dispatch(Action::PrimaryAction),
            AppOutcome::TasksChanged
        );
        assert_eq!(
            app.tasks()
                .completed()
                .map(|task| task.description())
                .collect::<Vec<_>>(),
            ["Done first", "Todo first"]
        );

        let _ = app.dispatch(Action::NavigateFocus(Direction::Right));
        assert_eq!(
            app.dispatch(Action::PrimaryAction),
            AppOutcome::TasksChanged
        );
        assert_eq!(
            app.tasks()
                .pending()
                .map(|task| task.description())
                .collect::<Vec<_>>(),
            ["Todo second", "Done first"]
        );
    }

    #[test]
    fn reset_below_ten_seconds_is_immediate() {
        let mut app = App::new();
        let _ = app.dispatch(Action::PrimaryAction);
        let _ = app.tick(Duration::from_secs(9));

        let _ = app.dispatch(Action::ResetSession);

        assert!(!app.is_confirmation_open());
        assert_eq!(app.timer().state(), TimerState::Ready(SessionKind::Focus));
        assert_eq!(app.timer().remaining(), Duration::from_secs(25 * 60));
    }

    #[test]
    fn reset_at_ten_seconds_pauses_and_requests_confirmation() {
        let mut app = App::new();
        let _ = app.dispatch(Action::PrimaryAction);
        let _ = app.tick(Duration::from_secs(10));

        let _ = app.dispatch(Action::ResetSession);

        assert!(app.is_confirmation_open());
        assert_eq!(app.timer().state(), TimerState::Paused(SessionKind::Focus));
        assert_eq!(app.timer().remaining(), Duration::from_secs(25 * 60 - 10));
    }

    #[test]
    fn confirming_reset_returns_the_same_session_to_ready() {
        let mut app = App::new();
        double_click_session(&mut app, SessionKind::LongBreak, Instant::now());
        let _ = app.tick(Duration::from_secs(10));
        let _ = app.dispatch(Action::ResetSession);

        let _ = app.dispatch(Action::ConfirmPendingAction);

        assert!(!app.is_confirmation_open());
        assert_eq!(
            app.timer().state(),
            TimerState::Ready(SessionKind::LongBreak)
        );
        assert_eq!(app.timer().remaining(), Duration::from_secs(15 * 60));
    }

    #[test]
    fn cancelling_reset_restores_running_but_preserves_paused() {
        let mut running = App::new();
        let _ = running.dispatch(Action::PrimaryAction);
        let _ = running.tick(Duration::from_secs(10));
        let _ = running.dispatch(Action::ResetSession);
        let _ = running.dispatch(Action::CancelPendingAction);
        assert_eq!(
            running.timer().state(),
            TimerState::Running(SessionKind::Focus)
        );

        let mut paused = App::new();
        let _ = paused.dispatch(Action::PrimaryAction);
        let _ = paused.tick(Duration::from_secs(10));
        let _ = paused.dispatch(Action::PrimaryAction);
        let _ = paused.dispatch(Action::ResetSession);
        let _ = paused.dispatch(Action::CancelPendingAction);
        assert_eq!(
            paused.timer().state(),
            TimerState::Paused(SessionKind::Focus)
        );
    }

    #[test]
    fn confirmation_ignores_unrelated_actions_and_mouse() {
        let mut app = App::new();
        let _ = app.dispatch(Action::PrimaryAction);
        let _ = app.tick(Duration::from_secs(10));
        let _ = app.dispatch(Action::CycleSession);
        let focus = app.ui_focus();
        let remaining = app.timer().remaining();

        assert_eq!(app.dispatch(Action::Quit), AppOutcome::None);
        let _ = app.dispatch(Action::PrimaryAction);
        let _ = app.dispatch(Action::NavigateFocus(Direction::Down));
        let _ = app.handle_click_target(ClickTarget::Todo, Instant::now());

        assert!(app.is_confirmation_open());
        assert_eq!(app.ui_focus(), focus);
        assert_eq!(app.timer().remaining(), remaining);
        assert_eq!(app.timer().state(), TimerState::Paused(SessionKind::Focus));
    }

    #[test]
    fn ready_session_cycles_without_starting() {
        let mut app = App::new();

        let _ = app.dispatch(Action::CycleSession);

        assert_eq!(
            app.timer().state(),
            TimerState::Ready(SessionKind::ShortBreak)
        );
        assert_eq!(app.timer().remaining(), Duration::from_secs(5 * 60));
    }

    #[test]
    fn cycling_below_ten_seconds_immediately_discards_progress() {
        for pause_first in [false, true] {
            let mut app = App::new();
            let _ = app.dispatch(Action::PrimaryAction);
            let _ = app.tick(Duration::from_secs(9));
            if pause_first {
                let _ = app.dispatch(Action::PrimaryAction);
            }

            let _ = app.dispatch(Action::CycleSession);

            assert!(!app.is_confirmation_open());
            assert_eq!(
                app.timer().state(),
                TimerState::Ready(SessionKind::ShortBreak)
            );
            assert_eq!(app.timer().remaining(), Duration::from_secs(5 * 60));
        }
    }

    #[test]
    fn cycling_at_ten_seconds_pauses_and_requests_confirmation() {
        for pause_first in [false, true] {
            let mut app = App::new();
            let _ = app.dispatch(Action::PrimaryAction);
            let _ = app.tick(Duration::from_secs(10));
            if pause_first {
                let _ = app.dispatch(Action::PrimaryAction);
            }

            let _ = app.dispatch(Action::CycleSession);

            assert!(app.is_confirmation_open());
            assert_eq!(
                app.pending_confirmation(),
                Some(ConfirmationOperation::TimerChange(TimerChange::Cycle))
            );
            assert_eq!(app.timer().state(), TimerState::Paused(SessionKind::Focus));
            assert_eq!(app.timer().remaining(), Duration::from_secs(25 * 60 - 10));
        }
    }

    #[test]
    fn confirming_cycle_discards_progress_and_prepares_following_session() {
        let mut app = App::new();
        let _ = app.dispatch(Action::PrimaryAction);
        let _ = app.tick(Duration::from_secs(10));
        let _ = app.dispatch(Action::CycleSession);

        let _ = app.dispatch(Action::ConfirmPendingAction);

        assert!(!app.is_confirmation_open());
        assert_eq!(
            app.timer().state(),
            TimerState::Ready(SessionKind::ShortBreak)
        );
        assert_eq!(app.timer().remaining(), Duration::from_secs(5 * 60));
    }

    #[test]
    fn cancelling_cycle_restores_running_but_preserves_paused() {
        let mut running = App::new();
        let _ = running.dispatch(Action::PrimaryAction);
        let _ = running.tick(Duration::from_secs(10));
        let _ = running.dispatch(Action::CycleSession);
        let _ = running.dispatch(Action::CancelPendingAction);
        assert_eq!(
            running.timer().state(),
            TimerState::Running(SessionKind::Focus)
        );
        assert_eq!(
            running.timer().remaining(),
            Duration::from_secs(25 * 60 - 10)
        );

        let mut paused = App::new();
        let _ = paused.dispatch(Action::PrimaryAction);
        let _ = paused.tick(Duration::from_secs(10));
        let _ = paused.dispatch(Action::PrimaryAction);
        let _ = paused.dispatch(Action::CycleSession);
        let _ = paused.dispatch(Action::CancelPendingAction);
        assert_eq!(
            paused.timer().state(),
            TimerState::Paused(SessionKind::Focus)
        );
        assert_eq!(
            paused.timer().remaining(),
            Duration::from_secs(25 * 60 - 10)
        );
    }

    #[test]
    fn session_control_single_click_selects_and_double_click_starts() {
        let mut app = App::new();
        let now = Instant::now();
        let target = ClickTarget::SessionControl(SessionKind::LongBreak);

        let _ = app.handle_click_target(target, now);
        assert_eq!(app.ui_focus(), UiFocus::Clock);
        assert_eq!(
            app.timer().state(),
            TimerState::Ready(SessionKind::LongBreak)
        );

        let _ = app.handle_click_target(target, now + Duration::from_millis(100));
        assert_eq!(
            app.timer().state(),
            TimerState::Running(SessionKind::LongBreak)
        );
    }

    #[test]
    fn different_or_too_slow_session_clicks_remain_ready() {
        let mut different = App::new();
        let now = Instant::now();
        let _ =
            different.handle_click_target(ClickTarget::SessionControl(SessionKind::LongBreak), now);
        let _ = different.handle_click_target(
            ClickTarget::SessionControl(SessionKind::ShortBreak),
            now + Duration::from_millis(100),
        );
        assert_eq!(
            different.timer().state(),
            TimerState::Ready(SessionKind::ShortBreak)
        );

        let mut slow = App::new();
        let target = ClickTarget::SessionControl(SessionKind::LongBreak);
        let _ = slow.handle_click_target(target, now);
        let _ = slow.handle_click_target(target, now + Duration::from_millis(501));
        assert_eq!(
            slow.timer().state(),
            TimerState::Ready(SessionKind::LongBreak)
        );
    }

    #[test]
    fn double_clicking_active_session_control_pauses_or_resumes() {
        for (initially_paused, expected) in [
            (false, TimerState::Paused(SessionKind::Focus)),
            (true, TimerState::Running(SessionKind::Focus)),
        ] {
            let mut app = active_focus(Duration::from_secs(10), initially_paused);
            let now = Instant::now();
            let target = ClickTarget::SessionControl(SessionKind::Focus);

            let _ = app.handle_click_target(target, now);
            assert!(!app.is_confirmation_open());
            let _ = app.handle_click_target(target, now + Duration::from_millis(100));

            assert_eq!(app.timer().state(), expected);
            assert_eq!(app.timer().remaining(), Duration::from_secs(25 * 60 - 10));
        }
    }

    #[test]
    fn single_clicking_different_session_below_threshold_selects_it_immediately() {
        for initially_paused in [false, true] {
            let mut app = active_focus(Duration::from_secs(9), initially_paused);

            let _ = app.handle_click_target(
                ClickTarget::SessionControl(SessionKind::ShortBreak),
                Instant::now(),
            );

            assert!(!app.is_confirmation_open());
            assert_eq!(
                app.timer().state(),
                TimerState::Ready(SessionKind::ShortBreak)
            );
            assert_eq!(app.timer().remaining(), Duration::from_secs(5 * 60));
        }
    }

    #[test]
    fn single_clicking_different_session_at_threshold_confirms_selection() {
        for initially_paused in [false, true] {
            let mut app = active_focus(Duration::from_secs(10), initially_paused);
            let now = Instant::now();
            let target = ClickTarget::SessionControl(SessionKind::LongBreak);

            let _ = app.handle_click_target(target, now);

            assert_eq!(
                app.pending_confirmation(),
                Some(ConfirmationOperation::TimerChange(
                    TimerChange::SelectSession(SessionKind::LongBreak)
                ))
            );
            let _ = app.dispatch(Action::ConfirmPendingAction);
            assert_eq!(
                app.timer().state(),
                TimerState::Ready(SessionKind::LongBreak)
            );
            assert_eq!(app.timer().remaining(), Duration::from_secs(15 * 60));

            let _ = app.handle_click_target(target, now + Duration::from_millis(100));
            assert_eq!(
                app.timer().state(),
                TimerState::Ready(SessionKind::LongBreak)
            );
        }
    }

    #[test]
    fn double_clicking_different_session_below_threshold_starts_it_immediately() {
        for initially_paused in [false, true] {
            let mut app = active_focus(Duration::from_secs(9), initially_paused);
            let now = Instant::now();
            let target = ClickTarget::SessionControl(SessionKind::ShortBreak);

            let _ = app.handle_click_target(target, now);
            let _ = app.handle_click_target(target, now + Duration::from_millis(100));

            assert!(!app.is_confirmation_open());
            assert_eq!(
                app.timer().state(),
                TimerState::Running(SessionKind::ShortBreak)
            );
            assert_eq!(app.timer().remaining(), Duration::from_secs(5 * 60));
        }
    }

    #[test]
    fn second_matching_click_upgrades_confirmed_change_to_change_and_start() {
        for initially_paused in [false, true] {
            let mut app = active_focus(Duration::from_secs(10), initially_paused);
            let now = Instant::now();
            let target = ClickTarget::SessionControl(SessionKind::ShortBreak);

            let _ = app.handle_click_target(target, now);
            assert_eq!(
                app.pending_confirmation(),
                Some(ConfirmationOperation::TimerChange(
                    TimerChange::SelectSession(SessionKind::ShortBreak)
                ))
            );
            let _ = app.handle_click_target(target, now + Duration::from_millis(100));
            assert_eq!(
                app.pending_confirmation(),
                Some(ConfirmationOperation::TimerChange(
                    TimerChange::StartSession(SessionKind::ShortBreak)
                ))
            );
            let _ = app.dispatch(Action::ConfirmPendingAction);

            assert_eq!(
                app.timer().state(),
                TimerState::Running(SessionKind::ShortBreak)
            );
            assert_eq!(app.timer().remaining(), Duration::from_secs(5 * 60));
        }
    }

    #[test]
    fn cancelling_upgraded_session_change_restores_prior_activity() {
        for initially_paused in [false, true] {
            let mut app = active_focus(Duration::from_secs(10), initially_paused);
            let now = Instant::now();
            let target = ClickTarget::SessionControl(SessionKind::LongBreak);
            let _ = app.handle_click_target(target, now);
            let _ = app.handle_click_target(target, now + Duration::from_millis(100));

            let _ = app.dispatch(Action::CancelPendingAction);

            let expected = if initially_paused {
                TimerState::Paused(SessionKind::Focus)
            } else {
                TimerState::Running(SessionKind::Focus)
            };
            assert_eq!(app.timer().state(), expected);
            assert_eq!(app.timer().remaining(), Duration::from_secs(25 * 60 - 10));
        }
    }

    #[test]
    fn mismatched_or_late_second_click_does_not_upgrade_pending_selection() {
        let now = Instant::now();
        for second_click in [
            (
                ClickTarget::SessionControl(SessionKind::LongBreak),
                Duration::from_millis(100),
            ),
            (
                ClickTarget::SessionControl(SessionKind::ShortBreak),
                Duration::from_millis(501),
            ),
        ] {
            let mut app = active_focus(Duration::from_secs(10), false);
            let _ =
                app.handle_click_target(ClickTarget::SessionControl(SessionKind::ShortBreak), now);
            let _ = app.handle_click_target(second_click.0, now + second_click.1);
            let _ = app.dispatch(Action::ConfirmPendingAction);

            assert_eq!(
                app.timer().state(),
                TimerState::Ready(SessionKind::ShortBreak)
            );
        }
    }

    #[test]
    fn mouse_is_ignored_during_task_editing() {
        let mut app = App::new();
        let _ = app.dispatch(Action::NavigateFocus(Direction::Down));
        let _ = app.dispatch(Action::BeginAdd);

        let _ = app.handle_click_target(ClickTarget::Clock, Instant::now());

        assert_eq!(app.ui_focus(), UiFocus::Todo);
        assert_eq!(app.edit_mode(), EditMode::Adding);
    }

    #[test]
    fn double_clicking_a_target_runs_its_contextual_action_once() {
        let mut app = App::new();
        let first = Instant::now();
        let _ = app.handle_click_target(ClickTarget::Clock, first);
        let _ = app.handle_click_target(ClickTarget::Clock, first + Duration::from_millis(200));
        assert_eq!(app.timer().state(), TimerState::Running(SessionKind::Focus));

        let _ = app.handle_click_target(ClickTarget::Clock, first + Duration::from_millis(300));
        assert_eq!(app.timer().state(), TimerState::Running(SessionKind::Focus));
    }

    #[test]
    fn double_clicking_a_todo_task_completes_the_selected_task() {
        let mut app = App::new();
        add_task(&mut app, "First");
        add_task(&mut app, "Complete me");
        let first = Instant::now();

        let _ = app.handle_click_target(ClickTarget::TodoTask(1), first);
        assert_eq!(
            app.handle_click_target(ClickTarget::TodoTask(1), first + Duration::from_millis(200)),
            AppOutcome::TasksChanged
        );

        assert_eq!(app.tasks().pending().count(), 1);
        assert_eq!(
            app.tasks().completed().next().unwrap().description(),
            "Complete me"
        );
    }

    #[test]
    fn double_clicking_a_done_task_returns_the_selected_task() {
        let mut app = App::new();
        add_task(&mut app, "Return me");
        let _ = app.dispatch(Action::NavigateFocus(Direction::Down));
        let _ = app.dispatch(Action::PrimaryAction);
        let first = Instant::now();

        let _ = app.handle_click_target(ClickTarget::DoneTask(0), first);
        assert_eq!(
            app.handle_click_target(ClickTarget::DoneTask(0), first + Duration::from_millis(200)),
            AppOutcome::TasksChanged
        );

        assert_eq!(app.tasks().completed().count(), 0);
        assert_eq!(
            app.tasks().pending().next().unwrap().description(),
            "Return me"
        );
    }

    #[test]
    fn clicks_outside_the_window_or_on_different_targets_stay_single() {
        let mut app = App::new();
        let first = Instant::now();
        let _ = app.handle_click_target(ClickTarget::Clock, first);
        let _ = app.handle_click_target(ClickTarget::Clock, first + Duration::from_millis(501));
        assert_eq!(app.timer().state(), TimerState::Ready(SessionKind::Focus));

        let _ = app.handle_click_target(ClickTarget::TodoTask(0), first + Duration::from_secs(1));
        let _ = app.handle_click_target(
            ClickTarget::TodoTask(1),
            first + Duration::from_millis(1100),
        );
        assert_eq!(app.tasks().completed().count(), 0);
    }

    #[test]
    fn click_targets_update_focus_and_task_selection() {
        let mut app = App::new();
        add_task(&mut app, "First");
        add_task(&mut app, "Second");
        let now = Instant::now();

        let _ = app.handle_click_target(ClickTarget::TodoTask(1), now);
        assert_eq!(app.ui_focus(), UiFocus::Todo);
        assert_eq!(app.todo_selection(), 1);

        let _ = app.handle_click_target(ClickTarget::Done, now);
        assert_eq!(app.ui_focus(), UiFocus::Done);
    }

    #[test]
    fn non_actionable_clicks_break_double_click_sequences() {
        let mut app = App::new();
        let first = Instant::now();

        let _ = app.handle_click_target(ClickTarget::Clock, first);
        let _ = app.handle_click_target(ClickTarget::Outside, first + Duration::from_millis(100));
        let _ = app.handle_click_target(ClickTarget::Clock, first + Duration::from_millis(200));

        assert_eq!(app.timer().state(), TimerState::Ready(SessionKind::Focus));
    }

    #[test]
    fn settings_preserve_running_and_paused_activity() {
        for initially_paused in [false, true] {
            let mut app = active_focus(Duration::from_secs(1), initially_paused);
            let expected_state = if initially_paused {
                TimerState::Paused(SessionKind::Focus)
            } else {
                TimerState::Running(SessionKind::Focus)
            };

            assert_eq!(app.dispatch(Action::OpenSettings), AppOutcome::None);
            assert_eq!(app.timer().state(), expected_state);
            assert!(app.is_settings_open());
            assert_eq!(app.tick(Duration::from_secs(1)), AppOutcome::None);
            assert_eq!(
                app.timer().progress(),
                Duration::from_secs(if initially_paused { 1 } else { 2 })
            );

            assert_eq!(app.dispatch(Action::SettingsClose), AppOutcome::None);
            assert_eq!(app.timer().state(), expected_state);
            assert!(!app.is_settings_open());
        }
    }

    #[test]
    fn settings_cancel_leaves_nested_editing_and_close_closes_overlay() {
        let mut app = App::new();
        let _ = app.dispatch(Action::OpenSettings);
        let _ = app.dispatch(Action::SettingsActivate);

        assert_eq!(app.settings_mode(), SettingsMode::EditingValue);
        assert_eq!(app.dispatch(Action::SettingsCancel), AppOutcome::None);
        assert_eq!(app.settings_mode(), SettingsMode::Navigating);
        assert_eq!(app.dispatch(Action::SettingsClose), AppOutcome::None);
        assert_eq!(app.settings_mode(), SettingsMode::Closed);
    }

    #[test]
    fn accepted_settings_binding_is_emitted_immediately_and_closes_overlay() {
        let mut app = App::new();
        let _ = app.dispatch(Action::OpenSettings);
        move_settings_to(&mut app, SettingField::Key(KeyAction::Settings));
        let _ = app.dispatch(Action::SettingsActivate);
        let outcome = app.dispatch(Action::SettingsCaptureKey(ConfigKey::Character('t')));

        assert_eq!(app.input_keys().settings(), [ConfigKey::Character('t')]);
        assert!(app.is_settings_open());
        let AppOutcome::SettingsChanged(config) = outcome else {
            panic!("accepted setting was not emitted")
        };
        assert_eq!(config.keys().settings(), [ConfigKey::Character('t')]);
        assert_eq!(app.dispatch(Action::SettingsClose), AppOutcome::None);
        assert!(!app.is_settings_open());
    }

    #[test]
    fn non_timer_settings_apply_immediately_without_changing_activity() {
        let mut app = App::new();
        let _ = app.dispatch(Action::PrimaryAction);
        let _ = app.dispatch(Action::OpenSettings);
        move_settings_to(&mut app, SettingField::PersistTasks);
        let outcome = app.dispatch(Action::SettingsActivate);
        let AppOutcome::SettingsChanged(config) = outcome else {
            panic!("settings were not emitted")
        };
        assert!(!config.tasks().persist());
        assert_eq!(app.timer().state(), TimerState::Running(SessionKind::Focus));
        assert_eq!(app.dispatch(Action::SettingsClose), AppOutcome::None);
        assert_eq!(app.timer().state(), TimerState::Running(SessionKind::Focus));
    }

    #[test]
    fn active_timer_keeps_its_installed_duration_when_settings_change() {
        let mut app = App::new();
        let _ = app.dispatch(Action::PrimaryAction);
        let _ = app.tick(Duration::from_secs(10));
        let _ = app.dispatch(Action::OpenSettings);
        let _ = app.dispatch(Action::SettingsActivate);
        for _ in 0..5 {
            let _ = app.dispatch(Action::SettingsPopInput);
        }
        for character in "30:00".chars() {
            let _ = app.dispatch(Action::SettingsPushInput(character));
        }
        let outcome = app.dispatch(Action::SettingsSubmitInput);
        let AppOutcome::SettingsChanged(config) = outcome else {
            panic!("timer setting was not emitted")
        };
        assert_eq!(config.timer().focus_duration().as_secs(), 30 * 60);
        assert_eq!(app.timer().state(), TimerState::Running(SessionKind::Focus));
        assert_eq!(app.timer().progress(), Duration::from_secs(10));
        assert_eq!(app.timer().remaining(), Duration::from_secs(25 * 60 - 10));

        assert_eq!(app.dispatch(Action::SettingsClose), AppOutcome::None);
        assert_eq!(app.timer().state(), TimerState::Running(SessionKind::Focus));
        assert_eq!(app.timer().progress(), Duration::from_secs(10));
    }

    #[test]
    fn ready_timer_adopts_duration_settings_immediately() {
        let mut app = App::new();
        let _ = app.dispatch(Action::OpenSettings);
        let _ = app.dispatch(Action::SettingsActivate);
        for _ in 0..5 {
            let _ = app.dispatch(Action::SettingsPopInput);
        }
        for character in "30:00".chars() {
            let _ = app.dispatch(Action::SettingsPushInput(character));
        }

        assert!(matches!(
            app.dispatch(Action::SettingsSubmitInput),
            AppOutcome::SettingsChanged(_)
        ));
        assert_eq!(app.timer().state(), TimerState::Ready(SessionKind::Focus));
        assert_eq!(app.timer().remaining(), Duration::from_secs(30 * 60));
    }
}
