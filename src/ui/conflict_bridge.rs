//! Conflict Bridge - view-model conversion
//!
//! Provides types and utilities for bridging the Rust ConflictService
//! with the view layer. Converts conflict data to render-ready
//! types that can be displayed in the conflicts screen.
//!
//! ## Example
//!
//! ```rust,ignore
//! use e2manage_pos_terminal::services::ConflictService;
//! use e2manage_pos_terminal::ui::ConflictBridge;
//! use std::sync::Arc;
//!
//! let conflict_service = Arc::new(ConflictService::new(api, db));
//! let bridge = ConflictBridge::new(Arc::clone(&conflict_service));
//!
//! // Get conflicts for display
//! let conflicts = bridge.get_conflicts_model("LYD", "ar");
//! for conflict in conflicts {
//!     println!("{}: {} LYD - {}", conflict.transaction_number, conflict.grand_total, conflict.error_message);
//! }
//! ```

use crate::utils::formatting;
use pos_models::OperatorId;
use pos_services::ConflictService;
use std::sync::Arc;

/// Conflict transaction view model
///
/// This struct contains all the data needed to display a conflict
/// by the view layer.
#[derive(Clone, Debug, Default)]
pub struct ConflictModel {
    /// Offline transaction ID
    pub offline_id: String,
    /// Transaction number (e.g., "TXN-001")
    pub transaction_number: String,
    /// Transaction type (SALE, RETURN, etc.)
    pub transaction_type: String,
    /// Grand total amount (pre-formatted string like "123.450")
    pub grand_total: String,
    /// When the transaction was created
    pub created_at: String,
    /// The error message that caused the conflict
    pub error_message: String,
    /// Number of retry attempts
    pub retry_count: i32,
    /// The operator who rang the conflicting transaction, when the queued row records one.
    ///
    /// `Option`, not an empty string. The row's `operator_id` column is nullable, and a conflict
    /// with no operator is a different thing for a supervisor to look at than one belonging to a
    /// cashier called "". The view layer decides how to render the absence; the model does not
    /// decide it for them by flattening it away.
    pub operator_id: Option<OperatorId>,
    /// Receipt number (if available)
    pub receipt_number: String,
}

/// Bridge between ConflictService and the view layer
///
/// Provides methods to convert conflict data into render-ready types.
/// The bridge holds an Arc reference to the conflict service.
pub struct ConflictBridge {
    service: Arc<ConflictService>,
}

impl ConflictBridge {
    /// Creates a new conflict bridge with the given service
    pub fn new(service: Arc<ConflictService>) -> Self {
        Self { service }
    }

    /// Gets all conflict transactions formatted for display
    pub fn get_conflicts_model(&self, currency: &str, _locale: &str) -> Vec<ConflictModel> {
        match self.service.get_conflicts() {
            Ok(conflicts) => conflicts
                .into_iter()
                .map(|c| {
                    let decimals = formatting::currency_decimals(currency);
                    ConflictModel {
                        offline_id: c.offline_id,
                        transaction_number: c.transaction_number,
                        transaction_type: c.transaction_type,
                        grand_total: format!("{:.prec$}", c.grand_total, prec = decimals as usize),
                        created_at: format_date(&c.created_at),
                        error_message: c.error_message,
                        retry_count: c.retry_count,
                        operator_id: c.operator_id,
                        receipt_number: c.receipt_number.unwrap_or_default(),
                    }
                })
                .collect(),
            Err(e) => {
                tracing::error!("Failed to get conflicts: {}", e);
                Vec::new()
            }
        }
    }

    /// Gets the count of conflict transactions
    pub fn get_conflict_count(&self) -> i32 {
        self.service.get_conflict_count().unwrap_or(0) as i32
    }

    /// Checks if there are any conflicts
    pub fn has_conflicts(&self) -> bool {
        self.service.has_conflicts().unwrap_or(false)
    }

    /// Retries a conflict transaction
    ///
    /// # Arguments
    /// * `offline_id` - The offline transaction ID
    ///
    /// # Returns
    /// true if successful, false otherwise
    pub fn retry(&self, offline_id: &str) -> bool {
        match self.service.retry(offline_id) {
            Ok(result) => {
                tracing::info!("Conflict retry result: {}", result.message);
                result.success
            }
            Err(e) => {
                tracing::error!("Failed to retry conflict {}: {}", offline_id, e);
                false
            }
        }
    }

    /// Discards a conflict transaction
    ///
    /// # Arguments
    /// * `offline_id` - The offline transaction ID
    ///
    /// # Returns
    /// true if successful, false otherwise
    pub fn discard(&self, offline_id: &str) -> bool {
        match self.service.discard(offline_id) {
            Ok(result) => {
                tracing::info!("Conflict discard result: {}", result.message);
                result.success
            }
            Err(e) => {
                tracing::error!("Failed to discard conflict {}: {}", offline_id, e);
                false
            }
        }
    }

    /// Retries all conflict transactions
    ///
    /// # Returns
    /// Number of transactions reset for retry
    pub fn retry_all(&self) -> i32 {
        match self.service.retry_all() {
            Ok(count) => {
                tracing::info!("Retrying {} conflicts", count);
                count as i32
            }
            Err(e) => {
                tracing::error!("Failed to retry all conflicts: {}", e);
                0
            }
        }
    }

    /// Discards all conflict transactions
    ///
    /// # Returns
    /// Number of transactions discarded
    pub fn discard_all(&self) -> i32 {
        match self.service.discard_all() {
            Ok(count) => {
                tracing::info!("Discarded {} conflicts", count);
                count as i32
            }
            Err(e) => {
                tracing::error!("Failed to discard all conflicts: {}", e);
                0
            }
        }
    }

    /// Gets a reference to the underlying conflict service
    pub fn service(&self) -> &Arc<ConflictService> {
        &self.service
    }
}

impl Clone for ConflictBridge {
    fn clone(&self) -> Self {
        Self {
            service: Arc::clone(&self.service),
        }
    }
}

/// Formats an ISO date string for display
///
/// # Arguments
/// * `iso_date` - ISO 8601 date string
///
/// # Returns
/// Formatted date string (e.g., "2025-01-15 10:30")
fn format_date(iso_date: &str) -> String {
    // Try to parse and reformat the date
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(iso_date) {
        dt.format("%Y-%m-%d %H:%M").to_string()
    } else {
        // Return as-is if parsing fails
        iso_date.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_date() {
        assert_eq!(format_date("2025-01-15T10:30:00+00:00"), "2025-01-15 10:30");
        assert_eq!(format_date("invalid"), "invalid");
    }

    #[test]
    fn test_conflict_model_default() {
        let model = ConflictModel::default();
        assert!(model.offline_id.is_empty());
        assert!(model.transaction_number.is_empty());
        assert!(model.grand_total.is_empty());
        assert_eq!(model.retry_count, 0);
    }
}
