//! Reading domain values out of SQLite columns.
//!
//! These are free functions and not `impl FromSql for …`, because that impl would be `E0117`:
//! `FromSql` belongs to `rusqlite` and the domain types to `pos-models`, and neither is local to
//! this crate. Depending on a crate confers no coherence standing over its types — the same wall
//! that shaped `pos_models::StoreFailure`.
//!
//! Every one of these **returns the parse failure rather than defaulting**. A stored value the
//! domain type does not admit means the row was written by something other than this code, or
//! the contract moved; reading it as a plausible default is how a corrupt row becomes a silent
//! privilege or a silently misattributed sale.
//!
//! The public ones are used by `pos-services`, which reads a few rows directly.

use pos_models::{OperatorId, OperatorName, OperatorRole, RecordedOperatorName};
use rusqlite::types::Type;
use rusqlite::{Error, Result as SqliteResult, Row};

/// Wraps a domain parse failure as the conversion failure it is, keeping the column index so the
/// row that needs fixing can be found.
fn conversion_failed(index: usize, error: impl std::error::Error + Send + Sync + 'static) -> Error {
    Error::FromSqlConversionFailure(index, Type::Text, Box::new(error))
}

/// Reads an operator id from a `NOT NULL` column.
pub fn operator_id(row: &Row<'_>, index: usize) -> SqliteResult<OperatorId> {
    let raw: String = row.get(index)?;
    OperatorId::new(raw).map_err(|error| conversion_failed(index, error))
}

/// Reads an operator id from a nullable column.
///
/// `NULL` is `None`. An empty string is **not** `None`: it is a value that should never have been
/// written, and reporting it is the point of the newtype.
pub fn optional_operator_id(row: &Row<'_>, index: usize) -> SqliteResult<Option<OperatorId>> {
    match row.get::<_, Option<String>>(index)? {
        None => Ok(None),
        Some(raw) => OperatorId::new(raw)
            .map(Some)
            .map_err(|error| conversion_failed(index, error)),
    }
}

/// Reads the operator name recorded on a document, from a nullable column.
pub fn optional_recorded_operator_name(
    row: &Row<'_>,
    index: usize,
) -> SqliteResult<Option<RecordedOperatorName>> {
    match row.get::<_, Option<String>>(index)? {
        None => Ok(None),
        Some(raw) => RecordedOperatorName::new(raw)
            .map(Some)
            .map_err(|error| conversion_failed(index, error)),
    }
}

/// Reads an operator's name from the two columns the store keeps it in.
///
/// Takes both indices because the two columns are **one** value. Read separately they can drift:
/// a row with a blank Latin name and a present Arabic one is an operator the domain says cannot
/// exist, and only a reader holding both at once can say so.
pub fn operator_name(
    row: &Row<'_>,
    latin_index: usize,
    arabic_index: usize,
) -> SqliteResult<OperatorName> {
    let latin: String = row.get(latin_index)?;
    let arabic: Option<String> = row.get(arabic_index)?;
    OperatorName::new(latin, arabic).map_err(|error| conversion_failed(latin_index, error))
}

/// Reads an operator's role, refusing a value the server's enum does not admit.
///
/// A role this till does not recognise means the contract moved; reading it as `Cashier` would be
/// a privilege decision made by a fallback.
pub fn operator_role(row: &Row<'_>, index: usize) -> SqliteResult<OperatorRole> {
    let raw: String = row.get(index)?;
    raw.parse().map_err(|error| conversion_failed(index, error))
}
