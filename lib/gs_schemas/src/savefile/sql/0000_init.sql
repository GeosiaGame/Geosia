-- Ran on database initialization, configures the DB engine

PRAGMA application_id = 0x47735366; -- GsSf - Geosia Savefile
PRAGMA auto_vacuum = INCREMENTAL; -- needs manual trigger by PRAGMA incremental_vacuum;
PRAGMA automatic_index = 0;
PRAGMA cache_size = -8000;
PRAGMA cell_size_check = 1;
PRAGMA encoding = 'UTF-8';
PRAGMA foreign_keys = 1;
PRAGMA fullfsync = 1;
PRAGMA journal_mode = WAL;
PRAGMA journal_size_limit = 16777216;
PRAGMA mmap_size = 1073741824; -- 1 GiB
PRAGMA page_size = 4096;
PRAGMA recursive_triggers = 1;
PRAGMA synchronous = NORMAL;
PRAGMA trusted_schema = 0;

PRAGMA main.optimize=0x10002;
