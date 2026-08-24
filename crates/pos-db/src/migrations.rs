//! Database Migrations
//!
//! Handles schema versioning and migrations for the local SQLite database.

use rusqlite::{params, Connection, Result as SqliteResult};
use tracing::{debug, info};

use super::schema::{
    CURRENT_SCHEMA_VERSION, SCHEMA_V1, SCHEMA_V13, SCHEMA_V14, SCHEMA_V2, SCHEMA_V3, SCHEMA_V4,
    SCHEMA_V5, SCHEMA_V6, SCHEMA_V7, SCHEMA_V8, SCHEMA_V9,
};
use crate::projection::scalar;

/// Whether `table` already has a column called `column`.
///
/// Seven migrations asked this by hand, and **they did not agree with each other about what a
/// failure means**: three defaulted to `false` (column absent — re-add it), three to `0` (the
/// same), and `apply_v2` to `true` (column present — skip it). Same query, same shape, opposite
/// conclusions, and nothing in the file said which was intended.
///
/// None of them was. `COUNT(*)` over `pragma_table_info` returns exactly one row — zero for a
/// table that does not exist — so `QueryReturnedNoRows` cannot occur and the default could only
/// ever absorb a real failure. Absorbing it means a migration silently skipping an `ALTER TABLE`
/// or silently re-running one, in the step that decides whether the schema is what the rest of
/// this crate assumes. The error propagates now; a migration that cannot read the schema should
/// stop, not guess.
fn has_column(conn: &Connection, table: &str, column: &str) -> SqliteResult<bool> {
    let present: i64 = scalar(
        conn,
        "SELECT COUNT(*) FROM pragma_table_info(?1) WHERE name = ?2",
        params![table, column],
    )?;
    Ok(present > 0)
}

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

    // Get current version. `COALESCE(MAX(...), 0)` over the table created immediately above
    // always returns one row, so the `.unwrap_or(0)` this replaces could only have absorbed a
    // real failure — and answering 0 for it means re-applying every migration from v1 against a
    // database that may already hold data.
    let current_version: i32 = get_schema_version(conn)?;

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

    if current_version < 13 {
        info!("Applying migration v13: the operator PIN hash leaves the till");
        apply_v13(conn)?;
    }

    if current_version < 14 {
        info!("Applying migration v14: the operator session survives a restart");
        apply_v14(conn)?;
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
        if !has_column(conn, "offline_transactions", col_name)? {
            conn.execute(
                &format!(
                    "ALTER TABLE offline_transactions ADD COLUMN {} {}",
                    col_name, col_def
                ),
                [],
            )?;
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
    if !has_column(conn, "offline_transactions", "catalog_etag")? {
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
    if !has_column(conn, "terminal_registration", "license_key")? {
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
        if !has_column(conn, "operators", col_name)? {
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
        if !has_column(conn, "terminal_config", col_name)? {
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
/// Drops `operators.pin_hash`.
///
/// The platform stopped sending `pinHash`, so every synced row held `""` — and
/// `bcrypt::verify(pin, "")` fails, was read as a **wrong PIN**, and was charged to the operator's
/// lockout budget. A shop with no network could not open, and every cashier who tried was locked
/// out for it. See [`SCHEMA_V13`] for the whole story.
///
/// Guarded on `pragma_table_info` like the migrations above it, for two reasons rather than one:
/// a database created fresh from `SCHEMA_V1` never had the column (it left the `CREATE TABLE` in
/// the same change), and re-running a migration must not be an error.
fn apply_v13(conn: &Connection) -> SqliteResult<()> {
    if has_column(conn, "operators", "pin_hash")? {
        conn.execute_batch(SCHEMA_V13)?;
    }

    conn.execute(
        "INSERT INTO schema_version (version, description) \
         VALUES (13, 'The operator PIN hash leaves the till')",
        [],
    )?;

    info!("Migration v13 applied successfully");
    Ok(())
}

/// Applies version 14: the `operator_sessions` table.
///
/// `CREATE TABLE IF NOT EXISTS`, so this is a no-op on a database created fresh from
/// [`SCHEMA_V1`], which declares the same table. No guard beyond that is needed — unlike
/// [`apply_v13`], which drops a column and has to ask whether it is there.
fn apply_v14(conn: &Connection) -> SqliteResult<()> {
    conn.execute_batch(SCHEMA_V14)?;

    conn.execute(
        "INSERT INTO schema_version (version, description) \
         VALUES (14, 'The operator session survives a restart')",
        [],
    )?;

    info!("Migration v14 applied successfully");
    Ok(())
}

fn apply_v12(conn: &Connection) -> SqliteResult<()> {
    // Add product type columns if they don't exist
    let columns_to_add = vec![
        ("product_type", "TEXT DEFAULT 'PHYSICAL_GOOD'"),
        ("track_inventory", "INTEGER DEFAULT 1"),
        ("product_nature", "TEXT DEFAULT 'TANGIBLE'"),
    ];

    for (col_name, col_def) in columns_to_add {
        if !has_column(conn, "products", col_name)? {
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
    scalar(
        conn,
        "SELECT COALESCE(MAX(version), 0) FROM schema_version",
        [],
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

    /// A device upgrading from v13 gains the `operator_sessions` table, and a fresh one has it
    /// from [`SCHEMA_V1`].
    ///
    /// Both directions, because they are different code paths: `apply_v1` runs the `CREATE TABLE`
    /// inside the v1 batch and `apply_v14` runs it as a migration. A table declared in only one of
    /// them works everywhere except on the devices that took the other route, and which route a
    /// device took is invisible afterwards.
    #[test]
    fn v14_gives_every_till_an_operator_sessions_table() {
        let upgraded = Connection::open_in_memory().unwrap();
        upgraded
            .execute_batch(
                r#"CREATE TABLE schema_version (
                       version INTEGER PRIMARY KEY,
                       description TEXT,
                       applied_at TEXT DEFAULT (datetime('now'))
                   );"#,
            )
            .unwrap();
        apply_v14(&upgraded).unwrap();

        let fresh = Connection::open_in_memory().unwrap();
        run_migrations(&fresh).unwrap();

        for (route, conn) in [("upgraded from v13", &upgraded), ("created fresh", &fresh)] {
            let exists: i32 = scalar(
                conn,
                "SELECT COUNT(*) FROM sqlite_master \
                 WHERE type = 'table' AND name = 'operator_sessions'",
                [],
            )
            .unwrap();
            assert_eq!(exists, 1, "a till {route} must have the session table");

            // The one-row invariant, asserted rather than assumed: two operators signed in at once
            // would make `SELECT ... WHERE id = 1` pick whichever the insert order left behind.
            conn.execute(
                "INSERT INTO operator_sessions (id, operator_id, token, expires_at) \
                 VALUES (1, 'op-1', 'tok', '2026-08-24T06:00:00+00:00')",
                [],
            )
            .unwrap();
            let second = conn.execute(
                "INSERT INTO operator_sessions (id, operator_id, token, expires_at) \
                 VALUES (2, 'op-2', 'tok2', '2026-08-24T06:00:00+00:00')",
                [],
            );
            assert!(
                second.is_err(),
                "a till {route} must not be able to hold two operator sessions at once"
            );
        }
    }

    /// Re-running v14 is not an error, the way re-running any migration must not be.
    #[test]
    fn v14_is_a_no_op_on_a_schema_that_already_has_the_table() {
        let conn = Connection::open_in_memory().unwrap();
        run_migrations(&conn).unwrap();

        conn.execute(
            "INSERT INTO operator_sessions (id, operator_id, token, expires_at) \
             VALUES (1, 'op-1', 'tok', '2026-08-24T06:00:00+00:00')",
            [],
        )
        .unwrap();

        // `CREATE TABLE IF NOT EXISTS`, so the row survives. A migration that silently recreated
        // the table would sign the cashier out on every upgrade.
        conn.execute_batch(SCHEMA_V14).unwrap();

        let held: String = scalar(
            &conn,
            "SELECT operator_id FROM operator_sessions WHERE id = 1",
            [],
        )
        .unwrap();
        assert_eq!(held, "op-1");
    }

    /// A till that already holds operators loses their PIN hashes, and keeps everything else.
    ///
    /// The migration that matters most on a real device: `pin_hash` was `NOT NULL`, so the drop
    /// has to work against rows that already exist rather than only against a fresh schema. The
    /// row below is inserted the way v12 wrote them, with a hash, and read back afterwards to
    /// prove the operator survived the column.
    #[test]
    fn v13_drops_the_pin_hash_column_from_a_populated_table() {
        let conn = Connection::open_in_memory().unwrap();

        // Build the table as it stood at v12, hash column and all.
        conn.execute_batch(
            r#"CREATE TABLE operators (
                   id TEXT PRIMARY KEY,
                   code TEXT NOT NULL,
                   name TEXT NOT NULL,
                   name_ar TEXT,
                   pin_hash TEXT NOT NULL,
                   role TEXT DEFAULT 'CASHIER',
                   avatar_url TEXT,
                   permissions_json TEXT,
                   is_active INTEGER DEFAULT 1,
                   updated_at TEXT DEFAULT (datetime('now'))
               );
               INSERT INTO operators (id, code, name, pin_hash, role)
               VALUES ('op-1', 'C001', 'Ahmed', '$2b$12$averyrealbcrypthashindeed', 'MANAGER');
               CREATE TABLE schema_version (
                   version INTEGER PRIMARY KEY,
                   description TEXT,
                   applied_at TEXT DEFAULT (datetime('now'))
               );"#,
        )
        .unwrap();

        apply_v13(&conn).unwrap();

        let has_column: i32 = scalar(
            &conn,
            "SELECT COUNT(*) FROM pragma_table_info('operators') WHERE name = 'pin_hash'",
            [],
        )
        .unwrap();
        assert_eq!(has_column, 0, "the PIN hash must be gone from the table");

        // The operator is still there, with everything that is not a secret.
        //
        // Two one-column reads rather than one two-column read. The pair was `row.get(0)` and
        // `row.get(1)` against a hand-written `SELECT` list, which is the coupling this issue
        // removes; a declared mapping is the usual replacement, but the two columns it would have
        // to bind are `name` and `role`, and those cross the boundary through
        // `column::OPERATOR_NAME` (a *pair* codec, needing `name_ar` too) and
        // `column::OPERATOR_ROLE`. Declaring a shape here to assert two raw strings survived a
        // migration would either restate those codecs or spell an operator's identity as a
        // `String` — which `operator_identity_never_survives_as_a_bare_string` refuses, correctly.
        // A one-column query has no ordinal to get wrong, so `scalar` says exactly what is meant.
        let surviving_name: String =
            scalar(&conn, "SELECT name FROM operators WHERE id = 'op-1'", []).unwrap();
        let surviving_role: String =
            scalar(&conn, "SELECT role FROM operators WHERE id = 'op-1'", []).unwrap();
        assert_eq!(surviving_name, "Ahmed");
        assert_eq!(surviving_role, "MANAGER");

        // And the hash is not merely hidden — the column does not exist, so a query naming it is
        // an error rather than a value. `unwrap_or(false)` on this would be the same defect as
        // the one v13 exists to remove.
        assert!(conn
            .query_row("SELECT pin_hash FROM operators", [], |row| row
                .get::<_, String>(0))
            .is_err());
    }

    /// A database created fresh never had the column, and v13 must still record itself.
    #[test]
    fn v13_is_a_no_op_on_a_schema_that_never_had_the_column() {
        let conn = Connection::open_in_memory().unwrap();
        run_migrations(&conn).unwrap();

        let has_column: i32 = scalar(
            &conn,
            "SELECT COUNT(*) FROM pragma_table_info('operators') WHERE name = 'pin_hash'",
            [],
        )
        .unwrap();
        assert_eq!(has_column, 0);
        assert_eq!(get_schema_version(&conn).unwrap(), CURRENT_SCHEMA_VERSION);

        // This used to read `assert_eq!(CURRENT_SCHEMA_VERSION, 13)`, which is a tripwire that has
        // to be edited by every migration that follows — so it fires on the correct change and
        // teaches whoever hits it to bump a number. What it was actually guarding is worth
        // keeping: that the constant was not raised without a migration to match. Counting the
        // recorded rows says the same thing and stays true, because every `apply_vN` writes
        // exactly one.
        let recorded: i32 = scalar(&conn, "SELECT COUNT(*) FROM schema_version", []).unwrap();
        assert_eq!(
            recorded, CURRENT_SCHEMA_VERSION,
            "CURRENT_SCHEMA_VERSION is {CURRENT_SCHEMA_VERSION} and {recorded} migrations recorded \
             themselves; a version was bumped without a migration, or one does not record itself"
        );
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
            let count: i64 = scalar(
                &conn,
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?",
                [table],
            )
            .unwrap_or(0);

            assert!(count > 0, "Table '{}' should exist", table);
        }
    }
}
