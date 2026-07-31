use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use super::{
    CompletionSoundConfig, Config, ConfigError, ConfigKey, ConfigValidationError, FocusSoundConfig,
    KeyBindings, KeysConfig, NotificationConfig, SoundConfig, TasksConfig, ThemeColor, ThemeConfig,
    TimerConfig,
};

static NEXT_TEMP_PATH: AtomicU64 = AtomicU64::new(0);

fn temp_path(name: &str) -> PathBuf {
    let unique = NEXT_TEMP_PATH.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "pomock-config-test-{}-{unique}-{name}",
        std::process::id()
    ))
}

fn backup_paths(path: &Path) -> Vec<PathBuf> {
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

fn write_current_config(path: &PathBuf, overrides: impl AsRef<str>) -> std::io::Result<()> {
    Config::default().save_to(path).unwrap();
    let mut current: toml::Value = toml::from_str(&fs::read_to_string(path).unwrap()).unwrap();
    let overrides: toml::Value = toml::from_str(overrides.as_ref()).unwrap();
    merge_toml(&mut current, overrides);
    fs::write(path, toml::to_string_pretty(&current).unwrap())
}

fn merge_toml(current: &mut toml::Value, overrides: toml::Value) {
    match (current, overrides) {
        (toml::Value::Table(current), toml::Value::Table(overrides)) => {
            for (key, value) in overrides {
                match current.get_mut(&key) {
                    Some(current) => merge_toml(current, value),
                    None => {
                        current.insert(key, value);
                    }
                }
            }
        }
        (current, overrides) => *current = overrides,
    }
}

fn remove_toml_field(value: &mut toml::Value, path: &str) {
    let (parents, field) = path.rsplit_once('.').unwrap_or(("", path));
    let mut table = value.as_table_mut().unwrap();
    for parent in parents.split('.').filter(|parent| !parent.is_empty()) {
        table = table.get_mut(parent).unwrap().as_table_mut().unwrap();
    }
    assert!(
        table.remove(field).is_some(),
        "missing fixture field {path}"
    );
}

#[test]
fn defaults_match_the_product_contract() {
    let config = Config::default();

    assert_eq!(config.timer().focus_duration().as_secs(), 25 * 60);
    assert_eq!(config.timer().short_break_duration().as_secs(), 5 * 60);
    assert_eq!(config.timer().long_break_duration().as_secs(), 15 * 60);
    assert_eq!(config.timer().long_break_interval().get(), 4);
    assert!(!config.timer().autostart_breaks());
    assert!(!config.timer().autostart_focus());
    assert!(config.tasks().persist());
    assert!(config.tasks().show_numbers());
    assert_eq!(config.theme(), &ThemeConfig::default());
    assert_eq!(config.keys(), &KeysConfig::default());
    assert!(config.notification().enabled());
    assert_eq!(config.sound(), &SoundConfig::default());
    assert!(!config.sound().completion().enabled());
    assert!(config.sound().completion().file().is_none());
    assert!(!config.sound().focus().enabled());
    assert!(config.sound().focus().file().is_none());
    assert_eq!(
        config.keys().list_down(),
        [ConfigKey::Character('j'), ConfigKey::Down]
    );
    assert_eq!(
        config.keys().list_up(),
        [ConfigKey::Character('k'), ConfigKey::Up]
    );
    assert_eq!(config.keys().settings(), [ConfigKey::Character('s')]);
    assert_eq!(config.keys().move_task_up(), [ConfigKey::Character('u')]);
    assert_eq!(config.keys().move_task_down(), [ConfigKey::Character('d')]);
}

#[test]
fn missing_file_uses_defaults() {
    let path = temp_path("missing.toml");

    assert_eq!(Config::load_from(path).unwrap(), Config::default());
}

#[test]
fn missing_config_fields_are_filled_from_defaults_without_rewriting() {
    const REQUIRED_FIELDS: &[&str] = &[
        "timer",
        "timer.focus_duration",
        "timer.short_break_duration",
        "timer.long_break_duration",
        "timer.long_break_interval",
        "timer.autostart_breaks",
        "timer.autostart_focus",
        "notification",
        "notification.enabled",
        "sound",
        "sound.completion",
        "sound.completion.enabled",
        "sound.focus",
        "sound.focus.enabled",
        "tasks",
        "tasks.persist",
        "tasks.show_numbers",
        "keys",
        "keys.quit",
        "keys.settings",
        "keys.focus_left",
        "keys.focus_down",
        "keys.focus_up",
        "keys.focus_right",
        "keys.clock_primary",
        "keys.cycle_session",
        "keys.reset_session",
        "keys.add_task",
        "keys.edit_task",
        "keys.delete_task",
        "keys.task_primary",
        "keys.list_down",
        "keys.list_up",
        "keys.move_task_up",
        "keys.move_task_down",
        "theme",
        "theme.focused_border",
        "theme.unfocused_border",
        "theme.focus",
        "theme.short_break",
        "theme.long_break",
        "theme.todo_highlight",
        "theme.done_highlight",
    ];

    for field in REQUIRED_FIELDS {
        let path = temp_path(&format!("missing-{}.toml", field.replace('.', "-")));
        Config::default().save_to(&path).unwrap();
        let mut stored: toml::Value = toml::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        remove_toml_field(&mut stored, field);
        fs::write(&path, toml::to_string_pretty(&stored).unwrap()).unwrap();

        let original = fs::read_to_string(&path).unwrap();

        assert_eq!(
            Config::load_from(&path).unwrap(),
            Config::default(),
            "missing field {field} did not use its default"
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), original);
        assert!(backup_paths(&path).is_empty());
        fs::remove_file(path).unwrap();
    }
}

#[test]
fn unknown_config_fields_are_invalid_and_left_unchanged() {
    let path = temp_path("unknown-field.toml");
    Config::default().save_to(&path).unwrap();
    let mut stored: toml::Value = toml::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    stored
        .as_table_mut()
        .unwrap()
        .insert("obsolete".to_owned(), "remove me".into());
    stored["timer"]
        .as_table_mut()
        .unwrap()
        .insert("legacy_mode".to_owned(), true.into());
    let original = toml::to_string_pretty(&stored).unwrap();
    fs::write(&path, &original).unwrap();

    assert!(matches!(
        Config::load_from(&path),
        Err(ConfigError::Parse { .. })
    ));
    assert_eq!(fs::read_to_string(&path).unwrap(), original);
    assert!(backup_paths(&path).is_empty());
    fs::remove_file(path).unwrap();
}

#[test]
fn saved_default_config_follows_the_documented_settings_order() {
    let path = temp_path("ordered/config.toml");
    Config::default().save_to(&path).unwrap();

    let contents = fs::read_to_string(&path).unwrap();
    assert_eq!(
        contents,
        concat!(
            "[timer]\n",
            "focus_duration = \"25:00\"\n",
            "short_break_duration = \"05:00\"\n",
            "long_break_duration = \"15:00\"\n",
            "long_break_interval = 4\n",
            "autostart_breaks = false\n",
            "autostart_focus = false\n",
            "\n",
            "[notification]\n",
            "enabled = true\n",
            "\n",
            "[sound.completion]\n",
            "enabled = false\n",
            "\n",
            "[sound.focus]\n",
            "enabled = false\n",
            "\n",
            "[tasks]\n",
            "persist = true\n",
            "show_numbers = true\n",
            "\n",
            "[keys]\n",
            "quit = \"q\"\n",
            "settings = \"s\"\n",
            "focus_left = \"H\"\n",
            "focus_down = \"J\"\n",
            "focus_up = \"K\"\n",
            "focus_right = \"L\"\n",
            "clock_primary = \"space\"\n",
            "cycle_session = \"c\"\n",
            "reset_session = \"r\"\n",
            "add_task = \"a\"\n",
            "edit_task = \"e\"\n",
            "delete_task = \"x\"\n",
            "task_primary = \"space\"\n",
            "list_down = [\n",
            "    \"j\",\n",
            "    \"down\",\n",
            "]\n",
            "list_up = [\n",
            "    \"k\",\n",
            "    \"up\",\n",
            "]\n",
            "move_task_up = \"u\"\n",
            "move_task_down = \"d\"\n",
            "\n",
            "[theme]\n",
            "focused_border = \"light_red\"\n",
            "unfocused_border = \"dark_gray\"\n",
            "focus = \"magenta\"\n",
            "short_break = \"cyan\"\n",
            "long_break = \"green\"\n",
            "todo_highlight = \"red\"\n",
            "done_highlight = \"green\"\n",
        )
    );
    fs::remove_dir_all(path.parent().unwrap()).unwrap();
}

#[test]
fn notification_and_sound_files_round_trip() {
    let path = temp_path("sound/config.toml");
    let sound_file = PathBuf::from("~/sounds/session-complete.mp3");
    let focus_file = PathBuf::from("~/sounds/focus.ogg");
    let config = Config::default()
        .with_notification(NotificationConfig::new(false))
        .with_sound(
            SoundConfig::default()
                .with_completion(CompletionSoundConfig::new(true, Some(sound_file.clone())))
                .with_focus(FocusSoundConfig::new(true, Some(focus_file.clone()))),
        )
        .unwrap();

    config.save_to(&path).unwrap();

    assert_eq!(Config::load_from(&path).unwrap(), config);
    assert_eq!(
        config.sound().completion().playback_file(),
        Some(
            directories::UserDirs::new()
                .unwrap()
                .home_dir()
                .join("sounds/session-complete.mp3")
                .as_path()
        )
    );
    assert_eq!(
        config.sound().focus().playback_file(),
        Some(
            directories::UserDirs::new()
                .unwrap()
                .home_dir()
                .join("sounds/focus.ogg")
                .as_path()
        )
    );
    let contents = fs::read_to_string(&path).unwrap();
    assert!(contents.contains("[notification]"));
    assert!(contents.contains("enabled = false"));
    assert!(contents.contains("[sound.completion]"));
    assert!(contents.contains("enabled = true"));
    assert!(contents.contains("file = \"~/sounds/session-complete.mp3\""));
    assert!(contents.contains("[sound.focus]"));
    assert!(contents.contains("file = \"~/sounds/focus.ogg\""));

    fs::remove_dir_all(path.parent().unwrap()).unwrap();
}

#[test]
fn absolute_sound_paths_pass_through_unchanged() {
    let sound_file = temp_path("absolute-sound.wav");
    let config = Config::default()
        .with_sound(
            SoundConfig::default()
                .with_completion(CompletionSoundConfig::new(true, Some(sound_file.clone()))),
        )
        .unwrap();

    assert_eq!(
        config.sound().completion().file(),
        Some(sound_file.as_path())
    );
    assert_eq!(
        config.sound().completion().playback_file(),
        Some(sound_file.as_path())
    );
}

#[test]
fn other_relative_sound_paths_are_rejected_with_the_config_path() {
    let path = temp_path("relative-sound.toml");
    write_current_config(
        &path,
        "[timer]\nfocus_duration = \"25:00\"\nshort_break_duration = \"05:00\"\nlong_break_duration = \"15:00\"\nlong_break_interval = 4\n\n[sound.completion]\nenabled = true\nfile = \"sounds/done.wav\"\n",
    )
    .unwrap();

    let error = Config::load_from(&path).unwrap_err();

    assert!(matches!(error, ConfigError::Validation { .. }));
    assert!(error.to_string().contains(path.to_str().unwrap()));
    assert!(error.to_string().contains("absolute path or start with ~/"));
    assert!(error.to_string().contains("sounds/done.wav"));
    fs::remove_file(path).unwrap();
}

#[test]
fn relative_focus_sound_paths_report_the_precise_field_and_config_path() {
    let path = temp_path("relative-focus-sound.toml");
    write_current_config(
        &path,
        "[timer]\nfocus_duration = \"25:00\"\nshort_break_duration = \"05:00\"\nlong_break_duration = \"15:00\"\nlong_break_interval = 4\n\n[sound.focus]\nenabled = true\nfile = \"sounds/focus.wav\"\n",
    )
    .unwrap();

    let error = Config::load_from(&path).unwrap_err();

    assert!(error.to_string().contains(path.to_str().unwrap()));
    assert!(error.to_string().contains("sound.focus.file"));
    assert!(error.to_string().contains("sounds/focus.wav"));
    fs::remove_file(path).unwrap();
}

#[test]
fn saves_and_loads_a_valid_toml_round_trip() {
    let path = temp_path("round-trip/config.toml");
    let config = Config::with_settings(
        TimerConfig::from_seconds(50 * 60 + 30, 10 * 60 + 15, 30 * 60 + 45, 3)
            .unwrap()
            .with_autostart(true, false),
        TasksConfig::with_numbering(false, false),
        ThemeConfig::new(
            ThemeColor::LightBlue,
            ThemeColor::Black,
            ThemeColor::LightYellow,
            ThemeColor::LightGreen,
        ),
        KeysConfig {
            clock_primary: KeyBindings::one(ConfigKey::Enter.with_modifiers(true, false, false)),
            cycle_session: KeyBindings::one(ConfigKey::Character('n')),
            ..KeysConfig::default()
        },
    )
    .unwrap();

    config.save_to(&path).unwrap();
    assert_eq!(Config::load_from(&path).unwrap(), config);

    let contents = fs::read_to_string(&path).unwrap();
    assert!(contents.contains("[timer]"));
    assert!(contents.contains("focus_duration = \"50:30\""));
    assert!(contents.contains("autostart_breaks = true"));
    assert!(contents.contains("autostart_focus = false"));
    assert!(contents.contains("[tasks]"));
    assert!(contents.contains("persist = false"));
    assert!(contents.contains("show_numbers = false"));
    assert!(contents.contains("[theme]"));
    assert!(contents.contains("focused_border = \"light_blue\""));
    assert!(!contents.contains("completed_sessions"));
    assert!(contents.contains("[keys]"));
    assert!(contents.contains("clock_primary = \"ctrl+enter\""));
    assert!(contents.contains("cycle_session = \"n\""));
    fs::remove_dir_all(path.parent().unwrap()).unwrap();
}

#[test]
fn shifted_non_character_keys_round_trip_canonically() {
    let keys = KeysConfig::from_test_toml(
        "clock_primary = \"alt+shift+enter\"\nfocus_left = \"shift+ctrl+left\"\n",
    );

    let serialized = toml::to_string(&keys).unwrap();
    assert!(serialized.contains("clock_primary = \"alt+shift+enter\""));
    assert!(serialized.contains("focus_left = \"ctrl+shift+left\""));
    assert_eq!(toml::from_str::<KeysConfig>(&serialized).unwrap(), keys);
}

#[test]
fn keys_accept_ordered_lists_and_single_values() {
    let path = temp_path("multiple-keys.toml");
    write_current_config(
        &path,
        "[timer]\nfocus_duration = \"25:00\"\nshort_break_duration = \"05:00\"\nlong_break_duration = \"15:00\"\nlong_break_interval = 4\n\n[keys]\nlist_down = [\"j\", \"down\"]\nlist_up = [\"k\", \"up\"]\nquit = \"q\"\n",
    )
    .unwrap();

    let config = Config::load_from(&path).unwrap();

    assert_eq!(
        config.keys().list_down(),
        [ConfigKey::Character('j'), ConfigKey::Down]
    );
    assert_eq!(
        config.keys().list_up(),
        [ConfigKey::Character('k'), ConfigKey::Up]
    );
    assert_eq!(config.keys().quit(), [ConfigKey::Character('q')]);

    config.save_to(&path).unwrap();
    let contents = fs::read_to_string(&path).unwrap();
    assert!(contents.contains("list_down = [\n    \"j\",\n    \"down\",\n]"));
    assert!(contents.contains("quit = \"q\""));
    fs::remove_file(path).unwrap();
}

#[test]
fn empty_key_lists_are_rejected_with_the_field_path() {
    let path = temp_path("empty-keys.toml");
    write_current_config(
        &path,
        "[timer]\nfocus_duration = \"25:00\"\nshort_break_duration = \"05:00\"\nlong_break_duration = \"15:00\"\nlong_break_interval = 4\n\n[keys]\nquit = []\n",
    )
    .unwrap();

    let error = Config::load_from(&path).unwrap_err();

    assert!(matches!(error, ConfigError::Validation { .. }));
    assert!(error.to_string().contains("keys.quit"));
    assert!(error.to_string().contains("at least one key"));
    fs::remove_file(path).unwrap();
}

#[test]
fn conflicts_are_detected_in_secondary_keys() {
    let path = temp_path("conflicting-secondary-key.toml");
    write_current_config(
        &path,
        "[timer]\nfocus_duration = \"25:00\"\nshort_break_duration = \"05:00\"\nlong_break_duration = \"15:00\"\nlong_break_interval = 4\n\n[keys]\ncycle_session = [\"c\", \"q\"]\n",
    )
    .unwrap();

    let error = Config::load_from(&path).unwrap_err();

    assert!(matches!(error, ConfigError::Validation { .. }));
    assert!(error.to_string().contains("keys.quit"));
    assert!(error.to_string().contains("keys.cycle_session"));
    fs::remove_file(path).unwrap();
}

#[test]
fn plain_enter_and_escape_are_reserved_for_modal_controls() {
    for key in ["enter", "esc"] {
        let path = temp_path(&format!("reserved-{key}.toml"));
        write_current_config(
            &path,
            format!(
                "[timer]\nfocus_duration = \"25:00\"\nshort_break_duration = \"05:00\"\nlong_break_duration = \"15:00\"\nlong_break_interval = 4\n\n[keys]\ncycle_session = \"{key}\"\n"
            ),
        )
        .unwrap();

        let error = Config::load_from(&path).unwrap_err();

        assert!(matches!(error, ConfigError::Validation { .. }));
        assert!(error.to_string().contains(path.to_str().unwrap()));
        assert!(error.to_string().contains("keys.cycle_session"));
        assert!(error.to_string().contains(&format!("reserved key {key}")));
        fs::remove_file(path).unwrap();
    }
}

#[test]
fn settings_rejects_fixed_overlay_controls() {
    let path = temp_path("settings-overlay-key.toml");
    write_current_config(
        &path,
        "[timer]\nfocus_duration = \"25:00\"\nshort_break_duration = \"05:00\"\nlong_break_duration = \"15:00\"\nlong_break_interval = 4\n\n[keys]\nsettings = \"enter\"\n",
    )
    .unwrap();

    let error = Config::load_from(&path).unwrap_err();

    assert!(matches!(error, ConfigError::Validation { .. }));
    assert!(error.to_string().contains(path.to_str().unwrap()));
    assert!(error.to_string().contains("keys.settings"));
    assert!(error.to_string().contains("reserved key enter"));
    fs::remove_file(path).unwrap();
}

#[test]
fn lowercase_s_can_be_reused_after_settings_is_rebound() {
    let keys = KeysConfig {
        settings: KeyBindings::one(ConfigKey::Character('t')),
        cycle_session: KeyBindings::one(ConfigKey::Character('s')),
        ..KeysConfig::default()
    };

    let config = Config::with_settings(
        TimerConfig::default(),
        TasksConfig::default(),
        ThemeConfig::default(),
        keys,
    )
    .unwrap();

    assert_eq!(config.keys().settings(), [ConfigKey::Character('t')]);
    assert_eq!(config.keys().cycle_session(), [ConfigKey::Character('s')]);
}

#[test]
fn settings_is_validated_as_a_global_action() {
    let keys = KeysConfig {
        settings: KeyBindings::one(ConfigKey::Character('q')),
        ..KeysConfig::default()
    };

    let error = Config::with_settings(
        TimerConfig::default(),
        TasksConfig::default(),
        ThemeConfig::default(),
        keys,
    )
    .unwrap_err();

    assert!(matches!(
        error,
        ConfigValidationError::ConflictingKeys { .. }
    ));
    assert!(error.to_string().contains("keys.quit"));
    assert!(error.to_string().contains("keys.settings"));
}

#[test]
fn invalid_key_name_reports_its_path_and_supported_forms() {
    let path = temp_path("invalid-key.toml");
    write_current_config(
        &path,
        "[timer]\nfocus_duration = \"25:00\"\nshort_break_duration = \"05:00\"\nlong_break_duration = \"15:00\"\nlong_break_interval = 4\n\n[keys]\ncycle_session = \"page_down\"\n",
    )
    .unwrap();

    let error = Config::load_from(&path).unwrap_err();

    assert!(matches!(error, ConfigError::Parse { .. }));
    assert!(error.to_string().contains(path.to_str().unwrap()));
    assert!(error.to_string().contains("page_down"));
    assert!(error.to_string().contains("one printable character"));
    assert!(!error.to_string().contains("enter"));
    assert!(!error.to_string().contains("esc"));
    fs::remove_file(path).unwrap();
}

#[test]
fn conflicting_contextual_keys_report_both_fields_and_path() {
    let path = temp_path("conflicting-keys.toml");
    write_current_config(
        &path,
        "[timer]\nfocus_duration = \"25:00\"\nshort_break_duration = \"05:00\"\nlong_break_duration = \"15:00\"\nlong_break_interval = 4\n\n[keys]\ncycle_session = \"q\"\n",
    )
    .unwrap();

    let error = Config::load_from(&path).unwrap_err();

    assert!(matches!(error, ConfigError::Validation { .. }));
    assert!(error.to_string().contains(path.to_str().unwrap()));
    assert!(error.to_string().contains("keys.quit"));
    assert!(error.to_string().contains("keys.cycle_session"));
    fs::remove_file(path).unwrap();
}

#[test]
fn task_movement_keys_share_task_context_validation() {
    let path = temp_path("conflicting-task-movement-key.toml");
    write_current_config(
        &path,
        "[timer]\nfocus_duration = \"25:00\"\nshort_break_duration = \"05:00\"\nlong_break_duration = \"15:00\"\nlong_break_interval = 4\n\n[keys]\nmove_task_up = \"a\"\nmove_task_down = \"d\"\n",
    )
    .unwrap();

    let error = Config::load_from(&path).unwrap_err();

    assert!(matches!(error, ConfigError::Validation { .. }));
    assert!(error.to_string().contains("keys.add_task"));
    assert!(error.to_string().contains("keys.move_task_up"));
    fs::remove_file(path).unwrap();
}

#[test]
fn same_key_is_allowed_for_commands_in_disjoint_contexts() {
    let keys = KeysConfig {
        cycle_session: KeyBindings::one(ConfigKey::Character('a')),
        ..KeysConfig::default()
    };

    assert!(
        Config::with_settings(
            TimerConfig::default(),
            TasksConfig::default(),
            ThemeConfig::default(),
            keys,
        )
        .is_ok()
    );
}

#[test]
fn hex_theme_colors_load_and_save_canonically() {
    let path = temp_path("hex-theme.toml");
    write_current_config(
        &path,
        "[timer]\nfocus_duration = \"25:00\"\nshort_break_duration = \"05:00\"\nlong_break_duration = \"15:00\"\nlong_break_interval = 4\n\n[theme]\nfocused_border = \"#5FD7fF\"\n",
    )
    .unwrap();

    let config = Config::load_from(&path).unwrap();
    assert_eq!(
        config.theme().focused_border(),
        ThemeColor::Rgb(0x5f, 0xd7, 0xff)
    );

    config.save_to(&path).unwrap();
    assert!(
        fs::read_to_string(&path)
            .unwrap()
            .contains("focused_border = \"#5fd7ff\"")
    );
    fs::remove_file(path).unwrap();
}

#[test]
fn unsupported_theme_color_reports_its_path_and_allowed_values() {
    let path = temp_path("invalid-theme.toml");
    write_current_config(
        &path,
        "[timer]\nfocus_duration = \"25:00\"\nshort_break_duration = \"05:00\"\nlong_break_duration = \"15:00\"\nlong_break_interval = 4\n\n[theme]\nfocused_border = \"orange\"\nunfocused_border = \"dark_gray\"\ntodo_highlight = \"yellow\"\ndone_highlight = \"green\"\n",
    )
    .unwrap();

    let error = Config::load_from(&path).unwrap_err();

    assert!(matches!(error, ConfigError::Parse { .. }));
    assert!(error.to_string().contains(path.to_str().unwrap()));
    assert!(error.to_string().contains("orange"));
    fs::remove_file(path).unwrap();
}

#[test]
fn malformed_toml_reports_its_path_and_parse_error() {
    let path = temp_path("malformed.toml");
    fs::write(&path, "[timer\nfocus_duration = \"25:00\"").unwrap();

    let error = Config::load_from(&path).unwrap_err();

    assert!(matches!(error, ConfigError::Parse { .. }));
    assert!(error.to_string().contains(path.to_str().unwrap()));
    fs::remove_file(path).unwrap();
}

#[test]
fn zero_duration_is_rejected_with_the_field_name() {
    let error = TimerConfig::new(0, 5, 15, 4).unwrap_err();

    assert_eq!(
        error,
        ConfigValidationError::ZeroDuration {
            field: "focus_duration"
        }
    );
}

#[test]
fn zero_long_break_interval_is_rejected() {
    let error = TimerConfig::new(25, 5, 15, 0).unwrap_err();

    assert_eq!(error, ConfigValidationError::ZeroLongBreakInterval);
}

#[test]
fn invalid_file_values_include_the_file_path() {
    let path = temp_path("invalid.toml");
    write_current_config(
        &path,
        "[timer]\nfocus_duration = \"25:00\"\nshort_break_duration = \"05:00\"\nlong_break_duration = \"15:00\"\nlong_break_interval = 0\n",
    )
    .unwrap();

    let error = Config::load_from(&path).unwrap_err();

    assert!(matches!(error, ConfigError::Validation { .. }));
    assert!(error.to_string().contains(path.to_str().unwrap()));
    assert!(error.to_string().contains("long_break_interval"));
    fs::remove_file(path).unwrap();
}

#[test]
fn read_errors_include_the_file_path() {
    let path = temp_path("directory-instead-of-file");
    fs::create_dir(&path).unwrap();

    let error = Config::load_from(&path).unwrap_err();

    assert!(matches!(error, ConfigError::Read { .. }));
    assert!(error.to_string().contains(path.to_str().unwrap()));
    fs::remove_dir(path).unwrap();
}

#[test]
fn save_errors_include_the_failed_directory() {
    let parent = temp_path("parent-is-file");
    let path = parent.join("config.toml");
    fs::write(&parent, "not a directory").unwrap();

    let error = Config::default().save_to(path).unwrap_err();

    assert!(matches!(error, ConfigError::CreateDirectory { .. }));
    assert!(error.to_string().contains(parent.to_str().unwrap()));
    fs::remove_file(parent).unwrap();
}
