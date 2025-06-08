//! Runs capnp codegen for the crate.

use std::{
    collections::BTreeMap,
    error::Error,
    ffi::OsStr,
    fmt::Write,
    fs::DirEntry,
    path::{Path, PathBuf},
};

type Result<T, E = Box<dyn Error>> = std::result::Result<T, E>;

fn main() -> Result<()> {
    regen_sql()?;
    regen_capnproto();
    Ok(())
}

struct SqlMigrationFile {
    path: PathBuf,
    var_name: String,
}

fn write_file_if_changed(path: &Path, contents: &[u8]) -> std::io::Result<()> {
    let old_contents: Vec<u8> = std::fs::read(path).unwrap_or_default();
    if &old_contents[..] != contents {
        std::fs::write(path, contents)?;
    }
    Ok(())
}

fn regen_sql() -> Result<()> {
    let sql_dir = Path::new("src/savefile/sql");
    let sql_rs_path = Path::new("src/savefile/sql.rs");
    // Used for intellisense
    let new_game_template_path_uncompressed = Path::new("src/savefile/sql/0000_new_game.sqlite");
    let new_game_template_path = Path::new("src/savefile/sql/0000_new_game.sqlite.zst");
    build::rerun_if_changed(sql_dir);

    let mut sql_rs =
        String::from("//! Auto-generated module including all the SQL sources as static string literals.\n");

    let mut migrations: BTreeMap<i32, SqlMigrationFile> = BTreeMap::new();

    let mut dir_entries: Vec<DirEntry> = std::fs::read_dir(sql_dir)?.filter_map(Result::ok).collect::<Vec<_>>();
    dir_entries.sort_by_key(|de| de.file_name());

    writeln!(&mut sql_rs, "\n/// Template database data for a new game savefile").unwrap();
    writeln!(
        &mut sql_rs,
        r#"pub static SQL_0000_NEW_GAME_TEMPLATE_ZST: &[u8] = include_bytes!("sql/0000_new_game.sqlite.zst");"#
    )
    .unwrap();

    for de in dir_entries {
        if !de.file_type()?.is_file() {
            continue;
        }
        let path = de.path();
        if path.extension() != Some(OsStr::new("sql")) {
            continue;
        }
        let file_name = path.file_name().unwrap().to_os_string().into_string().unwrap();
        let const_name = path
            .file_stem()
            .expect("SQL source file without an extension")
            .to_string_lossy()
            .to_uppercase();
        let var_name = format!("SQL_{const_name}");

        let doc_comment = std::fs::read_to_string(&path)?
            .lines()
            .map(|l| l.trim())
            .filter_map(|l| l.strip_prefix("-- ").map(str::to_owned))
            .next();
        let Some(doc_comment) = doc_comment else {
            panic!("Doc comment missing from {}", path.to_string_lossy())
        };

        writeln!(&mut sql_rs, "\n/// {doc_comment}").unwrap();
        writeln!(
            &mut sql_rs,
            r#"pub static {var_name}: &str = include_str!("sql/{file_name}");"#
        )
        .unwrap();

        let migration_version: i32 = file_name
            .split_once('_')
            .unwrap()
            .0
            .parse()
            .expect("Could not parse sql migration number");
        if migration_version == 0 {
            continue;
        }

        let migration_data = SqlMigrationFile { path, var_name };
        if migrations.insert(migration_version, migration_data).is_some() {
            panic!("Duplicate SQL migrations for version {migration_version}");
        }
    }

    writeln!(
        &mut sql_rs,
        "\n/// All migrations in order, with their schema version number"
    )
    .unwrap();
    writeln!(
        &mut sql_rs,
        "pub static SQL_MIGRATIONS: [(i32, &str); {count}] = [",
        count = migrations.len()
    )
    .unwrap();
    for (&i, migration) in &migrations {
        writeln!(&mut sql_rs, "    ({i}i32, {var})", var = &migration.var_name).unwrap();
    }
    writeln!(&mut sql_rs, "];").unwrap();

    write_file_if_changed(sql_rs_path, sql_rs.as_bytes())?;

    // Test all migrations
    let conn = rusqlite::Connection::open_in_memory()?;
    conn.execute_batch(&std::fs::read_to_string("src/savefile/sql/0000_init.sql")?)?;
    for migration in migrations.values() {
        conn.execute_batch(&std::fs::read_to_string(&migration.path)?)?;
    }
    conn.execute_batch(&std::fs::read_to_string("src/savefile/sql/0000_quit.sql")?)?;
    conn.execute_batch("VACUUM;")?;
    // Save new savefile template to bytes
    let db_data = conn.serialize(rusqlite::MAIN_DB)?;
    write_file_if_changed(new_game_template_path_uncompressed, &db_data)?;
    let db_zstd_data = zstd::encode_all(&db_data as &[u8], 3)?;
    write_file_if_changed(new_game_template_path, &db_zstd_data)?;

    conn.close().map_err(|(_, e)| e)?;

    Ok(())
}

fn regen_capnproto() {
    build::rerun_if_changed("capnp");
    #[cfg(feature = "regenerate-capnp")]
    {
        use std::path::Path;

        use capnpc::CompilerCommand as Capnp;

        let generated = Path::new("capnp-generated/");

        if generated.is_dir() {
            std::fs::remove_dir_all(generated).expect("Could not clear the capnp-generated directory");
        }

        Capnp::new()
            .src_prefix("capnp/")
            .import_path("capnp/")
            .output_path(generated)
            .file("capnp/game_types.capnp")
            .file("capnp/network.capnp")
            .file("capnp/voxel_mesh.capnp")
            .run()
            .expect("compiling capnp schema");
    }
}
