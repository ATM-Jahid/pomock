use std::{io, path::Path};

use directories::ProjectDirs;

use std::fs::{self, File, OpenOptions, TryLockError};

use super::{ConfigStore, TaskPersistenceError, TaskStore, validate_workspace_name};

const WORKSPACE_LOCK_FILE_NAME: &str = ".workspace.lock";

impl TaskStore {
    /// Acquires the exclusive, process-lifetime writer lock for this workspace.
    pub fn lock_workspace(&self) -> Result<WorkspaceLock, TaskPersistenceError> {
        let workspace_directory = self
            .path
            .parent()
            .ok_or(TaskPersistenceError::DirectoryUnavailable)?;
        fs::create_dir_all(workspace_directory).map_err(|source| {
            TaskPersistenceError::CreateDirectory {
                path: workspace_directory.to_owned(),
                source,
            }
        })?;

        let path = workspace_directory.join(WORKSPACE_LOCK_FILE_NAME);
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)
            .map_err(|source| TaskPersistenceError::OpenLock {
                path: path.clone(),
                source,
            })?;

        match file.try_lock() {
            Ok(()) => Ok(WorkspaceLock { file }),
            Err(TryLockError::WouldBlock) => Err(TaskPersistenceError::WorkspaceAlreadyOpen {
                path: self.path.clone(),
            }),
            Err(TryLockError::Error(source)) => Err(TaskPersistenceError::Lock { path, source }),
        }
    }
}

/// The exclusive writer lease for one task workspace.
#[derive(Debug)]
pub struct WorkspaceLock {
    file: File,
}

impl Drop for WorkspaceLock {
    fn drop(&mut self) {
        let _ = self.file.unlock();
    }
}

/// Lists workspace subdirectories with valid configuration files in alphabetical order.
/// Inspects the configuration directory without creating files or taking locks.
pub fn list_workspaces() -> Result<Vec<String>, TaskPersistenceError> {
    let dirs =
        ProjectDirs::from("", "", "pomock").ok_or(TaskPersistenceError::DirectoryUnavailable)?;
    list_workspaces_in(dirs.config_dir())
}

fn list_workspaces_in(config_dir: &Path) -> Result<Vec<String>, TaskPersistenceError> {
    let mut names = Vec::new();
    let read_error = |source| TaskPersistenceError::ReadDirectory {
        path: config_dir.to_path_buf(),
        source,
    };
    let entries = match fs::read_dir(config_dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(names),
        Err(error) => return Err(read_error(error)),
    };
    for entry in entries {
        let entry = entry.map_err(read_error)?;
        let Ok(name) = entry.file_name().into_string() else {
            continue;
        };
        if validate_workspace_name(&name).is_err()
            || !entry.file_type().map_err(read_error)?.is_dir()
        {
            continue;
        }
        let path = entry.path().join("config.toml");
        let Ok(contents) = fs::read_to_string(&path) else {
            continue;
        };
        if ConfigStore::parse_contents(&path, &contents).is_ok() {
            names.push(name);
        }
    }
    names.sort();
    Ok(names)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn listing_requires_valid_configs_and_sorts_names() {
        let directory = tempfile::tempdir().unwrap();
        let config_dir = directory.path();
        for (name, contents) in [
            ("zebra", ""),
            ("alpha", "[timer]\nfocus_duration = '42:00'\n"),
            ("main", "[timer]\nlong_break_interval = 0\n"),
            ("broken", "[timer"),
            ("unknown", "unknown_setting = true"),
            ("two words", ""),
        ] {
            fs::create_dir(config_dir.join(name)).unwrap();
            fs::write(config_dir.join(name).join("config.toml"), contents).unwrap();
        }
        fs::create_dir(config_dir.join("missing")).unwrap();
        fs::create_dir_all(config_dir.join("nested/child")).unwrap();
        fs::write(config_dir.join("nested/child/config.toml"), "").unwrap();
        fs::create_dir_all(config_dir.join("directory/config.toml")).unwrap();
        fs::write(config_dir.join("config.toml"), "").unwrap();
        assert_eq!(list_workspaces_in(config_dir).unwrap(), ["alpha", "zebra"]);
        // Main follows the same validation rules as every other workspace.
        fs::write(config_dir.join("main/config.toml"), "").unwrap();
        assert_eq!(
            list_workspaces_in(config_dir).unwrap(),
            ["alpha", "main", "zebra"]
        );
    }

    #[test]
    fn missing_config_dir_is_empty_without_creating_directories() {
        let directory = tempfile::tempdir().unwrap();
        let missing = directory.path().join("missing");
        assert!(list_workspaces_in(&missing).unwrap().is_empty());
        assert!(!missing.exists());
    }

    #[test]
    fn listing_reports_unreadable_config_dir() {
        let file = tempfile::NamedTempFile::new().unwrap();
        assert!(matches!(
            list_workspaces_in(file.path()),
            Err(TaskPersistenceError::ReadDirectory { .. })
        ));
    }
}
