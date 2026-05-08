//! Database Module - SQLite local storage
//!
//! This module manages the local SQLite database for offline operation.
//!
//! ## Architecture
//!
//! The database provides:
//! - **Offline-first operation**: All data is stored locally first
//! - **Sync state tracking**: ETags and timestamps for efficient syncing
//! - **FTS5 search**: Fast full-text search for products
//! - **Transaction queue**: Pending transactions for upload
//!
//! ## Usage
//!
//! ```rust,ignore
//! use crate::db::{init_database, Database};
//!
//! let data_dir = PathBuf::from("./data");
//! let db = init_database(&data_dir)?;
//!
//! // Search products
//! let products = db.search_products("milk", 10)?;
//!
//! // Get sync state
//! let etag = db.get_etag(SyncResource::Products)?;
//! ```

pub mod active_cart;
pub mod connection;
pub mod draft_sync_queue;
pub mod drafts;
pub mod features;
pub mod migrations;
pub mod operators;
pub mod products;
pub mod schema;
pub mod shared_drafts;
pub mod shifts;
pub mod sync_state;
pub mod transactions;
pub mod z_reports;

// Re-export main types
pub use connection::Database;
pub use draft_sync_queue::{
    DraftQueueSyncStatus, DraftSyncOperation, DraftSyncQueueItem, MAX_DRAFT_SYNC_RETRIES,
};
pub use drafts::DraftRow;
pub use shared_drafts::{SharedDraftRow, SharedDraftSyncStatus};
pub use migrations::{get_schema_version, needs_migration, run_migrations};
pub use operators::{OperatorPermissions, OperatorRow};
pub use products::{CategoryRow, ProductRow};
pub use schema::CURRENT_SCHEMA_VERSION;
pub use shifts::{ShiftRow, ShiftStatus};
pub use sync_state::{SyncResource, SyncState};
pub use transactions::{OfflineTransactionRow, SyncStatus, TransactionType};
pub use z_reports::DayTotals;

use rust_decimal::Decimal;
use rust_decimal::prelude::*;
use rusqlite::Result as SqliteResult;
use std::path::PathBuf;
use tracing::{debug, info};

/// Converts an f64 value read from SQLite into a `Decimal`.
///
/// SQLite stores all numbers as IEEE 754 f64. This function wraps the
/// conversion so that all monetary arithmetic happens in `Decimal` space,
/// eliminating floating-point rounding errors.
///
/// The result is normalized (trailing zeros removed) to avoid f64 artifacts
/// like `9.990000000000000213...` becoming just `9.99`.
///
/// Returns `Decimal::ZERO` if the f64 value cannot be represented.
pub fn decimal_from_sqlite(val: f64) -> Decimal {
    Decimal::from_f64_retain(val)
        .map(|d| d.round_dp(6).normalize())
        .unwrap_or(Decimal::ZERO)
}

/// Converts a `Decimal` value into f64 for writing to SQLite.
///
/// Because the `Decimal` was already rounded to the desired precision
/// before storage, the f64 representation preserves the intended value.
///
/// Returns `0.0` if the conversion fails.
pub fn decimal_to_sqlite(val: &Decimal) -> f64 {
    val.to_f64().unwrap_or(0.0)
}

/// Initializes the database at the specified data directory.
///
/// Creates the directory if needed, opens or creates the database file,
/// and runs any pending migrations.
///
/// # Arguments
///
/// * `data_dir` - Path to the data directory where `pos_terminal.db` will be stored
///
/// # Returns
///
/// Returns a configured `Database` instance ready for use.
///
/// # Example
///
/// ```rust,ignore
/// let data_dir = PathBuf::from("./data");
/// let db = init_database(&data_dir)?;
/// ```
pub fn init_database(data_dir: &PathBuf) -> SqliteResult<Database> {
    // Create data directory if it doesn't exist
    if let Err(e) = std::fs::create_dir_all(data_dir) {
        debug!("Could not create data directory: {}", e);
    }

    let db_path = data_dir.join("pos_terminal.db");
    info!("Opening database at: {:?}", db_path);

    let db = Database::new(&db_path)?;

    // Run migrations
    {
        let conn = db.connection();
        let conn = conn.lock();
        run_migrations(&conn)?;

        let version = get_schema_version(&conn)?;
        info!("Database schema version: {}", version);
    }

    Ok(db)
}

/// Creates an in-memory database (useful for testing).
///
/// The database is initialized with all migrations applied.
///
/// # Example
///
/// ```rust,ignore
/// let db = init_memory_database()?;
/// // Use for testing
/// ```
pub fn init_memory_database() -> SqliteResult<Database> {
    let db = Database::in_memory()?;

    {
        let conn = db.connection();
        let conn = conn.lock();
        run_migrations(&conn)?;
    }

    Ok(db)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;
    use tempfile::TempDir;

    #[test]
    fn test_init_database() {
        let temp_dir = TempDir::new().unwrap();
        let db = init_database(&temp_dir.path().to_path_buf()).unwrap();

        // Verify database is usable
        let count = db.get_product_count().unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn test_init_memory_database() {
        let db = init_memory_database().unwrap();

        // Verify database is usable
        let operators = db.get_operators().unwrap();
        assert!(operators.is_empty());
    }

    #[test]
    fn test_full_workflow() {
        let db = init_memory_database().unwrap();

        // 1. Save some products
        let product = ProductRow {
            id: "p1".to_string(),
            sku: "SKU001".to_string(),
            barcode: Some("1234567890123".to_string()),
            name: "Test Product".to_string(),
            name_ar: Some("منتج تجريبي".to_string()),
            price: Decimal::from_str("10.99").unwrap(),
            ..Default::default()
        };
        db.save_product(&product).unwrap();

        // 2. Save an operator
        let operator = OperatorRow {
            id: "op1".to_string(),
            code: "C001".to_string(),
            name: "Ahmed".to_string(),
            name_ar: Some("أحمد".to_string()),
            pin_hash: "hashed".to_string(),
            ..Default::default()
        };
        db.save_operator(&operator).unwrap();

        // 3. Start a shift
        let shift = db
            .start_shift("s1", "SHIFT-001", "op1", Some("TERM-01"), Decimal::from(100))
            .unwrap();
        assert_eq!(shift.status, "ACTIVE");

        // 4. Create a draft
        let draft = db
            .create_draft(
                "d1",
                r#"[{"product_id":"p1","qty":2}]"#,
                Some("op1"),
                Some("s1"),
                None,
                None,
                Some(8),
            )
            .unwrap();
        assert!(draft.name.is_some());

        // 5. Search products
        let found = db.search_products("Test", 10).unwrap();
        assert_eq!(found.len(), 1);

        // 6. Get by barcode
        let by_barcode = db.get_product_by_barcode("1234567890123").unwrap();
        assert!(by_barcode.is_some());

        // 7. End shift
        db.end_shift("s1", Decimal::from(250), Decimal::from(200), Some("Good day")).unwrap();
        let ended = db.get_shift_by_id("s1").unwrap().unwrap();
        assert_eq!(ended.status, "CLOSED");
        assert_eq!(ended.variance, Some(Decimal::from(50)));
    }
}
