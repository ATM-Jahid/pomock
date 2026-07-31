use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

/// Replaces a file without exposing a partially written destination.
pub(crate) fn write(path: &Path, contents: &[u8]) -> io::Result<()> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;

    temporary.write_all(contents)?;
    temporary.as_file().sync_all()?;
    temporary.persist(path).map_err(|error| error.error)?;

    sync_parent(parent)
}

/// Preserves the supplied contents in a uniquely named file beside `path`.
///
/// The backup itself is written atomically so a failed backup never leaves a
/// partial recovery copy that could be mistaken for a complete one.
pub(crate) fn backup(path: &Path, contents: &[u8]) -> io::Result<PathBuf> {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let file_name = path
        .file_name()
        .unwrap_or(path.as_os_str())
        .to_string_lossy();

    for suffix in 0_u32.. {
        let disambiguator = if suffix == 0 {
            String::new()
        } else {
            format!("-{suffix}")
        };
        let backup = path.with_file_name(format!(
            "{file_name}.backup-{}-{:09}{disambiguator}",
            timestamp.as_secs(),
            timestamp.subsec_nanos()
        ));
        if backup.try_exists()? {
            continue;
        }
        if write_new(&backup, contents)? {
            return Ok(backup);
        }
    }

    unreachable!("the backup suffix space is inexhaustible in practice")
}

pub(crate) fn write_new(path: &Path, contents: &[u8]) -> io::Result<bool> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    temporary.write_all(contents)?;
    temporary.as_file().sync_all()?;
    match temporary.persist_noclobber(path) {
        Ok(_) => {
            sync_parent(parent)?;
            Ok(true)
        }
        Err(error) if error.error.kind() == io::ErrorKind::AlreadyExists => Ok(false),
        Err(error) => Err(error.error),
    }
}

#[cfg(unix)]
fn sync_parent(parent: &Path) -> io::Result<()> {
    fs::File::open(parent)?.sync_all()
}

#[cfg(not(unix))]
fn sync_parent(_parent: &Path) -> io::Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::{backup, write};

    #[test]
    fn replaces_existing_contents_and_leaves_no_temporary_file() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("state.toml");
        fs::write(&path, "old").unwrap();

        write(&path, b"new").unwrap();

        assert_eq!(fs::read_to_string(path).unwrap(), "new");
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
    }

    #[test]
    fn failed_replacement_preserves_the_destination() {
        let directory = tempfile::tempdir().unwrap();
        let destination = directory.path().join("state.toml");
        fs::create_dir(&destination).unwrap();
        fs::write(destination.join("sentinel"), "preserved").unwrap();

        assert!(write(&destination, b"new").is_err());

        assert_eq!(
            fs::read_to_string(destination.join("sentinel")).unwrap(),
            "preserved"
        );
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
    }

    #[test]
    fn backup_preserves_contents_beside_the_original() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("state.toml");
        fs::write(&path, "history").unwrap();

        let backup_path = backup(&path, b"history").unwrap();

        assert_eq!(backup_path.parent(), path.parent());
        assert!(
            backup_path
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("state.toml.backup-")
        );
        assert_eq!(fs::read_to_string(path).unwrap(), "history");
        assert_eq!(fs::read_to_string(backup_path).unwrap(), "history");
    }
}
