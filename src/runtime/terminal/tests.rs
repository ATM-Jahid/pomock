use super::{RunError, combine_run_and_restore_results};
use std::io;

#[test]
fn run_error_is_preserved_when_restoration_succeeds() {
    let run_error = io::Error::new(io::ErrorKind::BrokenPipe, "run failed");

    let error = combine_run_and_restore_results::<()>(Err(RunError::Terminal(run_error)), Ok(()))
        .unwrap_err();

    assert!(matches!(
        error,
        RunError::Terminal(ref error) if error.kind() == io::ErrorKind::BrokenPipe
    ));
    assert_eq!(error.to_string(), "run failed");
}

#[test]
fn restoration_error_is_reported_after_a_successful_run() {
    let restore_error = io::Error::other("restore failed");

    let error = combine_run_and_restore_results(Ok(()), Err(restore_error)).unwrap_err();

    assert!(matches!(error, RunError::Terminal(_)));
    assert_eq!(error.to_string(), "restore failed");
}

#[test]
fn simultaneous_run_and_restoration_errors_are_both_reported() {
    let run_error = io::Error::new(io::ErrorKind::BrokenPipe, "run failed");
    let restore_error = io::Error::other("restore failed");

    let error = combine_run_and_restore_results::<()>(
        Err(RunError::Terminal(run_error)),
        Err(restore_error),
    )
    .unwrap_err();

    assert!(matches!(error, RunError::TerminalRestoration { .. }));
    assert_eq!(
        error.to_string(),
        "run failed; terminal restoration also failed: restore failed"
    );
}
