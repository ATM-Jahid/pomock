use super::{FileWriteError, apply_settings_change, task_store_for_config};
use crate::{
    runtime::{RuntimeContext, Workspace},
    startup::load_tasks_for_startup,
    test_support::{backup_paths, only_backup, task_state, temp_path},
};
use pomock::{
    app::{Action, App, AppOutcome, Direction, FocusAudioAction, TaskState},
    config::{Config, TasksConfig, TimerConfig},
    notification::Notifier,
    persistence::{ConfigStore, TaskStore},
    sound::SoundPlayer,
};
use std::{fs, io::Cursor, path::PathBuf};

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

    let config = Config::default();
    let task_store = Some(store.clone());
    let notifier = RecordingNotifier::default();
    let sound_player = RecordingSoundPlayer::default();

    let mut runtime = RuntimeContext {
        config,
        task_store,
        workspace: Workspace {
            task_store: store.clone(),
            config_store: ConfigStore::at(temp_path("effects-config.toml")),
        },
        notifier,
        sound_player,
    };

    assert!(!runtime.handle_outcome(outcome, &mut app).unwrap());
    assert!(runtime.notifier.completions.is_empty());
    assert!(runtime.sound_player.files.is_empty());
    assert_eq!(
        runtime.task_store.as_ref().unwrap().load().unwrap(),
        app.task_state()
    );

    fs::remove_file(path).unwrap();
}

#[test]
fn failed_task_save_reports_an_error_and_allows_a_later_save() {
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
    let config = Config::default();
    let task_store = Some(store.clone());
    let notifier = RecordingNotifier::default();
    let sound_player = RecordingSoundPlayer::default();

    let mut runtime = RuntimeContext {
        config,
        task_store,
        workspace: Workspace {
            task_store: store.clone(),
            config_store: ConfigStore::at(temp_path("effects-config.toml")),
        },
        notifier,
        sound_player,
    };

    assert!(!runtime.handle_outcome(outcome, &mut app).unwrap());
    assert!(app.task_write_error().is_some());
    assert!(!app.is_confirmation_open());
    fs::remove_file(&parent).unwrap();
    fs::create_dir(&parent).unwrap();
    let _ = app.dispatch(Action::BeginAdd);
    for character in "Save me too".chars() {
        let _ = app.dispatch(Action::PushInput(character));
    }
    let next_change = app.dispatch(Action::SubmitEdit);
    assert!(!runtime.handle_outcome(next_change, &mut app).unwrap());
    assert!(app.task_write_error().is_some());
    assert_eq!(store.load().unwrap(), app.task_state());
    fs::remove_dir_all(parent).unwrap();
}

#[test]
fn disabled_task_persistence_starts_empty_and_does_not_save_changes() {
    let path = temp_path("disabled-tasks.toml");
    let store = TaskStore::at(&path);
    let config = Config::with_tasks(TimerConfig::default(), TasksConfig::new(false)).unwrap();
    let disabled_store = task_store_for_config(&config, &store);
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

    let notifier = RecordingNotifier::default();
    let sound_player = RecordingSoundPlayer::default();

    let mut runtime = RuntimeContext {
        config,
        task_store: disabled_store,
        workspace: Workspace {
            task_store: store.clone(),
            config_store: ConfigStore::at(temp_path("effects-config.toml")),
        },
        notifier,
        sound_player,
    };
    assert!(!runtime.handle_outcome(outcome, &mut app).unwrap());
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

    apply_settings_change(
        updated.clone(),
        &state,
        &mut config,
        &mut task_store,
        Some(next_store),
        |_| {
            assert_eq!(TaskStore::at(&path).load().unwrap(), state);
            Ok(())
        },
    );

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

    apply_settings_change(
        updated.clone(),
        &state,
        &mut config,
        &mut task_store,
        Some(next_store),
        |_| Ok(()),
    );

    assert_eq!(config, updated);
    assert_eq!(task_store.unwrap().load().unwrap(), state);
    let backup = only_backup(&path);
    assert_eq!(fs::read(&backup).unwrap(), original);
    fs::remove_dir_all(path.parent().unwrap()).unwrap();
}

#[test]
fn enabling_persistence_does_not_back_up_unchanged_task_file() {
    let path = temp_path("enable-persistence-unchanged/tasks.toml");
    let next_store = TaskStore::at(&path);
    let state = task_state("Current task");
    next_store.save(&state).unwrap();
    let mut config = Config::with_tasks(TimerConfig::default(), TasksConfig::new(false)).unwrap();
    let updated = Config::default();
    let mut task_store = None;

    apply_settings_change(
        updated.clone(),
        &state,
        &mut config,
        &mut task_store,
        Some(next_store),
        |_| Ok(()),
    );

    assert_eq!(config, updated);
    assert!(backup_paths(&path).is_empty());
    assert_eq!(task_store.unwrap().load().unwrap(), state);
    fs::remove_dir_all(path.parent().unwrap()).unwrap();
}

#[test]
fn settings_write_failures_are_reported_and_settings_remain_active() {
    let parent = temp_path("enable-persistence-parent-is-file");
    fs::write(&parent, "not a directory").unwrap();
    let next_store = TaskStore::at(parent.join("tasks.toml"));
    let mut config = Config::with_tasks(TimerConfig::default(), TasksConfig::new(false)).unwrap();
    let mut task_store = None;
    let updated = Config::default();

    let errors = apply_settings_change(
        updated.clone(),
        &task_state("Unsaved"),
        &mut config,
        &mut task_store,
        Some(next_store),
        |_| Err(pomock::persistence::ConfigError::DirectoryUnavailable),
    );

    assert!(matches!(
        errors.as_slice(),
        [FileWriteError::Tasks(_), FileWriteError::Config(_)]
    ));
    assert_eq!(config, updated);
    assert!(task_store.is_some());
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

    apply_settings_change(
        updated.clone(),
        &task_state("In memory"),
        &mut config,
        &mut task_store,
        Some(next_store),
        |_| Ok(()),
    );

    assert_eq!(config, updated);
    assert!(!path.exists());
}

#[test]
fn completion_outcome_routes_notification_and_audio_effects() {
    let mut app = App::new();
    let sound_file = temp_path("custom-completion.mp3");
    let config = Config::default()
        .with_sound(pomock::config::SoundConfig::default().with_completion(
            pomock::config::CompletionSoundConfig::new(true, Some(sound_file.clone())),
        ))
        .unwrap();
    let task_store = None;
    let workspace = Workspace {
        task_store: TaskStore::at(temp_path("completion-workspace/tasks.toml")),
        config_store: ConfigStore::at(temp_path("effects-config.toml")),
    };
    let notifier = RecordingNotifier::default();
    let sound_player = RecordingSoundPlayer::default();

    let mut runtime = RuntimeContext {
        config,
        task_store,
        workspace,
        notifier,
        sound_player,
    };

    assert!(
        !runtime
            .handle_outcome(
                AppOutcome::SessionCompleted(pomock::SessionKind::Focus),
                &mut app
            )
            .unwrap()
    );
    assert_eq!(runtime.notifier.completions, [pomock::SessionKind::Focus]);
    assert_eq!(runtime.sound_player.files, [sound_file]);
    assert_eq!(runtime.sound_player.focus_actions, ["stop"]);
}

#[test]
fn disabled_notifications_do_not_suppress_completion_audio() {
    let mut app = App::new();
    let sound_file = temp_path("completion.wav");
    let config = Config::default()
        .with_notification(pomock::config::NotificationConfig::new(false))
        .with_sound(pomock::config::SoundConfig::default().with_completion(
            pomock::config::CompletionSoundConfig::new(true, Some(sound_file.clone())),
        ))
        .unwrap();
    let task_store = None;
    let workspace = Workspace {
        task_store: TaskStore::at(temp_path("notification-workspace/tasks.toml")),
        config_store: ConfigStore::at(temp_path("effects-config.toml")),
    };
    let notifier = RecordingNotifier::default();
    let sound_player = RecordingSoundPlayer::default();

    let mut runtime = RuntimeContext {
        config,
        task_store,
        workspace,
        notifier,
        sound_player,
    };

    runtime
        .handle_outcome(
            AppOutcome::SessionCompleted(pomock::SessionKind::ShortBreak),
            &mut app,
        )
        .unwrap();

    assert!(runtime.notifier.completions.is_empty());
    assert_eq!(runtime.sound_player.files, [sound_file]);
    assert!(runtime.sound_player.focus_actions.is_empty());
}

#[test]
fn combined_timer_effect_stops_completion_before_starting_focus_audio() {
    let focus_file = temp_path("focus-loop.wav");
    let config = Config::default()
        .with_sound(pomock::config::SoundConfig::default().with_focus(
            pomock::config::FocusSoundConfig::new(true, Some(focus_file.clone())),
        ))
        .unwrap();
    let mut app = App::from_config(&config);
    let task_store = None;
    let workspace = Workspace {
        task_store: TaskStore::at(temp_path("timer-effects-workspace/tasks.toml")),
        config_store: ConfigStore::at(temp_path("effects-config.toml")),
    };
    let notifier = RecordingNotifier::default();
    let sound = RecordingSoundPlayer::default();

    let mut runtime = RuntimeContext {
        config,
        task_store,
        workspace,
        notifier,
        sound_player: sound,
    };

    runtime
        .handle_outcome(
            AppOutcome::TimerEffects {
                focus_audio: Some(FocusAudioAction::StartOrResume),
                stop_completion_audio: true,
            },
            &mut app,
        )
        .unwrap();

    assert_eq!(
        runtime.sound_player.focus_actions,
        ["stop_completion", "start"]
    );
    assert_eq!(runtime.sound_player.focus_files, [focus_file]);
}

#[test]
fn focus_audio_outcomes_route_only_configured_starts_and_always_cleanup() {
    let mut app = App::new();
    let focus_file = temp_path("focus.ogg");
    let config = Config::default()
        .with_sound(pomock::config::SoundConfig::default().with_focus(
            pomock::config::FocusSoundConfig::new(true, Some(focus_file.clone())),
        ))
        .unwrap();
    let task_store = None;
    let workspace = Workspace {
        task_store: TaskStore::at(temp_path("focus-audio-workspace/tasks.toml")),
        config_store: ConfigStore::at(temp_path("effects-config.toml")),
    };
    let notifier = RecordingNotifier::default();
    let sound_player = RecordingSoundPlayer::default();

    let mut runtime = RuntimeContext {
        config,
        task_store,
        workspace,
        notifier,
        sound_player,
    };

    for outcome in [
        AppOutcome::FocusAudio(FocusAudioAction::StartOrResume),
        AppOutcome::FocusAudio(FocusAudioAction::Pause),
        AppOutcome::FocusAudio(FocusAudioAction::Stop),
    ] {
        runtime.handle_outcome(outcome, &mut app).unwrap();
    }

    assert_eq!(
        runtime.sound_player.focus_actions,
        ["start", "pause", "stop"]
    );
    assert_eq!(runtime.sound_player.focus_files, [focus_file]);

    runtime.config = Config::default();
    runtime
        .handle_outcome(
            AppOutcome::FocusAudio(FocusAudioAction::StartOrResume),
            &mut app,
        )
        .unwrap();
    assert_eq!(
        runtime.sound_player.focus_actions,
        ["start", "pause", "stop"]
    );
}

#[test]
fn disabled_sound_options_keep_configured_files_silent() {
    let mut app = App::new();
    let config = Config::default()
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
    let task_store = None;
    let workspace = Workspace {
        task_store: TaskStore::at(temp_path("disabled-sound-workspace/tasks.toml")),
        config_store: ConfigStore::at(temp_path("effects-config.toml")),
    };
    let notifier = RecordingNotifier::default();
    let sound_player = RecordingSoundPlayer::default();

    let mut runtime = RuntimeContext {
        config,
        task_store,
        workspace,
        notifier,
        sound_player,
    };

    for outcome in [
        AppOutcome::FocusAudio(FocusAudioAction::StartOrResume),
        AppOutcome::SessionCompleted(pomock::SessionKind::ShortBreak),
    ] {
        runtime.handle_outcome(outcome, &mut app).unwrap();
    }

    assert!(runtime.sound_player.files.is_empty());
    assert!(runtime.sound_player.focus_actions.is_empty());
}

#[test]
fn settings_outcome_saves_to_the_selected_workspace_even_without_task_persistence() {
    let directory = tempfile::tempdir().unwrap();
    let main_config_store = ConfigStore::at(directory.path().join("config.toml"));
    main_config_store.save(&Config::default()).unwrap();
    let workspace = Workspace {
        task_store: TaskStore::at(directory.path().join("client/tasks.toml")),
        config_store: ConfigStore::at(directory.path().join("client/config.toml")),
    };
    workspace
        .config_store
        .create_workspace_file(&main_config_store)
        .unwrap();
    let config = Config::default();
    let updated = Config::with_tasks(TimerConfig::default(), TasksConfig::new(false)).unwrap();
    let mut app = App::from_config(&config);
    let task_store = Some(workspace.task_store.clone());

    let mut runtime = RuntimeContext {
        config,
        task_store,
        workspace,
        notifier: RecordingNotifier::default(),
        sound_player: RecordingSoundPlayer::default(),
    };
    runtime
        .handle_outcome(
            AppOutcome::SettingsChanged(Box::new(updated.clone())),
            &mut app,
        )
        .unwrap();
    assert_eq!(runtime.workspace.config_store.load().unwrap(), updated);
    assert_eq!(main_config_store.load().unwrap(), Config::default());
    assert!(runtime.task_store.is_none());
}
