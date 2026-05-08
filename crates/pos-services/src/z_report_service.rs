//! Z-Report Service
//!
//! Handles end-of-day (Z-Report) generation and day closing operations.
//! Aggregates all shift data for the day and resets counters.

use anyhow::Result;
use parking_lot::RwLock;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{info, warn};

use pos_api::ApiClient;
use pos_db::Database;
use pos_models::{ShiftSummary, VarianceStatus, ZReport};

/// Error types for Z-Report operations
#[derive(Debug, Clone)]
pub enum ZReportError {
    /// No shifts to close
    NoShiftsToClose,
    /// Open shifts exist
    OpenShiftsExist(i32),
    /// Pending transactions to sync
    PendingSyncRequired(i32),
    /// Already closed today
    AlreadyClosedToday,
    /// Invalid state
    InvalidState(String),
    /// Database error
    DatabaseError(String),
    /// Network error (offline)
    NetworkError(String),
    /// API error from server
    ApiError(String),
}

impl std::fmt::Display for ZReportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ZReportError::NoShiftsToClose => write!(f, "No shifts found for today"),
            ZReportError::OpenShiftsExist(count) => {
                write!(f, "{} shift(s) are still open. Close all shifts before generating Z-Report.", count)
            }
            ZReportError::PendingSyncRequired(count) => {
                write!(f, "{} transactions pending sync. Sync all before closing day.", count)
            }
            ZReportError::AlreadyClosedToday => write!(f, "Z-Report already generated for today"),
            ZReportError::InvalidState(s) => write!(f, "Invalid state: {}", s),
            ZReportError::DatabaseError(s) => write!(f, "Database error: {}", s),
            ZReportError::NetworkError(s) => write!(f, "Network error: {}", s),
            ZReportError::ApiError(s) => write!(f, "API error: {}", s),
        }
    }
}

impl std::error::Error for ZReportError {}

/// Result type for Z-Report operations
pub type ZReportResult<T> = std::result::Result<T, ZReportError>;

// Note: ZReport struct is now in pos_models crate

/// Result of closing the day
#[derive(Debug, Clone)]
pub struct CloseDayResult {
    /// The generated Z-Report
    pub report: ZReport,
    /// Whether synced to server
    pub synced: bool,
    /// Any warnings
    pub warnings: Vec<String>,
}

/// API DTOs for Z-Report sync
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ZReportRequest {
    report_number: String,
    report_date: String,
    terminal_id: String,
    currency: String,
    total_shifts: u32,
    total_transactions: u32,
    gross_sales: Decimal,
    discounts: Decimal,
    returns: Decimal,
    net_sales: Decimal,
    tax_collected: Decimal,
    cash_total: Decimal,
    card_total: Decimal,
    wallet_total: Decimal,
    credit_total: Decimal,
    opening_float: Decimal,
    expected_cash: Decimal,
    actual_cash: Decimal,
    variance: Decimal,
    generated_at: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ZReportResponse {
    id: String,
}

/// Z-Report service for end-of-day operations
pub struct ZReportService {
    api: Arc<ApiClient>,
    db: Arc<Database>,
    current_report: RwLock<Option<ZReport>>,
}

impl ZReportService {
    /// Creates a new Z-Report service
    pub fn new(api: Arc<ApiClient>, db: Arc<Database>) -> Self {
        Self {
            api,
            db,
            current_report: RwLock::new(None),
        }
    }

    /// Checks if Z-Report was already generated today
    pub fn is_closed_today(&self, terminal_id: &str) -> ZReportResult<bool> {
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();

        // Check database for today's Z-Report
        let count = self.db.count_z_reports_for_date(terminal_id, &today)
            .map_err(|e| ZReportError::DatabaseError(e.to_string()))?;

        Ok(count > 0)
    }

    /// Checks if day can be closed
    pub fn can_close_day(&self, terminal_id: &str) -> ZReportResult<(bool, Vec<String>)> {
        let mut warnings = Vec::new();
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();

        // Check for already closed
        if self.is_closed_today(terminal_id)? {
            return Err(ZReportError::AlreadyClosedToday);
        }

        // Check for open shifts
        let open_shifts = self.db.count_open_shifts(terminal_id)
            .map_err(|e| ZReportError::DatabaseError(e.to_string()))?;

        if open_shifts > 0 {
            return Err(ZReportError::OpenShiftsExist(open_shifts as i32));
        }

        // Check for pending sync transactions
        let pending = self.db.count_pending_sync()
            .map_err(|e| ZReportError::DatabaseError(e.to_string()))?;

        if pending > 0 {
            warnings.push(format!("{} transactions pending sync", pending));
        }

        // Check for shifts today
        let shifts_today = self.db.count_shifts_for_date(terminal_id, &today)
            .map_err(|e| ZReportError::DatabaseError(e.to_string()))?;

        if shifts_today == 0 {
            return Err(ZReportError::NoShiftsToClose);
        }

        Ok((true, warnings))
    }

    /// Generates a Z-Report (preview, does not close)
    pub fn generate(&self, terminal_id: &str) -> ZReportResult<ZReport> {
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        let start_of_day = format!("{} 00:00:00", today);
        let end_of_day = format!("{} 23:59:59", today);

        // Get all closed shifts for today
        let shifts = self.db.get_shifts_in_range(&start_of_day, &end_of_day)
            .map_err(|e| ZReportError::DatabaseError(e.to_string()))?;

        if shifts.is_empty() {
            return Err(ZReportError::NoShiftsToClose);
        }

        // Convert to ShiftSummary
        let shift_summaries: Vec<ShiftSummary> = shifts.iter().map(|row| {
            ShiftSummary {
                id: row.id.clone(),
                shift_number: row.shift_number.clone(),
                operator_id: row.operator_id.clone(),
                operator_name: String::new(), // Would need lookup
                terminal_id: row.terminal_id.clone().unwrap_or_default(),
                opening_cash: row.opening_cash,
                expected_cash: row.expected_cash.unwrap_or(row.opening_cash),
                counted_cash: row.closing_cash,
                variance: row.variance,
                variance_status: row.variance.map(VarianceStatus::from_variance),
                started_at: row.started_at.clone(),
                ended_at: row.ended_at.clone(),
                status: pos_models::ShiftStatus::from_str(&row.status),
                transaction_count: 0,
                cash_sales: Decimal::ZERO,
                card_sales: Decimal::ZERO,
                wallet_sales: Decimal::ZERO,
                returns_total: Decimal::ZERO,
                discounts_total: Decimal::ZERO,
                gross_sales: Decimal::ZERO,
                net_sales: Decimal::ZERO,
                currency: "LYD".to_string(),
                note: row.notes.clone(),
            }
        }).collect();

        // Get transaction totals for today
        let day_totals = self.db.get_day_totals(terminal_id, &today)
            .map_err(|e| ZReportError::DatabaseError(e.to_string()))?;

        // Aggregate shift data
        let total_shifts = shift_summaries.len() as u32;
        let opening_float = shift_summaries.first()
            .map(|s| s.opening_cash)
            .unwrap_or(Decimal::ZERO);

        let total_expected: Decimal = shift_summaries.iter()
            .map(|s| s.expected_cash)
            .sum();
        let total_counted: Decimal = shift_summaries.iter()
            .filter_map(|s| s.counted_cash)
            .sum();

        // Calculate aggregated variance
        let variance = total_counted - total_expected;
        let variance_status = VarianceStatus::from_variance(variance);

        // Generate report number
        let report_number = self.generate_report_number(terminal_id)?;
        let now = chrono::Utc::now();

        let report = ZReport {
            report_number,
            report_date: today.clone(),
            terminal_id: terminal_id.to_string(),
            currency: "LYD".to_string(),
            total_shifts,
            total_transactions: day_totals.transaction_count as u32,
            gross_sales: day_totals.gross_sales,
            discounts: day_totals.discounts,
            returns: day_totals.returns,
            net_sales: day_totals.net_sales,
            tax_collected: day_totals.tax_collected,
            tax_rate: "15%".to_string(),
            cash_total: day_totals.cash_total,
            card_total: day_totals.card_total,
            wallet_total: day_totals.wallet_total,
            credit_total: day_totals.credit_total,
            cash_count: day_totals.cash_count as u32,
            card_count: day_totals.card_count as u32,
            wallet_count: day_totals.wallet_count as u32,
            credit_count: day_totals.credit_count as u32,
            opening_float,
            total_cash_in: day_totals.cash_total,
            total_cash_out: day_totals.cash_refunds,
            expected_cash: total_expected,
            actual_cash: total_counted,
            variance,
            variance_status,
            return_count: day_totals.return_count as u32,
            return_total: day_totals.returns,
            void_count: day_totals.void_count as u32,
            void_total: day_totals.void_total,
            shifts: shift_summaries,
            generated_at: now.to_rfc3339(),
            synced: false,
            server_id: None,
        };

        *self.current_report.write() = Some(report.clone());
        Ok(report)
    }

    /// Closes the day (finalizes Z-Report)
    pub async fn close_day(&self, terminal_id: &str) -> ZReportResult<CloseDayResult> {
        // Validate can close
        let (_, warnings) = self.can_close_day(terminal_id)?;

        // Generate final report if not already generated
        let mut report = match self.current_report.read().clone() {
            Some(r) if r.terminal_id == terminal_id => r,
            _ => self.generate(terminal_id)?,
        };

        // Save to database
        self.db.save_z_report(&report)
            .map_err(|e| ZReportError::DatabaseError(e.to_string()))?;

        // Mark day as closed in database
        self.db.mark_day_closed(terminal_id, &report.report_date)
            .map_err(|e| ZReportError::DatabaseError(e.to_string()))?;

        // Try to sync to server
        let synced = if self.api.is_online().await.is_online() {
            match self.sync_z_report(&report).await {
                Ok(server_id) => {
                    report.server_id = Some(server_id.clone());
                    report.synced = true;
                    let _ = self.db.mark_z_report_synced(&report.report_number, &server_id);
                    info!("Z-Report {} synced to server", report.report_number);
                    true
                }
                Err(e) => {
                    warn!("Failed to sync Z-Report to server: {}", e);
                    false
                }
            }
        } else {
            info!("Offline - Z-Report {} queued for sync", report.report_number);
            false
        };

        // Clear current report
        *self.current_report.write() = None;

        Ok(CloseDayResult {
            report,
            synced,
            warnings,
        })
    }

    /// Gets the current report preview
    pub fn current_report(&self) -> Option<ZReport> {
        self.current_report.read().clone()
    }

    /// Gets historical Z-Reports
    pub fn get_reports_in_range(&self, start_date: &str, end_date: &str) -> ZReportResult<Vec<ZReport>> {
        self.db.get_z_reports_in_range(start_date, end_date)
            .map_err(|e| ZReportError::DatabaseError(e.to_string()))
    }

    // ========================================================================
    // Private helpers
    // ========================================================================

    fn generate_report_number(&self, terminal_id: &str) -> ZReportResult<String> {
        let today = chrono::Local::now();
        let date_part = today.format("%Y%m%d").to_string();

        // Get sequence number for today
        let seq = self.db.get_next_z_report_sequence(terminal_id, &date_part)
            .map_err(|e| ZReportError::DatabaseError(e.to_string()))?;

        Ok(format!("Z-{}-{}-{:03}", terminal_id, date_part, seq))
    }

    async fn sync_z_report(&self, report: &ZReport) -> Result<String> {
        let request = ZReportRequest {
            report_number: report.report_number.clone(),
            report_date: report.report_date.clone(),
            terminal_id: report.terminal_id.clone(),
            currency: report.currency.clone(),
            total_shifts: report.total_shifts,
            total_transactions: report.total_transactions,
            gross_sales: report.gross_sales,
            discounts: report.discounts,
            returns: report.returns,
            net_sales: report.net_sales,
            tax_collected: report.tax_collected,
            cash_total: report.cash_total,
            card_total: report.card_total,
            wallet_total: report.wallet_total,
            credit_total: report.credit_total,
            opening_float: report.opening_float,
            expected_cash: report.expected_cash,
            actual_cash: report.actual_cash,
            variance: report.variance,
            generated_at: report.generated_at.clone(),
        };

        let response: ZReportResponse = self
            .api
            .post("/api/pos/z-reports", &request)
            .await?;

        Ok(response.id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_z_report_default() {
        let report = ZReport::default();
        assert_eq!(report.total_shifts, 0);
        assert_eq!(report.currency, "LYD");
        assert_eq!(report.variance_status, VarianceStatus::Balanced);
        assert!(!report.synced);
    }

    #[test]
    fn test_z_report_format_amount() {
        assert_eq!(ZReport::format_amount_static(Decimal::new(12345, 1)), "1234.500");
        assert_eq!(ZReport::format_amount_static(Decimal::ZERO), "0.000");
        assert_eq!(ZReport::format_amount_static(Decimal::new(100123, 3)), "100.123");
    }

    #[test]
    fn test_z_report_can_close() {
        let mut report = ZReport::default();
        assert!(!report.can_close());

        report.total_shifts = 1;
        assert!(report.can_close());
    }

    #[test]
    fn test_z_report_variance_status_str() {
        let mut report = ZReport::default();

        report.variance_status = VarianceStatus::Balanced;
        assert_eq!(report.variance_status_str(), "balanced");

        report.variance_status = VarianceStatus::Short;
        assert_eq!(report.variance_status_str(), "short");

        report.variance_status = VarianceStatus::Over;
        assert_eq!(report.variance_status_str(), "over");
    }

    #[test]
    fn test_z_report_error_display() {
        assert_eq!(
            ZReportError::NoShiftsToClose.to_string(),
            "No shifts found for today"
        );
        assert_eq!(
            ZReportError::OpenShiftsExist(2).to_string(),
            "2 shift(s) are still open. Close all shifts before generating Z-Report."
        );
        assert_eq!(
            ZReportError::PendingSyncRequired(5).to_string(),
            "5 transactions pending sync. Sync all before closing day."
        );
        assert_eq!(
            ZReportError::AlreadyClosedToday.to_string(),
            "Z-Report already generated for today"
        );
    }

    #[test]
    fn test_close_day_result() {
        let result = CloseDayResult {
            report: ZReport::default(),
            synced: true,
            warnings: vec!["Test warning".to_string()],
        };

        assert!(result.synced);
        assert_eq!(result.warnings.len(), 1);
    }
}
