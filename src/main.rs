use std::io::{self, Stdout};

use crossterm::{
    event::{self, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};

use ratatui::{
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout},
    style::{Modifier, Style},
    widgets::{Block, Borders, Paragraph},
    Frame, Terminal,
};

use std::time::Duration;

use display::{format_duration, format_state};
use timer::PomodoroTimer;

mod display;
mod timer;

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
    let timer = PomodoroTimer::new(
        Duration::from_secs(25 * 60),
        Duration::from_secs(5 * 60),
    );

    loop {
        terminal.draw(|frame| {
            draw(frame, &timer);
        })?;

        if let Event::Key(key) = event::read()? {
            if key.code == KeyCode::Char('q') {
                break;
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

fn draw(frame: &mut Frame, timer: &PomodoroTimer) {
    let area = frame.area();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(35),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Percentage(35),
        ])
        .split(area);

    let block = Block::default()
        .title("pomock")
        .borders(Borders::ALL);

    frame.render_widget(block, area);

    let state = Paragraph::new(format_state(timer.state()))
        .alignment(Alignment::Center);

    let remaining = Paragraph::new(format_duration(timer.remaining()))
        .alignment(Alignment::Center)
        .style(Style::default().add_modifier(Modifier::BOLD));

    frame.render_widget(state, chunks[1]);
    frame.render_widget(remaining, chunks[2]);
}
