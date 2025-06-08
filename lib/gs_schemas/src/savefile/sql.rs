//! Auto-generated module including all the SQL sources as static string literals.

/// Template database data for a new game savefile
pub static SQL_0000_NEW_GAME_TEMPLATE_ZST: &[u8] = include_bytes!("sql/0000_new_game.sqlite.zst");

/// Ran on database initialization, configures the DB engine
pub static SQL_0000_INIT: &str = include_str!("sql/0000_init.sql");

/// Ran before closing a database to clean up the data
pub static SQL_0000_QUIT: &str = include_str!("sql/0000_quit.sql");

/// Initial table setup
pub static SQL_0001_DB_SETUP: &str = include_str!("sql/0001_db_setup.sql");

/// All migrations in order, with their schema version number
pub static SQL_MIGRATIONS: [(i32, &str); 1] = [
    (1i32, SQL_0001_DB_SETUP)
];
