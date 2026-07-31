use std::{
    fs,
    fs::{File, OpenOptions, TryLockError},
    io,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use super::{TaskPersistenceError, TaskStore};

const INSTANCE_DIRECTORY_NAME: &str = ".instances";
const INSTANCE_FILE_PREFIX: &str = "instance-";
static NEXT_INSTANCE_FILE: AtomicU64 = AtomicU64::new(0);

impl TaskStore {
    /// Registers this process as an instance using this task location.
    pub fn register_instance(&self) -> Result<WorkspaceInstance, TaskPersistenceError> {
        let workspace_directory = self
            .path
            .parent()
            .ok_or(TaskPersistenceError::DirectoryUnavailable)?;
        let instance_directory = workspace_directory.join(INSTANCE_DIRECTORY_NAME);
        fs::create_dir_all(&instance_directory).map_err(|source| {
            TaskPersistenceError::CreateDirectory {
                path: instance_directory.clone(),
                source,
            }
        })?;

        let registry_path = instance_directory.join("registry.lock");
        let registry = open_lock_file(&registry_path)?;
        registry
            .lock()
            .map_err(|source| TaskPersistenceError::Lock {
                path: registry_path.clone(),
                source,
            })?;

        let already_open = find_live_instance(&instance_directory)?;
        let (path, file) = create_instance_file(&instance_directory)?;
        file.lock().map_err(|source| TaskPersistenceError::Lock {
            path: path.clone(),
            source,
        })?;
        registry
            .unlock()
            .map_err(|source| TaskPersistenceError::Lock {
                path: registry_path,
                source,
            })?;

        Ok(WorkspaceInstance {
            file,
            path,
            already_open,
        })
    }
}

/// A process-lifetime registration for one task workspace.
#[derive(Debug)]
pub struct WorkspaceInstance {
    file: File,
    path: PathBuf,
    already_open: bool,
}

impl WorkspaceInstance {
    /// Whether another process had already registered this workspace.
    pub fn already_open(&self) -> bool {
        self.already_open
    }
}

impl Drop for WorkspaceInstance {
    fn drop(&mut self) {
        let _ = self.file.unlock();
        let _ = fs::remove_file(&self.path);
    }
}

fn open_lock_file(path: &Path) -> Result<File, TaskPersistenceError> {
    OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)
        .map_err(|source| TaskPersistenceError::OpenLock {
            path: path.to_owned(),
            source,
        })
}

fn find_live_instance(directory: &Path) -> Result<bool, TaskPersistenceError> {
    let entries =
        fs::read_dir(directory).map_err(|source| TaskPersistenceError::ReadDirectory {
            path: directory.to_owned(),
            source,
        })?;
    let mut already_open = false;

    for entry in entries {
        let entry = entry.map_err(|source| TaskPersistenceError::ReadDirectory {
            path: directory.to_owned(),
            source,
        })?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        if !name.starts_with(INSTANCE_FILE_PREFIX) || !name.ends_with(".lock") {
            continue;
        }

        let path = entry.path();
        let file = open_lock_file(&path)?;
        match file.try_lock() {
            Ok(()) => {
                let _ = file.unlock();
                let _ = fs::remove_file(path);
            }
            Err(TryLockError::WouldBlock) => already_open = true,
            Err(TryLockError::Error(source)) => {
                return Err(TaskPersistenceError::Lock { path, source });
            }
        }
    }

    Ok(already_open)
}

fn create_instance_file(directory: &Path) -> Result<(PathBuf, File), TaskPersistenceError> {
    loop {
        let sequence = NEXT_INSTANCE_FILE.fetch_add(1, Ordering::Relaxed);
        let path = directory.join(format!(
            "{INSTANCE_FILE_PREFIX}{}-{sequence}.lock",
            std::process::id()
        ));
        match OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(file) => return Ok((path, file)),
            Err(source) if source.kind() == io::ErrorKind::AlreadyExists => {}
            Err(source) => return Err(TaskPersistenceError::OpenLock { path, source }),
        }
    }
}
