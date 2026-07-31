use std::path::{Path, PathBuf};

use crate::app::{SettingsAdjustmentDirection, SettingsMoveDirection};
use crate::config::{
    CompletionSoundConfig, Config, ConfigKey, ConfigValidationError, FocusSoundConfig,
    NotificationConfig, TasksConfig, ThemeRole, TimerConfig, format_duration, parse_duration,
};

#[cfg(test)]
use crate::config::KeyAction;

mod field;

pub(crate) use field::SettingField;

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

    pub(crate) fn move_selection(&mut self, direction: SettingsMoveDirection) {
        if self.input.is_some() || self.capturing_key {
            return;
        }
        match direction {
            SettingsMoveDirection::Down => {
                self.selection = (self.selection + 1).min(SettingField::ALL.len() - 1);
            }
            SettingsMoveDirection::Up => {
                self.selection = self.selection.saturating_sub(1);
            }
        }
        self.error = None;
    }

    pub(crate) fn adjust(&mut self, direction: SettingsAdjustmentDirection) {
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
                let color = match direction {
                    SettingsAdjustmentDirection::Forward => theme.color(role).cycle_forward(),
                    SettingsAdjustmentDirection::Backward => theme.color(role).cycle_backward(),
                };
                self.replace(
                    self.config.timer().to_owned(),
                    *self.config.tasks(),
                    theme.with_color(role, color),
                    self.config.keys().clone(),
                );
            }
            _ if field.is_number() => {
                let current = self.number_value(field);
                let next = match direction {
                    SettingsAdjustmentDirection::Forward => current.saturating_add(1),
                    SettingsAdjustmentDirection::Backward => current.saturating_sub(1).max(1),
                };
                self.set_number(field, next.to_string());
            }
            _ if field.is_duration() => {
                let current = self.duration_value(field).as_secs();
                let next = match direction {
                    SettingsAdjustmentDirection::Forward => current.saturating_add(60),
                    SettingsAdjustmentDirection::Backward => current.saturating_sub(60).max(1),
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
                | SettingField::ShowTaskNumbers => {
                    self.adjust(SettingsAdjustmentDirection::Forward)
                }
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

    fn set_number(&mut self, field: SettingField, value: String) {
        let field_name = match field {
            SettingField::LongBreakInterval => "long_break_interval",
            _ => return,
        };
        let parsed = value.parse::<u64>().map_err(|error| match error.kind() {
            std::num::IntErrorKind::PosOverflow => {
                ConfigValidationError::IntegerOverflow { field: field_name }
            }
            _ => ConfigValidationError::InvalidPositiveInteger { field: field_name },
        });
        let result = parsed.and_then(|value| {
            let interval = u32::try_from(value)
                .map_err(|_| ConfigValidationError::IntegerOverflow { field: field_name })?;
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
#[path = "tests.rs"]
mod tests;
