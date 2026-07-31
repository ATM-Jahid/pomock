use crate::config::{KeyAction, ThemeRole};

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
    pub(super) const KEYS: [Self; 17] = [
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
    pub(super) const THEME: [Self; 7] = [
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

    pub(super) fn is_number(self) -> bool {
        matches!(self, Self::LongBreakInterval)
    }

    pub(super) fn is_duration(self) -> bool {
        matches!(
            self,
            Self::FocusDuration | Self::ShortBreakDuration | Self::LongBreakDuration
        )
    }

    pub(super) fn is_text(self) -> bool {
        self.is_number()
            || self.is_duration()
            || matches!(
                self,
                Self::CompletionSoundFile | Self::FocusSoundFile | Self::Theme(_)
            )
    }
}
