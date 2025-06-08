//! Typed queries on the database format for convenience and testing.
//!
//! Development notes:
//!  - Add `// language=sqlite` before SQL query strings to enable syntax highlighting and query checking in Jetbrains IDEs
//!  - Use `sqlite3 -readonly lib/gs_schemas/src/savefile/sql/0000_new_game.sqlite` to access the template DB for local query explaining

use std::path::Path;
use std::rc::Rc;
use std::time::{Duration, UNIX_EPOCH};

use chrono::{DateTime, Utc};
use itertools::Itertools;
use rusqlite::types::Value;
use rusqlite::{Connection, MAIN_DB, OpenFlags, Result, Transaction, named_params, params};
use uuid::Uuid;

use super::sql;
use super::{SAVEFILE_META_CREATED_AT_UTC_KEY, SAVEFILE_META_NAME_KEY, SAVEFILE_META_UNIVERSE_UUID_KEY};
use crate::coordinates::AbsChunkPos;
use crate::registry::{RegistryId, RegistryName, RegistryNameRef};
use crate::schemas::AlignedBytesMut;

/// Creates an empty savefile in-memory DB of the latest version for testing.
pub fn create_test_memory_db() -> Result<Connection> {
    let mut conn = Connection::open_in_memory_with_flags(
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX | OpenFlags::SQLITE_OPEN_EXRESCODE,
    )?;
    rusqlite::vtab::array::load_module(&conn)?;
    conn.execute_batch(sql::SQL_0000_INIT)?;
    conn.set_prepared_statement_cache_capacity(64);
    let data =
        zstd::decode_all(sql::SQL_0000_NEW_GAME_TEMPLATE_ZST).expect("Could not decompress builtin savefile template");
    conn.deserialize_read_exact(MAIN_DB, &data[..], data.len(), false)?;
    Ok(conn)
}

/// Opens a sqlite connection to the given path and initializes it with the SQL prelude.
pub fn open_rw_connection(path: &Path) -> Result<Connection> {
    let conn = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX | OpenFlags::SQLITE_OPEN_EXRESCODE,
    )?;
    rusqlite::vtab::array::load_module(&conn)?;
    conn.execute_batch(sql::SQL_0000_INIT)?;
    conn.set_prepared_statement_cache_capacity(64);
    Ok(conn)
}

/// Closes a sqlite connection, running the quit SQL before dropping the connection object.
pub fn close_rw_connection(db: Connection) -> Result<()> {
    db.execute_batch(sql::SQL_0000_QUIT)?;
    db.close().map_err(|(_, e)| e)
}

/// Gets the schema version of the savefile.
pub fn select_savefile_schema_version(db: &Connection) -> Result<i32> {
    // Do not set language=sqlite here because pragma tables don't appear in the schema
    db.query_row("SELECT user_version FROM pragma_user_version;", [], |row| {
        row.get::<_, i32>(0)
    })
}

/// Gets the display name of the savefile.
pub fn select_savefile_meta_name(db: &Connection) -> Result<String> {
    db.query_row(
        // language=sqlite
        "SELECT field_name, field_value FROM geosia_savefile_metadata WHERE field_name=?1;",
        params![SAVEFILE_META_NAME_KEY],
        |row| row.get::<_, String>(1),
    )
}

/// Gets the UTC timestamp of the savefile creation.
pub fn select_savefile_meta_created_at(db: &Connection) -> Result<DateTime<Utc>> {
    db.query_row(
        // language=sqlite
        "SELECT field_name, field_value FROM geosia_savefile_metadata WHERE field_name=?1;",
        params![SAVEFILE_META_CREATED_AT_UTC_KEY],
        |row| row.get::<_, DateTime<Utc>>(1),
    )
}

/// Gets the universe UUID of the savefile.
pub fn select_savefile_meta_universe_uuid(db: &Connection) -> Result<Uuid> {
    let id_str = db.query_row(
        // language=sqlite
        "SELECT field_name, field_value FROM geosia_savefile_metadata WHERE field_name=?1;",
        params![SAVEFILE_META_UNIVERSE_UUID_KEY],
        |row| row.get::<_, String>(1),
    )?;
    Uuid::parse_str(&id_str).map_err(|e| rusqlite::Error::ToSqlConversionFailure(e.into()))
}

/// Overwrites the savefile metadata for a brand new savefile.
pub fn insert_new_savefile_meta(db: &Connection, name: &str, created_at: DateTime<Utc>, uuid: Uuid) -> Result<()> {
    let uuid = format!("{}", uuid.hyphenated());
    db.execute(
        // language=sqlite
        "INSERT INTO geosia_savefile_metadata (field_name, field_value)
        VALUES (:name_key, :name),
            (:created_at_key, :created_at),
            (:uuid_key, :uuid);",
        named_params! {
            ":name_key": SAVEFILE_META_NAME_KEY,
            ":name": name,
            ":created_at_key": SAVEFILE_META_CREATED_AT_UTC_KEY,
            ":created_at": created_at,
            ":uuid_key": SAVEFILE_META_UNIVERSE_UUID_KEY,
            ":uuid": uuid,
        },
    )?;
    Ok(())
}

/// Adds the given entries to the critical operations log on the savefile.
pub fn insert_critical_log_entries<S: AsRef<str>>(db: &Transaction, entries: &[S]) -> Result<()> {
    if entries.is_empty() {
        return Ok(());
    }
    let timestamp = UNIX_EPOCH
        .elapsed()
        .as_ref()
        .map(Duration::as_secs_f64)
        .unwrap_or(0.0f64);
    // language=sqlite
    let mut stmt =
        db.prepare_cached("INSERT INTO geosia_critical_log (unix_timestamp, log_message) VALUES (?1, ?2);")?;
    for msg in entries {
        stmt.execute(params![timestamp, msg.as_ref()])?;
    }
    Ok(())
}

/// Queries all the registry mappings from a given named registry.
pub fn select_registry_entries(db: &Connection, registry: RegistryNameRef) -> Result<Vec<(RegistryId, RegistryName)>> {
    // language=sqlite
    let mut stmt = db.prepare_cached(
        "SELECT entry_name, entry_id, entry_registry, registry_name FROM geosia_registry_entries
        INNER JOIN geosia_registry_types on registry_id = entry_registry
        WHERE registry_name = ?1;",
    )?;
    let mut query = stmt.query(params![registry])?;
    let mut output: Vec<(RegistryId, RegistryName)> = Vec::new();
    while let Some(x) = query.next()? {
        let entry_name: RegistryName = x.get(0)?;
        let entry_id: RegistryId = x.get(1)?;
        output.push((entry_id, entry_name));
    }
    Ok(output)
}

/// Rewrites the registry entries for the given registry to match the given ID-Name set.
/// All updates are logged in the critical update log.
/// Returns the number of modified entries and log entries for all modifications.
pub fn update_registry_entries<'names>(
    db: &Transaction,
    registry: RegistryNameRef,
    entries: impl Iterator<Item = (RegistryId, RegistryNameRef<'names>)>,
) -> Result<(usize, Vec<String>)> {
    let mut log_entries: Vec<String> = Vec::new();
    let mut modified = 0usize;
    // language=sqlite
    db.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS temp.update_registry_entries (
            new_id INTEGER NOT NULL PRIMARY KEY,
            new_name TEXT NOT NULL UNIQUE
        ) STRICT;
        DELETE FROM temp.update_registry_entries;
        ",
    )?;
    {
        // language=sqlite
        let mut lut_insert =
            db.prepare_cached("INSERT INTO temp.update_registry_entries (new_id, new_name) VALUES (?1, ?2)")?;
        for (new_id, new_name) in entries {
            lut_insert.execute(params![new_id, new_name])?;
        }
    }

    // Get registry foreign key
    let reg_key: i64 = match db.query_row(
        // language=sqlite
        "SELECT registry_id, registry_name FROM geosia_registry_types WHERE registry_name = ?1;",
        params![registry],
        |r| r.get(0),
    ) {
        Ok(key) => key,
        Err(rusqlite::Error::QueryReturnedNoRows) => {
            db.query_row(
                // language=sqlite
                "INSERT INTO geosia_registry_types (registry_name) VALUES (?1) RETURNING registry_id;",
                params![registry],
                |r| r.get(0),
            )?
        }
        Err(e) => return Err(e),
    };

    // Delete now-missing IDs
    {
        // language=sqlite
        let mut delete_stmt = db.prepare_cached(
            "DELETE FROM geosia_registry_entries
            WHERE entry_registry = ?1
                AND NOT EXISTS (SELECT 1 FROM temp.update_registry_entries
                                         WHERE temp.update_registry_entries.new_id = geosia_registry_entries.entry_id)
            RETURNING entry_id, entry_name;",
        )?;
        let mut q = delete_stmt.query(params![reg_key])?;
        while let Some(row) = q.next()? {
            let deleted_id: RegistryId = row.get(0)?;
            let deleted_name: RegistryName = row.get(1)?;
            log_entries.push(format!(
                "DeleteRegistryEntry(registry={registry}, id={deleted_id}, name={deleted_name})"
            ));
            modified += 1;
        }
    }

    // Delete renamed IDs, do not count as modified because they're about to be re-inserted
    // This step is required to not violate UNIQUE constraints mid-execution during the insert
    {
        // language=sqlite
        let mut delete_stmt = db.prepare_cached(
            "DELETE FROM geosia_registry_entries
            WHERE entry_registry = ?1
                AND EXISTS (SELECT 1 FROM temp.update_registry_entries
                                     WHERE temp.update_registry_entries.new_id = geosia_registry_entries.entry_id AND temp.update_registry_entries.new_name != geosia_registry_entries.entry_name)
            RETURNING entry_id, entry_name;",
        )?;
        let mut q = delete_stmt.query(params![reg_key])?;
        while let Some(row) = q.next()? {
            let deleted_id: RegistryId = row.get(0)?;
            let deleted_name: RegistryName = row.get(1)?;
            log_entries.push(format!(
                "DeleteRegistryEntryForOverwrite(registry={registry}, id={deleted_id}, name={deleted_name})"
            ));
        }
    }

    // Update remaining IDs
    {
        // language=sqlite
        let mut update_stmt = db.prepare_cached("INSERT INTO geosia_registry_entries
            (entry_registry, entry_id, entry_name) SELECT ?1, new_id, new_name FROM temp.update_registry_entries
            WHERE TRUE
            ON CONFLICT(entry_registry, entry_id) DO UPDATE SET entry_name=excluded.entry_name WHERE entry_name != excluded.entry_name
            RETURNING entry_id, entry_name;")?;
        let mut q = update_stmt.query(params![reg_key])?;
        while let Some(row) = q.next()? {
            let new_id: RegistryId = row.get(0)?;
            let new_name: RegistryName = row.get(1)?;
            log_entries.push(format!(
                "UpdateRegistryEntry(registry={registry}, id={new_id}, name={new_name})"
            ));
            modified += 1;
        }
    }

    // language=sqlite
    db.execute_batch("DELETE FROM temp.update_registry_entries;")?;
    insert_critical_log_entries(db, &log_entries)?;
    Ok((modified, log_entries))
}

/// A database query result for a given chunk's serialized data.
#[derive(Clone, Hash, Debug, Eq, PartialEq)]
pub enum ReadChunkResult {
    /// There was no chunk stored with the given position.
    Missing(AbsChunkPos),
    /// There was a chunk stored with the given position.
    Present {
        /// The position of the stored chunk.
        position: AbsChunkPos,
        /// The serialized data, aligned to a 8 byte boundary.
        data: AlignedBytesMut,
    },
}

impl ReadChunkResult {
    /// Gets the position of the read chunk.
    pub fn position(&self) -> AbsChunkPos {
        match self {
            ReadChunkResult::Missing(position) => *position,
            ReadChunkResult::Present { position, .. } => *position,
        }
    }
}

/// Attempts to read the data for chunks at all the given positions, returns a [`ReadChunkResult`] for every entry in the positions array (not necessarily in order).
pub fn try_read_chunks(db: &Connection, positions: &[AbsChunkPos]) -> Result<Vec<ReadChunkResult>> {
    // TODO: This is some horrible allocation spam, we might need to switch this to the C api.
    let input_positions: Vec<Value> = positions.iter().map(|p| p.into()).collect_vec();
    let input_positions = Rc::new(input_positions);
    // language=sqlite
    let mut query_stmt = db.prepare_cached(
        "SELECT inputs.value, chunks.packed_coordinates, chunks.chunk_data
            FROM rarray(?1) AS inputs
            LEFT JOIN geosia_chunks AS chunks ON inputs.value = chunks.packed_coordinates;",
    )?;
    let mut q = query_stmt.query([input_positions])?;
    let mut read_results = Vec::with_capacity(positions.len());
    while let Some(row) = q.next()? {
        let input_position: AbsChunkPos = row.get(0)?;
        let output_position: Option<AbsChunkPos> = row.get(1)?;
        let Some(output_position) = output_position else {
            read_results.push(ReadChunkResult::Missing(input_position));
            continue;
        };
        // This should only misbehave if the SQL query is wrong.
        debug_assert_eq!(input_position, output_position);
        let chunk_data_unaligned: Option<Vec<u8>> = row.get(2)?;
        let Some(chunk_data_unaligned) = chunk_data_unaligned else {
            read_results.push(ReadChunkResult::Missing(input_position));
            continue;
        };
        read_results.push(ReadChunkResult::Present {
            position: output_position,
            data: AlignedBytesMut::from_bytes(&chunk_data_unaligned),
        });
    }
    Ok(read_results)
}

/// A query parameter for writing new chunk data to the database.
pub struct OverwriteChunkRequest {
    /// The position of the chunk to overwrite.
    pub position: AbsChunkPos,
    /// The new data for the chunk.
    pub data: Vec<u8>,
}

/// Overwrites the chunks at the given positions with new data, returns the number of rows written.
pub fn overwrite_chunks(
    tx: &mut Transaction,
    write_requests: impl Iterator<Item = OverwriteChunkRequest>,
) -> Result<usize> {
    // language=sqlite
    let mut q = tx.prepare_cached(
        "INSERT INTO geosia_chunks (packed_coordinates, chunk_data)
        VALUES (?1, ?2)
        ON CONFLICT(packed_coordinates) DO UPDATE SET chunk_data=excluded.chunk_data;",
    )?;
    let mut changes = 0;
    for OverwriteChunkRequest { position, data } in write_requests {
        changes += q.execute(params!(position, data))?;
    }
    Ok(changes)
}

#[cfg(test)]
mod test {
    use anyhow::Result;
    use rusqlite::TransactionBehavior;

    use super::*;

    #[test]
    fn meta_storage() -> Result<()> {
        let name = "abc";
        let made_at = Utc::now();
        let uuid = Uuid::new_v4();

        let db = create_test_memory_db()?;
        insert_new_savefile_meta(&db, name, made_at, uuid)?;
        assert_eq!(name, select_savefile_meta_name(&db)?);
        assert_eq!(made_at, select_savefile_meta_created_at(&db)?);
        assert_eq!(uuid, select_savefile_meta_universe_uuid(&db)?);

        close_rw_connection(db)?;
        Ok(())
    }

    #[test]
    fn registry_storage() -> Result<()> {
        fn id_sorted_entries(tx: &Transaction, reg_name: RegistryNameRef) -> Result<Vec<(RegistryId, RegistryName)>> {
            let mut out = select_registry_entries(tx, reg_name)?;
            out.sort_by_key(|e| e.0);
            Ok(out)
        }

        let reg_name = const { RegistryName::gs_const("test_ids") };
        let reg_ref = reg_name.as_ref();

        let id_1 = const { RegistryId::try_new(1).unwrap() };
        let id_2 = const { RegistryId::try_new(2).unwrap() };
        let obj_a = const { RegistryName::gs_const("a") };
        let obj_b = const { RegistryName::gs_const("b") };
        let ref_a = obj_a.as_ref();
        let ref_b = obj_b.as_ref();

        let mut db = create_test_memory_db()?;

        {
            let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
            assert_eq!(0, select_registry_entries(&tx, reg_ref)?.len());
            assert_eq!(1, update_registry_entries(&tx, reg_ref, [(id_1, ref_a)].into_iter())?.0);
            assert_eq!([(id_1, obj_a.clone())], &id_sorted_entries(&tx, reg_ref)?[..]);
            assert_eq!(0, update_registry_entries(&tx, reg_ref, [(id_1, ref_a)].into_iter())?.0);
            assert_eq!([(id_1, obj_a.clone())], &id_sorted_entries(&tx, reg_ref)?[..]);
            assert_eq!(2, update_registry_entries(&tx, reg_ref, [(id_2, ref_b)].into_iter())?.0);
            assert_eq!([(id_2, obj_b.clone())], &id_sorted_entries(&tx, reg_ref)?[..]);
            assert_eq!(0, update_registry_entries(&tx, reg_ref, [(id_2, ref_b)].into_iter())?.0);
            assert_eq!(
                1,
                update_registry_entries(&tx, reg_ref, [(id_1, ref_a), (id_2, ref_b)].into_iter())?.0
            );
            assert_eq!(
                [(id_1, obj_a.clone()), (id_2, obj_b.clone())],
                &id_sorted_entries(&tx, reg_ref)?[..]
            );
            assert_eq!(
                0,
                update_registry_entries(&tx, reg_ref, [(id_1, ref_a), (id_2, ref_b)].into_iter())?.0
            );
            assert_eq!(
                [(id_1, obj_a.clone()), (id_2, obj_b.clone())],
                &id_sorted_entries(&tx, reg_ref)?[..]
            );
            // swap keys
            assert_eq!(
                2,
                update_registry_entries(&tx, reg_ref, [(id_2, ref_a), (id_1, ref_b)].into_iter())?.0
            );
            assert_eq!(
                [(id_1, obj_b.clone()), (id_2, obj_a.clone())],
                &id_sorted_entries(&tx, reg_ref)?[..]
            );
        }

        close_rw_connection(db)?;
        Ok(())
    }

    #[test]
    fn chunk_storage() -> Result<()> {
        let mut db = create_test_memory_db()?;
        let c0 = AbsChunkPos::ZERO;
        let c1 = AbsChunkPos::X;
        assert!(c0 < c1);

        let mut tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        // Ensure no chunks are saved in a blank db
        {
            let mut result = try_read_chunks(&tx, &[c0, c1])?;
            result.sort_by_key(ReadChunkResult::position);
            assert_eq!(vec![ReadChunkResult::Missing(c0), ReadChunkResult::Missing(c1)], result);
        }
        // Save 1 chunk
        {
            let modified = overwrite_chunks(
                &mut tx,
                [OverwriteChunkRequest {
                    position: c0,
                    data: vec![0],
                }]
                .into_iter(),
            )?;
            assert_eq!(1, modified);
        }
        // Read 2 chunks
        {
            let mut result = try_read_chunks(&tx, &[c0, c1])?;
            result.sort_by_key(ReadChunkResult::position);
            assert_eq!(
                vec![
                    ReadChunkResult::Present {
                        position: c0,
                        data: vec![0].into()
                    },
                    ReadChunkResult::Missing(c1)
                ],
                result
            );
        }
        // Overwrite 1 chunk, save 1 new chunk
        {
            let modified = overwrite_chunks(
                &mut tx,
                [
                    OverwriteChunkRequest {
                        position: c0,
                        data: vec![1],
                    },
                    OverwriteChunkRequest {
                        position: c1,
                        data: vec![1],
                    },
                ]
                .into_iter(),
            )?;
            assert_eq!(2, modified);
        }
        // Read 2 chunks
        {
            let mut result = try_read_chunks(&tx, &[c0, c1])?;
            result.sort_by_key(ReadChunkResult::position);
            assert_eq!(
                vec![
                    ReadChunkResult::Present {
                        position: c0,
                        data: vec![1].into()
                    },
                    ReadChunkResult::Present {
                        position: c1,
                        data: vec![1].into()
                    }
                ],
                result
            );
        }
        // Overwrite 2 chunks, keeping 1 identical
        {
            let modified = overwrite_chunks(
                &mut tx,
                [
                    OverwriteChunkRequest {
                        position: c0,
                        data: vec![1],
                    },
                    OverwriteChunkRequest {
                        position: c1,
                        data: vec![2],
                    },
                ]
                .into_iter(),
            )?;
            assert_eq!(2, modified);
        }
        // Read 2 chunks
        {
            let mut result = try_read_chunks(&tx, &[c0, c1])?;
            result.sort_by_key(ReadChunkResult::position);
            assert_eq!(
                vec![
                    ReadChunkResult::Present {
                        position: c0,
                        data: vec![1].into()
                    },
                    ReadChunkResult::Present {
                        position: c1,
                        data: vec![2].into()
                    }
                ],
                result
            );
        }
        // Overwrite 1 chunk twice, make sure last write wins
        {
            let modified = overwrite_chunks(
                &mut tx,
                [
                    OverwriteChunkRequest {
                        position: c0,
                        data: vec![3],
                    },
                    OverwriteChunkRequest {
                        position: c0,
                        data: vec![4],
                    },
                ]
                .into_iter(),
            )?;
            assert_eq!(2, modified);
        }
        // Read 2 chunks
        {
            let mut result = try_read_chunks(&tx, &[c0, c1])?;
            result.sort_by_key(ReadChunkResult::position);
            assert_eq!(
                vec![
                    ReadChunkResult::Present {
                        position: c0,
                        data: vec![4].into()
                    },
                    ReadChunkResult::Present {
                        position: c1,
                        data: vec![2].into()
                    }
                ],
                result
            );
        }

        drop(tx);
        close_rw_connection(db)?;
        Ok(())
    }
}
