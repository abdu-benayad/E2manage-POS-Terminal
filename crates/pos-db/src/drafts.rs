//! Drafts Repository
//!
//! Handles draft/held orders storage and management.

use chrono::Utc;
use rusqlite::{params, OptionalExtension, Result as SqliteResult};
use serde::{Deserialize, Serialize};

use super::Database;

/// Draft row from database
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DraftRow {
    pub id: String,
    pub name: Option<String>,
    pub items_json: String,
    pub customer_id: Option<String>,
    pub customer_name: Option<String>,
    pub discount_json: Option<String>,
    pub notes: Option<String>,
    pub created_at: String,
    pub expires_at: Option<String>,
    pub operator_id: Option<String>,
    pub shift_id: Option<String>,
}

impl Default for DraftRow {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: None,
            items_json: "[]".to_string(),
            customer_id: None,
            customer_name: None,
            discount_json: None,
            notes: None,
            created_at: String::new(),
            expires_at: None,
            operator_id: None,
            shift_id: None,
        }
    }
}

impl Database {
    /// Saves a draft
    pub fn save_draft(&self, draft: &DraftRow) -> SqliteResult<()> {
        self.execute(
            r#"INSERT OR REPLACE INTO drafts
               (id, name, items_json, customer_id, customer_name, discount_json, notes,
                created_at, expires_at, operator_id, shift_id)
               VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)"#,
            &[
                &draft.id,
                &draft.name,
                &draft.items_json,
                &draft.customer_id,
                &draft.customer_name,
                &draft.discount_json,
                &draft.notes,
                &draft.created_at,
                &draft.expires_at,
                &draft.operator_id,
                &draft.shift_id,
            ],
        )?;
        Ok(())
    }

    /// Gets a draft by ID
    pub fn get_draft_by_id(&self, id: &str) -> SqliteResult<Option<DraftRow>> {
        let conn = self.connection();
        let conn = conn.lock();

        conn.query_row(
            r#"SELECT id, name, items_json, customer_id, customer_name, discount_json, notes,
                      created_at, expires_at, operator_id, shift_id
               FROM drafts WHERE id = ?1"#,
            [id],
            |row| {
                Ok(DraftRow {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    items_json: row.get(2)?,
                    customer_id: row.get(3)?,
                    customer_name: row.get(4)?,
                    discount_json: row.get(5)?,
                    notes: row.get(6)?,
                    created_at: row.get(7)?,
                    expires_at: row.get(8)?,
                    operator_id: row.get(9)?,
                    shift_id: row.get(10)?,
                })
            },
        )
        .optional()
    }

    /// Gets all active drafts (not expired)
    pub fn get_active_drafts(&self) -> SqliteResult<Vec<DraftRow>> {
        let conn = self.connection();
        let conn = conn.lock();

        let now = Utc::now().to_rfc3339();

        let mut stmt = conn.prepare(
            r#"SELECT id, name, items_json, customer_id, customer_name, discount_json, notes,
                      created_at, expires_at, operator_id, shift_id
               FROM drafts
               WHERE expires_at IS NULL OR datetime(expires_at) > datetime(?1)
               ORDER BY created_at DESC"#,
        )?;

        let rows = stmt.query_map([now], |row: &rusqlite::Row| {
            Ok(DraftRow {
                id: row.get(0)?,
                name: row.get(1)?,
                items_json: row.get(2)?,
                customer_id: row.get(3)?,
                customer_name: row.get(4)?,
                discount_json: row.get(5)?,
                notes: row.get(6)?,
                created_at: row.get(7)?,
                expires_at: row.get(8)?,
                operator_id: row.get(9)?,
                shift_id: row.get(10)?,
            })
        })?;

        rows.collect()
    }

    /// Gets drafts for an operator
    pub fn get_drafts_by_operator(&self, operator_id: &str) -> SqliteResult<Vec<DraftRow>> {
        let conn = self.connection();
        let conn = conn.lock();

        let now = Utc::now().to_rfc3339();

        let mut stmt = conn.prepare(
            r#"SELECT id, name, items_json, customer_id, customer_name, discount_json, notes,
                      created_at, expires_at, operator_id, shift_id
               FROM drafts
               WHERE operator_id = ?1
                 AND (expires_at IS NULL OR datetime(expires_at) > datetime(?2))
               ORDER BY created_at DESC"#,
        )?;

        let rows = stmt.query_map(params![operator_id, now], |row: &rusqlite::Row| {
            Ok(DraftRow {
                id: row.get(0)?,
                name: row.get(1)?,
                items_json: row.get(2)?,
                customer_id: row.get(3)?,
                customer_name: row.get(4)?,
                discount_json: row.get(5)?,
                notes: row.get(6)?,
                created_at: row.get(7)?,
                expires_at: row.get(8)?,
                operator_id: row.get(9)?,
                shift_id: row.get(10)?,
            })
        })?;

        rows.collect()
    }

    /// Gets drafts for a shift
    pub fn get_drafts_by_shift(&self, shift_id: &str) -> SqliteResult<Vec<DraftRow>> {
        let conn = self.connection();
        let conn = conn.lock();

        let mut stmt = conn.prepare(
            r#"SELECT id, name, items_json, customer_id, customer_name, discount_json, notes,
                      created_at, expires_at, operator_id, shift_id
               FROM drafts
               WHERE shift_id = ?1
               ORDER BY created_at DESC"#,
        )?;

        let rows = stmt.query_map([shift_id], |row: &rusqlite::Row| {
            Ok(DraftRow {
                id: row.get(0)?,
                name: row.get(1)?,
                items_json: row.get(2)?,
                customer_id: row.get(3)?,
                customer_name: row.get(4)?,
                discount_json: row.get(5)?,
                notes: row.get(6)?,
                created_at: row.get(7)?,
                expires_at: row.get(8)?,
                operator_id: row.get(9)?,
                shift_id: row.get(10)?,
            })
        })?;

        rows.collect()
    }

    /// Counts active drafts
    pub fn get_draft_count(&self) -> SqliteResult<i64> {
        let conn = self.connection();
        let conn = conn.lock();

        let now = Utc::now().to_rfc3339();

        conn.query_row(
            "SELECT COUNT(*) FROM drafts WHERE expires_at IS NULL OR datetime(expires_at) > datetime(?1)",
            [now],
            |row| row.get(0),
        )
    }

    /// Deletes a draft
    pub fn delete_draft(&self, id: &str) -> SqliteResult<bool> {
        let deleted = self.execute("DELETE FROM drafts WHERE id = ?1", &[&id])?;
        Ok(deleted > 0)
    }

    /// Deletes expired drafts
    pub fn cleanup_expired_drafts(&self) -> SqliteResult<usize> {
        let now = Utc::now().to_rfc3339();
        self.execute(
            "DELETE FROM drafts WHERE expires_at IS NOT NULL AND datetime(expires_at) <= datetime(?1)",
            &[&now],
        )
    }

    /// Creates a draft with auto-generated name
    #[expect(
        clippy::too_many_arguments,
        reason = "pub API across crate boundary; refactor is contract-breaking and out of scope for clippy cleanup"
    )]
    pub fn create_draft(
        &self,
        id: &str,
        items_json: &str,
        operator_id: Option<&str>,
        shift_id: Option<&str>,
        customer_id: Option<&str>,
        customer_name: Option<&str>,
        expires_hours: Option<i32>,
    ) -> SqliteResult<DraftRow> {
        let now = Utc::now();
        let created_at = now.to_rfc3339();

        let expires_at = expires_hours.map(|hours| {
            (now + chrono::Duration::hours(hours as i64)).to_rfc3339()
        });

        // Generate name like "Hold #3"
        let count = self.get_draft_count()? + 1;
        let name = format!("Hold #{}", count);

        let draft = DraftRow {
            id: id.to_string(),
            name: Some(name),
            items_json: items_json.to_string(),
            customer_id: customer_id.map(String::from),
            customer_name: customer_name.map(String::from),
            discount_json: None,
            notes: None,
            created_at,
            expires_at,
            operator_id: operator_id.map(String::from),
            shift_id: shift_id.map(String::from),
        };

        self.save_draft(&draft)?;
        Ok(draft)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::migrations::run_migrations;
    use crate::operators::OperatorRow;

    fn setup_db() -> Database {
        let db = Database::in_memory().unwrap();
        {
            let conn = db.connection();
            let conn = conn.lock();
            run_migrations(&conn).unwrap();
        }
        db
    }

    fn create_test_operator(db: &Database, id: &str) {
        let operator = OperatorRow {
            id: id.to_string(),
            code: format!("C{}", id),
            name: "Test Operator".to_string(),
            pin_hash: "hash".to_string(),
            ..Default::default()
        };
        db.save_operator(&operator).unwrap();
    }

    #[test]
    fn test_create_and_get_draft() {
        let db = setup_db();
        create_test_operator(&db, "op-1");

        let draft = db
            .create_draft(
                "draft-1",
                r#"[{"id":"prod-1","qty":2}]"#,
                Some("op-1"),
                None,  // Don't use shift foreign key in tests
                None,
                None,
                Some(24),
            )
            .unwrap();

        assert_eq!(draft.name, Some("Hold #1".to_string()));
        assert!(draft.expires_at.is_some());

        let found = db.get_draft_by_id("draft-1").unwrap();
        assert!(found.is_some());
    }

    #[test]
    fn test_get_active_drafts() {
        let db = setup_db();
        create_test_operator(&db, "op-1");

        // Create some drafts
        for i in 1..=3 {
            db.create_draft(
                &format!("draft-{}", i),
                "[]",
                Some("op-1"),
                None,
                None,
                None,
                Some(24),
            )
            .unwrap();
        }

        let drafts = db.get_active_drafts().unwrap();
        assert_eq!(drafts.len(), 3);

        let count = db.get_draft_count().unwrap();
        assert_eq!(count, 3);
    }

    #[test]
    fn test_delete_draft() {
        let db = setup_db();

        db.create_draft("draft-1", "[]", None, None, None, None, None)
            .unwrap();

        let deleted = db.delete_draft("draft-1").unwrap();
        assert!(deleted);

        let found = db.get_draft_by_id("draft-1").unwrap();
        assert!(found.is_none());
    }

    #[test]
    fn test_drafts_by_operator() {
        let db = setup_db();
        create_test_operator(&db, "op-1");
        create_test_operator(&db, "op-2");

        db.create_draft("d1", "[]", Some("op-1"), None, None, None, None)
            .unwrap();
        db.create_draft("d2", "[]", Some("op-1"), None, None, None, None)
            .unwrap();
        db.create_draft("d3", "[]", Some("op-2"), None, None, None, None)
            .unwrap();

        let op1_drafts = db.get_drafts_by_operator("op-1").unwrap();
        assert_eq!(op1_drafts.len(), 2);

        let op2_drafts = db.get_drafts_by_operator("op-2").unwrap();
        assert_eq!(op2_drafts.len(), 1);
    }
}
