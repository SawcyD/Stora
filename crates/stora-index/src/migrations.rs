use rusqlite::Connection;
use stora_core::{Result, StoraError};

/// Ordered migration list. Migrations are embedded at compile time so a
/// deployed `stora.exe` never depends on files next to it.
///
/// Append only — never edit a shipped migration.
const MIGRATIONS: &[(i32, &str, &str)] = &[
    (
        1,
        "initial",
        include_str!("../../../migrations/001_initial.sql"),
    ),
    (
        2,
        "scan_categories",
        include_str!("../../../migrations/002_scan_categories.sql"),
    ),
    (
        3,
        "applications",
        include_str!("../../../migrations/003_applications.sql"),
    ),
    (
        4,
        "automation",
        include_str!("../../../migrations/004_automation.sql"),
    ),
    (
        5,
        "knowledge",
        include_str!("../../../migrations/005_knowledge.sql"),
    ),
];

pub fn latest_version() -> i32 {
    MIGRATIONS
        .last()
        .map(|(version, _, _)| *version)
        .unwrap_or(0)
}

/// Applies every migration newer than the database's current `user_version`.
///
/// Each migration runs inside its own transaction, so a failure leaves the
/// database at the last version that fully applied.
pub fn run(connection: &mut Connection) -> Result<i32> {
    let current: i32 = connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .map_err(map_err)?;

    let mut applied = current;

    for (version, name, sql) in MIGRATIONS {
        if *version <= current {
            continue;
        }

        let tx = connection.transaction().map_err(map_err)?;
        tx.execute_batch(sql).map_err(|err| {
            StoraError::Database(format!("migration {version} ({name}) failed: {err}"))
        })?;
        // PRAGMA does not accept a bound parameter; `version` is a compile-time
        // constant from this file, so the format is safe.
        tx.pragma_update(None, "user_version", version)
            .map_err(map_err)?;
        tx.commit().map_err(map_err)?;

        tracing::info!(version, name, "applied migration");
        applied = *version;
    }

    Ok(applied)
}

pub(crate) fn map_err(err: rusqlite::Error) -> StoraError {
    match err {
        rusqlite::Error::SqliteFailure(inner, _)
            if inner.code == rusqlite::ErrorCode::DatabaseBusy
                || inner.code == rusqlite::ErrorCode::DatabaseLocked =>
        {
            StoraError::DatabaseBusy
        }
        other => StoraError::Database(other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn memory_db() -> Connection {
        Connection::open_in_memory().unwrap()
    }

    #[test]
    fn applies_all_migrations_from_empty() {
        let mut db = memory_db();
        let version = run(&mut db).unwrap();
        assert_eq!(version, latest_version());

        let stored: i32 = db
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(stored, latest_version());
    }

    #[test]
    fn is_idempotent() {
        let mut db = memory_db();
        run(&mut db).unwrap();
        // A second run must not attempt to recreate existing tables.
        let version = run(&mut db).unwrap();
        assert_eq!(version, latest_version());
    }

    #[test]
    fn creates_every_expected_table() {
        let mut db = memory_db();
        run(&mut db).unwrap();

        let expected = [
            "drives",
            "scans",
            "scan_entries",
            "scan_categories",
            "folder_aggregates",
            "applications",
            "application_activity",
            "automation_rules",
            "automation_runs",
            "folder_snapshots",
            "knowledge_entries",
            "scan_errors",
            "cleanup_operations",
            "cleanup_items",
            "quarantine_items",
            "exclusions",
            "settings",
        ];

        for table in expected {
            let count: i64 = db
                .query_row(
                    "SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
                    [table],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(count, 1, "missing table: {table}");
        }
    }

    #[test]
    fn creates_the_indexes_the_ui_queries_depend_on() {
        let mut db = memory_db();
        run(&mut db).unwrap();

        for index in [
            "idx_entries_scan_parent",
            "idx_entries_scan_size",
            "idx_aggregates_parent",
            "idx_cleanup_started",
        ] {
            let count: i64 = db
                .query_row(
                    "SELECT count(*) FROM sqlite_master WHERE type = 'index' AND name = ?1",
                    [index],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(count, 1, "missing index: {index}");
        }
    }

    #[test]
    fn upgrades_a_database_created_at_an_older_version() {
        let mut db = memory_db();

        // Apply only migration 1, as a database from the previous release
        // would have.
        db.execute_batch(MIGRATIONS[0].2).unwrap();
        db.pragma_update(None, "user_version", 1).unwrap();

        let version = run(&mut db).unwrap();
        assert_eq!(version, latest_version());

        // The table added by migration 2 must now exist.
        let count: i64 = db
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name = 'scan_categories'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn migration_versions_are_ordered_and_unique() {
        let mut previous = 0;
        for (version, name, sql) in MIGRATIONS {
            assert!(
                *version > previous,
                "migration {name} is out of order or duplicated"
            );
            assert!(!sql.trim().is_empty(), "migration {name} is empty");
            previous = *version;
        }
    }
}
