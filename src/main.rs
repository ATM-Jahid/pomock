use std::io::{self, Stdout};

use crossterm::{
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, MouseButton, MouseEvent,
        MouseEventKind,
    },
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};

use ratatui::{Terminal, backend::CrosstermBackend, layout::Rect};

use std::time::{Duration, Instant};

use pomock::{
    app::{App, AppOutcome, EditMode},
    input::map_key,
    ui::{click_target, draw},
};

fn handle_mouse(app: &mut App, mouse: MouseEvent, area: Rect, now: Instant) {
    if mouse.kind != MouseEventKind::Down(MouseButton::Left) {
        return;
    }

    let target = click_target(area, (mouse.column, mouse.row), app);
    app.handle_click_target(target, now);
}

fn handle_outcome(outcome: AppOutcome) -> bool {
    match outcome {
        AppOutcome::None => false,
        // Completion has no configured external effect yet. Notifications and
        // sound can be connected here without coupling them to App.
        AppOutcome::SessionCompleted(_) => false,
        AppOutcome::Quit => true,
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

        if handle_outcome(app.tick(elapsed)) {
            break;
        }

        terminal.draw(|frame| {
            draw(frame, &mut app);
        })?;

        if event::poll(Duration::from_millis(100))? {
            match event::read()? {
                Event::Key(key) => {
                    if let Some(action) = map_key(key.code, app.edit_mode(), app.ui_focus())
                        && handle_outcome(app.dispatch(action))
                    {
                        break;
                    }
                }
                Event::Mouse(mouse) if app.edit_mode() == EditMode::Normal => {
                    handle_mouse(&mut app, mouse, terminal.size()?.into(), Instant::now());
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
