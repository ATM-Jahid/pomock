use std::{
    io::{self, Write},
    path::Path,
};

#[cfg(unix)]
use std::fs;

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

    use super::write;

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
}
