use std::{error::Error, fmt};

/// Reports a workspace name rejected by the directory-name rules.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceNameError {
    name: String,
}

impl fmt::Display for WorkspaceNameError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "invalid workspace name {:?}; use letters, numbers, '.', '-', or '_'",
            self.name
        )
    }
}

impl Error for WorkspaceNameError {}

/// Accepts nonempty ASCII letters, numbers, dots, hyphens, and underscores,
/// excluding the directory components `.` and `..`.
pub fn validate_workspace_name(name: &str) -> Result<(), WorkspaceNameError> {
    let valid = !name.is_empty()
        && name != "."
        && name != ".."
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'));
    valid.then_some(()).ok_or_else(|| WorkspaceNameError {
        name: name.to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::validate_workspace_name;
    use crate::persistence::{ConfigError, ConfigStore, TaskPersistenceError, TaskStore};
    use std::error::Error;

    #[test]
    fn storage_constructors_reject_invalid_workspace_names() {
        for name in [
            "",
            ".",
            "..",
            "../shared",
            "nested/workspace",
            "/tmp/workspace",
            "nested\\workspace",
            "C:\\workspace",
            "two words",
            "café",
            "bad\0name",
        ] {
            let validation = validate_workspace_name(name).unwrap_err();
            let config = ConfigStore::user_in_workspace(Some(name)).unwrap_err();
            let tasks = TaskStore::user_in_workspace(Some(name)).unwrap_err();
            assert!(
                matches!(config, ConfigError::InvalidWorkspaceName(_)),
                "{name:?}"
            );
            assert!(
                matches!(tasks, TaskPersistenceError::InvalidWorkspaceName(_)),
                "{name:?}"
            );
            for error in [&config as &dyn Error, &tasks as &dyn Error] {
                assert_eq!(error.to_string(), validation.to_string());
                assert_eq!(error.source().unwrap().to_string(), validation.to_string());
            }
        }
    }

    #[test]
    fn valid_workspace_names_stay_under_the_application_directories() {
        let dirs = directories::ProjectDirs::from("", "", "pomock").unwrap();
        for name in [
            "main",
            "client-one",
            "personal.2026",
            "Client_42",
            ".hidden",
            "...",
            "-",
            "_",
        ] {
            validate_workspace_name(name).unwrap();
            assert_eq!(
                ConfigStore::user_in_workspace(Some(name)).unwrap().path(),
                dirs.config_dir().join(name).join("config.toml")
            );
            assert_eq!(
                TaskStore::user_in_workspace(Some(name)).unwrap().path(),
                dirs.data_local_dir().join(name).join("tasks.toml")
            );
        }
    }
}
