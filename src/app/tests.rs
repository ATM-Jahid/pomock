use std::time::{Duration, Instant};

use crate::{
    config::{Config, ConfigKey, KeyAction, TasksConfig, TimerConfig},
    settings::SettingField,
    timer::{SessionKind, TimerState},
};

use super::{
    Action, App, AppOutcome, ClickTarget, ConfirmationOperation, Direction, EditMode,
    FocusAudioAction, ScrollTarget, SettingsAdjustmentDirection, SettingsMode,
    SettingsMoveDirection, TaskState, TimerChange, UiFocus,
};

fn add_task(app: &mut App, description: &str) {
    let _ = app.dispatch(Action::NavigateFocus(Direction::Down));
    let _ = app.dispatch(Action::BeginAdd);
    for character in description.chars() {
        let _ = app.dispatch(Action::PushInput(character));
    }
    let _ = app.dispatch(Action::SubmitEdit);
    let _ = app.dispatch(Action::NavigateFocus(Direction::Up));
}

fn move_settings_to(app: &mut App, field: SettingField) {
    while app.settings().unwrap().field() != field {
        let _ = app.dispatch(Action::SettingsMove(SettingsMoveDirection::Down));
    }
}

fn double_click_session(app: &mut App, session: SessionKind, first_click: Instant) {
    let target = ClickTarget::SessionControl(session);
    let _ = app.handle_click_target(target, first_click);
    let _ = app.handle_click_target(target, first_click + Duration::from_millis(100));
}

fn active_focus(progress: Duration, pause: bool) -> App {
    let mut app = App::new();
    let _ = app.dispatch(Action::PrimaryAction);
    let _ = app.tick(progress);
    if pause {
        let _ = app.dispatch(Action::PrimaryAction);
    }
    app
}

#[test]
fn configured_durations_and_interval_drive_the_timer() {
    let config = Config::new(TimerConfig::new(2, 1, 3, 2).unwrap()).unwrap();
    let mut app = App::from_config(&config);

    assert_eq!(app.timer().remaining(), Duration::from_secs(2 * 60));

    for expected_next in [SessionKind::ShortBreak, SessionKind::LongBreak] {
        let _ = app.dispatch(Action::PrimaryAction);
        assert_eq!(
            app.tick(Duration::from_secs(2 * 60)),
            AppOutcome::SessionCompleted(SessionKind::Focus)
        );
        assert_eq!(app.timer().state(), TimerState::Ready(expected_next));
        let _ = app.dispatch(Action::CycleSession);
        if expected_next == SessionKind::ShortBreak {
            let _ = app.dispatch(Action::CycleSession);
        }
    }
}

#[test]
fn configured_task_numbering_is_available_to_the_ui() {
    assert!(App::new().show_task_numbers());

    let config = Config::with_tasks(
        TimerConfig::default(),
        TasksConfig::with_numbering(true, false),
    )
    .unwrap();

    assert!(!App::from_config(&config).show_task_numbers());
}

#[test]
fn durable_task_state_restores_order_and_completion() {
    let state = TaskState::from_lists(
        vec!["Pending first".to_owned(), "Pending second".to_owned()],
        vec!["Completed first".to_owned()],
    );

    let app = App::from_config_and_tasks(&Config::default(), state.clone());

    assert_eq!(app.task_state(), state);
    assert_eq!(
        app.tasks().completed().next().unwrap().description(),
        "Completed first"
    );
    assert_eq!(
        app.tasks().pending().next().unwrap().description(),
        "Pending first"
    );
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

    assert_eq!(
        app.dispatch(Action::PrimaryAction),
        AppOutcome::TasksChanged
    );
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
fn task_write_errors_expire_after_three_seconds() {
    let mut app = App::new();
    app.report_task_write_error("save failed".to_owned(), "detail".to_owned());

    let _ = app.tick(Duration::from_secs(2));
    assert_eq!(app.task_write_error(), Some("save failed"));
    let _ = app.tick(Duration::from_secs(1));
    assert_eq!(app.task_write_error(), None);
    assert_eq!(app.write_error_log(), ["detail"]);
}

#[test]
fn config_write_errors_use_the_settings_overlay_and_expire_after_three_seconds() {
    let mut app = App::new();
    let _ = app.dispatch(Action::OpenSettings);
    app.report_config_write_error("config save failed".to_owned(), "detail".to_owned());

    assert_eq!(
        app.settings().unwrap().write_error(),
        Some("config save failed")
    );
    let _ = app.tick(Duration::from_secs(2));
    assert_eq!(
        app.settings().unwrap().write_error(),
        Some("config save failed")
    );
    let _ = app.tick(Duration::from_secs(1));
    assert_eq!(app.settings().unwrap().write_error(), None);
}

#[test]
fn focus_audio_lifecycle_follows_timer_transitions_and_confirmations() {
    let mut app = App::new();

    assert_eq!(
        app.dispatch(Action::PrimaryAction),
        AppOutcome::FocusAudio(FocusAudioAction::StartOrResume)
    );
    assert_eq!(
        app.dispatch(Action::PrimaryAction),
        AppOutcome::FocusAudio(FocusAudioAction::Pause)
    );
    assert_eq!(
        app.dispatch(Action::PrimaryAction),
        AppOutcome::FocusAudio(FocusAudioAction::StartOrResume)
    );
    let _ = app.tick(Duration::from_secs(10));
    assert_eq!(
        app.dispatch(Action::CycleSession),
        AppOutcome::FocusAudio(FocusAudioAction::Pause)
    );
    assert_eq!(
        app.dispatch(Action::CancelPendingAction),
        AppOutcome::FocusAudio(FocusAudioAction::StartOrResume)
    );
    assert_eq!(
        app.dispatch(Action::CycleSession),
        AppOutcome::FocusAudio(FocusAudioAction::Pause)
    );
    assert_eq!(
        app.dispatch(Action::ConfirmPendingAction),
        AppOutcome::FocusAudio(FocusAudioAction::Stop)
    );
}

#[test]
fn focus_audio_lifecycle_is_emitted_for_mouse_timer_controls() {
    let mut app = App::new();
    let now = Instant::now();

    assert_eq!(
        app.handle_click_target(ClickTarget::Clock, now),
        AppOutcome::None
    );
    assert_eq!(
        app.handle_click_target(ClickTarget::Clock, now + Duration::from_millis(100)),
        AppOutcome::FocusAudio(FocusAudioAction::StartOrResume)
    );
    let target = ClickTarget::SessionControl(SessionKind::Focus);
    assert_eq!(
        app.handle_click_target(target, now + Duration::from_millis(200)),
        AppOutcome::None
    );
    assert_eq!(
        app.handle_click_target(target, now + Duration::from_millis(300)),
        AppOutcome::FocusAudio(FocusAudioAction::Pause)
    );
}

#[test]
fn quit_below_ten_seconds_is_immediate_for_running_and_paused_sessions() {
    for initially_paused in [false, true] {
        let mut app = active_focus(Duration::from_secs(9), initially_paused);

        assert_eq!(app.dispatch(Action::Quit), AppOutcome::Quit);
        assert!(!app.is_confirmation_open());
    }
}

#[test]
fn quit_at_ten_seconds_pauses_and_requests_confirmation() {
    for initially_paused in [false, true] {
        let mut app = active_focus(Duration::from_secs(10), initially_paused);

        let expected = if initially_paused {
            AppOutcome::None
        } else {
            AppOutcome::FocusAudio(FocusAudioAction::Pause)
        };
        assert_eq!(app.dispatch(Action::Quit), expected);

        assert!(app.is_confirmation_open());
        assert_eq!(
            app.pending_confirmation(),
            Some(ConfirmationOperation::Quit)
        );
        assert_eq!(app.timer().state(), TimerState::Paused(SessionKind::Focus));
        assert_eq!(app.timer().remaining(), Duration::from_secs(25 * 60 - 10));
    }
}

#[test]
fn confirming_quit_emits_quit_outcome() {
    let mut app = active_focus(Duration::from_secs(10), false);
    assert_eq!(
        app.dispatch(Action::Quit),
        AppOutcome::FocusAudio(FocusAudioAction::Pause)
    );

    assert_eq!(app.dispatch(Action::ConfirmPendingAction), AppOutcome::Quit);
    assert!(!app.is_confirmation_open());
}

#[test]
fn cancelling_quit_restores_running_but_preserves_paused() {
    for initially_paused in [false, true] {
        let mut app = active_focus(Duration::from_secs(10), initially_paused);
        let _ = app.dispatch(Action::Quit);

        let expected_outcome = if initially_paused {
            AppOutcome::None
        } else {
            AppOutcome::FocusAudio(FocusAudioAction::StartOrResume)
        };
        assert_eq!(app.dispatch(Action::CancelPendingAction), expected_outcome);

        let expected = if initially_paused {
            TimerState::Paused(SessionKind::Focus)
        } else {
            TimerState::Running(SessionKind::Focus)
        };
        assert_eq!(app.timer().state(), expected);
        assert_eq!(app.timer().remaining(), Duration::from_secs(25 * 60 - 10));
    }
}

#[test]
fn tick_reports_each_session_completion_exactly_once() {
    for (session, duration) in [
        (SessionKind::Focus, Duration::from_secs(25 * 60)),
        (SessionKind::ShortBreak, Duration::from_secs(5 * 60)),
        (SessionKind::LongBreak, Duration::from_secs(15 * 60)),
    ] {
        let mut app = App::new();
        if session == SessionKind::Focus {
            let _ = app.dispatch(Action::PrimaryAction);
        } else {
            double_click_session(&mut app, session, Instant::now());
        }

        assert_eq!(app.tick(duration), AppOutcome::SessionCompleted(session));
        assert_eq!(app.tick(Duration::from_secs(1)), AppOutcome::None);
    }
}

fn autostart_app(breaks: bool, focus: bool) -> App {
    let timer = TimerConfig::default().with_autostart(breaks, focus);
    App::from_config(&Config::new(timer).unwrap())
}

#[test]
fn configured_break_autostart_counts_down_and_starts_the_recommendation() {
    let mut app = autostart_app(true, false);
    let _ = app.dispatch(Action::PrimaryAction);

    assert_eq!(
        app.tick(Duration::from_secs(25 * 60)),
        AppOutcome::SessionCompleted(SessionKind::Focus)
    );
    assert_eq!(app.pending_autostart(), Some((SessionKind::ShortBreak, 5)));
    assert_eq!(app.tick(Duration::from_millis(4_999)), AppOutcome::None);
    assert_eq!(app.pending_autostart(), Some((SessionKind::ShortBreak, 1)));
    assert_eq!(
        app.tick(Duration::from_millis(1)),
        AppOutcome::TimerEffects {
            focus_audio: None,
            stop_completion_audio: true,
        }
    );
    assert_eq!(
        app.timer().state(),
        TimerState::Running(SessionKind::ShortBreak)
    );
}

#[test]
fn oversized_tick_does_not_reduce_new_autostart_countdown() {
    let mut app = autostart_app(true, false);
    let _ = app.dispatch(Action::PrimaryAction);

    assert_eq!(
        app.tick(Duration::from_secs(25 * 60 + 30)),
        AppOutcome::SessionCompleted(SessionKind::Focus)
    );
    assert_eq!(app.pending_autostart(), Some((SessionKind::ShortBreak, 5)));
    assert_eq!(
        app.timer().state(),
        TimerState::Ready(SessionKind::ShortBreak)
    );
    assert_eq!(app.timer().remaining(), Duration::from_secs(5 * 60));
}

#[test]
fn oversized_autostart_tick_does_not_reduce_new_session() {
    let mut app = autostart_app(true, false);
    let _ = app.dispatch(Action::PrimaryAction);
    let _ = app.tick(Duration::from_secs(25 * 60));

    assert_eq!(
        app.tick(Duration::from_secs(30)),
        AppOutcome::TimerEffects {
            focus_audio: None,
            stop_completion_audio: true,
        }
    );
    assert_eq!(app.pending_autostart(), None);
    assert_eq!(
        app.timer().state(),
        TimerState::Running(SessionKind::ShortBreak)
    );
    assert_eq!(app.timer().remaining(), Duration::from_secs(5 * 60));
}

#[test]
fn focus_autostart_is_independent_from_break_autostart() {
    let mut app = autostart_app(false, true);
    double_click_session(&mut app, SessionKind::ShortBreak, Instant::now());

    assert_eq!(
        app.tick(Duration::from_secs(5 * 60)),
        AppOutcome::SessionCompleted(SessionKind::ShortBreak)
    );
    assert_eq!(app.pending_autostart(), Some((SessionKind::Focus, 5)));
    assert_eq!(
        app.tick(Duration::from_secs(5)),
        AppOutcome::TimerEffects {
            focus_audio: Some(FocusAudioAction::StartOrResume),
            stop_completion_audio: true,
        }
    );
    assert_eq!(app.timer().state(), TimerState::Running(SessionKind::Focus));
}

#[test]
fn primary_starts_pending_session_while_escape_and_cycle_cancel_it() {
    let mut immediate = autostart_app(true, false);
    let _ = immediate.dispatch(Action::PrimaryAction);
    let _ = immediate.tick(Duration::from_secs(25 * 60));
    assert_eq!(
        immediate.dispatch(Action::PrimaryAction),
        AppOutcome::TimerEffects {
            focus_audio: None,
            stop_completion_audio: true,
        }
    );
    assert_eq!(
        immediate.timer().state(),
        TimerState::Running(SessionKind::ShortBreak)
    );

    let mut cancelled = autostart_app(true, false);
    let _ = cancelled.dispatch(Action::PrimaryAction);
    let _ = cancelled.tick(Duration::from_secs(25 * 60));
    let _ = cancelled.dispatch(Action::CancelPendingAction);
    assert_eq!(cancelled.pending_autostart(), None);
    assert_eq!(
        cancelled.timer().state(),
        TimerState::Ready(SessionKind::ShortBreak)
    );

    let mut cycled = autostart_app(true, false);
    let _ = cycled.dispatch(Action::PrimaryAction);
    let _ = cycled.tick(Duration::from_secs(25 * 60));
    let _ = cycled.dispatch(Action::CycleSession);
    assert_eq!(cycled.pending_autostart(), None);
    assert_eq!(
        cycled.timer().state(),
        TimerState::Ready(SessionKind::LongBreak)
    );
}

#[test]
fn manual_start_stops_completion_audio_without_autostart() {
    let mut app = App::new();
    let _ = app.dispatch(Action::PrimaryAction);
    let _ = app.tick(Duration::from_secs(25 * 60));

    assert_eq!(
        app.dispatch(Action::PrimaryAction),
        AppOutcome::TimerEffects {
            focus_audio: None,
            stop_completion_audio: true,
        }
    );
}

#[test]
fn selecting_another_session_cancels_autostart_and_leaves_it_ready() {
    let mut app = autostart_app(true, false);
    let _ = app.dispatch(Action::PrimaryAction);
    let _ = app.tick(Duration::from_secs(25 * 60));

    assert_eq!(
        app.handle_click_target(
            ClickTarget::SessionControl(SessionKind::LongBreak),
            Instant::now()
        ),
        AppOutcome::TimerEffects {
            focus_audio: None,
            stop_completion_audio: true,
        }
    );
    assert_eq!(app.pending_autostart(), None);
    assert_eq!(
        app.timer().state(),
        TimerState::Ready(SessionKind::LongBreak)
    );
}

#[test]
fn double_clicking_clock_starts_pending_session_immediately() {
    let mut app = autostart_app(true, false);
    let _ = app.dispatch(Action::PrimaryAction);
    let _ = app.tick(Duration::from_secs(25 * 60));
    let now = Instant::now();

    assert_eq!(
        app.handle_click_target(ClickTarget::Clock, now),
        AppOutcome::None
    );
    assert_eq!(app.pending_autostart(), Some((SessionKind::ShortBreak, 5)));
    assert_eq!(
        app.handle_click_target(ClickTarget::Clock, now + Duration::from_millis(100)),
        AppOutcome::TimerEffects {
            focus_audio: None,
            stop_completion_audio: true,
        }
    );
    assert_eq!(app.pending_autostart(), None);
    assert_eq!(
        app.timer().state(),
        TimerState::Running(SessionKind::ShortBreak)
    );
}

#[test]
fn dispatches_editing_actions_without_physical_key_codes() {
    let mut app = App::new();
    let _ = app.dispatch(Action::NavigateFocus(Direction::Down));

    let _ = app.dispatch(Action::BeginAdd);
    let _ = app.dispatch(Action::PushInput('a'));
    let _ = app.dispatch(Action::PushInput('b'));
    let _ = app.dispatch(Action::PopInput);
    assert_eq!(app.dispatch(Action::SubmitEdit), AppOutcome::TasksChanged);

    assert_eq!(app.tasks().pending().next().unwrap().description(), "a");
    assert_eq!(app.edit_mode(), EditMode::Normal);
}

#[test]
fn begin_add_action_works_from_task_list_focus() {
    let mut app = App::new();
    let _ = app.dispatch(Action::BeginAdd);
    assert_eq!(app.edit_mode(), EditMode::Normal);

    let _ = app.dispatch(Action::NavigateFocus(Direction::Down));
    let _ = app.dispatch(Action::BeginAdd);
    assert_eq!(app.edit_mode(), EditMode::Adding);

    let _ = app.dispatch(Action::CancelEdit);
    let _ = app.dispatch(Action::NavigateFocus(Direction::Right));
    let _ = app.dispatch(Action::BeginAdd);
    assert_eq!(app.edit_mode(), EditMode::Adding);
}

#[test]
fn adding_from_done_focus_creates_a_completed_task() {
    let mut app = App::new();
    let _ = app.dispatch(Action::NavigateFocus(Direction::Down));
    let _ = app.dispatch(Action::NavigateFocus(Direction::Right));
    let _ = app.dispatch(Action::BeginAdd);
    for character in "New task".chars() {
        let _ = app.dispatch(Action::PushInput(character));
    }

    assert_eq!(app.dispatch(Action::SubmitEdit), AppOutcome::TasksChanged);
    assert_eq!(app.tasks().pending().count(), 0);
    assert_eq!(
        app.tasks().completed().next().unwrap().description(),
        "New task"
    );
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
fn blank_or_contextless_submissions_do_not_report_task_changes() {
    let mut app = App::new();

    assert_eq!(app.dispatch(Action::SubmitEdit), AppOutcome::None);
    let _ = app.dispatch(Action::NavigateFocus(Direction::Down));
    let _ = app.dispatch(Action::BeginAdd);
    let _ = app.dispatch(Action::PushInput(' '));

    assert_eq!(app.dispatch(Action::SubmitEdit), AppOutcome::None);
    assert_eq!(app.task_state(), TaskState::default());
}

#[test]
fn row_navigation_stays_within_tasks_and_handles_empty_lists() {
    let mut app = App::new();
    let _ = app.dispatch(Action::NavigateFocus(Direction::Down));
    let _ = app.dispatch(Action::MoveSelection(Direction::Down));
    assert_eq!(app.todo_selection(), 0);

    let _ = app.dispatch(Action::BeginAdd);
    let _ = app.dispatch(Action::PushInput('1'));
    assert_eq!(app.dispatch(Action::SubmitEdit), AppOutcome::TasksChanged);
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
fn moving_selected_tasks_reorders_each_list_and_keeps_the_item_selected() {
    let state = TaskState::from_lists(
        vec!["Todo first".to_string(), "Todo selected".to_string()],
        vec!["Done selected".to_string(), "Done second".to_string()],
    );
    let mut app = App::from_config_and_tasks(&Config::default(), state);
    let _ = app.dispatch(Action::NavigateFocus(Direction::Down));
    let _ = app.dispatch(Action::MoveSelection(Direction::Down));

    assert_eq!(
        app.dispatch(Action::MoveSelectedTask(Direction::Up)),
        AppOutcome::TasksChanged
    );
    assert_eq!(app.todo_selection(), 0);
    assert_eq!(
        app.tasks()
            .pending()
            .map(|task| task.description())
            .collect::<Vec<_>>(),
        ["Todo selected", "Todo first"]
    );
    assert_eq!(
        app.dispatch(Action::MoveSelectedTask(Direction::Up)),
        AppOutcome::None
    );
    assert_eq!(app.todo_selection(), 0);

    let _ = app.dispatch(Action::NavigateFocus(Direction::Right));
    assert_eq!(
        app.dispatch(Action::MoveSelectedTask(Direction::Down)),
        AppOutcome::TasksChanged
    );
    assert_eq!(app.done_selection(), 1);
    assert_eq!(
        app.tasks()
            .completed()
            .map(|task| task.description())
            .collect::<Vec<_>>(),
        ["Done second", "Done selected"]
    );
    assert_eq!(
        app.dispatch(Action::MoveSelectedTask(Direction::Down)),
        AppOutcome::None
    );
    assert_eq!(app.done_selection(), 1);
}

#[test]
fn scrolling_task_lists_focuses_the_target_and_moves_its_selection() {
    let mut app = App::new();
    add_task(&mut app, "First");
    add_task(&mut app, "Second");

    let _ = app.dispatch(Action::Scroll(ScrollTarget::Todo, Direction::Down));
    assert_eq!(app.ui_focus(), UiFocus::Todo);
    assert_eq!(app.todo_selection(), 1);

    let _ = app.dispatch(Action::Scroll(ScrollTarget::Todo, Direction::Up));
    assert_eq!(app.todo_selection(), 0);
    let _ = app.dispatch(Action::Scroll(ScrollTarget::Done, Direction::Down));
    assert_eq!(app.ui_focus(), UiFocus::Done);
    assert_eq!(app.done_selection(), 0);
}

#[test]
fn scrolling_settings_moves_selection_but_is_locked_during_editing() {
    let mut app = App::new();
    let _ = app.dispatch(Action::OpenSettings);

    let _ = app.dispatch(Action::Scroll(ScrollTarget::Settings, Direction::Down));
    assert_eq!(app.settings().unwrap().selection(), 1);
    let _ = app.dispatch(Action::Scroll(ScrollTarget::Settings, Direction::Up));
    assert_eq!(app.settings().unwrap().selection(), 0);

    let _ = app.dispatch(Action::SettingsActivate);
    let _ = app.dispatch(Action::Scroll(ScrollTarget::Settings, Direction::Down));
    assert_eq!(app.settings().unwrap().selection(), 0);
}

#[test]
fn editing_a_selected_todo_updates_that_list_entry() {
    let mut app = App::new();
    add_task(&mut app, "Done");
    let _ = app.dispatch(Action::NavigateFocus(Direction::Down));
    assert_eq!(
        app.dispatch(Action::PrimaryAction),
        AppOutcome::TasksChanged
    );
    let _ = app.dispatch(Action::NavigateFocus(Direction::Up));
    add_task(&mut app, "Edit me");
    let _ = app.dispatch(Action::NavigateFocus(Direction::Down));
    let _ = app.dispatch(Action::EditSelected);
    assert_eq!(app.edit_mode(), EditMode::Editing { task_index: 0 });

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
    assert_eq!(
        app.dispatch(Action::PrimaryAction),
        AppOutcome::TasksChanged
    );
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
    assert_eq!(
        app.dispatch(Action::DeleteSelected),
        AppOutcome::TasksChanged
    );
    assert_eq!(app.todo_selection(), 0);
    assert_eq!(app.tasks().pending().count(), 1);
}

#[test]
fn moving_tasks_appends_them_to_the_destination_view() {
    let state = TaskState::from_lists(
        vec!["Todo first".to_string(), "Todo second".to_string()],
        vec!["Done first".to_string()],
    );
    let mut app = App::from_config_and_tasks(&Config::default(), state);
    let _ = app.dispatch(Action::NavigateFocus(Direction::Down));

    assert_eq!(
        app.dispatch(Action::PrimaryAction),
        AppOutcome::TasksChanged
    );
    assert_eq!(
        app.tasks()
            .completed()
            .map(|task| task.description())
            .collect::<Vec<_>>(),
        ["Done first", "Todo first"]
    );

    let _ = app.dispatch(Action::NavigateFocus(Direction::Right));
    assert_eq!(
        app.dispatch(Action::PrimaryAction),
        AppOutcome::TasksChanged
    );
    assert_eq!(
        app.tasks()
            .pending()
            .map(|task| task.description())
            .collect::<Vec<_>>(),
        ["Todo second", "Done first"]
    );
}

#[test]
fn reset_below_ten_seconds_is_immediate() {
    let mut app = App::new();
    let _ = app.dispatch(Action::PrimaryAction);
    let _ = app.tick(Duration::from_secs(9));

    let _ = app.dispatch(Action::ResetSession);

    assert!(!app.is_confirmation_open());
    assert_eq!(app.timer().state(), TimerState::Ready(SessionKind::Focus));
    assert_eq!(app.timer().remaining(), Duration::from_secs(25 * 60));
}

#[test]
fn reset_at_ten_seconds_pauses_and_requests_confirmation() {
    let mut app = App::new();
    let _ = app.dispatch(Action::PrimaryAction);
    let _ = app.tick(Duration::from_secs(10));

    let _ = app.dispatch(Action::ResetSession);

    assert!(app.is_confirmation_open());
    assert_eq!(app.timer().state(), TimerState::Paused(SessionKind::Focus));
    assert_eq!(app.timer().remaining(), Duration::from_secs(25 * 60 - 10));
}

#[test]
fn confirming_reset_returns_the_same_session_to_ready() {
    let mut app = App::new();
    double_click_session(&mut app, SessionKind::LongBreak, Instant::now());
    let _ = app.tick(Duration::from_secs(10));
    let _ = app.dispatch(Action::ResetSession);

    let _ = app.dispatch(Action::ConfirmPendingAction);

    assert!(!app.is_confirmation_open());
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
    let _ = running.dispatch(Action::CancelPendingAction);
    assert_eq!(
        running.timer().state(),
        TimerState::Running(SessionKind::Focus)
    );

    let mut paused = App::new();
    let _ = paused.dispatch(Action::PrimaryAction);
    let _ = paused.tick(Duration::from_secs(10));
    let _ = paused.dispatch(Action::PrimaryAction);
    let _ = paused.dispatch(Action::ResetSession);
    let _ = paused.dispatch(Action::CancelPendingAction);
    assert_eq!(
        paused.timer().state(),
        TimerState::Paused(SessionKind::Focus)
    );
}

#[test]
fn confirmation_ignores_unrelated_actions_and_mouse() {
    let mut app = App::new();
    let _ = app.dispatch(Action::PrimaryAction);
    let _ = app.tick(Duration::from_secs(10));
    let _ = app.dispatch(Action::CycleSession);
    let focus = app.ui_focus();
    let remaining = app.timer().remaining();

    assert_eq!(app.dispatch(Action::Quit), AppOutcome::None);
    let _ = app.dispatch(Action::PrimaryAction);
    let _ = app.dispatch(Action::NavigateFocus(Direction::Down));
    let _ = app.handle_click_target(ClickTarget::Todo, Instant::now());

    assert!(app.is_confirmation_open());
    assert_eq!(app.ui_focus(), focus);
    assert_eq!(app.timer().remaining(), remaining);
    assert_eq!(app.timer().state(), TimerState::Paused(SessionKind::Focus));
}

#[test]
fn ready_session_cycles_without_starting() {
    let mut app = App::new();

    let _ = app.dispatch(Action::CycleSession);

    assert_eq!(
        app.timer().state(),
        TimerState::Ready(SessionKind::ShortBreak)
    );
    assert_eq!(app.timer().remaining(), Duration::from_secs(5 * 60));
}

#[test]
fn cycling_below_ten_seconds_immediately_discards_progress() {
    for pause_first in [false, true] {
        let mut app = App::new();
        let _ = app.dispatch(Action::PrimaryAction);
        let _ = app.tick(Duration::from_secs(9));
        if pause_first {
            let _ = app.dispatch(Action::PrimaryAction);
        }

        let _ = app.dispatch(Action::CycleSession);

        assert!(!app.is_confirmation_open());
        assert_eq!(
            app.timer().state(),
            TimerState::Ready(SessionKind::ShortBreak)
        );
        assert_eq!(app.timer().remaining(), Duration::from_secs(5 * 60));
    }
}

#[test]
fn cycling_at_ten_seconds_pauses_and_requests_confirmation() {
    for pause_first in [false, true] {
        let mut app = App::new();
        let _ = app.dispatch(Action::PrimaryAction);
        let _ = app.tick(Duration::from_secs(10));
        if pause_first {
            let _ = app.dispatch(Action::PrimaryAction);
        }

        let _ = app.dispatch(Action::CycleSession);

        assert!(app.is_confirmation_open());
        assert_eq!(
            app.pending_confirmation(),
            Some(ConfirmationOperation::TimerChange(TimerChange::Cycle))
        );
        assert_eq!(app.timer().state(), TimerState::Paused(SessionKind::Focus));
        assert_eq!(app.timer().remaining(), Duration::from_secs(25 * 60 - 10));
    }
}

#[test]
fn confirming_cycle_discards_progress_and_prepares_following_session() {
    let mut app = App::new();
    let _ = app.dispatch(Action::PrimaryAction);
    let _ = app.tick(Duration::from_secs(10));
    let _ = app.dispatch(Action::CycleSession);

    let _ = app.dispatch(Action::ConfirmPendingAction);

    assert!(!app.is_confirmation_open());
    assert_eq!(
        app.timer().state(),
        TimerState::Ready(SessionKind::ShortBreak)
    );
    assert_eq!(app.timer().remaining(), Duration::from_secs(5 * 60));
}

#[test]
fn cancelling_cycle_restores_running_but_preserves_paused() {
    let mut running = App::new();
    let _ = running.dispatch(Action::PrimaryAction);
    let _ = running.tick(Duration::from_secs(10));
    let _ = running.dispatch(Action::CycleSession);
    let _ = running.dispatch(Action::CancelPendingAction);
    assert_eq!(
        running.timer().state(),
        TimerState::Running(SessionKind::Focus)
    );
    assert_eq!(
        running.timer().remaining(),
        Duration::from_secs(25 * 60 - 10)
    );

    let mut paused = App::new();
    let _ = paused.dispatch(Action::PrimaryAction);
    let _ = paused.tick(Duration::from_secs(10));
    let _ = paused.dispatch(Action::PrimaryAction);
    let _ = paused.dispatch(Action::CycleSession);
    let _ = paused.dispatch(Action::CancelPendingAction);
    assert_eq!(
        paused.timer().state(),
        TimerState::Paused(SessionKind::Focus)
    );
    assert_eq!(
        paused.timer().remaining(),
        Duration::from_secs(25 * 60 - 10)
    );
}

#[test]
fn session_control_single_click_selects_and_double_click_starts() {
    let mut app = App::new();
    let now = Instant::now();
    let target = ClickTarget::SessionControl(SessionKind::LongBreak);

    let _ = app.handle_click_target(target, now);
    assert_eq!(app.ui_focus(), UiFocus::Clock);
    assert_eq!(
        app.timer().state(),
        TimerState::Ready(SessionKind::LongBreak)
    );

    let _ = app.handle_click_target(target, now + Duration::from_millis(100));
    assert_eq!(
        app.timer().state(),
        TimerState::Running(SessionKind::LongBreak)
    );
}

#[test]
fn different_or_too_slow_session_clicks_remain_ready() {
    let mut different = App::new();
    let now = Instant::now();
    let _ = different.handle_click_target(ClickTarget::SessionControl(SessionKind::LongBreak), now);
    let _ = different.handle_click_target(
        ClickTarget::SessionControl(SessionKind::ShortBreak),
        now + Duration::from_millis(100),
    );
    assert_eq!(
        different.timer().state(),
        TimerState::Ready(SessionKind::ShortBreak)
    );

    let mut slow = App::new();
    let target = ClickTarget::SessionControl(SessionKind::LongBreak);
    let _ = slow.handle_click_target(target, now);
    let _ = slow.handle_click_target(target, now + Duration::from_millis(501));
    assert_eq!(
        slow.timer().state(),
        TimerState::Ready(SessionKind::LongBreak)
    );
}

#[test]
fn double_clicking_active_session_control_pauses_or_resumes() {
    for (initially_paused, expected) in [
        (false, TimerState::Paused(SessionKind::Focus)),
        (true, TimerState::Running(SessionKind::Focus)),
    ] {
        let mut app = active_focus(Duration::from_secs(10), initially_paused);
        let now = Instant::now();
        let target = ClickTarget::SessionControl(SessionKind::Focus);

        let _ = app.handle_click_target(target, now);
        assert!(!app.is_confirmation_open());
        let _ = app.handle_click_target(target, now + Duration::from_millis(100));

        assert_eq!(app.timer().state(), expected);
        assert_eq!(app.timer().remaining(), Duration::from_secs(25 * 60 - 10));
    }
}

#[test]
fn single_clicking_different_session_below_threshold_selects_it_immediately() {
    for initially_paused in [false, true] {
        let mut app = active_focus(Duration::from_secs(9), initially_paused);

        let _ = app.handle_click_target(
            ClickTarget::SessionControl(SessionKind::ShortBreak),
            Instant::now(),
        );

        assert!(!app.is_confirmation_open());
        assert_eq!(
            app.timer().state(),
            TimerState::Ready(SessionKind::ShortBreak)
        );
        assert_eq!(app.timer().remaining(), Duration::from_secs(5 * 60));
    }
}

#[test]
fn single_clicking_different_session_at_threshold_confirms_selection() {
    for initially_paused in [false, true] {
        let mut app = active_focus(Duration::from_secs(10), initially_paused);
        let now = Instant::now();
        let target = ClickTarget::SessionControl(SessionKind::LongBreak);

        let _ = app.handle_click_target(target, now);

        assert_eq!(
            app.pending_confirmation(),
            Some(ConfirmationOperation::TimerChange(
                TimerChange::SelectSession(SessionKind::LongBreak)
            ))
        );
        let _ = app.dispatch(Action::ConfirmPendingAction);
        assert_eq!(
            app.timer().state(),
            TimerState::Ready(SessionKind::LongBreak)
        );
        assert_eq!(app.timer().remaining(), Duration::from_secs(15 * 60));

        let _ = app.handle_click_target(target, now + Duration::from_millis(100));
        assert_eq!(
            app.timer().state(),
            TimerState::Ready(SessionKind::LongBreak)
        );
    }
}

#[test]
fn double_clicking_different_session_below_threshold_starts_it_immediately() {
    for initially_paused in [false, true] {
        let mut app = active_focus(Duration::from_secs(9), initially_paused);
        let now = Instant::now();
        let target = ClickTarget::SessionControl(SessionKind::ShortBreak);

        let _ = app.handle_click_target(target, now);
        let _ = app.handle_click_target(target, now + Duration::from_millis(100));

        assert!(!app.is_confirmation_open());
        assert_eq!(
            app.timer().state(),
            TimerState::Running(SessionKind::ShortBreak)
        );
        assert_eq!(app.timer().remaining(), Duration::from_secs(5 * 60));
    }
}

#[test]
fn second_matching_click_upgrades_confirmed_change_to_change_and_start() {
    for initially_paused in [false, true] {
        let mut app = active_focus(Duration::from_secs(10), initially_paused);
        let now = Instant::now();
        let target = ClickTarget::SessionControl(SessionKind::ShortBreak);

        let _ = app.handle_click_target(target, now);
        assert_eq!(
            app.pending_confirmation(),
            Some(ConfirmationOperation::TimerChange(
                TimerChange::SelectSession(SessionKind::ShortBreak)
            ))
        );
        let _ = app.handle_click_target(target, now + Duration::from_millis(100));
        assert_eq!(
            app.pending_confirmation(),
            Some(ConfirmationOperation::TimerChange(
                TimerChange::StartSession(SessionKind::ShortBreak)
            ))
        );
        let _ = app.dispatch(Action::ConfirmPendingAction);

        assert_eq!(
            app.timer().state(),
            TimerState::Running(SessionKind::ShortBreak)
        );
        assert_eq!(app.timer().remaining(), Duration::from_secs(5 * 60));
    }
}

#[test]
fn cancelling_upgraded_session_change_restores_prior_activity() {
    for initially_paused in [false, true] {
        let mut app = active_focus(Duration::from_secs(10), initially_paused);
        let now = Instant::now();
        let target = ClickTarget::SessionControl(SessionKind::LongBreak);
        let _ = app.handle_click_target(target, now);
        let _ = app.handle_click_target(target, now + Duration::from_millis(100));

        let _ = app.dispatch(Action::CancelPendingAction);

        let expected = if initially_paused {
            TimerState::Paused(SessionKind::Focus)
        } else {
            TimerState::Running(SessionKind::Focus)
        };
        assert_eq!(app.timer().state(), expected);
        assert_eq!(app.timer().remaining(), Duration::from_secs(25 * 60 - 10));
    }
}

#[test]
fn mismatched_or_late_second_click_does_not_upgrade_pending_selection() {
    let now = Instant::now();
    for second_click in [
        (
            ClickTarget::SessionControl(SessionKind::LongBreak),
            Duration::from_millis(100),
        ),
        (
            ClickTarget::SessionControl(SessionKind::ShortBreak),
            Duration::from_millis(501),
        ),
    ] {
        let mut app = active_focus(Duration::from_secs(10), false);
        let _ = app.handle_click_target(ClickTarget::SessionControl(SessionKind::ShortBreak), now);
        let _ = app.handle_click_target(second_click.0, now + second_click.1);
        let _ = app.dispatch(Action::ConfirmPendingAction);

        assert_eq!(
            app.timer().state(),
            TimerState::Ready(SessionKind::ShortBreak)
        );
    }
}

#[test]
fn mouse_is_ignored_during_task_editing() {
    let mut app = App::new();
    let _ = app.dispatch(Action::NavigateFocus(Direction::Down));
    let _ = app.dispatch(Action::BeginAdd);

    let _ = app.handle_click_target(ClickTarget::Clock, Instant::now());

    assert_eq!(app.ui_focus(), UiFocus::Todo);
    assert_eq!(app.edit_mode(), EditMode::Adding);
}

#[test]
fn double_clicking_a_target_runs_its_contextual_action_once() {
    let mut app = App::new();
    let first = Instant::now();
    let _ = app.handle_click_target(ClickTarget::Clock, first);
    let _ = app.handle_click_target(ClickTarget::Clock, first + Duration::from_millis(200));
    assert_eq!(app.timer().state(), TimerState::Running(SessionKind::Focus));

    let _ = app.handle_click_target(ClickTarget::Clock, first + Duration::from_millis(300));
    assert_eq!(app.timer().state(), TimerState::Running(SessionKind::Focus));
}

#[test]
fn double_clicking_a_todo_task_completes_the_selected_task() {
    let mut app = App::new();
    add_task(&mut app, "First");
    add_task(&mut app, "Complete me");
    let first = Instant::now();

    let _ = app.handle_click_target(ClickTarget::TodoTask(1), first);
    assert_eq!(
        app.handle_click_target(ClickTarget::TodoTask(1), first + Duration::from_millis(200)),
        AppOutcome::TasksChanged
    );

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

    let _ = app.handle_click_target(ClickTarget::DoneTask(0), first);
    assert_eq!(
        app.handle_click_target(ClickTarget::DoneTask(0), first + Duration::from_millis(200)),
        AppOutcome::TasksChanged
    );

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
    let _ = app.handle_click_target(ClickTarget::Clock, first);
    let _ = app.handle_click_target(ClickTarget::Clock, first + Duration::from_millis(501));
    assert_eq!(app.timer().state(), TimerState::Ready(SessionKind::Focus));

    let _ = app.handle_click_target(ClickTarget::TodoTask(0), first + Duration::from_secs(1));
    let _ = app.handle_click_target(
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

    let _ = app.handle_click_target(ClickTarget::TodoTask(1), now);
    assert_eq!(app.ui_focus(), UiFocus::Todo);
    assert_eq!(app.todo_selection(), 1);

    let _ = app.handle_click_target(ClickTarget::Done, now);
    assert_eq!(app.ui_focus(), UiFocus::Done);
}

#[test]
fn non_actionable_clicks_break_double_click_sequences() {
    let mut app = App::new();
    let first = Instant::now();

    let _ = app.handle_click_target(ClickTarget::Clock, first);
    let _ = app.handle_click_target(ClickTarget::Outside, first + Duration::from_millis(100));
    let _ = app.handle_click_target(ClickTarget::Clock, first + Duration::from_millis(200));

    assert_eq!(app.timer().state(), TimerState::Ready(SessionKind::Focus));
}

#[test]
fn settings_preserve_running_and_paused_activity() {
    for initially_paused in [false, true] {
        let mut app = active_focus(Duration::from_secs(1), initially_paused);
        let expected_state = if initially_paused {
            TimerState::Paused(SessionKind::Focus)
        } else {
            TimerState::Running(SessionKind::Focus)
        };

        assert_eq!(app.dispatch(Action::OpenSettings), AppOutcome::None);
        assert_eq!(app.timer().state(), expected_state);
        assert!(app.is_settings_open());
        assert_eq!(app.tick(Duration::from_secs(1)), AppOutcome::None);
        assert_eq!(
            app.timer().progress(),
            Duration::from_secs(if initially_paused { 1 } else { 2 })
        );

        assert_eq!(app.dispatch(Action::SettingsClose), AppOutcome::None);
        assert_eq!(app.timer().state(), expected_state);
        assert!(!app.is_settings_open());
    }
}

#[test]
fn settings_cancel_leaves_nested_editing_and_close_closes_overlay() {
    let mut app = App::new();
    let _ = app.dispatch(Action::OpenSettings);
    let _ = app.dispatch(Action::SettingsActivate);

    assert_eq!(app.settings_mode(), SettingsMode::EditingValue);
    assert_eq!(app.dispatch(Action::SettingsCancel), AppOutcome::None);
    assert_eq!(app.settings_mode(), SettingsMode::Navigating);
    assert_eq!(app.dispatch(Action::SettingsClose), AppOutcome::None);
    assert_eq!(app.settings_mode(), SettingsMode::Closed);
}

#[test]
fn accepted_settings_binding_is_emitted_immediately_and_closes_overlay() {
    let mut app = App::new();
    let _ = app.dispatch(Action::OpenSettings);
    move_settings_to(&mut app, SettingField::Key(KeyAction::Settings));
    let _ = app.dispatch(Action::SettingsActivate);
    let outcome = app.dispatch(Action::SettingsCaptureKey(ConfigKey::Character('t')));

    assert_eq!(app.input_keys().settings(), [ConfigKey::Character('t')]);
    assert!(app.is_settings_open());
    let AppOutcome::SettingsChanged(config) = outcome else {
        panic!("accepted setting was not emitted")
    };
    assert_eq!(config.keys().settings(), [ConfigKey::Character('t')]);
    assert_eq!(app.dispatch(Action::SettingsClose), AppOutcome::None);
    assert!(!app.is_settings_open());
}

#[test]
fn non_timer_settings_apply_immediately_without_changing_activity() {
    let mut app = App::new();
    let _ = app.dispatch(Action::PrimaryAction);
    let _ = app.dispatch(Action::OpenSettings);
    move_settings_to(&mut app, SettingField::PersistTasks);
    let outcome = app.dispatch(Action::SettingsAdjust(SettingsAdjustmentDirection::Forward));
    let AppOutcome::SettingsChanged(config) = outcome else {
        panic!("settings were not emitted")
    };
    assert!(!config.tasks().persist());
    assert_eq!(app.timer().state(), TimerState::Running(SessionKind::Focus));
    assert_eq!(app.dispatch(Action::SettingsClose), AppOutcome::None);
    assert_eq!(app.timer().state(), TimerState::Running(SessionKind::Focus));
}

#[test]
fn active_timer_keeps_its_installed_duration_when_settings_change() {
    let mut app = App::new();
    let _ = app.dispatch(Action::PrimaryAction);
    let _ = app.tick(Duration::from_secs(10));
    let _ = app.dispatch(Action::OpenSettings);
    let _ = app.dispatch(Action::SettingsActivate);
    for _ in 0..5 {
        let _ = app.dispatch(Action::SettingsPopInput);
    }
    for character in "30:00".chars() {
        let _ = app.dispatch(Action::SettingsPushInput(character));
    }
    let outcome = app.dispatch(Action::SettingsSubmitInput);
    let AppOutcome::SettingsChanged(config) = outcome else {
        panic!("timer setting was not emitted")
    };
    assert_eq!(config.timer().focus_duration().as_secs(), 30 * 60);
    assert_eq!(app.timer().state(), TimerState::Running(SessionKind::Focus));
    assert_eq!(app.timer().progress(), Duration::from_secs(10));
    assert_eq!(app.timer().remaining(), Duration::from_secs(25 * 60 - 10));

    assert_eq!(app.dispatch(Action::SettingsClose), AppOutcome::None);
    assert_eq!(app.timer().state(), TimerState::Running(SessionKind::Focus));
    assert_eq!(app.timer().progress(), Duration::from_secs(10));
}

#[test]
fn ready_timer_adopts_duration_settings_immediately() {
    let mut app = App::new();
    let _ = app.dispatch(Action::OpenSettings);
    let _ = app.dispatch(Action::SettingsActivate);
    for _ in 0..5 {
        let _ = app.dispatch(Action::SettingsPopInput);
    }
    for character in "30:00".chars() {
        let _ = app.dispatch(Action::SettingsPushInput(character));
    }

    assert!(matches!(
        app.dispatch(Action::SettingsSubmitInput),
        AppOutcome::SettingsChanged(_)
    ));
    assert_eq!(app.timer().state(), TimerState::Ready(SessionKind::Focus));
    assert_eq!(app.timer().remaining(), Duration::from_secs(30 * 60));
}
