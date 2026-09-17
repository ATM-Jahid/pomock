use super::{CliCommand, CliError, write_version};
use std::ffi::OsString;

#[test]
fn version_flags_print_the_package_version() {
    for flag in ["-v", "--version"] {
        assert_eq!(
            CliCommand::parse([OsString::from(flag)]).unwrap(),
            CliCommand::Version
        );
    }
    let mut output = Vec::new();
    write_version(&mut output).unwrap();
    assert_eq!(
        String::from_utf8(output).unwrap(),
        format!("pomock {}\n", env!("CARGO_PKG_VERSION"))
    );
}

#[test]
fn workspace_argument_accepts_separate_and_equals_forms() {
    for flag in ["-w", "--workspace"] {
        assert_eq!(
            CliCommand::parse([OsString::from(flag), OsString::from("client-one")]).unwrap(),
            CliCommand::Run {
                workspace: Some("client-one".to_owned())
            }
        );
    }
    assert_eq!(
        CliCommand::parse([OsString::from("--workspace=personal.2026")]).unwrap(),
        CliCommand::Run {
            workspace: Some("personal.2026".to_owned())
        }
    );
    assert_eq!(
        CliCommand::parse(Vec::<OsString>::new()).unwrap(),
        CliCommand::Run { workspace: None }
    );
}

#[test]
fn workspace_argument_rejects_missing_unsafe_and_duplicate_names() {
    for flag in ["-w", "--workspace"] {
        assert_eq!(
            CliCommand::parse([OsString::from(flag)]).unwrap_err(),
            CliError::MissingWorkspaceName
        );
    }
    assert!(matches!(
        CliCommand::parse([OsString::from("--workspace=../shared")]).unwrap_err(),
        CliError::InvalidWorkspaceName(_)
    ));
    assert_eq!(
        CliCommand::parse([
            OsString::from("--workspace=one"),
            OsString::from("-w"),
            OsString::from("two")
        ])
        .unwrap_err(),
        CliError::DuplicateWorkspace
    );
}
