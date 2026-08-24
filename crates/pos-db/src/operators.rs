//! Operators Repository
//!
//! Handles operator (cashier) data storage and retrieval.

use pos_models::{OperatorId, OperatorName, OperatorPermissions, OperatorRole};

use crate::column;
use crate::projection::OnConflict;
use crate::{row_mapping, row_reader};
use rusqlite::{params, Result as SqliteResult};

use super::Database;

/// Operator row from database (HR Employee integrated)
///
/// **Deliberately not `Serialize`/`Deserialize`.** Nothing serialises this type — the store reads
/// and writes it column by column — and `OperatorName` has no serde by design, because neither the
/// wire nor the store nests the two scripts. Deriving here would have forced a nested shape on
/// `OperatorName` to satisfy a trait no caller uses.
#[derive(Debug, Clone)]
pub struct OperatorRow {
    /// POS Operator Profile ID
    pub id: OperatorId,
    /// Operator code (for quick lookup)
    pub code: String,
    /// HR Employee ID
    pub employee_id: Option<String>,
    /// HR Employee Number (e.g., "EMP001")
    pub employee_number: Option<String>,
    /// The operator's name, in both scripts the store keeps.
    ///
    /// One field, not two. The `name` and `name_ar` columns are one value — a row with a blank
    /// English name and a present Arabic one is not an operator with half a name, it is a row
    /// that should never have been written. This is the only place in the workspace holding both
    /// scripts, so it is the only place that can answer an Arabic-locale screen.
    pub name: OperatorName,
    /// POS role, as the server's `POS_OperatorRole` enum defines it.
    pub role: OperatorRole,
    /// HR Department name
    pub department: Option<String>,
    /// HR Position/Job title
    pub position: Option<String>,
    /// What the operator is allowed to do, as synced from the platform.
    ///
    /// Stored in the `permissions_json` `TEXT` column, still under that name, and mapped by
    /// `pos_models::OperatorPermissions` — the one place in the workspace that spells the
    /// server's permission keys. Two crates cannot drift from a mapping neither of them defines.
    pub permissions: Option<OperatorPermissions>,
    /// Whether employee is active in HR
    pub is_active: bool,
}

row_mapping! {
    /// Every column of `operators` this till reads or writes, declared once.
    ///
    /// The four `SELECT` lists in this file, the two `INSERT` lists, and the four reader closures
    /// were six hand-maintained orderings of the same eleven columns. They are one now, and task 04
    /// deletes the copies.
    ///
    /// `avatar_url` is in the table and not here. It is one of the two columns schema v11's comment
    /// records as abandoned ("we'll just stop using them: code, avatar_url"), and a mapping names
    /// what the till uses, not what the table has. `code` is still read, so it stayed.
    ///
    /// `updated_at` is `managed`: the store writes `datetime('now')` and nothing reads it back.
    /// Declaring it that way is what keeps it out of the projection — a `managed` column cannot be
    /// read, because `OperatorRow` has no field for it and there is no rest-init arm to hide that.
    pub const OPERATOR_ROW: RowMapping<OperatorRow> = for "operators" {
        id                                  via column::OPERATOR_ID,
        code,
        employee_id,
        employee_number,
        name from ("name", "name_ar")       via column::OPERATOR_NAME,
        role                                via column::OPERATOR_ROLE,
        department,
        position,
        permissions from "permissions_json" via column::PERMISSIONS,
        is_active,
        managed "updated_at" = "datetime('now')",
    } on_conflict OnConflict::Replace;
}

// There is deliberately no `impl Default for OperatorRow`. It used to produce an operator whose
// id and name were both `String::new()` — a record belonging to nobody, which every
// `..Default::default()` then inherited and no reader could distinguish from a real one. Typing
// the two fields made the impl unwritable without an explicit sentinel, which is the type model
// stating the same objection. The three shift defaults task 09 met were deleted for this reason
// and this is the fourth. Construct the row's fields; there are twelve and they all mean
// something.

/// The five columns of `operators` the offline PIN path reads.
///
/// A deliberately different shape from [`OperatorRow`], not a subset of it by accident: this is
/// what the till needs in order to decide whether an operator may sign in — their name for the
/// message, their role and permissions for what they may do, and whether they are active at all.
/// It is emphatically **not** an operator record, and five columns could not be written back as
/// one, so it is a [`RowReader`](crate::projection::RowReader) and `write` will not take it.
///
/// There is no `pin_hash` here and there is none in the table: schema v13 took it off this till.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperatorCredentialsRow {
    pub name: OperatorName,
    pub role: OperatorRole,
    pub permissions_json: Option<String>,
    pub is_active: bool,
}

row_reader! {
    /// What the offline sign-in path reads about an operator, declared once.
    ///
    /// **`from "operators"` rather than a bare reader**, and the distinction earns its keep here.
    /// A read-only shape and a shape with no table are different things: this one projects a real
    /// table, so `every_mapping_names_columns_the_schema_has` must verify these five names against
    /// the schema. Declared table-less it would have been exempted from that check as an
    /// "aggregate" — and a five-column projection of a table nobody checks is exactly the shape
    /// whose names most need checking.
    pub const OPERATOR_CREDENTIALS_ROW: RowReader<OperatorCredentialsRow> = from "operators" {
        name from ("name", "name_ar")       via column::OPERATOR_NAME,
        role                                via column::OPERATOR_ROLE,
        permissions_json from "permissions_json",
        is_active,
    };
}

impl Database {
    /// Saves or updates an operator.
    pub fn save_operator(&self, operator: &OperatorRow) -> SqliteResult<()> {
        self.insert(&OPERATOR_ROW, operator)?;
        Ok(())
    }

    /// Saves every operator in one transaction.
    ///
    /// The count is rows changed, which for `OnConflict::Replace` is one per operator. The hand
    /// written version counted iterations and committed whatever had gone in before a failure;
    /// [`crate::projection::write_all`] rolls the batch back instead, so a catalogue sync that
    /// fails half way does not leave the till holding half a staff list.
    pub fn save_operators(&self, operators: &[OperatorRow]) -> SqliteResult<usize> {
        self.insert_all(&OPERATOR_ROW, operators)
    }

    /// Gets all active operators, in name order.
    pub fn get_operators(&self) -> SqliteResult<Vec<OperatorRow>> {
        self.select_all(
            OPERATOR_ROW.reader(),
            "FROM operators WHERE is_active = 1 ORDER BY name",
            [],
        )
    }

    /// Gets an operator by ID.
    pub fn get_operator_by_id(&self, id: &OperatorId) -> SqliteResult<Option<OperatorRow>> {
        self.select_one(
            OPERATOR_ROW.reader(),
            "FROM operators WHERE id = ?1",
            [id.as_str()],
        )
    }

    /// Gets an active operator by employee number.
    pub fn get_operator_by_employee_number(
        &self,
        employee_number: &str,
    ) -> SqliteResult<Option<OperatorRow>> {
        self.select_one(
            OPERATOR_ROW.reader(),
            "FROM operators WHERE employee_number = ?1 AND is_active = 1",
            [employee_number],
        )
    }

    /// Searches active operators by either name or employee number.
    pub fn search_operators(&self, query: &str, limit: i32) -> SqliteResult<Vec<OperatorRow>> {
        let search = format!("%{query}%");
        self.select_all(
            OPERATOR_ROW.reader(),
            "FROM operators
             WHERE is_active = 1
               AND (name LIKE ?1 OR name_ar LIKE ?1 OR employee_number LIKE ?1)
             ORDER BY name
             LIMIT ?2",
            params![search, limit],
        )
    }

    /// Gets the total operator count
    pub fn get_operator_count(&self) -> SqliteResult<i64> {
        self.select_scalar("SELECT COUNT(*) FROM operators WHERE is_active = 1", [])
    }

    /// Deactivates all operators (for full sync)
    pub fn deactivate_all_operators(&self) -> SqliteResult<usize> {
        self.execute("UPDATE operators SET is_active = 0", &[])
    }

    /// Deletes an operator by ID
    pub fn delete_operator(&self, id: &OperatorId) -> SqliteResult<bool> {
        let deleted = self.execute("DELETE FROM operators WHERE id = ?1", &[&id.as_str()])?;
        Ok(deleted > 0)
    }
}

// `display_name(prefer_ar: bool)` and `initials()` are gone from this type; they are
// `OperatorName::in_script(NameScript)` and `OperatorName::initials(NameScript)`. They were never
// about the row — a name renders the same whether it came from the store or the wire — and the
// boolean parameter was the unmarked socket `NameScript` replaced.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::migrations::run_migrations;
    use pos_models::{DiscountAuthority, DiscountPercent, NameScript, Permission};
    use rust_decimal::Decimal;

    fn setup_db() -> Database {
        let db = Database::in_memory().unwrap();
        {
            let conn = db.connection();
            let conn = conn.lock();
            run_migrations(&conn).unwrap();
        }
        db
    }

    /// A fixture operator, written out once.
    ///
    /// `OperatorRow` has no `Default` — the one it had produced a record belonging to nobody — so
    /// the tests below update from a real operator with `..an_operator(…)` instead.
    fn an_operator(id: &str, latin: &str, arabic: Option<&str>) -> OperatorRow {
        OperatorRow {
            id: OperatorId::new(id).unwrap(),
            code: format!("C{id}"),
            employee_id: None,
            employee_number: None,
            name: OperatorName::new(latin, arabic).unwrap(),
            role: OperatorRole::Cashier,
            department: None,
            position: None,
            permissions: None,
            is_active: true,
        }
    }

    #[test]
    fn test_save_and_get_operator() {
        let db = setup_db();

        let operator = an_operator("op-1", "Ahmed Hassan", Some("أحمد حسن"));

        db.save_operator(&operator).unwrap();

        let found = db
            .get_operator_by_id(&OperatorId::new("op-1").unwrap())
            .unwrap()
            .expect("the operator was saved");
        assert_eq!(found.name.latin(), "Ahmed Hassan");
        assert_eq!(found.name.arabic(), Some("أحمد حسن"));
    }

    #[test]
    fn test_get_operator_by_code() {
        let db = setup_db();

        let operator = OperatorRow {
            code: "C001".to_string(),
            ..an_operator("op-1", "Ahmed Hassan", None)
        };

        db.save_operator(&operator).unwrap();

        let found = db
            .get_operator_by_id(&OperatorId::new("op-1").unwrap())
            .unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().code, "C001");
    }

    #[test]
    fn test_search_operators() {
        let db = setup_db();

        let operators = vec![
            an_operator("op-1", "Ahmed Hassan", None),
            an_operator("op-2", "Sara Ahmed", None),
        ];

        for op in &operators {
            db.save_operator(op).unwrap();
        }

        let results = db.search_operators("Ahmed", 10).unwrap();
        assert_eq!(results.len(), 2);

        let results = db.search_operators("Sara", 10).unwrap();
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_operator_initials() {
        // Initials belong to the name, not to the row: `OperatorName::initials(NameScript)`.
        let operator = an_operator("op-1", "Ahmed Hassan", Some("أحمد حسن"));
        assert_eq!(operator.name.initials(NameScript::Latin), "AH");
        assert_eq!(operator.name.initials(NameScript::Arabic), "أح");

        let operator = an_operator("op-2", "Sara", None);
        assert_eq!(operator.name.initials(NameScript::Latin), "S");
        // No Arabic name synced, so the Arabic script falls back to the Latin one, as
        // `in_script` does. The absence is still reported by `arabic()`.
        assert_eq!(operator.name.initials(NameScript::Arabic), "S");
    }

    #[test]
    fn test_operator_permissions_round_trip_through_the_column() {
        // This test used to feed a **snake_case** literal — a shape the server has never sent —
        // through `OperatorRow::permissions()`, whose `.ok().unwrap_or_default()` meant it would
        // have passed for a camelCase payload too, with every permission silently false. It now
        // exercises the real path: `pos_models::OperatorPermissions` in, the column out, the
        // same value back.
        let db = setup_db();
        let permissions = OperatorPermissions::new(
            [Permission::VoidTransaction, Permission::ViewReports],
            DiscountAuthority::UpTo(DiscountPercent::new(Decimal::from(10)).unwrap()),
        );

        let operator = OperatorRow {
            permissions: Some(permissions.clone()),
            ..an_operator("op-1", "Ahmed Hassan", None)
        };
        db.save_operator(&operator).unwrap();

        let stored = db
            .get_operator_by_id(&OperatorId::new("op-1").unwrap())
            .unwrap()
            .expect("the operator was saved");

        assert_eq!(stored.permissions, Some(permissions));
    }

    #[test]
    fn test_operator_permissions_column_that_will_not_parse_is_a_read_failure() {
        // Not an operator with no privileges. `.ok().unwrap_or_default()` made those two the same
        // value, which is why the camelCase/snake_case drift went unnoticed for as long as it did.
        let db = setup_db();
        db.execute(
            "INSERT INTO operators (id, code, name, role, permissions_json, is_active) \
             VALUES ('op-1', 'C001', 'Ahmed', 'CASHIER', '{\"canVoid\": ', 1)",
            &[],
        )
        .unwrap();

        assert!(db
            .get_operator_by_id(&OperatorId::new("op-1").unwrap())
            .is_err());
    }

    #[test]
    fn an_operator_row_with_a_blank_name_is_a_read_failure() {
        // The column pair is one value, and the domain refuses a blank one. Before `OperatorName`
        // this row read back as an operator whose name was the empty string — which every screen
        // would have rendered as a nameless cashier rather than as the corrupt row it is.
        let db = setup_db();
        db.execute(
            "INSERT INTO operators (id, code, name, name_ar, role, is_active) \
             VALUES ('op-1', 'C001', '', 'أحمد', 'CASHIER', 1)",
            &[],
        )
        .unwrap();

        assert!(db
            .get_operator_by_id(&OperatorId::new("op-1").unwrap())
            .is_err());
    }

    // ------------------------------------------------------------------------------------------
    // `OPERATOR_ROW` — the declaration, held to `df4e089`'s discipline: a distinct value in every
    // column, every field asserted, a NULL pass, and each test mutation-verified. The call sites
    // above still read positionally; task 04 moves them.
    // ------------------------------------------------------------------------------------------

    /// An operator whose every column holds a value found nowhere else in the row.
    ///
    /// This is the point of the fixture. `an_operator` above leaves six columns `None` and two
    /// equal to each other, so a swap between any two of them reads identically — which is the
    /// defect this whole task is about, sitting inside the test that would have to catch it.
    fn an_operator_with_no_two_columns_alike() -> OperatorRow {
        OperatorRow {
            id: OperatorId::new("id-column").unwrap(),
            code: "code-column".to_string(),
            employee_id: Some("employee-id-column".to_string()),
            employee_number: Some("employee-number-column".to_string()),
            name: OperatorName::new("name-column", Some("name-ar-column")).unwrap(),
            // Not `Cashier`: that is the column's SQL `DEFAULT`, so a role that never reached the
            // store would read back as one that did.
            role: OperatorRole::Manager,
            department: Some("department-column".to_string()),
            position: Some("position-column".to_string()),
            permissions: Some(OperatorPermissions::new(
                [Permission::VoidTransaction],
                DiscountAuthority::UpTo(DiscountPercent::new(Decimal::from(37)).unwrap()),
            )),
            // Not `true`: that is the column's SQL `DEFAULT`, same reasoning as `role`.
            is_active: false,
        }
    }

    #[test]
    fn the_operator_mapping_names_every_column_it_writes_in_the_order_it_reads_them() {
        assert_eq!(
            OPERATOR_ROW.reader().select_list(),
            "id, code, employee_id, employee_number, name, name_ar, role, department, position, \
             permissions_json, is_active"
        );
        assert_eq!(
            OPERATOR_ROW.insert_statement(),
            "INSERT OR REPLACE INTO operators (id, code, employee_id, employee_number, name, \
             name_ar, role, department, position, permissions_json, is_active, updated_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, datetime('now'))"
        );
        // The pair column contributes two of those eleven, which is the arm a single-column
        // reading of `width()` would miss.
        assert_eq!(OPERATOR_ROW.reader().width(), 11);
    }

    #[test]
    fn every_column_of_a_fully_distinct_operator_survives_the_round_trip() {
        let db = setup_db();
        let written = an_operator_with_no_two_columns_alike();

        assert_eq!(db.insert(&OPERATOR_ROW, &written).unwrap(), 1);

        let read = db
            .select_one(
                OPERATOR_ROW.reader(),
                "FROM operators WHERE id = ?1",
                ["id-column"],
            )
            .unwrap()
            .expect("the row this test just wrote");

        // Every field, not a sample. The guard this replaces asserted three of them.
        assert_eq!(read.id, written.id);
        assert_eq!(read.code, written.code);
        assert_eq!(read.employee_id, written.employee_id);
        assert_eq!(read.employee_number, written.employee_number);
        assert_eq!(read.name.latin(), written.name.latin());
        assert_eq!(read.name.arabic(), written.name.arabic());
        assert_eq!(read.role, written.role);
        assert_eq!(read.department, written.department);
        assert_eq!(read.position, written.position);
        assert_eq!(read.permissions, written.permissions);
        assert_eq!(read.is_active, written.is_active);
    }

    /// Asserts that every column of the single stored row holds the value belonging to it.
    ///
    /// A round trip is symmetric: swap two columns in the declaration and it still passes, because
    /// the write and the read swap together. That is exactly the defect. This is the asymmetric
    /// half — read back **by name**, in queries the declaration had no hand in — and there is one
    /// caller of it per **writer**, because a writer is a hand-maintained path and two of them can
    /// disagree.
    ///
    /// Measured, not asserted: with `department from "position"` and `position from "department"`
    /// in the declaration, `every_column_of_a_fully_distinct_operator_survives_the_round_trip`
    /// stays green and this fails.
    fn assert_every_column_holds_its_own_value(db: &Database) {
        let conn = db.connection();
        let conn = conn.lock();

        // Every text column and the exact bytes that belong in it. One table rather than one
        // binding per column: a `let role: String` here is an operator's identity carried as a
        // bare string, which `operator_identity_never_survives_as_a_bare_string` refuses — and it
        // is right to, so the shape changed instead of the name.
        let manager = OperatorRole::Manager.as_wire_str();
        for (column, expected) in [
            ("id", "id-column"),
            ("code", "code-column"),
            ("employee_id", "employee-id-column"),
            ("employee_number", "employee-number-column"),
            ("name", "name-column"),
            ("name_ar", "name-ar-column"),
            ("role", manager),
            ("department", "department-column"),
            ("position", "position-column"),
        ] {
            let matched: bool = conn
                .query_row(
                    &format!("SELECT {column} = ?1 FROM operators"),
                    [expected],
                    |row| row.get(0),
                )
                .unwrap();
            assert!(matched, "the `{column}` column does not hold `{expected}`");
        }

        // The two whose stored type is not text.
        let is_active: i64 = conn
            .query_row("SELECT is_active FROM operators", [], |row| row.get(0))
            .unwrap();
        assert_eq!(is_active, 0);
        let permissions_carries_the_authority: bool = conn
            .query_row(
                "SELECT permissions_json LIKE '%37%' FROM operators",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(
            permissions_carries_the_authority,
            "the discount authority did not reach `permissions_json`"
        );
    }

    #[test]
    fn save_operator_puts_each_value_in_the_column_that_carries_its_name() {
        let db = setup_db();
        db.save_operator(&an_operator_with_no_two_columns_alike())
            .unwrap();
        assert_every_column_holds_its_own_value(&db);
    }

    #[test]
    fn save_operators_puts_each_value_in_the_column_that_carries_its_name() {
        // The second writer. It shares a mapping with the first now, but it did not before this
        // task and nothing in the type system stops it from diverging again — a bulk path is where
        // a hand-written column list historically grew back.
        let db = setup_db();
        assert_eq!(
            db.save_operators(&[an_operator_with_no_two_columns_alike()])
                .unwrap(),
            1
        );
        assert_every_column_holds_its_own_value(&db);
    }

    #[test]
    fn a_second_write_of_the_same_id_replaces_the_row_rather_than_failing_or_duplicating() {
        // A conflict disposition is invisible on the first write. `OPERATOR_ROW` declares
        // `Replace` because both hand-written inserts said `INSERT OR REPLACE`, and a catalogue
        // sync re-sends every operator it knows — under `Fail` the second sync would error, and
        // under no conflict clause at all a `PRIMARY KEY` violation would.
        let db = setup_db();
        let first = an_operator_with_no_two_columns_alike();
        db.save_operator(&first).unwrap();

        let renamed = OperatorRow {
            name: OperatorName::new("second-write", None::<&str>).unwrap(),
            ..first
        };
        db.save_operator(&renamed).unwrap();

        let rows: i64 = db
            .select_scalar("SELECT COUNT(*) FROM operators", [])
            .unwrap();
        assert_eq!(rows, 1);

        let stored = db
            .get_operator_by_id(&OperatorId::new("id-column").unwrap())
            .unwrap()
            .expect("the replaced row");
        assert_eq!(stored.name.latin(), "second-write");
        // The columns the second write did not change hold the second write's values, not a merge
        // of the two rows: `Replace` deletes and re-inserts.
        assert_eq!(stored.name.arabic(), None);
        assert_eq!(stored.code, "code-column");
    }

    /// One nullable column, blanked, and what must still be true of its neighbours afterwards.
    ///
    /// A table rather than six near-identical test bodies, and a named struct rather than a tuple
    /// of two function pointers, because the three fields are what the case *is*.
    struct AbsentColumn {
        /// The column as the schema spells it.
        column: &'static str,
        /// Removes exactly this column's value from an otherwise fully-distinct operator.
        blank: fn(&mut OperatorRow),
        /// Asserts the absence landed on this column's field, and that a neighbour survived.
        assert_absent: fn(&OperatorRow),
    }

    #[test]
    fn a_null_in_one_column_reaches_that_columns_field_and_no_other() {
        // The NULL pass, one column at a time. A row with every nullable column NULL cannot tell a
        // shift from a correct read — six `None`s look the same in any order, so the all-absent
        // version of this test passes under a permutation of the absent columns. Each case here
        // writes exactly one absence and asserts a neighbour did not vanish with it.
        let db = setup_db();
        let full = an_operator_with_no_two_columns_alike();

        let cases = [
            AbsentColumn {
                column: "employee_id",
                blank: |row| row.employee_id = None,
                assert_absent: |row| {
                    assert_eq!(row.employee_id, None);
                    assert_eq!(
                        row.employee_number.as_deref(),
                        Some("employee-number-column")
                    );
                },
            },
            AbsentColumn {
                column: "employee_number",
                blank: |row| row.employee_number = None,
                assert_absent: |row| {
                    assert_eq!(row.employee_number, None);
                    assert_eq!(row.employee_id.as_deref(), Some("employee-id-column"));
                },
            },
            AbsentColumn {
                column: "name_ar",
                blank: |row| {
                    row.name = OperatorName::new("name-column", None::<&str>).unwrap();
                },
                assert_absent: |row| {
                    assert_eq!(row.name.arabic(), None);
                    assert_eq!(row.name.latin(), "name-column");
                },
            },
            AbsentColumn {
                column: "department",
                blank: |row| row.department = None,
                assert_absent: |row| {
                    assert_eq!(row.department, None);
                    assert_eq!(row.position.as_deref(), Some("position-column"));
                },
            },
            AbsentColumn {
                column: "position",
                blank: |row| row.position = None,
                assert_absent: |row| {
                    assert_eq!(row.position, None);
                    assert_eq!(row.department.as_deref(), Some("department-column"));
                },
            },
            AbsentColumn {
                column: "permissions_json",
                blank: |row| row.permissions = None,
                assert_absent: |row| {
                    assert_eq!(row.permissions, None);
                    assert!(!row.is_active);
                },
            },
        ];

        for case in cases {
            let mut written = full.clone();
            (case.blank)(&mut written);
            db.save_operator(&written).unwrap();

            let stored: Option<String> = db
                .select_scalar(&format!("SELECT {} FROM operators", case.column), [])
                .unwrap();
            assert_eq!(stored, None, "`{}` was not written as NULL", case.column);

            let read = db
                .get_operator_by_id(&OperatorId::new("id-column").unwrap())
                .unwrap()
                .expect("the row this iteration wrote");
            (case.assert_absent)(&read);
        }
    }

    #[test]
    fn no_two_entries_in_the_operator_mapping_name_the_same_column() {
        // `department from "position"` beside `position` compiles clean — the field set is
        // policed by the compiler and the column strings are not. A duplicate is the half of that
        // hole a declaration can close on its own; task 14's `PRAGMA table_info` guard closes the
        // other half, a column the schema does not have.
        let mut seen = std::collections::HashSet::new();
        for column in OPERATOR_ROW.insert_column_names() {
            assert!(seen.insert(column), "`{column}` is named twice");
        }
        assert_eq!(seen.len(), 12, "eleven read columns plus `updated_at`");
    }

    // ------------------------------------------------------------------------------------------
    // `OPERATOR_CREDENTIALS_ROW`. Added because a mutation swapping `role` and `permissions_json`
    // in the declaration **survived**: the reader had no test at all, so nothing exercised it and
    // the swap was invisible. A shape with no test is not covered by the shape's own existence.
    // ------------------------------------------------------------------------------------------

    #[test]
    fn the_credentials_reader_names_its_five_columns_over_the_operators_table() {
        assert_eq!(
            OPERATOR_CREDENTIALS_ROW.select_list(),
            "name, name_ar, role, permissions_json, is_active"
        );
        assert_eq!(OPERATOR_CREDENTIALS_ROW.width(), 5);
        assert_eq!(
            OPERATOR_CREDENTIALS_ROW.source(),
            Some("operators"),
            "declared table-less, this shape would be exempted from the schema check as an \
             aggregate — and it is a projection of a real table"
        );
    }

    #[test]
    fn every_credentials_column_comes_from_its_own_position() {
        let db = setup_db();
        let written = OperatorRow {
            role: OperatorRole::Manager,
            permissions: Some(OperatorPermissions::none()),
            is_active: true,
            ..an_operator("credentials-1", "Latin Name", Some("الاسم"))
        };
        db.save_operator(&written).unwrap();

        let read = db
            .select_one(
                &OPERATOR_CREDENTIALS_ROW,
                "FROM operators WHERE id = ?1",
                ["credentials-1"],
            )
            .unwrap()
            .expect("the operator this test just wrote");

        // Both halves of the pair, because reading one without the other is the thing the pair
        // codec exists to prevent.
        assert_eq!(read.name.latin(), "Latin Name");
        assert_eq!(read.name.arabic(), Some("الاسم"));
        assert_eq!(read.role, OperatorRole::Manager);
        assert!(read.is_active);
        assert!(
            read.permissions_json.is_some(),
            "`permissions_json` read as absent for an operator that has permissions — the \
             likeliest cause is a column ahead of it in the declaration"
        );
    }

    /// An inactive operator reads as inactive, and the neighbouring columns do not move.
    ///
    /// The control for the test above: without a second row that differs, `is_active` could be
    /// hard-coded `true` and both would pass.
    #[test]
    fn an_inactive_operator_reads_as_inactive_through_the_credentials_reader() {
        let db = setup_db();
        db.save_operator(&OperatorRow {
            role: OperatorRole::Cashier,
            permissions: None,
            is_active: false,
            ..an_operator("credentials-2", "Other Name", None)
        })
        .unwrap();

        let read = db
            .select_one(
                &OPERATOR_CREDENTIALS_ROW,
                "FROM operators WHERE id = ?1",
                ["credentials-2"],
            )
            .unwrap()
            .unwrap();
        assert!(!read.is_active);
        assert_eq!(read.role, OperatorRole::Cashier);
        assert_eq!(read.name.latin(), "Other Name");
        assert_eq!(read.name.arabic(), None);
        assert_eq!(read.permissions_json, None);
    }
}
