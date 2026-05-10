//! Database Migrations
//!
//! Handles schema versioning and migrations for the local SQLite database.

use rusqlite::{Connection, Result as SqliteResult};
use tracing::{debug, info};

use super::schema::{
    CURRENT_SCHEMA_VERSION, SCHEMA_V1, SCHEMA_V2, SCHEMA_V3, SCHEMA_V4, SCHEMA_V5, SCHEMA_V6,
    SCHEMA_V7, SCHEMA_V8, SCHEMA_V9,
};

/// Runs all pending migrations on the database
pub fn run_migrations(conn: &Connection) -> SqliteResult<()> {
    // Create schema_version table if not exists
    conn.execute(
        "CREATE TABLE IF NOT EXISTS schema_version (
            version INTEGER PRIMARY KEY,
            applied_at TEXT DEFAULT (datetime('now')),
            description TEXT
        )",
        [],
    )?;

    // Get current version
    let current_version: i32 = conn
        .query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_version",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);

    debug!("Current schema version: {}", current_version);

    // Apply migrations
    if current_version < 1 {
        info!("Applying migration v1: Initial schema");
        apply_v1(conn)?;
    }

    if current_version < 2 {
        info!("Applying migration v2: Z-Reports and day closures");
        apply_v2(conn)?;
    }

    if current_version < 3 {
        info!("Applying migration v3: Terminal registration/pairing");
        apply_v3(conn)?;
    }

    if current_version < 4 {
        info!("Applying migration v4: Feature library");
        apply_v4(conn)?;
    }

    if current_version < 5 {
        info!("Applying migration v5: Company name in registration");
        apply_v5(conn)?;
    }

    if current_version < 6 {
        info!("Applying migration v6: Active cart persistence");
        apply_v6(conn)?;
    }

    if current_version < 7 {
        info!("Applying migration v7: Price version tracking");
        apply_v7(conn)?;
    }

    if current_version < 8 {
        info!("Applying migration v8: Platform license key");
        apply_v8(conn)?;
    }

    if current_version < 9 {
        info!("Applying migration v9: Shared drafts");
        apply_v9(conn)?;
    }

    if current_version < 10 {
        info!("Applying migration v10: HR Employee integration for operators");
        apply_v10(conn)?;
    }

    if current_version < 11 {
        info!("Applying migration v11: Tax config columns on terminal_config");
        apply_v11(conn)?;
    }

    if current_version < 12 {
        info!("Applying migration v12: Product type awareness");
        apply_v12(conn)?;
    }

    Ok(())
}

/// Applies version 1 schema (initial tables)
fn apply_v1(conn: &Connection) -> SqliteResult<()> {
    // Execute the schema SQL
    conn.execute_batch(SCHEMA_V1)?;

    // Record the migration
    conn.execute(
        "INSERT INTO schema_version (version, description) VALUES (1, 'Initial schema')",
        [],
    )?;

    info!("Migration v1 applied successfully");
    Ok(())
}

/// Applies version 2 schema (Z-Reports and day closures)
fn apply_v2(conn: &Connection) -> SqliteResult<()> {
    // Execute the schema SQL
    conn.execute_batch(SCHEMA_V2)?;

    // Add additional columns to offline_transactions if needed for Z-Report aggregation
    // Using PRAGMA to check column existence and add safely
    let columns_to_add = vec![
        ("type", "TEXT DEFAULT 'SALE'"),
        ("status", "TEXT DEFAULT 'COMPLETED'"),
        ("payment_method", "TEXT DEFAULT 'CASH'"),
        ("total", "REAL DEFAULT 0"),
        ("tax", "REAL DEFAULT 0"),
        ("discount", "REAL DEFAULT 0"),
    ];

    for (col_name, col_def) in columns_to_add {
        // Check if column exists
        let column_exists: bool = conn
            .query_row(
                &format!(
                    "SELECT COUNT(*) FROM pragma_table_info('offline_transactions') WHERE name = '{}'",
                    col_name
                ),
                [],
                |row| row.get::<_, i64>(0),
            )
            .map(|count| count > 0)
            .unwrap_or(true); // Assume exists if error

        if !column_exists {
            let sql = format!(
                "ALTER TABLE offline_transactions ADD COLUMN {} {}",
                col_name, col_def
            );
            let _ = conn.execute(&sql, []); // Ignore error if column exists
        }
    }

    // Record the migration
    conn.execute(
        "INSERT INTO schema_version (version, description) VALUES (2, 'Z-Reports and day closures')",
        [],
    )?;

    info!("Migration v2 applied successfully");
    Ok(())
}

/// Applies version 3 schema (Terminal registration/pairing support)
fn apply_v3(conn: &Connection) -> SqliteResult<()> {
    // Execute the schema SQL
    conn.execute_batch(SCHEMA_V3)?;

    // Record the migration
    conn.execute(
        "INSERT INTO schema_version (version, description) VALUES (3, 'Terminal registration/pairing')",
        [],
    )?;

    info!("Migration v3 applied successfully");
    Ok(())
}

/// Applies version 4 schema (Feature library for dynamic screen enablement)
fn apply_v4(conn: &Connection) -> SqliteResult<()> {
    // Execute the schema SQL
    conn.execute_batch(SCHEMA_V4)?;

    // Record the migration
    conn.execute(
        "INSERT INTO schema_version (version, description) VALUES (4, 'Feature library')",
        [],
    )?;

    info!("Migration v4 applied successfully");
    Ok(())
}

/// Applies version 5 schema (Add company_name to terminal_registration)
fn apply_v5(conn: &Connection) -> SqliteResult<()> {
    // Execute the schema SQL
    conn.execute_batch(SCHEMA_V5)?;

    // Record the migration
    conn.execute(
        "INSERT INTO schema_version (version, description) VALUES (5, 'Company name in registration')",
        [],
    )?;

    info!("Migration v5 applied successfully");
    Ok(())
}

/// Applies version 6 schema (Active cart persistence for crash recovery)
fn apply_v6(conn: &Connection) -> SqliteResult<()> {
    // Execute the schema SQL
    conn.execute_batch(SCHEMA_V6)?;

    // Record the migration
    conn.execute(
        "INSERT INTO schema_version (version, description) VALUES (6, 'Active cart persistence')",
        [],
    )?;

    info!("Migration v6 applied successfully");
    Ok(())
}

/// Applies version 7 schema (Price version tracking for offline transactions)
fn apply_v7(conn: &Connection) -> SqliteResult<()> {
    // Check if column already exists (for safety)
    let column_exists: bool = conn
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('offline_transactions') WHERE name = 'catalog_etag'",
            [],
            |row| row.get::<_, i64>(0),
        )
        .map(|count| count > 0)
        .unwrap_or(false);

    if !column_exists {
        // Execute the schema SQL
        conn.execute_batch(SCHEMA_V7)?;
    }

    // Record the migration
    conn.execute(
        "INSERT INTO schema_version (version, description) VALUES (7, 'Price version tracking')",
        [],
    )?;

    info!("Migration v7 applied successfully");
    Ok(())
}

/// Applies version 8 schema (Platform license key for platform registry)
fn apply_v8(conn: &Connection) -> SqliteResult<()> {
    // Check if column already exists (for safety)
    let column_exists: bool = conn
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('terminal_registration') WHERE name = 'license_key'",
            [],
            |row| row.get::<_, i64>(0),
        )
        .map(|count| count > 0)
        .unwrap_or(false);

    if !column_exists {
        // Execute the schema SQL
        conn.execute_batch(SCHEMA_V8)?;
    }

    // Record the migration
    conn.execute(
        "INSERT INTO schema_version (version, description) VALUES (8, 'Platform license key')",
        [],
    )?;

    info!("Migration v8 applied successfully");
    Ok(())
}

/// Applies version 9 schema (Shared drafts for cloud-synced draft support)
fn apply_v9(conn: &Connection) -> SqliteResult<()> {
    // Execute the schema SQL
    conn.execute_batch(SCHEMA_V9)?;

    // Record the migration
    conn.execute(
        "INSERT INTO schema_version (version, description) VALUES (9, 'Shared drafts')",
        [],
    )?;

    info!("Migration v9 applied successfully");
    Ok(())
}

/// Applies version 10 schema (HR Employee integration for operators)
fn apply_v10(conn: &Connection) -> SqliteResult<()> {
    // Add new columns to operators table for HR Employee integration
    // SQLite doesn't have IF NOT EXISTS for ALTER TABLE, so we check first
    let columns_to_add = vec![
        ("employee_id", "TEXT"),
        ("employee_number", "TEXT"),
        ("department", "TEXT"),
        ("position", "TEXT"),
    ];

    for (col_name, col_type) in columns_to_add {
        // Check if column exists
        let col_exists: i32 = conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('operators') WHERE name = ?",
                [col_name],
                |row| row.get(0),
            )
            .unwrap_or(0);

        if col_exists == 0 {
            conn.execute(
                &format!("ALTER TABLE operators ADD COLUMN {} {}", col_name, col_type),
                [],
            )?;
        }
    }

    // Create indexes (IF NOT EXISTS handles idempotency)
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_operators_employee_number ON operators(employee_number)",
        [],
    )?;
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_operators_employee_id ON operators(employee_id)",
        [],
    )?;

    // Record the migration
    conn.execute(
        "INSERT INTO schema_version (version, description) VALUES (10, 'HR Employee integration for operators')",
        [],
    )?;

    info!("Migration v10 applied successfully");
    Ok(())
}

/// Applies version 11 schema (Tax config columns on terminal_config)
fn apply_v11(conn: &Connection) -> SqliteResult<()> {
    // Add tax columns to terminal_config if they don't exist
    let columns_to_add = vec![
        ("tax_rate", "REAL DEFAULT 0"),
        ("tax_inclusive", "INTEGER DEFAULT 0"),
    ];

    for (col_name, col_def) in columns_to_add {
        let col_exists: i32 = conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('terminal_config') WHERE name = ?",
                [col_name],
                |row| row.get(0),
            )
            .unwrap_or(0);

        if col_exists == 0 {
            conn.execute(
                &format!(
                    "ALTER TABLE terminal_config ADD COLUMN {} {}",
                    col_name, col_def
                ),
                [],
            )?;
        }
    }

    // Record the migration
    conn.execute(
        "INSERT INTO schema_version (version, description) VALUES (11, 'Tax config columns on terminal_config')",
        [],
    )?;

    info!("Migration v11 applied successfully");
    Ok(())
}

/// Applies version 12 schema (Product type awareness - Phase 3 Track H)
fn apply_v12(conn: &Connection) -> SqliteResult<()> {
    // Add product type columns if they don't exist
    let columns_to_add = vec![
        ("product_type", "TEXT DEFAULT 'PHYSICAL_GOOD'"),
        ("track_inventory", "INTEGER DEFAULT 1"),
        ("product_nature", "TEXT DEFAULT 'TANGIBLE'"),
    ];

    for (col_name, col_def) in columns_to_add {
        let col_exists: i32 = conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('products') WHERE name = ?",
                [col_name],
                |row| row.get(0),
            )
            .unwrap_or(0);

        if col_exists == 0 {
            conn.execute(
                &format!("ALTER TABLE products ADD COLUMN {} {}", col_name, col_def),
                [],
            )?;
        }
    }

    // Create index for product type filtering
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_products_type ON products(product_type)",
        [],
    )?;

    // Record the migration
    conn.execute(
        "INSERT INTO schema_version (version, description) VALUES (12, 'Product type awareness')",
        [],
    )?;

    info!("Migration v12 applied successfully");
    Ok(())
}

/// Returns the current schema version
pub fn get_schema_version(conn: &Connection) -> SqliteResult<i32> {
    conn.query_row(
        "SELECT COALESCE(MAX(version), 0) FROM schema_version",
        [],
        |row| row.get(0),
    )
}

/// Checks if the database needs migration
pub fn needs_migration(conn: &Connection) -> SqliteResult<bool> {
    let current = get_schema_version(conn)?;
    Ok(current < CURRENT_SCHEMA_VERSION)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_run_migrations() {
        let conn = Connection::open_in_memory().unwrap();
        run_migrations(&conn).unwrap();

        // Verify version was set
        let version = get_schema_version(&conn).unwrap();
        assert_eq!(version, CURRENT_SCHEMA_VERSION);
    }

    #[test]
    fn test_migrations_idempotent() {
        let conn = Connection::open_in_memory().unwrap();

        // Run migrations twice
        run_migrations(&conn).unwrap();
        run_migrations(&conn).unwrap();

        // Should still be current version
        let version = get_schema_version(&conn).unwrap();
        assert_eq!(version, CURRENT_SCHEMA_VERSION);
    }

    #[test]
    fn test_tables_created() {
        let conn = Connection::open_in_memory().unwrap();
        run_migrations(&conn).unwrap();

        // Verify tables exist
        let tables = vec![
            "terminal_config",
            "sync_state",
            "operators",
            "categories",
            "products",
            "products_fts",
            "shifts",
            "offline_transactions",
            "drafts",
            "customers",
            "payment_methods",
            "screens",
            "settings",
            "print_queue",
            "z_reports",             // V2
            "day_closures",          // V2
            "terminal_registration", // V3
            "features",              // V4
            "feature_screens",       // V4
            "active_cart",           // V6
            "shared_drafts",         // V9
            "draft_sync_queue",      // V9
        ];

        for table in tables {
            let count: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?",
                    [table],
                    |row| row.get(0),
                )
                .unwrap_or(0);

            assert!(count > 0, "Table '{}' should exist", table);
        }
    }
}
