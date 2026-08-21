//! Offline Queue Service - Transaction queue management for offline mode
//!
//! This service manages offline transaction queuing and synchronization.
//! When the terminal is offline, transactions are stored locally in SQLite
//! and synced to the backend when connectivity is restored.
//!
//! ## Features
//!
//! - Queue transactions for offline storage
//! - Process pending transactions when online
//! - Retry failed transactions with exponential backoff
//! - Track sync status and server IDs
//! - Max retry limit to prevent infinite loops
//!
//! ## Usage
//!
//! ```rust,ignore
//! use crate::services::OfflineService;
//! use std::sync::Arc;
//!
//! let offline = OfflineService::new(Arc::clone(&api), Arc::clone(&db));
//!
//! // Queue a transaction
//! let offline_id = offline.queue_transaction(&transaction)?;
//!
//! // Check pending count
//! let count = offline.pending_count()?;
//!
//! // Process queue when online
//! let result = offline.process_queue().await?;
//! println!("Synced: {}, Failed: {}", result.synced, result.failed);
//! ```

use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use pos_api::ApiClient;
use pos_db::transactions::{OfflineTransactionRow, SyncStatus};
use pos_db::Database;
use pos_models::transaction::Transaction;
use pos_models::OperatorId;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

/// Maximum number of retry attempts before marking as failed
pub const MAX_RETRY_COUNT: i32 = 10;

/// Maximum number of transactions to process in one batch
pub const BATCH_SIZE: i32 = 50;

/// Classification of sync failure types for proper error handling
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncFailureType {
    /// Network error, timeout - will retry with backoff
    Transient,
    /// Duplicate, invalid reference - needs manager resolution
    Conflict,
    /// Invalid data - won't ever succeed, stop retrying
    Permanent,
}

impl SyncFailureType {
    /// Classifies an error into a failure type based on error message patterns
    ///
    /// # Arguments
    ///
    /// * `error` - The error to classify
    ///
    /// # Returns
    ///
    /// The appropriate SyncFailureType for handling
    pub fn classify(error: &anyhow::Error) -> Self {
        let msg = error.to_string().to_lowercase();

        // Conflict errors - need manager resolution
        if msg.contains("conflict")
            || msg.contains("already exists")
            || msg.contains("duplicate")
            || msg.contains("409")
        {
            return SyncFailureType::Conflict;
        }

        // Permanent errors - invalid data, won't succeed on retry
        if msg.contains("invalid")
            || msg.contains("not found")
            || msg.contains("400")
            || msg.contains("validation failed")
            || msg.contains("malformed")
        {
            return SyncFailureType::Permanent;
        }

        // Default to transient (network issues, timeouts, server errors)
        SyncFailureType::Transient
    }
}

/// Result of processing the offline queue
#[derive(Debug, Default, Clone)]
pub struct SyncResult {
    /// Number of transactions successfully synced
    pub synced: u32,
    /// Number of transactions that failed to sync (transient or permanent)
    pub failed: u32,
    /// Number of transactions skipped (max retries exceeded or in backoff)
    pub skipped: u32,
    /// Number of transactions with conflicts (need manager resolution)
    pub conflicts: u32,
}

impl SyncResult {
    /// Returns true if any transactions were processed
    pub fn has_activity(&self) -> bool {
        self.synced > 0 || self.failed > 0 || self.conflicts > 0
    }

    /// Returns total number of transactions attempted
    pub fn total_attempted(&self) -> u32 {
        self.synced + self.failed + self.conflicts
    }

    /// Returns true if there are any conflicts needing resolution
    pub fn has_conflicts(&self) -> bool {
        self.conflicts > 0
    }
}

/// Request payload for creating a transaction on the server
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CreateTransactionRequest {
    offline_id: String,
    transaction_number: Option<String>,
    transaction_type: String,
    items: serde_json::Value,
    payments: serde_json::Value,
    subtotal: Decimal,
    tax_total: Decimal,
    discount_total: Decimal,
    grand_total: Decimal,
    customer_id: Option<String>,
    customer_name: Option<String>,
    shift_id: Option<String>,
    operator_id: Option<OperatorId>,
    terminal_id: Option<String>,
    receipt_number: Option<String>,
    notes: Option<String>,
    created_at: String,
    /// Catalog ETag at the time this transaction was created (for price version tracking)
    #[serde(skip_serializing_if = "Option::is_none")]
    catalog_etag: Option<String>,
}

/// Response from server after creating a transaction
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateTransactionResponse {
    id: String,
    #[allow(dead_code)]
    #[serde(default)]
    transaction_number: Option<String>,
}

/// Offline queue service for managing transaction sync
pub struct OfflineService {
    api: Arc<ApiClient>,
    db: Arc<Database>,
}

impl OfflineService {
    /// Creates a new offline service
    ///
    /// # Arguments
    ///
    /// * `api` - Shared API client for backend communication
    /// * `db` - Shared database for local storage
    pub fn new(api: Arc<ApiClient>, db: Arc<Database>) -> Self {
        Self { api, db }
    }

    /// Calculates backoff delay in seconds based on retry count
    ///
    /// Uses exponential backoff: 1min, 2min, 4min, 8min, 16min, 32min, max 60min
    ///
    /// # Arguments
    ///
    /// * `retry_count` - Number of previous retry attempts
    ///
    /// # Returns
    ///
    /// Backoff delay in seconds
    fn calculate_backoff_seconds(retry_count: i32) -> i64 {
        let base = 60_i64; // 1 minute base
        let max = 3600_i64; // 1 hour max
        (base * 2_i64.pow(retry_count.min(6) as u32)).min(max)
    }

    /// Checks if a transaction is ready to be retried based on exponential backoff
    ///
    /// Returns true if:
    /// - Retry count is below max AND
    /// - Either no previous retry (first attempt) OR backoff period has elapsed
    ///
    /// # Arguments
    ///
    /// * `txn` - The offline transaction row to check
    ///
    /// # Returns
    ///
    /// true if the transaction should be retried now
    fn should_retry_now(txn: &OfflineTransactionRow) -> bool {
        if txn.retry_count >= MAX_RETRY_COUNT {
            return false;
        }

        match &txn.last_retry_at {
            None => true, // First attempt, no backoff needed
            Some(last_retry) => {
                match DateTime::parse_from_rfc3339(last_retry) {
                    Ok(last) => {
                        let backoff = Self::calculate_backoff_seconds(txn.retry_count);
                        let next_allowed = last + chrono::Duration::seconds(backoff);
                        Utc::now() >= next_allowed
                    }
                    Err(_) => true, // Invalid timestamp, allow retry
                }
            }
        }
    }

    /// Queues a transaction for offline storage
    ///
    /// Converts the transaction to a database row and saves it locally.
    /// Returns the generated offline ID for tracking.
    ///
    /// # Arguments
    ///
    /// * `txn` - The transaction to queue
    ///
    /// # Returns
    ///
    /// The offline ID (UUID) assigned to this transaction
    pub fn queue_transaction(&self, txn: &Transaction) -> Result<String> {
        self.queue_transaction_with_catalog(txn, None)
    }

    /// Queues a transaction for offline storage with catalog version tracking
    ///
    /// Converts the transaction to a database row and saves it locally.
    /// Stores the catalog ETag to track price version for offline transactions.
    ///
    /// # Arguments
    ///
    /// * `txn` - The transaction to queue
    /// * `catalog_etag` - The current catalog ETag (for price version tracking)
    ///
    /// # Returns
    ///
    /// The offline ID (UUID) assigned to this transaction
    pub fn queue_transaction_with_catalog(
        &self,
        txn: &Transaction,
        catalog_etag: Option<String>,
    ) -> Result<String> {
        let offline_id = Uuid::new_v4().to_string();

        // Serialize items and payments to JSON
        let items_json = serde_json::to_string(&txn.items)
            .map_err(|e| anyhow!("Failed to serialize items: {}", e))?;
        let payments_json = serde_json::to_string(&txn.payments)
            .map_err(|e| anyhow!("Failed to serialize payments: {}", e))?;

        let row = OfflineTransactionRow {
            offline_id: offline_id.clone(),
            transaction_number: Some(txn.transaction_number.clone()),
            transaction_type: txn.transaction_type.clone().to_uppercase(),
            items_json,
            payments_json,
            subtotal: txn.subtotal,
            tax_total: txn.tax_total,
            discount_total: txn.discount_total,
            grand_total: txn.grand_total,
            customer_id: txn.customer_id.clone(),
            customer_name: txn.customer_name.clone(),
            shift_id: Some(txn.shift_id.clone()),
            operator_id: Some(txn.operator_id.clone()),
            terminal_id: Some(txn.terminal_id.clone()),
            receipt_number: txn.receipt_number.clone(),
            notes: txn.note.clone(),
            created_at: Utc::now().to_rfc3339(),
            sync_status: SyncStatus::Pending.as_str().to_string(),
            server_id: None,
            retry_count: 0,
            last_error: None,
            last_retry_at: None,
            catalog_etag,
        };

        self.db
            .save_offline_transaction(&row)
            .map_err(|e| anyhow!("Failed to save offline transaction: {}", e))?;

        info!(
            "Transaction queued offline: {} ({})",
            offline_id, txn.transaction_number
        );

        Ok(offline_id)
    }

    /// Gets the count of pending transactions
    ///
    /// Returns the number of transactions waiting to be synced
    /// (status PENDING or FAILED with retry count < max).
    pub fn pending_count(&self) -> Result<u32> {
        let count = self
            .db
            .get_pending_transaction_count()
            .map_err(|e| anyhow!("Failed to get pending count: {}", e))?;
        Ok(count as u32)
    }

    /// Checks if there are any pending transactions
    pub fn has_pending(&self) -> Result<bool> {
        Ok(self.pending_count()? > 0)
    }

    /// Processes all pending transactions in the queue
    ///
    /// Iterates through pending transactions and attempts to sync each
    /// to the backend. Updates status based on success or failure.
    ///
    /// # Returns
    ///
    /// SyncResult containing counts of synced, failed, and skipped transactions
    pub async fn process_queue(&self) -> Result<SyncResult> {
        // Check if online first
        if !self.api.is_online().await.is_online() {
            debug!("Skipping offline queue processing: terminal is offline");
            return Ok(SyncResult::default());
        }

        let pending = self
            .db
            .get_pending_transactions(BATCH_SIZE)
            .map_err(|e| anyhow!("Failed to get pending transactions: {}", e))?;

        if pending.is_empty() {
            debug!("No pending transactions to process");
            return Ok(SyncResult::default());
        }

        info!("Processing {} pending transactions", pending.len());
        let mut result = SyncResult::default();

        for txn in pending {
            // Skip transactions based on backoff or max retries
            if !Self::should_retry_now(&txn) {
                if txn.retry_count >= MAX_RETRY_COUNT {
                    warn!(
                        "Skipping transaction {} - max retries ({}) exceeded",
                        txn.offline_id, MAX_RETRY_COUNT
                    );
                } else {
                    debug!(
                        "Skipping {} - backoff period not elapsed (retry {})",
                        txn.offline_id, txn.retry_count
                    );
                }
                result.skipped += 1;
                continue;
            }

            match self.sync_transaction(&txn).await {
                Ok(server_id) => {
                    if let Err(e) = self.db.mark_transaction_synced(&txn.offline_id, &server_id) {
                        error!(
                            "Failed to mark transaction {} as synced: {}",
                            txn.offline_id, e
                        );
                    }
                    result.synced += 1;
                    info!(
                        "Transaction synced: {} -> server_id: {}",
                        txn.offline_id, server_id
                    );
                }
                Err(e) => {
                    let error_msg = e.to_string();
                    let failure_type = SyncFailureType::classify(&e);

                    match failure_type {
                        SyncFailureType::Conflict => {
                            // Mark as conflict - needs manager resolution
                            if let Err(db_err) = self
                                .db
                                .mark_transaction_conflict(&txn.offline_id, &error_msg)
                            {
                                error!(
                                    "Failed to mark transaction {} as conflict: {}",
                                    txn.offline_id, db_err
                                );
                            }
                            warn!(
                                "Transaction conflict detected: {} - {}",
                                txn.offline_id, error_msg
                            );
                            result.conflicts += 1;
                        }
                        SyncFailureType::Permanent => {
                            // Set max retries to stop further attempts
                            if let Err(db_err) =
                                self.db.set_transaction_max_retries(&txn.offline_id)
                            {
                                error!(
                                    "Failed to set max retries for transaction {}: {}",
                                    txn.offline_id, db_err
                                );
                            }
                            if let Err(db_err) =
                                self.db.mark_transaction_failed(&txn.offline_id, &error_msg)
                            {
                                error!(
                                    "Failed to mark transaction {} as failed: {}",
                                    txn.offline_id, db_err
                                );
                            }
                            error!(
                                "Transaction permanently failed: {} - {}",
                                txn.offline_id, error_msg
                            );
                            result.failed += 1;
                        }
                        SyncFailureType::Transient => {
                            // Normal failure - will retry with backoff
                            if let Err(db_err) =
                                self.db.mark_transaction_failed(&txn.offline_id, &error_msg)
                            {
                                error!(
                                    "Failed to mark transaction {} as failed: {}",
                                    txn.offline_id, db_err
                                );
                            }
                            warn!(
                                "Transaction sync failed (transient): {} - {} (retry {})",
                                txn.offline_id,
                                error_msg,
                                txn.retry_count + 1
                            );
                            result.failed += 1;
                        }
                    }
                }
            }
        }

        info!(
            "Offline queue processed: {} synced, {} failed, {} conflicts, {} skipped",
            result.synced, result.failed, result.conflicts, result.skipped
        );

        Ok(result)
    }

    /// Syncs a single transaction to the backend
    ///
    /// Sends the transaction data to the server and returns the server-assigned ID.
    async fn sync_transaction(&self, txn: &OfflineTransactionRow) -> Result<String> {
        // Parse items and payments JSON back to Value for re-serialization
        let items: serde_json::Value = serde_json::from_str(&txn.items_json)
            .map_err(|e| anyhow!("Failed to parse items JSON: {}", e))?;
        let payments: serde_json::Value = serde_json::from_str(&txn.payments_json)
            .map_err(|e| anyhow!("Failed to parse payments JSON: {}", e))?;

        let request = CreateTransactionRequest {
            offline_id: txn.offline_id.clone(),
            transaction_number: txn.transaction_number.clone(),
            transaction_type: txn.transaction_type.clone(),
            items,
            payments,
            subtotal: txn.subtotal,
            tax_total: txn.tax_total,
            discount_total: txn.discount_total,
            grand_total: txn.grand_total,
            customer_id: txn.customer_id.clone(),
            customer_name: txn.customer_name.clone(),
            shift_id: txn.shift_id.clone(),
            operator_id: txn.operator_id.clone(),
            terminal_id: txn.terminal_id.clone(),
            receipt_number: txn.receipt_number.clone(),
            notes: txn.notes.clone(),
            created_at: txn.created_at.clone(),
            catalog_etag: txn.catalog_etag.clone(),
        };

        let response: CreateTransactionResponse =
            self.api.post("/api/pos/offline/upload", &request).await?;

        Ok(response.id)
    }

    /// Gets a specific offline transaction by ID
    pub fn get_transaction(&self, offline_id: &str) -> Result<Option<OfflineTransactionRow>> {
        self.db
            .get_offline_transaction(offline_id)
            .map_err(|e| anyhow!("Failed to get transaction {}: {}", offline_id, e))
    }

    /// Gets transactions for a specific shift
    pub fn get_shift_transactions(&self, shift_id: &str) -> Result<Vec<OfflineTransactionRow>> {
        self.db
            .get_transactions_by_shift(shift_id)
            .map_err(|e| anyhow!("Failed to get shift transactions: {}", e))
    }

    /// Cleans up old synced transactions
    ///
    /// Removes transactions that have been synced and are older than the
    /// specified number of days.
    ///
    /// # Arguments
    ///
    /// * `older_than_days` - Remove synced transactions older than this
    ///
    /// # Returns
    ///
    /// Number of transactions cleaned up
    pub fn cleanup_synced(&self, older_than_days: i64) -> Result<usize> {
        let count = self
            .db
            .cleanup_synced_transactions(older_than_days)
            .map_err(|e| anyhow!("Failed to cleanup synced transactions: {}", e))?;

        if count > 0 {
            info!(
                "Cleaned up {} synced transactions older than {} days",
                count, older_than_days
            );
        }

        Ok(count)
    }

    /// Gets queue statistics
    pub fn get_stats(&self) -> Result<QueueStats> {
        let pending = self.pending_count()?;

        // Get total count (all statuses)
        let conn = self.db.connection();
        let conn = conn.lock();

        let total: i64 = conn
            .query_row("SELECT COUNT(*) FROM offline_transactions", [], |row| {
                row.get(0)
            })
            .unwrap_or(0);

        let synced: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM offline_transactions WHERE sync_status = 'SYNCED'",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);

        let failed: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM offline_transactions WHERE sync_status = 'FAILED' AND retry_count >= ?1",
                [MAX_RETRY_COUNT],
                |row| row.get(0),
            )
            .unwrap_or(0);

        Ok(QueueStats {
            total: total as u32,
            pending,
            synced: synced as u32,
            failed_permanently: failed as u32,
        })
    }
}

/// Statistics about the offline queue
#[derive(Debug, Clone, Default)]
pub struct QueueStats {
    /// Total transactions in the queue
    pub total: u32,
    /// Pending transactions (not yet synced)
    pub pending: u32,
    /// Successfully synced transactions
    pub synced: u32,
    /// Transactions that failed permanently (max retries exceeded)
    pub failed_permanently: u32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use pos_db::init_memory_database;
    use pos_db::operators::OperatorRow;
    use pos_models::cart::{Cart, CartItem};
    use pos_models::product::{Product, ProductUnit};
    use rust_decimal::Decimal;

    fn op_id(id: &str) -> OperatorId {
        OperatorId::new(id).expect("a fixture id is never blank")
    }

    fn op_name(name: &str) -> pos_models::RecordedOperatorName {
        pos_models::RecordedOperatorName::new(name).expect("a fixture name is never blank")
    }

    /// Creates a database with required foreign key references
    fn setup() -> (Arc<ApiClient>, Arc<Database>) {
        let api = Arc::new(ApiClient::new("http://localhost:3000"));
        let db = Arc::new(init_memory_database().unwrap());

        // Create test operator for foreign key reference
        let operator = OperatorRow {
            id: "op-1".to_string(),
            code: "C001".to_string(),
            name: "Ahmed".to_string(),
            pin_hash: "hash".to_string(),
            ..Default::default()
        };
        db.save_operator(&operator).unwrap();

        // Create test shift for foreign key reference
        db.start_shift(
            "shift-1",
            "SHIFT-001",
            &op_id("op-1"),
            Some("TERM-001"),
            Decimal::from(100),
        )
        .unwrap();

        (api, db)
    }

    fn create_test_product() -> Product {
        Product {
            id: "prod-001".to_string(),
            sku: "SKU001".to_string(),
            barcode: Some("1234567890123".to_string()),
            name: "Test Product".to_string(),
            name_ar: Some("منتج تجريبي".to_string()),
            price: Decimal::from(10),
            tax_rate: Decimal::from(15),
            tax_inclusive: false,
            unit: ProductUnit::Unit,
            ..Default::default()
        }
    }

    fn create_test_transaction() -> Transaction {
        let product = create_test_product();
        let mut cart = Cart::new();
        cart.items.push(CartItem::new(&product, Decimal::from(2)));
        cart.recalculate();

        Transaction::from_cart(
            &cart,
            "LYD",
            "shift-1",
            "TERM-001",
            op_id("op-1"),
            op_name("Ahmed"),
        )
    }

    #[test]
    fn test_offline_service_creation() {
        let (api, db) = setup();
        let _service = OfflineService::new(api, db);
    }

    #[test]
    fn test_queue_transaction() {
        let (api, db) = setup();
        let service = OfflineService::new(api, db);

        let txn = create_test_transaction();
        let offline_id = service.queue_transaction(&txn).unwrap();

        assert!(!offline_id.is_empty());
        assert!(Uuid::parse_str(&offline_id).is_ok());
    }

    #[test]
    fn test_pending_count() {
        let (api, db) = setup();
        let service = OfflineService::new(api, db);

        // Initially no pending
        assert_eq!(service.pending_count().unwrap(), 0);
        assert!(!service.has_pending().unwrap());

        // Queue a transaction
        let txn = create_test_transaction();
        service.queue_transaction(&txn).unwrap();

        // Should have 1 pending
        assert_eq!(service.pending_count().unwrap(), 1);
        assert!(service.has_pending().unwrap());

        // Queue another
        let txn2 = create_test_transaction();
        service.queue_transaction(&txn2).unwrap();

        // Should have 2 pending
        assert_eq!(service.pending_count().unwrap(), 2);
    }

    #[test]
    fn test_get_transaction() {
        let (api, db) = setup();
        let service = OfflineService::new(api, db);

        let txn = create_test_transaction();
        let offline_id = service.queue_transaction(&txn).unwrap();

        let retrieved = service.get_transaction(&offline_id).unwrap();
        assert!(retrieved.is_some());

        let row = retrieved.unwrap();
        assert_eq!(row.offline_id, offline_id);
        assert_eq!(row.transaction_type, "SALE");
        assert_eq!(row.grand_total, Decimal::from(23));
    }

    #[test]
    fn test_get_nonexistent_transaction() {
        let (api, db) = setup();
        let service = OfflineService::new(api, db);

        let retrieved = service.get_transaction("nonexistent").unwrap();
        assert!(retrieved.is_none());
    }

    #[test]
    fn test_get_queue_stats() {
        let (api, db) = setup();
        let service = OfflineService::new(api, db);

        // Initially empty
        let stats = service.get_stats().unwrap();
        assert_eq!(stats.total, 0);
        assert_eq!(stats.pending, 0);
        assert_eq!(stats.synced, 0);

        // Queue some transactions
        for _ in 0..3 {
            let txn = create_test_transaction();
            service.queue_transaction(&txn).unwrap();
        }

        let stats = service.get_stats().unwrap();
        assert_eq!(stats.total, 3);
        assert_eq!(stats.pending, 3);
        assert_eq!(stats.synced, 0);
    }

    #[test]
    fn test_sync_result() {
        let result = SyncResult {
            synced: 5,
            failed: 2,
            skipped: 1,
            conflicts: 1,
        };

        assert!(result.has_activity());
        assert_eq!(result.total_attempted(), 8); // 5 + 2 + 1

        let empty = SyncResult::default();
        assert!(!empty.has_activity());
        assert_eq!(empty.total_attempted(), 0);
    }

    #[tokio::test]
    async fn test_process_queue_offline() {
        let (api, db) = setup();
        let service = OfflineService::new(api, db);

        // Queue a transaction
        let txn = create_test_transaction();
        service.queue_transaction(&txn).unwrap();

        // Process (will fail since we're not actually connected)
        // but should not error out
        let result = service.process_queue().await.unwrap();

        // Should either be empty (offline check) or failed (connection error)
        // Depending on whether is_online returns true or false
        assert!(result.synced == 0 || result.failed > 0);
    }

    #[test]
    fn test_cleanup_synced() {
        let (api, db) = setup();
        let db_clone = Arc::clone(&db);
        let service = OfflineService::new(api, db);

        // Queue and manually mark as synced
        let txn = create_test_transaction();
        let offline_id = service.queue_transaction(&txn).unwrap();

        // Mark as synced using the cloned db reference
        db_clone
            .mark_transaction_synced(&offline_id, "server-123")
            .unwrap();

        // Cleanup (0 days means anything synced)
        // This won't actually delete since the transaction was just created
        let cleaned = service.cleanup_synced(30).unwrap();
        // Recent transactions won't be cleaned
        assert_eq!(cleaned, 0);
    }

    #[test]
    fn test_constants() {
        assert_eq!(MAX_RETRY_COUNT, 10);
        assert_eq!(BATCH_SIZE, 50);
    }

    #[test]
    fn test_backoff_calculation() {
        // Verify exponential growth: 1min, 2min, 4min, 8min, 16min, 32min, 60min (max)
        assert_eq!(OfflineService::calculate_backoff_seconds(0), 60); // 1 min
        assert_eq!(OfflineService::calculate_backoff_seconds(1), 120); // 2 min
        assert_eq!(OfflineService::calculate_backoff_seconds(2), 240); // 4 min
        assert_eq!(OfflineService::calculate_backoff_seconds(3), 480); // 8 min
        assert_eq!(OfflineService::calculate_backoff_seconds(4), 960); // 16 min
        assert_eq!(OfflineService::calculate_backoff_seconds(5), 1920); // 32 min
        assert_eq!(OfflineService::calculate_backoff_seconds(6), 3600); // 60 min (max)
        assert_eq!(OfflineService::calculate_backoff_seconds(7), 3600); // Still max
        assert_eq!(OfflineService::calculate_backoff_seconds(10), 3600); // Still max
    }

    #[test]
    fn test_first_retry_allowed_immediately() {
        // First attempt (no last_retry_at) should always be allowed
        let txn = OfflineTransactionRow {
            offline_id: "test-1".to_string(),
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
            created_at: Utc::now().to_rfc3339(),
            sync_status: "PENDING".to_string(),
            server_id: None,
            retry_count: 0,
            last_error: None,
            last_retry_at: None, // No previous retry
            catalog_etag: None,
        };

        assert!(OfflineService::should_retry_now(&txn));
    }

    #[test]
    fn test_should_retry_respects_backoff() {
        // Transaction with recent retry should be skipped
        let recent_retry = Utc::now().to_rfc3339();
        let txn_recent = OfflineTransactionRow {
            offline_id: "test-2".to_string(),
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
            created_at: Utc::now().to_rfc3339(),
            sync_status: "FAILED".to_string(),
            server_id: None,
            retry_count: 1, // Has been retried once
            last_error: Some("Network error".to_string()),
            last_retry_at: Some(recent_retry), // Just retried now
            catalog_etag: None,
        };

        // Should NOT retry yet (backoff period not elapsed)
        assert!(!OfflineService::should_retry_now(&txn_recent));

        // Transaction with old retry (more than backoff period ago) should be allowed
        let old_retry = (Utc::now() - chrono::Duration::minutes(5)).to_rfc3339();
        let txn_old = OfflineTransactionRow {
            offline_id: "test-3".to_string(),
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
            created_at: Utc::now().to_rfc3339(),
            sync_status: "FAILED".to_string(),
            server_id: None,
            retry_count: 1, // First retry, needs 2min backoff
            last_error: Some("Network error".to_string()),
            last_retry_at: Some(old_retry), // 5 minutes ago, > 2 min backoff
            catalog_etag: None,
        };

        // Should retry (backoff period elapsed)
        assert!(OfflineService::should_retry_now(&txn_old));
    }

    #[test]
    fn test_max_retries_blocks_retry() {
        // Transaction at max retries should not be retried regardless of backoff
        let old_retry = (Utc::now() - chrono::Duration::hours(2)).to_rfc3339();
        let txn = OfflineTransactionRow {
            offline_id: "test-4".to_string(),
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
            created_at: Utc::now().to_rfc3339(),
            sync_status: "FAILED".to_string(),
            server_id: None,
            retry_count: MAX_RETRY_COUNT, // At max
            last_error: Some("Repeated failures".to_string()),
            last_retry_at: Some(old_retry), // Long ago
            catalog_etag: None,
        };

        // Should NOT retry (max retries exceeded)
        assert!(!OfflineService::should_retry_now(&txn));
    }

    // =========================================================================
    // SyncFailureType Tests
    // =========================================================================

    #[test]
    fn test_sync_failure_type_classify_conflict() {
        // Test various conflict error messages
        let conflict_errors = [
            "Conflict detected: duplicate transaction",
            "Transaction already exists",
            "DUPLICATE key violation",
            "HTTP 409: Conflict",
        ];

        for error_msg in &conflict_errors {
            let error = anyhow::anyhow!("{}", error_msg);
            assert_eq!(
                SyncFailureType::classify(&error),
                SyncFailureType::Conflict,
                "Expected Conflict for: {}",
                error_msg
            );
        }
    }

    #[test]
    fn test_sync_failure_type_classify_permanent() {
        // Test various permanent error messages
        let permanent_errors = [
            "Invalid product ID",
            "Customer not found",
            "HTTP 400: Bad Request",
            "Validation failed: amount must be positive",
            "Malformed JSON payload",
        ];

        for error_msg in &permanent_errors {
            let error = anyhow::anyhow!("{}", error_msg);
            assert_eq!(
                SyncFailureType::classify(&error),
                SyncFailureType::Permanent,
                "Expected Permanent for: {}",
                error_msg
            );
        }
    }

    #[test]
    fn test_sync_failure_type_classify_transient() {
        // Test various transient error messages (network issues, timeouts, server errors)
        let transient_errors = [
            "Connection refused",
            "Network timeout",
            "HTTP 500: Internal Server Error",
            "HTTP 503: Service Unavailable",
            "Failed to connect to server",
            "DNS lookup failed",
        ];

        for error_msg in &transient_errors {
            let error = anyhow::anyhow!("{}", error_msg);
            assert_eq!(
                SyncFailureType::classify(&error),
                SyncFailureType::Transient,
                "Expected Transient for: {}",
                error_msg
            );
        }
    }

    #[test]
    fn test_sync_failure_type_classify_case_insensitive() {
        // Ensure classification is case-insensitive
        let error = anyhow::anyhow!("CONFLICT DETECTED");
        assert_eq!(SyncFailureType::classify(&error), SyncFailureType::Conflict);

        let error = anyhow::anyhow!("INVALID data");
        assert_eq!(
            SyncFailureType::classify(&error),
            SyncFailureType::Permanent
        );
    }

    // =========================================================================
    // SyncResult Tests
    // =========================================================================

    #[test]
    fn test_sync_result_default() {
        let result = SyncResult::default();
        assert_eq!(result.synced, 0);
        assert_eq!(result.failed, 0);
        assert_eq!(result.skipped, 0);
        assert_eq!(result.conflicts, 0);
        assert!(!result.has_activity());
        assert!(!result.has_conflicts());
        assert_eq!(result.total_attempted(), 0);
    }

    #[test]
    fn test_sync_result_has_activity() {
        let single_counter = [
            SyncResult {
                synced: 1,
                ..Default::default()
            },
            SyncResult {
                failed: 1,
                ..Default::default()
            },
            SyncResult {
                conflicts: 1,
                ..Default::default()
            },
        ];

        for result in single_counter {
            assert!(result.has_activity(), "{result:?} should report activity");
        }
    }

    #[test]
    fn test_sync_result_has_conflicts() {
        let mut result = SyncResult::default();
        assert!(!result.has_conflicts());

        result.conflicts = 1;
        assert!(result.has_conflicts());

        result.conflicts = 5;
        assert!(result.has_conflicts());
    }

    #[test]
    fn test_sync_result_total_attempted() {
        let result = SyncResult {
            synced: 3,
            failed: 2,
            skipped: 10, // Skipped are NOT counted in attempted
            conflicts: 1,
        };

        assert_eq!(result.total_attempted(), 6); // 3 + 2 + 1
    }
}
