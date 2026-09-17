use std::{
    error::Error,
    ffi::OsString,
    fmt,
    io::{self, Write},
};

use pomock::persistence::{WorkspaceNameError, validate_workspace_name};

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum CliCommand {
    Run { workspace: Option<String> },
    Help,
    Version,
}

impl CliCommand {
    pub(crate) fn parse(arguments: impl IntoIterator<Item = OsString>) -> Result<Self, CliError> {
        let mut arguments = arguments.into_iter();
        let mut workspace = None;

        while let Some(argument) = arguments.next() {
            let argument = argument
                .into_string()
                .map_err(|_| CliError::NonUnicodeArgument)?;
            match argument.as_str() {
                "-h" | "--help" => return Ok(Self::Help),
                "-v" | "--version" => return Ok(Self::Version),
                "-w" | "--workspace" => {
                    if workspace.is_some() {
                        return Err(CliError::DuplicateWorkspace);
                    }
                    let name = arguments.next().ok_or(CliError::MissingWorkspaceName)?;
                    let name = name
                        .into_string()
                        .map_err(|_| CliError::NonUnicodeArgument)?;
                    validate_workspace_name(&name).map_err(CliError::InvalidWorkspaceName)?;
                    workspace = Some(name);
                }
                _ if argument.starts_with("--workspace=") => {
                    if workspace.is_some() {
                        return Err(CliError::DuplicateWorkspace);
                    }
                    let name = argument.trim_start_matches("--workspace=");
                    validate_workspace_name(name).map_err(CliError::InvalidWorkspaceName)?;
                    workspace = Some(name.to_owned());
                }
                _ => return Err(CliError::UnexpectedArgument(argument)),
            }
        }

        Ok(Self::Run { workspace })
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum CliError {
    MissingWorkspaceName,
    DuplicateWorkspace,
    InvalidWorkspaceName(WorkspaceNameError),
    UnexpectedArgument(String),
    NonUnicodeArgument,
}

impl fmt::Display for CliError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingWorkspaceName => {
                formatter.write_str("--workspace requires a workspace name")
            }
            Self::DuplicateWorkspace => {
                formatter.write_str("--workspace may only be specified once")
            }
            Self::InvalidWorkspaceName(error) => error.fmt(formatter),
            Self::UnexpectedArgument(argument) => write!(
                formatter,
                "unexpected argument {argument:?}; run `pomock --help` for usage"
            ),
            Self::NonUnicodeArgument => formatter.write_str("arguments must be valid Unicode"),
        }
    }
}

impl Error for CliError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvalidWorkspaceName(error) => Some(error),
            _ => None,
        }
    }
}

pub(crate) fn write_help(writer: &mut impl Write) -> io::Result<()> {
    writeln!(
        writer,
        "pomock - a Pomodoro timer and task workspace\n\nUsage: pomock [OPTIONS]\n\nOptions:\n  -w, --workspace NAME  Use or create a named workspace\n  -h, --help            Show this help\n  -v, --version         Show version"
    )
}

pub(crate) fn write_version(writer: &mut impl Write) -> io::Result<()> {
    writeln!(writer, "pomock {}", env!("CARGO_PKG_VERSION"))
}

#[cfg(test)]
mod tests;
