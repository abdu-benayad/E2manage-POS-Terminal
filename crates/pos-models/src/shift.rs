//! Shift Domain Models
//!
//! This module contains shift-related domain models.

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

/// Variance status for cash reconciliation
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

impl Default for VarianceStatus {
    fn default() -> Self {
        VarianceStatus::Balanced
    }
}

/// Shift status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ShiftStatus {
    /// Shift is currently active
    Active,
    /// Shift was closed normally
    Closed,
    /// Shift was suspended (interrupted)
    Suspended,
}

impl ShiftStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            ShiftStatus::Active => "active",
            ShiftStatus::Closed => "closed",
            ShiftStatus::Suspended => "suspended",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "active" => ShiftStatus::Active,
            "closed" => ShiftStatus::Closed,
            "suspended" => ShiftStatus::Suspended,
            _ => ShiftStatus::Active,
        }
    }
}

impl Default for ShiftStatus {
    fn default() -> Self {
        ShiftStatus::Active
    }
}

/// Shift summary for UI display and reports
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShiftSummary {
    /// Shift ID
    pub id: String,
    /// Shift number (e.g., "POS-001-20241213-001")
    pub shift_number: String,
    /// Operator ID
    pub operator_id: String,
    /// Operator name
    pub operator_name: String,
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

impl Default for ShiftSummary {
    fn default() -> Self {
        Self {
            id: String::new(),
            shift_number: String::new(),
            operator_id: String::new(),
            operator_name: String::new(),
            terminal_id: String::new(),
            opening_cash: Decimal::ZERO,
            expected_cash: Decimal::ZERO,
            counted_cash: None,
            variance: None,
            variance_status: None,
            started_at: String::new(),
            ended_at: None,
            status: ShiftStatus::Active,
            transaction_count: 0,
            cash_sales: Decimal::ZERO,
            card_sales: Decimal::ZERO,
            wallet_sales: Decimal::ZERO,
            returns_total: Decimal::ZERO,
            discounts_total: Decimal::ZERO,
            gross_sales: Decimal::ZERO,
            net_sales: Decimal::ZERO,
            currency: "LYD".to_string(),
            note: None,
        }
    }
}
