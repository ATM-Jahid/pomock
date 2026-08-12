use std::{
    error::Error,
    fmt, io,
    path::{Path, PathBuf},
};

use directories::ProjectDirs;

mod tasks;
mod workspace;

pub use workspace::WorkspaceLock;

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
        Self::user_in_workspace(None)
    }

    /// Uses the per-user task file for an optional named workspace.
    pub fn user_in_workspace(workspace: Option<&str>) -> Result<Self, TaskPersistenceError> {
        let path = ProjectDirs::from("", "", "pomock")
            .map(|dirs| {
                let directory = workspace.map_or_else(
                    || dirs.data_local_dir().to_owned(),
                    |name| dirs.data_local_dir().join(name),
                );
                directory.join(TASKS_FILE_NAME)
            })
            .ok_or(TaskPersistenceError::DirectoryUnavailable)?;
        Ok(Self { path })
    }

    /// Uses an explicit file path, primarily for embedding and tests.
    pub fn at(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// Returns the backing path used by this store.
    pub fn path(&self) -> &Path {
        &self.path
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
    ReadDirectory {
        path: PathBuf,
        source: io::Error,
    },
    OpenLock {
        path: PathBuf,
        source: io::Error,
    },
    Lock {
        path: PathBuf,
        source: io::Error,
    },
    WorkspaceAlreadyOpen {
        path: PathBuf,
    },
    Backup {
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
            Self::ReadDirectory { path, source } => write!(
                formatter,
                "could not inspect workspace instances in {}: {source}",
                path.display()
            ),
            Self::OpenLock { path, source } => write!(
                formatter,
                "could not open workspace instance lock {}: {source}",
                path.display()
            ),
            Self::Lock { path, source } => write!(
                formatter,
                "could not lock workspace instance file {}: {source}",
                path.display()
            ),
            Self::WorkspaceAlreadyOpen { path } => write!(
                formatter,
                "workspace at {} is already open in another pomock process",
                path.parent().unwrap_or(path).display()
            ),
            Self::Backup { path, source } => write!(
                formatter,
                "could not back up task data file {}: {source}",
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
            | Self::UnsupportedVersion { .. }
            | Self::WorkspaceAlreadyOpen { .. } => None,
            Self::Read { source, .. }
            | Self::CreateDirectory { source, .. }
            | Self::ReadDirectory { source, .. }
            | Self::OpenLock { source, .. }
            | Self::Lock { source, .. }
            | Self::Backup { source, .. }
            | Self::Write { source, .. } => Some(source),
            Self::Parse { source, .. } => Some(source),
            Self::Serialize(source) => Some(source),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        env, fs,
        io::{BufRead, BufReader, Write},
        path::PathBuf,
        process::{Command, Stdio},
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
    fn named_workspace_uses_a_child_of_the_default_data_directory() {
        let default_store = TaskStore::user().unwrap();
        let named_store = TaskStore::user_in_workspace(Some("client-one")).unwrap();

        assert_eq!(
            named_store.path(),
            default_store
                .path()
                .parent()
                .unwrap()
                .join("client-one")
                .join("tasks.toml")
        );
    }

    #[test]
    fn workspace_lock_excludes_a_second_writer_until_released() {
        let path = temp_path("instance-detection/tasks.toml");
        let store = TaskStore::at(&path);

        let first = store.lock_workspace().unwrap();
        assert!(matches!(
            store.lock_workspace(),
            Err(TaskPersistenceError::WorkspaceAlreadyOpen { .. })
        ));
        drop(first);

        let after_release = store.lock_workspace().unwrap();
        drop(after_release);
        fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn workspace_lock_excludes_a_writer_in_a_separate_process() {
        let path = temp_path("separate-process/tasks.toml");
        let mut child = Command::new(env::current_exe().unwrap())
            .args([
                "--exact",
                "persistence::tests::workspace_lock_subprocess_helper",
                "--nocapture",
            ])
            .env("POMOCK_TEST_WORKSPACE_LOCK_PATH", &path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();

        let mut output = BufReader::new(child.stdout.take().unwrap());
        let mut line = String::new();
        loop {
            line.clear();
            assert_ne!(output.read_line(&mut line).unwrap(), 0);
            if line.contains("WORKSPACE_LOCKED") {
                break;
            }
        }

        assert!(matches!(
            TaskStore::at(&path).lock_workspace(),
            Err(TaskPersistenceError::WorkspaceAlreadyOpen { .. })
        ));

        writeln!(child.stdin.take().unwrap()).unwrap();
        assert!(child.wait().unwrap().success());
        fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn workspace_lock_subprocess_helper() {
        let Some(path) = env::var_os("POMOCK_TEST_WORKSPACE_LOCK_PATH") else {
            return;
        };
        let _lock = TaskStore::at(path).lock_workspace().unwrap();
        println!("WORKSPACE_LOCKED");
        std::io::stdout().flush().unwrap();
        let mut release = String::new();
        std::io::stdin().read_line(&mut release).unwrap();
    }

    #[test]
    fn different_workspaces_can_be_locked_together() {
        let first_path = temp_path("separate-one/tasks.toml");
        let second_path = temp_path("separate-two/tasks.toml");
        let first = TaskStore::at(&first_path).lock_workspace().unwrap();
        let second = TaskStore::at(&second_path).lock_workspace().unwrap();
        drop(first);
        drop(second);
        fs::remove_dir_all(first_path.parent().unwrap()).unwrap();
        fs::remove_dir_all(second_path.parent().unwrap()).unwrap();
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
    fn incomplete_task_file_uses_defaults_without_rewriting() {
        let path = temp_path("incomplete.toml");
        let original = "version = 1\ntodo = [\"remember me\"]\n";
        fs::write(&path, original).unwrap();

        let store = TaskStore::at(&path);
        let state = store.load().unwrap();

        assert_eq!(state.todo().collect::<Vec<_>>(), vec!["remember me"]);
        assert_eq!(state.done().count(), 0);
        assert_eq!(fs::read_to_string(&path).unwrap(), original);
        let prefix = format!("{}.backup-", path.file_name().unwrap().to_string_lossy());
        assert!(
            fs::read_dir(path.parent().unwrap())
                .unwrap()
                .filter_map(Result::ok)
                .map(|entry| entry.path())
                .all(|candidate| {
                    !candidate
                        .file_name()
                        .unwrap()
                        .to_string_lossy()
                        .starts_with(&prefix)
                })
        );
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn unknown_task_file_keys_are_invalid() {
        let path = temp_path("unknown-key.toml");
        let original = "version = 1\ntodo = []\ndone = []\nlegacy = true\n";
        fs::write(&path, original).unwrap();
        let store = TaskStore::at(&path);

        assert!(matches!(
            store.load(),
            Err(TaskPersistenceError::Parse { .. })
        ));
        assert_eq!(fs::read_to_string(&path).unwrap(), original);
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
