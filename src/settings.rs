use std::path::{Path, PathBuf};

use crate::config::{
    CompletionSoundConfig, Config, ConfigKey, ConfigValidationError, FocusSoundConfig, KeyAction,
    NotificationConfig, TasksConfig, ThemeRole, TimerConfig, format_duration, parse_duration,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SettingField {
    FocusDuration,
    ShortBreakDuration,
    LongBreakDuration,
    LongBreakInterval,
    AutostartBreaks,
    AutostartFocus,
    NotificationEnabled,
    CompletionSoundEnabled,
    CompletionSoundFile,
    FocusSoundEnabled,
    FocusSoundFile,
    PersistTasks,
    ShowTaskNumbers,
    Theme(ThemeRole),
    Key(KeyAction),
}

impl SettingField {
    const TIMER: [Self; 6] = [
        Self::FocusDuration,
        Self::ShortBreakDuration,
        Self::LongBreakDuration,
        Self::LongBreakInterval,
        Self::AutostartBreaks,
        Self::AutostartFocus,
    ];
    const NOTIFICATION: [Self; 1] = [Self::NotificationEnabled];
    const SOUND: [Self; 4] = [
        Self::CompletionSoundEnabled,
        Self::CompletionSoundFile,
        Self::FocusSoundEnabled,
        Self::FocusSoundFile,
    ];
    const TASKS: [Self; 2] = [Self::PersistTasks, Self::ShowTaskNumbers];
    const KEYS: [Self; 17] = [
        Self::Key(KeyAction::Quit),
        Self::Key(KeyAction::Settings),
        Self::Key(KeyAction::FocusLeft),
        Self::Key(KeyAction::FocusDown),
        Self::Key(KeyAction::FocusUp),
        Self::Key(KeyAction::FocusRight),
        Self::Key(KeyAction::ClockPrimary),
        Self::Key(KeyAction::CycleSession),
        Self::Key(KeyAction::ResetSession),
        Self::Key(KeyAction::AddTask),
        Self::Key(KeyAction::EditTask),
        Self::Key(KeyAction::DeleteTask),
        Self::Key(KeyAction::TaskPrimary),
        Self::Key(KeyAction::ListDown),
        Self::Key(KeyAction::ListUp),
        Self::Key(KeyAction::MoveTaskUp),
        Self::Key(KeyAction::MoveTaskDown),
    ];
    const THEME: [Self; 7] = [
        Self::Theme(ThemeRole::FocusedBorder),
        Self::Theme(ThemeRole::UnfocusedBorder),
        Self::Theme(ThemeRole::Focus),
        Self::Theme(ThemeRole::ShortBreak),
        Self::Theme(ThemeRole::LongBreak),
        Self::Theme(ThemeRole::TodoHighlight),
        Self::Theme(ThemeRole::DoneHighlight),
    ];
    pub(crate) const GROUPS: [(&'static str, &'static [Self]); 6] = [
        ("Timer", &Self::TIMER),
        ("Notification", &Self::NOTIFICATION),
        ("Sound", &Self::SOUND),
        ("Tasks", &Self::TASKS),
        ("Keys", &Self::KEYS),
        ("Theme", &Self::THEME),
    ];
    const FIELD_COUNT: usize = Self::TIMER.len()
        + Self::NOTIFICATION.len()
        + Self::SOUND.len()
        + Self::TASKS.len()
        + Self::KEYS.len()
        + Self::THEME.len();
    pub(crate) const ALL: [Self; Self::FIELD_COUNT] = Self::flatten_groups();

    const fn flatten_groups() -> [Self; Self::FIELD_COUNT] {
        let mut all = [Self::FocusDuration; Self::FIELD_COUNT];
        let mut all_index = 0;
        let mut group_index = 0;
        while group_index < Self::GROUPS.len() {
            let fields = Self::GROUPS[group_index].1;
            let mut field_index = 0;
            while field_index < fields.len() {
                all[all_index] = fields[field_index];
                all_index += 1;
                field_index += 1;
            }
            group_index += 1;
        }
        all
    }

    fn is_number(self) -> bool {
        matches!(self, Self::LongBreakInterval)
    }

    fn is_duration(self) -> bool {
        matches!(
            self,
            Self::FocusDuration | Self::ShortBreakDuration | Self::LongBreakDuration
        )
    }

    fn is_text(self) -> bool {
        self.is_number()
            || self.is_duration()
            || matches!(
                self,
                Self::CompletionSoundFile | Self::FocusSoundFile | Self::Theme(_)
            )
    }
}

#[derive(Debug, Clone)]
pub(crate) struct SettingsOverlay {
    config: Config,
    selection: usize,
    offset: usize,
    input: Option<String>,
    capturing_key: bool,
    error: Option<String>,
}

impl SettingsOverlay {
    pub(crate) fn new(config: &Config) -> Self {
        Self {
            config: config.clone(),
            selection: 0,
            offset: 0,
            input: None,
            capturing_key: false,
            error: None,
        }
    }

    pub(crate) fn config(&self) -> &Config {
        &self.config
    }

    pub(crate) fn selection(&self) -> usize {
        self.selection
    }

    pub(crate) fn offset(&self) -> usize {
        self.offset
    }

    pub(crate) fn set_offset(&mut self, offset: usize) {
        self.offset = offset;
    }

    pub(crate) fn field(&self) -> SettingField {
        SettingField::ALL[self.selection]
    }

    pub(crate) fn input(&self) -> Option<&str> {
        self.input.as_deref()
    }

    pub(crate) fn is_capturing_key(&self) -> bool {
        self.capturing_key
    }

    pub(crate) fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub(crate) fn select(&mut self, selection: usize) {
        if self.input.is_none() && !self.capturing_key {
            self.selection = selection.min(SettingField::ALL.len() - 1);
            self.error = None;
        }
    }

    pub(crate) fn move_selection(&mut self, down: bool) {
        if self.input.is_some() || self.capturing_key {
            return;
        }
        if down {
            self.selection = (self.selection + 1).min(SettingField::ALL.len() - 1);
        } else {
            self.selection = self.selection.saturating_sub(1);
        }
        self.error = None;
    }

    pub(crate) fn adjust(&mut self, forward: bool) {
        if self.input.is_some() || self.capturing_key {
            return;
        }
        let field = self.field();
        match field {
            SettingField::NotificationEnabled => {
                self.set_notification(!self.config.notification().enabled());
            }
            SettingField::AutostartBreaks => self.set_autostart(
                !self.config.timer().autostart_breaks(),
                self.config.timer().autostart_focus(),
            ),
            SettingField::AutostartFocus => self.set_autostart(
                self.config.timer().autostart_breaks(),
                !self.config.timer().autostart_focus(),
            ),
            SettingField::CompletionSoundEnabled => {
                self.set_sound_enabled(!self.config.sound().completion().enabled(), false);
            }
            SettingField::FocusSoundEnabled => {
                self.set_sound_enabled(!self.config.sound().focus().enabled(), true);
            }
            SettingField::PersistTasks => self.set_tasks(
                !self.config.tasks().persist(),
                self.config.tasks().show_numbers(),
            ),
            SettingField::ShowTaskNumbers => self.set_tasks(
                self.config.tasks().persist(),
                !self.config.tasks().show_numbers(),
            ),
            SettingField::Theme(role) => {
                let theme = *self.config.theme();
                let color = theme.color(role).cycle(forward);
                self.replace(
                    self.config.timer().to_owned(),
                    *self.config.tasks(),
                    theme.with_color(role, color),
                    self.config.keys().clone(),
                );
            }
            _ if field.is_number() => {
                let current = self.number_value(field);
                let next = if forward {
                    current.saturating_add(1)
                } else {
                    current.saturating_sub(1).max(1)
                };
                self.set_number(field, next.to_string());
            }
            _ if field.is_duration() => {
                let current = self.duration_value(field).as_secs();
                let next = if forward {
                    current.saturating_add(60)
                } else {
                    current.saturating_sub(60).max(1)
                };
                self.set_duration(field, format_duration(std::time::Duration::from_secs(next)));
            }
            _ => {}
        }
    }

    pub(crate) fn activate(&mut self) {
        let field = self.field();
        if field.is_text() {
            self.input = Some(match field {
                SettingField::Theme(role) => self.config.theme().color(role).to_string(),
                SettingField::CompletionSoundFile => self
                    .config
                    .sound()
                    .completion()
                    .file()
                    .map_or_else(String::new, |path| path.display().to_string()),
                SettingField::FocusSoundFile => self
                    .config
                    .sound()
                    .focus()
                    .file()
                    .map_or_else(String::new, |path| path.display().to_string()),
                _ if field.is_duration() => format_duration(self.duration_value(field)),
                _ => self.number_value(field).to_string(),
            });
            self.error = None;
        } else {
            match field {
                SettingField::AutostartBreaks
                | SettingField::AutostartFocus
                | SettingField::NotificationEnabled
                | SettingField::CompletionSoundEnabled
                | SettingField::FocusSoundEnabled
                | SettingField::PersistTasks
                | SettingField::ShowTaskNumbers => self.adjust(true),
                SettingField::Key(_) => {
                    self.capturing_key = true;
                    self.error = None;
                }
                _ => {}
            }
        }
    }

    pub(crate) fn push_input(&mut self, character: char) {
        if let Some(input) = &mut self.input {
            input.push(character);
        }
    }

    pub(crate) fn pop_input(&mut self) {
        if let Some(input) = &mut self.input {
            input.pop();
        }
    }

    pub(crate) fn submit_input(&mut self) {
        let Some(value) = self.input.take() else {
            return;
        };
        match self.field() {
            SettingField::Theme(role) => self.set_color(role, value),
            SettingField::CompletionSoundFile => self.set_sound_file(value, false),
            SettingField::FocusSoundFile => self.set_sound_file(value, true),
            field if field.is_duration() => self.set_duration(field, value),
            field => self.set_number(field, value),
        }
    }

    pub(crate) fn cancel_nested(&mut self) -> bool {
        if self.input.take().is_some() || self.capturing_key || self.error.take().is_some() {
            self.capturing_key = false;
            self.error = None;
            true
        } else {
            false
        }
    }

    pub(crate) fn capture_key(&mut self, key: ConfigKey) {
        if !self.capturing_key {
            return;
        }
        let SettingField::Key(action) = self.field() else {
            return;
        };
        let keys = self.config.keys().clone().with_binding(action, key);
        match Config::with_all_settings(
            self.config.timer().to_owned(),
            *self.config.tasks(),
            *self.config.theme(),
            keys,
            self.config.notification(),
            self.config.sound().clone(),
        ) {
            Ok(config) => {
                self.config = config;
                self.capturing_key = false;
                self.error = None;
            }
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    fn number_value(&self, field: SettingField) -> u64 {
        match field {
            SettingField::LongBreakInterval => {
                u64::from(self.config.timer().long_break_interval().get())
            }
            _ => 0,
        }
    }

    fn duration_value(&self, field: SettingField) -> std::time::Duration {
        match field {
            SettingField::FocusDuration => self.config.timer().focus_duration(),
            SettingField::ShortBreakDuration => self.config.timer().short_break_duration(),
            SettingField::LongBreakDuration => self.config.timer().long_break_duration(),
            _ => std::time::Duration::ZERO,
        }
    }

    fn set_duration(&mut self, field: SettingField, value: String) {
        let field_name = match field {
            SettingField::FocusDuration => "focus_duration",
            SettingField::ShortBreakDuration => "short_break_duration",
            SettingField::LongBreakDuration => "long_break_duration",
            _ => return,
        };
        let result = parse_duration(&value, field_name).and_then(|value| {
            let timer = self.config.timer();
            let (focus, short, long) = match field {
                SettingField::FocusDuration => (
                    value,
                    timer.short_break_duration().as_secs(),
                    timer.long_break_duration().as_secs(),
                ),
                SettingField::ShortBreakDuration => (
                    timer.focus_duration().as_secs(),
                    value,
                    timer.long_break_duration().as_secs(),
                ),
                SettingField::LongBreakDuration => (
                    timer.focus_duration().as_secs(),
                    timer.short_break_duration().as_secs(),
                    value,
                ),
                _ => unreachable!(),
            };
            TimerConfig::from_seconds(focus, short, long, timer.long_break_interval().get()).map(
                |updated| updated.with_autostart(timer.autostart_breaks(), timer.autostart_focus()),
            )
        });
        match result {
            Ok(timer) => self.replace(
                timer,
                *self.config.tasks(),
                *self.config.theme(),
                self.config.keys().clone(),
            ),
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    fn set_number(&mut self, _field: SettingField, value: String) {
        let parsed = value
            .parse::<u64>()
            .map_err(|_| ConfigValidationError::ZeroDuration { field: "setting" });
        let result = parsed.and_then(|value| {
            let interval =
                u32::try_from(value).map_err(|_| ConfigValidationError::DurationOverflow {
                    field: "long_break_interval",
                })?;
            let timer = self.config.timer();
            TimerConfig::from_seconds(
                timer.focus_duration().as_secs(),
                timer.short_break_duration().as_secs(),
                timer.long_break_duration().as_secs(),
                interval,
            )
            .map(|updated| {
                updated.with_autostart(timer.autostart_breaks(), timer.autostart_focus())
            })
        });
        match result {
            Ok(timer) => self.replace(
                timer,
                *self.config.tasks(),
                *self.config.theme(),
                self.config.keys().clone(),
            ),
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    fn set_tasks(&mut self, persist: bool, show_numbers: bool) {
        self.replace(
            self.config.timer().to_owned(),
            TasksConfig::with_numbering(persist, show_numbers),
            *self.config.theme(),
            self.config.keys().clone(),
        );
    }

    fn set_autostart(&mut self, breaks: bool, focus: bool) {
        self.replace(
            self.config.timer().to_owned().with_autostart(breaks, focus),
            *self.config.tasks(),
            *self.config.theme(),
            self.config.keys().clone(),
        );
    }

    fn set_notification(&mut self, enabled: bool) {
        let mut config = self.config.clone();
        config = config.with_notification(NotificationConfig::new(enabled));
        self.config = config;
        self.error = None;
    }

    fn set_sound_file(&mut self, value: String, focus: bool) {
        let file = (!value.trim().is_empty()).then(|| PathBuf::from(value));
        let sound = if focus {
            self.config
                .sound()
                .clone()
                .with_focus(FocusSoundConfig::new(
                    self.config.sound().focus().enabled(),
                    file,
                ))
        } else {
            self.config
                .sound()
                .clone()
                .with_completion(CompletionSoundConfig::new(
                    self.config.sound().completion().enabled(),
                    file,
                ))
        };
        match self.config.clone().with_sound(sound) {
            Ok(config) => {
                self.config = config;
                self.error = None;
            }
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    fn set_sound_enabled(&mut self, enabled: bool, focus: bool) {
        let sound = if focus {
            self.config
                .sound()
                .clone()
                .with_focus(FocusSoundConfig::new(
                    enabled,
                    self.config.sound().focus().file().map(Path::to_path_buf),
                ))
        } else {
            self.config
                .sound()
                .clone()
                .with_completion(CompletionSoundConfig::new(
                    enabled,
                    self.config
                        .sound()
                        .completion()
                        .file()
                        .map(Path::to_path_buf),
                ))
        };
        match self.config.clone().with_sound(sound) {
            Ok(config) => {
                self.config = config;
                self.error = None;
            }
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    fn set_color(&mut self, role: ThemeRole, value: String) {
        match value.parse() {
            Ok(color) => {
                let theme = self.config.theme().with_color(role, color);
                self.replace(
                    self.config.timer().to_owned(),
                    *self.config.tasks(),
                    theme,
                    self.config.keys().clone(),
                );
            }
            Err(error) => self.error = Some(error),
        }
    }

    fn replace(
        &mut self,
        timer: TimerConfig,
        tasks: TasksConfig,
        theme: crate::config::ThemeConfig,
        keys: crate::config::KeysConfig,
    ) {
        match Config::with_all_settings(
            timer,
            tasks,
            theme,
            keys,
            self.config.notification(),
            self.config.sound().clone(),
        ) {
            Ok(config) => {
                self.config = config;
                self.error = None;
            }
            Err(error) => self.error = Some(error.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{SoundConfig, ThemeColor, ThemeConfig, ThemeRole};

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
    fn duration_edits_require_mm_ss_and_reject_invalid_seconds() {
        let mut settings = SettingsOverlay::new(&Config::default());

        for invalid in ["5:30", "05:60", "00:00", "05"] {
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
        settings.adjust(true);
        select(&mut settings, SettingField::AutostartBreaks);
        settings.adjust(true);
        select(&mut settings, SettingField::AutostartFocus);
        settings.activate();
        select(&mut settings, SettingField::Theme(ThemeRole::FocusedBorder));
        settings.adjust(true);

        assert!(!settings.config().tasks().persist());
        assert!(settings.config().timer().autostart_breaks());
        assert!(settings.config().timer().autostart_focus());
        assert_eq!(
            settings.config().theme().color(ThemeRole::FocusedBorder),
            original_border.cycle(true)
        );
        assert!(original.tasks().persist());
        assert_eq!(
            original.theme().color(ThemeRole::FocusedBorder),
            original_border
        );

        select(&mut settings, SettingField::Theme(ThemeRole::Focus));
        settings.adjust(true);
        assert_eq!(
            settings.config().theme().focus(),
            original_focus.cycle(true)
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

        settings.adjust(true);

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
        settings.move_selection(true);
        assert_eq!(settings.selection(), 0);
        assert!(settings.cancel_nested());
    }
}
