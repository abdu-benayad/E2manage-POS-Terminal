//! Shared Drafts Repository
//!
//! Handles storage and retrieval of shared drafts (cloud-synced carts).
//! These are carts that have been synced to the backend and can be accessed
//! by any terminal in the same warehouse.

use std::str::FromStr;

use chrono::Utc;
use rusqlite::{params, Result as SqliteResult};
use serde::{Deserialize, Serialize};

use pos_models::{OperatorId, RecordedOperatorName};

use super::Database;
use crate::column;
use crate::parse::ParseError;
use crate::projection::{self, OnConflict};
use crate::row_mapping;

/// Shared draft row from database
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SharedDraftRow {
    /// Backend cart UUID
    pub id: String,
    /// Unique 6-character token for recall
    pub token: String,
    /// Display name
    pub name: Option<String>,
    /// Serialized cart items
    pub items_json: String,
    /// Customer ID
    pub customer_id: Option<String>,
    /// Customer name
    pub customer_name: Option<String>,
    /// Serialized discount info
    pub discount_json: Option<String>,
    /// Cart notes
    pub notes: Option<String>,
    /// Number of line items
    pub item_count: i32,
    /// Total amount
    pub total_amount: f64,
    /// Currency code
    pub currency: String,
    /// Warehouse ID
    pub warehouse_id: String,
    /// Terminal device ID
    pub device_id: Option<String>,
    /// Operator who created the draft
    pub operator_id: Option<OperatorId>,
    /// The operator's name as recorded on the draft — one column, so one script.
    pub operator_name: Option<RecordedOperatorName>,
    /// When the draft was created
    pub created_at: String,
    /// When the draft expires
    pub expires_at: Option<String>,
    /// When we last fetched from server
    pub fetched_at: String,
    /// Sync status
    pub sync_status: String,
}

impl Default for SharedDraftRow {
    fn default() -> Self {
        Self {
            id: String::new(),
            token: String::new(),
            name: None,
            items_json: "[]".to_string(),
            customer_id: None,
            customer_name: None,
            discount_json: None,
            notes: None,
            item_count: 0,
            total_amount: 0.0,
            currency: "LYD".to_string(),
            warehouse_id: String::new(),
            device_id: None,
            operator_id: None,
            operator_name: None,
            created_at: Utc::now().to_rfc3339(),
            expires_at: None,
            fetched_at: Utc::now().to_rfc3339(),
            sync_status: "SYNCED".to_string(),
        }
    }
}

/// Sync status for shared drafts
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SharedDraftSyncStatus {
    /// Draft is synced with backend
    Synced,
    /// Draft needs to be converted on backend
    PendingConvert,
    /// Draft needs to be deleted on backend
    PendingDelete,
}

impl SharedDraftSyncStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Synced => "SYNCED",
            Self::PendingConvert => "PENDING_CONVERT",
            Self::PendingDelete => "PENDING_DELETE",
        }
    }
}

impl FromStr for SharedDraftSyncStatus {
    type Err = ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "SYNCED" => Ok(Self::Synced),
            "PENDING_CONVERT" => Ok(Self::PendingConvert),
            "PENDING_DELETE" => Ok(Self::PendingDelete),
            _ => Err(ParseError::SharedDraftSyncStatus(s.to_string())),
        }
    }
}

row_mapping! {
    /// Every column of `shared_drafts` this till reads or writes, declared once.
    ///
    /// Three reader closures and **two** `INSERT` lists, all nineteen names in the same order.
    ///
    /// The two writers had already drifted, in a way column affinity was hiding:
    /// `save_shared_draft` bound `item_count` and `total_amount` through `.to_string()`, into an
    /// `INTEGER` and a `REAL` column, while `replace_shared_drafts_cache` bound them natively.
    /// Measured before collapsing them — SQLite converts a well-formed numeric string on the way
    /// into an affinity column, and Rust's `f64::to_string` round-trips exactly, so both paths
    /// stored `typeof` `integer` and `real` with identical values, `0.1 + 0.2` included. So this
    /// is behaviour-preserving. But two writers of one table disagreeing about how to bind a money
    /// column *is* the drift, and only the affinity rule stood between it and `TEXT` in a `REAL`.
    ///
    /// `total_amount` stays an `f64`, and that is scope, not endorsement: it is
    /// `money-and-currency-in-the-till`, not a row task.
    pub const SHARED_DRAFT_ROW: RowMapping<SharedDraftRow> = for "shared_drafts" {
        id,
        token,
        name            via column::OPTIONAL_TEXT,
        items_json,
        customer_id     via column::OPTIONAL_TEXT,
        customer_name   via column::OPTIONAL_TEXT,
        discount_json   via column::OPTIONAL_TEXT,
        notes           via column::OPTIONAL_TEXT,
        item_count,
        total_amount,
        currency,
        warehouse_id,
        device_id       via column::OPTIONAL_TEXT,
        operator_id     via column::OPTIONAL_OPERATOR_ID,
        operator_name   via column::OPTIONAL_RECORDED_OPERATOR_NAME,
        created_at,
        expires_at      via column::OPTIONAL_TEXT,
        fetched_at,
        sync_status,
    } on_conflict OnConflict::Replace;
}

impl Database {
    /// Saves or updates a shared draft.
    pub fn save_shared_draft(&self, draft: &SharedDraftRow) -> SqliteResult<()> {
        self.insert(&SHARED_DRAFT_ROW, draft)?;
        Ok(())
    }

    /// Gets a shared draft by its recall token.
    pub fn get_shared_draft_by_token(&self, token: &str) -> SqliteResult<Option<SharedDraftRow>> {
        self.select_one(
            SHARED_DRAFT_ROW.reader(),
            "FROM shared_drafts WHERE token = ?1",
            [token],
        )
    }

    /// Gets a shared draft by its backend cart ID.
    pub fn get_shared_draft_by_id(&self, id: &str) -> SqliteResult<Option<SharedDraftRow>> {
        self.select_one(
            SHARED_DRAFT_ROW.reader(),
            "FROM shared_drafts WHERE id = ?1",
            [id],
        )
    }

    /// Lists one warehouse's shared drafts, newest first, excluding those queued for deletion.
    ///
    /// Not an expiry filter — `expires_at` is read but never used as a predicate here, unlike
    /// `drafts`. Written out because the two tables look alike and the difference is easy to
    /// "tidy" into existence.
    pub fn list_shared_drafts(&self, warehouse_id: &str) -> SqliteResult<Vec<SharedDraftRow>> {
        self.select_all(
            SHARED_DRAFT_ROW.reader(),
            "FROM shared_drafts
             WHERE warehouse_id = ?1 AND sync_status != 'PENDING_DELETE'
             ORDER BY created_at DESC",
            [warehouse_id],
        )
    }

    /// Deletes a shared draft by ID
    pub fn delete_shared_draft(&self, id: &str) -> SqliteResult<bool> {
        let deleted = self.execute("DELETE FROM shared_drafts WHERE id = ?1", &[&id])?;
        Ok(deleted > 0)
    }

    /// Deletes a shared draft by token
    pub fn delete_shared_draft_by_token(&self, token: &str) -> SqliteResult<bool> {
        let deleted = self.execute("DELETE FROM shared_drafts WHERE token = ?1", &[&token])?;
        Ok(deleted > 0)
    }

    /// Clears all shared drafts cache
    pub fn clear_shared_drafts_cache(&self) -> SqliteResult<usize> {
        self.execute("DELETE FROM shared_drafts", &[])
    }

    /// Updates the sync status of a shared draft
    pub fn update_shared_draft_status(
        &self,
        id: &str,
        status: SharedDraftSyncStatus,
    ) -> SqliteResult<bool> {
        let updated = self.execute(
            "UPDATE shared_drafts SET sync_status = ?1 WHERE id = ?2",
            &[&status.as_str().to_string(), &id.to_string()],
        )?;
        Ok(updated > 0)
    }

    /// Counts shared drafts for a warehouse
    pub fn count_shared_drafts(&self, warehouse_id: &str) -> SqliteResult<i64> {
        let conn = self.connection();
        let conn = conn.lock();

        conn.query_row(
            "SELECT COUNT(*) FROM shared_drafts WHERE warehouse_id = ?1 AND sync_status != 'PENDING_DELETE'",
            [warehouse_id],
            |row| row.get(0),
        )
    }

    /// Replaces the entire shared drafts cache with new data
    ///
    /// This is used when syncing with the backend to ensure the cache
    /// Replaces one warehouse's synced cache with `drafts`, atomically.
    ///
    /// # This used to leak an open transaction on failure
    ///
    /// It ran `BEGIN TRANSACTION` and `COMMIT` as bare statements with `?` between them. A failing
    /// insert returned early, the `COMMIT` never ran, and **the transaction stayed open on the
    /// shared connection** — every later write from any caller joined it, and the guard is held
    /// across the whole `Database`. Measured: after a constraint violation between an explicit
    /// `BEGIN` and its `COMMIT`, the connection still reports itself in a transaction.
    ///
    /// `unchecked_transaction` closes it by construction: the value rolls back when it is dropped,
    /// so an early return cannot leave one open. That is a behaviour fix, not a refactor.
    ///
    /// **No test in this file pins it, and that is stated rather than papered over.** Measured:
    /// removing the transaction from this function leaves the whole suite green, because no
    /// statement here can fail mid-batch — the table has no foreign keys, no `CHECK`, no `STRICT`,
    /// every `NOT NULL` column maps to a non-optional field, and `INSERT OR REPLACE` resolves
    /// every uniqueness constraint rather than erroring. The same RAII guard is exercised on the
    /// failure path by `projection::tests::a_bulk_write_that_fails_part_way_leaves_the_table_as_it
    /// _found_it`, which can force one through `OnConflict::Fail`.
    pub fn replace_shared_drafts_cache(
        &self,
        warehouse_id: &str,
        drafts: &[SharedDraftRow],
    ) -> SqliteResult<()> {
        let conn = self.connection();
        let conn = conn.lock();

        let transaction = conn.unchecked_transaction()?;
        conn.execute(
            "DELETE FROM shared_drafts WHERE warehouse_id = ?1 AND sync_status = 'SYNCED'",
            params![warehouse_id],
        )?;
        for draft in drafts {
            projection::write(&conn, &SHARED_DRAFT_ROW, draft)?;
        }
        transaction.commit()
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

    fn create_test_draft(id: &str, token: &str, warehouse_id: &str) -> SharedDraftRow {
        SharedDraftRow {
            id: id.to_string(),
            token: token.to_string(),
            name: Some(format!("Draft {}", id)),
            items_json: r#"[{"productId":"p1","quantity":2}]"#.to_string(),
            warehouse_id: warehouse_id.to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn test_save_and_get_shared_draft() {
        let db = setup_db();
        let draft = create_test_draft("draft-1", "ABC123", "wh-1");

        db.save_shared_draft(&draft).unwrap();

        let found = db.get_shared_draft_by_token("ABC123").unwrap();
        assert!(found.is_some());
        let found = found.unwrap();
        assert_eq!(found.id, "draft-1");
        assert_eq!(found.token, "ABC123");
    }

    #[test]
    fn test_get_shared_draft_by_id() {
        let db = setup_db();
        let draft = create_test_draft("draft-1", "ABC123", "wh-1");

        db.save_shared_draft(&draft).unwrap();

        let found = db.get_shared_draft_by_id("draft-1").unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().token, "ABC123");
    }

    #[test]
    fn test_list_shared_drafts() {
        let db = setup_db();

        // Add drafts to same warehouse
        db.save_shared_draft(&create_test_draft("d1", "TOK1", "wh-1"))
            .unwrap();
        db.save_shared_draft(&create_test_draft("d2", "TOK2", "wh-1"))
            .unwrap();
        db.save_shared_draft(&create_test_draft("d3", "TOK3", "wh-2"))
            .unwrap();

        let wh1_drafts = db.list_shared_drafts("wh-1").unwrap();
        let wh2_drafts = db.list_shared_drafts("wh-2").unwrap();

        assert_eq!(wh1_drafts.len(), 2);
        assert_eq!(wh2_drafts.len(), 1);
    }

    #[test]
    fn test_delete_shared_draft() {
        let db = setup_db();
        let draft = create_test_draft("draft-1", "ABC123", "wh-1");

        db.save_shared_draft(&draft).unwrap();
        let deleted = db.delete_shared_draft("draft-1").unwrap();
        assert!(deleted);

        let found = db.get_shared_draft_by_id("draft-1").unwrap();
        assert!(found.is_none());
    }

    #[test]
    fn test_delete_shared_draft_by_token() {
        let db = setup_db();
        let draft = create_test_draft("draft-1", "ABC123", "wh-1");

        db.save_shared_draft(&draft).unwrap();
        let deleted = db.delete_shared_draft_by_token("ABC123").unwrap();
        assert!(deleted);

        let found = db.get_shared_draft_by_token("ABC123").unwrap();
        assert!(found.is_none());
    }

    #[test]
    fn test_count_shared_drafts() {
        let db = setup_db();

        db.save_shared_draft(&create_test_draft("d1", "TOK1", "wh-1"))
            .unwrap();
        db.save_shared_draft(&create_test_draft("d2", "TOK2", "wh-1"))
            .unwrap();

        let count = db.count_shared_drafts("wh-1").unwrap();
        assert_eq!(count, 2);
    }

    #[test]
    fn test_update_shared_draft_status() {
        let db = setup_db();
        let draft = create_test_draft("draft-1", "ABC123", "wh-1");

        db.save_shared_draft(&draft).unwrap();
        db.update_shared_draft_status("draft-1", SharedDraftSyncStatus::PendingConvert)
            .unwrap();

        let found = db.get_shared_draft_by_id("draft-1").unwrap().unwrap();
        assert_eq!(found.sync_status, "PENDING_CONVERT");
    }

    #[test]
    fn test_clear_shared_drafts_cache() {
        let db = setup_db();

        db.save_shared_draft(&create_test_draft("d1", "TOK1", "wh-1"))
            .unwrap();
        db.save_shared_draft(&create_test_draft("d2", "TOK2", "wh-1"))
            .unwrap();

        let cleared = db.clear_shared_drafts_cache().unwrap();
        assert_eq!(cleared, 2);

        let count = db.count_shared_drafts("wh-1").unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn test_replace_shared_drafts_cache() {
        let db = setup_db();

        // Add initial drafts
        db.save_shared_draft(&create_test_draft("d1", "TOK1", "wh-1"))
            .unwrap();
        db.save_shared_draft(&create_test_draft("d2", "TOK2", "wh-1"))
            .unwrap();

        // Replace with new drafts
        let new_drafts = vec![
            create_test_draft("d3", "TOK3", "wh-1"),
            create_test_draft("d4", "TOK4", "wh-1"),
            create_test_draft("d5", "TOK5", "wh-1"),
        ];

        db.replace_shared_drafts_cache("wh-1", &new_drafts).unwrap();

        let drafts = db.list_shared_drafts("wh-1").unwrap();
        assert_eq!(drafts.len(), 3);

        // Old drafts should be gone
        assert!(db.get_shared_draft_by_token("TOK1").unwrap().is_none());
        // New drafts should exist
        assert!(db.get_shared_draft_by_token("TOK3").unwrap().is_some());
    }

    #[test]
    fn test_shared_draft_sync_status() {
        assert_eq!(SharedDraftSyncStatus::Synced.as_str(), "SYNCED");
        assert_eq!(
            SharedDraftSyncStatus::PendingConvert.as_str(),
            "PENDING_CONVERT"
        );
        assert_eq!(
            SharedDraftSyncStatus::PendingDelete.as_str(),
            "PENDING_DELETE"
        );

        assert_eq!(
            SharedDraftSyncStatus::from_str("SYNCED"),
            Ok(SharedDraftSyncStatus::Synced)
        );
        assert_eq!(
            SharedDraftSyncStatus::from_str("PENDING_CONVERT"),
            Ok(SharedDraftSyncStatus::PendingConvert)
        );
    }

    #[test]
    fn shared_draft_sync_status_rejects_unknown_rather_than_reporting_it_synced() {
        // Was: `assert_eq!(from_str("unknown"), Synced)` — the test pinned the defect. A draft
        // whose status cannot be read must not be reported as already synced.
        assert_eq!(
            SharedDraftSyncStatus::from_str("unknown"),
            Err(ParseError::SharedDraftSyncStatus("unknown".to_string()))
        );
    }

    // ------------------------------------------------------------------------------------------
    // `SHARED_DRAFT_ROW`. Two writers, so two column-identity tests.
    // ------------------------------------------------------------------------------------------

    /// A shared draft whose every column holds a value found nowhere else in the row.
    fn a_shared_draft_with_no_two_columns_alike() -> SharedDraftRow {
        SharedDraftRow {
            id: "id-column".to_string(),
            token: "token-column".to_string(),
            name: Some("name-column".to_string()),
            items_json: r#"["items-json-column"]"#.to_string(),
            customer_id: Some("customer-id-column".to_string()),
            customer_name: Some("customer-name-column".to_string()),
            discount_json: Some(r#"{"discount":"json-column"}"#.to_string()),
            notes: Some("notes-column".to_string()),
            item_count: 11,
            total_amount: 22.25,
            currency: "currency-column".to_string(),
            warehouse_id: "warehouse-id-column".to_string(),
            device_id: Some("device-id-column".to_string()),
            operator_id: Some(OperatorId::new("operator-id-column").unwrap()),
            operator_name: Some(RecordedOperatorName::new("operator-name-column").unwrap()),
            created_at: "2026-08-24T10:00:00Z".to_string(),
            expires_at: Some("2099-01-01T00:00:00Z".to_string()),
            fetched_at: "2026-08-24T11:00:00Z".to_string(),
            // Not `SYNCED`: that is the column's SQL `DEFAULT`, and `replace_shared_drafts_cache`
            // deletes exactly the `SYNCED` rows, so the two tests below would interfere.
            sync_status: "PENDING_CONVERT".to_string(),
        }
    }

    #[test]
    fn the_shared_draft_mapping_names_every_column_in_the_order_it_reads_them() {
        assert_eq!(
            SHARED_DRAFT_ROW.reader().select_list(),
            "id, token, name, items_json, customer_id, customer_name, discount_json, notes, \
             item_count, total_amount, currency, warehouse_id, device_id, operator_id, \
             operator_name, created_at, expires_at, fetched_at, sync_status"
        );
        assert_eq!(SHARED_DRAFT_ROW.reader().width(), 19);
        assert_eq!(SHARED_DRAFT_ROW.insert_column_names().count(), 19);
    }

    #[test]
    fn every_column_of_a_fully_distinct_shared_draft_survives_the_round_trip() {
        let db = setup_db();
        let written = a_shared_draft_with_no_two_columns_alike();
        db.save_shared_draft(&written).unwrap();

        let read = db
            .get_shared_draft_by_id("id-column")
            .unwrap()
            .expect("the shared draft this test just wrote");
        assert_eq!(read.id, written.id);
        assert_eq!(read.token, written.token);
        assert_eq!(read.name, written.name);
        assert_eq!(read.items_json, written.items_json);
        assert_eq!(read.customer_id, written.customer_id);
        assert_eq!(read.customer_name, written.customer_name);
        assert_eq!(read.discount_json, written.discount_json);
        assert_eq!(read.notes, written.notes);
        assert_eq!(read.item_count, written.item_count);
        assert_eq!(read.total_amount, written.total_amount);
        assert_eq!(read.currency, written.currency);
        assert_eq!(read.warehouse_id, written.warehouse_id);
        assert_eq!(read.device_id, written.device_id);
        assert_eq!(read.operator_id, written.operator_id);
        assert_eq!(read.operator_name, written.operator_name);
        assert_eq!(read.created_at, written.created_at);
        assert_eq!(read.expires_at, written.expires_at);
        assert_eq!(read.fetched_at, written.fetched_at);
        assert_eq!(read.sync_status, written.sync_status);
    }

    /// Reads every column of the single stored shared draft back **by name**.
    fn assert_every_shared_draft_column_holds_its_own_value(db: &Database) {
        let conn = db.connection();
        let conn = conn.lock();
        for (column, expected) in [
            ("id", "id-column"),
            ("token", "token-column"),
            ("name", "name-column"),
            ("items_json", r#"["items-json-column"]"#),
            ("customer_id", "customer-id-column"),
            ("customer_name", "customer-name-column"),
            ("discount_json", r#"{"discount":"json-column"}"#),
            ("notes", "notes-column"),
            ("currency", "currency-column"),
            ("warehouse_id", "warehouse-id-column"),
            ("device_id", "device-id-column"),
            ("operator_id", "operator-id-column"),
            ("operator_name", "operator-name-column"),
            ("created_at", "2026-08-24T10:00:00Z"),
            ("expires_at", "2099-01-01T00:00:00Z"),
            ("fetched_at", "2026-08-24T11:00:00Z"),
            ("sync_status", "PENDING_CONVERT"),
        ] {
            let matched: bool = conn
                .query_row(
                    &format!("SELECT {column} = ?1 FROM shared_drafts"),
                    [expected],
                    |row| row.get(0),
                )
                .unwrap();
            assert!(matched, "the `{column}` column does not hold `{expected}`");
        }

        // The two numeric columns, and the `typeof` each was stored under. This is the assertion
        // that would have caught the two writers disagreeing: one bound them through `.to_string()`
        // and only SQLite's affinity rule kept `TEXT` out of a `REAL` column.
        let (count, count_type): (i64, String) = conn
            .query_row(
                "SELECT item_count, typeof(item_count) FROM shared_drafts",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!((count, count_type.as_str()), (11, "integer"));
        let (amount, amount_type): (f64, String) = conn
            .query_row(
                "SELECT total_amount, typeof(total_amount) FROM shared_drafts",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!((amount, amount_type.as_str()), (22.25, "real"));
    }

    #[test]
    fn save_shared_draft_puts_each_value_in_the_column_that_carries_its_name() {
        let db = setup_db();
        db.save_shared_draft(&a_shared_draft_with_no_two_columns_alike())
            .unwrap();
        assert_every_shared_draft_column_holds_its_own_value(&db);
    }

    #[test]
    fn replace_shared_drafts_cache_puts_each_value_in_the_column_that_carries_its_name() {
        // The second writer. It had its own nineteen-column list, and it bound two of them
        // differently from the first.
        let db = setup_db();
        db.replace_shared_drafts_cache(
            "warehouse-id-column",
            &[a_shared_draft_with_no_two_columns_alike()],
        )
        .unwrap();
        assert_every_shared_draft_column_holds_its_own_value(&db);
    }

    #[test]
    fn a_cache_replacement_removes_the_synced_rows_and_keeps_the_rest() {
        // What this asserts is the *state* a replacement leaves: the synced rows for the
        // warehouse are gone, the pending ones are not, and the batch's rows are in.
        //
        // # It does not test the transaction, and the name used to say it did
        //
        // It was called `..._is_one_unit_and_leaves_no_transaction_open` until the mutation pass:
        // **deleting `unchecked_transaction` and `commit` from `replace_shared_drafts_cache`
        // entirely leaves this test, and all 175 others, green.** Nothing here can tell a
        // transaction from no transaction, because the difference is only observable when a
        // statement in the middle fails — and no statement here can.
        //
        // The fix that landed is still real: the old code ran `BEGIN TRANSACTION` and `COMMIT` as
        // bare statements, so a failing insert returned early, the `COMMIT` never ran, and the
        // transaction stayed open on the shared connection for every later caller. Measured
        // directly against SQLite. `unchecked_transaction` closes that by construction. But the
        // guarantee is pinned by
        // `projection::tests::a_bulk_write_that_fails_part_way_leaves_the_table_as_it_found_it`,
        // which exercises the same RAII guard and *can* force a failure, not by this test.
        //
        // # Why no failure is reachable here
        //
        // The first version of it built a batch whose second row repeated a `UNIQUE` token,
        // expecting a mid-batch failure, and closed with `assert!(outcome.is_ok() ||
        // outcome.is_err())` — a tautology, which is what hid the fact that the batch never fails.
        // Measured: `INSERT OR REPLACE` resolves **every** uniqueness constraint, not only the
        // primary key, so the conflicting row is deleted and replaced without error.
        //
        // Nothing else on this table can fail mid-batch either: no foreign keys, no `CHECK`, no
        // `STRICT`, and every `NOT NULL` column maps to a non-optional Rust field. So the failure
        // arm is unreachable from `SharedDraftRow` and this test cannot exercise it. The same RAII
        // guard is exercised on the failure path by
        // `projection::tests::a_bulk_write_that_fails_part_way_leaves_the_table_as_it_found_it`,
        // which can force one through `OnConflict::Fail`.
        let db = setup_db();

        // A synced row the replacement must remove, and a pending one it must leave alone.
        db.save_shared_draft(&SharedDraftRow {
            id: "stale-synced".to_string(),
            token: "stale-token".to_string(),
            sync_status: "SYNCED".to_string(),
            ..a_shared_draft_with_no_two_columns_alike()
        })
        .unwrap();
        db.save_shared_draft(&SharedDraftRow {
            id: "kept-pending".to_string(),
            token: "kept-token".to_string(),
            ..a_shared_draft_with_no_two_columns_alike()
        })
        .unwrap();

        db.replace_shared_drafts_cache(
            "warehouse-id-column",
            &[a_shared_draft_with_no_two_columns_alike()],
        )
        .unwrap();

        let ids: Vec<String> = db
            .select_all(
                SHARED_DRAFT_ROW.reader(),
                "FROM shared_drafts ORDER BY id",
                [],
            )
            .unwrap()
            .into_iter()
            .map(|row| row.id)
            .collect();
        assert_eq!(
            ids,
            ["id-column", "kept-pending"],
            "the delete and the inserts did not happen as one unit"
        );

        // A later write still lands. This does not distinguish a leaked transaction from none —
        // see the note above — but it is the shape a leak would break first, and it costs nothing.
        db.save_shared_draft(&SharedDraftRow {
            id: "written-afterwards".to_string(),
            token: "another-token".to_string(),
            ..a_shared_draft_with_no_two_columns_alike()
        })
        .expect("a write after the batch must not be blocked by a leaked transaction");
        let later: i64 = db
            .select_scalar(
                "SELECT COUNT(*) FROM shared_drafts WHERE id = ?1",
                ["written-afterwards"],
            )
            .unwrap();
        assert_eq!(later, 1, "the later write did not land");
    }

    #[test]
    fn a_second_write_of_the_same_shared_draft_id_replaces_the_row() {
        let db = setup_db();
        let first = a_shared_draft_with_no_two_columns_alike();
        db.save_shared_draft(&first).unwrap();
        db.save_shared_draft(&SharedDraftRow {
            notes: Some("second-write".to_string()),
            ..first
        })
        .unwrap();

        let rows: i64 = db
            .select_scalar("SELECT COUNT(*) FROM shared_drafts", [])
            .unwrap();
        assert_eq!(rows, 1, "the second write inserted rather than replaced");
    }

    /// One nullable column, blanked, and what must still hold of its neighbours.
    ///
    /// A named struct rather than a tuple of two function pointers, for the reason
    /// `clippy::type_complexity` gives and `operators.rs` already settled: the three fields are
    /// what the case *is*.
    struct AbsentSharedDraftColumn {
        column: &'static str,
        blank: fn(&mut SharedDraftRow),
        assert_absent: fn(&SharedDraftRow),
    }

    #[test]
    fn a_null_in_one_shared_draft_column_reaches_that_columns_field_and_no_other() {
        let db = setup_db();
        let full = a_shared_draft_with_no_two_columns_alike();

        let cases = [
            AbsentSharedDraftColumn {
                column: "name",
                blank: |row| row.name = None,
                assert_absent: |row| {
                    assert_eq!(row.name, None);
                    assert_eq!(row.notes.as_deref(), Some("notes-column"));
                },
            },
            AbsentSharedDraftColumn {
                column: "customer_id",
                blank: |row| row.customer_id = None,
                assert_absent: |row| {
                    assert_eq!(row.customer_id, None);
                    assert_eq!(row.customer_name.as_deref(), Some("customer-name-column"));
                },
            },
            AbsentSharedDraftColumn {
                column: "customer_name",
                blank: |row| row.customer_name = None,
                assert_absent: |row| {
                    assert_eq!(row.customer_name, None);
                    assert_eq!(row.customer_id.as_deref(), Some("customer-id-column"));
                },
            },
            AbsentSharedDraftColumn {
                column: "discount_json",
                blank: |row| row.discount_json = None,
                assert_absent: |row| {
                    assert_eq!(row.discount_json, None);
                    assert_eq!(row.notes.as_deref(), Some("notes-column"));
                },
            },
            AbsentSharedDraftColumn {
                column: "device_id",
                blank: |row| row.device_id = None,
                assert_absent: |row| {
                    assert_eq!(row.device_id, None);
                    assert!(row.operator_id.is_some());
                },
            },
            AbsentSharedDraftColumn {
                column: "operator_id",
                blank: |row| row.operator_id = None,
                assert_absent: |row| {
                    assert_eq!(row.operator_id, None);
                    assert!(
                        row.operator_name.is_some(),
                        "the neighbouring name went with it"
                    );
                },
            },
            AbsentSharedDraftColumn {
                column: "operator_name",
                blank: |row| row.operator_name = None,
                assert_absent: |row| {
                    assert_eq!(row.operator_name, None);
                    assert!(
                        row.operator_id.is_some(),
                        "the neighbouring id went with it"
                    );
                },
            },
        ];

        for case in cases {
            let mut written = full.clone();
            (case.blank)(&mut written);
            db.save_shared_draft(&written).unwrap();

            let stored: Option<String> = db
                .select_scalar(&format!("SELECT {} FROM shared_drafts", case.column), [])
                .unwrap();
            assert_eq!(stored, None, "`{}` was not written as NULL", case.column);

            let read = db
                .get_shared_draft_by_id("id-column")
                .unwrap()
                .expect("the row this iteration wrote");
            (case.assert_absent)(&read);
        }
    }

    #[test]
    fn every_reader_of_the_shared_drafts_table_returns_the_same_row() {
        let db = setup_db();
        db.save_shared_draft(&a_shared_draft_with_no_two_columns_alike())
            .unwrap();

        let by_id = db.get_shared_draft_by_id("id-column").unwrap().unwrap();
        let by_token = db
            .get_shared_draft_by_token("token-column")
            .unwrap()
            .unwrap();
        let listed = db.list_shared_drafts("warehouse-id-column").unwrap();

        assert_eq!(listed.len(), 1);
        for (name, row) in [("by_token", &by_token), ("list", &listed[0])] {
            assert_eq!(row.id, by_id.id, "{name}");
            assert_eq!(row.total_amount, by_id.total_amount, "{name}");
            assert_eq!(row.operator_id, by_id.operator_id, "{name}");
            assert_eq!(row.operator_name, by_id.operator_name, "{name}");
            assert_eq!(row.sync_status, by_id.sync_status, "{name}");
        }
    }
}
