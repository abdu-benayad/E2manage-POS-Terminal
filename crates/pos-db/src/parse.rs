//! Parsing the enums this crate stores as text columns.
//!
//! Every such enum implements `FromStr` rather than carrying an inherent `from_str`, so the
//! parse is reachable through the standard trait and its failure is a value the caller must
//! handle. Several of these previously returned `Self` and mapped any unrecognised string onto
//! a default variant, which turned a corrupt column into a plausible-looking row.

use thiserror::Error;

/// A text column that names no variant of the type it was read as.
///
/// Each variant carries the rejected input verbatim, because the message has to name the row
/// that needs fixing.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ParseError {
    /// `transactions.type` held something other than a known transaction type.
    #[error("`{0}` is not a transaction type (expected SALE, RETURN, EXCHANGE or VOID)")]
    TransactionType(String),

    /// `offline_transactions.sync_status` held something other than a known sync status.
    #[error("`{0}` is not a sync status (expected PENDING, SYNCING, SYNCED, FAILED, CONFLICT or DISCARDED)")]
    SyncStatus(String),

    /// `shifts.status` held something other than a known shift status.
    #[error("`{0}` is not a shift status (expected ACTIVE, CLOSED or SUSPENDED)")]
    ShiftStatus(String),

    /// `draft_sync_queue.operation` held something other than a known operation.
    #[error("`{0}` is not a draft sync operation (expected CREATE, CONVERT or DELETE)")]
    DraftSyncOperation(String),

    /// `draft_sync_queue.sync_status` held something other than a known queue status.
    #[error(
        "`{0}` is not a draft queue sync status (expected PENDING, SYNCING, SYNCED or FAILED)"
    )]
    DraftQueueSyncStatus(String),

    /// `z_reports.variance_status` held something other than a known variance status.
    #[error("`{0}` is not a variance status (expected balanced, short or over)")]
    VarianceStatus(String),

    /// `shared_drafts.sync_status` held something other than a known shared-draft status.
    #[error("`{0}` is not a shared draft sync status (expected SYNCED, PENDING_CONVERT or PENDING_DELETE)")]
    SharedDraftSyncStatus(String),
}
