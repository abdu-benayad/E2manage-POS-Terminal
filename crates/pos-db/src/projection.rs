//! Row mappings: the ordered columns of one row shape, and everything that moves it.
//!
//! A **row mapping** is the ordered columns a query asks for, the reading of a row of exactly that
//! shape, the binding of a value back to those columns, and what happens when the key already
//! exists. One value, because every defect this module exists to end is a disagreement between two
//! of those, and separate objects are where such a disagreement lives.
//!
//! Before this module the same row shape was stated four times with nothing linking them: the
//! `SELECT` list once per query, the `row.get(N)` block once per query, the `INSERT` column list,
//! and the `params![]` beside it. `ProductRow` was written out six times; `OfflineTransactionRow`
//! five, across two crates, and the fifth had already drifted.
//!
//! # The two reader families, and why the free ones are not a convenience
//!
//! [`scalar`], [`read_one`] and [`read_all`] take a `&Connection`. The [`Database`] methods lock
//! and delegate. **This is not duplication for taste.** `Database`'s guard is a `parking_lot::
//! Mutex`, which is *not* reentrant, and several call sites already hold it across their reads —
//! `z_reports::get_day_totals` holds it across two row-shaped reads, and `return_service` and
//! `offline_service` have the same shape. A `&self` method dropped in at any of them deadlocks the
//! till with no error and no panic. Callers that already hold the guard pass it; callers that do
//! not use the method.

use rusqlite::types::{FromSql, Value};
use rusqlite::{Connection, OptionalExtension, Params, Result as SqliteResult, Row};

use crate::connection::Database;

/// Reads a row left to right, one projected column at a time.
///
/// The cursor exists so that `row_mapping!` needs no index arithmetic: it emits one `take` per
/// declared column, in declaration order, and the index is the cursor's business. A cursor
/// advanced by hand would reintroduce exactly what it removes — `take` in the wrong order is a
/// swap, and nothing would say so — which is why the generated reader binds each column to a named
/// local and builds the struct literal from those, leaving field order irrelevant.
///
/// **Two lifetime parameters, deliberately.** `RowCursor<'a> { row: &'a Row<'a> }` also compiles,
/// but only because `Row` is covariant in `'stmt` — the variance of a private field in a vendored
/// crate, which its API does not promise. One extra parameter costs no call site and removes the
/// dependency on that.
pub struct RowCursor<'a, 'stmt> {
    row: &'a Row<'stmt>,
    next: usize,
}

impl<'a, 'stmt> RowCursor<'a, 'stmt> {
    /// Starts at the first projected column.
    pub fn new(row: &'a Row<'stmt>) -> Self {
        Self { row, next: 0 }
    }

    fn advance(&mut self) -> usize {
        let index = self.next;
        self.next += 1;
        index
    }

    /// Takes the next column as an ordinary `FromSql` value.
    //
    // The split across two lines below is DELIBERATE and load-bearing, not a formatting accident,
    // and `#[rustfmt::skip]` is what keeps it that way: this expression fits on one line, so
    // `cargo fmt` joins it back the moment the attribute is removed. It did exactly that once
    // already while this module was being written.
    //
    // It is the shape rustfmt *produces* for a positional read carrying a chained call, and it is
    // the form a line-based scanner cannot see: line one has no `.get`, line two has no `row`.
    // Sixteen reads in this repo took that shape and every measurement of them was wrong until one
    // was written that could match across the newline. `tests/guards.rs` uses this exact read as
    // the witness for its positional-read scan — a scanner that fails to find it is a scanner that
    // would report a clean tree for a reason unrelated to the tree being clean. Joining these
    // lines does not fail the guard; it makes the guard stop being able to fail. Do not.
    #[rustfmt::skip]
    pub fn take<T: FromSql>(&mut self) -> SqliteResult<T> {
        let index = self.advance();
        self.row
            .get(index)
    }

    /// Takes the next column through a codec.
    pub fn take_via<T>(&mut self, codec: &crate::column::ColumnCodec<T>) -> SqliteResult<T> {
        let index = self.advance();
        codec.read(self.row, index)
    }

    /// Takes the next **two** columns as one value.
    ///
    /// Two columns because they are one value: an operator's name is `name` and `name_ar`, and a
    /// reader holding only one of them cannot tell a half-named row from a whole one.
    pub fn take_pair_via<T>(
        &mut self,
        codec: &crate::column::PairColumnCodec<T>,
    ) -> SqliteResult<T> {
        let first = self.advance();
        let second = self.advance();
        codec.read(self.row, first, second)
    }

    /// How many columns have been taken.
    pub fn taken(&self) -> usize {
        self.next
    }
}

/// What the store does when the row's key is already present.
///
/// **A property of the table, never of the helper that writes it.** This crate's writers use all
/// four dispositions, and unifying them would corrupt data rather than merely annoy: `features`
/// and `feature_screens` use `DO UPDATE` because `feature_screens` declares
/// `ON DELETE CASCADE` on `features(feature_id)` and this connection sets `PRAGMA foreign_keys =
/// ON` — so `INSERT OR REPLACE`, which deletes the conflicting row before inserting, would
/// **cascade-delete every screen of the feature being re-synced**. `feature_screens` also has an
/// `AUTOINCREMENT` id that delete-and-reinsert would reassign. Meanwhile `z_reports` uses a plain
/// `INSERT` so that re-running a day close errors instead of silently overwriting a finalised
/// fiscal record, and `products` needs `OR REPLACE` because the catalogue re-syncs every ten
/// minutes.
///
/// A single-write test cannot see any of this. Conflict disposition only appears on the **second**
/// write, which is why every round-trip test in this issue writes the same key twice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OnConflict {
    /// Plain `INSERT`: a duplicate key is an error the caller sees.
    Fail,
    /// `INSERT OR REPLACE`: the conflicting row is **deleted** and re-inserted. Never use this on
    /// a table any foreign key cascades from, or on one with an `AUTOINCREMENT` id a reader keeps.
    Replace,
    /// `INSERT … ON CONFLICT(key) DO UPDATE SET set = excluded.set`: the row keeps its identity.
    ///
    /// Both lists are needed and they are different: `feature_screens` conflicts on
    /// `(feature_id, screen_id)` and updates five other columns.
    Update {
        /// The conflict target — the unique or primary key columns.
        key: &'static [&'static str],
        /// The columns to overwrite from the incoming row.
        set: &'static [&'static str],
    },
}

/// The ordered columns of one row shape and the reading of a row of exactly that shape.
///
/// The read half on its own, because **some row shapes cannot be written**: `DayTotalsRow` is read
/// from an aggregate over `offline_transactions` and there is no table to insert it into. Giving
/// the read half its own type is what makes writing one a compile error rather than a runtime
/// check — `write` takes a [`RowMapping`], which an aggregate never produces, so there is no
/// "this mapping has no table" error path to get wrong or to forget to test.
///
/// # Why `entries` is a pair per column
///
/// The SQL a projection asks for and the name the result answers to are the same string for every
/// ordinary column, and **different for an aggregate**: `COALESCE(SUM(…), 0) AS gross_sales` is
/// projected as the expression and answers to `gross_sales`. One field serving both roles is a bug
/// that appears on exactly one row shape in this crate — and that shape prints the Z-report.
pub struct RowReader<T> {
    /// One entry per **column**, in projection order. A two-column field contributes two.
    entries: &'static [(&'static str, &'static str)],
    read: fn(&Row<'_>) -> SqliteResult<T>,
}

impl<T> RowReader<T> {
    /// Called only by `row_mapping!`.
    #[doc(hidden)]
    pub const fn new(
        entries: &'static [(&'static str, &'static str)],
        read: fn(&Row<'_>) -> SqliteResult<T>,
    ) -> Self {
        Self { entries, read }
    }

    /// The columns, in projection order, as the result names they answer to.
    pub fn column_names(&self) -> impl Iterator<Item = &'static str> + '_ {
        self.entries.iter().map(|(_, name)| *name)
    }

    /// How many columns this shape projects.
    pub fn width(&self) -> usize {
        self.entries.len()
    }

    /// The `SELECT` list: `"id, sku, barcode, …"`.
    pub fn select_list(&self) -> String {
        self.entries
            .iter()
            .map(|(expression, name)| Self::aliased(expression, expression, name))
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// Renders one entry, aliasing it only where the expression is not already the result name.
    ///
    /// The alias is not cosmetic. SQLite names the result column of `COUNT(*)` after the
    /// expression, and [`Self::debug_assert_column_names`] compares result names against the
    /// declared ones — so an un-aliased aggregate fails its own shape check. Every ordinary
    /// column is its own name, which is why this went unnoticed until the first aggregate: a bug
    /// invisible in 181 of the 184 mappings here.
    fn aliased(rendered: &str, expression: &str, name: &str) -> String {
        if expression == name {
            rendered.to_string()
        } else {
            format!("{rendered} AS {name}")
        }
    }

    /// The `SELECT` list with every column qualified by a table alias: `"p.id, p.sku, …"`.
    ///
    /// Only meaningful where every entry is a bare column name, which is every shape except the
    /// aggregate one — an alias in front of `COALESCE(…)` is not SQL. Nothing qualifies an
    /// aggregate, so this is a documented precondition rather than a check.
    pub fn select_list_qualified(&self, alias: &str) -> String {
        self.entries
            .iter()
            .map(|(expression, name)| {
                Self::aliased(&format!("{alias}.{expression}"), expression, name)
            })
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// Reads a row of exactly this shape.
    ///
    /// Under `cfg(debug_assertions)` this first checks that the statement's column names are the
    /// ones the shape declares. **That is checking, not constraint** — it cannot make a wrong
    /// column unrepresentable, and it is documented as such. It earns its place because it costs
    /// nothing in release and fires on every query the test suite exercises, which is a wider net
    /// than any fixed set of round-trip tests. It sits here rather than in the cursor because this
    /// is where both the declared names and the statement are in scope, so it reports the first
    /// mismatch with both spellings instead of failing somewhere downstream as a type error.
    pub fn read(&self, row: &Row<'_>) -> SqliteResult<T> {
        #[cfg(debug_assertions)]
        self.debug_assert_column_names(row);
        (self.read)(row)
    }

    #[cfg(debug_assertions)]
    fn debug_assert_column_names(&self, row: &Row<'_>) {
        let statement = row.as_ref();
        for (index, (_, declared)) in self.entries.iter().enumerate() {
            match statement.column_name(index) {
                Ok(actual) if actual.eq_ignore_ascii_case(declared) => {}
                Ok(actual) => panic!(
                    "column {index} of this query is `{actual}`, but the shape declares \
                     `{declared}`. The SQL and the mapping have drifted."
                ),
                Err(error) => panic!(
                    "the shape declares {} columns but this query returned fewer: {error}",
                    self.entries.len()
                ),
            }
        }
    }
}

/// A [`RowReader`] plus everything needed to write the row back.
///
/// Constructed only by `row_mapping!`. Assembling one by hand would let the column list and the
/// reader disagree, which is the defect the type exists to remove.
pub struct RowMapping<T> {
    reader: RowReader<T>,
    /// Columns the store writes itself, as `(column, SQL expression)`. Present in the `INSERT`,
    /// absent from the projection, never bound to a parameter. This is `updated_at =
    /// datetime('now')` and nothing else so far.
    store_managed: &'static [(&'static str, &'static str)],
    table: &'static str,
    conflict: OnConflict,
    bind: fn(&T) -> SqliteResult<Vec<Value>>,
}

impl<T> RowMapping<T> {
    /// Called only by `row_mapping!`.
    #[doc(hidden)]
    pub const fn new(
        reader: RowReader<T>,
        store_managed: &'static [(&'static str, &'static str)],
        table: &'static str,
        conflict: OnConflict,
        bind: fn(&T) -> SqliteResult<Vec<Value>>,
    ) -> Self {
        Self {
            reader,
            store_managed,
            table,
            conflict,
            bind,
        }
    }

    /// The read half, for the query helpers.
    pub fn reader(&self) -> &RowReader<T> {
        &self.reader
    }

    /// The table this row lives in.
    pub fn table(&self) -> &'static str {
        self.table
    }

    /// What happens on a duplicate key.
    pub fn conflict(&self) -> OnConflict {
        self.conflict
    }

    /// The columns an `INSERT` names, in bind order, followed by the store-managed ones.
    ///
    /// **Derived, never declared.** An `insert_columns` field beside `entries` would be a second
    /// list of the same columns that nothing keeps in agreement — the exact shape this module
    /// exists to remove, one level up.
    pub fn insert_column_names(&self) -> impl Iterator<Item = &'static str> + '_ {
        self.reader
            .column_names()
            .chain(self.store_managed.iter().map(|(column, _)| *column))
    }

    /// The `INSERT` statement for this mapping.
    pub fn insert_statement(&self) -> String {
        let columns = self.insert_column_names().collect::<Vec<_>>().join(", ");

        let bound = (1..=self.reader.width()).map(|position| format!("?{position}"));
        let managed = self
            .store_managed
            .iter()
            .map(|(_, expression)| (*expression).to_string());
        let values = bound.chain(managed).collect::<Vec<_>>().join(", ");

        let verb = match self.conflict {
            OnConflict::Replace => "INSERT OR REPLACE INTO",
            OnConflict::Fail | OnConflict::Update { .. } => "INSERT INTO",
        };

        let suffix = match self.conflict {
            OnConflict::Fail | OnConflict::Replace => String::new(),
            OnConflict::Update { key, set } => {
                let assignments = set
                    .iter()
                    .map(|column| format!("{column} = excluded.{column}"))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!(
                    " ON CONFLICT({}) DO UPDATE SET {assignments}",
                    key.join(", ")
                )
            }
        };

        let table = self.table;
        format!("{verb} {table} ({columns}) VALUES ({values}){suffix}")
    }

    /// Renders a value as the parameters its `INSERT` binds, in projection order.
    pub fn bind(&self, value: &T) -> SqliteResult<Vec<Value>> {
        (self.bind)(value)
    }
}

/// Renders any `ToSql` value as an owned [`Value`].
///
/// The bare-identifier arm of `row_mapping!` uses this: a column whose type already knows how to
/// cross the boundary needs no codec, and inventing one per primitive would be nine kinds of
/// nothing. It deliberately does **not** go through `From<ValueRef> for Value`, which `expect`s on
/// invalid UTF-8 — a panic path in non-test code, however unreachable it looks from here.
pub fn to_value<T: rusqlite::ToSql + ?Sized>(value: &T) -> SqliteResult<Value> {
    use rusqlite::types::{ToSqlOutput, ValueRef};
    match value.to_sql()? {
        ToSqlOutput::Owned(owned) => Ok(owned),
        ToSqlOutput::Borrowed(ValueRef::Null) => Ok(Value::Null),
        ToSqlOutput::Borrowed(ValueRef::Integer(int)) => Ok(Value::Integer(int)),
        ToSqlOutput::Borrowed(ValueRef::Real(real)) => Ok(Value::Real(real)),
        ToSqlOutput::Borrowed(ValueRef::Text(bytes)) => String::from_utf8(bytes.to_vec())
            .map(Value::Text)
            .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error))),
        ToSqlOutput::Borrowed(ValueRef::Blob(bytes)) => Ok(Value::Blob(bytes.to_vec())),
        // `ToSqlOutput` is `#[non_exhaustive]`, and the remaining variants are all behind rusqlite
        // features this crate does not enable. If one ever arrives, it arrives as an error naming
        // itself rather than as a silently wrong binding.
        other => Err(rusqlite::Error::ToSqlConversionFailure(Box::new(
            UnsupportedBinding(format!("{other:?}")),
        ))),
    }
}

/// A `ToSql` output shape this crate has no column for.
#[derive(Debug, thiserror::Error)]
#[error("a value rendered as `{0}`, which no column here can hold")]
pub struct UnsupportedBinding(String);

// ============================================================================
// Reading against a connection the caller already holds
// ============================================================================

/// Reads a single-column, single-row query.
///
/// A one-column projection has no ordinal to get wrong, which is why this exists rather than a
/// mapping: `SELECT COUNT(*)`, `EXISTS` probes and `PRAGMA` scalars are not the defect and do not
/// need the machinery. It takes a `&Connection` because most of its call sites are in
/// `migrations.rs`, where there is no `Database` at all.
pub fn scalar<T: FromSql>(conn: &Connection, sql: &str, params: impl Params) -> SqliteResult<T> {
    conn.query_row(sql, params, |row| row.get(0))
}

/// Reads at most one row of `shape`.
///
/// `from_clause` is everything after the projection and **includes its own `FROM`** — `"FROM
/// operators WHERE id = ?1"`. It is spelled that way because [`read_all_qualified`]'s one caller
/// needs `FROM products p JOIN products_fts fts …`, which is not a table name with decoration.
/// The parameter is named for the clause rather than the table because a `&str` that must begin
/// with a keyword is an unmarked socket: the author of this module wrote three call sites without
/// the `FROM` within an hour of writing the function. The failure is loud — SQLite refuses the
/// statement — so the name is the fix, not a validator.
pub fn read_one<T>(
    conn: &Connection,
    shape: &RowReader<T>,
    from_clause: &str,
    params: impl Params,
) -> SqliteResult<Option<T>> {
    let sql = format!("SELECT {} {from_clause}", shape.select_list());
    conn.query_row(&sql, params, |row| shape.read(row))
        .optional()
}

/// Reads every row of `shape`.
pub fn read_all<T>(
    conn: &Connection,
    shape: &RowReader<T>,
    from_clause: &str,
    params: impl Params,
) -> SqliteResult<Vec<T>> {
    let sql = format!("SELECT {} {from_clause}", shape.select_list());
    let mut statement = conn.prepare(&sql)?;
    let rows = statement.query_map(params, |row| shape.read(row))?;
    rows.collect()
}

/// Reads every row of `shape`, with each column qualified by a table alias.
///
/// For the one query in this crate that joins: the FTS5 product search needs `p.id, p.sku, …`
/// against `FROM products p JOIN products_fts fts …`.
pub fn read_all_qualified<T>(
    conn: &Connection,
    shape: &RowReader<T>,
    alias: &str,
    from_clause: &str,
    params: impl Params,
) -> SqliteResult<Vec<T>> {
    let sql = format!(
        "SELECT {} {from_clause}",
        shape.select_list_qualified(alias)
    );
    let mut statement = conn.prepare(&sql)?;
    let rows = statement.query_map(params, |row| shape.read(row))?;
    rows.collect()
}

/// Writes one row through `mapping`, honouring its [`OnConflict`].
///
/// There is no "this shape cannot be written" failure to report. A shape read from an aggregate is
/// a [`RowReader`] and never a [`RowMapping`], so it cannot reach this function at all — the
/// distinction is a type, not a branch.
pub fn write<T>(conn: &Connection, mapping: &RowMapping<T>, value: &T) -> SqliteResult<usize> {
    let bound = mapping.bind(value)?;
    conn.execute(
        &mapping.insert_statement(),
        rusqlite::params_from_iter(bound.iter()),
    )
}

// ============================================================================
// The same, for callers not already holding the lock
// ============================================================================

impl Database {
    /// Reads at most one row of `shape`.
    ///
    /// Takes the connection lock. A caller already holding it must use [`read_one`] instead — the
    /// guard is not reentrant and this would hang with no error and no panic.
    pub fn select_one<T>(
        &self,
        shape: &RowReader<T>,
        from_clause: &str,
        params: impl Params,
    ) -> SqliteResult<Option<T>> {
        let conn = self.connection();
        let conn = conn.lock();
        read_one(&conn, shape, from_clause, params)
    }

    /// Reads every row of `shape`. See [`Database::select_one`] on the lock.
    pub fn select_all<T>(
        &self,
        shape: &RowReader<T>,
        from_clause: &str,
        params: impl Params,
    ) -> SqliteResult<Vec<T>> {
        let conn = self.connection();
        let conn = conn.lock();
        read_all(&conn, shape, from_clause, params)
    }

    /// Reads every row of `shape` with each column qualified. See the lock note above.
    pub fn select_all_qualified<T>(
        &self,
        shape: &RowReader<T>,
        alias: &str,
        from_clause: &str,
        params: impl Params,
    ) -> SqliteResult<Vec<T>> {
        let conn = self.connection();
        let conn = conn.lock();
        read_all_qualified(&conn, shape, alias, from_clause, params)
    }

    /// Reads a single-column, single-row query. See the lock note above.
    pub fn select_scalar<T: FromSql>(&self, sql: &str, params: impl Params) -> SqliteResult<T> {
        let conn = self.connection();
        let conn = conn.lock();
        scalar(&conn, sql, params)
    }

    /// Writes one row through `mapping`. See the lock note above.
    pub fn insert<T>(&self, mapping: &RowMapping<T>, value: &T) -> SqliteResult<usize> {
        let conn = self.connection();
        let conn = conn.lock();
        write(&conn, mapping, value)
    }
}

// ============================================================================
// Declaring a row shape
// ============================================================================

/// Declares a row shape that can be read **and** written.
///
/// One declaration produces four things that used to be four hand-maintained lists: the `SELECT`
/// projection, the reader, the `INSERT` column list, and the parameter binding. They cannot
/// disagree because they come from one token sequence.
///
/// ```ignore
/// row_mapping! {
///     /// Every column of `operators` the till reads and writes, in one order.
///     pub const OPERATOR_ROW: RowMapping<OperatorRow> = for "operators" {
///         id                                  via column::OPERATOR_ID,
///         code,
///         employee_id,
///         name from ("name", "name_ar")       via column::OPERATOR_NAME,
///         role                                via column::OPERATOR_ROLE,
///         permissions from "permissions_json" via column::PERMISSIONS,
///         is_active,
///         managed "updated_at" = "datetime('now')",
///     } on_conflict OnConflict::Replace;
/// }
/// ```
///
/// `RowMapping<OperatorRow>` there is **macro syntax, not a type path**: the macro matches those
/// tokens and writes `$crate::projection::RowMapping<OperatorRow>` itself, so the declaring module
/// does not import `RowMapping`. It is spelled out because the declaration should read as what it
/// produces. The same is true of `RowReader<T>` in [`row_reader!`]. `OnConflict` is a real path and
/// is imported.
///
/// # The seven entry shapes
///
/// | shape | meaning |
/// |---|---|
/// | `field` | column named after the field, ordinary `FromSql`/`ToSql` |
/// | `field via CODEC` | column named after the field, domain conversion |
/// | `field from "col"` | column named differently, ordinary conversion |
/// | `field from "col" via CODEC` | both |
/// | `field from ("a", "b") via PAIR` | one value, two columns |
/// | `field from ("expr" as "name") [via CODEC]` | an aggregate: the SQL differs from the result name |
/// | `managed "col" = "sql"` | store-written, never read, never bound |
///
/// A **bare identifier means the column is named after the field**, which is 181 of the 184
/// mappings in this crate. Divergence costs explicit syntax, so the three real ones are the only
/// three places the question is even asked.
///
/// # What it expands to
///
/// Twenty readers stop being greppable and stop being visible to `symbol` when they move inside a
/// macro. This comment is the only mitigation, so `OPERATOR_ROW` is written out **in full** — no
/// elision, not even in the middle. An abbreviated expansion would misrepresent *adjacency*: a
/// reader who trusts `code` to be followed by `name` has a wrong picture of the generated code,
/// not merely an incomplete one, and a wrong picture is worse than no picture for a mitigation
/// whose entire job is to stand in for reading the real thing.
///
/// ```ignore
/// pub const OPERATOR_ROW: RowMapping<OperatorRow> = {
///     fn read(row: &rusqlite::Row<'_>) -> rusqlite::Result<OperatorRow> {
///         let mut cursor = RowCursor::new(row);
///         let id = cursor.take_via(&column::OPERATOR_ID)?;
///         let code = cursor.take()?;
///         let employee_id = cursor.take()?;
///         let employee_number = cursor.take()?;
///         let name = cursor.take_pair_via(&column::OPERATOR_NAME)?;   // consumes TWO columns
///         let role = cursor.take_via(&column::OPERATOR_ROLE)?;
///         let department = cursor.take()?;
///         let position = cursor.take()?;
///         let permissions = cursor.take_via(&column::PERMISSIONS)?;
///         let is_active = cursor.take()?;
///         Ok(OperatorRow {
///             id, code, employee_id, employee_number, name,
///             role, department, position, permissions, is_active,
///         })
///     }
///     fn bind(value: &OperatorRow) -> rusqlite::Result<Vec<Value>> {
///         let mut out = Vec::new();
///         out.extend([column::OPERATOR_ID.write(&value.id)?]);
///         out.extend([to_value(&value.code)?]);
///         out.extend([to_value(&value.employee_id)?]);
///         out.extend([to_value(&value.employee_number)?]);
///         {
///             let (first, second) = column::OPERATOR_NAME.write(&value.name)?;
///             out.extend([first, second]);
///         }
///         out.extend([column::OPERATOR_ROLE.write(&value.role)?]);
///         out.extend([to_value(&value.department)?]);
///         out.extend([to_value(&value.position)?]);
///         out.extend([column::PERMISSIONS.write(&value.permissions)?]);
///         out.extend([to_value(&value.is_active)?]);
///         Ok(out)
///     }
///     RowMapping::new(
///         RowReader::new(
///             &[
///                 ("id", "id"),
///                 ("code", "code"),
///                 ("employee_id", "employee_id"),
///                 ("employee_number", "employee_number"),
///                 ("name", "name"),
///                 ("name_ar", "name_ar"),
///                 ("role", "role"),
///                 ("department", "department"),
///                 ("position", "position"),
///                 ("permissions_json", "permissions_json"),
///                 ("is_active", "is_active"),
///             ],
///             read,
///         ),
///         &[("updated_at", "datetime('now')")],
///         "operators",
///         OnConflict::Replace,
///         bind,
///     )
/// };
/// ```
///
/// Every path the macro emits is written `$crate::…`, so a declaring module imports none of them;
/// they are spelled short above to keep the shape readable.
///
/// **Ten `let`s, eleven entries.** Not one `let` per entry — this file uses "entry" for a *column*
/// (see [`RowReader`]'s `entries`), and `name` is one `let` over two of them. One `let` per
/// declared item is the true statement; the two counts diverge by exactly the pair's extra column,
/// which is what `RowReader::width()` reports and what `RowCursor` advances past.
///
/// Every arm contributes an **array** rather than a value, which is why `bind` extends instead of
/// pushing: a pair column contributes two, and an entry that could only ever contribute one would
/// have no arm for it.
///
/// The `let` sequence and the column list come from the same tokens, so an index cannot disagree
/// with a name. The struct literal is built from **named locals**, so its field order is
/// irrelevant — reordering fields in the struct declaration cannot swap a column.
///
/// # What the compiler refuses, and what it does not
///
/// - a field of the struct absent from the declaration → **E0063**, missing field in initializer;
/// - an entry that is not a field → **E0560**, no field named, *and* **E0609**, no field on type —
///   one from the struct literal in `read`, one from `&value.<field>` in `bind`;
/// - a field listed twice → **E0062**, field specified more than once.
///
/// Those three are measured, not predicted: each was produced by mutating the `OPERATOR_ROW`
/// declaration in `operators.rs` and reading `rustc`'s output. A macro's compile-time guarantees
/// are the easiest thing in a doc comment to assert and never check.
///
/// **And there it stops. The field set is policed; the column strings are not.** This compiles
/// clean with zero warnings:
///
/// ```ignore
/// department from "position" via column::OPTIONAL_TEXT,
/// position,
/// ```
///
/// and `department` silently receives the `position` value. Nothing here ties a `from` literal to
/// the schema. Two other things do, and they are not optional extras: the per-mapping
/// no-duplicate-column assertion and the `PRAGMA table_info` subset check
/// (`every_mapping_names_columns_the_schema_has`) catch a column that does not exist or is already
/// spoken for, and the column-identity tests — write through the store, read every column back
/// **by name** in a hand-written query — catch a column that exists and is the wrong one.
///
/// # There is no rest-init arm, and there must not be
///
/// Two row shapes in this crate close their reads with `..Default::default()` because the struct
/// is wider than the table. Teaching this macro that would make "a column cannot be added to a row
/// type without being added to the declaration" false for every declaration that used it —
/// starting with the shape that carries money. Those two split at the store boundary instead.
#[macro_export]
macro_rules! row_mapping {
    (
        $(#[$meta:meta])*
        $vis:vis const $name:ident: RowMapping<$row:ident> = for $table:literal {
            $($entries:tt)*
        } on_conflict $conflict:expr;
    ) => {
        $(#[$meta])*
        $vis const $name: $crate::projection::RowMapping<$row> = $crate::__row_shape!(
            @munch (mapping $table $conflict) $row cursor value out
            [] [] [] [] []
            $($entries)*
        );
    };
}

/// Declares a row shape that can only be **read**.
///
/// For a shape with no table to write back to — an aggregate. Because it produces a
/// [`RowReader`] and never a [`RowMapping`], and `write` takes a `RowMapping`, writing one is a
/// compile error rather than a runtime check. The entry shapes are the same, minus `managed`.
///
/// ```ignore
/// row_reader! {
///     /// The day's totals, aggregated over `offline_transactions`.
///     pub const DAY_TOTALS_ROW: RowReader<DayTotalsRow> = {
///         transaction_count from ("COALESCE(COUNT(*), 0)" as "transaction_count"),
///         gross_sales from ("COALESCE(SUM(total), 0)" as "gross_sales") via column::DECIMAL,
///     };
/// }
/// ```
#[macro_export]
macro_rules! row_reader {
    (
        $(#[$meta:meta])*
        $vis:vis const $name:ident: RowReader<$row:ident> = {
            $($entries:tt)*
        };
    ) => {
        $(#[$meta])*
        $vis const $name: $crate::projection::RowReader<$row> = $crate::__row_shape!(
            @munch (reader) $row cursor value out
            [] [] [] [] []
            $($entries)*
        );
    };
}

/// The token muncher behind [`row_mapping!`] and [`row_reader!`]. Not public API.
///
/// A muncher rather than one arm, and this is not a stylistic preference. `macro_rules!` has no
/// `else`, so a single arm with optional `$(via …)?` fragments silently drops them; and a
/// two-column entry has to contribute **two** elements to a `&'static [_]`, which a macro in
/// expression position cannot do at all. The cursor, the bound value and the output vector are
/// threaded as `$cur $val $out` because macro hygiene would otherwise make the identifier written
/// in the terminal arm a *different* identifier from the one written in the recursive arms.
#[doc(hidden)]
#[macro_export]
macro_rules! __row_shape {
    // ---------------------------------------------------------------- terminal: read-only
    (@munch (reader) $row:ident $cur:ident $val:ident $out:ident
        [$($entry:expr,)*] [$($read:tt)*] [$($field:ident,)*] [$($bind:tt)*] [$($managed:expr,)*]
    ) => {{
        fn read(row: &::rusqlite::Row<'_>) -> ::rusqlite::Result<$row> {
            let mut $cur = $crate::projection::RowCursor::new(row);
            $($read)*
            ::std::result::Result::Ok($row { $($field,)* })
        }
        $crate::projection::RowReader::new(&[$($entry,)*], read)
    }};

    // ---------------------------------------------------------------- terminal: readable + writable
    (@munch (mapping $table:literal $conflict:expr) $row:ident $cur:ident $val:ident $out:ident
        [$($entry:expr,)*] [$($read:tt)*] [$($field:ident,)*] [$($bind:tt)*] [$($managed:expr,)*]
    ) => {{
        fn read(row: &::rusqlite::Row<'_>) -> ::rusqlite::Result<$row> {
            let mut $cur = $crate::projection::RowCursor::new(row);
            $($read)*
            ::std::result::Result::Ok($row { $($field,)* })
        }
        fn bind($val: &$row) -> ::rusqlite::Result<::std::vec::Vec<::rusqlite::types::Value>> {
            let mut $out = ::std::vec::Vec::new();
            $($bind)*
            ::std::result::Result::Ok($out)
        }
        $crate::projection::RowMapping::new(
            $crate::projection::RowReader::new(&[$($entry,)*], read),
            &[$($managed,)*],
            $table,
            $conflict,
            bind,
        )
    }};

    // ---------------------------------------------------------------- managed "col" = "sql"
    (@munch $mode:tt $row:ident $cur:ident $val:ident $out:ident
        [$($entry:expr,)*] [$($read:tt)*] [$($field:ident,)*] [$($bind:tt)*] [$($managed:expr,)*]
        managed $column:literal = $sql:literal, $($rest:tt)*
    ) => {
        $crate::__row_shape!(@munch $mode $row $cur $val $out
            [$($entry,)*] [$($read)*] [$($field,)*] [$($bind)*] [$($managed,)* ($column, $sql),]
            $($rest)*)
    };

    // ---------------------------------------------------------------- field from ("expr" as "name") via CODEC
    (@munch $mode:tt $row:ident $cur:ident $val:ident $out:ident
        [$($entry:expr,)*] [$($read:tt)*] [$($field:ident,)*] [$($bind:tt)*] [$($managed:expr,)*]
        $f:ident from ($expr:literal as $name:literal) via $codec:path, $($rest:tt)*
    ) => {
        $crate::__row_shape!(@munch $mode $row $cur $val $out
            [$($entry,)* ($expr, $name),]
            [$($read)* let $f = $cur.take_via(&$codec)?;]
            [$($field,)* $f,]
            [$($bind)* $out.extend([$codec.write(&$val.$f)?]);]
            [$($managed,)*]
            $($rest)*)
    };

    // ---------------------------------------------------------------- field from ("expr" as "name")
    (@munch $mode:tt $row:ident $cur:ident $val:ident $out:ident
        [$($entry:expr,)*] [$($read:tt)*] [$($field:ident,)*] [$($bind:tt)*] [$($managed:expr,)*]
        $f:ident from ($expr:literal as $name:literal), $($rest:tt)*
    ) => {
        $crate::__row_shape!(@munch $mode $row $cur $val $out
            [$($entry,)* ($expr, $name),]
            [$($read)* let $f = $cur.take()?;]
            [$($field,)* $f,]
            [$($bind)* $out.extend([$crate::projection::to_value(&$val.$f)?]);]
            [$($managed,)*]
            $($rest)*)
    };

    // ---------------------------------------------------------------- field from ("a", "b") via PAIR
    (@munch $mode:tt $row:ident $cur:ident $val:ident $out:ident
        [$($entry:expr,)*] [$($read:tt)*] [$($field:ident,)*] [$($bind:tt)*] [$($managed:expr,)*]
        $f:ident from ($first:literal, $second:literal) via $codec:path, $($rest:tt)*
    ) => {
        $crate::__row_shape!(@munch $mode $row $cur $val $out
            [$($entry,)* ($first, $first), ($second, $second),]
            [$($read)* let $f = $cur.take_pair_via(&$codec)?;]
            [$($field,)* $f,]
            [$($bind)* {
                let (first, second) = $codec.write(&$val.$f)?;
                $out.extend([first, second]);
            }]
            [$($managed,)*]
            $($rest)*)
    };

    // ---------------------------------------------------------------- field from "col" via CODEC
    (@munch $mode:tt $row:ident $cur:ident $val:ident $out:ident
        [$($entry:expr,)*] [$($read:tt)*] [$($field:ident,)*] [$($bind:tt)*] [$($managed:expr,)*]
        $f:ident from $column:literal via $codec:path, $($rest:tt)*
    ) => {
        $crate::__row_shape!(@munch $mode $row $cur $val $out
            [$($entry,)* ($column, $column),]
            [$($read)* let $f = $cur.take_via(&$codec)?;]
            [$($field,)* $f,]
            [$($bind)* $out.extend([$codec.write(&$val.$f)?]);]
            [$($managed,)*]
            $($rest)*)
    };

    // ---------------------------------------------------------------- field from "col"
    (@munch $mode:tt $row:ident $cur:ident $val:ident $out:ident
        [$($entry:expr,)*] [$($read:tt)*] [$($field:ident,)*] [$($bind:tt)*] [$($managed:expr,)*]
        $f:ident from $column:literal, $($rest:tt)*
    ) => {
        $crate::__row_shape!(@munch $mode $row $cur $val $out
            [$($entry,)* ($column, $column),]
            [$($read)* let $f = $cur.take()?;]
            [$($field,)* $f,]
            [$($bind)* $out.extend([$crate::projection::to_value(&$val.$f)?]);]
            [$($managed,)*]
            $($rest)*)
    };

    // ---------------------------------------------------------------- field via CODEC
    (@munch $mode:tt $row:ident $cur:ident $val:ident $out:ident
        [$($entry:expr,)*] [$($read:tt)*] [$($field:ident,)*] [$($bind:tt)*] [$($managed:expr,)*]
        $f:ident via $codec:path, $($rest:tt)*
    ) => {
        $crate::__row_shape!(@munch $mode $row $cur $val $out
            [$($entry,)* (stringify!($f), stringify!($f)),]
            [$($read)* let $f = $cur.take_via(&$codec)?;]
            [$($field,)* $f,]
            [$($bind)* $out.extend([$codec.write(&$val.$f)?]);]
            [$($managed,)*]
            $($rest)*)
    };

    // ---------------------------------------------------------------- field
    (@munch $mode:tt $row:ident $cur:ident $val:ident $out:ident
        [$($entry:expr,)*] [$($read:tt)*] [$($field:ident,)*] [$($bind:tt)*] [$($managed:expr,)*]
        $f:ident, $($rest:tt)*
    ) => {
        $crate::__row_shape!(@munch $mode $row $cur $val $out
            [$($entry,)* (stringify!($f), stringify!($f)),]
            [$($read)* let $f = $cur.take()?;]
            [$($field,)* $f,]
            [$($bind)* $out.extend([$crate::projection::to_value(&$val.$f)?]);]
            [$($managed,)*]
            $($rest)*)
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::column;
    use rusqlite::params;

    /// A two-column shape used to exercise the machinery without waiting for `row_mapping!`.
    #[derive(Debug, PartialEq, Eq)]
    struct Sample {
        id: String,
        label: Option<String>,
    }

    fn read_sample(row: &Row<'_>) -> SqliteResult<Sample> {
        let mut cursor = RowCursor::new(row);
        let id = cursor.take()?;
        let label = cursor.take_via(&column::OPTIONAL_TEXT)?;
        Ok(Sample { id, label })
    }

    fn bind_sample(value: &Sample) -> SqliteResult<Vec<Value>> {
        Ok(vec![
            Value::Text(value.id.clone()),
            column::OPTIONAL_TEXT.write(&value.label)?,
        ])
    }

    const SAMPLE_READER: RowReader<Sample> =
        RowReader::new(&[("id", "id"), ("label", "label")], read_sample);

    const SAMPLE: RowMapping<Sample> = RowMapping::new(
        SAMPLE_READER,
        &[("updated_at", "datetime('now')")],
        "sample",
        OnConflict::Replace,
        bind_sample,
    );

    fn database() -> Database {
        let db = Database::in_memory().expect("in-memory database");
        db.execute_batch("CREATE TABLE sample (id TEXT PRIMARY KEY, label TEXT, updated_at TEXT);")
            .expect("the sample table");
        db
    }

    // ------------------------------------------------------------------ cursor

    #[test]
    fn the_cursor_walks_columns_left_to_right() {
        let conn = Connection::open_in_memory().unwrap();
        let taken: (i64, i64, i64) = conn
            .query_row("SELECT 10, 20, 30", [], |row| {
                let mut cursor = RowCursor::new(row);
                Ok((cursor.take()?, cursor.take()?, cursor.take()?))
            })
            .unwrap();
        assert_eq!(taken, (10, 20, 30));
    }

    #[test]
    fn the_cursor_reports_how_far_it_got() {
        // `taken()` is what lets a generated reader be checked against its declared width.
        let conn = Connection::open_in_memory().unwrap();
        let width = conn
            .query_row("SELECT 1, 2", [], |row| {
                let mut cursor = RowCursor::new(row);
                let _: i64 = cursor.take()?;
                let _: i64 = cursor.take()?;
                Ok(cursor.taken())
            })
            .unwrap();
        assert_eq!(width, 2);
    }

    #[test]
    fn a_pair_codec_consumes_two_columns_not_one() {
        // The failure this guards: if `take_pair_via` advanced by one, every column after an
        // operator's name would shift by one and still type-check.
        let conn = Connection::open_in_memory().unwrap();
        let after = conn
            .query_row("SELECT 'Ahmed', 'أحمد', 99", [], |row| {
                let mut cursor = RowCursor::new(row);
                let name = cursor.take_pair_via(&column::OPERATOR_NAME)?;
                let trailing: i64 = cursor.take()?;
                assert_eq!(name.latin(), "Ahmed");
                Ok(trailing)
            })
            .unwrap();
        assert_eq!(
            after, 99,
            "the column after a two-column field must not shift"
        );
    }

    // ------------------------------------------------------------------ SQL

    #[test]
    fn the_select_list_is_the_projection_in_order() {
        assert_eq!(SAMPLE_READER.select_list(), "id, label");
        assert_eq!(SAMPLE_READER.select_list_qualified("p"), "p.id, p.label");
    }

    #[test]
    fn an_aggregate_projects_its_expression_and_answers_to_its_alias() {
        // The one shape where the two halves of an entry differ, and the reason `entries` is a
        // pair. Holding only the result name here would emit `SELECT gross_sales FROM …` against
        // a table that has no such column.
        //
        // The `AS` is **derived, not declared**. This test used to write it into the expression
        // by hand — `("COALESCE(…) AS gross_sales", "gross_sales")` — which spells the alias
        // twice and lets the two spellings disagree. `select_list` renders it now, so an entry
        // cannot project one name and read another.
        const TOTALS: RowReader<i64> =
            RowReader::new(&[("COALESCE(SUM(amount), 0)", "gross_sales")], |row| {
                row.get(0)
            });
        assert_eq!(
            TOTALS.select_list(),
            "COALESCE(SUM(amount), 0) AS gross_sales"
        );
        assert_eq!(TOTALS.column_names().collect::<Vec<_>>(), ["gross_sales"]);
    }

    #[test]
    fn insert_columns_are_derived_from_the_projection_plus_the_managed_ones() {
        assert_eq!(
            SAMPLE.insert_column_names().collect::<Vec<_>>(),
            ["id", "label", "updated_at"]
        );
    }

    #[test]
    fn a_store_managed_column_gets_sql_and_not_a_parameter() {
        let sql = SAMPLE.insert_statement();
        assert_eq!(
            sql,
            "INSERT OR REPLACE INTO sample (id, label, updated_at) \
             VALUES (?1, ?2, datetime('now'))"
        );
    }

    #[test]
    fn each_conflict_disposition_emits_its_own_statement() {
        // The four are not interchangeable: `Replace` deletes the conflicting row before
        // inserting, which cascade-deletes dependents and reassigns AUTOINCREMENT ids, and `Fail`
        // is what stops a re-run silently overwriting a finalised fiscal record.
        const READER: RowReader<Sample> =
            RowReader::new(&[("id", "id"), ("label", "label")], read_sample);

        const FAILS: RowMapping<Sample> =
            RowMapping::new(READER, &[], "sample", OnConflict::Fail, bind_sample);
        assert_eq!(
            FAILS.insert_statement(),
            "INSERT INTO sample (id, label) VALUES (?1, ?2)"
        );

        const UPDATES: RowMapping<Sample> = RowMapping::new(
            READER,
            &[],
            "sample",
            OnConflict::Update {
                key: &["id"],
                set: &["label"],
            },
            bind_sample,
        );
        assert_eq!(
            UPDATES.insert_statement(),
            "INSERT INTO sample (id, label) VALUES (?1, ?2) \
             ON CONFLICT(id) DO UPDATE SET label = excluded.label"
        );
    }

    #[test]
    fn a_multi_column_conflict_target_keeps_both_columns() {
        // `feature_screens` conflicts on `(feature_id, screen_id)`. A single-column target would
        // silently match the wrong row.
        const READER: RowReader<Sample> =
            RowReader::new(&[("id", "id"), ("label", "label")], read_sample);
        const M: RowMapping<Sample> = RowMapping::new(
            READER,
            &[],
            "sample",
            OnConflict::Update {
                key: &["a", "b"],
                set: &["label"],
            },
            bind_sample,
        );
        assert!(M.insert_statement().contains("ON CONFLICT(a, b) DO UPDATE"));
    }

    // ------------------------------------------------------------------ round trip

    #[test]
    fn a_row_survives_write_then_read() {
        let db = database();
        let value = Sample {
            id: "one".to_string(),
            label: Some("first".to_string()),
        };
        db.insert(&SAMPLE, &value).unwrap();
        let read = db
            .select_one(&SAMPLE_READER, "FROM sample WHERE id = ?1", params!["one"])
            .unwrap();
        assert_eq!(read, Some(value));
    }

    #[test]
    fn a_null_column_reaches_its_own_field() {
        let db = database();
        let value = Sample {
            id: "one".to_string(),
            label: None,
        };
        db.insert(&SAMPLE, &value).unwrap();
        let read = db
            .select_one(&SAMPLE_READER, "FROM sample WHERE id = ?1", params!["one"])
            .unwrap();
        assert_eq!(read.unwrap().label, None);
    }

    #[test]
    fn replace_overwrites_on_the_second_write_rather_than_erroring() {
        // Conflict disposition is invisible to a single write. Every round-trip test in this
        // issue writes the same key twice for this reason.
        let db = database();
        let first = Sample {
            id: "one".to_string(),
            label: Some("first".to_string()),
        };
        let second = Sample {
            id: "one".to_string(),
            label: Some("second".to_string()),
        };
        db.insert(&SAMPLE, &first).unwrap();
        db.insert(&SAMPLE, &second)
            .expect("OR REPLACE accepts the duplicate key");
        let read = db
            .select_one(&SAMPLE_READER, "FROM sample WHERE id = ?1", params!["one"])
            .unwrap();
        assert_eq!(read, Some(second));
        let rows: i64 = db.select_scalar("SELECT COUNT(*) FROM sample", []).unwrap();
        assert_eq!(rows, 1);
    }

    #[test]
    fn fail_refuses_the_second_write() {
        let db = database();
        const READER: RowReader<Sample> =
            RowReader::new(&[("id", "id"), ("label", "label")], read_sample);
        const FAILS: RowMapping<Sample> =
            RowMapping::new(READER, &[], "sample", OnConflict::Fail, bind_sample);

        let value = Sample {
            id: "one".to_string(),
            label: None,
        };
        db.insert(&FAILS, &value).unwrap();
        assert!(
            db.insert(&FAILS, &value).is_err(),
            "a plain INSERT must report the duplicate key rather than overwrite"
        );
    }

    #[test]
    fn update_keeps_the_row_and_overwrites_only_the_named_columns() {
        let db = database();
        db.execute_batch("INSERT INTO sample (id, label, updated_at) VALUES ('one', 'kept', 'x');")
            .unwrap();
        let before: i64 = db
            .select_scalar("SELECT rowid FROM sample WHERE id = 'one'", [])
            .unwrap();

        const READER: RowReader<Sample> =
            RowReader::new(&[("id", "id"), ("label", "label")], read_sample);
        const UPDATES: RowMapping<Sample> = RowMapping::new(
            READER,
            &[],
            "sample",
            OnConflict::Update {
                key: &["id"],
                set: &["label"],
            },
            bind_sample,
        );

        db.insert(
            &UPDATES,
            &Sample {
                id: "one".to_string(),
                label: Some("new".to_string()),
            },
        )
        .unwrap();

        let after: i64 = db
            .select_scalar("SELECT rowid FROM sample WHERE id = 'one'", [])
            .unwrap();
        assert_eq!(
            before, after,
            "DO UPDATE must keep the row's identity; OR REPLACE would reassign it"
        );
    }

    // ------------------------------------------------------------------ readers

    #[test]
    fn read_all_returns_every_row_in_query_order() {
        let db = database();
        for id in ["a", "b", "c"] {
            db.insert(
                &SAMPLE,
                &Sample {
                    id: id.to_string(),
                    label: None,
                },
            )
            .unwrap();
        }
        let rows = db
            .select_all(&SAMPLE_READER, "FROM sample ORDER BY id", [])
            .unwrap();
        assert_eq!(
            rows.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
            ["a", "b", "c"]
        );
    }

    #[test]
    fn select_one_is_none_rather_than_an_error_when_nothing_matches() {
        let db = database();
        let read = db
            .select_one(
                &SAMPLE_READER,
                "FROM sample WHERE id = ?1",
                params!["absent"],
            )
            .unwrap();
        assert!(read.is_none());
    }

    #[test]
    fn the_qualified_reader_works_against_an_aliased_table() {
        // The FTS5 product search is the one query that needs this.
        let db = database();
        db.insert(
            &SAMPLE,
            &Sample {
                id: "one".to_string(),
                label: None,
            },
        )
        .unwrap();
        let conn = db.connection();
        let conn = conn.lock();
        let rows = read_all_qualified(&conn, &SAMPLE_READER, "s", "FROM sample s", []).unwrap();
        assert_eq!(rows.len(), 1);
    }

    #[test]
    fn the_free_readers_work_against_a_lock_the_caller_already_holds() {
        // The reason they exist: `parking_lot::Mutex` is not reentrant, so a `&self` method here
        // would deadlock with no error and no panic. This test is the shape of the real call
        // sites in `z_reports::get_day_totals`, `return_service` and `offline_service`.
        let db = database();
        db.insert(
            &SAMPLE,
            &Sample {
                id: "one".to_string(),
                label: None,
            },
        )
        .unwrap();

        let conn = db.connection();
        let conn = conn.lock();
        let count: i64 = scalar(&conn, "SELECT COUNT(*) FROM sample", []).unwrap();
        let rows = read_all(&conn, &SAMPLE_READER, "FROM sample", []).unwrap();
        let one = read_one(
            &conn,
            &SAMPLE_READER,
            "FROM sample WHERE id = ?1",
            params!["one"],
        )
        .unwrap();
        assert_eq!(count, 1);
        assert_eq!(rows.len(), 1);
        assert!(one.is_some());
    }

    #[test]
    fn a_bind_failure_reaches_the_caller_instead_of_writing_a_wrong_value() {
        // `ColumnCodec::write` is fallible so that a value which cannot be rendered is reported
        // rather than written as NULL. Nothing here can be written wrong silently.
        fn refuses(_: &Sample) -> SqliteResult<Vec<Value>> {
            Err(rusqlite::Error::ToSqlConversionFailure(Box::new(
                std::fmt::Error,
            )))
        }
        const READER: RowReader<Sample> =
            RowReader::new(&[("id", "id"), ("label", "label")], read_sample);
        const BROKEN: RowMapping<Sample> =
            RowMapping::new(READER, &[], "sample", OnConflict::Fail, refuses);

        let db = database();
        let result = db.insert(
            &BROKEN,
            &Sample {
                id: "one".to_string(),
                label: None,
            },
        );
        assert!(result.is_err());
        let rows: i64 = db.select_scalar("SELECT COUNT(*) FROM sample", []).unwrap();
        assert_eq!(rows, 0, "a refused bind must write nothing");
    }

    #[test]
    #[should_panic(expected = "but the shape declares")]
    fn a_query_whose_columns_disagree_with_the_shape_is_caught_under_debug() {
        // Checking, not constraint — and documented as such. It fires on every query the suite
        // exercises, which is why it is worth the debug-only cost.
        let db = database();
        db.insert(
            &SAMPLE,
            &Sample {
                id: "one".to_string(),
                label: None,
            },
        )
        .unwrap();
        // `label, id` against a shape declaring `id, label`.
        let conn = db.connection();
        let conn = conn.lock();
        let mut statement = conn.prepare("SELECT label, id FROM sample").unwrap();
        // Consumed, not merely constructed: `query_map` is lazy, so a discarded iterator never
        // calls the closure and this test would pass without ever performing the read it is about.
        let _ = statement
            .query_map([], |row| SAMPLE_READER.read(row))
            .unwrap()
            .collect::<Vec<_>>();
    }

    // ------------------------------------------------------------------ the declaration macro
    //
    // `OPERATOR_ROW` in `operators.rs` exercises four of the seven entry shapes — bare, `via`,
    // the pair, and `from "col" via` — plus `managed`. The two it cannot reach are here: a column
    // whose name differs from the field with no codec, and the aggregate arm, which needs a shape
    // with no table behind it.

    row_mapping! {
        /// The same shape as the hand-written [`SAMPLE`] above, declared instead of assembled.
        const SAMPLE_BY_MACRO: RowMapping<Sample> = for "sample" {
            id,
            label via column::OPTIONAL_TEXT,
            managed "updated_at" = "datetime('now')",
        } on_conflict OnConflict::Replace;
    }

    #[derive(Debug, PartialEq, Eq)]
    struct Renamed {
        key: String,
        caption: Option<String>,
    }

    row_mapping! {
        /// Two fields, neither named after its column. Nothing here is checked by the compiler —
        /// that is the point of declaring it.
        const RENAMED: RowMapping<Renamed> = for "sample" {
            key from "id",
            caption from "label" via column::OPTIONAL_TEXT,
            managed "updated_at" = "datetime('now')",
        } on_conflict OnConflict::Fail;
    }

    #[derive(Debug, PartialEq, Eq)]
    struct SampleTotals {
        rows: i64,
        last_id: Option<String>,
    }

    row_reader! {
        /// An aggregate: every column is an expression, and there is no row to write back.
        const SAMPLE_TOTALS: RowReader<SampleTotals> = {
            rows from ("COUNT(*)" as "rows"),
            last_id from ("MAX(id)" as "last_id") via column::OPTIONAL_TEXT,
        };
    }

    #[test]
    fn the_macro_reproduces_a_hand_written_mapping_exactly() {
        // The control for every other test in this section. If the macro's four products can
        // diverge from the four lists a human would write, they diverge here first.
        assert_eq!(
            SAMPLE_BY_MACRO.reader().select_list(),
            SAMPLE.reader().select_list()
        );
        assert_eq!(
            SAMPLE_BY_MACRO.insert_statement(),
            SAMPLE.insert_statement()
        );

        let db = database();
        let value = Sample {
            id: "id-value".to_string(),
            label: Some("label-value".to_string()),
        };
        db.insert(&SAMPLE_BY_MACRO, &value).unwrap();

        // Read the row back through the *hand-written* reader: the macro wrote it, the human's
        // code reads it, so an agreement here is not the macro agreeing with itself.
        let read = db
            .select_one(SAMPLE.reader(), "FROM sample WHERE id = ?1", ["id-value"])
            .unwrap()
            .expect("the row the macro's mapping wrote");
        assert_eq!(read, value);
    }

    #[test]
    fn a_field_reads_and_writes_the_column_its_from_clause_names_not_its_own() {
        let db = database();
        let value = Renamed {
            key: "key-value".to_string(),
            caption: Some("caption-value".to_string()),
        };
        db.insert(&RENAMED, &value).unwrap();

        assert_eq!(
            RENAMED.insert_statement(),
            "INSERT INTO sample (id, label, updated_at) VALUES (?1, ?2, datetime('now'))"
        );

        // By name, in a query the declaration had no hand in: `key` reached `id`, not `label`.
        let conn = db.connection();
        let conn = conn.lock();
        let id: String = conn
            .query_row("SELECT id FROM sample", [], |row| row.get(0))
            .unwrap();
        let label: String = conn
            .query_row("SELECT label FROM sample", [], |row| row.get(0))
            .unwrap();
        assert_eq!(id, "key-value");
        assert_eq!(label, "caption-value");

        let read = read_one(
            &conn,
            RENAMED.reader(),
            "FROM sample WHERE id = ?1",
            ["key-value"],
        )
        .unwrap()
        .expect("the row this test just wrote");
        assert_eq!(read, value);
    }

    #[test]
    fn an_aggregate_shape_selects_its_expressions_under_the_names_it_declares() {
        // The `as` in the entry is not decoration: without it the result column is named
        // `COUNT(*)`, and the debug-only name check in `RowReader::read` compares against the
        // declared name. This asserts the rendered SQL, then runs it under the same check.
        assert_eq!(
            SAMPLE_TOTALS.select_list(),
            "COUNT(*) AS rows, MAX(id) AS last_id"
        );

        let db = database();
        for id in ["a-id", "b-id"] {
            db.insert(
                &SAMPLE,
                &Sample {
                    id: id.to_string(),
                    label: None,
                },
            )
            .unwrap();
        }

        let totals = db
            .select_one(&SAMPLE_TOTALS, "FROM sample", [])
            .unwrap()
            .expect("an aggregate always returns a row");
        assert_eq!(
            totals,
            SampleTotals {
                rows: 2,
                last_id: Some("b-id".to_string()),
            }
        );
    }

    #[test]
    fn an_aggregate_over_no_rows_still_answers_and_says_so() {
        // `COUNT(*)` is 0 and `MAX(id)` is NULL. A shape that read the empty case as "no row"
        // would be the silent-zero defect, and this is the shape most likely to grow one.
        let db = database();
        let totals = db
            .select_one(&SAMPLE_TOTALS, "FROM sample", [])
            .unwrap()
            .expect("an aggregate always returns a row");
        assert_eq!(
            totals,
            SampleTotals {
                rows: 0,
                last_id: None,
            }
        );
    }

    #[test]
    fn a_managed_column_is_written_by_the_store_and_absent_from_the_projection() {
        let db = database();
        db.insert(
            &SAMPLE_BY_MACRO,
            &Sample {
                id: "id-value".to_string(),
                label: None,
            },
        )
        .unwrap();

        assert!(
            !SAMPLE_BY_MACRO
                .reader()
                .column_names()
                .any(|name| name == "updated_at"),
            "a managed column must not be readable"
        );
        assert!(SAMPLE_BY_MACRO
            .insert_column_names()
            .any(|name| name == "updated_at"));

        let stamped: Option<String> = db
            .select_scalar("SELECT updated_at FROM sample", [])
            .unwrap();
        assert!(
            stamped.is_some_and(|value| !value.is_empty()),
            "the store's expression did not run"
        );
    }

    #[test]
    fn a_declared_mappings_conflict_disposition_is_the_one_it_names() {
        // `RENAMED` declares `Fail` where the other two declare `Replace`, so this reads
        // differently for the two dispositions rather than confirming a constant.
        let db = database();
        let value = Renamed {
            key: "key-value".to_string(),
            caption: None,
        };
        db.insert(&RENAMED, &value).unwrap();
        assert!(
            db.insert(&RENAMED, &value).is_err(),
            "`OnConflict::Fail` accepted a second write"
        );

        db.execute("DELETE FROM sample", &[]).unwrap();
        let replaceable = Sample {
            id: "key-value".to_string(),
            label: None,
        };
        db.insert(&SAMPLE_BY_MACRO, &replaceable).unwrap();
        db.insert(&SAMPLE_BY_MACRO, &replaceable)
            .expect("`OnConflict::Replace` refused a second write");
    }

    // ------------------------------------------------------------------ the doc comment itself

    /// The lines of `row_mapping!`'s worked expansion, with the `///` stripped.
    ///
    /// `include_str!` on the file this test lives in. The doc block is prose to `rustc` and
    /// invisible to every other check in this crate, which is exactly the problem: it is the only
    /// mitigation for twenty readers that `grep` and `symbol` cannot see, and a mitigation nothing
    /// verifies is a comment.
    fn the_worked_expansion() -> Vec<String> {
        const SOURCE: &str = include_str!("projection.rs");
        let opening = "/// pub const OPERATOR_ROW: RowMapping<OperatorRow> = {";
        let start = SOURCE
            .find(opening)
            .expect("the worked expansion has been renamed or removed");
        let block = &SOURCE[start..];
        let end = block
            .find("/// ```")
            .expect("the worked expansion's code fence is gone");
        block[..end]
            .lines()
            .map(|line| {
                line.trim_start()
                    .trim_start_matches("///")
                    .trim()
                    .to_string()
            })
            .collect()
    }

    #[test]
    fn the_worked_expansion_lists_the_columns_the_operator_mapping_actually_projects() {
        // Measured against a real subagent finding: the block used to show `code` followed
        // directly by `name`, deleting `employee_id` and `employee_number` from the middle with
        // no marker. Adjacency was wrong, not merely incomplete — and a reader trusting it got a
        // wrong picture of the generated code, which is worse than no picture.
        let documented: Vec<String> = the_worked_expansion()
            .iter()
            .filter_map(|line| {
                let inner = line.strip_prefix("(\"")?;
                let (name, rest) = inner.split_once('"')?;
                // Only the projection tuples, whose two halves are the same string. The managed
                // array's `("updated_at", "datetime('now')")` is not one and must not be counted.
                rest.contains(&format!("\"{name}\""))
                    .then(|| name.to_string())
            })
            .collect();

        let actual: Vec<String> = crate::operators::OPERATOR_ROW
            .reader()
            .column_names()
            .map(str::to_string)
            .collect();
        assert_eq!(
            documented, actual,
            "the worked expansion and `OPERATOR_ROW` project different columns"
        );
    }

    #[test]
    fn the_worked_expansion_binds_one_local_per_declared_field_not_one_per_column() {
        // The other half of the same finding: the block claimed "one `let` per entry", and this
        // file uses "entry" for a column. The pair field is one `let` over two of them, so the
        // two counts differ by exactly one here — which is the fact the claim erased.
        let lets = the_worked_expansion()
            .iter()
            .filter(|line| line.starts_with("let ") && line.contains("cursor."))
            .count();
        let entries = crate::operators::OPERATOR_ROW.reader().width();
        assert_eq!(lets, 10, "one `let` per declared field");
        assert_eq!(entries, 11, "one entry per column");
        assert_eq!(
            entries - lets,
            1,
            "the pair column is the only place the two counts diverge"
        );
    }
}
