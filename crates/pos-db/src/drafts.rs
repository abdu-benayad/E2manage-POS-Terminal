//! Drafts Repository
//!
//! Handles draft/held orders storage and management.

use chrono::Utc;
use rusqlite::{params, Result as SqliteResult};
use serde::{Deserialize, Serialize};

use pos_models::OperatorId;

use super::Database;
use crate::column;
use crate::projection::OnConflict;
use crate::row_mapping;

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
    pub operator_id: Option<OperatorId>,
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

row_mapping! {
    /// Every column of `drafts` this till reads or writes, declared once.
    ///
    /// Four reader closures and one `INSERT` list, all eleven names in the same order.
    /// `created_at` is supplied by the caller rather than `managed`: a held order records when the
    /// cashier parked it.
    pub const DRAFT_ROW: RowMapping<DraftRow> = for "drafts" {
        id,
        name            via column::OPTIONAL_TEXT,
        items_json,
        customer_id     via column::OPTIONAL_TEXT,
        customer_name   via column::OPTIONAL_TEXT,
        discount_json   via column::OPTIONAL_TEXT,
        notes           via column::OPTIONAL_TEXT,
        created_at,
        expires_at      via column::OPTIONAL_TEXT,
        operator_id     via column::OPTIONAL_OPERATOR_ID,
        shift_id        via column::OPTIONAL_TEXT,
    } on_conflict OnConflict::Replace;
}

impl Database {
    /// Saves a draft.
    pub fn save_draft(&self, draft: &DraftRow) -> SqliteResult<()> {
        self.insert(&DRAFT_ROW, draft)?;
        Ok(())
    }

    /// Gets a draft by ID.
    pub fn get_draft_by_id(&self, id: &str) -> SqliteResult<Option<DraftRow>> {
        self.select_one(DRAFT_ROW.reader(), "FROM drafts WHERE id = ?1", [id])
    }

    /// Gets every draft that has not expired, newest first.
    pub fn get_active_drafts(&self) -> SqliteResult<Vec<DraftRow>> {
        self.select_all(
            DRAFT_ROW.reader(),
            "FROM drafts
             WHERE expires_at IS NULL OR datetime(expires_at) > datetime(?1)
             ORDER BY created_at DESC",
            [Utc::now().to_rfc3339()],
        )
    }

    /// Gets every unexpired draft one operator parked, newest first.
    pub fn get_drafts_by_operator(&self, operator_id: &OperatorId) -> SqliteResult<Vec<DraftRow>> {
        self.select_all(
            DRAFT_ROW.reader(),
            "FROM drafts
             WHERE operator_id = ?1
               AND (expires_at IS NULL OR datetime(expires_at) > datetime(?2))
             ORDER BY created_at DESC",
            params![operator_id.as_str(), Utc::now().to_rfc3339()],
        )
    }

    /// Gets every unexpired draft parked during one shift, newest first.
    pub fn get_drafts_by_shift(&self, shift_id: &str) -> SqliteResult<Vec<DraftRow>> {
        self.select_all(
            DRAFT_ROW.reader(),
            "FROM drafts
             WHERE shift_id = ?1
               AND (expires_at IS NULL OR datetime(expires_at) > datetime(?2))
             ORDER BY created_at DESC",
            params![shift_id, Utc::now().to_rfc3339()],
        )
    }

    /// Counts active drafts
    pub fn get_draft_count(&self) -> SqliteResult<i64> {
        self.select_scalar(
            "SELECT COUNT(*) FROM drafts \
             WHERE expires_at IS NULL OR datetime(expires_at) > datetime(?1)",
            [Utc::now().to_rfc3339()],
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
    // The adjacent `Option<&str>` parameters are freely swappable at the call site, which is the
    // defect, not the count. Task 09 typed `operator_id` as `OperatorId`, so that one is no
    // longer swappable with the other three; `shift_id`, `customer_id` and `customer_name` still
    // are, and belong to later tiers of `type-driven-domain-core`. The arity is unchanged, so the
    // `expect` below still holds.
    #[expect(
        clippy::too_many_arguments,
        reason = "eight parameters, and `OperatorId` (task 09) typed two of them without \
                  reducing the count; the remaining primitives belong to later tiers of \
                  type-driven-domain-core"
    )]
    pub fn create_draft(
        &self,
        id: &str,
        items_json: &str,
        operator_id: Option<&OperatorId>,
        shift_id: Option<&str>,
        customer_id: Option<&str>,
        customer_name: Option<&str>,
        expires_hours: Option<i32>,
    ) -> SqliteResult<DraftRow> {
        let now = Utc::now();
        let created_at = now.to_rfc3339();

        let expires_at =
            expires_hours.map(|hours| (now + chrono::Duration::hours(hours as i64)).to_rfc3339());

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
            operator_id: operator_id.cloned(),
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
    use crate::projection::scalar;
    use pos_models::{OperatorName, OperatorRole};

    fn op_id(id: &str) -> OperatorId {
        OperatorId::new(id).expect("a non-blank id")
    }

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
            id: OperatorId::new(id).unwrap(),
            code: format!("C{id}"),
            employee_id: None,
            employee_number: None,
            name: OperatorName::new("Test Operator", None::<&str>).unwrap(),
            role: OperatorRole::Cashier,
            department: None,
            position: None,
            permissions: None,
            is_active: true,
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
                Some(&op_id("op-1")),
                None, // Don't use shift foreign key in tests
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
                Some(&op_id("op-1")),
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

        db.create_draft("d1", "[]", Some(&op_id("op-1")), None, None, None, None)
            .unwrap();
        db.create_draft("d2", "[]", Some(&op_id("op-1")), None, None, None, None)
            .unwrap();
        db.create_draft("d3", "[]", Some(&op_id("op-2")), None, None, None, None)
            .unwrap();

        let op1_drafts = db.get_drafts_by_operator(&op_id("op-1")).unwrap();
        assert_eq!(op1_drafts.len(), 2);

        let op2_drafts = db.get_drafts_by_operator(&op_id("op-2")).unwrap();
        assert_eq!(op2_drafts.len(), 1);
    }

    // ------------------------------------------------------------------------------------------
    // `DRAFT_ROW`. The three patterns from task 04.
    // ------------------------------------------------------------------------------------------

    fn setup_db_with_operator() -> Database {
        // `drafts.operator_id` is a real foreign key. The fixture names an operator rather than
        // blanking the column, because a blank there is indistinguishable from six other absences.
        let db = setup_db();
        db.save_operator(&OperatorRow {
            id: op_id("operator-id-column"),
            code: "C1".to_string(),
            employee_id: None,
            employee_number: None,
            name: OperatorName::new("Referent", None::<&str>).unwrap(),
            role: OperatorRole::Cashier,
            department: None,
            position: None,
            permissions: None,
            is_active: true,
        })
        .expect("a referent operator");
        db
    }

    /// A draft whose every column holds a value found nowhere else in the row.
    fn a_draft_with_no_two_columns_alike() -> DraftRow {
        DraftRow {
            id: "id-column".to_string(),
            name: Some("name-column".to_string()),
            items_json: r#"["items-json-column"]"#.to_string(),
            customer_id: Some("customer-id-column".to_string()),
            customer_name: Some("customer-name-column".to_string()),
            discount_json: Some(r#"{"discount":"json-column"}"#.to_string()),
            notes: Some("notes-column".to_string()),
            created_at: "2026-08-24T10:00:00Z".to_string(),
            // Far enough ahead that `get_active_drafts` includes it.
            expires_at: Some("2099-01-01T00:00:00Z".to_string()),
            operator_id: Some(op_id("operator-id-column")),
            shift_id: Some("shift-id-column".to_string()),
        }
    }

    #[test]
    fn the_draft_mapping_names_every_column_it_writes_in_the_order_it_reads_them() {
        assert_eq!(
            DRAFT_ROW.reader().select_list(),
            "id, name, items_json, customer_id, customer_name, discount_json, notes, created_at, \
             expires_at, operator_id, shift_id"
        );
        assert_eq!(DRAFT_ROW.reader().width(), 11);
        assert_eq!(DRAFT_ROW.insert_column_names().count(), 11);
    }

    #[test]
    fn every_column_of_a_fully_distinct_draft_survives_the_round_trip() {
        let db = setup_db_with_operator();
        let written = a_draft_with_no_two_columns_alike();
        db.save_draft(&written).unwrap();

        let read = db
            .get_draft_by_id("id-column")
            .unwrap()
            .expect("the draft this test just wrote");
        assert_eq!(read.id, written.id);
        assert_eq!(read.name, written.name);
        assert_eq!(read.items_json, written.items_json);
        assert_eq!(read.customer_id, written.customer_id);
        assert_eq!(read.customer_name, written.customer_name);
        assert_eq!(read.discount_json, written.discount_json);
        assert_eq!(read.notes, written.notes);
        assert_eq!(read.created_at, written.created_at);
        assert_eq!(read.expires_at, written.expires_at);
        assert_eq!(read.operator_id, written.operator_id);
        assert_eq!(read.shift_id, written.shift_id);
    }

    #[test]
    fn save_draft_puts_each_value_in_the_column_that_carries_its_name() {
        let db = setup_db_with_operator();
        db.save_draft(&a_draft_with_no_two_columns_alike()).unwrap();

        let conn = db.connection();
        let conn = conn.lock();
        for (column, expected) in [
            ("id", "id-column"),
            ("name", "name-column"),
            ("items_json", r#"["items-json-column"]"#),
            ("customer_id", "customer-id-column"),
            ("customer_name", "customer-name-column"),
            ("discount_json", r#"{"discount":"json-column"}"#),
            ("notes", "notes-column"),
            ("created_at", "2026-08-24T10:00:00Z"),
            ("expires_at", "2099-01-01T00:00:00Z"),
            ("operator_id", "operator-id-column"),
            ("shift_id", "shift-id-column"),
        ] {
            let matched: bool = scalar(
                &conn,
                &format!("SELECT {column} = ?1 FROM drafts"),
                [expected],
            )
            .unwrap();
            assert!(matched, "the `{column}` column does not hold `{expected}`");
        }
    }

    #[test]
    fn a_second_write_of_the_same_draft_id_replaces_the_row() {
        let db = setup_db_with_operator();
        let first = a_draft_with_no_two_columns_alike();
        db.save_draft(&first).unwrap();
        db.save_draft(&DraftRow {
            notes: Some("second-write".to_string()),
            ..first
        })
        .unwrap();

        let rows: i64 = db.select_scalar("SELECT COUNT(*) FROM drafts", []).unwrap();
        assert_eq!(rows, 1, "the second write inserted rather than replaced");
        assert_eq!(
            db.get_draft_by_id("id-column")
                .unwrap()
                .unwrap()
                .notes
                .as_deref(),
            Some("second-write")
        );
    }

    /// One nullable draft column, blanked, and what must still hold of its neighbours.
    struct AbsentDraftColumn {
        column: &'static str,
        blank: fn(&mut DraftRow),
        assert_absent: fn(&DraftRow),
    }

    #[test]
    fn a_null_in_one_draft_column_reaches_that_columns_field_and_no_other() {
        let db = setup_db_with_operator();
        let full = a_draft_with_no_two_columns_alike();

        let cases = [
            AbsentDraftColumn {
                column: "name",
                blank: |row| row.name = None,
                assert_absent: |row| {
                    assert_eq!(row.name, None);
                    assert_eq!(row.notes.as_deref(), Some("notes-column"));
                },
            },
            AbsentDraftColumn {
                column: "customer_id",
                blank: |row| row.customer_id = None,
                assert_absent: |row| {
                    assert_eq!(row.customer_id, None);
                    assert_eq!(row.customer_name.as_deref(), Some("customer-name-column"));
                },
            },
            AbsentDraftColumn {
                column: "customer_name",
                blank: |row| row.customer_name = None,
                assert_absent: |row| {
                    assert_eq!(row.customer_name, None);
                    assert_eq!(row.customer_id.as_deref(), Some("customer-id-column"));
                },
            },
            AbsentDraftColumn {
                column: "discount_json",
                blank: |row| row.discount_json = None,
                assert_absent: |row| {
                    assert_eq!(row.discount_json, None);
                    assert_eq!(row.items_json, r#"["items-json-column"]"#);
                },
            },
            AbsentDraftColumn {
                column: "notes",
                blank: |row| row.notes = None,
                assert_absent: |row| {
                    assert_eq!(row.notes, None);
                    assert_eq!(row.created_at, "2026-08-24T10:00:00Z");
                },
            },
            AbsentDraftColumn {
                column: "expires_at",
                blank: |row| row.expires_at = None,
                assert_absent: |row| {
                    assert_eq!(row.expires_at, None);
                    assert!(row.operator_id.is_some());
                },
            },
            AbsentDraftColumn {
                column: "operator_id",
                blank: |row| row.operator_id = None,
                assert_absent: |row| {
                    assert_eq!(row.operator_id, None);
                    assert_eq!(row.shift_id.as_deref(), Some("shift-id-column"));
                },
            },
            AbsentDraftColumn {
                column: "shift_id",
                blank: |row| row.shift_id = None,
                assert_absent: |row| {
                    assert_eq!(row.shift_id, None);
                    assert!(row.operator_id.is_some());
                },
            },
        ];

        for case in cases {
            let mut written = full.clone();
            (case.blank)(&mut written);
            db.save_draft(&written).unwrap();

            let stored: Option<String> = db
                .select_scalar(&format!("SELECT {} FROM drafts", case.column), [])
                .unwrap();
            assert_eq!(stored, None, "`{}` was not written as NULL", case.column);

            let read = db
                .get_draft_by_id("id-column")
                .unwrap()
                .expect("the row this iteration wrote");
            (case.assert_absent)(&read);
        }
    }

    #[test]
    fn every_reader_of_the_drafts_table_returns_the_same_row() {
        let db = setup_db_with_operator();
        db.save_draft(&a_draft_with_no_two_columns_alike()).unwrap();

        let by_id = db.get_draft_by_id("id-column").unwrap().expect("by id");
        let active = db.get_active_drafts().unwrap();
        let by_operator = db
            .get_drafts_by_operator(&op_id("operator-id-column"))
            .unwrap();
        let by_shift = db.get_drafts_by_shift("shift-id-column").unwrap();

        assert_eq!(active.len(), 1);
        assert_eq!(by_operator.len(), 1);
        assert_eq!(by_shift.len(), 1);
        for (name, row) in [
            ("get_active_drafts", &active[0]),
            ("get_drafts_by_operator", &by_operator[0]),
            ("get_drafts_by_shift", &by_shift[0]),
        ] {
            assert_eq!(row.id, by_id.id, "{name}");
            assert_eq!(row.discount_json, by_id.discount_json, "{name}");
            assert_eq!(row.operator_id, by_id.operator_id, "{name}");
            assert_eq!(row.shift_id, by_id.shift_id, "{name}");
        }
    }
}
