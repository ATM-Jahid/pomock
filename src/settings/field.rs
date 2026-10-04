use crate::config::{KeyAction, ThemeRole};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SettingField {
    FocusDuration,
    ShortBreakDuration,
    LongBreakDuration,
    LongBreakInterval,
    AutostartBreaks,
    AutostartFocus,
    PersistTimer,
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
    const TIMER: &[Self] = &[
        Self::FocusDuration,
        Self::ShortBreakDuration,
        Self::LongBreakDuration,
        Self::LongBreakInterval,
        Self::AutostartBreaks,
        Self::AutostartFocus,
        Self::PersistTimer,
    ];
    const NOTIFICATION: &[Self] = &[Self::NotificationEnabled];
    const SOUND: &[Self] = &[
        Self::CompletionSoundEnabled,
        Self::CompletionSoundFile,
        Self::FocusSoundEnabled,
        Self::FocusSoundFile,
    ];
    const TASKS: &[Self] = &[Self::PersistTasks, Self::ShowTaskNumbers];
    pub(super) const KEYS: &[Self] = &[
        Self::Key(KeyAction::Quit),
        Self::Key(KeyAction::Settings),
        Self::Key(KeyAction::FocusLeft),
        Self::Key(KeyAction::FocusDown),
        Self::Key(KeyAction::FocusUp),
        Self::Key(KeyAction::FocusRight),
        Self::Key(KeyAction::ClockPrimary),
        Self::Key(KeyAction::CycleSession),
        Self::Key(KeyAction::ResetSession),
        Self::Key(KeyAction::ClearTimerState),
        Self::Key(KeyAction::AddTask),
        Self::Key(KeyAction::EditTask),
        Self::Key(KeyAction::DeleteTask),
        Self::Key(KeyAction::TaskPrimary),
        Self::Key(KeyAction::ListDown),
        Self::Key(KeyAction::ListUp),
        Self::Key(KeyAction::MoveTaskUp),
        Self::Key(KeyAction::MoveTaskDown),
    ];
    pub(super) const THEME: &[Self] = &[
        Self::Theme(ThemeRole::FocusedBorder),
        Self::Theme(ThemeRole::UnfocusedBorder),
        Self::Theme(ThemeRole::Focus),
        Self::Theme(ThemeRole::ShortBreak),
        Self::Theme(ThemeRole::LongBreak),
        Self::Theme(ThemeRole::TodoHighlight),
        Self::Theme(ThemeRole::DoneHighlight),
    ];
    pub(crate) const GROUPS: &[(&'static str, &'static [Self])] = &[
        ("Timer", Self::TIMER),
        ("Notification", Self::NOTIFICATION),
        ("Sound", Self::SOUND),
        ("Tasks", Self::TASKS),
        ("Keys", Self::KEYS),
        ("Theme", Self::THEME),
    ];
    const FIELD_COUNT: usize = {
        let mut count = 0;
        let mut index = 0;
        while index < Self::GROUPS.len() {
            count += Self::GROUPS[index].1.len();
            index += 1;
        }
        count
    };
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
