use std::{fs, io, path::Path};

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

        let stored: StoredTaskFile =
            toml::from_str(&contents).map_err(|source| TaskPersistenceError::Parse {
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

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredTaskFile {
    version: u32,
    #[serde(default)]
    todo: Vec<String>,
    #[serde(default)]
    done: Vec<String>,
}
