//! Shift Service
//!
//! Handles shift management operations including starting, ending, and cash reconciliation.
//! Supports offline operation with sync queue.

use anyhow::Result;
use parking_lot::RwLock;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{info, warn};

use pos_api::{ApiClient, DenominationDto, EndShiftRequest, StartShiftRequest};
use pos_db::shifts::{ShiftRow, ShiftStatus};
use pos_db::Database;
use pos_models::{OperatorId, RecordedOperatorName};

/// Error types for shift operations
#[derive(Debug, Clone)]
pub enum ShiftError {
    /// Shift not found
    NotFound,
    /// Shift already active
    AlreadyActive,
    /// No active shift
    NoActiveShift,
    /// Shift already closed
    AlreadyClosed,
    /// Invalid shift state
    InvalidState(String),
    /// Cash count required
    CashCountRequired,
    /// Variance note required
    VarianceNoteRequired,
    /// Database error
    DatabaseError(String),
    /// Network error (offline)
    NetworkError(String),
    /// API error from server
    ApiError(String),
}

impl std::fmt::Display for ShiftError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ShiftError::NotFound => write!(f, "Shift not found"),
            ShiftError::AlreadyActive => write!(f, "A shift is already active"),
            ShiftError::NoActiveShift => write!(f, "No active shift"),
            ShiftError::AlreadyClosed => write!(f, "Shift already closed"),
            ShiftError::InvalidState(s) => write!(f, "Invalid shift state: {}", s),
            ShiftError::CashCountRequired => write!(f, "Cash count is required to close shift"),
            ShiftError::VarianceNoteRequired => write!(f, "Note is required when variance exists"),
            ShiftError::DatabaseError(s) => write!(f, "Database error: {}", s),
            ShiftError::NetworkError(s) => write!(f, "Network error: {}", s),
            ShiftError::ApiError(s) => write!(f, "API error: {}", s),
        }
    }
}

impl std::error::Error for ShiftError {}

/// Result type for shift operations
pub type ShiftResult<T> = std::result::Result<T, ShiftError>;

/// Denomination for cash counting
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Denomination {
    /// Display label (e.g., "50", "0.500")
    pub label: String,
    /// Monetary value
    pub value: Decimal,
    /// Count of this denomination
    pub count: i32,
    /// Subtotal (value * count)
    pub subtotal: Decimal,
}

impl Denomination {
    /// Creates a new denomination
    pub fn new(label: &str, value: Decimal) -> Self {
        Self {
            label: label.to_string(),
            value,
            count: 0,
            subtotal: Decimal::ZERO,
        }
    }

    /// Updates the count and recalculates subtotal
    pub fn set_count(&mut self, count: i32) {
        self.count = count;
        self.subtotal = self.value * Decimal::from(count);
    }
}

/// Default denominations for Libyan Dinar (LYD)
pub fn default_denominations_lyd() -> Vec<Denomination> {
    vec![
        Denomination::new("50", Decimal::from(50)),
        Denomination::new("20", Decimal::from(20)),
        Denomination::new("10", Decimal::from(10)),
        Denomination::new("5", Decimal::from(5)),
        Denomination::new("1", Decimal::ONE),
        Denomination::new("0.500", Decimal::new(5, 1)),
        Denomination::new("0.250", Decimal::new(25, 2)),
        Denomination::new("0.100", Decimal::new(1, 1)),
    ]
}

/// Cash count result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CashCount {
    /// Denomination breakdown
    pub denominations: Vec<Denomination>,
    /// Total counted cash
    pub total: Decimal,
    /// Currency
    pub currency: String,
}

impl CashCount {
    /// Creates a new cash count with default denominations
    pub fn new(currency: &str) -> Self {
        let denominations = match currency {
            "LYD" => default_denominations_lyd(),
            _ => default_denominations_lyd(), // Default to LYD
        };
        Self {
            denominations,
            total: Decimal::ZERO,
            currency: currency.to_string(),
        }
    }

    /// Updates a denomination count and recalculates total
    pub fn update_denomination(&mut self, index: usize, count: i32) {
        if index < self.denominations.len() {
            self.denominations[index].set_count(count);
            self.recalculate_total();
        }
    }

    /// Recalculates the total from all denominations
    pub fn recalculate_total(&mut self) {
        self.total = self.denominations.iter().map(|d| d.subtotal).sum();
    }
}

/// Variance status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VarianceStatus {
    /// Cash matches expected
    Balanced,
    /// Cash is less than expected
    Short,
    /// Cash is more than expected
    Over,
}

impl VarianceStatus {
    /// Returns the status as a string
    pub fn as_str(&self) -> &'static str {
        match self {
            VarianceStatus::Balanced => "balanced",
            VarianceStatus::Short => "short",
            VarianceStatus::Over => "over",
        }
    }

    /// Determines status from variance amount (exact zero comparison)
    pub fn from_variance(variance: Decimal) -> Self {
        if variance.is_zero() {
            VarianceStatus::Balanced
        } else if variance.is_sign_negative() {
            VarianceStatus::Short
        } else {
            VarianceStatus::Over
        }
    }
}

/// Shift summary for UI display
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShiftSummary {
    /// Shift ID
    pub id: String,
    /// Shift number (e.g., "POS-001-20241213-001")
    pub shift_number: String,
    /// Operator who owns the shift
    pub operator_id: OperatorId,
    /// The operator's name as recorded on this shift, when it is known.
    ///
    /// Optional because the `shifts` table stores only `operator_id`; `row_to_summary` cannot
    /// name the operator without a lookup that does not exist yet. `None` says the lookup did
    /// not happen — the empty string it replaces said the operator had no name.
    pub operator_name: Option<RecordedOperatorName>,
    /// Terminal ID
    pub terminal_id: String,
    /// Opening cash float
    pub opening_cash: Decimal,
    /// Expected cash (opening + cash sales - returns)
    pub expected_cash: Decimal,
    /// Counted cash (if closed)
    pub counted_cash: Option<Decimal>,
    /// Variance (counted - expected)
    pub variance: Option<Decimal>,
    /// Variance status
    pub variance_status: Option<VarianceStatus>,
    /// Start time
    pub started_at: String,
    /// End time (if closed)
    pub ended_at: Option<String>,
    /// Status
    pub status: ShiftStatus,
    /// Transaction count
    pub transaction_count: i32,
    /// Total cash sales
    pub cash_sales: Decimal,
    /// Total card sales
    pub card_sales: Decimal,
    /// Total wallet/mobile sales
    pub wallet_sales: Decimal,
    /// Total returns
    pub returns_total: Decimal,
    /// Total discounts
    pub discounts_total: Decimal,
    /// Gross sales
    pub gross_sales: Decimal,
    /// Net sales (gross - discounts - returns)
    pub net_sales: Decimal,
    /// Currency
    pub currency: String,
    /// Note (variance explanation)
    pub note: Option<String>,
}

// There is deliberately no `impl Default for ShiftSummary`.
//
// The one it replaces gave a summary an empty `operator_id`, an empty `operator_name` and
// `status: ShiftStatus::Active`, so `ShiftSummary::default()` was an *open shift belonging to
// nobody* and every `..Default::default()` inherited it. Typing the id made the problem
// unignorable rather than solving it: an `OperatorId::new("unassigned")` would be the same
// sentinel wearing the newtype, which is the one outcome this migration must not produce.
// `shifts.operator_id` is `NOT NULL`; there is no such shift to model.

/// Result of starting a shift
#[derive(Debug, Clone)]
pub struct StartShiftResult {
    /// The new shift ID
    pub shift_id: String,
    /// The shift number
    pub shift_number: String,
    /// Whether synced to server
    pub synced: bool,
}

/// Result of closing a shift
#[derive(Debug, Clone)]
pub struct CloseShiftResult {
    /// The shift ID
    pub shift_id: String,
    /// The shift number
    pub shift_number: String,
    /// Final variance
    pub variance: Decimal,
    /// Variance status
    pub variance_status: VarianceStatus,
    /// Whether synced to server
    pub synced: bool,
    /// X-Report data for printing
    pub report: ShiftSummary,
}

/// Shift service for managing POS shifts
pub struct ShiftService {
    api: Arc<ApiClient>,
    db: Arc<Database>,
    current_shift: RwLock<Option<ShiftSummary>>,
    cash_count: RwLock<Option<CashCount>>,
}

impl ShiftService {
    /// Creates a new shift service
    pub fn new(api: Arc<ApiClient>, db: Arc<Database>) -> Self {
        Self {
            api,
            db,
            current_shift: RwLock::new(None),
            cash_count: RwLock::new(None),
        }
    }

    /// Checks if there's an active shift
    pub fn has_active_shift(&self) -> bool {
        self.current_shift.read().is_some()
    }

    /// Gets the current active shift
    pub fn current_shift(&self) -> Option<ShiftSummary> {
        self.current_shift.read().clone()
    }

    /// Gets the current cash count
    pub fn current_cash_count(&self) -> Option<CashCount> {
        self.cash_count.read().clone()
    }

    /// Loads any existing active shift from database
    pub fn load_active_shift(&self) -> ShiftResult<Option<ShiftSummary>> {
        let shift_row = self
            .db
            .get_current_active_shift()
            .map_err(|e| ShiftError::DatabaseError(e.to_string()))?;

        if let Some(row) = shift_row {
            let summary = self.row_to_summary(&row)?;
            *self.current_shift.write() = Some(summary.clone());
            Ok(Some(summary))
        } else {
            Ok(None)
        }
    }

    /// Starts a new shift
    pub async fn start_shift(
        &self,
        operator_id: &OperatorId,
        operator_name: &RecordedOperatorName,
        terminal_id: &str,
        opening_cash: Decimal,
        currency: &str,
    ) -> ShiftResult<StartShiftResult> {
        // Check for existing active shift
        if let Some(existing) = self
            .db
            .get_current_active_shift()
            .map_err(|e| ShiftError::DatabaseError(e.to_string()))?
        {
            if existing.status == "ACTIVE" {
                return Err(ShiftError::AlreadyActive);
            }
        }

        // Generate shift number
        let shift_number = self
            .db
            .generate_shift_number(terminal_id)
            .map_err(|e| ShiftError::DatabaseError(e.to_string()))?;

        // Generate shift ID
        let shift_id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now();

        // Save to local database
        let shift_row = self
            .db
            .start_shift(
                &shift_id,
                &shift_number,
                operator_id,
                Some(terminal_id),
                opening_cash,
            )
            .map_err(|e| ShiftError::DatabaseError(e.to_string()))?;

        // Create summary
        let summary = ShiftSummary {
            id: shift_id.clone(),
            shift_number: shift_number.clone(),
            operator_id: operator_id.clone(),
            operator_name: Some(operator_name.clone()),
            terminal_id: terminal_id.to_string(),
            opening_cash,
            expected_cash: opening_cash, // Initially just opening float
            started_at: shift_row.started_at,
            status: ShiftStatus::Active,
            currency: currency.to_string(),
            counted_cash: None,
            variance: None,
            variance_status: None,
            ended_at: None,
            transaction_count: 0,
            cash_sales: Decimal::ZERO,
            card_sales: Decimal::ZERO,
            wallet_sales: Decimal::ZERO,
            returns_total: Decimal::ZERO,
            discounts_total: Decimal::ZERO,
            gross_sales: Decimal::ZERO,
            net_sales: Decimal::ZERO,
            note: None,
        };

        *self.current_shift.write() = Some(summary);

        // Try to sync to server
        let synced = if self.api.is_online().await.is_online() {
            match self
                .sync_shift_start(
                    &shift_id,
                    &shift_number,
                    operator_id,
                    terminal_id,
                    opening_cash,
                    currency,
                    &now.to_rfc3339(),
                )
                .await
            {
                Ok(server_id) => {
                    // Update local record with server ID
                    let _ = self.db.mark_shift_synced(&shift_id, &server_id);
                    info!("Shift {} synced to server", shift_number);
                    true
                }
                Err(e) => {
                    warn!("Failed to sync shift to server: {}", e);
                    false
                }
            }
        } else {
            info!("Offline - shift {} queued for sync", shift_number);
            false
        };

        Ok(StartShiftResult {
            shift_id,
            shift_number,
            synced,
        })
    }

    /// Updates the expected cash based on transactions
    pub fn update_expected_cash(&self, cash_sales: Decimal, returns: Decimal) {
        let mut shift = self.current_shift.write();
        if let Some(ref mut s) = *shift {
            s.cash_sales = cash_sales;
            s.returns_total = returns;
            s.expected_cash = s.opening_cash + cash_sales - returns;
        }
    }

    /// Starts cash counting for shift end
    pub fn start_cash_count(&self, currency: &str) -> CashCount {
        let cash_count = CashCount::new(currency);
        *self.cash_count.write() = Some(cash_count.clone());
        cash_count
    }

    /// Updates a denomination count
    pub fn update_denomination(&self, index: usize, count: i32) -> ShiftResult<Decimal> {
        let mut cash_count = self.cash_count.write();
        if let Some(ref mut cc) = *cash_count {
            cc.update_denomination(index, count);
            Ok(cc.total)
        } else {
            Err(ShiftError::InvalidState(
                "Cash count not started".to_string(),
            ))
        }
    }

    /// Calculates variance between counted and expected cash
    pub fn calculate_variance(&self) -> ShiftResult<(Decimal, VarianceStatus)> {
        let shift = self.current_shift.read();
        let cash_count = self.cash_count.read();

        let expected = shift
            .as_ref()
            .map(|s| s.expected_cash)
            .unwrap_or(Decimal::ZERO);
        let counted = cash_count
            .as_ref()
            .map(|c| c.total)
            .unwrap_or(Decimal::ZERO);

        let variance = counted - expected;
        let status = VarianceStatus::from_variance(variance);

        Ok((variance, status))
    }

    /// Closes the current shift
    pub async fn close_shift(
        &self,
        counted_cash: Decimal,
        note: Option<&str>,
    ) -> ShiftResult<CloseShiftResult> {
        let shift = self.current_shift.read().clone();
        let shift = shift.ok_or(ShiftError::NoActiveShift)?;

        let variance = counted_cash - shift.expected_cash;
        let variance_status = VarianceStatus::from_variance(variance);

        // Require note if variance exists
        if variance_status != VarianceStatus::Balanced && note.is_none() {
            return Err(ShiftError::VarianceNoteRequired);
        }

        // Update local database
        self.db
            .end_shift(&shift.id, counted_cash, shift.expected_cash, note)
            .map_err(|e| ShiftError::DatabaseError(e.to_string()))?;

        // Create final summary
        let mut final_summary = shift.clone();
        final_summary.counted_cash = Some(counted_cash);
        final_summary.variance = Some(variance);
        final_summary.variance_status = Some(variance_status);
        final_summary.ended_at = Some(chrono::Utc::now().to_rfc3339());
        final_summary.status = ShiftStatus::Closed;
        final_summary.note = note.map(String::from);

        // Try to sync to server
        let synced = if self.api.is_online().await.is_online() {
            match self
                .sync_shift_end(&shift.id, counted_cash, shift.expected_cash, variance, note)
                .await
            {
                Ok(_) => {
                    info!("Shift {} end synced to server", shift.shift_number);
                    true
                }
                Err(e) => {
                    warn!("Failed to sync shift end to server: {}", e);
                    false
                }
            }
        } else {
            info!("Offline - shift end queued for sync");
            false
        };

        // Clear current shift and cash count
        *self.current_shift.write() = None;
        *self.cash_count.write() = None;

        Ok(CloseShiftResult {
            shift_id: shift.id,
            shift_number: shift.shift_number,
            variance,
            variance_status,
            synced,
            report: final_summary,
        })
    }

    /// Gets shift summary for reports (X-Report data)
    pub fn get_shift_summary(&self) -> ShiftResult<ShiftSummary> {
        self.current_shift
            .read()
            .clone()
            .ok_or(ShiftError::NoActiveShift)
    }

    /// Gets historical shifts for a date range
    pub fn get_shifts_in_range(
        &self,
        start_date: &str,
        end_date: &str,
    ) -> ShiftResult<Vec<ShiftSummary>> {
        let rows = self
            .db
            .get_shifts_in_range(start_date, end_date)
            .map_err(|e| ShiftError::DatabaseError(e.to_string()))?;

        rows.into_iter()
            .map(|row| self.row_to_summary(&row))
            .collect()
    }

    // ========================================================================
    // Private helpers
    // ========================================================================

    fn row_to_summary(&self, row: &ShiftRow) -> ShiftResult<ShiftSummary> {
        let status = row
            .status
            .parse::<ShiftStatus>()
            .map_err(|e| ShiftError::InvalidState(e.to_string()))?;

        let variance_status = row.variance.map(VarianceStatus::from_variance);

        // TODO: Get actual transaction totals from database
        Ok(ShiftSummary {
            id: row.id.clone(),
            shift_number: row.shift_number.clone(),
            operator_id: row.operator_id.clone(),
            // The `shifts` table keeps no name column, so this summary genuinely does not know
            // it. `None` reports that; the `String::new()` it replaces claimed the operator was
            // called "". The lookup this needs is `operators.name` by id, and adding it is a
            // behavioural change that belongs with whoever owns the shift screen.
            operator_name: None,
            terminal_id: row.terminal_id.clone().unwrap_or_default(),
            opening_cash: row.opening_cash,
            expected_cash: row.expected_cash.unwrap_or(row.opening_cash),
            counted_cash: row.closing_cash,
            variance: row.variance,
            variance_status,
            started_at: row.started_at.clone(),
            ended_at: row.ended_at.clone(),
            status,
            transaction_count: 0, // Would need to query
            cash_sales: Decimal::ZERO,
            card_sales: Decimal::ZERO,
            wallet_sales: Decimal::ZERO,
            returns_total: Decimal::ZERO,
            discounts_total: Decimal::ZERO,
            gross_sales: Decimal::ZERO,
            net_sales: Decimal::ZERO,
            currency: "LYD".to_string(),
            note: row.notes.clone(),
        })
    }

    // Task 09 typed `operator_id`, so it is no longer swappable with its neighbours.
    // `_shift_id`, `shift_number`, `terminal_id`, `currency` and `started_at` still are, and
    // belong to later tiers of `type-driven-domain-core`. The arity is unchanged.
    #[expect(
        clippy::too_many_arguments,
        reason = "eight parameters; `operator_id` is typed, the rest belong to later tiers of \
                  type-driven-domain-core"
    )]
    async fn sync_shift_start(
        &self,
        _shift_id: &str,
        shift_number: &str,
        operator_id: &OperatorId,
        terminal_id: &str,
        opening_cash: Decimal,
        currency: &str,
        started_at: &str,
    ) -> Result<String> {
        let request = StartShiftRequest {
            shift_number: shift_number.to_string(),
            operator_id: operator_id.clone(),
            terminal_id: terminal_id.to_string(),
            opening_cash,
            currency: currency.to_string(),
            started_at: started_at.to_string(),
        };

        let response = self.api.start_shift(&request).await?;

        Ok(response.id)
    }

    async fn sync_shift_end(
        &self,
        shift_id: &str,
        closing_cash: Decimal,
        expected_cash: Decimal,
        variance: Decimal,
        note: Option<&str>,
    ) -> Result<()> {
        // Scoped so the `parking_lot` read guard is released before the `.await` below.
        // Holding it across the await can deadlock: the future may be resumed on another
        // thread while a writer is waiting, and `parking_lot` guards are not send-safe here.
        let denomination_breakdown: Option<Vec<DenominationDto>> = {
            let cash_count = self.cash_count.read();
            cash_count.as_ref().map(|cc| {
                cc.denominations
                    .iter()
                    .filter(|d| d.count > 0)
                    .map(|d| DenominationDto {
                        label: d.label.clone(),
                        value: d.value,
                        count: d.count,
                        subtotal: d.subtotal,
                    })
                    .collect()
            })
        };

        let request = EndShiftRequest {
            closing_cash,
            expected_cash,
            variance,
            note: note.map(String::from),
            ended_at: chrono::Utc::now().to_rfc3339(),
            denomination_breakdown,
        };

        self.api.end_shift(shift_id, &request).await?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_denomination_new() {
        let denom = Denomination::new("50", Decimal::from(50));
        assert_eq!(denom.label, "50");
        assert_eq!(denom.value, Decimal::from(50));
        assert_eq!(denom.count, 0);
        assert_eq!(denom.subtotal, Decimal::ZERO);
    }

    #[test]
    fn test_denomination_set_count() {
        let mut denom = Denomination::new("20", Decimal::from(20));
        denom.set_count(5);
        assert_eq!(denom.count, 5);
        assert_eq!(denom.subtotal, Decimal::from(100));
    }

    #[test]
    fn test_default_denominations_lyd() {
        let denoms = default_denominations_lyd();
        assert_eq!(denoms.len(), 8);
        assert_eq!(denoms[0].value, Decimal::from(50));
        assert_eq!(denoms[denoms.len() - 1].value, Decimal::new(1, 1));
    }

    #[test]
    fn test_cash_count_new() {
        let cc = CashCount::new("LYD");
        assert_eq!(cc.denominations.len(), 8);
        assert_eq!(cc.total, Decimal::ZERO);
        assert_eq!(cc.currency, "LYD");
    }

    #[test]
    fn test_cash_count_update_denomination() {
        let mut cc = CashCount::new("LYD");
        cc.update_denomination(0, 10); // 10 x 50 = 500
        cc.update_denomination(1, 5); // 5 x 20 = 100
        assert_eq!(cc.total, Decimal::from(600));
    }

    #[test]
    fn test_variance_status_balanced() {
        assert_eq!(
            VarianceStatus::from_variance(Decimal::ZERO),
            VarianceStatus::Balanced
        );
    }

    #[test]
    fn test_variance_status_short() {
        assert_eq!(
            VarianceStatus::from_variance(Decimal::from(-10)),
            VarianceStatus::Short
        );
        assert_eq!(
            VarianceStatus::from_variance(Decimal::new(-2, 2)),
            VarianceStatus::Short
        );
    }

    #[test]
    fn test_variance_status_over() {
        assert_eq!(
            VarianceStatus::from_variance(Decimal::from(10)),
            VarianceStatus::Over
        );
        assert_eq!(
            VarianceStatus::from_variance(Decimal::new(2, 2)),
            VarianceStatus::Over
        );
    }

    #[test]
    fn test_variance_status_as_str() {
        assert_eq!(VarianceStatus::Balanced.as_str(), "balanced");
        assert_eq!(VarianceStatus::Short.as_str(), "short");
        assert_eq!(VarianceStatus::Over.as_str(), "over");
    }
}
