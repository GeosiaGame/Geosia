-- Ran before closing a database to clean up the data

PRAGMA main.incremental_vacuum;
PRAGMA main.optimize;
PRAGMA main.wal_checkpoint(RESTART);
