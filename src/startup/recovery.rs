use std::{
    error::Error,
    fmt, fs,
    io::{self, BufRead, Write},
    path::Path,
};

use pomock::{
    app::TaskState,
    config::{Config, ConfigError},
    persistence::{TaskPersistenceError, TaskStore},
};

pub(crate) fn load_config_for_startup(
    reader: &mut impl BufRead,
    writer: &mut impl Write,
) -> Result<Option<Config>, StartupError> {
    let path = Config::path()?;
    load_config_path_for_startup(&path, reader, writer)
}

pub(crate) fn load_config_path_for_startup(
    path: &Path,
    reader: &mut impl BufRead,
    writer: &mut impl Write,
) -> Result<Option<Config>, StartupError> {
    loop {
        match Config::load_from(path) {
            Ok(config) => {
                if Config::create_default_file(path)? {
                    return Ok(Some(Config::default()));
                }
                return Ok(Some(config));
            }
            Err(error) if is_invalid_config(&error) => {
                let Some((error, contents)) = stable_invalid_config(path)? else {
                    continue;
                };
                if !confirm_backup_and_new_file(
                    reader,
                    writer,
                    "configuration",
                    "config",
                    path,
                    &error,
                )? {
                    return Ok(None);
                }
                let backup = match Config::replace_with_default_if_unchanged(path, &contents) {
                    Ok(Some(backup)) => backup,
                    Ok(None) => continue,
                    Err(ConfigError::Read { source, .. })
                        if source.kind() == io::ErrorKind::NotFound =>
                    {
                        continue;
                    }
                    Err(error) => return Err(error.into()),
                };
                writeln!(
                    writer,
                    "Backed up the invalid file to {}.",
                    backup.display()
                )?;
                return Ok(Some(Config::default()));
            }
            Err(error) => return Err(error.into()),
        }
    }
}

pub(crate) fn load_tasks_for_startup(
    task_store: Option<&TaskStore>,
    reader: &mut impl BufRead,
    writer: &mut impl Write,
) -> Result<Option<TaskState>, StartupError> {
    let Some(task_store) = task_store else {
        return Ok(Some(TaskState::default()));
    };

    loop {
        match task_store.load() {
            Ok(state) => {
                if task_store.create_default_file()? {
                    return Ok(Some(TaskState::default()));
                }
                return Ok(Some(state));
            }
            Err(error) if is_invalid_task_file(&error) => {
                let Some((error, contents)) = stable_invalid_task_file(task_store)? else {
                    continue;
                };
                if !confirm_backup_and_new_file(
                    reader,
                    writer,
                    "task data",
                    "task file",
                    task_store.path(),
                    &error,
                )? {
                    return Ok(None);
                }
                let backup = match task_store.replace_with_default_if_unchanged(&contents) {
                    Ok(Some(backup)) => backup,
                    Ok(None) => continue,
                    Err(TaskPersistenceError::Read { source, .. })
                        if source.kind() == io::ErrorKind::NotFound =>
                    {
                        continue;
                    }
                    Err(error) => return Err(error.into()),
                };
                writeln!(
                    writer,
                    "Backed up the invalid file to {}.",
                    backup.display()
                )?;
                return Ok(Some(TaskState::default()));
            }
            Err(error) => return Err(error.into()),
        }
    }
}

fn stable_invalid_config(path: &Path) -> Result<Option<(ConfigError, Vec<u8>)>, StartupError> {
    let contents = match fs::read(path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    let error = match Config::load_from(path) {
        Err(error) if is_invalid_config(&error) => error,
        _ => return Ok(None),
    };
    let unchanged = match fs::read(path) {
        Ok(current) => current == contents,
        Err(error) if error.kind() == io::ErrorKind::NotFound => false,
        Err(error) => return Err(error.into()),
    };
    Ok(unchanged.then_some((error, contents)))
}

fn stable_invalid_task_file(
    task_store: &TaskStore,
) -> Result<Option<(TaskPersistenceError, Vec<u8>)>, StartupError> {
    let contents = match fs::read(task_store.path()) {
        Ok(contents) => contents,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    let error = match task_store.load() {
        Err(error) if is_invalid_task_file(&error) => error,
        _ => return Ok(None),
    };
    let unchanged = match fs::read(task_store.path()) {
        Ok(current) => current == contents,
        Err(error) if error.kind() == io::ErrorKind::NotFound => false,
        Err(error) => return Err(error.into()),
    };
    Ok(unchanged.then_some((error, contents)))
}

fn confirm_backup_and_new_file(
    reader: &mut impl BufRead,
    writer: &mut impl Write,
    description: &str,
    new_file_description: &str,
    path: &Path,
    error: &impl fmt::Display,
) -> Result<bool, StartupError> {
    writeln!(
        writer,
        "pomock could not load the {description} file at {}:\n{error}",
        path.display(),
    )?;

    loop {
        write!(
            writer,
            "\n[b] Back up the invalid file, create a new {new_file_description}, and continue\n[q] Quit\nChoice: "
        )?;
        writer.flush()?;

        let mut choice = String::new();
        if reader.read_line(&mut choice)? == 0 {
            return Ok(false);
        }

        match choice.trim().to_ascii_lowercase().as_str() {
            "b" | "backup" => return Ok(true),
            "q" | "quit" => return Ok(false),
            _ => writeln!(
                writer,
                "Enter b to back up and create a new {new_file_description}, or q to quit."
            )?,
        }
    }
}

fn is_invalid_config(error: &ConfigError) -> bool {
    matches!(
        error,
        ConfigError::Parse { .. } | ConfigError::Validation { .. }
    )
}

fn is_invalid_task_file(error: &TaskPersistenceError) -> bool {
    matches!(
        error,
        TaskPersistenceError::Parse { .. }
            | TaskPersistenceError::Validation { .. }
            | TaskPersistenceError::UnsupportedVersion { .. }
    )
}

#[derive(Debug)]
pub(crate) enum StartupError {
    Config(ConfigError),
    TaskPersistence(TaskPersistenceError),
    Io(io::Error),
}

impl fmt::Display for StartupError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Config(error) => error.fmt(formatter),
            Self::TaskPersistence(error) => error.fmt(formatter),
            Self::Io(error) => error.fmt(formatter),
        }
    }
}

impl Error for StartupError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Config(error) => Some(error),
            Self::TaskPersistence(error) => Some(error),
            Self::Io(error) => Some(error),
        }
    }
}

impl From<ConfigError> for StartupError {
    fn from(error: ConfigError) -> Self {
        Self::Config(error)
    }
}

impl From<TaskPersistenceError> for StartupError {
    fn from(error: TaskPersistenceError) -> Self {
        Self::TaskPersistence(error)
    }
}

impl From<io::Error> for StartupError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}
