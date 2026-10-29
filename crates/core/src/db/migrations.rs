//! Schema migrations. Each migration runs once, in its own transaction; the schema version is
//! SQLite's `user_version`. Migrations are append-only: never edit one that has shipped.

use rusqlite::Connection;

use crate::{Error, Result};

const MIGRATIONS: &[&str] = &[
    include_str!("migrations/0001_initial.sql"),
    include_str!("migrations/0002_mail_html.sql"),
];

/// Schema version after all migrations of this build have run.
pub fn latest_version() -> u32 {
    u32::try_from(MIGRATIONS.len()).expect("fewer than 2^32 migrations")
}

pub fn schema_version(conn: &Connection) -> Result<u32> {
    Ok(conn.pragma_query_value(None, "user_version", |row| row.get(0))?)
}

/// Brings the schema up to date. Refuses databases written by a newer app, so an older
/// version never touches data it does not understand.
pub fn migrate(conn: &mut Connection) -> Result<()> {
    let current = schema_version(conn)?;
    let latest = latest_version();
    if current > latest {
        return Err(Error::SchemaTooNew {
            found: current,
            supported: latest,
        });
    }

    for (index, sql) in MIGRATIONS.iter().enumerate().skip(current as usize) {
        let version = u32::try_from(index + 1).expect("checked by latest_version");
        let apply = |conn: &mut Connection| -> rusqlite::Result<()> {
            let tx = conn.transaction()?;
            tx.execute_batch(sql)?;
            tx.pragma_update(None, "user_version", version)?;
            tx.commit()
        };
        apply(conn).map_err(|source| Error::Migration { version, source })?;
        tracing::info!(version, "applied database migration");
    }
    Ok(())
}
