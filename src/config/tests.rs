use super::{
    CompletionSoundConfig, Config, ConfigKey, ConfigValidationError, KeyBindings, KeysConfig,
    SoundConfig, TasksConfig, ThemeConfig, TimerConfig,
};

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
fn absolute_sound_paths_pass_through_unchanged() {
    let sound_file = std::env::temp_dir().join("absolute-sound.wav");
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
fn focused_timer_updates_validate_and_preserve_other_fields() {
    let timer = TimerConfig::from_seconds(120, 30, 60, 3)
        .unwrap()
        .with_autostart(true, true);
    for (updated, expected) in [
        (timer.with_focus_duration(90), (90, 30, 60, 3)),
        (timer.with_short_break_duration(90), (120, 90, 60, 3)),
        (timer.with_long_break_duration(90), (120, 30, 90, 3)),
        (timer.with_long_break_interval(7), (120, 30, 60, 7)),
    ] {
        let updated = updated.unwrap();
        assert_eq!(
            (
                updated.focus_duration().as_secs(),
                updated.short_break_duration().as_secs(),
                updated.long_break_duration().as_secs(),
                updated.long_break_interval().get()
            ),
            expected
        );
        assert!(updated.autostart_breaks());
        assert!(updated.autostart_focus());
    }
    for seconds in [0, 600_000, u64::MAX] {
        assert!(timer.with_focus_duration(seconds).is_err());
        assert!(timer.with_short_break_duration(seconds).is_err());
        assert!(timer.with_long_break_duration(seconds).is_err());
    }
    assert_eq!(
        timer.with_long_break_interval(0),
        Err(ConfigValidationError::ZeroLongBreakInterval)
    );
}

#[test]
fn focused_config_updates_preserve_unrelated_settings() {
    let original = Config::with_tasks(
        TimerConfig::default().with_autostart(true, true),
        TasksConfig::with_numbering(false, false),
    )
    .unwrap()
    .with_notification(super::NotificationConfig::new(false))
    .with_sound(
        SoundConfig::default().with_completion(CompletionSoundConfig::new(
            true,
            Some(std::env::temp_dir().join("complete.wav")),
        )),
    )
    .unwrap();
    let updated = original
        .clone()
        .with_timer(original.timer().with_focus_duration(90).unwrap())
        .unwrap()
        .with_color(super::ThemeRole::Focus, super::ThemeColor::Blue)
        .with_key_binding(super::KeyAction::Quit, ConfigKey::Character('Q'))
        .unwrap();
    let restored = updated
        .with_timer(*original.timer())
        .unwrap()
        .with_color(
            super::ThemeRole::Focus,
            original.theme().color(super::ThemeRole::Focus),
        )
        .with_key_binding(super::KeyAction::Quit, ConfigKey::Character('q'))
        .unwrap();
    assert_eq!(restored, original);
    assert!(
        original
            .clone()
            .with_key_binding(super::KeyAction::Quit, ConfigKey::Character('s'))
            .is_err()
    );
}
