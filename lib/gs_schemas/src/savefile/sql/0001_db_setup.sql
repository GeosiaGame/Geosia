-- Initial table setup

BEGIN EXCLUSIVE TRANSACTION;

PRAGMA user_version = 1;

CREATE TABLE geosia_savefile_metadata (
    field_name TEXT NOT NULL PRIMARY KEY ON CONFLICT REPLACE,
    field_value TEXT
) STRICT;

CREATE TABLE geosia_registry_types (
    registry_id INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
    registry_name TEXT NOT NULL UNIQUE
) STRICT;

CREATE TABLE geosia_registry_entries (
    entry_rowid INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
    entry_registry INTEGER NOT NULL,
    entry_name TEXT NOT NULL,
    entry_id INTEGER NOT NULL,
    UNIQUE(entry_registry, entry_name),
    UNIQUE(entry_registry, entry_id),
    FOREIGN KEY(entry_registry) REFERENCES geosia_registry_types(registry_id) ON UPDATE CASCADE ON DELETE CASCADE
) STRICT;

CREATE TABLE geosia_chunks (
    packed_coordinates BLOB NOT NULL PRIMARY KEY, -- z-packed i128 AbsChunkPos
    voxel_data BLOB,
    entity_data BLOB
) STRICT;

CREATE TABLE geosia_global_entities (
    entity_id INTEGER NOT NULL PRIMARY KEY,
    entity_data BLOB NOT NULL
) STRICT;

COMMIT TRANSACTION;
