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

    fn focus(&mut self, focus: UiFocus) {
        self.ui_focus = focus;
        if matches!(focus, UiFocus::Todo | UiFocus::Done) {
            self.last_task_focus = focus;
        }
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
        self.focus(self.ui_focus.navigate(direction));
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
#[path = "app/tests.rs"]
mod tests;
