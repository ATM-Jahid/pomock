use std::{
    fs, io,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use super::{TASK_FILE_VERSION, TaskPersistenceError, TaskStore};
use crate::{app::TaskState, atomic_write};

impl TaskStore {
    /// Loads task state, treating an absent file as an empty task list.
    pub fn load(&self) -> Result<TaskState, TaskPersistenceError> {
        let contents = match fs::read_to_string(&self.path) {
            Ok(contents) => contents,
            Err(source) if source.kind() == io::ErrorKind::NotFound => {
                return Ok(TaskState::default());
            }
            Err(source) => {
                return Err(TaskPersistenceError::Read {
                    path: self.path.clone(),
                    source,
                });
            }
        };

        let original: toml::Value =
            toml::from_str(&contents).map_err(|source| TaskPersistenceError::Parse {
                path: self.path.clone(),
                source,
            })?;
        let defaults = toml::Value::try_from(StoredTaskFile {
            version: TASK_FILE_VERSION,
            todo: Vec::new(),
            done: Vec::new(),
        })
        .expect("the default task file is serializable");
        let merged = merge_with_defaults(&original, &defaults);
        let stored: StoredTaskFile =
            merged
                .try_into()
                .map_err(|source| TaskPersistenceError::Parse {
                    path: self.path.clone(),
                    source,
                })?;

        if stored.version != TASK_FILE_VERSION {
            return Err(TaskPersistenceError::UnsupportedVersion {
                path: self.path.clone(),
                found: stored.version,
            });
        }

        Self::validate_list(&self.path, "todo", &stored.todo)?;
        Self::validate_list(&self.path, "done", &stored.done)?;
        Ok(TaskState::from_lists(stored.todo, stored.done))
    }

    /// Creates an empty task file if this store's path does not currently exist.
    ///
    /// Returns whether this call created the file.
    pub fn create_default_file(&self) -> Result<bool, TaskPersistenceError> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(|source| TaskPersistenceError::CreateDirectory {
                path: parent.to_owned(),
                source,
            })?;
        }
        let contents = toml::to_string_pretty(&StoredTaskFile {
            version: TASK_FILE_VERSION,
            todo: Vec::new(),
            done: Vec::new(),
        })
        .map_err(TaskPersistenceError::Serialize)?;
        atomic_write::write_new(&self.path, contents.as_bytes()).map_err(|source| {
            TaskPersistenceError::Write {
                path: self.path.clone(),
                source,
            }
        })
    }

    /// Replaces an invalid task file only if it still has the expected contents.
    ///
    /// Returns the backup path, or `None` when the file changed before replacement.
    pub fn replace_with_default_if_unchanged(
        &self,
        expected: &[u8],
    ) -> Result<Option<PathBuf>, TaskPersistenceError> {
        if fs::read(&self.path).map_err(|source| TaskPersistenceError::Read {
            path: self.path.clone(),
            source,
        })? != expected
        {
            return Ok(None);
        }

        let backup = atomic_write::backup(&self.path, expected).map_err(|source| {
            TaskPersistenceError::Backup {
                path: self.path.clone(),
                source,
            }
        })?;
        if fs::read(&self.path).map_err(|source| TaskPersistenceError::Read {
            path: self.path.clone(),
            source,
        })? != expected
        {
            return Ok(None);
        }

        self.save(&TaskState::default())?;
        Ok(Some(backup))
    }

    /// Creates a timestamped recovery copy beside the task data file.
    pub fn backup_file(&self) -> Result<PathBuf, TaskPersistenceError> {
        let contents = fs::read(&self.path).map_err(|source| TaskPersistenceError::Read {
            path: self.path.clone(),
            source,
        })?;
        atomic_write::backup(&self.path, &contents).map_err(|source| TaskPersistenceError::Backup {
            path: self.path.clone(),
            source,
        })
    }

    /// Saves task state, creating the parent application data directory.
    pub fn save(&self, state: &TaskState) -> Result<(), TaskPersistenceError> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(|source| TaskPersistenceError::CreateDirectory {
                path: parent.to_owned(),
                source,
            })?;
        }

        let stored = StoredTaskFile {
            version: TASK_FILE_VERSION,
            todo: state.todo().map(str::to_owned).collect(),
            done: state.done().map(str::to_owned).collect(),
        };
        let contents = toml::to_string_pretty(&stored).map_err(TaskPersistenceError::Serialize)?;
        atomic_write::write(&self.path, contents.as_bytes()).map_err(|source| {
            TaskPersistenceError::Write {
                path: self.path.clone(),
                source,
            }
        })
    }

    /// Backs up an existing task file, then replaces it with `state`.
    ///
    /// A missing task file is created without producing a backup. Returns the
    /// backup path when an existing file was preserved.
    pub fn replace_with_backup(
        &self,
        state: &TaskState,
    ) -> Result<Option<PathBuf>, TaskPersistenceError> {
        let backup = match fs::read(&self.path) {
            Ok(contents) => Some(atomic_write::backup(&self.path, &contents).map_err(
                |source| TaskPersistenceError::Backup {
                    path: self.path.clone(),
                    source,
                },
            )?),
            Err(source) if source.kind() == io::ErrorKind::NotFound => None,
            Err(source) => {
                return Err(TaskPersistenceError::Read {
                    path: self.path.clone(),
                    source,
                });
            }
        };

        self.save(state)?;
        Ok(backup)
    }

    fn validate_list(
        path: &Path,
        list: &'static str,
        descriptions: &[String],
    ) -> Result<(), TaskPersistenceError> {
        for (index, description) in descriptions.iter().enumerate() {
            Self::validate_description(path, list, index, description)?;
        }
        Ok(())
    }

    fn validate_description(
        path: &Path,
        list: &'static str,
        index: usize,
        description: &str,
    ) -> Result<(), TaskPersistenceError> {
        if description.trim().is_empty() {
            return Err(TaskPersistenceError::Validation {
                path: path.to_owned(),
                list,
                index,
            });
        }
        Ok(())
    }
}

fn merge_with_defaults(existing: &toml::Value, defaults: &toml::Value) -> toml::Value {
    let (Some(existing), Some(defaults)) = (existing.as_table(), defaults.as_table()) else {
        return existing.clone();
    };
    let mut merged = defaults.clone();
    for (key, value) in existing {
        let value = defaults.get(key).map_or_else(
            || value.clone(),
            |default| merge_with_defaults(value, default),
        );
        merged.insert(key.clone(), value);
    }
    toml::Value::Table(merged)
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredTaskFile {
    version: u32,
    todo: Vec<String>,
    done: Vec<String>,
}
