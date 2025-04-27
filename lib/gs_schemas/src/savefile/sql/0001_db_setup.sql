-- Initial table setup

BEGIN EXCLUSIVE TRANSACTION;

-- Currently applied migration number is stored here.
-- Each new change to the savefile format must create a new .sql number with a higher filename number and a matching PRAGMA like this.
PRAGMA user_version = 1;

-- Simple RegistryName key - value store for arbitrary savefile metadata
CREATE TABLE geosia_savefile_metadata (
    field_name TEXT NOT NULL PRIMARY KEY ON CONFLICT REPLACE,
    field_value TEXT
) STRICT;

INSERT INTO geosia_savefile_metadata (field_name, field_value)
    VALUES ('gs:name', 'New Savefile');

-- Logs about changes done to the savefile for analysing corruption and changes to the database over time.
CREATE TABLE geosia_critical_log(
    unix_timestamp REAL NOT NULL,
    log_message TEXT NOT NULL,
) STRICT;

-- RegistryName keys for various ID registry types, like gs:blocks.
CREATE TABLE geosia_registry_types (
    registry_id INTEGER NOT NULL PRIMARY KEY,
    registry_name TEXT NOT NULL UNIQUE
) STRICT;

-- Entry storage for each registry type, keeping track of the numeric ID-RegistryName ID mappings.
CREATE TABLE geosia_registry_entries (
    entry_rowid INTEGER NOT NULL PRIMARY KEY,
    entry_registry INTEGER NOT NULL,
    entry_name TEXT NOT NULL,
    entry_id INTEGER NOT NULL,
    UNIQUE(entry_registry, entry_name),
    UNIQUE(entry_registry, entry_id),
    FOREIGN KEY(entry_registry) REFERENCES geosia_registry_types(registry_id) ON UPDATE CASCADE ON DELETE CASCADE
) STRICT;

-- Per-chunk data store.
CREATE TABLE geosia_chunks (
    packed_coordinates BLOB NOT NULL PRIMARY KEY, -- z-packed i128 AbsChunkPos
    voxel_data BLOB,
    entity_data BLOB
) STRICT;

-- Storage for entities that should be globally loaded and are not tied to a particular chunk.
CREATE TABLE geosia_global_entities (
    entity_id INTEGER NOT NULL PRIMARY KEY,
    entity_data BLOB NOT NULL
) STRICT;

COMMIT TRANSACTION;
