use std::path::{Path, PathBuf};

use directories::UserDirs;
use serde::{Deserialize, Serialize};

use super::ConfigValidationError;

/// Completion and Focus-loop sound settings.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SoundConfig {
    completion: CompletionSoundConfig,
    focus: FocusSoundConfig,
}

impl SoundConfig {
    pub fn with_completion(mut self, completion: CompletionSoundConfig) -> Self {
        self.completion = completion;
        self
    }

    pub fn with_focus(mut self, focus: FocusSoundConfig) -> Self {
        self.focus = focus;
        self
    }

    pub const fn completion(&self) -> &CompletionSoundConfig {
        &self.completion
    }

    pub const fn focus(&self) -> &FocusSoundConfig {
        &self.focus
    }

    pub(super) fn validate(&mut self) -> Result<(), ConfigValidationError> {
        self.completion.resolved_file =
            resolve(self.completion.file.as_deref(), "sound.completion.file")?;
        self.focus.resolved_file = resolve(self.focus.file.as_deref(), "sound.focus.file")?;
        Ok(())
    }
}

/// One-shot sound played when any session completes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct CompletionSoundConfig {
    enabled: bool,
    file: Option<PathBuf>,
    #[serde(skip)]
    resolved_file: Option<PathBuf>,
}

impl CompletionSoundConfig {
    pub fn new(enabled: bool, file: Option<PathBuf>) -> Self {
        Self {
            enabled,
            file,
            resolved_file: None,
        }
    }

    pub const fn enabled(&self) -> bool {
        self.enabled
    }

    pub fn file(&self) -> Option<&Path> {
        self.file.as_deref()
    }

    pub fn playback_file(&self) -> Option<&Path> {
        self.enabled
            .then_some(self.resolved_file.as_deref())
            .flatten()
    }
}

impl Default for CompletionSoundConfig {
    fn default() -> Self {
        Self::new(false, None)
    }
}

/// Sound loop active only while a Focus session is running.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct FocusSoundConfig {
    enabled: bool,
    file: Option<PathBuf>,
    #[serde(skip)]
    resolved_file: Option<PathBuf>,
}

impl FocusSoundConfig {
    pub fn new(enabled: bool, file: Option<PathBuf>) -> Self {
        Self {
            enabled,
            file,
            resolved_file: None,
        }
    }

    pub const fn enabled(&self) -> bool {
        self.enabled
    }

    pub fn file(&self) -> Option<&Path> {
        self.file.as_deref()
    }

    pub fn playback_file(&self) -> Option<&Path> {
        self.enabled
            .then_some(self.resolved_file.as_deref())
            .flatten()
    }
}

fn resolve(
    file: Option<&Path>,
    field: &'static str,
) -> Result<Option<PathBuf>, ConfigValidationError> {
    let Some(file) = file else {
        return Ok(None);
    };
    let resolved = if file.is_absolute() {
        file.to_owned()
    } else if let Some(remainder) = file.to_str().and_then(|file| file.strip_prefix("~/")) {
        let home = UserDirs::new()
            .ok_or(ConfigValidationError::HomeDirectoryUnavailable { field })?
            .home_dir()
            .to_owned();
        home.join(remainder)
    } else {
        return Err(ConfigValidationError::RelativeSoundPath {
            field,
            path: file.to_owned(),
        });
    };
    Ok(Some(resolved))
}
