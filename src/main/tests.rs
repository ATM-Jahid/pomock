use std::{
    cell::Cell,
    ffi::OsString,
    fs,
    io::{BufRead, Cursor, Read},
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

fn backup_paths(path: &std::path::Path) -> Vec<PathBuf> {
    let prefix = format!("{}.backup-", path.file_name().unwrap().to_string_lossy());
    fs::read_dir(path.parent().unwrap())
        .unwrap()
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|candidate| {
            candidate
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with(&prefix)
        })
        .collect()
}

fn only_backup(path: &std::path::Path) -> PathBuf {
    let backups = backup_paths(path);
    assert_eq!(backups.len(), 1);
    backups.into_iter().next().unwrap()
}

fn write_config_with_unknown_field(path: &std::path::Path) -> String {
    Config::default().save_to(path).unwrap();
    let mut stored: toml::Value = toml::from_str(&fs::read_to_string(path).unwrap()).unwrap();
    stored
        .as_table_mut()
        .unwrap()
        .insert("obsolete".to_owned(), true.into());
    let contents = toml::to_string_pretty(&stored).unwrap();
    fs::write(path, &contents).unwrap();
    contents
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

enum FileMutation {
    Write(Vec<u8>),
    Delete,
}

struct MutatingReader {
    input: Cursor<Vec<u8>>,
    path: PathBuf,
    mutation: Option<FileMutation>,
}

impl MutatingReader {
    fn writing(path: &std::path::Path, contents: impl Into<Vec<u8>>, input: &[u8]) -> Self {
        Self {
            input: Cursor::new(input.to_vec()),
            path: path.to_owned(),
            mutation: Some(FileMutation::Write(contents.into())),
        }
    }

    fn deleting(path: &std::path::Path, input: &[u8]) -> Self {
        Self {
            input: Cursor::new(input.to_vec()),
            path: path.to_owned(),
            mutation: Some(FileMutation::Delete),
        }
    }

    fn apply_mutation(&mut self) -> io::Result<()> {
        match self.mutation.take() {
            Some(FileMutation::Write(contents)) => fs::write(&self.path, contents),
            Some(FileMutation::Delete) => fs::remove_file(&self.path),
            None => Ok(()),
        }
    }
}

impl Read for MutatingReader {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        self.apply_mutation()?;
        self.input.read(buffer)
    }
}

impl BufRead for MutatingReader {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        self.apply_mutation()?;
        self.input.fill_buf()
    }

    fn consume(&mut self, amount: usize) {
        self.input.consume(amount);
    }
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
fn startup_creates_missing_config_and_task_files() {
    let config_path = temp_path("missing-startup/config.toml");
    let tasks_path = temp_path("missing-startup/tasks.toml");
    let task_store = TaskStore::at(&tasks_path);

    let config = load_config_path_for_startup(
        &config_path,
        &mut Cursor::new(Vec::<u8>::new()),
        &mut Vec::new(),
    )
    .unwrap()
    .unwrap();
    let tasks = load_tasks_for_startup(
        Some(&task_store),
        &mut Cursor::new(Vec::<u8>::new()),
        &mut Vec::new(),
    )
    .unwrap()
    .unwrap();

    assert_eq!(config, Config::default());
    assert_eq!(Config::load_from(&config_path).unwrap(), Config::default());
    assert_eq!(tasks, TaskState::default());
    assert_eq!(task_store.load().unwrap(), TaskState::default());
    fs::remove_dir_all(config_path.parent().unwrap()).unwrap();
    fs::remove_dir_all(tasks_path.parent().unwrap()).unwrap();
}

#[test]
fn valid_config_edit_during_confirmation_is_preserved_and_used() {
    let path = temp_path("config-edited-during-prompt.toml");
    fs::write(&path, "not valid toml =").unwrap();
    let valid_path = temp_path("replacement-config.toml");
    Config::default().save_to(&valid_path).unwrap();
    let valid = fs::read(&valid_path).unwrap();
    fs::remove_file(valid_path).unwrap();
    let mut reader = MutatingReader::writing(&path, valid.clone(), b"b\n");

    let config = load_config_path_for_startup(&path, &mut reader, &mut Vec::new())
        .unwrap()
        .unwrap();

    assert_eq!(config, Config::default());
    assert_eq!(fs::read(&path).unwrap(), valid);
    assert!(backup_paths(&path).is_empty());
    fs::remove_file(path).unwrap();
}

#[test]
fn changed_invalid_config_is_prompted_again_before_replacement() {
    let path = temp_path("config-invalid-edit-during-prompt.toml");
    fs::write(&path, "first invalid =").unwrap();
    let replacement = b"second invalid =".to_vec();
    let mut reader = MutatingReader::writing(&path, replacement.clone(), b"b\nb\n");
    let mut output = Vec::new();

    let config = load_config_path_for_startup(&path, &mut reader, &mut output)
        .unwrap()
        .unwrap();

    assert_eq!(config, Config::default());
    assert_eq!(fs::read(only_backup(&path)).unwrap(), replacement);
    assert_eq!(
        String::from_utf8(output)
            .unwrap()
            .matches("pomock could not load the configuration file")
            .count(),
        2
    );
    for backup in backup_paths(&path) {
        fs::remove_file(backup).unwrap();
    }
    fs::remove_file(path).unwrap();
}

#[test]
fn config_deleted_during_confirmation_is_recreated_without_backup() {
    let path = temp_path("config-deleted-during-prompt.toml");
    fs::write(&path, "not valid toml =").unwrap();
    let mut reader = MutatingReader::deleting(&path, b"b\n");

    let config = load_config_path_for_startup(&path, &mut reader, &mut Vec::new())
        .unwrap()
        .unwrap();

    assert_eq!(config, Config::default());
    assert_eq!(Config::load_from(&path).unwrap(), Config::default());
    assert!(backup_paths(&path).is_empty());
    fs::remove_file(path).unwrap();
}

#[test]
fn valid_task_edit_during_confirmation_is_preserved_and_used() {
    let path = temp_path("tasks-edited-during-prompt.toml");
    fs::write(&path, "not valid toml =").unwrap();
    let valid = b"version = 1\ntodo = [\"edited\"]\ndone = []\n".to_vec();
    let store = TaskStore::at(&path);
    let mut reader = MutatingReader::writing(&path, valid.clone(), b"b\n");

    let state = load_tasks_for_startup(Some(&store), &mut reader, &mut Vec::new())
        .unwrap()
        .unwrap();

    assert_eq!(state, task_state("edited"));
    assert_eq!(fs::read(&path).unwrap(), valid);
    assert!(backup_paths(&path).is_empty());
    fs::remove_file(path).unwrap();
}

#[test]
fn unknown_config_key_can_be_left_in_place_when_quitting() {
    let path = temp_path("unknown-key-declined.toml");
    let original = write_config_with_unknown_field(&path);
    let mut output = Vec::new();

    let config =
        load_config_path_for_startup(&path, &mut Cursor::new(b"q\n"), &mut output).unwrap();

    assert!(config.is_none());
    assert_eq!(fs::read_to_string(&path).unwrap(), original);
    assert!(backup_paths(&path).is_empty());
    assert!(
        String::from_utf8(output)
            .unwrap()
            .contains("Back up the invalid file, create a new config, and continue")
    );
    fs::remove_file(path).unwrap();
}

#[test]
fn unknown_config_key_can_be_backed_up_and_replaced_with_defaults() {
    let path = temp_path("unknown-key-approved.toml");
    let original = write_config_with_unknown_field(&path);
    let mut output = Vec::new();

    let config = load_config_path_for_startup(&path, &mut Cursor::new(b"b\n"), &mut output)
        .unwrap()
        .unwrap();

    assert_eq!(config, Config::default());
    assert!(!fs::read_to_string(&path).unwrap().contains("obsolete"));
    let backup = only_backup(&path);
    assert_eq!(fs::read_to_string(&backup).unwrap(), original);
    assert!(
        String::from_utf8(output)
            .unwrap()
            .contains("Backed up the invalid file")
    );
    fs::remove_file(backup).unwrap();
    fs::remove_file(path).unwrap();
}

#[test]
fn invalid_config_can_be_backed_up_and_atomically_replaced_with_defaults() {
    let path = temp_path("invalid-config.toml");
    let original = "not valid toml =";
    fs::write(&path, original).unwrap();
    let mut input = Cursor::new(b"invalid\nbackup\n");
    let mut output = Vec::new();

    let config = load_config_path_for_startup(&path, &mut input, &mut output)
        .unwrap()
        .unwrap();

    assert_eq!(config, Config::default());
    assert_eq!(Config::load_from(&path).unwrap(), Config::default());
    let backup = only_backup(&path);
    assert_eq!(fs::read_to_string(&backup).unwrap(), original);
    let output = String::from_utf8(output).unwrap();
    assert!(output.contains("could not load the configuration file"));
    assert!(output.contains("Enter b to back up and create a new config, or q to quit."));
    assert!(output.contains("Back up the invalid file, create a new config, and continue"));
    assert!(output.contains(backup.to_str().unwrap()));
    fs::remove_file(backup).unwrap();
    fs::remove_file(path).unwrap();
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
    assert!(backup_paths(&path).is_empty());
    fs::remove_file(path).unwrap();
}

#[test]
fn invalid_task_file_can_be_backed_up_and_replaced_with_empty_state() {
    let path = temp_path("invalid-tasks.toml");
    let original = "version = 2\ntodo = []\ndone = []\n";
    fs::write(&path, original).unwrap();
    let store = TaskStore::at(&path);
    let mut input = Cursor::new(b"b\n");
    let mut output = Vec::new();

    let state = load_tasks_for_startup(Some(&store), &mut input, &mut output)
        .unwrap()
        .unwrap();

    assert_eq!(state, TaskState::default());
    assert_eq!(store.load().unwrap(), TaskState::default());
    let backup = only_backup(&path);
    assert_eq!(fs::read_to_string(&backup).unwrap(), original);
    let output = String::from_utf8(output).unwrap();
    assert!(output.contains("could not load the task data file"));
    assert!(output.contains("Back up the invalid file, create a new task file, and continue"));
    fs::remove_file(backup).unwrap();
    fs::remove_file(path).unwrap();
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
            &mut app,
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
fn failed_task_save_removes_the_change_and_shows_a_message() {
    let parent = temp_path("failed-task-save-parent");
    fs::write(&parent, "not a directory").unwrap();
    let store = TaskStore::at(parent.join("tasks.toml"));
    let mut app = App::new();
    let _ = app.dispatch(Action::NavigateFocus(Direction::Down));
    let _ = app.dispatch(Action::BeginAdd);
    for character in "Keep me".chars() {
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
            &mut app,
            &mut config,
            &mut task_store,
            &store,
            &mut notifier,
            &mut sound_player,
        )
        .unwrap()
    );
    assert!(app.show_task_save_failure());
    assert!(!app.is_confirmation_open());
    assert_eq!(app.task_state(), TaskState::default());

    fs::remove_file(&parent).unwrap();
    fs::create_dir(&parent).unwrap();
    let _ = app.dispatch(Action::BeginAdd);
    for character in "Save me too".chars() {
        let _ = app.dispatch(Action::PushInput(character));
    }
    let next_change = app.dispatch(Action::SubmitEdit);
    assert!(
        !handle_outcome(
            next_change,
            &mut app,
            &mut config,
            &mut task_store,
            &store,
            &mut notifier,
            &mut sound_player,
        )
        .unwrap()
    );
    assert!(!app.show_task_save_failure());
    assert_eq!(store.load().unwrap(), app.task_state());
    fs::remove_dir_all(parent).unwrap();
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
            &mut app,
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
fn enabling_persistence_backs_up_and_replaces_an_existing_task_file() {
    let path = temp_path("enable-persistence-existing/tasks.toml");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let original = b"invalid dormant task data";
    fs::write(&path, original).unwrap();
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
        |_| Ok(()),
    )
    .unwrap();

    assert_eq!(config, updated);
    assert_eq!(task_store.unwrap().load().unwrap(), state);
    let backup = only_backup(&path);
    assert_eq!(fs::read(&backup).unwrap(), original);
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
    let mut app = App::new();
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
            &mut app,
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
    let mut app = App::new();
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
        &mut app,
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
    let mut app = App::from_config(&config);
    let mut task_store = None;
    let workspace_store = TaskStore::at(temp_path("timer-effects-workspace/tasks.toml"));
    let mut notifier = RecordingNotifier::default();
    let mut sound = RecordingSoundPlayer::default();

    handle_outcome(
        AppOutcome::TimerEffects {
            focus_audio: Some(FocusAudioAction::StartOrResume),
            stop_completion_audio: true,
        },
        &mut app,
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
    let mut app = App::new();
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
            &mut app,
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
        &mut app,
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
    let mut app = App::new();
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
            &mut app,
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
