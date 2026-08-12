use std::{
    io::Stdout,
    time::{Duration, Instant},
};

use crossterm::event::{self, Event, KeyEventKind, MouseButton, MouseEvent, MouseEventKind};
use pomock::{
    app::{Action, App, AppOutcome, Direction, EditMode, TaskState},
    config::Config,
    input::map_key_event,
    notification::DesktopNotifier,
    persistence::TaskStore,
    sound::FileSoundPlayer,
    ui::{FrameGeometry, Theme, action_target_visible, click_target, draw, scroll_target},
};
use ratatui::{Terminal, backend::CrosstermBackend};

mod effects;
mod terminal;

pub(crate) use effects::task_store_for_config;
#[cfg(test)]
pub(crate) use effects::{RunError, commit_settings_change, handle_outcome};
pub(crate) use terminal::{TerminalSession, combine_run_and_restore_results};

pub(crate) fn handle_mouse(
    app: &mut App,
    mouse: MouseEvent,
    geometry: &FrameGeometry,
    now: Instant,
) -> AppOutcome {
    match mouse.kind {
        MouseEventKind::Down(MouseButton::Left) => {
            let target = click_target(geometry, (mouse.column, mouse.row), app);
            app.handle_click_target(target, now)
        }
        MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => {
            let Some(target) = scroll_target(geometry, (mouse.column, mouse.row), app) else {
                return AppOutcome::None;
            };
            let direction = if mouse.kind == MouseEventKind::ScrollUp {
                Direction::Up
            } else {
                Direction::Down
            };
            app.dispatch(Action::Scroll(target, direction))
        }
        _ => AppOutcome::None,
    }
}

pub(crate) fn should_handle_key_event(kind: KeyEventKind) -> bool {
    kind != KeyEventKind::Release
}

pub(crate) fn advance_timer(app: &mut App, last_tick: &mut Instant, now: Instant) -> AppOutcome {
    let elapsed = now.duration_since(*last_tick);
    *last_tick = now;
    app.tick(elapsed)
}

pub(crate) fn run_app(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    mut config: Config,
    mut task_store: Option<TaskStore>,
    task_state: TaskState,
    workspace_store: TaskStore,
) -> Result<(), effects::RunError> {
    let mut app = App::from_config_and_tasks(&config, task_state);
    let mut notifier = DesktopNotifier;
    let mut sound_player = FileSoundPlayer::default();

    let mut last_tick = Instant::now();

    loop {
        let now = Instant::now();
        let outcome = advance_timer(&mut app, &mut last_tick, now);
        if effects::handle_outcome(
            outcome,
            &mut app,
            &mut config,
            &mut task_store,
            &workspace_store,
            &mut notifier,
            &mut sound_player,
        )? {
            break;
        }

        let mut frame_geometry = None;
        terminal.draw(|frame| {
            frame_geometry = Some(draw(
                frame,
                &mut app,
                Theme::from(config.theme()),
                config.keys(),
            ));
        })?;
        let frame_geometry = frame_geometry.expect("terminal draw must resolve frame geometry");

        if event::poll(Duration::from_millis(100))? {
            let event = event::read()?;
            let now = Instant::now();
            let outcome = advance_timer(&mut app, &mut last_tick, now);
            if effects::handle_outcome(
                outcome,
                &mut app,
                &mut config,
                &mut task_store,
                &workspace_store,
                &mut notifier,
                &mut sound_player,
            )? {
                break;
            }

            match event {
                Event::Key(key) if should_handle_key_event(key.kind) => {
                    app.clear_task_save_message();
                    if let Some(action) = map_key_event(
                        key,
                        app.edit_mode(),
                        app.ui_focus(),
                        app.is_confirmation_open(),
                        app.settings_mode(),
                        app.input_keys(),
                    ) && action_target_visible(&frame_geometry, app.ui_focus(), &action)
                    {
                        let outcome = app.dispatch(action);
                        if effects::handle_outcome(
                            outcome,
                            &mut app,
                            &mut config,
                            &mut task_store,
                            &workspace_store,
                            &mut notifier,
                            &mut sound_player,
                        )? {
                            break;
                        }
                    }
                }
                Event::Mouse(mouse) => {
                    if matches!(mouse.kind, MouseEventKind::Down(_)) {
                        app.clear_task_save_message();
                    }
                    if app.edit_mode() == EditMode::Normal {
                        let outcome = handle_mouse(&mut app, mouse, &frame_geometry, now);
                        if effects::handle_outcome(
                            outcome,
                            &mut app,
                            &mut config,
                            &mut task_store,
                            &workspace_store,
                            &mut notifier,
                            &mut sound_player,
                        )? {
                            break;
                        }
                    }
                }
                _ => {}
            }
        }
    }

    Ok(())
}
