use std::{
    error::Error,
    fmt, fs, io,
    path::{Path, PathBuf},
};

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

use super::{DEFAULT_WORKSPACE, WorkspaceNameError, validate_workspace_name};
use crate::{atomic_write, timer::TimerSnapshot};

/// Filesystem boundary for durable timer state in each workspace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimerStore {
    path: PathBuf,
}

impl TimerStore {
    pub fn user() -> Result<Self, TimerPersistenceError> {
        Self::user_in_workspace(None)
    }

    pub fn user_in_workspace(workspace: Option<&str>) -> Result<Self, TimerPersistenceError> {
        let workspace = workspace.unwrap_or(DEFAULT_WORKSPACE);
        validate_workspace_name(workspace).map_err(TimerPersistenceError::InvalidWorkspaceName)?;
        let dirs = ProjectDirs::from("", "", "pomock")
            .ok_or(TimerPersistenceError::DirectoryUnavailable)?;
        Ok(Self::at(
            dirs.data_local_dir().join(workspace).join("timer.toml"),
        ))
    }

    pub fn at(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Missing files have no saved session. Invalid files are never overwritten by loading.
    pub fn load(&self) -> Result<Option<TimerSnapshot>, TimerPersistenceError> {
        let contents = match fs::read_to_string(&self.path) {
            Ok(contents) => contents,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(self.io_error(error)),
        };
        let stored: StoredTimer =
            toml::from_str(&contents).map_err(|source| TimerPersistenceError::Parse {
                path: self.path.clone(),
                source,
            })?;
        if stored.version != 1 {
            return Err(TimerPersistenceError::UnsupportedVersion(stored.version));
        }
        if !stored.timer.is_valid() {
            return Err(TimerPersistenceError::InvalidState);
        }
        Ok(Some(stored.timer))
    }

    /// Atomically replaces only the timer file.
    pub fn save(&self, state: &TimerSnapshot) -> Result<(), TimerPersistenceError> {
        if !state.is_valid() {
            return Err(TimerPersistenceError::InvalidState);
        }
        let contents = toml::to_string_pretty(&StoredTimer {
            version: 1,
            timer: state.clone(),
        })
        .map_err(TimerPersistenceError::Serialize)?;
        if let Some(parent) = self
            .path
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
        {
            fs::create_dir_all(parent).map_err(|error| self.io_error(error))?;
        }
        atomic_write::write(&self.path, contents.as_bytes()).map_err(|error| self.io_error(error))
    }

    /// Discards stale state without parsing it; a missing file is already cleared.
    pub fn clear(&self) -> Result<(), TimerPersistenceError> {
        match fs::remove_file(&self.path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(self.io_error(error)),
        }
    }

    fn io_error(&self, source: io::Error) -> TimerPersistenceError {
        TimerPersistenceError::Io {
            path: self.path.clone(),
            source,
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredTimer {
    version: u32,
    timer: TimerSnapshot,
}

#[derive(Debug)]
pub enum TimerPersistenceError {
    InvalidWorkspaceName(WorkspaceNameError),
    DirectoryUnavailable,
    Io {
        path: PathBuf,
        source: io::Error,
    },
    Parse {
        path: PathBuf,
        source: toml::de::Error,
    },
    Serialize(toml::ser::Error),
    UnsupportedVersion(u32),
    InvalidState,
}

impl fmt::Display for TimerPersistenceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidWorkspaceName(error) => error.fmt(formatter),
            Self::DirectoryUnavailable => {
                formatter.write_str("could not locate the timer data directory")
            }
            Self::Io { path, source } => write!(
                formatter,
                "could not access timer file {}: {source}",
                path.display()
            ),
            Self::Parse { path, source } => write!(
                formatter,
                "could not parse timer file {}: {source}",
                path.display()
            ),
            Self::Serialize(error) => write!(formatter, "could not serialize timer state: {error}"),
            Self::UnsupportedVersion(version) => {
                write!(formatter, "unsupported timer file version {version}")
            }
            Self::InvalidState => {
                formatter.write_str("invalid saved timer duration or remaining time")
            }
        }
    }
}

impl Error for TimerPersistenceError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvalidWorkspaceName(error) => Some(error),
            Self::Io { source, .. } => Some(source),
            Self::Parse { source, .. } => Some(source),
            Self::Serialize(error) => Some(error),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{config::Config, timer::PomodoroTimer};
    use std::time::Duration;

    #[test]
    fn default_and_named_workspaces_use_sibling_data_directories() {
        let main = TimerStore::user().unwrap();
        assert_eq!(main, TimerStore::user_in_workspace(Some("main")).unwrap());
        assert!(main.path().ends_with("main/timer.toml"));
        let other = TimerStore::user_in_workspace(Some("other")).unwrap();
        assert_eq!(
            other.path(),
            main.path()
                .parent()
                .unwrap()
                .parent()
                .unwrap()
                .join("other/timer.toml")
        );
        assert!(TimerStore::user_in_workspace(Some("../escape")).is_err());
    }

    #[test]
    fn clear_removes_even_invalid_state_and_accepts_missing_files() {
        let directory = tempfile::tempdir().unwrap();
        let store = TimerStore::at(directory.path().join("timer.toml"));
        store.clear().unwrap();
        fs::write(store.path(), "invalid = [").unwrap();
        store.clear().unwrap();
        assert_eq!(store.load().unwrap(), None);
        fs::create_dir(store.path()).unwrap();
        assert!(store.clear().is_err());
    }

    #[test]
    fn snapshots_round_trip() {
        let directory = tempfile::tempdir().unwrap();
        let store = TimerStore::at(directory.path().join("workspace/timer.toml"));
        assert_eq!(store.load().unwrap(), None);
        let config = Config::default();
        let settings = config.timer();
        let mut timer = PomodoroTimer::new(
            settings.focus_duration(),
            settings.short_break_duration(),
            settings.long_break_duration(),
            settings.long_break_interval(),
        );
        for step in 0..4 {
            match step {
                1 => timer.primary_action(),
                2 => {
                    timer.tick(Duration::from_millis(1234));
                    timer.pause();
                }
                3 => {
                    timer.resume();
                    timer.tick(timer.remaining());
                }
                _ => {}
            }
            store.save(&timer.snapshot()).unwrap();
            assert_eq!(store.load().unwrap(), Some(timer.snapshot()));
        }
        let contents = fs::read_to_string(store.path()).unwrap();
        fs::write(
            store.path(),
            contents.replace("version = 1", "version = 99"),
        )
        .unwrap();
        assert!(matches!(
            store.load(),
            Err(TimerPersistenceError::UnsupportedVersion(99))
        ));
        fs::write(store.path(), "broken = [").unwrap();
        assert!(matches!(
            store.load(),
            Err(TimerPersistenceError::Parse { .. })
        ));
        assert_eq!(fs::read_to_string(store.path()).unwrap(), "broken = [");
    }
}
