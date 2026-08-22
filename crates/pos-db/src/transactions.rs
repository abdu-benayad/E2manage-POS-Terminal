//! Transactions Repository
//!
//! Handles offline transactions storage and management.

use std::str::FromStr;

use chrono::Utc;
use rusqlite::{OptionalExtension, Result as SqliteResult};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use pos_models::OperatorId;

use super::{decimal_from_sqlite, decimal_to_sqlite, Database};
use crate::column;
use crate::parse::ParseError;

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

impl Database {
    /// Saves an offline transaction
    pub fn save_offline_transaction(&self, txn: &OfflineTransactionRow) -> SqliteResult<()> {
        let subtotal_f64 = decimal_to_sqlite(&txn.subtotal);
        let tax_total_f64 = decimal_to_sqlite(&txn.tax_total);
        let discount_total_f64 = decimal_to_sqlite(&txn.discount_total);
        let grand_total_f64 = decimal_to_sqlite(&txn.grand_total);

        self.execute(
            r#"INSERT OR REPLACE INTO offline_transactions
               (offline_id, transaction_number, transaction_type, items_json, payments_json,
                subtotal, tax_total, discount_total, grand_total, customer_id, customer_name,
                shift_id, operator_id, terminal_id, receipt_number, notes, created_at,
                sync_status, server_id, retry_count, last_error, last_retry_at, catalog_etag)
               VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23)"#,
            &[
                &txn.offline_id,
                &txn.transaction_number,
                &txn.transaction_type,
                &txn.items_json,
                &txn.payments_json,
                &subtotal_f64,
                &tax_total_f64,
                &discount_total_f64,
                &grand_total_f64,
                &txn.customer_id,
                &txn.customer_name,
                &txn.shift_id,
                &txn.operator_id.as_ref().map(OperatorId::as_str),
                &txn.terminal_id,
                &txn.receipt_number,
                &txn.notes,
                &txn.created_at,
                &txn.sync_status,
                &txn.server_id,
                &txn.retry_count,
                &txn.last_error,
                &txn.last_retry_at,
                &txn.catalog_etag,
            ],
        )?;
        Ok(())
    }

    /// Gets an offline transaction by ID
    pub fn get_offline_transaction(
        &self,
        offline_id: &str,
    ) -> SqliteResult<Option<OfflineTransactionRow>> {
        let conn = self.connection();
        let conn = conn.lock();

        conn.query_row(
            r#"SELECT offline_id, transaction_number, transaction_type, items_json, payments_json,
                      subtotal, tax_total, discount_total, grand_total, customer_id, customer_name,
                      shift_id, operator_id, terminal_id, receipt_number, notes, created_at,
                      sync_status, server_id, retry_count, last_error, last_retry_at, catalog_etag
               FROM offline_transactions WHERE offline_id = ?1"#,
            [offline_id],
            |row| {
                Ok(OfflineTransactionRow {
                    offline_id: row.get(0)?,
                    transaction_number: row.get(1)?,
                    transaction_type: row.get(2)?,
                    items_json: row.get(3)?,
                    payments_json: row.get(4)?,
                    subtotal: decimal_from_sqlite(row.get(5)?),
                    tax_total: decimal_from_sqlite(row.get(6)?),
                    discount_total: decimal_from_sqlite(row.get(7)?),
                    grand_total: decimal_from_sqlite(row.get(8)?),
                    customer_id: row.get(9)?,
                    customer_name: row.get(10)?,
                    shift_id: row.get(11)?,
                    operator_id: column::optional_operator_id(row, 12)?,
                    terminal_id: row.get(13)?,
                    receipt_number: row.get(14)?,
                    notes: row.get(15)?,
                    created_at: row.get(16)?,
                    sync_status: row.get(17)?,
                    server_id: row.get(18)?,
                    retry_count: row.get(19)?,
                    last_error: row.get(20)?,
                    last_retry_at: row.get(21)?,
                    catalog_etag: row.get(22)?,
                })
            },
        )
        .optional()
    }

    /// Gets all pending offline transactions
    pub fn get_pending_transactions(&self, limit: i32) -> SqliteResult<Vec<OfflineTransactionRow>> {
        let conn = self.connection();
        let conn = conn.lock();

        let mut stmt = conn.prepare(
            r#"SELECT offline_id, transaction_number, transaction_type, items_json, payments_json,
                      subtotal, tax_total, discount_total, grand_total, customer_id, customer_name,
                      shift_id, operator_id, terminal_id, receipt_number, notes, created_at,
                      sync_status, server_id, retry_count, last_error, last_retry_at, catalog_etag
               FROM offline_transactions
               WHERE sync_status IN ('PENDING', 'FAILED')
               ORDER BY created_at ASC
               LIMIT ?1"#,
        )?;

        let rows = stmt.query_map([limit], |row: &rusqlite::Row| {
            Ok(OfflineTransactionRow {
                offline_id: row.get(0)?,
                transaction_number: row.get(1)?,
                transaction_type: row.get(2)?,
                items_json: row.get(3)?,
                payments_json: row.get(4)?,
                subtotal: decimal_from_sqlite(row.get(5)?),
                tax_total: decimal_from_sqlite(row.get(6)?),
                discount_total: decimal_from_sqlite(row.get(7)?),
                grand_total: decimal_from_sqlite(row.get(8)?),
                customer_id: row.get(9)?,
                customer_name: row.get(10)?,
                shift_id: row.get(11)?,
                operator_id: column::optional_operator_id(row, 12)?,
                terminal_id: row.get(13)?,
                receipt_number: row.get(14)?,
                notes: row.get(15)?,
                created_at: row.get(16)?,
                sync_status: row.get(17)?,
                server_id: row.get(18)?,
                retry_count: row.get(19)?,
                last_error: row.get(20)?,
                last_retry_at: row.get(21)?,
                catalog_etag: row.get(22)?,
            })
        })?;

        rows.collect()
    }

    /// Gets count of pending transactions
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

    /// Gets transactions by sync status
    pub fn get_transactions_by_status(
        &self,
        status: &str,
    ) -> SqliteResult<Vec<OfflineTransactionRow>> {
        let conn = self.connection();
        let conn = conn.lock();

        let mut stmt = conn.prepare(
            r#"SELECT offline_id, transaction_number, transaction_type, items_json, payments_json,
                      subtotal, tax_total, discount_total, grand_total, customer_id, customer_name,
                      shift_id, operator_id, terminal_id, receipt_number, notes, created_at,
                      sync_status, server_id, retry_count, last_error, last_retry_at, catalog_etag
               FROM offline_transactions
               WHERE sync_status = ?1
               ORDER BY created_at DESC"#,
        )?;

        let rows = stmt.query_map([status], |row: &rusqlite::Row| {
            Ok(OfflineTransactionRow {
                offline_id: row.get(0)?,
                transaction_number: row.get(1)?,
                transaction_type: row.get(2)?,
                items_json: row.get(3)?,
                payments_json: row.get(4)?,
                subtotal: decimal_from_sqlite(row.get(5)?),
                tax_total: decimal_from_sqlite(row.get(6)?),
                discount_total: decimal_from_sqlite(row.get(7)?),
                grand_total: decimal_from_sqlite(row.get(8)?),
                customer_id: row.get(9)?,
                customer_name: row.get(10)?,
                shift_id: row.get(11)?,
                operator_id: column::optional_operator_id(row, 12)?,
                terminal_id: row.get(13)?,
                receipt_number: row.get(14)?,
                notes: row.get(15)?,
                created_at: row.get(16)?,
                sync_status: row.get(17)?,
                server_id: row.get(18)?,
                retry_count: row.get(19)?,
                last_error: row.get(20)?,
                last_retry_at: row.get(21)?,
                catalog_etag: row.get(22)?,
            })
        })?;

        rows.collect()
    }

    /// Resets a transaction for retry (clears retry count and status)
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

    /// Gets transactions by shift
    pub fn get_transactions_by_shift(
        &self,
        shift_id: &str,
    ) -> SqliteResult<Vec<OfflineTransactionRow>> {
        let conn = self.connection();
        let conn = conn.lock();

        let mut stmt = conn.prepare(
            r#"SELECT offline_id, transaction_number, transaction_type, items_json, payments_json,
                      subtotal, tax_total, discount_total, grand_total, customer_id, customer_name,
                      shift_id, operator_id, terminal_id, receipt_number, notes, created_at,
                      sync_status, server_id, retry_count, last_error, last_retry_at, catalog_etag
               FROM offline_transactions
               WHERE shift_id = ?1
               ORDER BY created_at DESC"#,
        )?;

        let rows = stmt.query_map([shift_id], |row: &rusqlite::Row| {
            Ok(OfflineTransactionRow {
                offline_id: row.get(0)?,
                transaction_number: row.get(1)?,
                transaction_type: row.get(2)?,
                items_json: row.get(3)?,
                payments_json: row.get(4)?,
                subtotal: decimal_from_sqlite(row.get(5)?),
                tax_total: decimal_from_sqlite(row.get(6)?),
                discount_total: decimal_from_sqlite(row.get(7)?),
                grand_total: decimal_from_sqlite(row.get(8)?),
                customer_id: row.get(9)?,
                customer_name: row.get(10)?,
                shift_id: row.get(11)?,
                operator_id: column::optional_operator_id(row, 12)?,
                terminal_id: row.get(13)?,
                receipt_number: row.get(14)?,
                notes: row.get(15)?,
                created_at: row.get(16)?,
                sync_status: row.get(17)?,
                server_id: row.get(18)?,
                retry_count: row.get(19)?,
                last_error: row.get(20)?,
                last_retry_at: row.get(21)?,
                catalog_etag: row.get(22)?,
            })
        })?;

        rows.collect()
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
            pin_hash: "hash".to_string(),
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
}
