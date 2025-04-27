//! Savefile format definitions, search utilities and anything related.
//!
//! Writable game data is stored at `<cwd>/local/`, saves at `local/saves/`.
//! The basic save format is a sqlite database at `local/saves/dir-name/geosia.sqlite`.

use std::{
    fs::{self, FileType},
    ops::{Deref, DerefMut},
    path::{Path, PathBuf},
    sync::OnceLock,
};

use anyhow::{Context, anyhow};
use kstring::KString;
use rusqlite::{OpenFlags, ToSql, types::FromSql};

use crate::ErrorList;

pub mod sql;

/// sqlite-compatible [`KString`] wrapper
#[derive(Clone, Debug, Default, Hash, PartialEq, Eq, PartialOrd, Ord)]
#[repr(transparent)]
pub struct SqlKString(KString);

impl Deref for SqlKString {
    type Target = KString;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for SqlKString {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl FromSql for SqlKString {
    fn column_result(value: rusqlite::types::ValueRef<'_>) -> rusqlite::types::FromSqlResult<Self> {
        Ok(Self(KString::from_ref(value.as_str()?)))
    }
}

impl ToSql for SqlKString {
    fn to_sql(&self) -> rusqlite::Result<rusqlite::types::ToSqlOutput<'_>> {
        Ok(rusqlite::types::ToSqlOutput::Borrowed(rusqlite::types::ValueRef::Text(
            self.as_bytes(),
        )))
    }
}

const NEWEST_SUPPORTED_SAVE_VERSION: i32 = sql::SQL_MIGRATIONS.last().unwrap().0;
static WRITABLE_DATA_DIRECTORY_CACHE: OnceLock<&'static Path> = OnceLock::new();
static SAVES_DIRECTORY_CACHE: OnceLock<&'static Path> = OnceLock::new();
/// Name of the database file inside the savefile directory.
pub static SAVEFILE_DB_NAME: &str = "geosia.sqlite";

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
pub fn saves_directory() -> &'static Path {
    fn init_saves_diretory() -> &'static Path {
        let path = PathBuf::from("local").join("saves");
        ensure_writable_dir(&path)
            .unwrap_or_else(|e| panic!("Could not ensure a writable saves directory at {path:?}: {e}"));
        Box::leak(path.into_boxed_path())
    }
    SAVES_DIRECTORY_CACHE.get_or_init(init_saves_diretory)
}

/// Metadata about a game savefile.
#[derive(Clone, Debug)]
pub struct SavefileMetadata {
    /// The full path to the savefile directory.
    pub path: PathBuf,
    /// The name of the save.
    pub name: String,
    /// The directory name, shown as a secondary name in the UI if not the same as `name`.
    pub dir_name: String,
    /// Size of the savefile on disk in bytes.
    pub disk_size: u64,
}

/// Reads the savefile metadata (if available) for a given save directory.
pub fn get_save_metadata(save_dir_path: &Path) -> anyhow::Result<SavefileMetadata> {
    let db_path = save_dir_path.join(SAVEFILE_DB_NAME);
    let db_stat = fs::metadata(&db_path)?;
    let conn = rusqlite::Connection::open_with_flags(
        &db_path,
        OpenFlags::SQLITE_OPEN_EXRESCODE | OpenFlags::SQLITE_OPEN_NO_MUTEX | OpenFlags::SQLITE_OPEN_READ_ONLY,
    )?;
    let schema_ver = conn
        .query_row("SELECT user_version FROM pragma_user_version", [], |row| {
            row.get::<_, i32>(0)
        })
        .context("Reading the schema version")?;
    if schema_ver > NEWEST_SUPPORTED_SAVE_VERSION {
        return Err(anyhow!(
            "Saved schema version {schema_ver} greater than max supported {NEWEST_SUPPORTED_SAVE_VERSION}, you probably need to update the game."
        ));
    }
    let name = conn.query_row(
        "SELECT field_name, field_value FROM geosia_savefile_metadata WHERE field_name='name'",
        [],
        |row| row.get::<_, String>(1),
    )?;
    conn.close().map_err(|(_, e)| e)?;
    Ok(SavefileMetadata {
        path: save_dir_path.to_owned(),
        name,
        dir_name: save_dir_path
            .file_name()
            .ok_or_else(|| anyhow!("Could not extract directory name from {db_path:?}"))?
            .to_string_lossy()
            .into_owned(),
        disk_size: db_stat.len(),
    })
}

/// Lists all the savefiles in the given directory and gets their metadata.
/// Returned along a list of errors found in unparseable save directories.
pub fn list_saves(saves_directory: &Path) -> (Vec<SavefileMetadata>, ErrorList) {
    let mut saves = Vec::new();
    let mut errors = ErrorList::new();

    let dir_entries = match fs::read_dir(saves_directory) {
        Ok(de) => de,
        Err(e) => {
            errors.attach_if_err::<(), _>(Err(e).context("Could not enumerate saves subdirectories"));
            return (saves, errors);
        }
    };
    let mut dir_entries = dir_entries.filter_map(|e| e.ok()).collect::<Vec<_>>();
    dir_entries.sort_by_key(|de| de.file_name());
    for de in dir_entries {
        if !de.file_type().as_ref().is_ok_and(FileType::is_dir) {
            continue;
        }
        let dir_path = de.path();
        match get_save_metadata(&dir_path) {
            Ok(meta) => {
                saves.push(meta);
            }
            Err(e) => {
                errors.attach_if_err::<(), _>(Err(e).context(dir_path.to_string_lossy().into_owned()));
                continue;
            }
        }
    }

    (saves, errors)
}
