use serde::{Deserialize, Serialize};

/// Native desktop-notification settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct NotificationConfig {
    enabled: bool,
}

impl NotificationConfig {
    pub const fn new(enabled: bool) -> Self {
        Self { enabled }
    }

    pub const fn enabled(self) -> bool {
        self.enabled
    }
}

impl Default for NotificationConfig {
    fn default() -> Self {
        Self { enabled: true }
    }
}
