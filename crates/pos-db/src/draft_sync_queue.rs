//! Draft Sync Queue Repository
//!
//! Handles the queue for draft operations that need to be synced to the backend.
//! Used when the terminal is offline - operations are queued here and synced
//! when connectivity is restored.

use chrono::Utc;
use rusqlite::{params, OptionalExtension, Result as SqliteResult};
use serde::{Deserialize, Serialize};

use super::Database;

/// Maximum retries for failed sync operations
pub const MAX_DRAFT_SYNC_RETRIES: i32 = 5;

/// Operation types for draft sync queue
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DraftSyncOperation {
    /// Create a new shared draft on backend
    Create,
    /// Mark draft as converted to transaction
    Convert,
    /// Delete/cancel draft on backend
    Delete,
}

impl DraftSyncOperation {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Create => "CREATE",
            Self::Convert => "CONVERT",
            Self::Delete => "DELETE",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "CONVERT" => Self::Convert,
            "DELETE" => Self::Delete,
            _ => Self::Create,
        }
    }
}

/// Sync status for queue items
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DraftQueueSyncStatus {
    /// Waiting to be synced
    Pending,
    /// Currently being synced
    Syncing,
    /// Successfully synced
    Synced,
    /// Failed to sync
    Failed,
}

impl DraftQueueSyncStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "PENDING",
            Self::Syncing => "SYNCING",
            Self::Synced => "SYNCED",
            Self::Failed => "FAILED",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "SYNCING" => Self::Syncing,
            "SYNCED" => Self::Synced,
            "FAILED" => Self::Failed,
            _ => Self::Pending,
        }
    }
}

/// Draft sync queue item
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DraftSyncQueueItem {
    /// Queue item ID
    pub id: String,
    /// Local draft ID reference
    pub local_draft_id: String,
    /// Operation type
    pub operation: String,
    /// Serialized request payload
    pub payload_json: String,
    /// Backend cart ID (set after successful create)
    pub server_id: Option<String>,
    /// Backend token (set after successful create)
    pub server_token: Option<String>,
    /// Transaction ID (for CONVERT operation)
    pub transaction_id: Option<String>,
    /// Current sync status
    pub sync_status: String,
    /// Number of retry attempts
    pub retry_count: i32,
    /// Last error message
    pub last_error: Option<String>,
    /// When the item was created
    pub created_at: String,
    /// When the last sync attempt was made
    pub last_attempt_at: Option<String>,
}

impl DraftSyncQueueItem {
    /// Creates a new queue item for a CREATE operation
    pub fn new_create(id: &str, local_draft_id: &str, payload_json: &str) -> Self {
        Self {
            id: id.to_string(),
            local_draft_id: local_draft_id.to_string(),
            operation: DraftSyncOperation::Create.as_str().to_string(),
            payload_json: payload_json.to_string(),
            server_id: None,
            server_token: None,
            transaction_id: None,
            sync_status: DraftQueueSyncStatus::Pending.as_str().to_string(),
            retry_count: 0,
            last_error: None,
            created_at: Utc::now().to_rfc3339(),
            last_attempt_at: None,
        }
    }

    /// Creates a new queue item for a CONVERT operation
    pub fn new_convert(
        id: &str,
        local_draft_id: &str,
        server_id: &str,
        transaction_id: &str,
    ) -> Self {
        Self {
            id: id.to_string(),
            local_draft_id: local_draft_id.to_string(),
            operation: DraftSyncOperation::Convert.as_str().to_string(),
            payload_json: "{}".to_string(),
            server_id: Some(server_id.to_string()),
            server_token: None,
            transaction_id: Some(transaction_id.to_string()),
            sync_status: DraftQueueSyncStatus::Pending.as_str().to_string(),
            retry_count: 0,
            last_error: None,
            created_at: Utc::now().to_rfc3339(),
            last_attempt_at: None,
        }
    }

    /// Creates a new queue item for a DELETE operation
    pub fn new_delete(id: &str, local_draft_id: &str, server_id: &str) -> Self {
        Self {
            id: id.to_string(),
            local_draft_id: local_draft_id.to_string(),
            operation: DraftSyncOperation::Delete.as_str().to_string(),
            payload_json: "{}".to_string(),
            server_id: Some(server_id.to_string()),
            server_token: None,
            transaction_id: None,
            sync_status: DraftQueueSyncStatus::Pending.as_str().to_string(),
            retry_count: 0,
            last_error: None,
            created_at: Utc::now().to_rfc3339(),
            last_attempt_at: None,
        }
    }

    /// Returns the operation type
    pub fn operation_type(&self) -> DraftSyncOperation {
        DraftSyncOperation::from_str(&self.operation)
    }

    /// Returns the sync status
    pub fn status(&self) -> DraftQueueSyncStatus {
        DraftQueueSyncStatus::from_str(&self.sync_status)
    }

    /// Returns true if the item has exceeded max retries
    pub fn has_exceeded_retries(&self) -> bool {
        self.retry_count >= MAX_DRAFT_SYNC_RETRIES
    }
}

impl Database {
    /// Adds an item to the draft sync queue
    pub fn queue_draft_sync(&self, item: &DraftSyncQueueItem) -> SqliteResult<()> {
        self.execute(
            r#"INSERT OR REPLACE INTO draft_sync_queue
               (id, local_draft_id, operation, payload_json, server_id, server_token,
                transaction_id, sync_status, retry_count, last_error, created_at, last_attempt_at)
               VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)"#,
            &[
                &item.id,
                &item.local_draft_id,
                &item.operation,
                &item.payload_json,
                &item.server_id,
                &item.server_token,
                &item.transaction_id,
                &item.sync_status,
                &item.retry_count.to_string(),
                &item.last_error,
                &item.created_at,
                &item.last_attempt_at,
            ],
        )?;
        Ok(())
    }

    /// Gets pending draft sync items (ordered by created_at)
    pub fn get_pending_draft_syncs(&self, limit: i32) -> SqliteResult<Vec<DraftSyncQueueItem>> {
        let conn = self.connection();
        let conn = conn.lock();

        let mut stmt = conn.prepare(
            r#"SELECT id, local_draft_id, operation, payload_json, server_id, server_token,
                      transaction_id, sync_status, retry_count, last_error, created_at, last_attempt_at
               FROM draft_sync_queue
               WHERE sync_status = 'PENDING' AND retry_count < ?1
               ORDER BY created_at ASC
               LIMIT ?2"#,
        )?;

        let rows = stmt.query_map(params![MAX_DRAFT_SYNC_RETRIES, limit], |row| {
            Ok(DraftSyncQueueItem {
                id: row.get(0)?,
                local_draft_id: row.get(1)?,
                operation: row.get(2)?,
                payload_json: row.get(3)?,
                server_id: row.get(4)?,
                server_token: row.get(5)?,
                transaction_id: row.get(6)?,
                sync_status: row.get(7)?,
                retry_count: row.get(8)?,
                last_error: row.get(9)?,
                created_at: row.get(10)?,
                last_attempt_at: row.get(11)?,
            })
        })?;

        rows.collect()
    }

    /// Gets a queue item by ID
    pub fn get_draft_sync_item(&self, id: &str) -> SqliteResult<Option<DraftSyncQueueItem>> {
        let conn = self.connection();
        let conn = conn.lock();

        conn.query_row(
            r#"SELECT id, local_draft_id, operation, payload_json, server_id, server_token,
                      transaction_id, sync_status, retry_count, last_error, created_at, last_attempt_at
               FROM draft_sync_queue WHERE id = ?1"#,
            [id],
            |row| {
                Ok(DraftSyncQueueItem {
                    id: row.get(0)?,
                    local_draft_id: row.get(1)?,
                    operation: row.get(2)?,
                    payload_json: row.get(3)?,
                    server_id: row.get(4)?,
                    server_token: row.get(5)?,
                    transaction_id: row.get(6)?,
                    sync_status: row.get(7)?,
                    retry_count: row.get(8)?,
                    last_error: row.get(9)?,
                    created_at: row.get(10)?,
                    last_attempt_at: row.get(11)?,
                })
            },
        )
        .optional()
    }

    /// Gets queue items for a local draft ID
    pub fn get_draft_sync_items_by_local_id(
        &self,
        local_draft_id: &str,
    ) -> SqliteResult<Vec<DraftSyncQueueItem>> {
        let conn = self.connection();
        let conn = conn.lock();

        let mut stmt = conn.prepare(
            r#"SELECT id, local_draft_id, operation, payload_json, server_id, server_token,
                      transaction_id, sync_status, retry_count, last_error, created_at, last_attempt_at
               FROM draft_sync_queue WHERE local_draft_id = ?1
               ORDER BY created_at ASC"#,
        )?;

        let rows = stmt.query_map([local_draft_id], |row| {
            Ok(DraftSyncQueueItem {
                id: row.get(0)?,
                local_draft_id: row.get(1)?,
                operation: row.get(2)?,
                payload_json: row.get(3)?,
                server_id: row.get(4)?,
                server_token: row.get(5)?,
                transaction_id: row.get(6)?,
                sync_status: row.get(7)?,
                retry_count: row.get(8)?,
                last_error: row.get(9)?,
                created_at: row.get(10)?,
                last_attempt_at: row.get(11)?,
            })
        })?;

        rows.collect()
    }

    /// Marks a queue item as syncing
    pub fn mark_draft_sync_syncing(&self, id: &str) -> SqliteResult<bool> {
        let now = Utc::now().to_rfc3339();
        let updated = self.execute(
            "UPDATE draft_sync_queue SET sync_status = 'SYNCING', last_attempt_at = ?1 WHERE id = ?2",
            &[&now, &id.to_string()],
        )?;
        Ok(updated > 0)
    }

    /// Marks a queue item as completed and updates server info
    pub fn mark_draft_sync_complete(
        &self,
        id: &str,
        server_id: Option<&str>,
        server_token: Option<&str>,
    ) -> SqliteResult<bool> {
        let now = Utc::now().to_rfc3339();
        let updated = self.execute(
            r#"UPDATE draft_sync_queue
               SET sync_status = 'SYNCED', server_id = COALESCE(?1, server_id),
                   server_token = COALESCE(?2, server_token), last_attempt_at = ?3
               WHERE id = ?4"#,
            &[
                &server_id.map(|s| s.to_string()),
                &server_token.map(|s| s.to_string()),
                &now,
                &id.to_string(),
            ],
        )?;
        Ok(updated > 0)
    }

    /// Marks a queue item as failed with error
    pub fn mark_draft_sync_failed(&self, id: &str, error: &str) -> SqliteResult<bool> {
        let now = Utc::now().to_rfc3339();
        let updated = self.execute(
            r#"UPDATE draft_sync_queue
               SET sync_status = 'PENDING', retry_count = retry_count + 1,
                   last_error = ?1, last_attempt_at = ?2
               WHERE id = ?3"#,
            &[&error.to_string(), &now, &id.to_string()],
        )?;
        Ok(updated > 0)
    }

    /// Deletes a queue item
    pub fn delete_draft_sync_item(&self, id: &str) -> SqliteResult<bool> {
        let deleted = self.execute("DELETE FROM draft_sync_queue WHERE id = ?1", &[&id])?;
        Ok(deleted > 0)
    }

    /// Deletes completed queue items
    pub fn cleanup_completed_draft_syncs(&self) -> SqliteResult<usize> {
        self.execute(
            "DELETE FROM draft_sync_queue WHERE sync_status = 'SYNCED'",
            &[],
        )
    }

    /// Deletes queue items for a local draft
    pub fn delete_draft_sync_items_by_local_id(&self, local_draft_id: &str) -> SqliteResult<usize> {
        self.execute(
            "DELETE FROM draft_sync_queue WHERE local_draft_id = ?1",
            &[&local_draft_id],
        )
    }

    /// Counts pending draft sync items
    pub fn count_pending_draft_syncs(&self) -> SqliteResult<i64> {
        let conn = self.connection();
        let conn = conn.lock();

        conn.query_row(
            "SELECT COUNT(*) FROM draft_sync_queue WHERE sync_status = 'PENDING'",
            [],
            |row| row.get(0),
        )
    }

    /// Gets failed draft sync items (exceeded retries)
    pub fn get_failed_draft_syncs(&self) -> SqliteResult<Vec<DraftSyncQueueItem>> {
        let conn = self.connection();
        let conn = conn.lock();

        let mut stmt = conn.prepare(
            r#"SELECT id, local_draft_id, operation, payload_json, server_id, server_token,
                      transaction_id, sync_status, retry_count, last_error, created_at, last_attempt_at
               FROM draft_sync_queue
               WHERE retry_count >= ?1
               ORDER BY created_at ASC"#,
        )?;

        let rows = stmt.query_map(params![MAX_DRAFT_SYNC_RETRIES], |row| {
            Ok(DraftSyncQueueItem {
                id: row.get(0)?,
                local_draft_id: row.get(1)?,
                operation: row.get(2)?,
                payload_json: row.get(3)?,
                server_id: row.get(4)?,
                server_token: row.get(5)?,
                transaction_id: row.get(6)?,
                sync_status: row.get(7)?,
                retry_count: row.get(8)?,
                last_error: row.get(9)?,
                created_at: row.get(10)?,
                last_attempt_at: row.get(11)?,
            })
        })?;

        rows.collect()
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
    fn test_queue_and_get_draft_sync() {
        let db = setup_db();
        let item = DraftSyncQueueItem::new_create("q1", "draft-1", r#"{"test":true}"#);

        db.queue_draft_sync(&item).unwrap();

        let found = db.get_draft_sync_item("q1").unwrap();
        assert!(found.is_some());
        let found = found.unwrap();
        assert_eq!(found.local_draft_id, "draft-1");
        assert_eq!(found.operation, "CREATE");
        assert_eq!(found.sync_status, "PENDING");
    }

    #[test]
    fn test_get_pending_draft_syncs() {
        let db = setup_db();

        db.queue_draft_sync(&DraftSyncQueueItem::new_create("q1", "d1", "{}")).unwrap();
        db.queue_draft_sync(&DraftSyncQueueItem::new_create("q2", "d2", "{}")).unwrap();

        let pending = db.get_pending_draft_syncs(10).unwrap();
        assert_eq!(pending.len(), 2);
    }

    #[test]
    fn test_mark_draft_sync_complete() {
        let db = setup_db();
        let item = DraftSyncQueueItem::new_create("q1", "draft-1", "{}");
        db.queue_draft_sync(&item).unwrap();

        db.mark_draft_sync_syncing("q1").unwrap();
        db.mark_draft_sync_complete("q1", Some("server-123"), Some("TOK123")).unwrap();

        let found = db.get_draft_sync_item("q1").unwrap().unwrap();
        assert_eq!(found.sync_status, "SYNCED");
        assert_eq!(found.server_id, Some("server-123".to_string()));
        assert_eq!(found.server_token, Some("TOK123".to_string()));
    }

    #[test]
    fn test_mark_draft_sync_failed() {
        let db = setup_db();
        let item = DraftSyncQueueItem::new_create("q1", "draft-1", "{}");
        db.queue_draft_sync(&item).unwrap();

        db.mark_draft_sync_failed("q1", "Network error").unwrap();

        let found = db.get_draft_sync_item("q1").unwrap().unwrap();
        assert_eq!(found.sync_status, "PENDING");
        assert_eq!(found.retry_count, 1);
        assert_eq!(found.last_error, Some("Network error".to_string()));
    }

    #[test]
    fn test_count_pending_draft_syncs() {
        let db = setup_db();

        db.queue_draft_sync(&DraftSyncQueueItem::new_create("q1", "d1", "{}")).unwrap();
        db.queue_draft_sync(&DraftSyncQueueItem::new_create("q2", "d2", "{}")).unwrap();

        let count = db.count_pending_draft_syncs().unwrap();
        assert_eq!(count, 2);
    }

    #[test]
    fn test_cleanup_completed_draft_syncs() {
        let db = setup_db();

        db.queue_draft_sync(&DraftSyncQueueItem::new_create("q1", "d1", "{}")).unwrap();
        db.queue_draft_sync(&DraftSyncQueueItem::new_create("q2", "d2", "{}")).unwrap();

        db.mark_draft_sync_complete("q1", None, None).unwrap();

        let cleaned = db.cleanup_completed_draft_syncs().unwrap();
        assert_eq!(cleaned, 1);

        let count = db.count_pending_draft_syncs().unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn test_draft_sync_operation() {
        assert_eq!(DraftSyncOperation::Create.as_str(), "CREATE");
        assert_eq!(DraftSyncOperation::Convert.as_str(), "CONVERT");
        assert_eq!(DraftSyncOperation::Delete.as_str(), "DELETE");

        assert_eq!(DraftSyncOperation::from_str("CREATE"), DraftSyncOperation::Create);
        assert_eq!(DraftSyncOperation::from_str("CONVERT"), DraftSyncOperation::Convert);
        assert_eq!(DraftSyncOperation::from_str("DELETE"), DraftSyncOperation::Delete);
        assert_eq!(DraftSyncOperation::from_str("UNKNOWN"), DraftSyncOperation::Create);
    }

    #[test]
    fn test_new_convert_item() {
        let item = DraftSyncQueueItem::new_convert("q1", "draft-1", "server-1", "txn-1");
        assert_eq!(item.operation, "CONVERT");
        assert_eq!(item.server_id, Some("server-1".to_string()));
        assert_eq!(item.transaction_id, Some("txn-1".to_string()));
    }

    #[test]
    fn test_new_delete_item() {
        let item = DraftSyncQueueItem::new_delete("q1", "draft-1", "server-1");
        assert_eq!(item.operation, "DELETE");
        assert_eq!(item.server_id, Some("server-1".to_string()));
    }

    #[test]
    fn test_get_draft_sync_items_by_local_id() {
        let db = setup_db();

        db.queue_draft_sync(&DraftSyncQueueItem::new_create("q1", "draft-1", "{}")).unwrap();
        db.queue_draft_sync(&DraftSyncQueueItem::new_convert("q2", "draft-1", "s1", "t1")).unwrap();
        db.queue_draft_sync(&DraftSyncQueueItem::new_create("q3", "draft-2", "{}")).unwrap();

        let items = db.get_draft_sync_items_by_local_id("draft-1").unwrap();
        assert_eq!(items.len(), 2);
    }
}
