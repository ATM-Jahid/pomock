use std::{
    error::Error,
    fmt, fs,
    io::{self, BufRead, Write},
    path::{Path, PathBuf},
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
    match Config::load_from(path) {
        Ok(config) => Ok(Some(config)),
        Err(error) if is_invalid_config(&error) => {
            let recovered = recover_invalid_file(reader, writer, "configuration", path, &error)?;
            Ok(recovered.then_some(Config::default()))
        }
        Err(error) => Err(error.into()),
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

    match task_store.load() {
        Ok(state) => Ok(Some(state)),
        Err(error) if is_invalid_task_file(&error) => {
            let recovered =
                recover_invalid_file(reader, writer, "task data", task_store.path(), &error)?;
            Ok(recovered.then_some(TaskState::default()))
        }
        Err(error) => Err(error.into()),
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

fn recover_invalid_file(
    reader: &mut impl BufRead,
    writer: &mut impl Write,
    description: &str,
    path: &Path,
    error: &impl fmt::Display,
) -> Result<bool, StartupError> {
    writeln!(
        writer,
        "pomock could not load the {description} file at {}:\n{error}",
        path.display()
    )?;

    loop {
        write!(
            writer,
            "\n[d] Delete the invalid file and continue\n[q] Quit\nChoice: "
        )?;
        writer.flush()?;

        let mut choice = String::new();
        if reader.read_line(&mut choice)? == 0 {
            return Ok(false);
        }

        match choice.trim().to_ascii_lowercase().as_str() {
            "d" | "delete" => {
                match fs::remove_file(path) {
                    Ok(()) => {}
                    Err(source) if source.kind() == io::ErrorKind::NotFound => {}
                    Err(source) => {
                        return Err(StartupError::DeleteInvalidFile {
                            path: path.to_owned(),
                            source,
                        });
                    }
                }
                writeln!(writer, "Deleted {}.", path.display())?;
                return Ok(true);
            }
            "q" | "quit" => return Ok(false),
            _ => writeln!(writer, "Enter d to delete the file or q to quit.")?,
        }
    }
}

#[derive(Debug)]
pub(crate) enum StartupError {
    Config(ConfigError),
    TaskPersistence(TaskPersistenceError),
    Io(io::Error),
    DeleteInvalidFile { path: PathBuf, source: io::Error },
}

impl fmt::Display for StartupError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Config(error) => error.fmt(formatter),
            Self::TaskPersistence(error) => error.fmt(formatter),
            Self::Io(error) => error.fmt(formatter),
            Self::DeleteInvalidFile { path, source } => write!(
                formatter,
                "could not delete invalid file {}: {source}",
                path.display()
            ),
        }
    }
}

impl Error for StartupError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Config(error) => Some(error),
            Self::TaskPersistence(error) => Some(error),
            Self::Io(error) | Self::DeleteInvalidFile { source: error, .. } => Some(error),
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
