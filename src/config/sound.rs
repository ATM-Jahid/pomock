use std::path::{Path, PathBuf};

use directories::UserDirs;
use serde::{Deserialize, Serialize};

use super::ConfigValidationError;

/// File-backed session-completion sound settings.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SoundConfig {
    file: Option<PathBuf>,
    #[serde(skip)]
    resolved_file: Option<PathBuf>,
}

impl SoundConfig {
    /// Creates sound settings that play the selected file on completion.
    pub fn new(file: impl Into<PathBuf>) -> Self {
        Self {
            file: Some(file.into()),
            resolved_file: None,
        }
    }

    /// Returns the selected sound file, or `None` when sound is disabled.
    pub fn file(&self) -> Option<&Path> {
        self.file.as_deref()
    }

    /// Returns the absolute path used for playback after validation.
    pub fn resolved_file(&self) -> Option<&Path> {
        self.resolved_file.as_deref()
    }

    pub(super) fn validate(&mut self) -> Result<(), ConfigValidationError> {
        let Some(file) = &self.file else {
            self.resolved_file = None;
            return Ok(());
        };

        let resolved = if file.is_absolute() {
            file.clone()
        } else if let Some(remainder) = file.to_str().and_then(|file| file.strip_prefix("~/")) {
            let home = UserDirs::new()
                .ok_or(ConfigValidationError::HomeDirectoryUnavailable)?
                .home_dir()
                .to_owned();
            home.join(remainder)
        } else {
            return Err(ConfigValidationError::RelativeSoundPath { path: file.clone() });
        };
        self.resolved_file = Some(resolved);
        Ok(())
    }
}
