use crate::{
    SessionKind,
    config::{Config, ConfigKey},
};

/// A semantic navigation direction, independent of its physical key mapping.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Left,
    Down,
    Up,
    Right,
}

/// A user intention after terminal input has been translated from a physical key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Quit,
    NavigateFocus(Direction),
    MoveSelection(Direction),
    MoveSelectedTask(Direction),
    PrimaryAction,
    CycleSession,
    ResetSession,
    ConfirmPendingAction,
    CancelPendingAction,
    BeginAdd,
    EditSelected,
    DeleteSelected,
    SubmitEdit,
    CancelEdit,
    PushInput(char),
    PopInput,
    OpenSettings,
    SettingsMove(bool),
    SettingsAdjust(bool),
    SettingsActivate,
    SettingsClose,
    SettingsCancel,
    SettingsPushInput(char),
    SettingsPopInput,
    SettingsSubmitInput,
    SettingsCaptureKey(ConfigKey),
    Scroll(ScrollTarget, Direction),
}

/// A list that can be scrolled by a pointing device.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollTarget {
    Todo,
    Done,
    Settings,
}

/// A boundary-relevant result of applying an application transition.
#[must_use]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppOutcome {
    None,
    Quit,
    FocusAudio(FocusAudioAction),
    TimerEffects {
        focus_audio: Option<FocusAudioAction>,
        stop_completion_audio: bool,
    },
    SessionCompleted(SessionKind),
    TasksChanged,
    SettingsChanged(Box<Config>),
}

/// A lifecycle operation for the optional looping Focus audio.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusAudioAction {
    StartOrResume,
    Pause,
    Stop,
}
