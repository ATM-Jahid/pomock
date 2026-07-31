use super::*;
use crate::{
    app::{SettingsAdjustmentDirection, SettingsMoveDirection},
    config::{SoundConfig, ThemeColor, ThemeConfig, ThemeRole},
};

fn select(settings: &mut SettingsOverlay, field: SettingField) {
    let index = SettingField::ALL
        .iter()
        .position(|candidate| *candidate == field)
        .unwrap();
    settings.select(index);
}

#[test]
fn field_groups_define_the_flat_settings_order() {
    let grouped = SettingField::GROUPS
        .iter()
        .flat_map(|(_, fields)| fields.iter().copied())
        .collect::<Vec<_>>();

    assert_eq!(grouped, SettingField::ALL);
    assert_eq!(
        SettingField::KEYS,
        [
            SettingField::Key(KeyAction::Quit),
            SettingField::Key(KeyAction::Settings),
            SettingField::Key(KeyAction::FocusLeft),
            SettingField::Key(KeyAction::FocusDown),
            SettingField::Key(KeyAction::FocusUp),
            SettingField::Key(KeyAction::FocusRight),
            SettingField::Key(KeyAction::ClockPrimary),
            SettingField::Key(KeyAction::CycleSession),
            SettingField::Key(KeyAction::ResetSession),
            SettingField::Key(KeyAction::AddTask),
            SettingField::Key(KeyAction::EditTask),
            SettingField::Key(KeyAction::DeleteTask),
            SettingField::Key(KeyAction::TaskPrimary),
            SettingField::Key(KeyAction::ListDown),
            SettingField::Key(KeyAction::ListUp),
            SettingField::Key(KeyAction::MoveTaskUp),
            SettingField::Key(KeyAction::MoveTaskDown),
        ]
    );
    assert_eq!(
        SettingField::THEME,
        [
            SettingField::Theme(ThemeRole::FocusedBorder),
            SettingField::Theme(ThemeRole::UnfocusedBorder),
            SettingField::Theme(ThemeRole::Focus),
            SettingField::Theme(ThemeRole::ShortBreak),
            SettingField::Theme(ThemeRole::LongBreak),
            SettingField::Theme(ThemeRole::TodoHighlight),
            SettingField::Theme(ThemeRole::DoneHighlight),
        ]
    );
}

#[test]
fn numeric_edits_are_validated_before_updating_the_config() {
    let mut settings = SettingsOverlay::new(&Config::default());
    settings.activate();
    settings.pop_input();
    settings.pop_input();
    settings.submit_input();

    assert_eq!(
        settings.config().timer().focus_duration().as_secs(),
        25 * 60
    );
    assert!(settings.error().is_some());

    settings.activate();
    for _ in 0..5 {
        settings.pop_input();
    }
    for character in "40:30".chars() {
        settings.push_input(character);
    }
    settings.submit_input();

    assert_eq!(
        settings.config().timer().focus_duration().as_secs(),
        40 * 60 + 30
    );
    assert!(settings.error().is_none());
}

#[test]
fn malformed_long_break_intervals_are_rejected_as_non_integers() {
    for invalid in ["abc", "", "-1", "1.5"] {
        let mut settings = SettingsOverlay::new(&Config::default());

        settings.set_number(SettingField::LongBreakInterval, invalid.to_string());

        assert_eq!(settings.config().timer().long_break_interval().get(), 4);
        assert_eq!(
            settings.error(),
            Some("long_break_interval must be a positive integer"),
            "unexpected validation result for {invalid:?}"
        );
    }
}

#[test]
fn zero_long_break_interval_is_rejected_by_the_overlay() {
    let mut settings = SettingsOverlay::new(&Config::default());

    settings.set_number(SettingField::LongBreakInterval, "0".to_string());

    assert_eq!(settings.config().timer().long_break_interval().get(), 4);
    assert_eq!(
        settings.error(),
        Some("long_break_interval must be greater than zero")
    );
}

#[test]
fn overflowing_long_break_intervals_are_rejected_as_too_large() {
    for overflow in [
        (u64::from(u32::MAX) + 1).to_string(),
        "18446744073709551616".to_string(),
    ] {
        let mut settings = SettingsOverlay::new(&Config::default());

        settings.set_number(SettingField::LongBreakInterval, overflow);

        assert_eq!(settings.config().timer().long_break_interval().get(), 4);
        assert_eq!(settings.error(), Some("long_break_interval is too large"));
    }
}

#[test]
fn valid_long_break_interval_updates_the_overlay_config() {
    let mut settings = SettingsOverlay::new(&Config::default());

    settings.set_number(SettingField::LongBreakInterval, "5".to_string());

    assert_eq!(settings.config().timer().long_break_interval().get(), 5);
    assert!(settings.error().is_none());
}

#[test]
fn duration_edits_require_mm_ss_and_reject_invalid_seconds() {
    let mut settings = SettingsOverlay::new(&Config::default());

    for invalid in ["5:30", "05:60", "00:00", "05", "10000:00"] {
        settings.activate();
        for _ in 0..settings.input().unwrap().len() {
            settings.pop_input();
        }
        for character in invalid.chars() {
            settings.push_input(character);
        }
        settings.submit_input();

        assert_eq!(
            settings.config().timer().focus_duration().as_secs(),
            25 * 60
        );
        assert!(settings.error().is_some(), "{invalid} should be rejected");
    }

    settings.activate();
    for _ in 0..settings.input().unwrap().len() {
        settings.pop_input();
    }
    for character in "9999:59".chars() {
        settings.push_input(character);
    }
    settings.submit_input();

    assert_eq!(
        settings.config().timer().focus_duration().as_secs(),
        9999 * 60 + 59
    );
    assert!(settings.error().is_none());
}

#[test]
fn timer_value_edits_preserve_autostart_settings() {
    let config = Config::new(TimerConfig::default().with_autostart(true, true)).unwrap();
    let mut settings = SettingsOverlay::new(&config);

    settings.set_duration(SettingField::FocusDuration, "20:30".to_string());
    assert!(settings.config().timer().autostart_breaks());
    assert!(settings.config().timer().autostart_focus());

    select(&mut settings, SettingField::LongBreakInterval);
    settings.set_number(SettingField::LongBreakInterval, "5".to_string());
    assert!(settings.config().timer().autostart_breaks());
    assert!(settings.config().timer().autostart_focus());
}

#[test]
fn editing_other_settings_preserves_the_completion_sound() {
    let sound_file = std::env::current_dir().unwrap().join("custom.wav");
    let config = Config::default()
        .with_sound(
            SoundConfig::default()
                .with_completion(CompletionSoundConfig::new(true, Some(sound_file.clone()))),
        )
        .unwrap();
    let mut settings = SettingsOverlay::new(&config);

    settings.set_tasks(false, false);

    assert_eq!(
        settings.config().sound().completion().file(),
        Some(sound_file.as_path())
    );
}

#[test]
fn notification_and_sound_changes_apply_to_the_overlay_config() {
    let mut settings = SettingsOverlay::new(&Config::default());
    let completion = std::env::current_dir().unwrap().join("complete.wav");
    let focus = std::env::current_dir().unwrap().join("focus.ogg");

    select(&mut settings, SettingField::NotificationEnabled);
    settings.activate();
    select(&mut settings, SettingField::CompletionSoundEnabled);
    settings.activate();
    select(&mut settings, SettingField::CompletionSoundFile);
    settings.activate();
    for character in completion.display().to_string().chars() {
        settings.push_input(character);
    }
    settings.submit_input();
    select(&mut settings, SettingField::FocusSoundEnabled);
    settings.activate();
    select(&mut settings, SettingField::FocusSoundFile);
    settings.activate();
    for character in focus.display().to_string().chars() {
        settings.push_input(character);
    }
    settings.submit_input();

    assert!(!settings.config().notification().enabled());
    assert!(settings.config().sound().completion().enabled());
    assert!(settings.config().sound().focus().enabled());
    assert_eq!(
        settings.config().sound().completion().file(),
        Some(completion.as_path())
    );
    assert_eq!(
        settings.config().sound().focus().file(),
        Some(focus.as_path())
    );

    settings.activate();
    for _ in 0..focus.display().to_string().len() {
        settings.pop_input();
    }
    settings.submit_input();
    assert!(settings.config().sound().focus().file().is_none());
}

#[test]
fn invalid_sound_path_is_rejected_without_replacing_the_accepted_value() {
    let accepted = std::env::current_dir().unwrap().join("complete.wav");
    let config = Config::default()
        .with_sound(
            SoundConfig::default()
                .with_completion(CompletionSoundConfig::new(true, Some(accepted.clone()))),
        )
        .unwrap();
    let mut settings = SettingsOverlay::new(&config);
    select(&mut settings, SettingField::CompletionSoundFile);
    settings.activate();
    for _ in 0..accepted.display().to_string().len() {
        settings.pop_input();
    }
    for character in "relative.wav".chars() {
        settings.push_input(character);
    }

    settings.submit_input();

    assert_eq!(
        settings.config().sound().completion().file(),
        Some(accepted.as_path())
    );
    assert!(settings.error().unwrap().contains("sound.completion.file"));
}

#[test]
fn booleans_and_theme_colors_update_the_overlay_config() {
    let original = Config::default();
    let mut settings = SettingsOverlay::new(&original);
    let original_border = original.theme().color(ThemeRole::FocusedBorder);
    let original_focus = original.theme().color(ThemeRole::Focus);
    select(&mut settings, SettingField::PersistTasks);
    settings.adjust(SettingsAdjustmentDirection::Forward);
    select(&mut settings, SettingField::AutostartBreaks);
    settings.adjust(SettingsAdjustmentDirection::Forward);
    select(&mut settings, SettingField::AutostartFocus);
    settings.activate();
    select(&mut settings, SettingField::Theme(ThemeRole::FocusedBorder));
    settings.adjust(SettingsAdjustmentDirection::Forward);

    assert!(!settings.config().tasks().persist());
    assert!(settings.config().timer().autostart_breaks());
    assert!(settings.config().timer().autostart_focus());
    assert_eq!(
        settings.config().theme().color(ThemeRole::FocusedBorder),
        original_border.cycle_forward()
    );
    assert!(original.tasks().persist());
    assert_eq!(
        original.theme().color(ThemeRole::FocusedBorder),
        original_border
    );

    select(&mut settings, SettingField::Theme(ThemeRole::Focus));
    settings.adjust(SettingsAdjustmentDirection::Forward);
    assert_eq!(
        settings.config().theme().focus(),
        original_focus.cycle_forward()
    );
}

#[test]
fn named_directions_move_selection_and_adjust_numbers() {
    let mut settings = SettingsOverlay::new(&Config::default());

    settings.move_selection(SettingsMoveDirection::Down);
    assert_eq!(settings.selection(), 1);
    settings.move_selection(SettingsMoveDirection::Up);
    assert_eq!(settings.selection(), 0);

    select(&mut settings, SettingField::LongBreakInterval);
    let original = settings.config().timer().long_break_interval().get();
    settings.adjust(SettingsAdjustmentDirection::Forward);
    assert_eq!(
        settings.config().timer().long_break_interval().get(),
        original + 1
    );
    settings.adjust(SettingsAdjustmentDirection::Backward);
    assert_eq!(
        settings.config().timer().long_break_interval().get(),
        original
    );
}

#[test]
fn valid_hex_color_edits_update_the_config_on_submit() {
    let mut settings = SettingsOverlay::new(&Config::default());
    select(&mut settings, SettingField::Theme(ThemeRole::FocusedBorder));
    settings.activate();
    let original_length = settings.input().unwrap().chars().count();
    for _ in 0..original_length {
        settings.pop_input();
    }
    for character in "#5FD7fF".chars() {
        settings.push_input(character);
    }

    settings.submit_input();
    assert_eq!(
        settings.config().theme().focused_border(),
        ThemeColor::Rgb(0x5f, 0xd7, 0xff)
    );
}

#[test]
fn invalid_color_edits_leave_the_config_unchanged() {
    let mut settings = SettingsOverlay::new(&Config::default());
    let original = settings.config().theme().focused_border();
    select(&mut settings, SettingField::Theme(ThemeRole::FocusedBorder));
    settings.activate();
    let original_length = settings.input().unwrap().chars().count();
    for _ in 0..original_length {
        settings.pop_input();
    }
    for character in "#12345".chars() {
        settings.push_input(character);
    }

    settings.submit_input();

    assert_eq!(settings.config().theme().focused_border(), original);
    assert!(settings.error().unwrap().contains("#RRGGBB"));
}

#[test]
fn arrows_and_h_l_can_cycle_from_a_custom_color_into_presets() {
    let theme =
        ThemeConfig::default().with_color(ThemeRole::FocusedBorder, ThemeColor::Rgb(1, 2, 3));
    let config =
        Config::with_tasks_and_theme(TimerConfig::default(), TasksConfig::default(), theme)
            .unwrap();
    let mut settings = SettingsOverlay::new(&config);
    select(&mut settings, SettingField::Theme(ThemeRole::FocusedBorder));

    settings.adjust(SettingsAdjustmentDirection::Forward);

    assert_eq!(
        settings.config().theme().focused_border(),
        ThemeColor::Black
    );
}

#[test]
fn key_capture_rejects_context_conflicts_and_accepts_valid_keys() {
    let mut settings = SettingsOverlay::new(&Config::default());
    select(&mut settings, SettingField::Key(KeyAction::CycleSession));
    settings.activate();
    settings.capture_key(ConfigKey::Space);
    assert!(settings.is_capturing_key());
    assert!(settings.error().is_some());

    settings.capture_key(ConfigKey::Character('n'));
    assert!(!settings.is_capturing_key());
    assert_eq!(
        settings.config().keys().binding(KeyAction::CycleSession),
        [ConfigKey::Character('n')]
    );
}

#[test]
fn settings_key_capture_rejects_overlay_controls_and_updates_the_config() {
    let mut settings = SettingsOverlay::new(&Config::default());
    select(&mut settings, SettingField::Key(KeyAction::Settings));
    settings.activate();
    settings.capture_key(ConfigKey::Enter);
    assert!(settings.is_capturing_key());
    assert!(settings.error().unwrap().contains("keys.settings"));

    settings.capture_key(ConfigKey::Character('t'));

    assert!(!settings.is_capturing_key());
    assert_eq!(
        settings.config().keys().settings(),
        [ConfigKey::Character('t')]
    );
}

#[test]
fn selection_is_clamped_and_locked_during_nested_editing() {
    let mut settings = SettingsOverlay::new(&Config::default());
    settings.select(usize::MAX);
    assert_eq!(
        settings.field(),
        SettingField::Theme(ThemeRole::DoneHighlight)
    );
    settings.select(0);
    settings.activate();
    settings.move_selection(SettingsMoveDirection::Down);
    assert_eq!(settings.selection(), 0);
    assert!(settings.cancel_nested());
}
