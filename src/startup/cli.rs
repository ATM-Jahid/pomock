use std::{
    error::Error,
    ffi::OsString,
    fmt,
    io::{self, Write},
};

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum CliCommand {
    Run { workspace: Option<String> },
    Help,
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
                "--wspace" => {
                    if workspace.is_some() {
                        return Err(CliError::DuplicateWorkspace);
                    }
                    let name = arguments.next().ok_or(CliError::MissingWorkspaceName)?;
                    let name = name
                        .into_string()
                        .map_err(|_| CliError::NonUnicodeArgument)?;
                    validate_workspace_name(&name)?;
                    workspace = Some(name);
                }
                _ if argument.starts_with("--wspace=") => {
                    if workspace.is_some() {
                        return Err(CliError::DuplicateWorkspace);
                    }
                    let name = argument.trim_start_matches("--wspace=");
                    validate_workspace_name(name)?;
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
    InvalidWorkspaceName(String),
    UnexpectedArgument(String),
    NonUnicodeArgument,
}

impl fmt::Display for CliError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingWorkspaceName => formatter.write_str("--wspace requires a workspace name"),
            Self::DuplicateWorkspace => formatter.write_str("--wspace may only be specified once"),
            Self::InvalidWorkspaceName(name) => write!(
                formatter,
                "invalid workspace name {name:?}; use letters, numbers, '.', '-', or '_'"
            ),
            Self::UnexpectedArgument(argument) => write!(
                formatter,
                "unexpected argument {argument:?}; run `pomock --help` for usage"
            ),
            Self::NonUnicodeArgument => formatter.write_str("arguments must be valid Unicode"),
        }
    }
}

impl Error for CliError {}

fn validate_workspace_name(name: &str) -> Result<(), CliError> {
    let valid = !name.is_empty()
        && name != "."
        && name != ".."
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'));
    valid
        .then_some(())
        .ok_or_else(|| CliError::InvalidWorkspaceName(name.to_owned()))
}

pub(crate) fn write_help(writer: &mut impl Write) -> io::Result<()> {
    writeln!(
        writer,
        "pomock - a Pomodoro timer and task workspace\n\nUsage: pomock [--wspace NAME]\n\nOptions:\n  --wspace NAME  Use or create a named task workspace\n  -h, --help     Show this help"
    )
}
