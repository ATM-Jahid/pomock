use std::io::{self, Stdout};

use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};

use super::effects::RunError;

pub(crate) struct TerminalSession {
    terminal: Terminal<CrosstermBackend<Stdout>>,
    restore_required: bool,
}

impl TerminalSession {
    pub(crate) fn start() -> io::Result<Self> {
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

    pub(crate) fn terminal_mut(&mut self) -> &mut Terminal<CrosstermBackend<Stdout>> {
        &mut self.terminal
    }

    pub(crate) fn restore(&mut self) -> io::Result<()> {
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

pub(crate) fn combine_run_and_restore_results(
    run_result: Result<(), RunError>,
    restore_result: io::Result<()>,
) -> Result<(), RunError> {
    match (run_result, restore_result) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(error), Ok(())) => Err(error),
        (Ok(()), Err(error)) => Err(RunError::Terminal(error)),
        (Err(run), Err(restore)) => Err(RunError::TerminalRestoration {
            run: Box::new(run),
            restore,
        }),
    }
}
