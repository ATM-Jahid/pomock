use super::{StartupError, load_config_for_startup, load_tasks_for_startup};
use crate::test_support::{backup_paths, only_backup, task_state, temp_path};
use pomock::{
    app::TaskState,
    config::Config,
    persistence::{ConfigStore, TaskStore},
};
use std::{
    fs,
    io::{self, BufRead, Cursor, Read},
    path::PathBuf,
};

fn write_config_with_unknown_field(path: &std::path::Path) -> String {
    ConfigStore::at(path).save(&Config::default()).unwrap();
    let mut stored: toml::Value = toml::from_str(&fs::read_to_string(path).unwrap()).unwrap();
    stored
        .as_table_mut()
        .unwrap()
        .insert("obsolete".to_owned(), true.into());
    let contents = toml::to_string_pretty(&stored).unwrap();
    fs::write(path, &contents).unwrap();
    contents
}

enum FileMutation {
    Write(Vec<u8>),
    Delete,
}

struct MutatingReader {
    input: Cursor<Vec<u8>>,
    path: PathBuf,
    mutation: Option<FileMutation>,
}

impl MutatingReader {
    fn writing(path: &std::path::Path, contents: impl Into<Vec<u8>>, input: &[u8]) -> Self {
        Self {
            input: Cursor::new(input.to_vec()),
            path: path.to_owned(),
            mutation: Some(FileMutation::Write(contents.into())),
        }
    }

    fn deleting(path: &std::path::Path, input: &[u8]) -> Self {
        Self {
            input: Cursor::new(input.to_vec()),
            path: path.to_owned(),
            mutation: Some(FileMutation::Delete),
        }
    }

    fn apply_mutation(&mut self) -> io::Result<()> {
        match self.mutation.take() {
            Some(FileMutation::Write(contents)) => fs::write(&self.path, contents),
            Some(FileMutation::Delete) => fs::remove_file(&self.path),
            None => Ok(()),
        }
    }
}

impl Read for MutatingReader {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        self.apply_mutation()?;
        self.input.read(buffer)
    }
}

impl BufRead for MutatingReader {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        self.apply_mutation()?;
        self.input.fill_buf()
    }

    fn consume(&mut self, amount: usize) {
        self.input.consume(amount);
    }
}

#[test]
fn startup_creates_missing_config_and_task_files() {
    let config_path = temp_path("missing-startup/config.toml");
    let tasks_path = temp_path("missing-startup/tasks.toml");
    let task_store = TaskStore::at(&tasks_path);

    let config = load_config_for_startup(
        &ConfigStore::at(&config_path),
        &mut Cursor::new(Vec::<u8>::new()),
        &mut Vec::new(),
    )
    .unwrap()
    .unwrap();
    let tasks = load_tasks_for_startup(
        Some(&task_store),
        &mut Cursor::new(Vec::<u8>::new()),
        &mut Vec::new(),
    )
    .unwrap()
    .unwrap();

    assert_eq!(config, Config::default());
    assert_eq!(
        ConfigStore::at(&config_path).load().unwrap(),
        Config::default()
    );
    assert_eq!(tasks, TaskState::default());
    assert_eq!(task_store.load().unwrap(), TaskState::default());
    fs::remove_dir_all(config_path.parent().unwrap()).unwrap();
    fs::remove_dir_all(tasks_path.parent().unwrap()).unwrap();
}

#[test]
fn valid_config_edit_during_confirmation_is_preserved_and_used() {
    let path = temp_path("config-edited-during-prompt.toml");
    fs::write(&path, "not valid toml =").unwrap();
    let valid_path = temp_path("replacement-config.toml");
    ConfigStore::at(&valid_path)
        .save(&Config::default())
        .unwrap();
    let valid = fs::read(&valid_path).unwrap();
    fs::remove_file(valid_path).unwrap();
    let mut reader = MutatingReader::writing(&path, valid.clone(), b"b\n");

    let config = load_config_for_startup(&ConfigStore::at(&path), &mut reader, &mut Vec::new())
        .unwrap()
        .unwrap();

    assert_eq!(config, Config::default());
    assert_eq!(fs::read(&path).unwrap(), valid);
    assert!(backup_paths(&path).is_empty());
    fs::remove_file(path).unwrap();
}

#[test]
fn changed_invalid_config_is_prompted_again_before_replacement() {
    let path = temp_path("config-invalid-edit-during-prompt.toml");
    fs::write(&path, "first invalid =").unwrap();
    let replacement = b"second invalid =".to_vec();
    let mut reader = MutatingReader::writing(&path, replacement.clone(), b"b\nb\n");
    let mut output = Vec::new();

    let config = load_config_for_startup(&ConfigStore::at(&path), &mut reader, &mut output)
        .unwrap()
        .unwrap();

    assert_eq!(config, Config::default());
    assert_eq!(fs::read(only_backup(&path)).unwrap(), replacement);
    assert_eq!(
        String::from_utf8(output)
            .unwrap()
            .matches("pomock could not load the configuration file")
            .count(),
        2
    );
    for backup in backup_paths(&path) {
        fs::remove_file(backup).unwrap();
    }
    fs::remove_file(path).unwrap();
}

#[test]
fn config_deleted_during_confirmation_is_recreated_without_backup() {
    let path = temp_path("config-deleted-during-prompt.toml");
    fs::write(&path, "not valid toml =").unwrap();
    let mut reader = MutatingReader::deleting(&path, b"b\n");

    let config = load_config_for_startup(&ConfigStore::at(&path), &mut reader, &mut Vec::new())
        .unwrap()
        .unwrap();

    assert_eq!(config, Config::default());
    assert_eq!(ConfigStore::at(&path).load().unwrap(), Config::default());
    assert!(backup_paths(&path).is_empty());
    fs::remove_file(path).unwrap();
}

#[test]
fn valid_task_edit_during_confirmation_is_preserved_and_used() {
    let path = temp_path("tasks-edited-during-prompt.toml");
    fs::write(&path, "not valid toml =").unwrap();
    let valid = b"version = 1\ntodo = [\"edited\"]\ndone = []\n".to_vec();
    let store = TaskStore::at(&path);
    let mut reader = MutatingReader::writing(&path, valid.clone(), b"b\n");

    let state = load_tasks_for_startup(Some(&store), &mut reader, &mut Vec::new())
        .unwrap()
        .unwrap();

    assert_eq!(state, task_state("edited"));
    assert_eq!(fs::read(&path).unwrap(), valid);
    assert!(backup_paths(&path).is_empty());
    fs::remove_file(path).unwrap();
}

#[test]
fn unknown_config_key_can_be_left_in_place_when_quitting() {
    let path = temp_path("unknown-key-declined.toml");
    let original = write_config_with_unknown_field(&path);
    let mut output = Vec::new();

    let config = load_config_for_startup(
        &ConfigStore::at(&path),
        &mut Cursor::new(b"q\n"),
        &mut output,
    )
    .unwrap();

    assert!(config.is_none());
    assert_eq!(fs::read_to_string(&path).unwrap(), original);
    assert!(backup_paths(&path).is_empty());
    assert!(
        String::from_utf8(output)
            .unwrap()
            .contains("Back up the invalid file, create a new config, and continue")
    );
    fs::remove_file(path).unwrap();
}

#[test]
fn unknown_config_key_can_be_backed_up_and_replaced_with_defaults() {
    let path = temp_path("unknown-key-approved.toml");
    let original = write_config_with_unknown_field(&path);
    let mut output = Vec::new();

    let config = load_config_for_startup(
        &ConfigStore::at(&path),
        &mut Cursor::new(b"b\n"),
        &mut output,
    )
    .unwrap()
    .unwrap();

    assert_eq!(config, Config::default());
    assert!(!fs::read_to_string(&path).unwrap().contains("obsolete"));
    let backup = only_backup(&path);
    assert_eq!(fs::read_to_string(&backup).unwrap(), original);
    assert!(
        String::from_utf8(output)
            .unwrap()
            .contains("Backed up the invalid file")
    );
    fs::remove_file(backup).unwrap();
    fs::remove_file(path).unwrap();
}

#[test]
fn invalid_config_can_be_backed_up_and_atomically_replaced_with_defaults() {
    let path = temp_path("invalid-config.toml");
    let original = "not valid toml =";
    fs::write(&path, original).unwrap();
    let mut input = Cursor::new(b"invalid\nbackup\n");
    let mut output = Vec::new();

    let config = load_config_for_startup(&ConfigStore::at(&path), &mut input, &mut output)
        .unwrap()
        .unwrap();

    assert_eq!(config, Config::default());
    assert_eq!(ConfigStore::at(&path).load().unwrap(), Config::default());
    let backup = only_backup(&path);
    assert_eq!(fs::read_to_string(&backup).unwrap(), original);
    let output = String::from_utf8(output).unwrap();
    assert!(output.contains("could not load the configuration file"));
    assert!(output.contains("Enter b to back up and create a new config, or q to quit."));
    assert!(output.contains("Back up the invalid file, create a new config, and continue"));
    assert!(output.contains(backup.to_str().unwrap()));
    fs::remove_file(backup).unwrap();
    fs::remove_file(path).unwrap();
}

#[test]
fn invalid_config_can_be_left_in_place_when_quitting() {
    let path = temp_path("invalid-config-quit.toml");
    let contents = "not valid toml =";
    fs::write(&path, contents).unwrap();
    let mut input = Cursor::new(b"q\n");
    let mut output = Vec::new();

    let config = load_config_for_startup(&ConfigStore::at(&path), &mut input, &mut output).unwrap();

    assert!(config.is_none());
    assert_eq!(fs::read_to_string(&path).unwrap(), contents);
    assert!(backup_paths(&path).is_empty());
    fs::remove_file(path).unwrap();
}

#[test]
fn invalid_task_file_can_be_backed_up_and_replaced_with_empty_state() {
    let path = temp_path("invalid-tasks.toml");
    let original = "version = 2\ntodo = []\ndone = []\n";
    fs::write(&path, original).unwrap();
    let store = TaskStore::at(&path);
    let mut input = Cursor::new(b"b\n");
    let mut output = Vec::new();

    let state = load_tasks_for_startup(Some(&store), &mut input, &mut output)
        .unwrap()
        .unwrap();

    assert_eq!(state, TaskState::default());
    assert_eq!(store.load().unwrap(), TaskState::default());
    let backup = only_backup(&path);
    assert_eq!(fs::read_to_string(&backup).unwrap(), original);
    let output = String::from_utf8(output).unwrap();
    assert!(output.contains("could not load the task data file"));
    assert!(output.contains("Back up the invalid file, create a new task file, and continue"));
    fs::remove_file(backup).unwrap();
    fs::remove_file(path).unwrap();
}

#[test]
fn task_read_errors_do_not_offer_to_delete_the_path() {
    let path = temp_path("task-read-error");
    fs::create_dir(&path).unwrap();
    let store = TaskStore::at(&path);
    let mut input = Cursor::new(b"d\n");
    let mut output = Vec::new();

    let error = load_tasks_for_startup(Some(&store), &mut input, &mut output).unwrap_err();

    assert!(matches!(error, StartupError::TaskPersistence(_)));
    assert!(output.is_empty());
    assert!(path.is_dir());
    fs::remove_dir(path).unwrap();
}
