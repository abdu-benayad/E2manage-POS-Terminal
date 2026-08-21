//! Parsing the enums this crate reads back from text.
//!
//! These implement `FromStr` rather than carrying an inherent `from_str`, so the parse is
//! reachable through the standard trait and its failure is a value the caller must handle.
//! Where a default is genuinely wanted — parsing a level out of an arbitrary log line — the
//! call site says so with `unwrap_or`, rather than the parser deciding silently for everyone.

use thiserror::Error;

/// A string that names no variant of the type it was read as.
///
/// Each variant carries the rejected input verbatim, so the message names the value to fix.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ParseError {
    /// A log line's level field held something other than a known level.
    #[error("`{0}` is not a log level (expected ERROR, WARN, INFO, DEBUG or TRACE)")]
    LogLevel(String),

    /// A return's reason held something other than a known reason.
    #[error("`{0}` is not a return reason (expected DEFECTIVE, WRONG_PRODUCT, CHANGED_MIND, EXPIRED, PRICE_ADJUSTMENT, EXCHANGE or OTHER)")]
    ReturnReason(String),

    /// A refund's method held something other than a known method.
    #[error("`{0}` is not a refund method (expected CASH, CARD, STORE_CREDIT or EXCHANGE)")]
    RefundMethod(String),
}
