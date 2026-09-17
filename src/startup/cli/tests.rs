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
    for name in [
        "",
        ".",
        "..",
        "../shared",
        "/tmp/workspace",
        "nested\\workspace",
        "two words",
        "café",
    ] {
        let expected = pomock::persistence::validate_workspace_name(name).unwrap_err();
        for arguments in [
            vec![OsString::from("-w"), OsString::from(name)],
            vec![OsString::from("--workspace"), OsString::from(name)],
            vec![OsString::from(format!("--workspace={name}"))],
        ] {
            let error = CliCommand::parse(arguments).unwrap_err();
            assert!(matches!(error, CliError::InvalidWorkspaceName(_)));
            assert_eq!(error.to_string(), expected.to_string());
        }
    }
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
