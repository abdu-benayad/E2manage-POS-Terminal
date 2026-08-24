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
            .map(|(expression, _)| *expression)
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// The `SELECT` list with every column qualified by a table alias: `"p.id, p.sku, …"`.
    ///
    /// Only meaningful where every entry is a bare column name, which is every shape except the
    /// aggregate one — an alias in front of `COALESCE(…)` is not SQL. Nothing qualifies an
    /// aggregate, so this is a documented precondition rather than a check.
    pub fn select_list_qualified(&self, alias: &str) -> String {
        self.entries
            .iter()
            .map(|(expression, _)| format!("{alias}.{expression}"))
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
pub fn read_one<T>(
    conn: &Connection,
    shape: &RowReader<T>,
    table_expression: &str,
    params: impl Params,
) -> SqliteResult<Option<T>> {
    let sql = format!("SELECT {} {table_expression}", shape.select_list());
    conn.query_row(&sql, params, |row| shape.read(row))
        .optional()
}

/// Reads every row of `shape`.
pub fn read_all<T>(
    conn: &Connection,
    shape: &RowReader<T>,
    table_expression: &str,
    params: impl Params,
) -> SqliteResult<Vec<T>> {
    let sql = format!("SELECT {} {table_expression}", shape.select_list());
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
    table_expression: &str,
    params: impl Params,
) -> SqliteResult<Vec<T>> {
    let sql = format!(
        "SELECT {} {table_expression}",
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
        table_expression: &str,
        params: impl Params,
    ) -> SqliteResult<Option<T>> {
        let conn = self.connection();
        let conn = conn.lock();
        read_one(&conn, shape, table_expression, params)
    }

    /// Reads every row of `shape`. See [`Database::select_one`] on the lock.
    pub fn select_all<T>(
        &self,
        shape: &RowReader<T>,
        table_expression: &str,
        params: impl Params,
    ) -> SqliteResult<Vec<T>> {
        let conn = self.connection();
        let conn = conn.lock();
        read_all(&conn, shape, table_expression, params)
    }

    /// Reads every row of `shape` with each column qualified. See the lock note above.
    pub fn select_all_qualified<T>(
        &self,
        shape: &RowReader<T>,
        alias: &str,
        table_expression: &str,
        params: impl Params,
    ) -> SqliteResult<Vec<T>> {
        let conn = self.connection();
        let conn = conn.lock();
        read_all_qualified(&conn, shape, alias, table_expression, params)
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
        const TOTALS: RowReader<i64> = RowReader::new(
            &[("COALESCE(SUM(amount), 0) AS gross_sales", "gross_sales")],
            |row| row.get(0),
        );
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
}
