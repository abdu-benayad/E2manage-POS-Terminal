//! Database Connection Management
//!
//! Provides a thread-safe wrapper around SQLite connection.

use parking_lot::Mutex;
use rusqlite::{Connection, Result as SqliteResult};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Thread-safe database wrapper
#[derive(Clone)]
pub struct Database {
    conn: Arc<Mutex<Connection>>,
    path: Option<PathBuf>,
}

impl Database {
    /// Creates a new database connection from a file path
    pub fn new(path: &PathBuf) -> SqliteResult<Self> {
        let conn = Connection::open(path)?;

        // Enable foreign keys
        conn.execute_batch(
            r#"
            PRAGMA foreign_keys = ON;
            PRAGMA journal_mode = WAL;
            PRAGMA synchronous = NORMAL;
            PRAGMA cache_size = -10000;
            PRAGMA temp_store = MEMORY;
            "#,
        )?;

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
            path: Some(path.clone()),
        })
    }

    /// Creates an in-memory database (useful for testing)
    pub fn in_memory() -> SqliteResult<Self> {
        let conn = Connection::open_in_memory()?;

        // Enable foreign keys
        conn.execute_batch("PRAGMA foreign_keys = ON;")?;

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
            path: None,
        })
    }

    /// Executes a SQL statement with parameters
    pub fn execute(&self, sql: &str, params: &[&dyn rusqlite::ToSql]) -> SqliteResult<usize> {
        let conn = self.conn.lock();
        conn.execute(sql, params)
    }

    /// Executes multiple SQL statements without parameters
    pub fn execute_batch(&self, sql: &str) -> SqliteResult<()> {
        let conn = self.conn.lock();
        conn.execute_batch(sql)
    }

    /// Returns the connection arc for complex operations
    pub fn connection(&self) -> Arc<Mutex<Connection>> {
        self.conn.clone()
    }

    /// Returns the database file path (None for in-memory)
    pub fn path(&self) -> Option<&PathBuf> {
        self.path.as_ref()
    }

    /// Begins a transaction and returns a handle to it
    pub fn begin_transaction<F, T>(&self, f: F) -> SqliteResult<T>
    where
        F: FnOnce(&Connection) -> SqliteResult<T>,
    {
        let conn = self.conn.lock();
        let tx = conn.unchecked_transaction()?;
        let result = f(&conn)?;
        tx.commit()?;
        Ok(result)
    }

    /// Checks if the database file exists
    pub fn exists(path: &Path) -> bool {
        path.exists()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_in_memory_database() {
        let db = Database::in_memory().unwrap();
        assert!(db.path().is_none());
    }

    #[test]
    fn test_execute() {
        let db = Database::in_memory().unwrap();
        db.execute_batch("CREATE TABLE test (id INTEGER PRIMARY KEY)")
            .unwrap();
        let rows = db
            .execute("INSERT INTO test (id) VALUES (?1)", &[&1i64])
            .unwrap();
        assert_eq!(rows, 1);
    }

    #[test]
    fn test_transaction() {
        let db = Database::in_memory().unwrap();
        db.execute_batch("CREATE TABLE test (id INTEGER PRIMARY KEY, value TEXT)")
            .unwrap();

        let result = db.begin_transaction(|conn| {
            conn.execute("INSERT INTO test VALUES (1, 'a')", [])?;
            conn.execute("INSERT INTO test VALUES (2, 'b')", [])?;
            Ok(())
        });

        assert!(result.is_ok());

        // Verify data was committed
        let conn = db.connection();
        let conn = conn.lock();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM test", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 2);
    }
}
