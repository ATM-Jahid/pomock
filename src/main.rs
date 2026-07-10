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
    widgets::{Block, Borders, Paragraph},
};

use std::time::{Duration, Instant};

use display::{format_big_duration, format_state};
use timer::{PomodoroTimer, TimerState};

mod display;
mod timer;

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
    let mut timer = PomodoroTimer::new(Duration::from_secs(25 * 60), Duration::from_secs(5 * 60));
    let mut ui_focus = UiFocus::Clock;

    let mut last_tick = Instant::now();

    loop {
        let now = Instant::now();
        let elapsed = now.duration_since(last_tick);
        last_tick = now;

        if matches!(timer.state(), TimerState::Focus | TimerState::Break) {
            timer.tick(elapsed);
        }

        terminal.draw(|frame| {
            draw(frame, &timer, ui_focus);
        })?;

        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('q') => break,
                    KeyCode::Char('f') => timer.start_focus(),
                    KeyCode::Char('b') => timer.start_break(),
                    KeyCode::Char(' ') => match timer.state() {
                        TimerState::Focus | TimerState::Break => timer.pause(),
                        TimerState::Paused => timer.resume(),
                        TimerState::Idle | TimerState::Completed => {}
                    },
                    KeyCode::Char('x') => timer.reset(),
                    KeyCode::Char(key @ ('h' | 'j' | 'k' | 'l')) => {
                        ui_focus = ui_focus.navigate(key);
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

fn draw(frame: &mut Frame, timer: &PomodoroTimer, ui_focus: UiFocus) {
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

    let clock_block = focused_block("Clock", ui_focus == UiFocus::Clock);
    let clock_area = clock_block.inner(chunks[0]);
    frame.render_widget(clock_block, chunks[0]);

    let clock_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(1)])
        .split(clock_area);

    let state = Paragraph::new(format_state(timer.state())).alignment(Alignment::Center);

    let remaining = Paragraph::new(format_big_duration(timer.remaining()))
        .alignment(Alignment::Center)
        .style(Style::default().add_modifier(Modifier::BOLD));

    let controls = Paragraph::new(
        "[h/j/k/l] move [f] focus [b] break [space] pause/resume [x] reset [q] quit",
    )
    .alignment(Alignment::Center);

    let todo = Paragraph::new("No tasks yet")
        .alignment(Alignment::Center)
        .block(focused_block("To-do", ui_focus == UiFocus::Todo));
    let done = Paragraph::new("No completed tasks")
        .alignment(Alignment::Center)
        .block(focused_block("Done", ui_focus == UiFocus::Done));

    frame.render_widget(state, clock_chunks[0]);
    frame.render_widget(remaining, clock_chunks[1]);
    frame.render_widget(todo, task_chunks[0]);
    frame.render_widget(done, task_chunks[1]);
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
    use super::UiFocus;

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
}
