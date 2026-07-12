use std::io::{self, Stdout};

use crossterm::{
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, MouseButton, MouseEvent,
        MouseEventKind,
    },
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};

use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction as LayoutDirection, Layout, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
};

use std::time::{Duration, Instant};

use pomock::{
    display::{format_big_duration, format_state},
    tasks::TaskList,
    timer::{PomodoroTimer, TimerState},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UiFocus {
    Clock,
    Todo,
    Done,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Direction {
    Left,
    Down,
    Up,
    Right,
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

fn focus_direction(key_code: KeyCode) -> Option<Direction> {
    match key_code {
        KeyCode::Char('H') => Some(Direction::Left),
        KeyCode::Char('J') => Some(Direction::Down),
        KeyCode::Char('K') => Some(Direction::Up),
        KeyCode::Char('L') => Some(Direction::Right),
        _ => None,
    }
}

fn row_direction(key_code: KeyCode) -> Option<Direction> {
    match key_code {
        KeyCode::Char('j') | KeyCode::Down => Some(Direction::Down),
        KeyCode::Char('k') | KeyCode::Up => Some(Direction::Up),
        _ => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EditMode {
    Normal,
    Adding,
    Editing { task_index: usize },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ClickTarget {
    Clock,
    Todo(usize),
    Done(usize),
}

#[derive(Debug)]
struct App {
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
}

impl App {
    fn new() -> Self {
        Self {
            timer: PomodoroTimer::new(Duration::from_secs(25 * 60), Duration::from_secs(5 * 60)),
            tasks: TaskList::new(),
            ui_focus: UiFocus::Clock,
            todo_selection: 0,
            done_selection: 0,
            todo_offset: 0,
            done_offset: 0,
            edit_mode: EditMode::Normal,
            input: String::new(),
            last_click: None,
        }
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

    fn navigate_focus(&mut self, key_code: KeyCode) -> bool {
        let Some(direction) = focus_direction(key_code) else {
            return false;
        };

        self.ui_focus = self.ui_focus.navigate(direction);
        true
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

    fn handle_clock_key(&mut self, key_code: KeyCode) {
        match key_code {
            KeyCode::Char(' ') => self.timer.primary_action(),
            KeyCode::Char('f') => self.timer.fast_forward(),
            KeyCode::Char('r') => self.timer.reset_session(),
            _ => {}
        }
    }

    fn handle_todo_key(&mut self, key_code: KeyCode) {
        let len = self.tasks.pending().count();
        match key_code {
            KeyCode::Char('a') => self.begin_add(),
            KeyCode::Char('e') => {
                if let Some(index) = self.selected_todo_index() {
                    self.begin_edit(index);
                }
            }
            KeyCode::Char('x') => {
                if let Some(index) = self.selected_todo_index() {
                    self.tasks.delete(index);
                    self.clamp_selections();
                }
            }
            KeyCode::Char(' ') => {
                if let Some(index) = self.selected_todo_index() {
                    self.tasks.complete(index);
                    self.clamp_selections();
                }
            }
            key => {
                if let Some(direction) = row_direction(key) {
                    Self::move_selection(&mut self.todo_selection, len, direction);
                }
            }
        }
    }

    fn handle_done_key(&mut self, key_code: KeyCode) {
        let len = self.tasks.completed().count();
        match key_code {
            KeyCode::Char('e') => {
                if let Some(index) = self.selected_done_index() {
                    self.begin_edit(index);
                }
            }
            KeyCode::Char('x') => {
                if let Some(index) = self.selected_done_index() {
                    self.tasks.delete(index);
                    self.clamp_selections();
                }
            }
            KeyCode::Char(' ') => {
                if let Some(index) = self.selected_done_index() {
                    self.tasks.uncomplete(index);
                    self.clamp_selections();
                }
            }
            key => {
                if let Some(direction) = row_direction(key) {
                    Self::move_selection(&mut self.done_selection, len, direction);
                }
            }
        }
    }

    fn handle_edit_key(&mut self, key: KeyCode) {
        match key {
            KeyCode::Enter => self.submit_edit(),
            KeyCode::Esc => self.cancel_edit(),
            KeyCode::Backspace => {
                self.input.pop();
            }
            KeyCode::Char(character) => self.input.push(character),
            _ => {}
        }
    }

    fn handle_mouse(&mut self, mouse: MouseEvent, layout: UiLayout, now: Instant) {
        if mouse.kind != MouseEventKind::Down(MouseButton::Left) {
            return;
        }

        let position = (mouse.column, mouse.row);
        if layout.clock.contains(position.into()) {
            self.ui_focus = UiFocus::Clock;
            self.handle_click_target(ClickTarget::Clock, now);
        } else if layout.todo.contains(position.into()) {
            self.ui_focus = UiFocus::Todo;
            if let Some(selection) = task_row_at(
                position,
                layout.todo,
                self.todo_offset,
                self.tasks.pending().count(),
            ) {
                self.todo_selection = selection;
                self.handle_click_target(ClickTarget::Todo(selection), now);
            } else {
                self.last_click = None;
            }
        } else if layout.done.contains(position.into()) {
            self.ui_focus = UiFocus::Done;
            if let Some(selection) = task_row_at(
                position,
                layout.done,
                self.done_offset,
                self.tasks.completed().count(),
            ) {
                self.done_selection = selection;
                self.handle_click_target(ClickTarget::Done(selection), now);
            } else {
                self.last_click = None;
            }
        } else {
            self.last_click = None;
        }
    }

    fn handle_click_target(&mut self, target: ClickTarget, now: Instant) {
        let is_double_click = self.last_click.is_some_and(|(last_target, last_time)| {
            last_target == target
                && now
                    .checked_duration_since(last_time)
                    .is_some_and(|elapsed| elapsed <= DOUBLE_CLICK_WINDOW)
        });

        if is_double_click {
            match target {
                ClickTarget::Clock => self.timer.primary_action(),
                ClickTarget::Todo(_) => self.handle_todo_key(KeyCode::Char(' ')),
                ClickTarget::Done(_) => self.handle_done_key(KeyCode::Char(' ')),
            }
            self.last_click = None;
        } else {
            self.last_click = Some((target, now));
        }
    }
}

const DOUBLE_CLICK_WINDOW: Duration = Duration::from_millis(500);

#[derive(Debug, Clone, Copy)]
struct UiLayout {
    clock: Rect,
    todo: Rect,
    done: Rect,
    controls: Rect,
}

fn ui_layout(area: Rect) -> UiLayout {
    let inner_area = Block::default().borders(Borders::ALL).inner(area);
    let chunks = Layout::default()
        .direction(LayoutDirection::Vertical)
        .constraints([
            Constraint::Percentage(55),
            Constraint::Percentage(45),
            Constraint::Length(1),
        ])
        .split(inner_area);
    let task_chunks = Layout::default()
        .direction(LayoutDirection::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(chunks[1]);

    UiLayout {
        clock: chunks[0],
        todo: task_chunks[0],
        done: task_chunks[1],
        controls: chunks[2],
    }
}

fn task_row_at(position: (u16, u16), area: Rect, offset: usize, len: usize) -> Option<usize> {
    let inner = Block::default().borders(Borders::ALL).inner(area);
    let point = position.into();
    if !inner.contains(point) {
        return None;
    }

    let index = offset + usize::from(position.1 - inner.y);
    (index < len).then_some(index)
}

fn main() -> std::io::Result<()> {
    let mut terminal = setup_terminal()?;
    let result = run_app(&mut terminal);
    restore_terminal(&mut terminal)?;

    result
}

fn setup_terminal() -> std::io::Result<Terminal<CrosstermBackend<Stdout>>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);

    Terminal::new(backend)
}

fn run_app(terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> std::io::Result<()> {
    let mut app = App::new();

    let mut last_tick = Instant::now();

    loop {
        let now = Instant::now();
        let elapsed = now.duration_since(last_tick);
        last_tick = now;

        if matches!(app.timer.state(), TimerState::Focus | TimerState::Break) {
            app.timer.tick(elapsed);
        }

        terminal.draw(|frame| {
            draw(frame, &mut app);
        })?;

        if event::poll(Duration::from_millis(100))? {
            match event::read()? {
                Event::Key(key) if app.edit_mode != EditMode::Normal => {
                    app.handle_edit_key(key.code);
                }
                Event::Key(key) => match key.code {
                    KeyCode::Char('q') => break,
                    key_code if app.navigate_focus(key_code) => {}
                    key_code => match app.ui_focus {
                        UiFocus::Clock => app.handle_clock_key(key_code),
                        UiFocus::Todo => app.handle_todo_key(key_code),
                        UiFocus::Done => app.handle_done_key(key_code),
                    },
                },
                Event::Mouse(mouse) if app.edit_mode == EditMode::Normal => {
                    app.handle_mouse(mouse, ui_layout(terminal.size()?.into()), Instant::now());
                }
                _ => {}
            }
        }
    }

    Ok(())
}

fn restore_terminal(terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> std::io::Result<()> {
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        DisableMouseCapture,
        LeaveAlternateScreen
    )?;
    terminal.show_cursor()?;

    Ok(())
}

fn draw(frame: &mut Frame, app: &mut App) {
    let area = frame.area();
    let layout = ui_layout(area);

    let outer_block = Block::default().title("pomock").borders(Borders::ALL);
    frame.render_widget(outer_block, area);

    let clock_block = focused_block("Clock", app.ui_focus == UiFocus::Clock);
    let clock_area = clock_block.inner(layout.clock);
    frame.render_widget(clock_block, layout.clock);

    let clock_chunks = Layout::default()
        .direction(LayoutDirection::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .split(clock_area);

    let state = Paragraph::new(format_state(app.timer.state())).alignment(Alignment::Center);

    let remaining = Paragraph::new(format_big_duration(app.timer.remaining()))
        .alignment(Alignment::Center)
        .style(Style::default().add_modifier(Modifier::BOLD));

    let completed_sessions = Paragraph::new(format!(
        "Focus sessions completed: {}",
        app.timer.completed_focus_sessions()
    ))
    .alignment(Alignment::Center)
    .style(Style::default().fg(Color::Green));

    let controls_text = match app.edit_mode {
        EditMode::Adding => format!("Add task: {}_", app.input),
        EditMode::Editing { .. } => format!("Edit task: {}_", app.input),
        EditMode::Normal => {
            match app.ui_focus {
                UiFocus::Clock => "[HJKL] box nav [space] start/pause [f] next [r] reset [q] quit",
                UiFocus::Todo => "[HJKL] box nav [jk/↓↑] list nav [a] add [e] edit [x] delete [space] complete [q] quit",
                UiFocus::Done => "[HJKL] box nav [jk/↓↑] list nav [e] edit [x] delete [space] return [q] quit",
            }
            .to_string()
        }
    };

    let controls = Paragraph::new(controls_text).alignment(Alignment::Center);

    let todo_items: Vec<ListItem> = app
        .tasks
        .pending()
        .map(|task| ListItem::new(format!("  {}", task.description())))
        .collect();

    let todo_is_empty = todo_items.is_empty();

    let todo = if todo_is_empty {
        List::new(vec![ListItem::new("No tasks yet")])
    } else {
        List::new(todo_items)
    }
    .block(focused_block("To-do", app.ui_focus == UiFocus::Todo))
    .highlight_style(
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
    )
    .highlight_symbol("> ");

    let mut todo_state = ListState::default();

    if !todo_is_empty {
        todo_state.select(Some(app.todo_selection));
        *todo_state.offset_mut() = app.todo_offset;
    }

    let done_items: Vec<ListItem> = app
        .tasks
        .completed()
        .map(|task| ListItem::new(format!("  {}", task.description())))
        .collect();

    let done_is_empty = done_items.is_empty();

    let done = if done_is_empty {
        List::new(vec![ListItem::new("No completed tasks")])
    } else {
        List::new(done_items)
    }
    .block(focused_block("Done", app.ui_focus == UiFocus::Done))
    .highlight_style(
        Style::default()
            .fg(Color::Green)
            .add_modifier(Modifier::BOLD),
    )
    .highlight_symbol("> ");

    let mut done_state = ListState::default();

    if !done_is_empty {
        done_state.select(Some(app.done_selection));
        *done_state.offset_mut() = app.done_offset;
    }

    frame.render_widget(state, clock_chunks[0]);
    frame.render_widget(remaining, clock_chunks[1]);
    frame.render_widget(completed_sessions, clock_chunks[2]);
    frame.render_stateful_widget(todo, layout.todo, &mut todo_state);
    frame.render_stateful_widget(done, layout.done, &mut done_state);
    frame.render_widget(controls, layout.controls);
    app.todo_offset = todo_state.offset();
    app.done_offset = done_state.offset();
}

fn focused_block(title: &str, focused: bool) -> Block<'_> {
    let border_color = if focused {
        Color::Yellow
    } else {
        Color::DarkGray
    };

    Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
}

#[cfg(test)]
mod tests {
    use super::{
        App, Direction, EditMode, UiFocus, focus_direction, row_direction, task_row_at, ui_layout,
    };
    use crossterm::event::{KeyCode, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
    use pomock::timer::{SessionKind, TimerState};
    use ratatui::layout::Rect;
    use std::time::{Duration, Instant};

    fn left_click(column: u16, row: u16) -> MouseEvent {
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column,
            row,
            modifiers: KeyModifiers::NONE,
        }
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
    fn maps_keys_to_directions_by_navigation_context() {
        assert_eq!(focus_direction(KeyCode::Char('H')), Some(Direction::Left));
        assert_eq!(focus_direction(KeyCode::Char('j')), None);
        assert_eq!(row_direction(KeyCode::Char('j')), Some(Direction::Down));
        assert_eq!(row_direction(KeyCode::Up), Some(Direction::Up));
        assert_eq!(row_direction(KeyCode::Char('h')), None);
        assert_eq!(row_direction(KeyCode::Left), None);
    }

    #[test]
    fn begin_add_only_works_from_todo_focus() {
        let mut app = App::new();

        app.begin_add();
        assert_eq!(app.edit_mode, EditMode::Normal);

        app.ui_focus = UiFocus::Todo;
        app.begin_add();
        assert_eq!(app.edit_mode, EditMode::Adding);
    }

    #[test]
    fn submitting_add_creates_a_task_and_returns_to_normal_mode() {
        let mut app = App::new();
        app.ui_focus = UiFocus::Todo;
        app.begin_add();
        app.input.push_str("Write tests");

        app.submit_edit();

        assert_eq!(app.edit_mode, EditMode::Normal);
        assert!(app.input.is_empty());
        assert_eq!(
            app.tasks.pending().next().unwrap().description(),
            "Write tests"
        );
    }

    #[test]
    fn cancelling_add_discards_the_input() {
        let mut app = App::new();
        app.ui_focus = UiFocus::Todo;
        app.begin_add();
        app.input.push_str("Discard me");

        app.cancel_edit();

        assert_eq!(app.edit_mode, EditMode::Normal);
        assert!(app.input.is_empty());
        assert_eq!(app.tasks.pending().count(), 0);
    }

    #[test]
    fn uppercase_vim_keys_move_box_focus() {
        let mut app = App::new();

        assert!(app.navigate_focus(KeyCode::Char('J')));
        assert_eq!(app.ui_focus, UiFocus::Todo);
        assert!(app.navigate_focus(KeyCode::Char('L')));
        assert_eq!(app.ui_focus, UiFocus::Done);
        assert!(!app.navigate_focus(KeyCode::Char('h')));
    }

    #[test]
    fn row_navigation_stays_within_visible_tasks() {
        let mut app = App::new();
        app.tasks.add("First".to_string());
        app.tasks.add("Second".to_string());
        app.ui_focus = UiFocus::Todo;

        app.handle_todo_key(KeyCode::Down);
        app.handle_todo_key(KeyCode::Char('j'));
        assert_eq!(app.todo_selection, 1);

        app.handle_todo_key(KeyCode::Char('k'));
        app.handle_todo_key(KeyCode::Up);
        assert_eq!(app.todo_selection, 0);
    }

    #[test]
    fn empty_list_navigation_keeps_a_safe_selection() {
        let mut app = App::new();
        app.ui_focus = UiFocus::Todo;

        app.handle_todo_key(KeyCode::Down);

        assert_eq!(app.todo_selection, 0);
        assert_eq!(app.selected_todo_index(), None);
    }

    #[test]
    fn editing_selected_filtered_task_updates_the_right_task() {
        let mut app = App::new();
        app.tasks.add("Done".to_string());
        app.tasks.add("Edit me".to_string());
        app.tasks.complete(0);
        app.ui_focus = UiFocus::Todo;

        app.handle_todo_key(KeyCode::Char('e'));
        assert_eq!(app.edit_mode, EditMode::Editing { task_index: 1 });
        assert_eq!(app.input, "Edit me");
        app.input = "Edited".to_string();
        app.submit_edit();

        assert_eq!(app.tasks.pending().next().unwrap().description(), "Edited");
    }

    #[test]
    fn complete_and_uncomplete_clamp_both_selections() {
        let mut app = App::new();
        app.tasks.add("First".to_string());
        app.tasks.add("Second".to_string());
        app.ui_focus = UiFocus::Todo;
        app.todo_selection = 1;

        app.handle_todo_key(KeyCode::Char(' '));
        assert_eq!(app.todo_selection, 0);
        assert_eq!(
            app.tasks.completed().next().unwrap().description(),
            "Second"
        );

        app.ui_focus = UiFocus::Done;
        app.handle_done_key(KeyCode::Char(' '));
        assert_eq!(app.done_selection, 0);
        assert_eq!(app.tasks.completed().count(), 0);
        assert_eq!(app.tasks.pending().count(), 2);
    }

    #[test]
    fn delete_selected_task_clamps_selection() {
        let mut app = App::new();
        app.tasks.add("First".to_string());
        app.tasks.add("Second".to_string());
        app.ui_focus = UiFocus::Todo;
        app.todo_selection = 1;

        app.handle_todo_key(KeyCode::Char('x'));

        assert_eq!(app.todo_selection, 0);
        assert_eq!(app.tasks.pending().count(), 1);
        assert_eq!(app.tasks.pending().next().unwrap().description(), "First");
    }

    #[test]
    fn task_hit_testing_ignores_borders_empty_space_and_empty_lists() {
        let area = Rect::new(10, 5, 12, 5);

        assert_eq!(task_row_at((10, 6), area, 0, 3), None);
        assert_eq!(task_row_at((11, 5), area, 0, 3), None);
        assert_eq!(task_row_at((11, 6), area, 0, 0), None);
        assert_eq!(task_row_at((11, 9), area, 0, 3), None);
    }

    #[test]
    fn task_hit_testing_maps_visible_rows_through_the_scroll_offset() {
        let area = Rect::new(10, 5, 12, 5);

        assert_eq!(task_row_at((11, 6), area, 4, 8), Some(4));
        assert_eq!(task_row_at((20, 8), area, 4, 8), Some(6));
    }

    #[test]
    fn clicking_boxes_focuses_them_and_clicking_a_task_selects_its_row() {
        let mut app = App::new();
        app.tasks.add("First".to_string());
        app.tasks.add("Second".to_string());
        let layout = ui_layout(Rect::new(0, 0, 80, 24));
        let now = Instant::now();

        app.handle_mouse(
            left_click(layout.todo.x + 1, layout.todo.y + 2),
            layout,
            now,
        );
        assert_eq!(app.ui_focus, UiFocus::Todo);
        assert_eq!(app.todo_selection, 1);

        app.handle_mouse(left_click(layout.clock.x, layout.clock.y), layout, now);
        assert_eq!(app.ui_focus, UiFocus::Clock);
    }

    #[test]
    fn double_clicking_the_clock_runs_its_primary_action_once() {
        let mut app = App::new();
        let layout = ui_layout(Rect::new(0, 0, 80, 24));
        let click = left_click(layout.clock.x + 1, layout.clock.y + 1);
        let first = Instant::now();

        app.handle_mouse(click, layout, first);
        assert_eq!(app.timer.state(), TimerState::Ready(SessionKind::Focus));

        app.handle_mouse(click, layout, first + Duration::from_millis(200));
        assert_eq!(app.timer.state(), TimerState::Focus);

        app.handle_mouse(click, layout, first + Duration::from_millis(300));
        assert_eq!(app.timer.state(), TimerState::Focus);
    }

    #[test]
    fn clock_clicks_outside_the_double_click_window_stay_single_clicks() {
        let mut app = App::new();
        let layout = ui_layout(Rect::new(0, 0, 80, 24));
        let click = left_click(layout.clock.x + 1, layout.clock.y + 1);
        let first = Instant::now();

        app.handle_mouse(click, layout, first);
        app.handle_mouse(click, layout, first + Duration::from_millis(501));

        assert_eq!(app.timer.state(), TimerState::Ready(SessionKind::Focus));
    }

    #[test]
    fn double_clicking_a_todo_row_completes_that_task() {
        let mut app = App::new();
        app.tasks.add("First".to_string());
        app.tasks.add("Complete me".to_string());
        let layout = ui_layout(Rect::new(0, 0, 80, 24));
        let click = left_click(layout.todo.x + 1, layout.todo.y + 2);
        let first = Instant::now();

        app.handle_mouse(click, layout, first);
        app.handle_mouse(click, layout, first + Duration::from_millis(200));

        assert_eq!(app.tasks.pending().count(), 1);
        assert_eq!(
            app.tasks.completed().next().unwrap().description(),
            "Complete me"
        );
    }

    #[test]
    fn double_clicking_a_done_row_returns_that_task_to_todo() {
        let mut app = App::new();
        app.tasks.add("Return me".to_string());
        app.tasks.complete(0);
        let layout = ui_layout(Rect::new(0, 0, 80, 24));
        let click = left_click(layout.done.x + 1, layout.done.y + 1);
        let first = Instant::now();

        app.handle_mouse(click, layout, first);
        app.handle_mouse(click, layout, first + Duration::from_millis(200));

        assert_eq!(app.tasks.completed().count(), 0);
        assert_eq!(
            app.tasks.pending().next().unwrap().description(),
            "Return me"
        );
    }

    #[test]
    fn clicks_on_different_task_rows_do_not_form_a_double_click() {
        let mut app = App::new();
        app.tasks.add("First".to_string());
        app.tasks.add("Second".to_string());
        let layout = ui_layout(Rect::new(0, 0, 80, 24));
        let first = Instant::now();

        app.handle_mouse(
            left_click(layout.todo.x + 1, layout.todo.y + 1),
            layout,
            first,
        );
        app.handle_mouse(
            left_click(layout.todo.x + 1, layout.todo.y + 2),
            layout,
            first + Duration::from_millis(200),
        );

        assert_eq!(app.tasks.pending().count(), 2);
        assert_eq!(app.tasks.completed().count(), 0);
    }
}
