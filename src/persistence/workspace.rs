use std::fs::{self, File, OpenOptions, TryLockError};

use super::{TaskPersistenceError, TaskStore};

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
