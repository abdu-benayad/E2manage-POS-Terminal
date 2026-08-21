//! Shared Drafts Repository
//!
//! Handles storage and retrieval of shared drafts (cloud-synced carts).
//! These are carts that have been synced to the backend and can be accessed
//! by any terminal in the same warehouse.

use std::str::FromStr;

use chrono::Utc;
use rusqlite::{params, OptionalExtension, Result as SqliteResult};
use serde::{Deserialize, Serialize};

use pos_models::{OperatorId, RecordedOperatorName};

use super::Database;
use crate::column;
use crate::parse::ParseError;

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

impl Database {
    /// Saves or updates a shared draft
    pub fn save_shared_draft(&self, draft: &SharedDraftRow) -> SqliteResult<()> {
        self.execute(
            r#"INSERT OR REPLACE INTO shared_drafts
               (id, token, name, items_json, customer_id, customer_name, discount_json,
                notes, item_count, total_amount, currency, warehouse_id, device_id,
                operator_id, operator_name, created_at, expires_at, fetched_at, sync_status)
               VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19)"#,
            &[
                &draft.id,
                &draft.token,
                &draft.name,
                &draft.items_json,
                &draft.customer_id,
                &draft.customer_name,
                &draft.discount_json,
                &draft.notes,
                &draft.item_count.to_string(),
                &draft.total_amount.to_string(),
                &draft.currency,
                &draft.warehouse_id,
                &draft.device_id,
                &draft.operator_id.as_ref().map(OperatorId::as_str),
                &draft.operator_name.as_ref().map(RecordedOperatorName::as_str),
                &draft.created_at,
                &draft.expires_at,
                &draft.fetched_at,
                &draft.sync_status,
            ],
        )?;
        Ok(())
    }

    /// Gets a shared draft by its token
    pub fn get_shared_draft_by_token(&self, token: &str) -> SqliteResult<Option<SharedDraftRow>> {
        let conn = self.connection();
        let conn = conn.lock();

        conn.query_row(
            r#"SELECT id, token, name, items_json, customer_id, customer_name, discount_json,
                      notes, item_count, total_amount, currency, warehouse_id, device_id,
                      operator_id, operator_name, created_at, expires_at, fetched_at, sync_status
               FROM shared_drafts WHERE token = ?1"#,
            [token],
            |row| {
                Ok(SharedDraftRow {
                    id: row.get(0)?,
                    token: row.get(1)?,
                    name: row.get(2)?,
                    items_json: row.get(3)?,
                    customer_id: row.get(4)?,
                    customer_name: row.get(5)?,
                    discount_json: row.get(6)?,
                    notes: row.get(7)?,
                    item_count: row.get(8)?,
                    total_amount: row.get(9)?,
                    currency: row.get(10)?,
                    warehouse_id: row.get(11)?,
                    device_id: row.get(12)?,
                    operator_id: column::optional_operator_id(row, 13)?,
                    operator_name: column::optional_recorded_operator_name(row, 14)?,
                    created_at: row.get(15)?,
                    expires_at: row.get(16)?,
                    fetched_at: row.get(17)?,
                    sync_status: row.get(18)?,
                })
            },
        )
        .optional()
    }

    /// Gets a shared draft by its ID
    pub fn get_shared_draft_by_id(&self, id: &str) -> SqliteResult<Option<SharedDraftRow>> {
        let conn = self.connection();
        let conn = conn.lock();

        conn.query_row(
            r#"SELECT id, token, name, items_json, customer_id, customer_name, discount_json,
                      notes, item_count, total_amount, currency, warehouse_id, device_id,
                      operator_id, operator_name, created_at, expires_at, fetched_at, sync_status
               FROM shared_drafts WHERE id = ?1"#,
            [id],
            |row| {
                Ok(SharedDraftRow {
                    id: row.get(0)?,
                    token: row.get(1)?,
                    name: row.get(2)?,
                    items_json: row.get(3)?,
                    customer_id: row.get(4)?,
                    customer_name: row.get(5)?,
                    discount_json: row.get(6)?,
                    notes: row.get(7)?,
                    item_count: row.get(8)?,
                    total_amount: row.get(9)?,
                    currency: row.get(10)?,
                    warehouse_id: row.get(11)?,
                    device_id: row.get(12)?,
                    operator_id: column::optional_operator_id(row, 13)?,
                    operator_name: column::optional_recorded_operator_name(row, 14)?,
                    created_at: row.get(15)?,
                    expires_at: row.get(16)?,
                    fetched_at: row.get(17)?,
                    sync_status: row.get(18)?,
                })
            },
        )
        .optional()
    }

    /// Lists all shared drafts for a warehouse
    pub fn list_shared_drafts(&self, warehouse_id: &str) -> SqliteResult<Vec<SharedDraftRow>> {
        let conn = self.connection();
        let conn = conn.lock();

        let mut stmt = conn.prepare(
            r#"SELECT id, token, name, items_json, customer_id, customer_name, discount_json,
                      notes, item_count, total_amount, currency, warehouse_id, device_id,
                      operator_id, operator_name, created_at, expires_at, fetched_at, sync_status
               FROM shared_drafts
               WHERE warehouse_id = ?1 AND sync_status != 'PENDING_DELETE'
               ORDER BY created_at DESC"#,
        )?;

        let rows = stmt.query_map([warehouse_id], |row| {
            Ok(SharedDraftRow {
                id: row.get(0)?,
                token: row.get(1)?,
                name: row.get(2)?,
                items_json: row.get(3)?,
                customer_id: row.get(4)?,
                customer_name: row.get(5)?,
                discount_json: row.get(6)?,
                notes: row.get(7)?,
                item_count: row.get(8)?,
                total_amount: row.get(9)?,
                currency: row.get(10)?,
                warehouse_id: row.get(11)?,
                device_id: row.get(12)?,
                operator_id: column::optional_operator_id(row, 13)?,
                operator_name: column::optional_recorded_operator_name(row, 14)?,
                created_at: row.get(15)?,
                expires_at: row.get(16)?,
                fetched_at: row.get(17)?,
                sync_status: row.get(18)?,
            })
        })?;

        rows.collect()
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
    /// is up-to-date.
    pub fn replace_shared_drafts_cache(
        &self,
        warehouse_id: &str,
        drafts: &[SharedDraftRow],
    ) -> SqliteResult<()> {
        let conn = self.connection();
        let conn = conn.lock();

        // Start transaction
        conn.execute("BEGIN TRANSACTION", [])?;

        // Delete existing drafts for this warehouse (except those pending sync)
        conn.execute(
            "DELETE FROM shared_drafts WHERE warehouse_id = ?1 AND sync_status = 'SYNCED'",
            params![warehouse_id],
        )?;

        // Insert new drafts
        let mut stmt = conn.prepare(
            r#"INSERT OR REPLACE INTO shared_drafts
               (id, token, name, items_json, customer_id, customer_name, discount_json,
                notes, item_count, total_amount, currency, warehouse_id, device_id,
                operator_id, operator_name, created_at, expires_at, fetched_at, sync_status)
               VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19)"#,
        )?;

        for draft in drafts {
            stmt.execute(params![
                draft.id,
                draft.token,
                draft.name,
                draft.items_json,
                draft.customer_id,
                draft.customer_name,
                draft.discount_json,
                draft.notes,
                draft.item_count,
                draft.total_amount,
                draft.currency,
                draft.warehouse_id,
                draft.device_id,
                draft.operator_id.as_ref().map(OperatorId::as_str),
                draft
                    .operator_name
                    .as_ref()
                    .map(RecordedOperatorName::as_str),
                draft.created_at,
                draft.expires_at,
                draft.fetched_at,
                draft.sync_status,
            ])?;
        }

        // Commit transaction
        conn.execute("COMMIT", [])?;

        Ok(())
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
}
