//! Shift Domain Models
//!
//! This module contains shift-related domain models.

use std::str::FromStr;

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::operator::{OperatorId, RecordedOperatorName};
use crate::parse::ParseError;

/// Variance status for cash reconciliation
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum VarianceStatus {
    /// Cash matches expected
    #[default]
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

/// Shift status
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ShiftStatus {
    /// Shift is currently active
    #[default]
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
}

impl FromStr for ShiftStatus {
    type Err = ParseError;

    /// Case-insensitive, because `pos-models` writes these lowercase and `pos-db` writes them
    /// uppercase into the same column.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "active" => Ok(ShiftStatus::Active),
            "closed" => Ok(ShiftStatus::Closed),
            "suspended" => Ok(ShiftStatus::Suspended),
            _ => Err(ParseError::ShiftStatus(s.to_string())),
        }
    }
}

/// Shift summary for UI display and reports
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShiftSummary {
    /// Shift ID
    pub id: String,
    /// Shift number (e.g., "POS-001-20241213-001")
    pub shift_number: String,
    /// Operator who owns the shift
    pub operator_id: OperatorId,
    /// The operator's name as recorded on this summary.
    ///
    /// Optional because the `shifts` table stores only `operator_id`; the name has to be looked
    /// up, and two call sites currently do not (`shift_service.rs` and `z_report_service.rs`,
    /// both marked `// Would need lookup`). `None` says the lookup did not happen, which is the
    /// truth; an empty string said the operator had no name.
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
// The one it replaces filled `operator_id` and `operator_name` with empty strings and set
// `status: ShiftStatus::Active` — so `ShiftSummary::default()` was an *open shift belonging to
// nobody*, and every `..Default::default()` inherited that. `shifts.operator_id` is `NOT NULL`,
// so there is no shift without an operator to model; a caller that wants a summary states who it
// belongs to.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shift_status_round_trips_through_its_stored_form() {
        for status in [
            ShiftStatus::Active,
            ShiftStatus::Closed,
            ShiftStatus::Suspended,
        ] {
            assert_eq!(ShiftStatus::from_str(status.as_str()), Ok(status));
        }
    }

    #[test]
    fn shift_status_parses_either_crate_s_casing() {
        // `pos-models` writes "active"; `pos-db` writes "ACTIVE" into the same column.
        assert_eq!(ShiftStatus::from_str("active"), Ok(ShiftStatus::Active));
        assert_eq!(ShiftStatus::from_str("ACTIVE"), Ok(ShiftStatus::Active));
        assert_eq!(
            ShiftStatus::from_str("Suspended"),
            Ok(ShiftStatus::Suspended)
        );
    }

    #[test]
    fn shift_status_rejects_unknown_rather_than_reporting_it_open() {
        // The defect this guards: the previous inherent `from_str` returned `Self` and mapped
        // every unrecognised string to `Active`, so a corrupt status column read back as an
        // open shift. A shift that cannot be identified must not silently look open.
        assert_eq!(
            ShiftStatus::from_str("bogus"),
            Err(ParseError::ShiftStatus("bogus".to_string()))
        );
        assert_eq!(
            ShiftStatus::from_str(""),
            Err(ParseError::ShiftStatus(String::new()))
        );
        let message = ShiftStatus::from_str("bogus").unwrap_err().to_string();
        assert!(message.contains("bogus"), "{message}");
    }

    #[test]
    fn defaults_are_unchanged_by_the_move_to_derive() {
        assert_eq!(ShiftStatus::default(), ShiftStatus::Active);
        assert_eq!(VarianceStatus::default(), VarianceStatus::Balanced);
    }

    #[test]
    fn variance_status_follows_the_sign_of_the_variance() {
        assert_eq!(
            VarianceStatus::from_variance(Decimal::ZERO),
            VarianceStatus::Balanced
        );
        assert_eq!(
            VarianceStatus::from_variance(Decimal::from(-1)),
            VarianceStatus::Short
        );
        assert_eq!(
            VarianceStatus::from_variance(Decimal::from(1)),
            VarianceStatus::Over
        );
    }
}
