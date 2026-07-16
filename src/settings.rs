use crate::config::{
    Config, ConfigKey, ConfigValidationError, KeyAction, TasksConfig, ThemeRole, TimerConfig,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SettingField {
    FocusMinutes,
    ShortBreakMinutes,
    LongBreakMinutes,
    LongBreakInterval,
    PersistTasks,
    ShowTaskNumbers,
    Theme(ThemeRole),
    Key(KeyAction),
}

impl SettingField {
    pub(crate) const ALL: [Self; 25] = [
        Self::FocusMinutes,
        Self::ShortBreakMinutes,
        Self::LongBreakMinutes,
        Self::LongBreakInterval,
        Self::PersistTasks,
        Self::ShowTaskNumbers,
        Self::Theme(ThemeRole::FocusedBorder),
        Self::Theme(ThemeRole::UnfocusedBorder),
        Self::Theme(ThemeRole::TodoHighlight),
        Self::Theme(ThemeRole::DoneHighlight),
        Self::Theme(ThemeRole::CompletedSessions),
        Self::Key(KeyAction::FocusLeft),
        Self::Key(KeyAction::FocusDown),
        Self::Key(KeyAction::FocusUp),
        Self::Key(KeyAction::FocusRight),
        Self::Key(KeyAction::ListDown),
        Self::Key(KeyAction::ListUp),
        Self::Key(KeyAction::Quit),
        Self::Key(KeyAction::ClockPrimary),
        Self::Key(KeyAction::CycleSession),
        Self::Key(KeyAction::ResetSession),
        Self::Key(KeyAction::AddTask),
        Self::Key(KeyAction::EditTask),
        Self::Key(KeyAction::DeleteTask),
        Self::Key(KeyAction::TaskPrimary),
    ];

    fn is_number(self) -> bool {
        matches!(
            self,
            Self::FocusMinutes
                | Self::ShortBreakMinutes
                | Self::LongBreakMinutes
                | Self::LongBreakInterval
        )
    }
}

#[derive(Debug, Clone)]
pub(crate) struct SettingsOverlay {
    draft: Config,
    selection: usize,
    input: Option<String>,
    capturing_key: bool,
    error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SettingsResult {
    Open,
    Save(Box<Config>),
    Cancel,
}

impl SettingsOverlay {
    pub(crate) fn new(config: &Config) -> Self {
        Self {
            draft: config.clone(),
            selection: 0,
            input: None,
            capturing_key: false,
            error: None,
        }
    }

    pub(crate) fn draft(&self) -> &Config {
        &self.draft
    }

    pub(crate) fn selection(&self) -> usize {
        self.selection
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
            SettingField::PersistTasks => self.set_tasks(
                !self.draft.tasks().persist(),
                self.draft.tasks().show_numbers(),
            ),
            SettingField::ShowTaskNumbers => self.set_tasks(
                self.draft.tasks().persist(),
                !self.draft.tasks().show_numbers(),
            ),
            SettingField::Theme(role) => {
                let theme = *self.draft.theme();
                let color = theme.color(role).cycle(forward);
                self.replace(
                    self.draft.timer().to_owned(),
                    *self.draft.tasks(),
                    theme.with_color(role, color),
                    self.draft.keys().clone(),
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
            _ => {}
        }
    }

    pub(crate) fn activate(&mut self) -> SettingsResult {
        let field = self.field();
        if field.is_number() {
            self.input = Some(self.number_value(field).to_string());
            self.error = None;
        } else {
            match field {
                SettingField::PersistTasks
                | SettingField::ShowTaskNumbers
                | SettingField::Theme(_) => self.adjust(true),
                SettingField::Key(_) => {
                    self.capturing_key = true;
                    self.error = None;
                }
                _ => {}
            }
        }
        SettingsResult::Open
    }

    pub(crate) fn save(&mut self) -> SettingsResult {
        if self.input.is_some() {
            self.submit_input();
        }
        if self.error.is_some() {
            return SettingsResult::Open;
        }
        SettingsResult::Save(Box::new(self.draft.clone()))
    }

    pub(crate) fn push_digit(&mut self, digit: char) {
        if digit.is_ascii_digit()
            && let Some(input) = &mut self.input
        {
            input.push(digit);
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
        self.set_number(self.field(), value);
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
        let keys = self.draft.keys().clone().with_binding(action, key);
        match Config::with_all_settings(
            self.draft.timer().to_owned(),
            *self.draft.tasks(),
            *self.draft.theme(),
            keys,
            self.draft.sound().clone(),
        ) {
            Ok(config) => {
                self.draft = config;
                self.capturing_key = false;
                self.error = None;
            }
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    fn number_value(&self, field: SettingField) -> u64 {
        match field {
            SettingField::FocusMinutes => self.draft.timer().focus_minutes(),
            SettingField::ShortBreakMinutes => self.draft.timer().short_break_minutes(),
            SettingField::LongBreakMinutes => self.draft.timer().long_break_minutes(),
            SettingField::LongBreakInterval => {
                u64::from(self.draft.timer().long_break_interval().get())
            }
            _ => 0,
        }
    }

    fn set_number(&mut self, field: SettingField, value: String) {
        let parsed = value
            .parse::<u64>()
            .map_err(|_| ConfigValidationError::ZeroDuration { field: "setting" });
        let result = parsed.and_then(|value| {
            let timer = self.draft.timer();
            let (focus, short, long, interval) = match field {
                SettingField::FocusMinutes => (
                    value,
                    timer.short_break_minutes(),
                    timer.long_break_minutes(),
                    u64::from(timer.long_break_interval().get()),
                ),
                SettingField::ShortBreakMinutes => (
                    timer.focus_minutes(),
                    value,
                    timer.long_break_minutes(),
                    u64::from(timer.long_break_interval().get()),
                ),
                SettingField::LongBreakMinutes => (
                    timer.focus_minutes(),
                    timer.short_break_minutes(),
                    value,
                    u64::from(timer.long_break_interval().get()),
                ),
                SettingField::LongBreakInterval => (
                    timer.focus_minutes(),
                    timer.short_break_minutes(),
                    timer.long_break_minutes(),
                    value,
                ),
                _ => return Ok(self.draft.timer().to_owned()),
            };
            let interval =
                u32::try_from(interval).map_err(|_| ConfigValidationError::DurationOverflow {
                    field: "long_break_interval",
                })?;
            TimerConfig::new(focus, short, long, interval)
        });
        match result {
            Ok(timer) => self.replace(
                timer,
                *self.draft.tasks(),
                *self.draft.theme(),
                self.draft.keys().clone(),
            ),
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    fn set_tasks(&mut self, persist: bool, show_numbers: bool) {
        self.replace(
            self.draft.timer().to_owned(),
            TasksConfig::with_numbering(persist, show_numbers),
            *self.draft.theme(),
            self.draft.keys().clone(),
        );
    }

    fn replace(
        &mut self,
        timer: TimerConfig,
        tasks: TasksConfig,
        theme: crate::config::ThemeConfig,
        keys: crate::config::KeysConfig,
    ) {
        match Config::with_all_settings(timer, tasks, theme, keys, self.draft.sound().clone()) {
            Ok(config) => {
                self.draft = config;
                self.error = None;
            }
            Err(error) => self.error = Some(error.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{SoundConfig, ThemeColor, ThemeRole};

    #[test]
    fn numeric_edits_are_validated_before_updating_the_draft() {
        let mut settings = SettingsOverlay::new(&Config::default());
        assert_eq!(settings.activate(), SettingsResult::Open);
        settings.pop_input();
        settings.pop_input();
        settings.submit_input();

        assert_eq!(settings.draft().timer().focus_minutes(), 25);
        assert!(settings.error().is_some());

        assert_eq!(settings.activate(), SettingsResult::Open);
        settings.pop_input();
        settings.pop_input();
        settings.push_digit('4');
        settings.push_digit('0');
        settings.submit_input();

        assert_eq!(settings.draft().timer().focus_minutes(), 40);
        assert!(settings.error().is_none());
    }

    #[test]
    fn editing_other_settings_preserves_the_completion_sound() {
        let sound_file = std::env::current_dir().unwrap().join("custom.wav");
        let config = Config::default()
            .with_sound(SoundConfig::new(&sound_file))
            .unwrap();
        let mut settings = SettingsOverlay::new(&config);

        settings.set_tasks(false, false);

        assert_eq!(settings.draft().sound().file(), Some(sound_file.as_path()));
    }

    #[test]
    fn booleans_and_theme_colors_change_only_the_draft() {
        let original = Config::default();
        let mut settings = SettingsOverlay::new(&original);
        settings.select(4);
        settings.adjust(true);
        settings.select(6);
        settings.adjust(true);

        assert!(!settings.draft().tasks().persist());
        assert_eq!(
            settings.draft().theme().color(ThemeRole::FocusedBorder),
            ThemeColor::Blue
        );
        assert!(original.tasks().persist());
        assert_eq!(
            original.theme().color(ThemeRole::FocusedBorder),
            ThemeColor::Yellow
        );
    }

    #[test]
    fn key_capture_rejects_context_conflicts_and_accepts_valid_keys() {
        let mut settings = SettingsOverlay::new(&Config::default());
        settings.select(19);
        assert_eq!(settings.activate(), SettingsResult::Open);
        settings.capture_key(ConfigKey::Space);
        assert!(settings.is_capturing_key());
        assert!(settings.error().is_some());

        settings.capture_key(ConfigKey::Character('n'));
        assert!(!settings.is_capturing_key());
        assert_eq!(
            settings.draft().keys().binding(KeyAction::CycleSession),
            [ConfigKey::Character('n')]
        );
    }

    #[test]
    fn selection_is_clamped_and_locked_during_nested_editing() {
        let mut settings = SettingsOverlay::new(&Config::default());
        settings.select(usize::MAX);
        assert_eq!(settings.field(), SettingField::Key(KeyAction::TaskPrimary));
        settings.select(0);
        let _ = settings.activate();
        settings.move_selection(true);
        assert_eq!(settings.selection(), 0);
        assert!(settings.cancel_nested());
    }

    #[test]
    fn saving_commits_a_valid_numeric_edit() {
        let mut settings = SettingsOverlay::new(&Config::default());
        let _ = settings.activate();
        settings.pop_input();
        settings.pop_input();
        settings.push_digit('3');
        settings.push_digit('0');

        let SettingsResult::Save(config) = settings.save() else {
            panic!("valid settings were not saved")
        };
        assert_eq!(config.timer().focus_minutes(), 30);
    }
}
