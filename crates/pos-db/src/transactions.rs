//! Transactions Repository
//!
//! Handles offline transactions storage and management.

use std::str::FromStr;

use chrono::Utc;
use rusqlite::Result as SqliteResult;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use pos_models::OperatorId;

use super::Database;
use crate::column;
use crate::parse::ParseError;
use crate::projection::OnConflict;
use crate::row_mapping;

/// Transaction type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TransactionType {
    Sale,
    Return,
    Exchange,
    Void,
}

impl TransactionType {
    pub fn as_str(&self) -> &'static str {
        match self {
            TransactionType::Sale => "SALE",
            TransactionType::Return => "RETURN",
            TransactionType::Exchange => "EXCHANGE",
            TransactionType::Void => "VOID",
        }
    }
}

impl FromStr for TransactionType {
    type Err = ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_uppercase().as_str() {
            "SALE" => Ok(TransactionType::Sale),
            "RETURN" => Ok(TransactionType::Return),
            "EXCHANGE" => Ok(TransactionType::Exchange),
            "VOID" => Ok(TransactionType::Void),
            _ => Err(ParseError::TransactionType(s.to_string())),
        }
    }
}

/// Sync status for offline transactions
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SyncStatus {
    Pending,
    Syncing,
    Synced,
    Failed,
    Conflict,
    Discarded,
}

impl SyncStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            SyncStatus::Pending => "PENDING",
            SyncStatus::Syncing => "SYNCING",
            SyncStatus::Synced => "SYNCED",
            SyncStatus::Failed => "FAILED",
            SyncStatus::Conflict => "CONFLICT",
            SyncStatus::Discarded => "DISCARDED",
        }
    }
}

impl FromStr for SyncStatus {
    type Err = ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_uppercase().as_str() {
            "PENDING" => Ok(SyncStatus::Pending),
            "SYNCING" => Ok(SyncStatus::Syncing),
            "SYNCED" => Ok(SyncStatus::Synced),
            "FAILED" => Ok(SyncStatus::Failed),
            "CONFLICT" => Ok(SyncStatus::Conflict),
            "DISCARDED" => Ok(SyncStatus::Discarded),
            _ => Err(ParseError::SyncStatus(s.to_string())),
        }
    }
}

/// Offline transaction row
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OfflineTransactionRow {
    pub offline_id: String,
    pub transaction_number: Option<String>,
    pub transaction_type: String,
    pub items_json: String,
    pub payments_json: String,
    pub subtotal: Decimal,
    pub tax_total: Decimal,
    pub discount_total: Decimal,
    pub grand_total: Decimal,
    pub customer_id: Option<String>,
    pub customer_name: Option<String>,
    pub shift_id: Option<String>,
    pub operator_id: Option<OperatorId>,
    pub terminal_id: Option<String>,
    pub receipt_number: Option<String>,
    pub notes: Option<String>,
    pub created_at: String,
    pub sync_status: String,
    pub server_id: Option<String>,
    pub retry_count: i32,
    pub last_error: Option<String>,
    pub last_retry_at: Option<String>,
    /// Catalog ETag at the time this transaction was created.
    /// Used to track transactions made with potentially outdated prices.
    pub catalog_etag: Option<String>,
}

row_mapping! {
    /// Every column of `offline_transactions` this till reads or writes, declared once.
    ///
    /// This is the shape the design opens with. The `INSERT` list and the first `SELECT` sat forty
    /// lines apart and were **byte-identical** — same twenty-three names, same order — and three
    /// more copies of the same list followed. Five unlinked lists of twenty-three columns, and a
    /// round-trip test asserting three fields.
    ///
    /// No `managed` entry: `offline_transactions` has no `updated_at`, so twenty-three columns are
    /// both projected and inserted. `created_at` is a plain column the caller supplies, which is
    /// why it is not `managed` — the till records when the sale happened, not when the row was
    /// written, and those differ for a transaction queued offline.
    pub const OFFLINE_TRANSACTION_ROW: RowMapping<OfflineTransactionRow> =
        for "offline_transactions" {
            offline_id,
            transaction_number  via column::OPTIONAL_TEXT,
            transaction_type,
            items_json,
            payments_json,
            subtotal            via column::DECIMAL,
            tax_total           via column::DECIMAL,
            discount_total      via column::DECIMAL,
            grand_total         via column::DECIMAL,
            customer_id         via column::OPTIONAL_TEXT,
            customer_name       via column::OPTIONAL_TEXT,
            shift_id            via column::OPTIONAL_TEXT,
            operator_id         via column::OPTIONAL_OPERATOR_ID,
            terminal_id         via column::OPTIONAL_TEXT,
            receipt_number      via column::OPTIONAL_TEXT,
            notes               via column::OPTIONAL_TEXT,
            created_at,
            sync_status,
            server_id           via column::OPTIONAL_TEXT,
            retry_count,
            last_error          via column::OPTIONAL_TEXT,
            last_retry_at       via column::OPTIONAL_TEXT,
            catalog_etag        via column::OPTIONAL_TEXT,
        } on_conflict OnConflict::Replace;
}

impl Database {
    /// Saves an offline transaction.
    pub fn save_offline_transaction(&self, txn: &OfflineTransactionRow) -> SqliteResult<()> {
        self.insert(&OFFLINE_TRANSACTION_ROW, txn)?;
        Ok(())
    }

    /// Gets an offline transaction by ID.
    pub fn get_offline_transaction(
        &self,
        offline_id: &str,
    ) -> SqliteResult<Option<OfflineTransactionRow>> {
        self.select_one(
            OFFLINE_TRANSACTION_ROW.reader(),
            "FROM offline_transactions WHERE offline_id = ?1",
            [offline_id],
        )
    }

    /// Gets the transactions still waiting to reach the server, oldest first.
    pub fn get_pending_transactions(&self, limit: i32) -> SqliteResult<Vec<OfflineTransactionRow>> {
        self.select_all(
            OFFLINE_TRANSACTION_ROW.reader(),
            "FROM offline_transactions
             WHERE sync_status IN ('PENDING', 'FAILED')
             ORDER BY created_at ASC
             LIMIT ?1",
            [limit],
        )
    }

    pub fn get_pending_transaction_count(&self) -> SqliteResult<i64> {
        let conn = self.connection();
        let conn = conn.lock();

        conn.query_row(
            "SELECT COUNT(*) FROM offline_transactions WHERE sync_status IN ('PENDING', 'FAILED')",
            [],
            |row| row.get(0),
        )
    }

    /// Updates sync status for a transaction
    pub fn update_transaction_sync_status(
        &self,
        offline_id: &str,
        status: SyncStatus,
        server_id: Option<&str>,
        error: Option<&str>,
    ) -> SqliteResult<()> {
        let now = Utc::now().to_rfc3339();

        self.execute(
            r#"UPDATE offline_transactions
               SET sync_status = ?1, server_id = ?2, last_error = ?3, last_retry_at = ?4,
                   retry_count = CASE WHEN ?1 = 'FAILED' THEN retry_count + 1 ELSE retry_count END
               WHERE offline_id = ?5"#,
            &[
                &status.as_str(),
                &server_id,
                &error,
                &Some(now.as_str()),
                &offline_id,
            ],
        )?;
        Ok(())
    }

    /// Marks a transaction as synced
    pub fn mark_transaction_synced(&self, offline_id: &str, server_id: &str) -> SqliteResult<()> {
        self.update_transaction_sync_status(offline_id, SyncStatus::Synced, Some(server_id), None)
    }

    /// Marks a transaction as failed
    pub fn mark_transaction_failed(&self, offline_id: &str, error: &str) -> SqliteResult<()> {
        self.update_transaction_sync_status(offline_id, SyncStatus::Failed, None, Some(error))
    }

    /// Marks a transaction as having a conflict (needs manager resolution)
    pub fn mark_transaction_conflict(&self, offline_id: &str, error: &str) -> SqliteResult<()> {
        self.update_transaction_sync_status(offline_id, SyncStatus::Conflict, None, Some(error))
    }

    /// Sets transaction retry count to max to stop further retries
    pub fn set_transaction_max_retries(&self, offline_id: &str) -> SqliteResult<()> {
        // Import MAX_RETRY_COUNT would create circular dependency, use hardcoded value
        const MAX_RETRIES: i32 = 10;
        self.execute(
            "UPDATE offline_transactions SET retry_count = ?1 WHERE offline_id = ?2",
            &[&MAX_RETRIES, &offline_id],
        )?;
        Ok(())
    }

    /// Gets all transactions with CONFLICT status (for manager resolution)
    pub fn get_conflict_transactions(&self) -> SqliteResult<Vec<OfflineTransactionRow>> {
        self.get_transactions_by_status("CONFLICT")
    }

    /// Gets every transaction in one sync status, newest first.
    pub fn get_transactions_by_status(
        &self,
        status: &str,
    ) -> SqliteResult<Vec<OfflineTransactionRow>> {
        self.select_all(
            OFFLINE_TRANSACTION_ROW.reader(),
            "FROM offline_transactions WHERE sync_status = ?1 ORDER BY created_at DESC",
            [status],
        )
    }

    pub fn reset_transaction_for_retry(&self, offline_id: &str) -> SqliteResult<()> {
        self.execute(
            r#"UPDATE offline_transactions
               SET sync_status = 'PENDING', retry_count = 0, last_error = NULL, last_retry_at = NULL
               WHERE offline_id = ?1"#,
            &[&offline_id],
        )?;
        Ok(())
    }

    /// Marks a transaction as discarded (manager decided to not sync)
    pub fn discard_transaction(&self, offline_id: &str) -> SqliteResult<()> {
        self.execute(
            r#"UPDATE offline_transactions
               SET sync_status = 'DISCARDED', last_retry_at = datetime('now')
               WHERE offline_id = ?1"#,
            &[&offline_id],
        )?;
        Ok(())
    }

    /// Gets count of conflict transactions
    pub fn get_conflict_count(&self) -> SqliteResult<i64> {
        let conn = self.connection();
        let conn = conn.lock();

        conn.query_row(
            "SELECT COUNT(*) FROM offline_transactions WHERE sync_status = 'CONFLICT'",
            [],
            |row| row.get(0),
        )
    }

    /// Gets every transaction recorded during one shift, oldest first.
    pub fn get_transactions_by_shift(
        &self,
        shift_id: &str,
    ) -> SqliteResult<Vec<OfflineTransactionRow>> {
        self.select_all(
            OFFLINE_TRANSACTION_ROW.reader(),
            "FROM offline_transactions WHERE shift_id = ?1 ORDER BY created_at ASC",
            [shift_id],
        )
    }

    /// Deletes synced transactions older than days
    pub fn cleanup_synced_transactions(&self, older_than_days: i64) -> SqliteResult<usize> {
        self.execute(
            r#"DELETE FROM offline_transactions
               WHERE sync_status = 'SYNCED'
                 AND datetime(created_at) < datetime('now', ?1)"#,
            &[&format!("-{} days", older_than_days)],
        )
    }
}

impl Default for OfflineTransactionRow {
    fn default() -> Self {
        Self {
            offline_id: String::new(),
            transaction_number: None,
            transaction_type: "SALE".to_string(),
            items_json: "[]".to_string(),
            payments_json: "[]".to_string(),
            subtotal: Decimal::ZERO,
            tax_total: Decimal::ZERO,
            discount_total: Decimal::ZERO,
            grand_total: Decimal::ZERO,
            customer_id: None,
            customer_name: None,
            shift_id: None,
            operator_id: None,
            terminal_id: None,
            receipt_number: None,
            notes: None,
            created_at: String::new(),
            sync_status: "PENDING".to_string(),
            server_id: None,
            retry_count: 0,
            last_error: None,
            last_retry_at: None,
            catalog_etag: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::migrations::run_migrations;
    use crate::operators::OperatorRow;
    use pos_models::{OperatorName, OperatorRole};
    use rust_decimal::Decimal;

    fn setup_db() -> Database {
        let db = Database::in_memory().unwrap();
        {
            let conn = db.connection();
            let conn = conn.lock();
            run_migrations(&conn).unwrap();
        }
        db
    }

    fn create_test_operator(db: &Database, id: &str) {
        let operator = OperatorRow {
            id: OperatorId::new(id).unwrap(),
            code: format!("C{id}"),
            employee_id: None,
            employee_number: None,
            name: OperatorName::new("Test Operator", None::<&str>).unwrap(),
            role: OperatorRole::Cashier,
            department: None,
            position: None,
            permissions: None,
            is_active: true,
        };
        db.save_operator(&operator).unwrap();
    }

    #[test]
    fn test_save_and_get_transaction() {
        let db = setup_db();
        create_test_operator(&db, "op-1");

        let txn = OfflineTransactionRow {
            offline_id: "txn-1".to_string(),
            transaction_number: Some("TXN-001".to_string()),
            transaction_type: "SALE".to_string(),
            items_json: r#"[{"id":"prod-1","qty":2}]"#.to_string(),
            payments_json: r#"[{"method":"CASH","amount":100}]"#.to_string(),
            subtotal: Decimal::from(90),
            tax_total: Decimal::from(10),
            discount_total: Decimal::ZERO,
            grand_total: Decimal::from(100),
            customer_id: None,
            customer_name: None,
            shift_id: None, // Don't use shift FK in tests
            operator_id: Some(OperatorId::new("op-1").unwrap()),
            terminal_id: Some("term-1".to_string()),
            receipt_number: Some("R001".to_string()),
            notes: None,
            created_at: Utc::now().to_rfc3339(),
            sync_status: "PENDING".to_string(),
            server_id: None,
            retry_count: 0,
            last_error: None,
            last_retry_at: None,
            catalog_etag: Some("v1-abc123".to_string()),
        };

        db.save_offline_transaction(&txn).unwrap();

        let found = db.get_offline_transaction("txn-1").unwrap();
        assert!(found.is_some());
        let found = found.unwrap();
        assert_eq!(found.grand_total, Decimal::from(100));
        assert_eq!(found.catalog_etag, Some("v1-abc123".to_string()));
    }

    #[test]
    fn test_pending_transactions() {
        let db = setup_db();
        create_test_operator(&db, "op-1");

        // Create pending transactions
        for i in 1..=5 {
            let txn = OfflineTransactionRow {
                offline_id: format!("txn-{}", i),
                transaction_number: Some(format!("TXN-{:03}", i)),
                transaction_type: "SALE".to_string(),
                items_json: "[]".to_string(),
                payments_json: "[]".to_string(),
                subtotal: Decimal::from(i) * Decimal::from(10),
                tax_total: Decimal::from(i),
                discount_total: Decimal::ZERO,
                grand_total: Decimal::from(i) * Decimal::from(11),
                customer_id: None,
                customer_name: None,
                shift_id: None, // Don't use shift FK in tests
                operator_id: Some(OperatorId::new("op-1").unwrap()),
                terminal_id: None,
                receipt_number: None,
                notes: None,
                created_at: Utc::now().to_rfc3339(),
                sync_status: "PENDING".to_string(),
                server_id: None,
                retry_count: 0,
                last_error: None,
                last_retry_at: None,
                catalog_etag: None,
            };
            db.save_offline_transaction(&txn).unwrap();
        }

        let pending = db.get_pending_transactions(10).unwrap();
        assert_eq!(pending.len(), 5);

        let count = db.get_pending_transaction_count().unwrap();
        assert_eq!(count, 5);
    }

    #[test]
    fn test_sync_status_update() {
        let db = setup_db();

        let txn = OfflineTransactionRow {
            offline_id: "txn-1".to_string(),
            transaction_type: "SALE".to_string(),
            items_json: "[]".to_string(),
            payments_json: "[]".to_string(),
            subtotal: Decimal::from(100),
            tax_total: Decimal::from(10),
            discount_total: Decimal::ZERO,
            grand_total: Decimal::from(110),
            created_at: Utc::now().to_rfc3339(),
            sync_status: "PENDING".to_string(),
            ..Default::default()
        };

        db.save_offline_transaction(&txn).unwrap();

        // Mark as synced
        db.mark_transaction_synced("txn-1", "server-id-123")
            .unwrap();

        let found = db.get_offline_transaction("txn-1").unwrap().unwrap();
        assert_eq!(found.sync_status, "SYNCED");
        assert_eq!(found.server_id, Some("server-id-123".to_string()));
    }

    // ------------------------------------------------------------------------------------------
    // `OFFLINE_TRANSACTION_ROW`. The three patterns from task 04.
    // ------------------------------------------------------------------------------------------

    /// A database holding the shift and operator the fixture points at.
    ///
    /// `offline_transactions.shift_id` and `.operator_id` are real foreign keys. Blanking either
    /// to dodge them would make it indistinguishable from the ten other absent columns, which is
    /// the failure the distinct-value discipline exists to avoid.
    fn setup_db_with_referents() -> Database {
        let db = setup_db();
        create_test_operator(&db, "operator-id-column");
        db.execute(
            "INSERT INTO shifts (id, shift_number, operator_id, terminal_id, opening_cash, started_at)
             VALUES ('shift-id-column', 'S1', 'operator-id-column', 'T1', 0, datetime('now'))",
            &[],
        )
        .expect("a referent shift");
        db
    }

    /// A transaction whose every column holds a value found nowhere else in the row.
    fn a_transaction_with_no_two_columns_alike() -> OfflineTransactionRow {
        OfflineTransactionRow {
            offline_id: "offline-id-column".to_string(),
            transaction_number: Some("transaction-number-column".to_string()),
            transaction_type: "RETURN".to_string(),
            items_json: r#"["items-json-column"]"#.to_string(),
            payments_json: r#"["payments-json-column"]"#.to_string(),
            subtotal: Decimal::from_str("11.11").unwrap(),
            tax_total: Decimal::from_str("22.22").unwrap(),
            discount_total: Decimal::from_str("33.33").unwrap(),
            grand_total: Decimal::from_str("44.44").unwrap(),
            customer_id: Some("customer-id-column".to_string()),
            customer_name: Some("customer-name-column".to_string()),
            shift_id: Some("shift-id-column".to_string()),
            operator_id: Some(OperatorId::new("operator-id-column").unwrap()),
            terminal_id: Some("terminal-id-column".to_string()),
            receipt_number: Some("receipt-number-column".to_string()),
            notes: Some("notes-column".to_string()),
            created_at: "2026-08-24T10:00:00Z".to_string(),
            // Not `PENDING`: that is the column's SQL `DEFAULT`, so a value that never reached the
            // store would read back as one that did.
            sync_status: "CONFLICT".to_string(),
            server_id: Some("server-id-column".to_string()),
            // Not 0, for the same reason.
            retry_count: 7,
            last_error: Some("last-error-column".to_string()),
            last_retry_at: Some("2026-08-24T11:00:00Z".to_string()),
            catalog_etag: Some("catalog-etag-column".to_string()),
        }
    }

    #[test]
    fn the_transaction_mapping_names_every_column_it_writes_in_the_order_it_reads_them() {
        // The list the `INSERT` and all four `SELECT`s used to spell separately, byte for byte.
        assert_eq!(
            OFFLINE_TRANSACTION_ROW.reader().select_list(),
            "offline_id, transaction_number, transaction_type, items_json, payments_json, \
             subtotal, tax_total, discount_total, grand_total, customer_id, customer_name, \
             shift_id, operator_id, terminal_id, receipt_number, notes, created_at, sync_status, \
             server_id, retry_count, last_error, last_retry_at, catalog_etag"
        );
        assert_eq!(OFFLINE_TRANSACTION_ROW.reader().width(), 23);
        // No `managed` column on this table, so the two lists are the same length — the one
        // mapping so far where they are.
        assert_eq!(OFFLINE_TRANSACTION_ROW.insert_column_names().count(), 23);
    }

    #[test]
    fn every_column_of_a_fully_distinct_transaction_survives_the_round_trip() {
        let db = setup_db_with_referents();
        let written = a_transaction_with_no_two_columns_alike();
        db.save_offline_transaction(&written).unwrap();

        let read = db
            .get_offline_transaction("offline-id-column")
            .unwrap()
            .expect("the transaction this test just wrote");

        assert_eq!(read.offline_id, written.offline_id);
        assert_eq!(read.transaction_number, written.transaction_number);
        assert_eq!(read.transaction_type, written.transaction_type);
        assert_eq!(read.items_json, written.items_json);
        assert_eq!(read.payments_json, written.payments_json);
        assert_eq!(read.subtotal, written.subtotal);
        assert_eq!(read.tax_total, written.tax_total);
        assert_eq!(read.discount_total, written.discount_total);
        assert_eq!(read.grand_total, written.grand_total);
        assert_eq!(read.customer_id, written.customer_id);
        assert_eq!(read.customer_name, written.customer_name);
        assert_eq!(read.shift_id, written.shift_id);
        assert_eq!(read.operator_id, written.operator_id);
        assert_eq!(read.terminal_id, written.terminal_id);
        assert_eq!(read.receipt_number, written.receipt_number);
        assert_eq!(read.notes, written.notes);
        assert_eq!(read.created_at, written.created_at);
        assert_eq!(read.sync_status, written.sync_status);
        assert_eq!(read.server_id, written.server_id);
        assert_eq!(read.retry_count, written.retry_count);
        assert_eq!(read.last_error, written.last_error);
        assert_eq!(read.last_retry_at, written.last_retry_at);
        assert_eq!(read.catalog_etag, written.catalog_etag);
    }

    /// Reads every column of the single stored transaction back **by name** and asserts it holds
    /// the value belonging to it. The asymmetric half — a round trip is invariant under a
    /// permutation applied to both the write and the read.
    fn assert_every_transaction_column_holds_its_own_value(db: &Database) {
        let conn = db.connection();
        let conn = conn.lock();

        for (column, expected) in [
            ("offline_id", "offline-id-column"),
            ("transaction_number", "transaction-number-column"),
            ("transaction_type", "RETURN"),
            ("items_json", r#"["items-json-column"]"#),
            ("payments_json", r#"["payments-json-column"]"#),
            ("customer_id", "customer-id-column"),
            ("customer_name", "customer-name-column"),
            ("shift_id", "shift-id-column"),
            ("operator_id", "operator-id-column"),
            ("terminal_id", "terminal-id-column"),
            ("receipt_number", "receipt-number-column"),
            ("notes", "notes-column"),
            ("created_at", "2026-08-24T10:00:00Z"),
            ("sync_status", "CONFLICT"),
            ("server_id", "server-id-column"),
            ("last_error", "last-error-column"),
            ("last_retry_at", "2026-08-24T11:00:00Z"),
            ("catalog_etag", "catalog-etag-column"),
        ] {
            let matched: bool = conn
                .query_row(
                    &format!("SELECT {column} = ?1 FROM offline_transactions"),
                    [expected],
                    |row| row.get(0),
                )
                .unwrap();
            assert!(matched, "the `{column}` column does not hold `{expected}`");
        }

        // The four money columns and the counter. Every value is distinct, so a swap among the
        // four `REAL`s is visible — four equal totals would hide it.
        for (column, expected) in [
            ("subtotal", 11.11_f64),
            ("tax_total", 22.22),
            ("discount_total", 33.33),
            ("grand_total", 44.44),
            ("retry_count", 7.0),
        ] {
            let matched: bool = conn
                .query_row(
                    &format!("SELECT {column} = ?1 FROM offline_transactions"),
                    [expected],
                    |row| row.get(0),
                )
                .unwrap();
            assert!(matched, "the `{column}` column does not hold `{expected}`");
        }
    }

    #[test]
    fn save_offline_transaction_puts_each_value_in_the_column_that_carries_its_name() {
        let db = setup_db_with_referents();
        db.save_offline_transaction(&a_transaction_with_no_two_columns_alike())
            .unwrap();
        assert_every_transaction_column_holds_its_own_value(&db);
    }

    #[test]
    fn a_second_write_of_the_same_offline_id_replaces_the_row() {
        // `INSERT OR REPLACE` is what the hand-written statement said, and a sync retry re-sends a
        // transaction the till already holds. Under `Fail` the retry would error; under no
        // conflict clause the primary key would.
        let db = setup_db_with_referents();
        let first = a_transaction_with_no_two_columns_alike();
        db.save_offline_transaction(&first).unwrap();
        db.save_offline_transaction(&OfflineTransactionRow {
            receipt_number: Some("second-write".to_string()),
            ..first
        })
        .unwrap();

        let rows: i64 = db
            .select_scalar("SELECT COUNT(*) FROM offline_transactions", [])
            .unwrap();
        assert_eq!(rows, 1, "the second write inserted rather than replaced");
        let stored = db
            .get_offline_transaction("offline-id-column")
            .unwrap()
            .unwrap();
        assert_eq!(stored.receipt_number.as_deref(), Some("second-write"));
        assert_eq!(stored.notes.as_deref(), Some("notes-column"));
    }

    /// One nullable transaction column, blanked, and what must still hold of its neighbours.
    struct AbsentTransactionColumn {
        column: &'static str,
        blank: fn(&mut OfflineTransactionRow),
        assert_absent: fn(&OfflineTransactionRow),
    }

    #[test]
    fn a_null_in_one_transaction_column_reaches_that_columns_field_and_no_other() {
        // Per column, never all at once: this row has twelve nullable columns, and twelve `None`s
        // are identical under any permutation of them.
        let db = setup_db_with_referents();
        let full = a_transaction_with_no_two_columns_alike();

        let cases = [
            AbsentTransactionColumn {
                column: "transaction_number",
                blank: |row| row.transaction_number = None,
                assert_absent: |row| {
                    assert_eq!(row.transaction_number, None);
                    assert_eq!(row.receipt_number.as_deref(), Some("receipt-number-column"));
                },
            },
            AbsentTransactionColumn {
                column: "customer_id",
                blank: |row| row.customer_id = None,
                assert_absent: |row| {
                    assert_eq!(row.customer_id, None);
                    assert_eq!(row.customer_name.as_deref(), Some("customer-name-column"));
                },
            },
            AbsentTransactionColumn {
                column: "customer_name",
                blank: |row| row.customer_name = None,
                assert_absent: |row| {
                    assert_eq!(row.customer_name, None);
                    assert_eq!(row.customer_id.as_deref(), Some("customer-id-column"));
                },
            },
            AbsentTransactionColumn {
                column: "operator_id",
                blank: |row| row.operator_id = None,
                assert_absent: |row| {
                    assert_eq!(row.operator_id, None);
                    assert_eq!(row.shift_id.as_deref(), Some("shift-id-column"));
                },
            },
            AbsentTransactionColumn {
                column: "shift_id",
                blank: |row| row.shift_id = None,
                assert_absent: |row| {
                    assert_eq!(row.shift_id, None);
                    assert!(row.operator_id.is_some());
                },
            },
            AbsentTransactionColumn {
                column: "terminal_id",
                blank: |row| row.terminal_id = None,
                assert_absent: |row| {
                    assert_eq!(row.terminal_id, None);
                    assert_eq!(row.receipt_number.as_deref(), Some("receipt-number-column"));
                },
            },
            AbsentTransactionColumn {
                column: "receipt_number",
                blank: |row| row.receipt_number = None,
                assert_absent: |row| {
                    assert_eq!(row.receipt_number, None);
                    assert_eq!(row.notes.as_deref(), Some("notes-column"));
                },
            },
            AbsentTransactionColumn {
                column: "notes",
                blank: |row| row.notes = None,
                assert_absent: |row| {
                    assert_eq!(row.notes, None);
                    assert_eq!(row.created_at, "2026-08-24T10:00:00Z");
                },
            },
            AbsentTransactionColumn {
                column: "server_id",
                blank: |row| row.server_id = None,
                assert_absent: |row| {
                    assert_eq!(row.server_id, None);
                    assert_eq!(row.retry_count, 7);
                },
            },
            AbsentTransactionColumn {
                column: "last_error",
                blank: |row| row.last_error = None,
                assert_absent: |row| {
                    assert_eq!(row.last_error, None);
                    assert_eq!(row.last_retry_at.as_deref(), Some("2026-08-24T11:00:00Z"));
                },
            },
            AbsentTransactionColumn {
                column: "last_retry_at",
                blank: |row| row.last_retry_at = None,
                assert_absent: |row| {
                    assert_eq!(row.last_retry_at, None);
                    assert_eq!(row.last_error.as_deref(), Some("last-error-column"));
                },
            },
            AbsentTransactionColumn {
                column: "catalog_etag",
                blank: |row| row.catalog_etag = None,
                assert_absent: |row| {
                    assert_eq!(row.catalog_etag, None);
                    assert_eq!(row.last_retry_at.as_deref(), Some("2026-08-24T11:00:00Z"));
                },
            },
        ];

        for case in cases {
            let mut written = full.clone();
            (case.blank)(&mut written);
            db.save_offline_transaction(&written).unwrap();

            let stored: Option<String> = db
                .select_scalar(
                    &format!("SELECT {} FROM offline_transactions", case.column),
                    [],
                )
                .unwrap();
            assert_eq!(stored, None, "`{}` was not written as NULL", case.column);

            let read = db
                .get_offline_transaction("offline-id-column")
                .unwrap()
                .expect("the row this iteration wrote");
            (case.assert_absent)(&read);
        }
    }

    #[test]
    fn every_reader_of_this_table_returns_the_same_row() {
        // Four readers shared one hand-copied projection and now share one declaration. This is
        // what would have caught a fifth copy drifting — which is exactly what happened in
        // `return_service.rs`, and is task 12.
        let db = setup_db_with_referents();
        let written = OfflineTransactionRow {
            sync_status: "PENDING".to_string(),
            ..a_transaction_with_no_two_columns_alike()
        };
        db.save_offline_transaction(&written).unwrap();

        let by_id = db
            .get_offline_transaction("offline-id-column")
            .unwrap()
            .expect("by id");
        let pending = db.get_pending_transactions(10).unwrap();
        let by_status = db.get_transactions_by_status("PENDING").unwrap();
        let by_shift = db.get_transactions_by_shift("shift-id-column").unwrap();

        assert_eq!(pending.len(), 1);
        assert_eq!(by_status.len(), 1);
        assert_eq!(by_shift.len(), 1);
        for (name, row) in [
            ("get_pending_transactions", &pending[0]),
            ("get_transactions_by_status", &by_status[0]),
            ("get_transactions_by_shift", &by_shift[0]),
        ] {
            assert_eq!(row.offline_id, by_id.offline_id, "{name}");
            assert_eq!(row.grand_total, by_id.grand_total, "{name}");
            assert_eq!(row.operator_id, by_id.operator_id, "{name}");
            assert_eq!(row.catalog_etag, by_id.catalog_etag, "{name}");
            assert_eq!(row.retry_count, by_id.retry_count, "{name}");
        }
    }
}
