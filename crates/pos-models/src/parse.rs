//! Parsing domain enums from the strings that storage and the wire use.
//!
//! Every enum in this crate that is persisted as text implements `FromStr` rather than carrying
//! an inherent `from_str`, so the parse is reachable through the standard trait and its failure
//! is a value the caller must handle.
//!
//! Several of these previously returned `Self` and mapped any unrecognised string onto a default
//! variant. Where that default is genuinely wanted the call site now says so with `unwrap_or`,
//! rather than the parser deciding silently on behalf of every caller.

use thiserror::Error;

/// A stored or transmitted string that names no variant of the domain type it was read as.
///
/// Each variant carries the rejected input verbatim. "Invalid status" without the value is not
/// a diagnosable error — the point of the message is to name the row that has to be fixed.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ParseError {
    /// A shift's status column held something other than a known shift status.
    #[error("`{0}` is not a shift status (expected active, closed or suspended)")]
    ShiftStatus(String),

    /// A transaction's status column held something other than a known transaction status.
    #[error("`{0}` is not a transaction status (expected DRAFT, COMPLETED, VOIDED or RETURNED)")]
    TransactionStatus(String),

    /// A payment's method column held something other than a known payment method.
    #[error("`{0}` is not a payment method (expected CASH, CARD, WALLET, CREDIT or OTHER)")]
    PaymentMethod(String),

    /// A product's type column held something other than a known product type.
    #[error("`{0}` is not a product type (expected PHYSICAL_GOOD, CONSUMABLE, BUNDLE, SERVICE, FEE, LABOR, DIGITAL, SUBSCRIPTION, RENTAL, WARRANTY or RAW_MATERIAL)")]
    ProductType(String),

    /// A product's nature column held something other than a known product nature.
    #[error("`{0}` is not a product nature (expected TANGIBLE, INTANGIBLE or HYBRID)")]
    ProductNature(String),
}
