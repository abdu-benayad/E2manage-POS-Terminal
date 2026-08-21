//! Operators Repository
//!
//! Handles operator (cashier) data storage and retrieval.

use pos_models::{OperatorPermissions, OperatorRole};

use crate::column::operator_role;
use rusqlite::{params, OptionalExtension, Result as SqliteResult};
use serde::{Deserialize, Serialize};

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

/// Reads an operator's permissions back out of the column.
///
/// A row whose permissions will not parse is a **read failure the caller sees**. It used to be
/// `.ok().unwrap_or_default()`, which turned an unreadable column into an operator holding no
/// privileges — the same value as a genuinely unprivileged cashier, and indistinguishable from
/// one. It failed closed, so nobody noticed; the mechanism was indifferent to direction.
fn read_permissions(
    row: &rusqlite::Row<'_>,
    index: usize,
) -> SqliteResult<Option<OperatorPermissions>> {
    match row.get::<_, Option<String>>(index)? {
        None => Ok(None),
        Some(json) => serde_json::from_str(&json).map(Some).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                index,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        }),
    }
}

/// Operator row from database (HR Employee integrated)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperatorRow {
    /// POS Operator Profile ID
    pub id: String,
    /// Operator code (for quick lookup)
    pub code: String,
    /// HR Employee ID
    pub employee_id: Option<String>,
    /// HR Employee Number (e.g., "EMP001")
    pub employee_number: Option<String>,
    /// Full name (English)
    pub name: String,
    /// Full name (Arabic)
    pub name_ar: Option<String>,
    /// BCrypt hashed PIN
    pub pin_hash: String,
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

impl Default for OperatorRow {
    fn default() -> Self {
        Self {
            id: String::new(),
            code: String::new(),
            employee_id: None,
            employee_number: None,
            name: String::new(),
            name_ar: None,
            pin_hash: String::new(),
            role: OperatorRole::Cashier,
            department: None,
            position: None,
            permissions: None,
            is_active: true,
        }
    }
}

impl Database {
    /// Saves or updates an operator
    pub fn save_operator(&self, operator: &OperatorRow) -> SqliteResult<()> {
        self.execute(
            r#"INSERT OR REPLACE INTO operators
               (id, code, employee_id, employee_number, name, name_ar, pin_hash, role, department, position, permissions_json, is_active, updated_at)
               VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, datetime('now'))"#,
            &[
                &operator.id,
                &operator.code,
                &operator.employee_id,
                &operator.employee_number,
                &operator.name,
                &operator.name_ar,
                &operator.pin_hash,
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
                   (id, code, employee_id, employee_number, name, name_ar, pin_hash, role, department, position, permissions_json, is_active, updated_at)
                   VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, datetime('now'))"#,
            )?;

            for operator in operators {
                stmt.execute(params![
                    operator.id,
                    operator.code,
                    operator.employee_id,
                    operator.employee_number,
                    operator.name,
                    operator.name_ar,
                    operator.pin_hash,
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
            r#"SELECT id, code, employee_id, employee_number, name, name_ar, pin_hash, role, department, position, permissions_json, is_active
               FROM operators
               WHERE is_active = 1
               ORDER BY name"#,
        )?;

        let rows = stmt.query_map([], |row: &rusqlite::Row| {
            Ok(OperatorRow {
                id: row.get(0)?,
                code: row.get(1)?,
                employee_id: row.get(2)?,
                employee_number: row.get(3)?,
                name: row.get(4)?,
                name_ar: row.get(5)?,
                pin_hash: row.get(6)?,
                role: operator_role(row, 7)?,
                department: row.get(8)?,
                position: row.get(9)?,
                permissions: read_permissions(row, 10)?,
                is_active: row.get(11)?,
            })
        })?;

        rows.collect()
    }

    /// Gets an operator by ID
    pub fn get_operator_by_id(&self, id: &str) -> SqliteResult<Option<OperatorRow>> {
        let conn = self.connection();
        let conn = conn.lock();

        conn.query_row(
            r#"SELECT id, code, employee_id, employee_number, name, name_ar, pin_hash, role, department, position, permissions_json, is_active
               FROM operators WHERE id = ?1"#,
            [id],
            |row| {
                Ok(OperatorRow {
                    id: row.get(0)?,
                    code: row.get(1)?,
                    employee_id: row.get(2)?,
                    employee_number: row.get(3)?,
                    name: row.get(4)?,
                    name_ar: row.get(5)?,
                    pin_hash: row.get(6)?,
                    role: operator_role(row, 7)?,
                    department: row.get(8)?,
                    position: row.get(9)?,
                    permissions: read_permissions(row, 10)?,
                    is_active: row.get(11)?,
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
            r#"SELECT id, code, employee_id, employee_number, name, name_ar, pin_hash, role, department, position, permissions_json, is_active
               FROM operators WHERE employee_number = ?1 AND is_active = 1"#,
            [employee_number],
            |row| {
                Ok(OperatorRow {
                    id: row.get(0)?,
                    code: row.get(1)?,
                    employee_id: row.get(2)?,
                    employee_number: row.get(3)?,
                    name: row.get(4)?,
                    name_ar: row.get(5)?,
                    pin_hash: row.get(6)?,
                    role: operator_role(row, 7)?,
                    department: row.get(8)?,
                    position: row.get(9)?,
                    permissions: read_permissions(row, 10)?,
                    is_active: row.get(11)?,
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
            r#"SELECT id, code, employee_id, employee_number, name, name_ar, pin_hash, role, department, position, permissions_json, is_active
               FROM operators
               WHERE is_active = 1
                 AND (name LIKE ?1 OR name_ar LIKE ?1 OR employee_number LIKE ?1)
               ORDER BY name
               LIMIT ?2"#,
        )?;

        let search = format!("%{}%", query);
        let rows = stmt.query_map(params![search, limit], |row: &rusqlite::Row| {
            Ok(OperatorRow {
                id: row.get(0)?,
                code: row.get(1)?,
                employee_id: row.get(2)?,
                employee_number: row.get(3)?,
                name: row.get(4)?,
                name_ar: row.get(5)?,
                pin_hash: row.get(6)?,
                role: operator_role(row, 7)?,
                department: row.get(8)?,
                position: row.get(9)?,
                permissions: read_permissions(row, 10)?,
                is_active: row.get(11)?,
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
    pub fn delete_operator(&self, id: &str) -> SqliteResult<bool> {
        let deleted = self.execute("DELETE FROM operators WHERE id = ?1", &[&id])?;
        Ok(deleted > 0)
    }
}

impl OperatorRow {
    /// Gets the display name (Arabic if available, otherwise English)
    pub fn display_name(&self, prefer_ar: bool) -> &str {
        if prefer_ar {
            self.name_ar.as_deref().unwrap_or(&self.name)
        } else {
            &self.name
        }
    }

    /// Gets operator initials for avatar
    pub fn initials(&self) -> String {
        self.name
            .split_whitespace()
            .filter_map(|word| word.chars().next())
            .take(2)
            .collect::<String>()
            .to_uppercase()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::migrations::run_migrations;
    use pos_models::{DiscountAuthority, DiscountPercent, Permission};
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

    #[test]
    fn test_save_and_get_operator() {
        let db = setup_db();

        let operator = OperatorRow {
            id: "op-1".to_string(),
            code: "C001".to_string(),
            name: "Ahmed Hassan".to_string(),
            name_ar: Some("أحمد حسن".to_string()),
            pin_hash: "$2b$12$hashedpin".to_string(),
            role: OperatorRole::Cashier,
            ..Default::default()
        };

        db.save_operator(&operator).unwrap();

        let found = db.get_operator_by_id("op-1").unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().name, "Ahmed Hassan");
    }

    #[test]
    fn test_get_operator_by_code() {
        let db = setup_db();

        let operator = OperatorRow {
            id: "op-1".to_string(),
            code: "C001".to_string(),
            name: "Ahmed Hassan".to_string(),
            pin_hash: "$2b$12$hashedpin".to_string(),
            ..Default::default()
        };

        db.save_operator(&operator).unwrap();

        let found = db.get_operator_by_id("op-1").unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().code, "C001");
    }

    #[test]
    fn test_search_operators() {
        let db = setup_db();

        let operators = vec![
            OperatorRow {
                id: "op-1".to_string(),
                code: "C001".to_string(),
                name: "Ahmed Hassan".to_string(),
                pin_hash: "hash1".to_string(),
                ..Default::default()
            },
            OperatorRow {
                id: "op-2".to_string(),
                code: "C002".to_string(),
                name: "Sara Ahmed".to_string(),
                pin_hash: "hash2".to_string(),
                ..Default::default()
            },
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
        let operator = OperatorRow {
            name: "Ahmed Hassan".to_string(),
            ..Default::default()
        };
        assert_eq!(operator.initials(), "AH");

        let operator = OperatorRow {
            name: "Sara".to_string(),
            ..Default::default()
        };
        assert_eq!(operator.initials(), "S");
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
            id: "op-1".to_string(),
            permissions: Some(permissions.clone()),
            ..Default::default()
        };
        db.save_operator(&operator).unwrap();

        let stored = db
            .get_operator_by_id("op-1")
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
            "INSERT INTO operators (id, code, name, pin_hash, role, permissions_json, is_active) \
             VALUES ('op-1', 'C001', 'Ahmed', 'hash', 'CASHIER', '{\"canVoid\": ', 1)",
            &[],
        )
        .unwrap();

        assert!(db.get_operator_by_id("op-1").is_err());
    }
}
