//! Z-Report Domain Model
//!
//! Z-Report (End of Day) report model.

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::shift::{ShiftSummary, VarianceStatus};

/// Z-Report (End of Day Report)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZReport {
    /// Report number (e.g., "Z-2024-015")
    pub report_number: String,
    /// Report date (YYYY-MM-DD)
    pub report_date: String,
    /// Terminal ID
    pub terminal_id: String,
    /// Currency
    pub currency: String,

    // Shift aggregates
    /// Total number of shifts closed today
    pub total_shifts: u32,
    /// Total number of transactions
    pub total_transactions: u32,

    // Sales totals
    /// Gross sales before discounts and returns
    pub gross_sales: Decimal,
    /// Total discounts applied
    pub discounts: Decimal,
    /// Total returns processed
    pub returns: Decimal,
    /// Net sales (gross - discounts - returns)
    pub net_sales: Decimal,
    /// Total tax collected
    pub tax_collected: Decimal,
    /// Tax rate (for display)
    pub tax_rate: String,

    // Payment method totals
    /// Cash payments total
    pub cash_total: Decimal,
    /// Card payments total
    pub card_total: Decimal,
    /// Wallet/mobile payments total
    pub wallet_total: Decimal,
    /// Credit sales total
    pub credit_total: Decimal,
    /// Cash payment count
    pub cash_count: u32,
    /// Card payment count
    pub card_count: u32,
    /// Wallet payment count
    pub wallet_count: u32,
    /// Credit payment count
    pub credit_count: u32,

    // Cash accountability
    /// Opening float (first shift)
    pub opening_float: Decimal,
    /// Total cash received
    pub total_cash_in: Decimal,
    /// Total cash paid out (refunds, etc.)
    pub total_cash_out: Decimal,
    /// Expected cash in drawer
    pub expected_cash: Decimal,
    /// Actual counted cash
    pub actual_cash: Decimal,
    /// Variance (actual - expected)
    pub variance: Decimal,
    /// Variance status
    pub variance_status: VarianceStatus,

    // Returns & Voids
    /// Number of return transactions
    pub return_count: u32,
    /// Total amount returned
    pub return_total: Decimal,
    /// Number of voided transactions
    pub void_count: u32,
    /// Total amount voided
    pub void_total: Decimal,

    // Shift details (for breakdown)
    pub shifts: Vec<ShiftSummary>,

    // Metadata
    /// Report generation timestamp
    pub generated_at: String,
    /// Whether synced to server
    pub synced: bool,
    /// Server-assigned ID (if synced)
    pub server_id: Option<String>,
}

impl Default for ZReport {
    fn default() -> Self {
        Self {
            report_number: String::new(),
            report_date: String::new(),
            terminal_id: String::new(),
            currency: "LYD".to_string(),
            total_shifts: 0,
            total_transactions: 0,
            gross_sales: Decimal::ZERO,
            discounts: Decimal::ZERO,
            returns: Decimal::ZERO,
            net_sales: Decimal::ZERO,
            tax_collected: Decimal::ZERO,
            tax_rate: "0%".to_string(),
            cash_total: Decimal::ZERO,
            card_total: Decimal::ZERO,
            wallet_total: Decimal::ZERO,
            credit_total: Decimal::ZERO,
            cash_count: 0,
            card_count: 0,
            wallet_count: 0,
            credit_count: 0,
            opening_float: Decimal::ZERO,
            total_cash_in: Decimal::ZERO,
            total_cash_out: Decimal::ZERO,
            expected_cash: Decimal::ZERO,
            actual_cash: Decimal::ZERO,
            variance: Decimal::ZERO,
            variance_status: VarianceStatus::Balanced,
            return_count: 0,
            return_total: Decimal::ZERO,
            void_count: 0,
            void_total: Decimal::ZERO,
            shifts: Vec::new(),
            generated_at: String::new(),
            synced: false,
            server_id: None,
        }
    }
}

impl ZReport {
    /// Formats a currency amount for display (static version, always 3 decimal places)
    pub fn format_amount_static(amount: Decimal) -> String {
        let rounded = amount.round_dp(3);
        let s = rounded.to_string();
        if let Some(dot_pos) = s.find('.') {
            let decimals = s.len() - dot_pos - 1;
            if decimals < 3 {
                format!("{}{}", s, "0".repeat(3 - decimals))
            } else {
                s
            }
        } else {
            format!("{}.000", s)
        }
    }

    /// Format amount for display with currency (always 3 decimal places)
    pub fn format_amount(&self, amount: Decimal) -> String {
        format!("{} {}", Self::format_amount_static(amount), self.currency)
    }

    /// Check if day can be closed (all shifts must be closed)
    pub fn can_close(&self) -> bool {
        self.total_shifts > 0
    }

    /// Get variance status as string
    pub fn variance_status_str(&self) -> &'static str {
        self.variance_status.as_str()
    }
}
