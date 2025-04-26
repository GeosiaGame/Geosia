//! Savefile format definitions, search utilities and anything related.

use std::{
    path::{Path, PathBuf},
    sync::OnceLock,
};

pub mod sql;

static WRITABLE_DATA_DIRECTORY_CACHE: OnceLock<&'static Path> = OnceLock::new();
static SAVES_DIRECTORY_CACHE: OnceLock<&'static Path> = OnceLock::new();

fn ensure_writable_dir(path: &Path) -> std::io::Result<&Path> {
    if !path.is_dir() {
        std::fs::create_dir_all(path)?;
    }
    let stat = std::fs::metadata(path)?;
    if !stat.is_dir() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotADirectory,
            "Path is not a directory",
        ));
    }
    if stat.permissions().readonly() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::ReadOnlyFilesystem,
            "Path is read only",
        ));
    }
    Ok(path)
}

/// Gets the writable data directory based on the CWD of the game proces, cached for the process.
/// Currently `<cwd>/local/`, created if doesn't already exist.
/// Panics if the current working directory is invalid or the data directory cannot be written to, this is basically irrecoverable for the game.
pub fn writable_data_directory() -> &'static Path {
    fn init_writable_data_directory() -> &'static Path {
        let local_path = std::env::current_dir()
            .expect("Current working directory could not be obtained")
            .join("local");
        ensure_writable_dir(&local_path)
            .unwrap_or_else(|e| panic!("Could not ensure a writable data directory at {local_path:?}: {e}"));
        Box::leak(local_path.into_boxed_path())
    }
    WRITABLE_DATA_DIRECTORY_CACHE.get_or_init(init_writable_data_directory)
}

/// Gets the saves subdirectory of [`writable_data_directory`].
pub fn saves_subdirectory() -> &'static Path {
    fn init_saves_diretory() -> &'static Path {
        let path = PathBuf::from("local").join("saves");
        ensure_writable_dir(&path)
            .unwrap_or_else(|e| panic!("Could not ensure a writable saves directory at {path:?}: {e}"));
        Box::leak(path.into_boxed_path())
    }
    SAVES_DIRECTORY_CACHE.get_or_init(init_saves_diretory)
}
