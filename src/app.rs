use std::{
    num::NonZeroU32,
    time::{Duration, Instant},
};

use crate::{
    SessionKind,
    tasks::TaskList,
    timer::{PomodoroTimer, TimerState},
};

const DOUBLE_CLICK_WINDOW: Duration = Duration::from_millis(500);
const RESET_CONFIRMATION_THRESHOLD: Duration = Duration::from_secs(10);

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
    PrimaryAction,
    SelectNextSession,
    ResetSession,
    ConfirmReset,
    CancelReset,
    BeginAdd,
    EditSelected,
    DeleteSelected,
    SubmitEdit,
    CancelEdit,
    PushInput(char),
    PopInput,
}

/// A boundary-relevant result of applying an application transition.
#[must_use]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppOutcome {
    None,
    Quit,
    SessionCompleted(SessionKind),
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

/// A semantic mouse target produced by UI coordinate hit testing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClickTarget {
    Clock,
    SessionControl(SessionKind),
    Todo,
    TodoTask(usize),
    Done,
    DoneTask(usize),
    Outside,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PendingReset {
    ResumeOnCancel,
    StayPaused,
}

/// Runtime application state and terminal-independent state transitions.
#[derive(Debug)]
pub struct App {
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
    pending_reset: Option<PendingReset>,
}

impl App {
    /// Creates an application with the current default durations and no tasks.
    pub fn new() -> Self {
        Self {
            timer: PomodoroTimer::new(
                Duration::from_secs(25 * 60),
                Duration::from_secs(5 * 60),
                Duration::from_secs(15 * 60),
                NonZeroU32::new(4).expect("the default long-break interval is positive"),
            ),
            tasks: TaskList::new(),
            ui_focus: UiFocus::Clock,
            todo_selection: 0,
            done_selection: 0,
            todo_offset: 0,
            done_offset: 0,
            edit_mode: EditMode::Normal,
            input: String::new(),
            last_click: None,
            pending_reset: None,
        }
    }

    pub(crate) fn timer(&self) -> &PomodoroTimer {
        &self.timer
    }

    pub(crate) fn tasks(&self) -> &TaskList {
        &self.tasks
    }

    /// Applies a semantic action without depending on its physical key mapping.
    pub fn dispatch(&mut self, action: Action) -> AppOutcome {
        if self.pending_reset.is_some() {
            match action {
                Action::ConfirmReset => self.confirm_reset(),
                Action::CancelReset => self.cancel_reset(),
                _ => {}
            }
            return AppOutcome::None;
        }

        match action {
            Action::Quit => return AppOutcome::Quit,
            Action::NavigateFocus(direction) => self.navigate_focus(direction),
            Action::MoveSelection(direction) => match self.ui_focus {
                UiFocus::Clock => {}
                UiFocus::Todo => self.move_todo_selection(direction),
                UiFocus::Done => self.move_done_selection(direction),
            },
            Action::PrimaryAction => match self.ui_focus {
                UiFocus::Clock => self.clock_primary_action(),
                UiFocus::Todo => self.complete_selected_todo(),
                UiFocus::Done => self.return_selected_done(),
            },
            Action::SelectNextSession => self.select_next_session(),
            Action::ResetSession => self.reset_session(),
            Action::ConfirmReset | Action::CancelReset => {}
            Action::BeginAdd => self.begin_add(),
            Action::EditSelected => match self.ui_focus {
                UiFocus::Clock => {}
                UiFocus::Todo => self.edit_selected_todo(),
                UiFocus::Done => self.edit_selected_done(),
            },
            Action::DeleteSelected => match self.ui_focus {
                UiFocus::Clock => {}
                UiFocus::Todo => self.delete_selected_todo(),
                UiFocus::Done => self.delete_selected_done(),
            },
            Action::SubmitEdit => self.submit_edit(),
            Action::CancelEdit => self.cancel_edit(),
            Action::PushInput(character) => self.push_input(character),
            Action::PopInput => self.pop_input(),
        }

        AppOutcome::None
    }

    /// Advances monotonic application time and reports a completed session.
    pub fn tick(&mut self, elapsed: Duration) -> AppOutcome {
        self.timer
            .tick(elapsed)
            .map_or(AppOutcome::None, AppOutcome::SessionCompleted)
    }

    /// Returns the area that receives contextual semantic actions.
    pub fn ui_focus(&self) -> UiFocus {
        self.ui_focus
    }

    /// Returns the current text-entry context.
    pub fn edit_mode(&self) -> EditMode {
        self.edit_mode
    }

    /// Reports whether reset confirmation currently owns keyboard and mouse input.
    pub fn is_reset_confirmation_open(&self) -> bool {
        self.pending_reset.is_some()
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

    fn focus(&mut self, focus: UiFocus) {
        self.ui_focus = focus;
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
        if self.ui_focus != UiFocus::Todo {
            return;
        }

        self.input.clear();
        self.edit_mode = EditMode::Adding;
    }

    fn cancel_edit(&mut self) {
        self.input.clear();
        self.edit_mode = EditMode::Normal;
    }

    fn submit_edit(&mut self) {
        let description = std::mem::take(&mut self.input);

        match self.edit_mode {
            EditMode::Adding => self.tasks.add(description),
            EditMode::Editing { task_index } => {
                self.tasks.edit(task_index, description);
            }
            EditMode::Normal => {}
        }

        self.edit_mode = EditMode::Normal;
        self.clamp_selections();
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

    fn select_next_session(&mut self) {
        self.timer.select_next_session();
    }

    fn reset_session(&mut self) {
        let pending_reset = match self.timer.state() {
            TimerState::Running(_) => PendingReset::ResumeOnCancel,
            TimerState::Paused(_) => PendingReset::StayPaused,
            TimerState::Ready(_) => return,
        };

        if self.timer.progress() < RESET_CONFIRMATION_THRESHOLD {
            self.timer.reset_session();
            return;
        }

        self.timer.pause();
        self.pending_reset = Some(pending_reset);
        self.clear_pending_click();
    }

    fn confirm_reset(&mut self) {
        self.timer.reset_session();
        self.pending_reset = None;
    }

    fn cancel_reset(&mut self) {
        if self.pending_reset == Some(PendingReset::ResumeOnCancel) {
            self.timer.resume();
        }
        self.pending_reset = None;
    }

    fn move_todo_selection(&mut self, direction: Direction) {
        let len = self.tasks.pending().count();
        Self::move_selection(&mut self.todo_selection, len, direction);
    }

    fn move_done_selection(&mut self, direction: Direction) {
        let len = self.tasks.completed().count();
        Self::move_selection(&mut self.done_selection, len, direction);
    }

    fn edit_selected_todo(&mut self) {
        if let Some(index) = self.selected_todo_index() {
            self.begin_edit(index);
        }
    }

    fn edit_selected_done(&mut self) {
        if let Some(index) = self.selected_done_index() {
            self.begin_edit(index);
        }
    }

    fn delete_selected_todo(&mut self) {
        if let Some(index) = self.selected_todo_index() {
            self.tasks.delete(index);
            self.clamp_selections();
        }
    }

    fn delete_selected_done(&mut self) {
        if let Some(index) = self.selected_done_index() {
            self.tasks.delete(index);
            self.clamp_selections();
        }
    }

    fn complete_selected_todo(&mut self) {
        if let Some(index) = self.selected_todo_index() {
            self.tasks.complete(index);
            self.clamp_selections();
        }
    }

    fn return_selected_done(&mut self) {
        if let Some(index) = self.selected_done_index() {
            self.tasks.uncomplete(index);
            self.clamp_selections();
        }
    }

    /// Applies a semantic click after the UI boundary performs hit testing.
    pub fn handle_click_target(&mut self, target: ClickTarget, now: Instant) {
        if self.edit_mode != EditMode::Normal || self.pending_reset.is_some() {
            return;
        }

        match target {
            ClickTarget::Clock => {
                self.focus(UiFocus::Clock);
                self.handle_actionable_click(target, now);
            }
            ClickTarget::SessionControl(session) => {
                self.focus(UiFocus::Clock);
                if !matches!(self.timer.state(), TimerState::Ready(_)) {
                    self.clear_pending_click();
                } else if self.is_double_click(target, now) {
                    self.timer.start_session(session);
                    self.clear_pending_click();
                } else {
                    self.timer.select_session(session);
                    self.last_click = Some((target, now));
                }
            }
            ClickTarget::Todo => {
                self.focus(UiFocus::Todo);
                self.clear_pending_click();
            }
            ClickTarget::TodoTask(selection) => {
                self.focus(UiFocus::Todo);
                self.select_todo(selection);
                self.handle_actionable_click(target, now);
            }
            ClickTarget::Done => {
                self.focus(UiFocus::Done);
                self.clear_pending_click();
            }
            ClickTarget::DoneTask(selection) => {
                self.focus(UiFocus::Done);
                self.select_done(selection);
                self.handle_actionable_click(target, now);
            }
            ClickTarget::Outside => self.clear_pending_click(),
        }
    }

    fn clear_pending_click(&mut self) {
        self.last_click = None;
    }

    fn handle_actionable_click(&mut self, target: ClickTarget, now: Instant) {
        let is_double_click = self.is_double_click(target, now);

        if is_double_click {
            match target {
                ClickTarget::Clock => self.clock_primary_action(),
                ClickTarget::TodoTask(_) => self.complete_selected_todo(),
                ClickTarget::DoneTask(_) => self.return_selected_done(),
                ClickTarget::SessionControl(_)
                | ClickTarget::Todo
                | ClickTarget::Done
                | ClickTarget::Outside => {
                    unreachable!("only actionable targets are recorded")
                }
            }
            self.last_click = None;
        } else {
            self.last_click = Some((target, now));
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

    fn selected_todo_index(&self) -> Option<usize> {
        self.tasks
            .pending_with_indices()
            .nth(self.todo_selection)
            .map(|(index, _)| index)
    }

    fn selected_done_index(&self) -> Option<usize> {
        self.tasks
            .completed_with_indices()
            .nth(self.done_selection)
            .map(|(index, _)| index)
    }

    fn begin_edit(&mut self, task_index: usize) {
        let description = self
            .tasks
            .pending_with_indices()
            .chain(self.tasks.completed_with_indices())
            .find(|(index, _)| *index == task_index)
            .map(|(_, task)| task.description().to_string());

        if let Some(description) = description {
            self.input = description;
            self.edit_mode = EditMode::Editing { task_index };
        }
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

    use crate::timer::{SessionKind, TimerState};

    use super::{Action, App, AppOutcome, ClickTarget, Direction, EditMode, UiFocus};

    fn add_task(app: &mut App, description: &str) {
        let _ = app.dispatch(Action::NavigateFocus(Direction::Down));
        let _ = app.dispatch(Action::BeginAdd);
        for character in description.chars() {
            let _ = app.dispatch(Action::PushInput(character));
        }
        let _ = app.dispatch(Action::SubmitEdit);
        let _ = app.dispatch(Action::NavigateFocus(Direction::Up));
    }

    fn double_click_session(app: &mut App, session: SessionKind, first_click: Instant) {
        let target = ClickTarget::SessionControl(session);
        app.handle_click_target(target, first_click);
        app.handle_click_target(target, first_click + Duration::from_millis(100));
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

        let _ = app.dispatch(Action::PrimaryAction);
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
    fn tick_reports_focus_completion_exactly_once() {
        let mut app = App::new();
        let _ = app.dispatch(Action::PrimaryAction);

        assert_eq!(
            app.tick(Duration::from_secs(25 * 60)),
            AppOutcome::SessionCompleted(SessionKind::Focus)
        );
        assert_eq!(app.tick(Duration::from_secs(1)), AppOutcome::None);
    }

    #[test]
    fn tick_reports_each_break_completion_exactly_once() {
        for (session, duration) in [
            (SessionKind::ShortBreak, Duration::from_secs(5 * 60)),
            (SessionKind::LongBreak, Duration::from_secs(15 * 60)),
        ] {
            let mut app = App::new();
            double_click_session(&mut app, session, Instant::now());

            assert_eq!(app.tick(duration), AppOutcome::SessionCompleted(session));
            assert_eq!(app.tick(Duration::from_secs(1)), AppOutcome::None);
        }
    }

    #[test]
    fn dispatches_editing_actions_without_physical_key_codes() {
        let mut app = App::new();
        let _ = app.dispatch(Action::NavigateFocus(Direction::Down));

        let _ = app.dispatch(Action::BeginAdd);
        let _ = app.dispatch(Action::PushInput('a'));
        let _ = app.dispatch(Action::PushInput('b'));
        let _ = app.dispatch(Action::PopInput);
        let _ = app.dispatch(Action::SubmitEdit);

        assert_eq!(app.tasks().pending().next().unwrap().description(), "a");
        assert_eq!(app.edit_mode(), EditMode::Normal);
    }

    #[test]
    fn begin_add_action_only_works_from_todo_focus() {
        let mut app = App::new();
        let _ = app.dispatch(Action::BeginAdd);
        assert_eq!(app.edit_mode(), EditMode::Normal);

        let _ = app.dispatch(Action::NavigateFocus(Direction::Down));
        let _ = app.dispatch(Action::BeginAdd);
        assert_eq!(app.edit_mode(), EditMode::Adding);
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
    fn row_navigation_stays_within_tasks_and_handles_empty_lists() {
        let mut app = App::new();
        let _ = app.dispatch(Action::NavigateFocus(Direction::Down));
        let _ = app.dispatch(Action::MoveSelection(Direction::Down));
        assert_eq!(app.todo_selection(), 0);

        let _ = app.dispatch(Action::BeginAdd);
        let _ = app.dispatch(Action::PushInput('1'));
        let _ = app.dispatch(Action::SubmitEdit);
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
    fn editing_selected_filtered_task_updates_the_right_task() {
        let mut app = App::new();
        add_task(&mut app, "Done");
        let _ = app.dispatch(Action::NavigateFocus(Direction::Down));
        let _ = app.dispatch(Action::PrimaryAction);
        let _ = app.dispatch(Action::NavigateFocus(Direction::Up));
        add_task(&mut app, "Edit me");
        let _ = app.dispatch(Action::NavigateFocus(Direction::Down));
        let _ = app.dispatch(Action::EditSelected);
        assert_eq!(app.edit_mode(), EditMode::Editing { task_index: 1 });

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
        let _ = app.dispatch(Action::PrimaryAction);
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
        let _ = app.dispatch(Action::DeleteSelected);
        assert_eq!(app.todo_selection(), 0);
        assert_eq!(app.tasks().pending().count(), 1);
    }

    #[test]
    fn reset_below_ten_seconds_is_immediate() {
        let mut app = App::new();
        let _ = app.dispatch(Action::PrimaryAction);
        let _ = app.tick(Duration::from_secs(9));

        let _ = app.dispatch(Action::ResetSession);

        assert!(!app.is_reset_confirmation_open());
        assert_eq!(app.timer().state(), TimerState::Ready(SessionKind::Focus));
        assert_eq!(app.timer().remaining(), Duration::from_secs(25 * 60));
    }

    #[test]
    fn reset_at_ten_seconds_pauses_and_requests_confirmation() {
        let mut app = App::new();
        let _ = app.dispatch(Action::PrimaryAction);
        let _ = app.tick(Duration::from_secs(10));

        let _ = app.dispatch(Action::ResetSession);

        assert!(app.is_reset_confirmation_open());
        assert_eq!(app.timer().state(), TimerState::Paused(SessionKind::Focus));
        assert_eq!(app.timer().remaining(), Duration::from_secs(25 * 60 - 10));
    }

    #[test]
    fn confirming_reset_returns_the_same_session_to_ready() {
        let mut app = App::new();
        double_click_session(&mut app, SessionKind::LongBreak, Instant::now());
        let _ = app.tick(Duration::from_secs(10));
        let _ = app.dispatch(Action::ResetSession);

        let _ = app.dispatch(Action::ConfirmReset);

        assert!(!app.is_reset_confirmation_open());
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
        let _ = running.dispatch(Action::CancelReset);
        assert_eq!(
            running.timer().state(),
            TimerState::Running(SessionKind::Focus)
        );

        let mut paused = App::new();
        let _ = paused.dispatch(Action::PrimaryAction);
        let _ = paused.tick(Duration::from_secs(10));
        let _ = paused.dispatch(Action::PrimaryAction);
        let _ = paused.dispatch(Action::ResetSession);
        let _ = paused.dispatch(Action::CancelReset);
        assert_eq!(
            paused.timer().state(),
            TimerState::Paused(SessionKind::Focus)
        );
    }

    #[test]
    fn reset_confirmation_ignores_unrelated_actions_and_mouse() {
        let mut app = App::new();
        let _ = app.dispatch(Action::PrimaryAction);
        let _ = app.tick(Duration::from_secs(10));
        let _ = app.dispatch(Action::ResetSession);
        let focus = app.ui_focus();
        let remaining = app.timer().remaining();

        assert_eq!(app.dispatch(Action::Quit), AppOutcome::None);
        let _ = app.dispatch(Action::PrimaryAction);
        let _ = app.dispatch(Action::NavigateFocus(Direction::Down));
        app.handle_click_target(ClickTarget::Todo, Instant::now());

        assert!(app.is_reset_confirmation_open());
        assert_eq!(app.ui_focus(), focus);
        assert_eq!(app.timer().remaining(), remaining);
        assert_eq!(app.timer().state(), TimerState::Paused(SessionKind::Focus));
    }

    #[test]
    fn session_control_single_click_selects_and_double_click_starts() {
        let mut app = App::new();
        let now = Instant::now();
        let target = ClickTarget::SessionControl(SessionKind::LongBreak);

        app.handle_click_target(target, now);
        assert_eq!(app.ui_focus(), UiFocus::Clock);
        assert_eq!(
            app.timer().state(),
            TimerState::Ready(SessionKind::LongBreak)
        );

        app.handle_click_target(target, now + Duration::from_millis(100));
        assert_eq!(
            app.timer().state(),
            TimerState::Running(SessionKind::LongBreak)
        );
    }

    #[test]
    fn different_or_too_slow_session_clicks_remain_ready() {
        let mut different = App::new();
        let now = Instant::now();
        different.handle_click_target(ClickTarget::SessionControl(SessionKind::LongBreak), now);
        different.handle_click_target(
            ClickTarget::SessionControl(SessionKind::ShortBreak),
            now + Duration::from_millis(100),
        );
        assert_eq!(
            different.timer().state(),
            TimerState::Ready(SessionKind::ShortBreak)
        );

        let mut slow = App::new();
        let target = ClickTarget::SessionControl(SessionKind::LongBreak);
        slow.handle_click_target(target, now);
        slow.handle_click_target(target, now + Duration::from_millis(501));
        assert_eq!(
            slow.timer().state(),
            TimerState::Ready(SessionKind::LongBreak)
        );
    }

    #[test]
    fn session_controls_do_not_replace_running_or_paused_sessions() {
        let mut app = App::new();
        let now = Instant::now();
        double_click_session(&mut app, SessionKind::LongBreak, now);

        app.handle_click_target(
            ClickTarget::SessionControl(SessionKind::ShortBreak),
            now + Duration::from_millis(200),
        );
        assert_eq!(
            app.timer().state(),
            TimerState::Running(SessionKind::LongBreak)
        );

        let _ = app.dispatch(Action::PrimaryAction);
        app.handle_click_target(
            ClickTarget::SessionControl(SessionKind::Focus),
            now + Duration::from_millis(300),
        );
        assert_eq!(
            app.timer().state(),
            TimerState::Paused(SessionKind::LongBreak)
        );
    }

    #[test]
    fn mouse_is_ignored_during_task_editing() {
        let mut app = App::new();
        let _ = app.dispatch(Action::NavigateFocus(Direction::Down));
        let _ = app.dispatch(Action::BeginAdd);

        app.handle_click_target(ClickTarget::Clock, Instant::now());

        assert_eq!(app.ui_focus(), UiFocus::Todo);
        assert_eq!(app.edit_mode(), EditMode::Adding);
    }

    #[test]
    fn double_clicking_a_target_runs_its_contextual_action_once() {
        let mut app = App::new();
        let first = Instant::now();
        app.handle_click_target(ClickTarget::Clock, first);
        app.handle_click_target(ClickTarget::Clock, first + Duration::from_millis(200));
        assert_eq!(app.timer().state(), TimerState::Running(SessionKind::Focus));

        app.handle_click_target(ClickTarget::Clock, first + Duration::from_millis(300));
        assert_eq!(app.timer().state(), TimerState::Running(SessionKind::Focus));
    }

    #[test]
    fn double_clicking_a_todo_task_completes_the_selected_task() {
        let mut app = App::new();
        add_task(&mut app, "First");
        add_task(&mut app, "Complete me");
        let first = Instant::now();

        app.handle_click_target(ClickTarget::TodoTask(1), first);
        app.handle_click_target(ClickTarget::TodoTask(1), first + Duration::from_millis(200));

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

        app.handle_click_target(ClickTarget::DoneTask(0), first);
        app.handle_click_target(ClickTarget::DoneTask(0), first + Duration::from_millis(200));

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
        app.handle_click_target(ClickTarget::Clock, first);
        app.handle_click_target(ClickTarget::Clock, first + Duration::from_millis(501));
        assert_eq!(app.timer().state(), TimerState::Ready(SessionKind::Focus));

        app.handle_click_target(ClickTarget::TodoTask(0), first + Duration::from_secs(1));
        app.handle_click_target(
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

        app.handle_click_target(ClickTarget::TodoTask(1), now);
        assert_eq!(app.ui_focus(), UiFocus::Todo);
        assert_eq!(app.todo_selection(), 1);

        app.handle_click_target(ClickTarget::Done, now);
        assert_eq!(app.ui_focus(), UiFocus::Done);
    }

    #[test]
    fn non_actionable_clicks_break_double_click_sequences() {
        let mut app = App::new();
        let first = Instant::now();

        app.handle_click_target(ClickTarget::Clock, first);
        app.handle_click_target(ClickTarget::Outside, first + Duration::from_millis(100));
        app.handle_click_target(ClickTarget::Clock, first + Duration::from_millis(200));

        assert_eq!(app.timer().state(), TimerState::Ready(SessionKind::Focus));
    }
}
