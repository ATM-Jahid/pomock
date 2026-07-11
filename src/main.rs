use std::io::{self, Stdout};

use crossterm::{
    event::{self, Event, KeyCode},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};

use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout},
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

impl UiFocus {
    fn navigate(self, key: char) -> Self {
        match (self, key) {
            (Self::Clock, 'j') => Self::Todo,
            (Self::Todo | Self::Done, 'k') => Self::Clock,
            (Self::Todo, 'l') => Self::Done,
            (Self::Done, 'h') => Self::Todo,
            _ => self,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EditMode {
    Normal,
    Adding,
    Editing { task_index: usize },
}

#[derive(Debug)]
struct App {
    timer: PomodoroTimer,
    tasks: TaskList,
    ui_focus: UiFocus,
    todo_selection: usize,
    done_selection: usize,
    edit_mode: EditMode,
    input: String,
}

impl App {
    fn new() -> Self {
        Self {
            timer: PomodoroTimer::new(Duration::from_secs(25 * 60), Duration::from_secs(5 * 60)),
            tasks: TaskList::new(),
            ui_focus: UiFocus::Clock,
            todo_selection: 0,
            done_selection: 0,
            edit_mode: EditMode::Normal,
            input: String::new(),
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
    execute!(stdout, EnterAlternateScreen)?;
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
            draw(frame, &app);
        })?;

        if event::poll(Duration::from_millis(100))?
            && let Event::Key(key) = event::read()?
        {
            if app.edit_mode != EditMode::Normal {
                app.handle_edit_key(key.code);
            } else {
                match key.code {
                    KeyCode::Char('q') => break,
                    KeyCode::Char('a') => app.begin_add(),
                    KeyCode::Char(' ') => app.timer.primary_action(),
                    KeyCode::Char('f') => app.timer.fast_forward(),
                    KeyCode::Char('r') => app.timer.reset_session(),
                    KeyCode::Char(key @ ('h' | 'j' | 'k' | 'l')) => {
                        app.ui_focus = app.ui_focus.navigate(key);
                    }
                    _ => {}
                }
            }
        }
    }

    Ok(())
}

fn restore_terminal(terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> std::io::Result<()> {
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    Ok(())
}

fn draw(frame: &mut Frame, app: &App) {
    let area = frame.area();

    let outer_block = Block::default().title("pomock").borders(Borders::ALL);
    let inner_area = outer_block.inner(area);
    frame.render_widget(outer_block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(55),
            Constraint::Percentage(45),
            Constraint::Length(1),
        ])
        .split(inner_area);

    let task_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(chunks[1]);

    let clock_block = focused_block("Clock", app.ui_focus == UiFocus::Clock);
    let clock_area = clock_block.inner(chunks[0]);
    frame.render_widget(clock_block, chunks[0]);

    let clock_chunks = Layout::default()
        .direction(Direction::Vertical)
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
            "[h/j/k/l] move [a] add [space] action [f] next [r] reset [q] quit".to_string()
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
    }

    frame.render_widget(state, clock_chunks[0]);
    frame.render_widget(remaining, clock_chunks[1]);
    frame.render_widget(completed_sessions, clock_chunks[2]);
    frame.render_stateful_widget(todo, task_chunks[0], &mut todo_state);
    frame.render_stateful_widget(done, task_chunks[1], &mut done_state);
    frame.render_widget(controls, chunks[2]);
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
    use super::{App, EditMode, UiFocus};

    #[test]
    fn navigates_between_adjacent_areas() {
        assert_eq!(UiFocus::Clock.navigate('j'), UiFocus::Todo);
        assert_eq!(UiFocus::Todo.navigate('k'), UiFocus::Clock);
        assert_eq!(UiFocus::Todo.navigate('l'), UiFocus::Done);
        assert_eq!(UiFocus::Done.navigate('h'), UiFocus::Todo);
        assert_eq!(UiFocus::Done.navigate('k'), UiFocus::Clock);
    }

    #[test]
    fn ignores_directions_without_an_adjacent_area() {
        assert_eq!(UiFocus::Clock.navigate('h'), UiFocus::Clock);
        assert_eq!(UiFocus::Clock.navigate('k'), UiFocus::Clock);
        assert_eq!(UiFocus::Clock.navigate('l'), UiFocus::Clock);
        assert_eq!(UiFocus::Todo.navigate('h'), UiFocus::Todo);
        assert_eq!(UiFocus::Todo.navigate('j'), UiFocus::Todo);
        assert_eq!(UiFocus::Done.navigate('j'), UiFocus::Done);
        assert_eq!(UiFocus::Done.navigate('l'), UiFocus::Done);
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
}
