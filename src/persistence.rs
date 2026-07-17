use std::{
    error::Error,
    fmt, fs, io,
    path::{Path, PathBuf},
};

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

use crate::app::TaskState;

const TASKS_FILE_NAME: &str = "tasks.toml";
const TASK_FILE_VERSION: u32 = 1;

/// Filesystem boundary for durable task state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskStore {
    path: PathBuf,
}

impl TaskStore {
    /// Uses the platform-appropriate per-user application data path.
    pub fn user() -> Result<Self, TaskPersistenceError> {
        let path = ProjectDirs::from("", "", "pomock")
            .map(|dirs| dirs.data_local_dir().join(TASKS_FILE_NAME))
            .ok_or(TaskPersistenceError::DirectoryUnavailable)?;
        Ok(Self { path })
    }

    /// Uses an explicit file path, primarily for embedding and tests.
    pub fn at(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

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
        fs::write(&self.path, contents).map_err(|source| TaskPersistenceError::Write {
            path: self.path.clone(),
            source,
        })
    }

    /// Returns the backing path used by this store.
    pub fn path(&self) -> &Path {
        &self.path
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

#[derive(Debug)]
pub enum TaskPersistenceError {
    DirectoryUnavailable,
    Read {
        path: PathBuf,
        source: io::Error,
    },
    Parse {
        path: PathBuf,
        source: toml::de::Error,
    },
    Validation {
        path: PathBuf,
        list: &'static str,
        index: usize,
    },
    UnsupportedVersion {
        path: PathBuf,
        found: u32,
    },
    CreateDirectory {
        path: PathBuf,
        source: io::Error,
    },
    Serialize(toml::ser::Error),
    Write {
        path: PathBuf,
        source: io::Error,
    },
}

impl fmt::Display for TaskPersistenceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DirectoryUnavailable => {
                formatter.write_str("could not determine the user application data directory")
            }
            Self::Read { path, source } => {
                write!(
                    formatter,
                    "could not read tasks from {}: {source}",
                    path.display()
                )
            }
            Self::Parse { path, source } => {
                write!(
                    formatter,
                    "could not parse tasks in {}: {source}",
                    path.display()
                )
            }
            Self::Validation { path, list, index } => write!(
                formatter,
                "invalid {list} task {} in {}: description must not be blank",
                index + 1,
                path.display()
            ),
            Self::UnsupportedVersion { path, found } => write!(
                formatter,
                "unsupported task data version {found} in {}; expected version {TASK_FILE_VERSION}",
                path.display()
            ),
            Self::CreateDirectory { path, source } => write!(
                formatter,
                "could not create task data directory {}: {source}",
                path.display()
            ),
            Self::Serialize(source) => write!(formatter, "could not serialize tasks: {source}"),
            Self::Write { path, source } => {
                write!(
                    formatter,
                    "could not write tasks to {}: {source}",
                    path.display()
                )
            }
        }
    }
}

impl Error for TaskPersistenceError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::DirectoryUnavailable
            | Self::Validation { .. }
            | Self::UnsupportedVersion { .. } => None,
            Self::Read { source, .. }
            | Self::CreateDirectory { source, .. }
            | Self::Write { source, .. } => Some(source),
            Self::Parse { source, .. } => Some(source),
            Self::Serialize(source) => Some(source),
        }
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

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::PathBuf,
        sync::atomic::{AtomicU64, Ordering},
    };

    use super::{TaskPersistenceError, TaskStore};
    use crate::app::TaskState;

    static NEXT_TEMP_PATH: AtomicU64 = AtomicU64::new(0);

    fn temp_path(name: &str) -> PathBuf {
        let unique = NEXT_TEMP_PATH.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "pomock-task-test-{}-{unique}-{name}",
            std::process::id()
        ))
    }

    #[test]
    fn missing_file_loads_an_empty_task_state() {
        let store = TaskStore::at(temp_path("missing.toml"));

        assert_eq!(store.load().unwrap(), TaskState::default());
    }

    #[test]
    fn independently_ordered_lists_round_trip_without_completion_flags() {
        let path = temp_path("round-trip/tasks.toml");
        let store = TaskStore::at(&path);
        let state = TaskState::from_lists(
            vec!["First todo".to_owned(), "Second todo".to_owned()],
            vec!["First done".to_owned()],
        );

        store.save(&state).unwrap();

        assert_eq!(store.load().unwrap(), state);
        let contents = fs::read_to_string(&path).unwrap();
        assert!(contents.contains("version = 1"));
        assert!(contents.contains("todo = ["));
        assert!(contents.contains("done = ["));
        assert!(!contents.contains("completed"));
        fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn malformed_toml_reports_the_path_and_parse_error() {
        let path = temp_path("malformed.toml");
        fs::write(&path, "version = 1\ntodo = ['broken'\ndone = []").unwrap();
        let store = TaskStore::at(&path);

        let error = store.load().unwrap_err();

        assert!(matches!(error, TaskPersistenceError::Parse { .. }));
        assert!(error.to_string().contains(path.to_str().unwrap()));
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn blank_descriptions_report_the_task_number_and_path() {
        let path = temp_path("blank.toml");
        fs::write(&path, "version = 1\ntodo = ['valid', '  ']\ndone = []\n").unwrap();
        let store = TaskStore::at(&path);

        let error = store.load().unwrap_err();

        assert!(matches!(
            error,
            TaskPersistenceError::Validation { index: 1, .. }
        ));
        assert!(error.to_string().contains("task 2"));
        assert!(error.to_string().contains(path.to_str().unwrap()));
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn unsupported_versions_report_the_found_version_and_path() {
        let path = temp_path("future-version.toml");
        fs::write(&path, "version = 2\ntodo = []\ndone = []\n").unwrap();
        let store = TaskStore::at(&path);

        let error = store.load().unwrap_err();

        assert!(matches!(
            error,
            TaskPersistenceError::UnsupportedVersion { found: 2, .. }
        ));
        assert!(error.to_string().contains(path.to_str().unwrap()));
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn read_errors_include_the_task_path() {
        let path = temp_path("directory-instead-of-file");
        fs::create_dir(&path).unwrap();
        let store = TaskStore::at(&path);

        let error = store.load().unwrap_err();

        assert!(matches!(error, TaskPersistenceError::Read { .. }));
        assert!(error.to_string().contains(path.to_str().unwrap()));
        fs::remove_dir(path).unwrap();
    }

    #[test]
    fn save_errors_include_the_failed_directory() {
        let parent = temp_path("parent-is-file");
        fs::write(&parent, "not a directory").unwrap();
        let store = TaskStore::at(parent.join("tasks.toml"));

        let error = store.save(&TaskState::default()).unwrap_err();

        assert!(matches!(
            error,
            TaskPersistenceError::CreateDirectory { .. }
        ));
        assert!(error.to_string().contains(parent.to_str().unwrap()));
        fs::remove_file(parent).unwrap();
    }
}
