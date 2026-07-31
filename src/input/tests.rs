use super::*;

fn modified_event(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
    KeyEvent::new(code, modifiers)
}

fn map_default(
    key: KeyCode,
    edit_mode: EditMode,
    focus: UiFocus,
    confirmation_open: bool,
) -> Option<Action> {
    map_key(
        key,
        edit_mode,
        focus,
        confirmation_open,
        SettingsMode::Closed,
        &KeysConfig::default(),
    )
}

#[test]
fn maps_global_normal_mode_actions() {
    assert_eq!(
        map_default(KeyCode::Char('J'), EditMode::Normal, UiFocus::Clock, false),
        Some(Action::NavigateFocus(Direction::Down))
    );
    assert_eq!(
        map_default(KeyCode::Char('q'), EditMode::Normal, UiFocus::Done, false),
        Some(Action::Quit)
    );
}

#[test]
fn maps_control_and_alt_bindings_without_matching_plain_keys() {
    let keys: KeysConfig =
        toml::from_str("cycle_session = \"ctrl+c\"\nlist_down = \"alt+down\"\n").unwrap();

    assert_eq!(
        map_key_event(
            modified_event(KeyCode::Char('c'), KeyModifiers::CONTROL),
            EditMode::Normal,
            UiFocus::Clock,
            false,
            SettingsMode::Closed,
            &keys,
        ),
        Some(Action::CycleSession)
    );
    assert_eq!(
        map_key(
            KeyCode::Char('c'),
            EditMode::Normal,
            UiFocus::Clock,
            false,
            SettingsMode::Closed,
            &keys,
        ),
        None
    );
    assert_eq!(
        map_key_event(
            modified_event(KeyCode::Down, KeyModifiers::ALT),
            EditMode::Normal,
            UiFocus::Todo,
            false,
            SettingsMode::Closed,
            &keys,
        ),
        Some(Action::MoveSelection(Direction::Down))
    );
}

#[test]
fn key_capture_preserves_control_and_alt_modifiers() {
    let keys = KeysConfig::default();
    let event = modified_event(
        KeyCode::Char('q'),
        KeyModifiers::CONTROL | KeyModifiers::ALT,
    );

    assert_eq!(
        map_key_event(
            event,
            EditMode::Normal,
            UiFocus::Clock,
            false,
            SettingsMode::CapturingKey,
            &keys,
        ),
        Some(Action::SettingsCaptureKey(
            ConfigKey::Character('q').with_modifiers(true, true, false)
        ))
    );
}

#[test]
fn key_capture_preserves_shift_for_every_supported_non_character_key() {
    let keys = KeysConfig::default();
    for (code, configured) in [
        (KeyCode::Char(' '), ConfigKey::Space),
        (KeyCode::Enter, ConfigKey::Enter),
        (KeyCode::Esc, ConfigKey::Escape),
        (KeyCode::Backspace, ConfigKey::Backspace),
        (KeyCode::Up, ConfigKey::Up),
        (KeyCode::Down, ConfigKey::Down),
        (KeyCode::Left, ConfigKey::Left),
        (KeyCode::Right, ConfigKey::Right),
    ] {
        assert_eq!(
            map_key_event(
                modified_event(code, KeyModifiers::SHIFT),
                EditMode::Normal,
                UiFocus::Clock,
                false,
                SettingsMode::CapturingKey,
                &keys,
            ),
            Some(Action::SettingsCaptureKey(
                configured.with_modifiers(false, false, true)
            ))
        );
    }

    assert_eq!(
        map_key_event(
            modified_event(KeyCode::Left, KeyModifiers::CONTROL | KeyModifiers::SHIFT),
            EditMode::Normal,
            UiFocus::Clock,
            false,
            SettingsMode::CapturingKey,
            &keys,
        ),
        Some(Action::SettingsCaptureKey(
            ConfigKey::Left.with_modifiers(true, false, true)
        ))
    );
}

#[test]
fn shifted_printable_key_capture_uses_the_reported_character() {
    let keys = KeysConfig::default();
    for character in ['A', '?'] {
        assert_eq!(
            map_key_event(
                modified_event(KeyCode::Char(character), KeyModifiers::SHIFT),
                EditMode::Normal,
                UiFocus::Clock,
                false,
                SettingsMode::CapturingKey,
                &keys,
            ),
            Some(Action::SettingsCaptureKey(ConfigKey::Character(character)))
        );
    }
}

#[test]
fn shifted_bindings_do_not_match_unmodified_keys() {
    let keys: KeysConfig = toml::from_str("list_down = \"shift+down\"\n").unwrap();

    assert_eq!(
        map_key_event(
            modified_event(KeyCode::Down, KeyModifiers::SHIFT),
            EditMode::Normal,
            UiFocus::Todo,
            false,
            SettingsMode::Closed,
            &keys,
        ),
        Some(Action::MoveSelection(Direction::Down))
    );
    assert_eq!(
        map_key(
            KeyCode::Down,
            EditMode::Normal,
            UiFocus::Todo,
            false,
            SettingsMode::Closed,
            &keys,
        ),
        None
    );
}

#[test]
fn super_meta_and_hyper_events_are_rejected_for_matching_and_capture() {
    let keys: KeysConfig = toml::from_str(
        "cycle_session = [\"q\", \"ctrl+q\", \"alt+q\"]\nlist_down = \"shift+down\"\n",
    )
    .unwrap();
    let events = [
        modified_event(KeyCode::Char('q'), KeyModifiers::SUPER),
        modified_event(
            KeyCode::Char('q'),
            KeyModifiers::CONTROL | KeyModifiers::META,
        ),
        modified_event(KeyCode::Char('q'), KeyModifiers::ALT | KeyModifiers::HYPER),
        modified_event(KeyCode::Down, KeyModifiers::SHIFT | KeyModifiers::SUPER),
    ];

    for event in events {
        assert_eq!(
            map_key_event(
                event,
                EditMode::Normal,
                UiFocus::Clock,
                false,
                SettingsMode::Closed,
                &keys,
            ),
            None
        );
        assert_eq!(
            map_key_event(
                event,
                EditMode::Normal,
                UiFocus::Clock,
                false,
                SettingsMode::CapturingKey,
                &keys,
            ),
            None
        );
    }
}

#[test]
fn modified_keys_do_not_trigger_fixed_modal_controls() {
    let keys = KeysConfig::default();
    assert_eq!(
        map_key_event(
            modified_event(KeyCode::Char('y'), KeyModifiers::CONTROL),
            EditMode::Normal,
            UiFocus::Clock,
            true,
            SettingsMode::Closed,
            &keys,
        ),
        None
    );
    assert_eq!(
        map_key_event(
            modified_event(KeyCode::Char('q'), KeyModifiers::ALT),
            EditMode::Adding,
            UiFocus::Todo,
            false,
            SettingsMode::Closed,
            &keys,
        ),
        None
    );
}

#[test]
fn maps_keys_by_focused_area() {
    assert_eq!(
        map_default(KeyCode::Char(' '), EditMode::Normal, UiFocus::Clock, false),
        Some(Action::PrimaryAction)
    );
    assert_eq!(
        map_default(KeyCode::Char('a'), EditMode::Normal, UiFocus::Todo, false),
        Some(Action::BeginAdd)
    );
    assert_eq!(
        map_default(KeyCode::Down, EditMode::Normal, UiFocus::Done, false),
        Some(Action::MoveSelection(Direction::Down))
    );
    assert_eq!(
        map_default(KeyCode::Char('a'), EditMode::Normal, UiFocus::Done, false),
        Some(Action::BeginAdd)
    );
    assert_eq!(
        map_default(KeyCode::Down, EditMode::Normal, UiFocus::Clock, false),
        None
    );
    assert_eq!(
        map_default(KeyCode::Char('u'), EditMode::Normal, UiFocus::Todo, false),
        Some(Action::MoveSelectedTask(Direction::Up))
    );
    assert_eq!(
        map_default(KeyCode::Char('d'), EditMode::Normal, UiFocus::Done, false),
        Some(Action::MoveSelectedTask(Direction::Down))
    );
    assert_eq!(
        map_default(KeyCode::Char('c'), EditMode::Normal, UiFocus::Clock, false),
        Some(Action::CycleSession)
    );
    assert_eq!(
        map_default(KeyCode::Char('c'), EditMode::Normal, UiFocus::Todo, false),
        None
    );
}

#[test]
fn edit_mode_takes_precedence_over_normal_commands() {
    assert_eq!(
        map_default(KeyCode::Char('q'), EditMode::Adding, UiFocus::Todo, false),
        Some(Action::PushInput('q'))
    );
    assert_eq!(
        map_default(
            KeyCode::Char('J'),
            EditMode::Editing { task_index: 0 },
            UiFocus::Todo,
            false,
        ),
        Some(Action::PushInput('J'))
    );
    assert_eq!(
        map_default(KeyCode::Enter, EditMode::Adding, UiFocus::Todo, false),
        Some(Action::SubmitEdit)
    );
    assert_eq!(
        map_default(KeyCode::Left, EditMode::Adding, UiFocus::Todo, false),
        None
    );
}

#[test]
fn normal_mode_ignores_unmapped_keys() {
    assert_eq!(
        map_default(KeyCode::Enter, EditMode::Normal, UiFocus::Todo, false),
        None
    );
    assert_eq!(
        map_default(KeyCode::Char('h'), EditMode::Normal, UiFocus::Todo, false),
        None
    );
}

#[test]
fn confirmation_keys_take_precedence_over_every_other_context() {
    for (key, expected) in [
        (KeyCode::Char('y'), Some(Action::ConfirmPendingAction)),
        (KeyCode::Enter, Some(Action::ConfirmPendingAction)),
        (KeyCode::Char('n'), Some(Action::CancelPendingAction)),
        (KeyCode::Esc, Some(Action::CancelPendingAction)),
        (KeyCode::Char('q'), None),
        (KeyCode::Char('H'), None),
    ] {
        assert_eq!(
            map_default(key, EditMode::Adding, UiFocus::Todo, true),
            expected
        );
    }
}

#[test]
fn configured_keys_replace_defaults_in_their_context() {
    let keys: KeysConfig = toml::from_str(
        "focus_left = \"left\"\nclock_primary = \"backspace\"\ncycle_session = \"n\"\n",
    )
    .unwrap();

    assert_eq!(
        map_key(
            KeyCode::Left,
            EditMode::Normal,
            UiFocus::Clock,
            false,
            SettingsMode::Closed,
            &keys
        ),
        Some(Action::NavigateFocus(Direction::Left))
    );
    assert_eq!(
        map_key(
            KeyCode::Backspace,
            EditMode::Normal,
            UiFocus::Clock,
            false,
            SettingsMode::Closed,
            &keys
        ),
        Some(Action::PrimaryAction)
    );
    assert_eq!(
        map_key(
            KeyCode::Char('n'),
            EditMode::Normal,
            UiFocus::Clock,
            false,
            SettingsMode::Closed,
            &keys
        ),
        Some(Action::CycleSession)
    );
    assert_eq!(
        map_key(
            KeyCode::Char('c'),
            EditMode::Normal,
            UiFocus::Clock,
            false,
            SettingsMode::Closed,
            &keys
        ),
        None
    );
}

#[test]
fn configured_list_keys_do_not_keep_default_aliases() {
    let keys: KeysConfig = toml::from_str("list_down = \"n\"\nlist_up = \"p\"\n").unwrap();

    assert_eq!(
        map_key(
            KeyCode::Char('n'),
            EditMode::Normal,
            UiFocus::Todo,
            false,
            SettingsMode::Closed,
            &keys
        ),
        Some(Action::MoveSelection(Direction::Down))
    );
    for key in [KeyCode::Char('j'), KeyCode::Down] {
        assert_eq!(
            map_key(
                key,
                EditMode::Normal,
                UiFocus::Todo,
                false,
                SettingsMode::Closed,
                &keys
            ),
            None
        );
    }
    assert_eq!(
        map_key(
            KeyCode::Char('p'),
            EditMode::Normal,
            UiFocus::Done,
            false,
            SettingsMode::Closed,
            &keys
        ),
        Some(Action::MoveSelection(Direction::Up))
    );
    for key in [KeyCode::Char('k'), KeyCode::Up] {
        assert_eq!(
            map_key(
                key,
                EditMode::Normal,
                UiFocus::Done,
                false,
                SettingsMode::Closed,
                &keys
            ),
            None
        );
    }
}

#[test]
fn configured_task_movement_keys_replace_the_defaults() {
    let keys: KeysConfig =
        toml::from_str("move_task_up = \"w\"\nmove_task_down = \"z\"\n").unwrap();

    assert_eq!(
        map_key(
            KeyCode::Char('w'),
            EditMode::Normal,
            UiFocus::Todo,
            false,
            SettingsMode::Closed,
            &keys
        ),
        Some(Action::MoveSelectedTask(Direction::Up))
    );
    assert_eq!(
        map_key(
            KeyCode::Char('z'),
            EditMode::Normal,
            UiFocus::Done,
            false,
            SettingsMode::Closed,
            &keys
        ),
        Some(Action::MoveSelectedTask(Direction::Down))
    );
    for key in [KeyCode::Char('u'), KeyCode::Char('d')] {
        assert_eq!(
            map_key(
                key,
                EditMode::Normal,
                UiFocus::Todo,
                false,
                SettingsMode::Closed,
                &keys
            ),
            None
        );
    }
}

#[test]
fn every_configured_key_for_an_action_is_mapped() {
    let keys: KeysConfig =
        toml::from_str("cycle_session = [\"c\", \"n\"]\nquit = [\"q\", \"z\"]\n").unwrap();

    for key in [KeyCode::Char('c'), KeyCode::Char('n')] {
        assert_eq!(
            map_key(
                key,
                EditMode::Normal,
                UiFocus::Clock,
                false,
                SettingsMode::Closed,
                &keys
            ),
            Some(Action::CycleSession)
        );
    }
    assert_eq!(
        map_key(
            KeyCode::Char('z'),
            EditMode::Normal,
            UiFocus::Done,
            false,
            SettingsMode::Closed,
            &keys
        ),
        Some(Action::Quit)
    );
}

#[test]
fn editing_and_confirmation_override_configured_normal_keys() {
    let keys: KeysConfig =
        toml::from_str("clock_primary = \"backspace\"\ncycle_session = \"n\"\n").unwrap();

    assert_eq!(
        map_key(
            KeyCode::Backspace,
            EditMode::Adding,
            UiFocus::Todo,
            false,
            SettingsMode::Closed,
            &keys
        ),
        Some(Action::PopInput)
    );
    assert_eq!(
        map_key(
            KeyCode::Char('n'),
            EditMode::Normal,
            UiFocus::Clock,
            true,
            SettingsMode::Closed,
            &keys
        ),
        Some(Action::CancelPendingAction)
    );
}

#[test]
fn settings_context_has_fixed_navigation_and_nested_editing_precedence() {
    let keys = KeysConfig::default();
    assert_eq!(
        map_key(
            KeyCode::Char('s'),
            EditMode::Normal,
            UiFocus::Clock,
            false,
            SettingsMode::Closed,
            &keys
        ),
        Some(Action::OpenSettings)
    );
    assert_eq!(
        map_key(
            KeyCode::Esc,
            EditMode::Normal,
            UiFocus::Clock,
            false,
            SettingsMode::Closed,
            &keys
        ),
        Some(Action::CancelPendingAction)
    );
    assert_eq!(
        map_key(
            KeyCode::Char('s'),
            EditMode::Normal,
            UiFocus::Clock,
            false,
            SettingsMode::Navigating,
            &keys
        ),
        Some(Action::SettingsClose)
    );
    assert_eq!(
        map_key(
            KeyCode::Esc,
            EditMode::Normal,
            UiFocus::Clock,
            false,
            SettingsMode::Navigating,
            &keys
        ),
        Some(Action::SettingsClose)
    );
    for (key, expected) in [
        (KeyCode::Up, Action::SettingsMove(SettingsMoveDirection::Up)),
        (
            KeyCode::Down,
            Action::SettingsMove(SettingsMoveDirection::Down),
        ),
        (
            KeyCode::Left,
            Action::SettingsAdjust(SettingsAdjustmentDirection::Backward),
        ),
        (
            KeyCode::Char('l'),
            Action::SettingsAdjust(SettingsAdjustmentDirection::Forward),
        ),
    ] {
        assert_eq!(
            map_key(
                key,
                EditMode::Normal,
                UiFocus::Clock,
                false,
                SettingsMode::Navigating,
                &keys
            ),
            Some(expected)
        );
    }
    assert_eq!(
        map_key(
            KeyCode::Char('7'),
            EditMode::Normal,
            UiFocus::Clock,
            false,
            SettingsMode::EditingValue,
            &keys
        ),
        Some(Action::SettingsPushInput('7'))
    );
    assert_eq!(
        map_key(
            KeyCode::Char('q'),
            EditMode::Normal,
            UiFocus::Clock,
            false,
            SettingsMode::CapturingKey,
            &keys
        ),
        Some(Action::SettingsCaptureKey(ConfigKey::Character('q')))
    );
    assert_eq!(
        map_key(
            KeyCode::Esc,
            EditMode::Normal,
            UiFocus::Clock,
            false,
            SettingsMode::CapturingKey,
            &keys
        ),
        Some(Action::SettingsCancel)
    );

    let keys: KeysConfig = toml::from_str("settings = \"t\"\ncycle_session = \"s\"\n").unwrap();
    assert_eq!(
        map_key(
            KeyCode::Char('t'),
            EditMode::Normal,
            UiFocus::Clock,
            false,
            SettingsMode::Closed,
            &keys
        ),
        Some(Action::OpenSettings)
    );
    assert_eq!(
        map_key(
            KeyCode::Char('t'),
            EditMode::Normal,
            UiFocus::Clock,
            false,
            SettingsMode::Navigating,
            &keys
        ),
        Some(Action::SettingsClose)
    );
    assert_eq!(
        map_key(
            KeyCode::Char('t'),
            EditMode::Adding,
            UiFocus::Todo,
            false,
            SettingsMode::Closed,
            &keys
        ),
        Some(Action::PushInput('t'))
    );
    assert_eq!(
        map_key(
            KeyCode::Char('t'),
            EditMode::Normal,
            UiFocus::Clock,
            false,
            SettingsMode::EditingValue,
            &keys
        ),
        Some(Action::SettingsPushInput('t'))
    );
}
