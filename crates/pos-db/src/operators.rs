//! Operators Repository
//!
//! Handles operator (cashier) data storage and retrieval.

use rusqlite::{params, OptionalExtension, Result as SqliteResult};
use serde::{Deserialize, Serialize};

use super::Database;

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
    /// POS role: CASHIER, SUPERVISOR, MANAGER
    pub role: String,
    /// HR Department name
    pub department: Option<String>,
    /// HR Position/Job title
    pub position: Option<String>,
    /// Permissions JSON
    pub permissions_json: Option<String>,
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
            role: "CASHIER".to_string(),
            department: None,
            position: None,
            permissions_json: None,
            is_active: true,
        }
    }
}

/// Operator permissions parsed from JSON
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OperatorPermissions {
    pub can_void: bool,
    pub can_refund: bool,
    pub can_discount: bool,
    pub max_discount_percent: f64,
    pub can_open_drawer: bool,
    pub can_view_reports: bool,
    pub can_manage_shifts: bool,
    pub can_access_settings: bool,
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
                &operator.role,
                &operator.department,
                &operator.position,
                &operator.permissions_json,
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
                    operator.role,
                    operator.department,
                    operator.position,
                    operator.permissions_json,
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
                role: row.get(7)?,
                department: row.get(8)?,
                position: row.get(9)?,
                permissions_json: row.get(10)?,
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
                    role: row.get(7)?,
                    department: row.get(8)?,
                    position: row.get(9)?,
                    permissions_json: row.get(10)?,
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
                    role: row.get(7)?,
                    department: row.get(8)?,
                    position: row.get(9)?,
                    permissions_json: row.get(10)?,
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
                role: row.get(7)?,
                department: row.get(8)?,
                position: row.get(9)?,
                permissions_json: row.get(10)?,
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
    /// Parses permissions from JSON
    pub fn permissions(&self) -> OperatorPermissions {
        self.permissions_json
            .as_ref()
            .and_then(|json| serde_json::from_str(json).ok())
            .unwrap_or_default()
    }

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
            role: "CASHIER".to_string(),
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
    fn test_operator_permissions() {
        let operator = OperatorRow {
            permissions_json: Some(
                r#"{"can_void": true, "can_refund": false, "can_discount": true, "max_discount_percent": 10.0, "can_open_drawer": false, "can_view_reports": true, "can_manage_shifts": false, "can_access_settings": false}"#
                    .to_string(),
            ),
            ..Default::default()
        };

        let perms = operator.permissions();
        assert!(perms.can_void);
        assert!(!perms.can_refund);
        assert_eq!(perms.max_discount_percent, 10.0);
    }
}
