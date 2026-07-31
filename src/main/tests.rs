use std::{
    cell::Cell,
    ffi::OsString,
    fs,
    io::Cursor,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

use super::runtime::{RunError, commit_settings_change, handle_outcome, task_store_for_config};
use super::runtime::{advance_timer, combine_run_and_restore_results, should_handle_key_event};
use super::startup::{CliError, StartupError, load_config_path_for_startup};
use super::*;
use crossterm::event::KeyEventKind;
use pomock::{
    app::{Action, App, AppOutcome, Direction, FocusAudioAction, TaskState},
    config::{Config, TasksConfig, TimerConfig},
    notification::Notifier,
    persistence::TaskStore,
    sound::SoundPlayer,
};
use std::{
    io,
    time::{Duration, Instant},
};

static NEXT_TEMP_PATH: AtomicU64 = AtomicU64::new(0);

#[derive(Default)]
struct RecordingNotifier {
    completions: Vec<pomock::SessionKind>,
}

impl Notifier for RecordingNotifier {
    fn session_completed(&mut self, session: pomock::SessionKind) {
        self.completions.push(session);
    }
}

#[derive(Default)]
struct RecordingSoundPlayer {
    files: Vec<PathBuf>,
    focus_actions: Vec<&'static str>,
    focus_files: Vec<PathBuf>,
}

impl SoundPlayer for RecordingSoundPlayer {
    fn play_completion(&mut self, file: &std::path::Path) {
        self.files.push(file.to_owned());
    }

    fn stop_completion(&mut self) {
        self.focus_actions.push("stop_completion");
    }

    fn start_or_resume_focus(&mut self, file: &std::path::Path) {
        self.focus_actions.push("start");
        self.focus_files.push(file.to_owned());
    }

    fn pause_focus(&mut self) {
        self.focus_actions.push("pause");
    }

    fn stop_focus(&mut self) {
        self.focus_actions.push("stop");
    }
}

fn temp_path(name: &str) -> PathBuf {
    let unique = NEXT_TEMP_PATH.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "pomock-main-test-{}-{unique}-{name}",
        std::process::id()
    ))
}

fn task_state(description: &str) -> TaskState {
    let mut app = App::new();
    let _ = app.dispatch(Action::NavigateFocus(Direction::Down));
    let _ = app.dispatch(Action::BeginAdd);
    for character in description.chars() {
        let _ = app.dispatch(Action::PushInput(character));
    }
    let _ = app.dispatch(Action::SubmitEdit);
    app.task_state()
}

#[test]
fn key_releases_are_ignored_while_presses_and_repeats_are_handled() {
    assert!(should_handle_key_event(KeyEventKind::Press));
    assert!(should_handle_key_event(KeyEventKind::Repeat));
    assert!(!should_handle_key_event(KeyEventKind::Release));
}

#[test]
fn workspace_argument_accepts_separate_and_equals_forms() {
    assert_eq!(
        CliCommand::parse([OsString::from("--wspace"), OsString::from("client-one")]).unwrap(),
        CliCommand::Run {
            workspace: Some("client-one".to_owned())
        }
    );
    assert_eq!(
        CliCommand::parse([OsString::from("--wspace=personal.2026")]).unwrap(),
        CliCommand::Run {
            workspace: Some("personal.2026".to_owned())
        }
    );
    assert_eq!(
        CliCommand::parse(Vec::<OsString>::new()).unwrap(),
        CliCommand::Run { workspace: None }
    );
}

#[test]
fn workspace_argument_rejects_missing_unsafe_and_duplicate_names() {
    assert_eq!(
        CliCommand::parse([OsString::from("--wspace")]).unwrap_err(),
        CliError::MissingWorkspaceName
    );
    assert!(matches!(
        CliCommand::parse([OsString::from("--wspace=../shared")]).unwrap_err(),
        CliError::InvalidWorkspaceName(_)
    ));
    assert_eq!(
        CliCommand::parse([
            OsString::from("--wspace=one"),
            OsString::from("--wspace=two")
        ])
        .unwrap_err(),
        CliError::DuplicateWorkspace
    );
}

#[test]
fn shared_workspace_warning_requires_explicit_acceptance() {
    let mut accepted_output = Vec::new();
    assert!(
        confirm_shared_workspace(
            Some("client"),
            &mut Cursor::new(b"maybe\nyes\n"),
            &mut accepted_output,
        )
        .unwrap()
    );
    let accepted_output = String::from_utf8(accepted_output).unwrap();
    assert!(accepted_output.contains("workspace \"client\" is already open"));
    assert!(accepted_output.contains("Enter y to continue or n to quit."));

    assert!(!confirm_shared_workspace(None, &mut Cursor::new(b"\n"), &mut Vec::new(),).unwrap());
}

#[test]
fn invalid_config_can_be_deleted_and_replaced_with_defaults() {
    let path = temp_path("invalid-config.toml");
    fs::write(&path, "not valid toml =").unwrap();
    let mut input = Cursor::new(b"invalid\ndelete\n");
    let mut output = Vec::new();

    let config = load_config_path_for_startup(&path, &mut input, &mut output)
        .unwrap()
        .unwrap();

    assert_eq!(config, Config::default());
    assert!(!path.exists());
    let output = String::from_utf8(output).unwrap();
    assert!(output.contains("could not load the configuration file"));
    assert!(output.contains("Enter d to delete the file or q to quit."));
    assert!(output.contains("Deleted"));
}

#[test]
fn invalid_config_can_be_left_in_place_when_quitting() {
    let path = temp_path("invalid-config-quit.toml");
    let contents = "not valid toml =";
    fs::write(&path, contents).unwrap();
    let mut input = Cursor::new(b"q\n");
    let mut output = Vec::new();

    let config = load_config_path_for_startup(&path, &mut input, &mut output).unwrap();

    assert!(config.is_none());
    assert_eq!(fs::read_to_string(&path).unwrap(), contents);
    fs::remove_file(path).unwrap();
}

#[test]
fn invalid_task_file_can_be_deleted_and_started_empty() {
    let path = temp_path("invalid-tasks.toml");
    fs::write(&path, "version = 2\ntodo = []\ndone = []\n").unwrap();
    let store = TaskStore::at(&path);
    let mut input = Cursor::new(b"d\n");
    let mut output = Vec::new();

    let state = load_tasks_for_startup(Some(&store), &mut input, &mut output)
        .unwrap()
        .unwrap();

    assert_eq!(state, TaskState::default());
    assert!(!path.exists());
    assert!(
        String::from_utf8(output)
            .unwrap()
            .contains("could not load the task data file")
    );
}

#[test]
fn task_read_errors_do_not_offer_to_delete_the_path() {
    let path = temp_path("task-read-error");
    fs::create_dir(&path).unwrap();
    let store = TaskStore::at(&path);
    let mut input = Cursor::new(b"d\n");
    let mut output = Vec::new();

    let error = load_tasks_for_startup(Some(&store), &mut input, &mut output).unwrap_err();

    assert!(matches!(error, StartupError::TaskPersistence(_)));
    assert!(output.is_empty());
    assert!(path.is_dir());
    fs::remove_dir(path).unwrap();
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
    assert_eq!(
        app.dispatch(Action::PrimaryAction),
        AppOutcome::FocusAudio(FocusAudioAction::StartOrResume)
    );

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
fn task_change_outcomes_are_saved_at_the_boundary() {
    let path = temp_path("tasks.toml");
    let store = TaskStore::at(&path);
    let mut app = App::new();
    let _ = app.dispatch(Action::NavigateFocus(Direction::Down));
    let _ = app.dispatch(Action::BeginAdd);
    for character in "Persist me".chars() {
        let _ = app.dispatch(Action::PushInput(character));
    }
    let outcome = app.dispatch(Action::SubmitEdit);

    let mut config = Config::default();
    let mut task_store = Some(store.clone());
    let mut notifier = RecordingNotifier::default();
    let mut sound_player = RecordingSoundPlayer::default();

    assert!(
        !handle_outcome(
            outcome,
            &app,
            &mut config,
            &mut task_store,
            &store,
            &mut notifier,
            &mut sound_player,
        )
        .unwrap()
    );
    assert!(notifier.completions.is_empty());
    assert!(sound_player.files.is_empty());
    assert_eq!(
        task_store.as_ref().unwrap().load().unwrap(),
        app.task_state()
    );

    fs::remove_file(path).unwrap();
}

#[test]
fn disabled_task_persistence_starts_empty_and_does_not_save_changes() {
    let path = temp_path("disabled-tasks.toml");
    let store = TaskStore::at(&path);
    let config = Config::with_tasks(TimerConfig::default(), TasksConfig::new(false)).unwrap();
    let mut disabled_store = task_store_for_config(&config, &store);
    assert!(disabled_store.is_none());

    let mut persisted_app = App::new();
    let _ = persisted_app.dispatch(Action::NavigateFocus(Direction::Down));
    let _ = persisted_app.dispatch(Action::BeginAdd);
    for character in "Keep on disk".chars() {
        let _ = persisted_app.dispatch(Action::PushInput(character));
    }
    let _ = persisted_app.dispatch(Action::SubmitEdit);
    let persisted = persisted_app.task_state();
    store.save(&persisted).unwrap();

    assert_eq!(
        load_tasks_for_startup(
            disabled_store.as_ref(),
            &mut Cursor::new(Vec::new()),
            &mut Vec::new(),
        )
        .unwrap()
        .unwrap(),
        TaskState::default()
    );

    let mut app = App::new();
    let _ = app.dispatch(Action::NavigateFocus(Direction::Down));
    let _ = app.dispatch(Action::BeginAdd);
    let _ = app.dispatch(Action::PushInput('x'));
    let outcome = app.dispatch(Action::SubmitEdit);

    let mut config = config;
    let mut notifier = RecordingNotifier::default();
    let mut sound_player = RecordingSoundPlayer::default();
    assert!(
        !handle_outcome(
            outcome,
            &app,
            &mut config,
            &mut disabled_store,
            &store,
            &mut notifier,
            &mut sound_player,
        )
        .unwrap()
    );
    assert_eq!(store.load().unwrap(), persisted);

    fs::remove_file(path).unwrap();
}

#[test]
fn enabling_persistence_saves_tasks_before_config() {
    let path = temp_path("enable-persistence/tasks.toml");
    let next_store = TaskStore::at(&path);
    let state = task_state("Current task");
    let mut config = Config::with_tasks(TimerConfig::default(), TasksConfig::new(false)).unwrap();
    let updated = Config::default();
    let mut task_store = None;

    commit_settings_change(
        updated.clone(),
        &state,
        &mut config,
        &mut task_store,
        Some(next_store),
        |_| {
            assert_eq!(TaskStore::at(&path).load().unwrap(), state);
            Ok(())
        },
    )
    .unwrap();

    assert_eq!(config, updated);
    assert_eq!(task_store.unwrap().load().unwrap(), state);
    fs::remove_dir_all(path.parent().unwrap()).unwrap();
}

#[test]
fn failed_task_snapshot_does_not_commit_persistence_setting() {
    let parent = temp_path("enable-persistence-parent-is-file");
    fs::write(&parent, "not a directory").unwrap();
    let next_store = TaskStore::at(parent.join("tasks.toml"));
    let mut config = Config::with_tasks(TimerConfig::default(), TasksConfig::new(false)).unwrap();
    let original = config.clone();
    let mut task_store = None;
    let config_saved = Cell::new(false);

    let result = commit_settings_change(
        Config::default(),
        &task_state("Unsaved"),
        &mut config,
        &mut task_store,
        Some(next_store),
        |_| {
            config_saved.set(true);
            Ok(())
        },
    );

    assert!(matches!(result, Err(RunError::TaskPersistence(_))));
    assert!(!config_saved.get());
    assert_eq!(config, original);
    assert!(task_store.is_none());
    fs::remove_file(parent).unwrap();
}

#[test]
fn unrelated_settings_changes_do_not_rewrite_tasks() {
    let path = temp_path("unchanged-persistence/tasks.toml");
    let next_store = TaskStore::at(&path);
    let mut config = Config::default();
    let updated = config
        .clone()
        .with_notification(pomock::config::NotificationConfig::new(false));
    let mut task_store = Some(next_store.clone());

    commit_settings_change(
        updated.clone(),
        &task_state("In memory"),
        &mut config,
        &mut task_store,
        Some(next_store),
        |_| Ok(()),
    )
    .unwrap();

    assert_eq!(config, updated);
    assert!(!path.exists());
}

#[test]
fn completion_outcome_routes_notification_and_audio_effects() {
    let app = App::new();
    let sound_file = temp_path("custom-completion.mp3");
    let mut config = Config::default()
        .with_sound(pomock::config::SoundConfig::default().with_completion(
            pomock::config::CompletionSoundConfig::new(true, Some(sound_file.clone())),
        ))
        .unwrap();
    let mut task_store = None;
    let workspace_store = TaskStore::at(temp_path("completion-workspace/tasks.toml"));
    let mut notifier = RecordingNotifier::default();
    let mut sound_player = RecordingSoundPlayer::default();

    assert!(
        !handle_outcome(
            AppOutcome::SessionCompleted(pomock::SessionKind::Focus),
            &app,
            &mut config,
            &mut task_store,
            &workspace_store,
            &mut notifier,
            &mut sound_player,
        )
        .unwrap()
    );
    assert_eq!(notifier.completions, [pomock::SessionKind::Focus]);
    assert_eq!(sound_player.files, [sound_file]);
    assert_eq!(sound_player.focus_actions, ["stop"]);
}

#[test]
fn disabled_notifications_do_not_suppress_completion_audio() {
    let app = App::new();
    let sound_file = temp_path("completion.wav");
    let mut config = Config::default()
        .with_notification(pomock::config::NotificationConfig::new(false))
        .with_sound(pomock::config::SoundConfig::default().with_completion(
            pomock::config::CompletionSoundConfig::new(true, Some(sound_file.clone())),
        ))
        .unwrap();
    let mut task_store = None;
    let workspace_store = TaskStore::at(temp_path("notification-workspace/tasks.toml"));
    let mut notifier = RecordingNotifier::default();
    let mut sound_player = RecordingSoundPlayer::default();

    handle_outcome(
        AppOutcome::SessionCompleted(pomock::SessionKind::ShortBreak),
        &app,
        &mut config,
        &mut task_store,
        &workspace_store,
        &mut notifier,
        &mut sound_player,
    )
    .unwrap();

    assert!(notifier.completions.is_empty());
    assert_eq!(sound_player.files, [sound_file]);
    assert!(sound_player.focus_actions.is_empty());
}

#[test]
fn combined_timer_effect_stops_completion_before_starting_focus_audio() {
    let focus_file = temp_path("focus-loop.wav");
    let mut config = Config::default()
        .with_sound(pomock::config::SoundConfig::default().with_focus(
            pomock::config::FocusSoundConfig::new(true, Some(focus_file.clone())),
        ))
        .unwrap();
    let app = App::from_config(&config);
    let mut task_store = None;
    let workspace_store = TaskStore::at(temp_path("timer-effects-workspace/tasks.toml"));
    let mut notifier = RecordingNotifier::default();
    let mut sound = RecordingSoundPlayer::default();

    handle_outcome(
        AppOutcome::TimerEffects {
            focus_audio: Some(FocusAudioAction::StartOrResume),
            stop_completion_audio: true,
        },
        &app,
        &mut config,
        &mut task_store,
        &workspace_store,
        &mut notifier,
        &mut sound,
    )
    .unwrap();

    assert_eq!(sound.focus_actions, ["stop_completion", "start"]);
    assert_eq!(sound.focus_files, [focus_file]);
}

#[test]
fn focus_audio_outcomes_route_only_configured_starts_and_always_cleanup() {
    let app = App::new();
    let focus_file = temp_path("focus.ogg");
    let mut config = Config::default()
        .with_sound(pomock::config::SoundConfig::default().with_focus(
            pomock::config::FocusSoundConfig::new(true, Some(focus_file.clone())),
        ))
        .unwrap();
    let mut task_store = None;
    let workspace_store = TaskStore::at(temp_path("focus-audio-workspace/tasks.toml"));
    let mut notifier = RecordingNotifier::default();
    let mut sound_player = RecordingSoundPlayer::default();

    for outcome in [
        AppOutcome::FocusAudio(FocusAudioAction::StartOrResume),
        AppOutcome::FocusAudio(FocusAudioAction::Pause),
        AppOutcome::FocusAudio(FocusAudioAction::Stop),
    ] {
        handle_outcome(
            outcome,
            &app,
            &mut config,
            &mut task_store,
            &workspace_store,
            &mut notifier,
            &mut sound_player,
        )
        .unwrap();
    }

    assert_eq!(sound_player.focus_actions, ["start", "pause", "stop"]);
    assert_eq!(sound_player.focus_files, [focus_file]);

    let mut disabled_config = Config::default();
    handle_outcome(
        AppOutcome::FocusAudio(FocusAudioAction::StartOrResume),
        &app,
        &mut disabled_config,
        &mut task_store,
        &workspace_store,
        &mut notifier,
        &mut sound_player,
    )
    .unwrap();
    assert_eq!(sound_player.focus_actions, ["start", "pause", "stop"]);
}

#[test]
fn disabled_sound_options_keep_configured_files_silent() {
    let app = App::new();
    let mut config = Config::default()
        .with_sound(
            pomock::config::SoundConfig::default()
                .with_completion(pomock::config::CompletionSoundConfig::new(
                    false,
                    Some(temp_path("disabled-completion.wav")),
                ))
                .with_focus(pomock::config::FocusSoundConfig::new(
                    false,
                    Some(temp_path("disabled-focus.ogg")),
                )),
        )
        .unwrap();
    let mut task_store = None;
    let workspace_store = TaskStore::at(temp_path("disabled-sound-workspace/tasks.toml"));
    let mut notifier = RecordingNotifier::default();
    let mut sound_player = RecordingSoundPlayer::default();

    for outcome in [
        AppOutcome::FocusAudio(FocusAudioAction::StartOrResume),
        AppOutcome::SessionCompleted(pomock::SessionKind::ShortBreak),
    ] {
        handle_outcome(
            outcome,
            &app,
            &mut config,
            &mut task_store,
            &workspace_store,
            &mut notifier,
            &mut sound_player,
        )
        .unwrap();
    }

    assert!(sound_player.files.is_empty());
    assert!(sound_player.focus_actions.is_empty());
}

#[test]
fn run_error_is_preserved_when_restoration_succeeds() {
    let run_error = io::Error::new(io::ErrorKind::BrokenPipe, "run failed");

    let error =
        combine_run_and_restore_results(Err(RunError::Terminal(run_error)), Ok(())).unwrap_err();

    assert!(matches!(
        error,
        RunError::Terminal(ref error) if error.kind() == io::ErrorKind::BrokenPipe
    ));
    assert_eq!(error.to_string(), "run failed");
}

#[test]
fn restoration_error_is_reported_after_a_successful_run() {
    let restore_error = io::Error::other("restore failed");

    let error = combine_run_and_restore_results(Ok(()), Err(restore_error)).unwrap_err();

    assert!(matches!(error, RunError::Terminal(_)));
    assert_eq!(error.to_string(), "restore failed");
}

#[test]
fn simultaneous_run_and_restoration_errors_are_both_reported() {
    let run_error = io::Error::new(io::ErrorKind::BrokenPipe, "run failed");
    let restore_error = io::Error::other("restore failed");

    let error =
        combine_run_and_restore_results(Err(RunError::Terminal(run_error)), Err(restore_error))
            .unwrap_err();

    assert!(matches!(error, RunError::TerminalRestoration { .. }));
    assert_eq!(
        error.to_string(),
        "run failed; terminal restoration also failed: restore failed"
    );
}
