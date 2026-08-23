//! Conflict Service - Manager conflict resolution
//!
//! This service handles sync conflict resolution for offline transactions.
//! Conflicts occur when a transaction cannot be synced due to:
//! - Duplicate transaction ID
//! - Invalid reference (deleted product, customer, etc.)
//! - Server-side conflict
//!
//! ## Features
//!
//! - Get list of conflict transactions
//! - Retry a conflict (reset and re-queue)
//! - Force sync (with manager override)
//! - Discard transaction (manager decides to not sync)
//!
//! ## Usage
//!
//! ```rust,ignore
//! use crate::services::ConflictService;
//! use std::sync::Arc;
//!
//! let conflict_service = ConflictService::new(Arc::clone(&api), Arc::clone(&db));
//!
//! // Get all conflicts
//! let conflicts = conflict_service.get_conflicts()?;
//!
//! // Retry a conflict
//! conflict_service.retry("offline-id-123")?;
//!
//! // Discard a conflict
//! conflict_service.discard("offline-id-456")?;
//! ```

use pos_api::{ApiClient, UploadOfflineTransactionRequest};
use pos_db::transactions::OfflineTransactionRow;
use pos_db::Database;
use pos_models::OperatorId;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{error, info, warn};

/// Error type for conflict operations
#[derive(Debug, Clone)]
pub enum ConflictError {
    /// Transaction not found
    NotFound(String),
    /// Transaction is not in conflict state
    NotConflict(String),
    /// Database error
    DatabaseError(String),
    /// API error
    ApiError(String),
    /// Manager authorization required
    AuthorizationRequired,
}

impl std::fmt::Display for ConflictError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConflictError::NotFound(id) => write!(f, "Transaction not found: {}", id),
            ConflictError::NotConflict(id) => {
                write!(f, "Transaction is not in conflict state: {}", id)
            }
            ConflictError::DatabaseError(msg) => write!(f, "Database error: {}", msg),
            ConflictError::ApiError(msg) => write!(f, "API error: {}", msg),
            ConflictError::AuthorizationRequired => write!(f, "Manager authorization required"),
        }
    }
}

impl std::error::Error for ConflictError {}

/// Result type for conflict operations
pub type ConflictResult<T> = std::result::Result<T, ConflictError>;

/// Summary of a conflict transaction for UI display
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConflictSummary {
    /// Offline transaction ID
    pub offline_id: String,
    /// Transaction number (e.g., "TXN-001")
    pub transaction_number: String,
    /// Transaction type (SALE, RETURN, etc.)
    pub transaction_type: String,
    /// Grand total amount
    pub grand_total: Decimal,
    /// When the transaction was created
    pub created_at: String,
    /// The error message that caused the conflict
    pub error_message: String,
    /// Number of retry attempts
    pub retry_count: i32,
    /// Operator name (if available)
    pub operator_id: Option<OperatorId>,
    /// Receipt number (if available)
    pub receipt_number: Option<String>,
}

impl From<&OfflineTransactionRow> for ConflictSummary {
    fn from(row: &OfflineTransactionRow) -> Self {
        Self {
            offline_id: row.offline_id.clone(),
            transaction_number: row
                .transaction_number
                .clone()
                .unwrap_or_else(|| "N/A".to_string()),
            transaction_type: row.transaction_type.clone(),
            grand_total: row.grand_total,
            created_at: row.created_at.clone(),
            error_message: row
                .last_error
                .clone()
                .unwrap_or_else(|| "Unknown error".to_string()),
            retry_count: row.retry_count,
            operator_id: row.operator_id.clone(),
            receipt_number: row.receipt_number.clone(),
        }
    }
}

/// Result of a conflict resolution action
#[derive(Debug, Clone)]
pub struct ResolutionResult {
    /// Whether the action was successful
    pub success: bool,
    /// Message describing the result
    pub message: String,
    /// New status of the transaction (if changed)
    pub new_status: Option<String>,
}

/// Conflict resolution service
pub struct ConflictService {
    api: Arc<ApiClient>,
    db: Arc<Database>,
}

impl ConflictService {
    /// Creates a new conflict service
    ///
    /// # Arguments
    ///
    /// * `api` - Shared API client for backend communication
    /// * `db` - Shared database for local storage
    pub fn new(api: Arc<ApiClient>, db: Arc<Database>) -> Self {
        Self { api, db }
    }

    /// Gets all transactions in conflict state
    ///
    /// # Returns
    ///
    /// List of conflict summaries for UI display
    pub fn get_conflicts(&self) -> ConflictResult<Vec<ConflictSummary>> {
        let rows = self
            .db
            .get_conflict_transactions()
            .map_err(|e| ConflictError::DatabaseError(e.to_string()))?;

        Ok(rows.iter().map(ConflictSummary::from).collect())
    }

    /// Gets the count of conflict transactions
    pub fn get_conflict_count(&self) -> ConflictResult<i64> {
        self.db
            .get_conflict_count()
            .map_err(|e| ConflictError::DatabaseError(e.to_string()))
    }

    /// Checks if there are any conflicts needing resolution
    pub fn has_conflicts(&self) -> ConflictResult<bool> {
        Ok(self.get_conflict_count()? > 0)
    }

    /// Gets a specific conflict transaction by ID
    ///
    /// # Arguments
    ///
    /// * `offline_id` - The offline transaction ID
    ///
    /// # Returns
    ///
    /// The conflict summary if found
    pub fn get_conflict(&self, offline_id: &str) -> ConflictResult<ConflictSummary> {
        let row = self
            .db
            .get_offline_transaction(offline_id)
            .map_err(|e| ConflictError::DatabaseError(e.to_string()))?
            .ok_or_else(|| ConflictError::NotFound(offline_id.to_string()))?;

        if row.sync_status != "CONFLICT" {
            return Err(ConflictError::NotConflict(offline_id.to_string()));
        }

        Ok(ConflictSummary::from(&row))
    }

    /// Retries a conflict transaction
    ///
    /// Resets the transaction status to PENDING and clears retry count,
    /// allowing it to be picked up by the next sync cycle.
    ///
    /// # Arguments
    ///
    /// * `offline_id` - The offline transaction ID
    ///
    /// # Returns
    ///
    /// Result indicating success or failure
    pub fn retry(&self, offline_id: &str) -> ConflictResult<ResolutionResult> {
        // Verify transaction exists and is in conflict state
        let _conflict = self.get_conflict(offline_id)?;

        // Reset for retry
        self.db
            .reset_transaction_for_retry(offline_id)
            .map_err(|e| ConflictError::DatabaseError(e.to_string()))?;

        info!("Conflict transaction reset for retry: {}", offline_id);

        Ok(ResolutionResult {
            success: true,
            message: "Transaction queued for retry".to_string(),
            new_status: Some("PENDING".to_string()),
        })
    }

    /// Discards a conflict transaction
    ///
    /// Marks the transaction as DISCARDED, preventing further sync attempts.
    /// This should only be used when the manager determines the transaction
    /// should not be synced (e.g., duplicate, test transaction, etc.)
    ///
    /// # Arguments
    ///
    /// * `offline_id` - The offline transaction ID
    ///
    /// # Returns
    ///
    /// Result indicating success or failure
    pub fn discard(&self, offline_id: &str) -> ConflictResult<ResolutionResult> {
        // Verify transaction exists and is in conflict state
        let _conflict = self.get_conflict(offline_id)?;

        // Mark as discarded
        self.db
            .discard_transaction(offline_id)
            .map_err(|e| ConflictError::DatabaseError(e.to_string()))?;

        warn!("Conflict transaction discarded: {}", offline_id);

        Ok(ResolutionResult {
            success: true,
            message: "Transaction discarded".to_string(),
            new_status: Some("DISCARDED".to_string()),
        })
    }

    /// Force syncs a conflict transaction (with manager override)
    ///
    /// Attempts to sync the transaction with a force flag, instructing
    /// the backend to accept it despite the conflict. This requires
    /// manager authorization.
    ///
    /// # Arguments
    ///
    /// * `offline_id` - The offline transaction ID
    ///
    /// # Returns
    ///
    /// Result indicating success or failure
    pub async fn force_sync(&self, offline_id: &str) -> ConflictResult<ResolutionResult> {
        // Verify transaction exists and is in conflict state
        let _conflict = self.get_conflict(offline_id)?;

        // Get the full transaction row
        let row = self
            .db
            .get_offline_transaction(offline_id)
            .map_err(|e| ConflictError::DatabaseError(e.to_string()))?
            .ok_or_else(|| ConflictError::NotFound(offline_id.to_string()))?;

        // Build force sync request
        let request = UploadOfflineTransactionRequest {
            offline_id: row.offline_id.clone(),
            transaction_number: row.transaction_number,
            transaction_type: row.transaction_type,
            items: serde_json::from_str(&row.items_json)
                .map_err(|e| ConflictError::DatabaseError(format!("Invalid items JSON: {}", e)))?,
            payments: serde_json::from_str(&row.payments_json).map_err(|e| {
                ConflictError::DatabaseError(format!("Invalid payments JSON: {}", e))
            })?,
            subtotal: row.subtotal,
            tax_total: row.tax_total,
            discount_total: row.discount_total,
            grand_total: row.grand_total,
            customer_id: row.customer_id,
            customer_name: row.customer_name,
            shift_id: row.shift_id,
            operator_id: row.operator_id,
            terminal_id: row.terminal_id,
            receipt_number: row.receipt_number,
            notes: row.notes,
            created_at: row.created_at,
            // This resolver's whole purpose: the operator has chosen this version, so the
            // platform's conflict check is being overridden deliberately.
            force: true,
            catalog_etag: None,
        };

        // Attempt force sync
        match self.api.upload_offline_transaction(&request).await {
            Ok(response) => {
                // Mark as synced
                self.db
                    .mark_transaction_synced(offline_id, &response.id)
                    .map_err(|e| ConflictError::DatabaseError(e.to_string()))?;

                info!(
                    "Conflict transaction force synced: {} -> {}",
                    offline_id, response.id
                );

                Ok(ResolutionResult {
                    success: true,
                    message: format!("Transaction synced with server ID: {}", response.id),
                    new_status: Some("SYNCED".to_string()),
                })
            }
            Err(e) => {
                error!("Force sync failed for {}: {}", offline_id, e);
                Err(ConflictError::ApiError(e.to_string()))
            }
        }
    }

    /// Retries all conflict transactions
    ///
    /// Resets all transactions in conflict state to PENDING.
    ///
    /// # Returns
    ///
    /// Number of transactions reset
    pub fn retry_all(&self) -> ConflictResult<usize> {
        let conflicts = self.get_conflicts()?;
        let count = conflicts.len();

        for conflict in conflicts {
            if let Err(e) = self.db.reset_transaction_for_retry(&conflict.offline_id) {
                error!("Failed to reset conflict {}: {}", conflict.offline_id, e);
            }
        }

        info!("Reset {} conflict transactions for retry", count);
        Ok(count)
    }

    /// Discards all conflict transactions
    ///
    /// Marks all transactions in conflict state as DISCARDED.
    ///
    /// # Returns
    ///
    /// Number of transactions discarded
    pub fn discard_all(&self) -> ConflictResult<usize> {
        let conflicts = self.get_conflicts()?;
        let count = conflicts.len();

        for conflict in conflicts {
            if let Err(e) = self.db.discard_transaction(&conflict.offline_id) {
                error!("Failed to discard conflict {}: {}", conflict.offline_id, e);
            }
        }

        warn!("Discarded {} conflict transactions", count);
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use pos_db::init_memory_database;
    use pos_db::operators::OperatorRow;
    use pos_db::transactions::OfflineTransactionRow;
    use pos_models::{OperatorName, OperatorRole};
    use rust_decimal::Decimal;

    fn setup() -> (Arc<ApiClient>, Arc<Database>) {
        let api = Arc::new(ApiClient::new("http://localhost:3000"));
        let db = Arc::new(init_memory_database().unwrap());

        // Create test operator for foreign key reference
        let operator = OperatorRow {
            id: OperatorId::new("op-1").unwrap(),
            code: "C001".to_string(),
            employee_id: None,
            employee_number: None,
            name: OperatorName::new("Ahmed", None::<&str>).unwrap(),
            role: OperatorRole::Cashier,
            department: None,
            position: None,
            permissions: None,
            is_active: true,
        };
        db.save_operator(&operator).unwrap();

        (api, db)
    }

    fn create_conflict_transaction(db: &Database, offline_id: &str) {
        let txn = OfflineTransactionRow {
            offline_id: offline_id.to_string(),
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
            shift_id: None,
            operator_id: Some(OperatorId::new("op-1").unwrap()),
            terminal_id: Some("term-1".to_string()),
            receipt_number: Some("R001".to_string()),
            notes: None,
            created_at: Utc::now().to_rfc3339(),
            sync_status: "CONFLICT".to_string(),
            server_id: None,
            retry_count: 3,
            last_error: Some("Duplicate transaction".to_string()),
            last_retry_at: Some(Utc::now().to_rfc3339()),
            catalog_etag: None,
        };
        db.save_offline_transaction(&txn).unwrap();
    }

    #[test]
    fn test_conflict_service_creation() {
        let (api, db) = setup();
        let _service = ConflictService::new(api, db);
    }

    #[test]
    fn test_get_conflicts_empty() {
        let (api, db) = setup();
        let service = ConflictService::new(api, db);

        let conflicts = service.get_conflicts().unwrap();
        assert!(conflicts.is_empty());
    }

    #[test]
    fn test_get_conflicts() {
        let (api, db) = setup();
        create_conflict_transaction(&db, "conflict-1");
        create_conflict_transaction(&db, "conflict-2");

        let service = ConflictService::new(api, db);
        let conflicts = service.get_conflicts().unwrap();

        assert_eq!(conflicts.len(), 2);
    }

    #[test]
    fn test_get_conflict_count() {
        let (api, db) = setup();
        create_conflict_transaction(&db, "conflict-1");

        let service = ConflictService::new(api, db);
        let count = service.get_conflict_count().unwrap();

        assert_eq!(count, 1);
    }

    #[test]
    fn test_has_conflicts() {
        let (api, db) = setup();
        let service = ConflictService::new(Arc::clone(&api), Arc::clone(&db));

        assert!(!service.has_conflicts().unwrap());

        create_conflict_transaction(&db, "conflict-1");

        assert!(service.has_conflicts().unwrap());
    }

    #[test]
    fn test_get_conflict() {
        let (api, db) = setup();
        create_conflict_transaction(&db, "conflict-1");

        let service = ConflictService::new(api, db);
        let conflict = service.get_conflict("conflict-1").unwrap();

        assert_eq!(conflict.offline_id, "conflict-1");
        assert_eq!(conflict.transaction_number, "TXN-001");
        assert_eq!(conflict.retry_count, 3);
    }

    #[test]
    fn test_get_conflict_not_found() {
        let (api, db) = setup();
        let service = ConflictService::new(api, db);

        let result = service.get_conflict("nonexistent");
        assert!(matches!(result, Err(ConflictError::NotFound(_))));
    }

    #[test]
    fn test_retry_conflict() {
        let (api, db) = setup();
        create_conflict_transaction(&db, "conflict-1");

        let service = ConflictService::new(Arc::clone(&api), Arc::clone(&db));
        let result = service.retry("conflict-1").unwrap();

        assert!(result.success);
        assert_eq!(result.new_status, Some("PENDING".to_string()));

        // Verify transaction status changed
        let txn = db.get_offline_transaction("conflict-1").unwrap().unwrap();
        assert_eq!(txn.sync_status, "PENDING");
        assert_eq!(txn.retry_count, 0);
    }

    #[test]
    fn test_discard_conflict() {
        let (api, db) = setup();
        create_conflict_transaction(&db, "conflict-1");

        let service = ConflictService::new(Arc::clone(&api), Arc::clone(&db));
        let result = service.discard("conflict-1").unwrap();

        assert!(result.success);
        assert_eq!(result.new_status, Some("DISCARDED".to_string()));

        // Verify transaction status changed
        let txn = db.get_offline_transaction("conflict-1").unwrap().unwrap();
        assert_eq!(txn.sync_status, "DISCARDED");
    }

    #[test]
    fn test_retry_all() {
        let (api, db) = setup();
        create_conflict_transaction(&db, "conflict-1");
        create_conflict_transaction(&db, "conflict-2");

        let service = ConflictService::new(Arc::clone(&api), Arc::clone(&db));
        let count = service.retry_all().unwrap();

        assert_eq!(count, 2);

        // All should now be PENDING
        assert!(!service.has_conflicts().unwrap());
    }

    #[test]
    fn test_discard_all() {
        let (api, db) = setup();
        create_conflict_transaction(&db, "conflict-1");
        create_conflict_transaction(&db, "conflict-2");

        let service = ConflictService::new(Arc::clone(&api), Arc::clone(&db));
        let count = service.discard_all().unwrap();

        assert_eq!(count, 2);

        // All should now be DISCARDED
        assert!(!service.has_conflicts().unwrap());
    }

    #[test]
    fn test_conflict_summary_from_row() {
        let row = OfflineTransactionRow {
            offline_id: "test-1".to_string(),
            transaction_number: Some("TXN-123".to_string()),
            transaction_type: "SALE".to_string(),
            items_json: "[]".to_string(),
            payments_json: "[]".to_string(),
            subtotal: Decimal::from(100),
            tax_total: Decimal::from(15),
            discount_total: Decimal::from(5),
            grand_total: Decimal::from(110),
            customer_id: None,
            customer_name: None,
            shift_id: None,
            operator_id: Some(OperatorId::new("op-1").unwrap()),
            terminal_id: None,
            receipt_number: Some("R-001".to_string()),
            notes: None,
            created_at: "2025-01-01T10:00:00Z".to_string(),
            sync_status: "CONFLICT".to_string(),
            server_id: None,
            retry_count: 5,
            last_error: Some("Conflict error".to_string()),
            last_retry_at: None,
            catalog_etag: None,
        };

        let summary = ConflictSummary::from(&row);

        assert_eq!(summary.offline_id, "test-1");
        assert_eq!(summary.transaction_number, "TXN-123");
        assert_eq!(summary.grand_total, Decimal::from(110));
        assert_eq!(summary.retry_count, 5);
        assert_eq!(summary.error_message, "Conflict error");
    }
}
