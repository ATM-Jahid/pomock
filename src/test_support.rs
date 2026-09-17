use pomock::app::{Action, App, Direction, TaskState};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT_TEMP_PATH: AtomicU64 = AtomicU64::new(0);

pub(crate) fn temp_path(name: &str) -> PathBuf {
    let unique = NEXT_TEMP_PATH.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "pomock-main-test-{}-{unique}-{name}",
        std::process::id()
    ))
}

pub(crate) fn backup_paths(path: &std::path::Path) -> Vec<PathBuf> {
    let prefix = format!("{}.backup-", path.file_name().unwrap().to_string_lossy());
    fs::read_dir(path.parent().unwrap())
        .unwrap()
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|candidate| {
            candidate
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with(&prefix)
        })
        .collect()
}

pub(crate) fn only_backup(path: &std::path::Path) -> PathBuf {
    let backups = backup_paths(path);
    assert_eq!(backups.len(), 1);
    backups.into_iter().next().unwrap()
}

pub(crate) fn task_state(description: &str) -> TaskState {
    let mut app = App::new();
    let _ = app.dispatch(Action::NavigateFocus(Direction::Down));
    let _ = app.dispatch(Action::BeginAdd);
    for character in description.chars() {
        let _ = app.dispatch(Action::PushInput(character));
    }
    let _ = app.dispatch(Action::SubmitEdit);
    app.task_state()
}
