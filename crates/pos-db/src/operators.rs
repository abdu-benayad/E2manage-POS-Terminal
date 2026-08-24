//! Operators Repository
//!
//! Handles operator (cashier) data storage and retrieval.

use pos_models::{OperatorId, OperatorName, OperatorPermissions, OperatorRole};

// `read_permissions` moved to `crate::column` as the read half of `column::PERMISSIONS`. It was a
// fourth positional reader living outside the module that owns them, which is why no measurement
// of this repo's positional reads has ever counted it. Aliased rather than renamed at the call
// sites so this task changes one line; task 04 migrates the four callers and drops the alias.
use crate::column::{operator_id, operator_name, operator_role, permissions as read_permissions};
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
}
