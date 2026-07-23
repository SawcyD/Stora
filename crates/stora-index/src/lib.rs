//! SQLite persistence for scans, aggregates, cleanup history, exclusions,
//! and settings.
//!
//! All queries are parameterized. Writes are batched inside transactions —
//! never one transaction per file.

pub mod applications;
pub mod automation;
pub mod knowledge;
pub mod migrations;
mod queries;

use std::path::Path;
use std::sync::Mutex;

use rusqlite::Connection;
use stora_core::Result;

/// Thread-safe handle to the Stora database.
///
/// A single connection behind a mutex is the right shape here: writes are
/// batched, and SQLite serializes writers anyway.
pub struct Index {
    connection: Mutex<Connection>,
}

impl Index {
    /// Opens (creating if needed) the database at `path` and migrates it.
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|err| {
                stora_core::StoraError::Internal(format!("could not create data folder: {err}"))
            })?;
        }
        let connection = Connection::open(path).map_err(migrations::map_err)?;
        Self::from_connection(connection)
    }

    pub fn open_in_memory() -> Result<Self> {
        let connection = Connection::open_in_memory().map_err(migrations::map_err)?;
        Self::from_connection(connection)
    }

    fn from_connection(mut connection: Connection) -> Result<Self> {
        // WAL keeps the UI's reads from blocking behind a scan's writes.
        connection
            .pragma_update(None, "journal_mode", "WAL")
            .map_err(migrations::map_err)?;
        // NORMAL is the right durability trade for a cache of derived data.
        connection
            .pragma_update(None, "synchronous", "NORMAL")
            .map_err(migrations::map_err)?;
        connection
            .pragma_update(None, "foreign_keys", "ON")
            .map_err(migrations::map_err)?;
        connection
            .busy_timeout(std::time::Duration::from_secs(5))
            .map_err(migrations::map_err)?;

        migrations::run(&mut connection)?;

        Ok(Self {
            connection: Mutex::new(connection),
        })
    }

    /// Runs `action` with exclusive access to the connection.
    pub fn with<T>(&self, action: impl FnOnce(&mut Connection) -> Result<T>) -> Result<T> {
        let mut guard = self.connection.lock().expect("index connection poisoned");
        action(&mut guard)
    }
}
