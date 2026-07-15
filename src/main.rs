use std::io::{self, Stdout};

use crossterm::{
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyEventKind, MouseButton,
        MouseEvent, MouseEventKind,
    },
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};

use ratatui::{Terminal, backend::CrosstermBackend, layout::Rect};

use std::time::{Duration, Instant};

use pomock::{
    app::{App, AppOutcome, EditMode},
    config::Config,
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

fn should_handle_key_event(kind: KeyEventKind) -> bool {
    kind != KeyEventKind::Release
}

fn advance_timer(app: &mut App, last_tick: &mut Instant, now: Instant) -> AppOutcome {
    let elapsed = now.duration_since(*last_tick);
    *last_tick = now;
    app.tick(elapsed)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = Config::load()?;
    let mut session = TerminalSession::start()?;
    let run_result = run_app(session.terminal_mut(), &config);
    let restore_result = session.restore();

    Ok(combine_run_and_restore_results(run_result, restore_result)?)
}

struct TerminalSession {
    terminal: Terminal<CrosstermBackend<Stdout>>,
    restore_required: bool,
}

impl TerminalSession {
    fn start() -> io::Result<Self> {
        enable_raw_mode()?;
        let mut rollback = SetupRollback::new();
        let mut stdout = io::stdout();

        rollback.alternate_screen = true;
        execute!(stdout, EnterAlternateScreen)?;
        rollback.mouse_capture = true;
        execute!(stdout, EnableMouseCapture)?;

        let backend = CrosstermBackend::new(stdout);
        let terminal = Terminal::new(backend)?;
        rollback.disarm();

        Ok(Self {
            terminal,
            restore_required: true,
        })
    }

    fn terminal_mut(&mut self) -> &mut Terminal<CrosstermBackend<Stdout>> {
        &mut self.terminal
    }

    fn restore(&mut self) -> io::Result<()> {
        if !self.restore_required {
            return Ok(());
        }
        self.restore_required = false;

        let mut first_error = None;
        record_error(
            &mut first_error,
            execute!(self.terminal.backend_mut(), DisableMouseCapture),
        );
        record_error(
            &mut first_error,
            execute!(self.terminal.backend_mut(), LeaveAlternateScreen),
        );
        record_error(&mut first_error, self.terminal.show_cursor());
        record_error(&mut first_error, disable_raw_mode());

        first_error.map_or(Ok(()), Err)
    }
}

impl Drop for TerminalSession {
    fn drop(&mut self) {
        let _ = self.restore();
    }
}

struct SetupRollback {
    armed: bool,
    alternate_screen: bool,
    mouse_capture: bool,
}

impl SetupRollback {
    fn new() -> Self {
        Self {
            armed: true,
            alternate_screen: false,
            mouse_capture: false,
        }
    }

    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for SetupRollback {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }

        let mut stdout = io::stdout();
        if self.mouse_capture {
            let _ = execute!(stdout, DisableMouseCapture);
        }
        if self.alternate_screen {
            let _ = execute!(stdout, LeaveAlternateScreen);
        }
        let _ = disable_raw_mode();
    }
}

fn record_error(first_error: &mut Option<io::Error>, result: io::Result<()>) {
    if let Err(error) = result
        && first_error.is_none()
    {
        *first_error = Some(error);
    }
}

fn combine_run_and_restore_results(
    run_result: io::Result<()>,
    restore_result: io::Result<()>,
) -> io::Result<()> {
    match (run_result, restore_result) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(error), Ok(())) | (Ok(()), Err(error)) => Err(error),
        (Err(run_error), Err(restore_error)) => Err(io::Error::new(
            run_error.kind(),
            format!("{run_error}; terminal restoration also failed: {restore_error}"),
        )),
    }
}

fn run_app(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    config: &Config,
) -> std::io::Result<()> {
    let mut app = App::from_config(config);

    let mut last_tick = Instant::now();

    loop {
        let now = Instant::now();
        if handle_outcome(advance_timer(&mut app, &mut last_tick, now)) {
            break;
        }

        terminal.draw(|frame| {
            draw(frame, &mut app);
        })?;

        if event::poll(Duration::from_millis(100))? {
            let event = event::read()?;
            let now = Instant::now();
            if handle_outcome(advance_timer(&mut app, &mut last_tick, now)) {
                break;
            }

            match event {
                Event::Key(key) if should_handle_key_event(key.kind) => {
                    if let Some(action) = map_key(
                        key.code,
                        app.edit_mode(),
                        app.ui_focus(),
                        app.is_confirmation_open(),
                    ) && handle_outcome(app.dispatch(action))
                    {
                        break;
                    }
                }
                Event::Mouse(mouse) if app.edit_mode() == EditMode::Normal => {
                    handle_mouse(&mut app, mouse, terminal.size()?.into(), now);
                }
                _ => {}
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use pomock::app::Action;

    #[test]
    fn key_releases_are_ignored_while_presses_and_repeats_are_handled() {
        assert!(should_handle_key_event(KeyEventKind::Press));
        assert!(should_handle_key_event(KeyEventKind::Repeat));
        assert!(!should_handle_key_event(KeyEventKind::Release));
    }

    #[test]
    fn ready_time_before_start_is_not_charged_to_the_running_session() {
        let mut app = App::new();
        let start = Instant::now();
        let mut last_tick = start;
        let key_time = start + Duration::from_millis(80);

        assert_eq!(
            advance_timer(&mut app, &mut last_tick, key_time),
            AppOutcome::None
        );
        assert_eq!(app.dispatch(Action::PrimaryAction), AppOutcome::None);

        assert_eq!(
            advance_timer(
                &mut app,
                &mut last_tick,
                key_time + Duration::from_secs(25 * 60) - Duration::from_millis(1),
            ),
            AppOutcome::None
        );
        assert_eq!(
            advance_timer(
                &mut app,
                &mut last_tick,
                key_time + Duration::from_secs(25 * 60),
            ),
            AppOutcome::SessionCompleted(pomock::SessionKind::Focus)
        );
    }

    #[test]
    fn run_error_is_preserved_when_restoration_succeeds() {
        let run_error = io::Error::new(io::ErrorKind::BrokenPipe, "run failed");

        let error = combine_run_and_restore_results(Err(run_error), Ok(())).unwrap_err();

        assert_eq!(error.kind(), io::ErrorKind::BrokenPipe);
        assert_eq!(error.to_string(), "run failed");
    }

    #[test]
    fn restoration_error_is_reported_after_a_successful_run() {
        let restore_error = io::Error::other("restore failed");

        let error = combine_run_and_restore_results(Ok(()), Err(restore_error)).unwrap_err();

        assert_eq!(error.to_string(), "restore failed");
    }

    #[test]
    fn simultaneous_run_and_restoration_errors_are_both_reported() {
        let run_error = io::Error::new(io::ErrorKind::BrokenPipe, "run failed");
        let restore_error = io::Error::other("restore failed");

        let error =
            combine_run_and_restore_results(Err(run_error), Err(restore_error)).unwrap_err();

        assert_eq!(error.kind(), io::ErrorKind::BrokenPipe);
        assert_eq!(
            error.to_string(),
            "run failed; terminal restoration also failed: restore failed"
        );
    }
}
