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
    style::{Modifier, Style},
    widgets::{Block, Borders, Paragraph},
};

use std::time::{Duration, Instant};

use display::{format_big_duration, format_state};
use timer::{PomodoroTimer, TimerState};

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
    let mut timer = PomodoroTimer::new(Duration::from_secs(25 * 60), Duration::from_secs(5 * 60));

    let mut last_tick = Instant::now();

    loop {
        let now = Instant::now();
        let elapsed = now.duration_since(last_tick);
        last_tick = now;

        if matches!(timer.state(), TimerState::Focus | TimerState::Break) {
            timer.tick(elapsed);
        }

        terminal.draw(|frame| {
            draw(frame, &timer);
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

fn draw(frame: &mut Frame, timer: &PomodoroTimer) {
    let area = frame.area();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(25),
            Constraint::Length(3),
            Constraint::Length(7),
            Constraint::Length(3),
            Constraint::Percentage(25),
        ])
        .split(area);

    let block = Block::default().title("pomock").borders(Borders::ALL);

    frame.render_widget(block, area);

    let state = Paragraph::new(format_state(timer.state())).alignment(Alignment::Center);

    let remaining = Paragraph::new(format_big_duration(timer.remaining()))
        .alignment(Alignment::Center)
        .style(Style::default().add_modifier(Modifier::BOLD));

    let controls = Paragraph::new("[f] focus [b] break [space] pause/resume [x] reset [q] quit")
        .alignment(Alignment::Center);

    frame.render_widget(state, chunks[1]);
    frame.render_widget(remaining, chunks[2]);
    frame.render_widget(controls, chunks[3]);
}
