//! Operators Repository
//!
//! Handles operator (cashier) data storage and retrieval.

use pos_models::{OperatorId, OperatorName, OperatorPermissions, OperatorRole};

// `read_permissions` moved to `crate::column` as the read half of `column::PERMISSIONS`. It was a
// fourth positional reader living outside the module that owns them, which is why no measurement
// of this repo's positional reads has ever counted it. Aliased rather than renamed at the call
// sites so this task changes one line; task 04 migrates the four callers and drops the alias.
use crate::column;
use crate::column::{operator_id, operator_name, operator_role, permissions as read_permissions};
use crate::projection::OnConflict;
use crate::row_mapping;
use rusqlite::{params, OptionalExtension, Result as SqliteResult};

use super::Database;

/// Serialises an operator's permissions for the `permissions_json` column.
///
/// `pos_models::OperatorPermissions` owns the only mapping to the server's shape, so this is a
/// call into it rather than a second spelling of the keys.
fn permissions_json(operator: &OperatorRow) -> SqliteResult<Option<String>> {
    operator
        .permissions
        .as_ref()
        .map(|permissions| {
            serde_json::to_string(permissions)
                .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))
        })
        .transpose()
}

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

impl Database {
    /// Saves or updates an operator
    pub fn save_operator(&self, operator: &OperatorRow) -> SqliteResult<()> {
        self.execute(
            r#"INSERT OR REPLACE INTO operators
               (id, code, employee_id, employee_number, name, name_ar, role, department, position, permissions_json, is_active, updated_at)
               VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, datetime('now'))"#,
            &[
                &operator.id.as_str(),
                &operator.code,
                &operator.employee_id,
                &operator.employee_number,
                &operator.name.latin(),
                &operator.name.arabic(),
                &operator.role.as_wire_str(),
                &operator.department,
                &operator.position,
                &permissions_json(operator)?,
                &operator.is_active,
            ],
        )?;
        Ok(())
    }

    /// Bulk saves operators
    pub fn save_operators(&self, operators: &[OperatorRow]) -> SqliteResult<usize> {
        let conn = self.connection();
        let conn = conn.lock();

        let tx = conn.unchecked_transaction()?;
        let mut count = 0;

        {
            let mut stmt = conn.prepare(
                r#"INSERT OR REPLACE INTO operators
                   (id, code, employee_id, employee_number, name, name_ar, role, department, position, permissions_json, is_active, updated_at)
                   VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, datetime('now'))"#,
            )?;

            for operator in operators {
                stmt.execute(params![
                    operator.id.as_str(),
                    operator.code,
                    operator.employee_id,
                    operator.employee_number,
                    operator.name.latin(),
                    operator.name.arabic(),
                    operator.role.as_wire_str(),
                    operator.department,
                    operator.position,
                    permissions_json(operator)?,
                    operator.is_active,
                ])?;
                count += 1;
            }
        }

        tx.commit()?;
        Ok(count)
    }

    /// Gets all active operators
    pub fn get_operators(&self) -> SqliteResult<Vec<OperatorRow>> {
        let conn = self.connection();
        let conn = conn.lock();

        let mut stmt = conn.prepare(
            r#"SELECT id, code, employee_id, employee_number, name, name_ar, role, department, position, permissions_json, is_active
               FROM operators
               WHERE is_active = 1
               ORDER BY name"#,
        )?;

        let rows = stmt.query_map([], |row: &rusqlite::Row| {
            Ok(OperatorRow {
                id: operator_id(row, 0)?,
                code: row.get(1)?,
                employee_id: row.get(2)?,
                employee_number: row.get(3)?,
                name: operator_name(row, 4, 5)?,
                role: operator_role(row, 6)?,
                department: row.get(7)?,
                position: row.get(8)?,
                permissions: read_permissions(row, 9)?,
                is_active: row.get(10)?,
            })
        })?;

        rows.collect()
    }

    /// Gets an operator by ID
    pub fn get_operator_by_id(&self, id: &OperatorId) -> SqliteResult<Option<OperatorRow>> {
        let conn = self.connection();
        let conn = conn.lock();

        conn.query_row(
            r#"SELECT id, code, employee_id, employee_number, name, name_ar, role, department, position, permissions_json, is_active
               FROM operators WHERE id = ?1"#,
            [id.as_str()],
            |row| {
                Ok(OperatorRow {
                    id: operator_id(row, 0)?,
                    code: row.get(1)?,
                    employee_id: row.get(2)?,
                    employee_number: row.get(3)?,
                    name: operator_name(row, 4, 5)?,
                        role: operator_role(row, 6)?,
                    department: row.get(7)?,
                    position: row.get(8)?,
                    permissions: read_permissions(row, 9)?,
                    is_active: row.get(10)?,
                })
            },
        )
        .optional()
    }

    /// Gets an operator by employee number
    pub fn get_operator_by_employee_number(
        &self,
        employee_number: &str,
    ) -> SqliteResult<Option<OperatorRow>> {
        let conn = self.connection();
        let conn = conn.lock();

        conn.query_row(
            r#"SELECT id, code, employee_id, employee_number, name, name_ar, role, department, position, permissions_json, is_active
               FROM operators WHERE employee_number = ?1 AND is_active = 1"#,
            [employee_number],
            |row| {
                Ok(OperatorRow {
                    id: operator_id(row, 0)?,
                    code: row.get(1)?,
                    employee_id: row.get(2)?,
                    employee_number: row.get(3)?,
                    name: operator_name(row, 4, 5)?,
                        role: operator_role(row, 6)?,
                    department: row.get(7)?,
                    position: row.get(8)?,
                    permissions: read_permissions(row, 9)?,
                    is_active: row.get(10)?,
                })
            },
        )
        .optional()
    }

    /// Searches operators by name or employee number
    pub fn search_operators(&self, query: &str, limit: i32) -> SqliteResult<Vec<OperatorRow>> {
        let conn = self.connection();
        let conn = conn.lock();

        let mut stmt = conn.prepare(
            r#"SELECT id, code, employee_id, employee_number, name, name_ar, role, department, position, permissions_json, is_active
               FROM operators
               WHERE is_active = 1
                 AND (name LIKE ?1 OR name_ar LIKE ?1 OR employee_number LIKE ?1)
               ORDER BY name
               LIMIT ?2"#,
        )?;

        let search = format!("%{}%", query);
        let rows = stmt.query_map(params![search, limit], |row: &rusqlite::Row| {
            Ok(OperatorRow {
                id: operator_id(row, 0)?,
                code: row.get(1)?,
                employee_id: row.get(2)?,
                employee_number: row.get(3)?,
                name: operator_name(row, 4, 5)?,
                role: operator_role(row, 6)?,
                department: row.get(7)?,
                position: row.get(8)?,
                permissions: read_permissions(row, 9)?,
                is_active: row.get(10)?,
            })
        })?;

        rows.collect()
    }

    /// Gets the total operator count
    pub fn get_operator_count(&self) -> SqliteResult<i64> {
        let conn = self.connection();
        let conn = conn.lock();

        conn.query_row(
            "SELECT COUNT(*) FROM operators WHERE is_active = 1",
            [],
            |row| row.get(0),
        )
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

    #[test]
    fn each_value_lands_in_the_column_that_carries_its_name() {
        // A round trip is symmetric: swap two columns in the declaration and it still passes,
        // because the write and the read swap together. That is exactly the defect. This is the
        // asymmetric half — write through the mapping, read back **by name**, in a query the
        // declaration had no hand in.
        //
        // Measured, not asserted: with `department from "position"` and `position from
        // "department"` in the declaration, the round-trip test above stays green and this one
        // fails. Three of the four new tests here survive that mutation.
        let db = setup_db();
        db.insert(&OPERATOR_ROW, &an_operator_with_no_two_columns_alike())
            .unwrap();

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
    fn a_row_whose_optional_columns_are_all_null_reads_back_as_absent_not_as_blank() {
        // The NULL pass. Six of the eleven columns are nullable, and `Option<T>` reading a
        // wrongly-indexed neighbour is the swap that a fully-populated fixture cannot show:
        // `None` and `Some("")` are different values that a positional shift turns into each other.
        let db = setup_db();
        let written = OperatorRow {
            employee_id: None,
            employee_number: None,
            department: None,
            position: None,
            permissions: None,
            name: OperatorName::new("only-latin", None::<&str>).unwrap(),
            ..an_operator_with_no_two_columns_alike()
        };
        db.insert(&OPERATOR_ROW, &written).unwrap();

        let read = db
            .select_one(
                OPERATOR_ROW.reader(),
                "FROM operators WHERE id = ?1",
                ["id-column"],
            )
            .unwrap()
            .expect("the row this test just wrote");

        assert_eq!(read.employee_id, None);
        assert_eq!(read.employee_number, None);
        assert_eq!(read.department, None);
        assert_eq!(read.position, None);
        assert_eq!(read.permissions, None);
        assert_eq!(read.name.arabic(), None);
        // The non-null neighbours are still themselves, which is what makes the `None`s above a
        // reading about the columns rather than about an empty table.
        assert_eq!(read.name.latin(), "only-latin");
        assert_eq!(read.code, "code-column");
        assert_eq!(read.role, OperatorRole::Manager);
    }

    #[test]
    fn the_declared_mapping_writes_the_same_columns_the_hand_written_insert_does() {
        // Task 04 replaces `save_operator`'s literal SQL with this mapping. The two agreeing today
        // is what makes that a refactor.
        //
        // What this does **not** catch: a swap inside the declaration. Measured — swapping the
        // `department` and `position` columns leaves this green, because the mapping's write and
        // the mapping's read swap together and the hand-written path is untouched. It catches a
        // column present on one side and absent on the other. `each_value_lands_in_the_column_that
        // _carries_its_name` is the one that catches a swap, and it is the only one that does.
        let db = setup_db();
        let operator = an_operator_with_no_two_columns_alike();

        db.save_operator(&operator).unwrap();
        let by_hand = db
            .get_operator_by_id(&OperatorId::new("id-column").unwrap())
            .unwrap()
            .expect("the hand-written insert");

        db.execute("DELETE FROM operators", &[]).unwrap();
        db.insert(&OPERATOR_ROW, &operator).unwrap();
        let by_mapping = db
            .select_one(
                OPERATOR_ROW.reader(),
                "FROM operators WHERE id = ?1",
                ["id-column"],
            )
            .unwrap()
            .expect("the mapping's insert");

        assert_eq!(by_hand.id, by_mapping.id);
        assert_eq!(by_hand.code, by_mapping.code);
        assert_eq!(by_hand.employee_id, by_mapping.employee_id);
        assert_eq!(by_hand.employee_number, by_mapping.employee_number);
        assert_eq!(by_hand.name.latin(), by_mapping.name.latin());
        assert_eq!(by_hand.name.arabic(), by_mapping.name.arabic());
        assert_eq!(by_hand.role, by_mapping.role);
        assert_eq!(by_hand.department, by_mapping.department);
        assert_eq!(by_hand.position, by_mapping.position);
        assert_eq!(by_hand.permissions, by_mapping.permissions);
        assert_eq!(by_hand.is_active, by_mapping.is_active);
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
}
