//! Reading and writing domain values across a SQLite column.
//!
//! These are free functions and values, not `impl FromSql for …` / `impl ToSql for …`, because
//! those impls would be `E0117`: the traits belong to `rusqlite` and the domain types to
//! `pos-models`, and neither is local to this crate. Depending on a crate confers no coherence
//! standing over its types — the same wall that shaped `pos_models::StoreFailure`. **That applies
//! in the write direction exactly as it does in the read direction**, which is why a column's two
//! halves are a [`ColumnCodec`] value rather than a trait impl pair.
//!
//! Every reader here **returns the parse failure rather than defaulting**. A stored value the
//! domain type does not admit means the row was written by something other than this code, or the
//! contract moved; reading it as a plausible default is how a corrupt row becomes a silent
//! privilege or a silently misattributed sale.
//!
//! # Why a codec is one value and not two functions
//!
//! A read that disagrees with its write is the defect this module exists to prevent, and two
//! separately-named functions are where that disagreement lives. Pairing them means a call site
//! names the column's conversion once and gets both directions, and `row_mapping!` can derive the
//! `SELECT` list, the `INSERT` list and both halves of the row from one declaration.
//!
//! # Why `to_sql` is fallible
//!
//! Not defensiveness — one real codec needs it. [`PERMISSIONS`] serialises to JSON and can fail.
//! With an infallible socket the only moves would be `unwrap`, forbidden in non-test code, or
//! `Value::Null` — which writes **no permissions** for an operator whose permissions would not
//! serialise, the same value as a genuinely unprivileged cashier and indistinguishable from one.
//! That is precisely the defect this crate already removed from the read direction; an infallible
//! `to_sql` would have reintroduced it going the other way.

use pos_models::{
    OperatorId, OperatorName, OperatorPermissions, OperatorRole, RecordedOperatorName,
};
use rusqlite::types::{Type, Value};
use rusqlite::{Error, Result as SqliteResult, Row};
use rust_decimal::Decimal;

use crate::{decimal_from_sqlite, decimal_to_sqlite};

/// How one domain value crosses a single column, both ways.
///
/// Constructed only in this module: a codec assembled elsewhere would be a read and a write that
/// nothing keeps in agreement, which is the shape this type exists to make unavailable.
pub struct ColumnCodec<T> {
    from_sql: fn(&Row<'_>, usize) -> SqliteResult<T>,
    to_sql: fn(&T) -> SqliteResult<Value>,
}

impl<T> ColumnCodec<T> {
    const fn new(
        from_sql: fn(&Row<'_>, usize) -> SqliteResult<T>,
        to_sql: fn(&T) -> SqliteResult<Value>,
    ) -> Self {
        Self { from_sql, to_sql }
    }

    /// Reads the column at `index`.
    pub fn read(&self, row: &Row<'_>, index: usize) -> SqliteResult<T> {
        (self.from_sql)(row, index)
    }

    /// Renders the value for binding.
    pub fn write(&self, value: &T) -> SqliteResult<Value> {
        (self.to_sql)(value)
    }
}

// Hand-written rather than derived: the fields are fn pointers, so a codec is `Copy` whatever `T`
// is, and `#[derive(Copy)]` would wrongly demand `T: Copy`.
impl<T> Clone for ColumnCodec<T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T> Copy for ColumnCodec<T> {}

/// How one domain value crosses **two** columns, both ways.
///
/// Two columns because they are one value. A row with a blank Latin name and a present Arabic one
/// is an operator the domain says cannot exist, and only a reader holding both at once can say so
/// — so splitting this into two `ColumnCodec`s would reintroduce the drift it prevents.
pub struct PairColumnCodec<T> {
    from_sql: fn(&Row<'_>, usize, usize) -> SqliteResult<T>,
    to_sql: fn(&T) -> SqliteResult<(Value, Value)>,
}

impl<T> PairColumnCodec<T> {
    const fn new(
        from_sql: fn(&Row<'_>, usize, usize) -> SqliteResult<T>,
        to_sql: fn(&T) -> SqliteResult<(Value, Value)>,
    ) -> Self {
        Self { from_sql, to_sql }
    }

    /// Reads the two columns at `first` and `second` as one value.
    pub fn read(&self, row: &Row<'_>, first: usize, second: usize) -> SqliteResult<T> {
        (self.from_sql)(row, first, second)
    }

    /// Renders the value as the two columns it occupies, in that order.
    pub fn write(&self, value: &T) -> SqliteResult<(Value, Value)> {
        (self.to_sql)(value)
    }
}

impl<T> Clone for PairColumnCodec<T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T> Copy for PairColumnCodec<T> {}

/// Wraps a domain parse failure as the conversion failure it is, keeping the column index so the
/// row that needs fixing can be found.
fn conversion_failed(index: usize, error: impl std::error::Error + Send + Sync + 'static) -> Error {
    Error::FromSqlConversionFailure(index, Type::Text, Box::new(error))
}

/// Wraps a serialisation failure on the way *out*.
fn write_failed(error: impl std::error::Error + Send + Sync + 'static) -> Error {
    Error::ToSqlConversionFailure(Box::new(error))
}

fn text(value: impl Into<String>) -> Value {
    Value::Text(value.into())
}

fn optional_text_value(value: Option<&str>) -> Value {
    match value {
        None => Value::Null,
        Some(raw) => text(raw),
    }
}

// ============================================================================
// Operator identity
// ============================================================================

/// Reads an operator id from a `NOT NULL` column.
pub fn operator_id(row: &Row<'_>, index: usize) -> SqliteResult<OperatorId> {
    let raw: String = row.get(index)?;
    OperatorId::new(raw).map_err(|error| conversion_failed(index, error))
}

fn operator_id_to_sql(id: &OperatorId) -> SqliteResult<Value> {
    Ok(text(id.as_str()))
}

/// An operator id in a `NOT NULL` column.
pub const OPERATOR_ID: ColumnCodec<OperatorId> = ColumnCodec::new(operator_id, operator_id_to_sql);

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

fn optional_operator_id_to_sql(id: &Option<OperatorId>) -> SqliteResult<Value> {
    Ok(optional_text_value(id.as_ref().map(OperatorId::as_str)))
}

/// An operator id in a nullable column.
pub const OPTIONAL_OPERATOR_ID: ColumnCodec<Option<OperatorId>> =
    ColumnCodec::new(optional_operator_id, optional_operator_id_to_sql);

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

fn optional_recorded_operator_name_to_sql(
    name: &Option<RecordedOperatorName>,
) -> SqliteResult<Value> {
    Ok(optional_text_value(
        name.as_ref().map(RecordedOperatorName::as_str),
    ))
}

/// The operator name recorded on a document, in a nullable column.
pub const OPTIONAL_RECORDED_OPERATOR_NAME: ColumnCodec<Option<RecordedOperatorName>> =
    ColumnCodec::new(
        optional_recorded_operator_name,
        optional_recorded_operator_name_to_sql,
    );

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

fn operator_name_to_sql(name: &OperatorName) -> SqliteResult<(Value, Value)> {
    Ok((text(name.latin()), optional_text_value(name.arabic())))
}

/// An operator's name, across the `name` and `name_ar` columns in that order.
pub const OPERATOR_NAME: PairColumnCodec<OperatorName> =
    PairColumnCodec::new(operator_name, operator_name_to_sql);

/// Reads an operator's role, refusing a value the server's enum does not admit.
///
/// A role this till does not recognise means the contract moved; reading it as `Cashier` would be
/// a privilege decision made by a fallback.
pub fn operator_role(row: &Row<'_>, index: usize) -> SqliteResult<OperatorRole> {
    let raw: String = row.get(index)?;
    raw.parse().map_err(|error| conversion_failed(index, error))
}

fn operator_role_to_sql(role: &OperatorRole) -> SqliteResult<Value> {
    Ok(text(role.as_wire_str()))
}

/// An operator's role, spelled as the server's enum spells it.
pub const OPERATOR_ROLE: ColumnCodec<OperatorRole> =
    ColumnCodec::new(operator_role, operator_role_to_sql);

// ============================================================================
// Operator permissions
// ============================================================================

/// Reads an operator's permissions back out of the `permissions_json` column.
///
/// A row whose permissions will not parse is a **read failure the caller sees**. It used to be
/// `.ok().unwrap_or_default()`, which turned an unreadable column into an operator holding no
/// privileges — the same value as a genuinely unprivileged cashier, and indistinguishable from
/// one. It failed closed, so nobody noticed; the mechanism was indifferent to direction.
pub fn permissions(row: &Row<'_>, index: usize) -> SqliteResult<Option<OperatorPermissions>> {
    match row.get::<_, Option<String>>(index)? {
        None => Ok(None),
        Some(json) => serde_json::from_str(&json)
            .map(Some)
            .map_err(|error| conversion_failed(index, error)),
    }
}

/// Serialises permissions for the `permissions_json` column.
///
/// **This is the reason [`ColumnCodec::write`] returns a `Result.`** `serde_json::to_string` can
/// fail, and the two things an infallible signature would have allowed here — `unwrap`, or a
/// silent `Value::Null` — are respectively forbidden in non-test code and the exact defect the
/// reader above documents removing.
///
/// `pos_models::OperatorPermissions` owns the only mapping to the server's shape, so this is a
/// call into it rather than a second spelling of the keys.
fn permissions_to_sql(permissions: &Option<OperatorPermissions>) -> SqliteResult<Value> {
    match permissions {
        None => Ok(Value::Null),
        Some(value) => serde_json::to_string(value).map(text).map_err(write_failed),
    }
}

/// An operator's permissions, in the `permissions_json` `TEXT` column.
pub const PERMISSIONS: ColumnCodec<Option<OperatorPermissions>> =
    ColumnCodec::new(permissions, permissions_to_sql);

// ============================================================================
// Money and text
// ============================================================================

/// Reads a monetary or quantity column as a `Decimal`.
pub fn decimal(row: &Row<'_>, index: usize) -> SqliteResult<Decimal> {
    Ok(decimal_from_sqlite(row.get::<_, f64>(index)?))
}

fn decimal_to_sql(value: &Decimal) -> SqliteResult<Value> {
    Ok(Value::Real(decimal_to_sqlite(value)))
}

/// A `Decimal` in a `NOT NULL REAL` column.
///
/// **Both halves currently lose information and neither reports it**: `decimal_from_sqlite` answers
/// `Decimal::ZERO` for a value it cannot convert, and `decimal_to_sqlite` answers `0.0`. That is
/// unchanged here on purpose — it is `money-and-currency-in-the-till`'s to fix, and this codec is
/// the hook that makes it a two-function change instead of a hunt through 102 call sites.
pub const DECIMAL: ColumnCodec<Decimal> = ColumnCodec::new(decimal, decimal_to_sql);

/// Reads a nullable monetary column as an optional `Decimal`.
pub fn optional_decimal(row: &Row<'_>, index: usize) -> SqliteResult<Option<Decimal>> {
    Ok(row.get::<_, Option<f64>>(index)?.map(decimal_from_sqlite))
}

fn optional_decimal_to_sql(value: &Option<Decimal>) -> SqliteResult<Value> {
    Ok(match value {
        None => Value::Null,
        Some(amount) => Value::Real(decimal_to_sqlite(amount)),
    })
}

/// A `Decimal` in a nullable `REAL` column. See [`DECIMAL`] on what both halves still lose.
pub const OPTIONAL_DECIMAL: ColumnCodec<Option<Decimal>> =
    ColumnCodec::new(optional_decimal, optional_decimal_to_sql);

/// Reads a nullable `TEXT` column.
pub fn optional_string(row: &Row<'_>, index: usize) -> SqliteResult<Option<String>> {
    row.get(index)
}

fn optional_string_to_sql(value: &Option<String>) -> SqliteResult<Value> {
    Ok(optional_text_value(value.as_deref()))
}

/// A plain nullable `TEXT` column, for a field whose column name differs from its own.
pub const OPTIONAL_TEXT: ColumnCodec<Option<String>> =
    ColumnCodec::new(optional_string, optional_string_to_sql);

#[cfg(test)]
mod tests {
    use super::*;
    use pos_models::{DiscountAuthority, DiscountPercent, Permission};
    use rusqlite::{params, Connection};

    /// Writes a value through the codec, binds what it produced, and reads it back through the
    /// same codec. A codec whose two halves disagree fails here rather than on a customer's row.
    fn round_trip<T>(codec: ColumnCodec<T>, value: &T) -> T {
        let conn = Connection::open_in_memory().expect("in-memory database");
        let stored = codec.write(value).expect("the write half");
        conn.query_row("SELECT ?1", params![stored], |row| codec.read(row, 0))
            .expect("the read half")
    }

    fn read_one<T>(codec: ColumnCodec<T>, stored: Value) -> SqliteResult<T> {
        let conn = Connection::open_in_memory().expect("in-memory database");
        conn.query_row("SELECT ?1", params![stored], |row| codec.read(row, 0))
    }

    #[test]
    fn operator_id_survives_both_directions() {
        let id = OperatorId::new("op-1").unwrap();
        assert_eq!(round_trip(OPERATOR_ID, &id), id);
    }

    #[test]
    fn an_optional_operator_id_keeps_null_and_a_value_apart() {
        let present = Some(OperatorId::new("op-1").unwrap());
        assert_eq!(round_trip(OPTIONAL_OPERATOR_ID, &present), present);
        assert_eq!(round_trip(OPTIONAL_OPERATOR_ID, &None), None);
    }

    #[test]
    fn an_empty_operator_id_is_refused_rather_than_read_as_absent() {
        // The distinction the newtype exists for: NULL is "nobody recorded one", `""` is a value
        // that should never have been written. Reading the second as the first hides the bug.
        assert!(read_one(OPTIONAL_OPERATOR_ID, Value::Text(String::new())).is_err());
        assert_eq!(read_one(OPTIONAL_OPERATOR_ID, Value::Null).unwrap(), None);
    }

    #[test]
    fn operator_role_survives_both_directions() {
        for role in [
            OperatorRole::Cashier,
            OperatorRole::Supervisor,
            OperatorRole::Manager,
        ] {
            assert_eq!(round_trip(OPERATOR_ROLE, &role), role);
        }
    }

    #[test]
    fn an_unrecognised_role_is_refused_rather_than_read_as_cashier() {
        // A role this till does not know means the contract moved. Defaulting would be a
        // privilege decision made by a fallback.
        assert!(read_one(OPERATOR_ROLE, Value::Text("EMPEROR".to_string())).is_err());
    }

    #[test]
    fn an_operator_name_survives_both_columns_in_both_directions() {
        let conn = Connection::open_in_memory().unwrap();
        for (latin, arabic) in [("Ahmed Hassan", Some("أحمد حسن")), ("Sara", None)] {
            let name = OperatorName::new(latin, arabic).unwrap();
            let (first, second) = OPERATOR_NAME.write(&name).unwrap();
            let read = conn
                .query_row("SELECT ?1, ?2", params![first, second], |row| {
                    OPERATOR_NAME.read(row, 0, 1)
                })
                .unwrap();
            assert_eq!(read.latin(), latin);
            assert_eq!(read.arabic(), arabic);
        }
    }

    #[test]
    fn the_two_name_columns_are_written_in_projection_order() {
        // Latin first, Arabic second — the order `name, name_ar` appears in every projection.
        // Swapped, a round trip through the pair still passes, so assert the order itself.
        let name = OperatorName::new("Ahmed", Some("أحمد")).unwrap();
        let (latin, arabic) = OPERATOR_NAME.write(&name).unwrap();
        assert_eq!(latin, Value::Text("Ahmed".to_string()));
        assert_eq!(arabic, Value::Text("أحمد".to_string()));
    }

    #[test]
    fn permissions_survive_both_directions() {
        let value = Some(OperatorPermissions::new(
            [Permission::VoidTransaction, Permission::ViewReports],
            DiscountAuthority::UpTo(DiscountPercent::new(Decimal::from(10)).unwrap()),
        ));
        assert_eq!(round_trip(PERMISSIONS, &value), value);
        assert_eq!(round_trip(PERMISSIONS, &None), None);
    }

    #[test]
    fn unreadable_permissions_are_a_read_failure_not_an_unprivileged_operator() {
        // The whole point of the column: `.ok().unwrap_or_default()` here used to turn a broken
        // row into a cashier with no privileges, indistinguishable from a real one.
        let broken = Value::Text("{not json".to_string());
        assert!(read_one(PERMISSIONS, broken).is_err());
    }

    // The *write* half of `PERMISSIONS` is the reason `ColumnCodec::write` returns a `Result`, and
    // it has no test because no value of `OperatorPermissions` can currently make
    // `serde_json::to_string` fail — it is a struct of enums and a `Decimal`. That is a fact about
    // today's type, not about the signature: the socket exists so that a future shape which *can*
    // fail is reported rather than `unwrap`ed or silently written as NULL. Recorded here rather
    // than faked with a test that could not fail, which is the shape this repo keeps paying for.

    #[test]
    fn decimal_survives_both_directions() {
        for raw in ["0", "12.5", "-3.25", "199.999999"] {
            let amount: Decimal = raw.parse().unwrap();
            assert_eq!(round_trip(DECIMAL, &amount), amount);
        }
    }

    #[test]
    fn an_optional_decimal_keeps_null_and_zero_apart() {
        // Zero money and no money are different answers, and a codec that flattened them would be
        // the same defect as `unwrap_or(0.0)` one layer down.
        assert_eq!(round_trip(OPTIONAL_DECIMAL, &None), None);
        let zero = Some(Decimal::ZERO);
        assert_eq!(round_trip(OPTIONAL_DECIMAL, &zero), zero);
        assert_eq!(
            OPTIONAL_DECIMAL.write(&None).unwrap(),
            Value::Null,
            "None must reach the column as NULL, not as 0.0"
        );
    }

    #[test]
    fn optional_text_keeps_null_and_the_empty_string_apart() {
        assert_eq!(round_trip(OPTIONAL_TEXT, &None), None);
        let empty = Some(String::new());
        assert_eq!(round_trip(OPTIONAL_TEXT, &empty), empty);
        assert_eq!(OPTIONAL_TEXT.write(&None).unwrap(), Value::Null);
    }

    #[test]
    fn a_recorded_operator_name_survives_both_directions() {
        let name = Some(RecordedOperatorName::new("Ahmed Hassan").unwrap());
        assert_eq!(round_trip(OPTIONAL_RECORDED_OPERATOR_NAME, &name), name);
        assert_eq!(round_trip(OPTIONAL_RECORDED_OPERATOR_NAME, &None), None);
    }
}
