//! Savefile format definitions, search utilities and anything related.
//!
//! Writable game data is stored at `<cwd>/local/`, saves at `local/saves/`.
//! The basic save format is a sqlite database at `local/saves/dir-name/geosia.sqlite`.

use std::{
    fmt::Write,
    fs::{self, FileType},
    ops::{Deref, DerefMut},
    path::{Path, PathBuf},
    sync::OnceLock,
};

use anyhow::{Context, anyhow};
use chrono::{DateTime, Utc};
use kstring::KString;
use rusqlite::{Connection, OpenFlags, ToSql, types::FromSql};
use uuid::Uuid;

use crate::{ErrorList, registry::RegistryName};

pub mod queries;
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

static NEWEST_SUPPORTED_SAVE_VERSION: i32 = sql::SQL_MIGRATIONS.last().unwrap().0;
static WRITABLE_DATA_DIRECTORY_CACHE: OnceLock<&'static Path> = OnceLock::new();
static SAVES_DIRECTORY_CACHE: OnceLock<&'static Path> = OnceLock::new();
/// Name of the database file inside the savefile directory.
pub static SAVEFILE_DB_NAME: &str = "geosia.sqlite";
/// The savefile meta table key for the savefile's display name.
pub static SAVEFILE_META_NAME_KEY: RegistryName = RegistryName::gs_const("name");
/// The savefile meta table key for the savefile's creation timestamp in ISO 8601 UTC time.
pub static SAVEFILE_META_CREATED_AT_UTC_KEY: RegistryName = RegistryName::gs_const("created_at_utc");
/// The savefile meta table key for the savefile's universe UUID.
pub static SAVEFILE_META_UNIVERSE_UUID_KEY: RegistryName = RegistryName::gs_const("universe_uuid");

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

/// Identifies where the savefile should be stored.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub enum SavefileLocation {
    /// In a persistent directory on disk
    Path(PathBuf),
    /// In a temporary in-memory database
    Memory,
}

impl SavefileLocation {
    /// Access the filesystem path if this is the `Path` variant.
    pub fn path(&self) -> Option<&Path> {
        match self {
            Self::Path(path) => Some(path),
            Self::Memory => None,
        }
    }
}

/// Metadata about a game savefile.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct SavefileMetadata {
    /// The full path to the savefile directory.
    pub location: SavefileLocation,
    /// The name of the save.
    pub name: String,
    /// The directory name, shown as a secondary name in the UI if not the same as `name`.
    pub dir_name: String,
    /// Size of the savefile on disk in bytes.
    pub disk_size: u64,
    /// Save creation time.
    pub created_at: DateTime<Utc>,
    /// Time the save was last modified at.
    pub modified_at: DateTime<Utc>,
    /// UUID of the universe saved in this file.
    pub uuid: Uuid,
}

impl SavefileMetadata {
    /// Creates a new in-memory savefile for testing.
    pub fn new_memory_savefile() -> Self {
        let now = Utc::now();
        Self {
            location: SavefileLocation::Memory,
            name: "Test Savefile".to_string(),
            dir_name: "Test Savefile".to_string(),
            disk_size: 0,
            created_at: now,
            modified_at: now,
            uuid: Uuid::new_v4(),
        }
    }

    /// Opens a read-write DB connection to this savefile's location.
    pub fn open_rw(&self) -> rusqlite::Result<Connection> {
        match &self.location {
            SavefileLocation::Path(path) => queries::open_rw_connection(path),
            SavefileLocation::Memory => {
                let conn = queries::create_test_memory_db()?;
                queries::insert_new_savefile_meta(&conn, &self.name, self.created_at, self.uuid)?;
                Ok(conn)
            }
        }
    }
}

/// Reads the savefile metadata (if available) for a given save directory.
pub fn get_save_metadata(save_dir_path: &Path) -> anyhow::Result<SavefileMetadata> {
    let db_path = save_dir_path.join(SAVEFILE_DB_NAME);
    let db_stat = fs::metadata(&db_path)?;
    let conn = Connection::open_with_flags(
        &db_path,
        OpenFlags::SQLITE_OPEN_EXRESCODE | OpenFlags::SQLITE_OPEN_NO_MUTEX | OpenFlags::SQLITE_OPEN_READ_ONLY,
    )?;
    let schema_ver = queries::select_savefile_schema_version(&conn).context("Reading the schema version")?;
    if schema_ver > NEWEST_SUPPORTED_SAVE_VERSION {
        return Err(anyhow!(
            "Saved schema version {schema_ver} greater than max supported {NEWEST_SUPPORTED_SAVE_VERSION}, you probably need to update the game."
        ));
    }
    let name = queries::select_savefile_meta_name(&conn)?;
    let created_at = queries::select_savefile_meta_created_at(&conn)?;
    let uuid = queries::select_savefile_meta_universe_uuid(&conn)?;
    let modified_at = db_stat.modified().map(DateTime::<Utc>::from).unwrap_or_default();
    conn.close().map_err(|(_, e)| e)?;
    Ok(SavefileMetadata {
        location: SavefileLocation::Path(save_dir_path.to_owned()),
        name,
        dir_name: save_dir_path
            .file_name()
            .ok_or_else(|| anyhow!("Could not extract directory name from {db_path:?}"))?
            .to_string_lossy()
            .into_owned(),
        disk_size: db_stat.len(),
        created_at,
        modified_at,
        uuid,
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

/// Converts an sqlite error into an IO error.
pub fn sql_to_io_error(err: rusqlite::Error) -> std::io::Error {
    use std::io::ErrorKind;
    let Some(err_code) = err.sqlite_error_code() else {
        return std::io::Error::other(err);
    };
    let kind = match err_code {
        rusqlite::ErrorCode::InternalMalfunction => ErrorKind::Other,
        rusqlite::ErrorCode::PermissionDenied => ErrorKind::PermissionDenied,
        rusqlite::ErrorCode::OperationAborted => ErrorKind::Interrupted,
        rusqlite::ErrorCode::DatabaseBusy => ErrorKind::ResourceBusy,
        rusqlite::ErrorCode::DatabaseLocked => ErrorKind::ResourceBusy,
        rusqlite::ErrorCode::OutOfMemory => ErrorKind::OutOfMemory,
        rusqlite::ErrorCode::ReadOnly => ErrorKind::ReadOnlyFilesystem,
        rusqlite::ErrorCode::OperationInterrupted => ErrorKind::Interrupted,
        rusqlite::ErrorCode::SystemIoFailure => ErrorKind::BrokenPipe,
        rusqlite::ErrorCode::DatabaseCorrupt => ErrorKind::InvalidData,
        rusqlite::ErrorCode::NotFound => ErrorKind::NotFound,
        rusqlite::ErrorCode::DiskFull => ErrorKind::StorageFull,
        rusqlite::ErrorCode::CannotOpen => ErrorKind::PermissionDenied,
        rusqlite::ErrorCode::FileLockingProtocolFailed => ErrorKind::Deadlock,
        rusqlite::ErrorCode::SchemaChanged => ErrorKind::ResourceBusy,
        rusqlite::ErrorCode::TooBig => ErrorKind::QuotaExceeded,
        rusqlite::ErrorCode::ConstraintViolation => ErrorKind::InvalidData,
        rusqlite::ErrorCode::TypeMismatch => ErrorKind::InvalidData,
        rusqlite::ErrorCode::ApiMisuse => ErrorKind::Unsupported,
        rusqlite::ErrorCode::NoLargeFileSupport => ErrorKind::Unsupported,
        rusqlite::ErrorCode::AuthorizationForStatementDenied => ErrorKind::PermissionDenied,
        rusqlite::ErrorCode::ParameterOutOfRange => ErrorKind::InvalidInput,
        rusqlite::ErrorCode::NotADatabase => ErrorKind::InvalidData,
        rusqlite::ErrorCode::Unknown => ErrorKind::Other,
        _ => todo!(),
    };
    std::io::Error::new(kind, err)
}

/// Creates a new savefile with the given display name in the saves directory (subdirectory name is automatically computed).
/// The saves directory should already exist.
pub fn new_save(saves_directory: &Path, mut name: &str) -> Result<SavefileMetadata, std::io::Error> {
    name = name.trim();
    if name.is_empty() {
        name = "Geosia";
    }
    // Checks for some common illegal or easily confusing characters
    fn illegal_path_char(c: char) -> bool {
        if c.is_control() {
            return true;
        }
        ['/', '\\', '.', '<', '>', ':', '"', '\'', '|', '?', '*', '!', '&'].contains(&c)
    }
    let pathsafe_name = name.replace(illegal_path_char, "_");
    let mut final_dir_name = String::with_capacity(pathsafe_name.len() + 4);
    let mut i = 0i32;
    loop {
        final_dir_name.clear();
        if i == 0 {
            final_dir_name.push_str(&pathsafe_name);
        } else {
            write!(&mut final_dir_name, "{pathsafe_name}_{i}").expect("Path format error");
        }
        match fs::create_dir(saves_directory.join(&final_dir_name)) {
            Ok(()) => break Ok(()),
            Err(e) if i >= 1024 => break Err(e),
            Err(_) => {}
        }
        i += 1;
    }?;
    let save_dir = saves_directory.join(&final_dir_name);
    let save_data =
        zstd::decode_all(sql::SQL_0000_NEW_GAME_TEMPLATE_ZST).expect("Internal new save file template is broken");
    let save_db_path = save_dir.join(SAVEFILE_DB_NAME);
    fs::write(&save_db_path, &save_data)?;
    // Write the save name and creation time.
    let conn = Connection::open_with_flags(
        &save_db_path,
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX | OpenFlags::SQLITE_OPEN_EXRESCODE,
    )
    .map_err(sql_to_io_error)?;
    let created_at = Utc::now();
    let uuid = Uuid::new_v4();
    queries::insert_new_savefile_meta(&conn, name, created_at, uuid).map_err(sql_to_io_error)?;
    conn.close().map_err(|(_, e)| sql_to_io_error(e))?;
    let db_meta = fs::metadata(&save_db_path)?;
    let modified_at = db_meta.modified().map(DateTime::<Utc>::from).unwrap_or_default();
    Ok(SavefileMetadata {
        location: SavefileLocation::Path(save_dir),
        name: name.to_owned(),
        dir_name: final_dir_name,
        disk_size: db_meta.len(),
        created_at,
        modified_at,
        uuid,
    })
}

#[cfg(test)]
mod test {
    use anyhow::Result;
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn savefile_manipulation() -> Result<()> {
        let tmpdir = tempdir()?;
        let saves_path = tmpdir.path().join("saves");
        ensure_writable_dir(&saves_path)?;

        let (saves, errs) = list_saves(&saves_path);
        errs.into_result()?;
        assert!(saves.is_empty());

        let sv1_meta = new_save(&saves_path, "Save 1")?;
        let sv2_meta = new_save(&saves_path, "Save 2")?;
        let sv3_meta = new_save(&saves_path, "Save 3")?;

        let (mut saves, errs) = list_saves(&saves_path);
        errs.into_result()?;
        saves.sort_by_cached_key(|s| s.name.clone());
        assert_eq!([sv1_meta.clone(), sv2_meta.clone(), sv3_meta.clone()], &saves[..]);

        fs::remove_dir_all(sv2_meta.location.path().unwrap())?;

        let (mut saves, errs) = list_saves(&saves_path);
        errs.into_result()?;
        saves.sort_by_cached_key(|s| s.name.clone());
        assert_eq!([sv1_meta.clone(), sv3_meta.clone()], &saves[..]);

        tmpdir.close()?;

        Ok(())
    }
}
