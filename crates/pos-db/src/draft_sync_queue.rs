//! Draft Sync Queue Repository
//!
//! Handles the queue for draft operations that need to be synced to the backend.
//! Used when the terminal is offline - operations are queued here and synced
//! when connectivity is restored.

use std::str::FromStr;

use chrono::Utc;
use rusqlite::{params, Result as SqliteResult};
use serde::{Deserialize, Serialize};

use super::Database;
use crate::column;
use crate::parse::ParseError;
use crate::projection::OnConflict;
use crate::row_mapping;

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
}

impl FromStr for DraftSyncOperation {
    type Err = ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "CREATE" => Ok(Self::Create),
            "CONVERT" => Ok(Self::Convert),
            "DELETE" => Ok(Self::Delete),
            _ => Err(ParseError::DraftSyncOperation(s.to_string())),
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
}

impl FromStr for DraftQueueSyncStatus {
    type Err = ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "PENDING" => Ok(Self::Pending),
            "SYNCING" => Ok(Self::Syncing),
            "SYNCED" => Ok(Self::Synced),
            "FAILED" => Ok(Self::Failed),
            _ => Err(ParseError::DraftQueueSyncStatus(s.to_string())),
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

    /// Returns the operation type, or names the stored value if it is not one.
    pub fn operation_type(&self) -> Result<DraftSyncOperation, ParseError> {
        self.operation.parse()
    }

    /// Returns the sync status, or names the stored value if it is not one.
    pub fn status(&self) -> Result<DraftQueueSyncStatus, ParseError> {
        self.sync_status.parse()
    }

    /// Returns true if the item has exceeded max retries
    pub fn has_exceeded_retries(&self) -> bool {
        self.retry_count >= MAX_DRAFT_SYNC_RETRIES
    }
}

row_mapping! {
    /// Every column of `draft_sync_queue` this till reads or writes, declared once.
    ///
    /// Four reader closures and one `INSERT` list, all twelve names in the same order. The writer
    /// bound `retry_count` through `.to_string()` into an `INTEGER` column, which affinity
    /// converted; the mapping binds it as the integer it is.
    pub const DRAFT_SYNC_QUEUE_ITEM_ROW: RowMapping<DraftSyncQueueItem> = for "draft_sync_queue" {
        id,
        local_draft_id,
        operation,
        payload_json,
        server_id       via column::OPTIONAL_TEXT,
        server_token    via column::OPTIONAL_TEXT,
        transaction_id  via column::OPTIONAL_TEXT,
        sync_status,
        retry_count,
        last_error      via column::OPTIONAL_TEXT,
        created_at,
        last_attempt_at via column::OPTIONAL_TEXT,
    } on_conflict OnConflict::Replace;
}

impl Database {
    /// Queues one draft operation for the next sync.
    pub fn queue_draft_sync(&self, item: &DraftSyncQueueItem) -> SqliteResult<()> {
        self.insert(&DRAFT_SYNC_QUEUE_ITEM_ROW, item)?;
        Ok(())
    }

    /// Gets the queued operations still waiting to reach the server, oldest first.
    pub fn get_pending_draft_syncs(&self, limit: i32) -> SqliteResult<Vec<DraftSyncQueueItem>> {
        self.select_all(
            DRAFT_SYNC_QUEUE_ITEM_ROW.reader(),
            "FROM draft_sync_queue
             WHERE sync_status = 'PENDING' AND retry_count < ?1
             ORDER BY created_at ASC
             LIMIT ?2",
            params![MAX_DRAFT_SYNC_RETRIES, limit],
        )
    }

    /// Gets a queue item by ID.
    pub fn get_draft_sync_item(&self, id: &str) -> SqliteResult<Option<DraftSyncQueueItem>> {
        self.select_one(
            DRAFT_SYNC_QUEUE_ITEM_ROW.reader(),
            "FROM draft_sync_queue WHERE id = ?1",
            [id],
        )
    }

    /// Gets every queue item raised against one local draft, oldest first.
    pub fn get_draft_sync_items_by_local_id(
        &self,
        local_draft_id: &str,
    ) -> SqliteResult<Vec<DraftSyncQueueItem>> {
        self.select_all(
            DRAFT_SYNC_QUEUE_ITEM_ROW.reader(),
            "FROM draft_sync_queue WHERE local_draft_id = ?1 ORDER BY created_at ASC",
            [local_draft_id],
        )
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

    /// Gets the queue items that have exhausted their retries, oldest first.
    pub fn get_failed_draft_syncs(&self) -> SqliteResult<Vec<DraftSyncQueueItem>> {
        self.select_all(
            DRAFT_SYNC_QUEUE_ITEM_ROW.reader(),
            "FROM draft_sync_queue WHERE retry_count >= ?1 ORDER BY created_at ASC",
            [MAX_DRAFT_SYNC_RETRIES],
        )
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

        db.queue_draft_sync(&DraftSyncQueueItem::new_create("q1", "d1", "{}"))
            .unwrap();
        db.queue_draft_sync(&DraftSyncQueueItem::new_create("q2", "d2", "{}"))
            .unwrap();

        let pending = db.get_pending_draft_syncs(10).unwrap();
        assert_eq!(pending.len(), 2);
    }

    #[test]
    fn test_mark_draft_sync_complete() {
        let db = setup_db();
        let item = DraftSyncQueueItem::new_create("q1", "draft-1", "{}");
        db.queue_draft_sync(&item).unwrap();

        db.mark_draft_sync_syncing("q1").unwrap();
        db.mark_draft_sync_complete("q1", Some("server-123"), Some("TOK123"))
            .unwrap();

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

        db.queue_draft_sync(&DraftSyncQueueItem::new_create("q1", "d1", "{}"))
            .unwrap();
        db.queue_draft_sync(&DraftSyncQueueItem::new_create("q2", "d2", "{}"))
            .unwrap();

        let count = db.count_pending_draft_syncs().unwrap();
        assert_eq!(count, 2);
    }

    #[test]
    fn test_cleanup_completed_draft_syncs() {
        let db = setup_db();

        db.queue_draft_sync(&DraftSyncQueueItem::new_create("q1", "d1", "{}"))
            .unwrap();
        db.queue_draft_sync(&DraftSyncQueueItem::new_create("q2", "d2", "{}"))
            .unwrap();

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

        assert_eq!(
            DraftSyncOperation::from_str("CREATE"),
            Ok(DraftSyncOperation::Create)
        );
        assert_eq!(
            DraftSyncOperation::from_str("CONVERT"),
            Ok(DraftSyncOperation::Convert)
        );
        assert_eq!(
            DraftSyncOperation::from_str("DELETE"),
            Ok(DraftSyncOperation::Delete)
        );
    }

    #[test]
    fn draft_sync_operation_rejects_unknown_rather_than_creating() {
        // Was: `assert_eq!(from_str("UNKNOWN"), Create)` — the test pinned the defect. An
        // unrecognised operation must not be silently executed as a cart create.
        assert_eq!(
            DraftSyncOperation::from_str("UNKNOWN"),
            Err(ParseError::DraftSyncOperation("UNKNOWN".to_string()))
        );
        assert_eq!(
            DraftQueueSyncStatus::from_str("NOPE"),
            Err(ParseError::DraftQueueSyncStatus("NOPE".to_string()))
        );
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

        db.queue_draft_sync(&DraftSyncQueueItem::new_create("q1", "draft-1", "{}"))
            .unwrap();
        db.queue_draft_sync(&DraftSyncQueueItem::new_convert(
            "q2", "draft-1", "s1", "t1",
        ))
        .unwrap();
        db.queue_draft_sync(&DraftSyncQueueItem::new_create("q3", "draft-2", "{}"))
            .unwrap();

        let items = db.get_draft_sync_items_by_local_id("draft-1").unwrap();
        assert_eq!(items.len(), 2);
    }

    // ------------------------------------------------------------------------------------------
    // `DRAFT_SYNC_QUEUE_ITEM_ROW`. The three patterns from task 04.
    // ------------------------------------------------------------------------------------------

    /// A queue item whose every column holds a value found nowhere else in the row.
    fn an_item_with_no_two_columns_alike() -> DraftSyncQueueItem {
        DraftSyncQueueItem {
            id: "id-column".to_string(),
            local_draft_id: "local-draft-id-column".to_string(),
            operation: "CONVERT".to_string(),
            payload_json: r#"{"payload":"json-column"}"#.to_string(),
            server_id: Some("server-id-column".to_string()),
            server_token: Some("server-token-column".to_string()),
            transaction_id: Some("transaction-id-column".to_string()),
            // Not `PENDING`: that is the column's SQL `DEFAULT`.
            sync_status: "SYNCING".to_string(),
            // Not 0, same reason; and below `MAX_DRAFT_SYNC_RETRIES` so the pending reader's
            // `retry_count < ?1` predicate is exercised rather than dodged.
            retry_count: 3,
            last_error: Some("last-error-column".to_string()),
            created_at: "2026-08-24T10:00:00Z".to_string(),
            last_attempt_at: Some("2026-08-24T11:00:00Z".to_string()),
        }
    }

    #[test]
    fn the_queue_mapping_names_every_column_in_the_order_it_reads_them() {
        assert_eq!(
            DRAFT_SYNC_QUEUE_ITEM_ROW.reader().select_list(),
            "id, local_draft_id, operation, payload_json, server_id, server_token, \
             transaction_id, sync_status, retry_count, last_error, created_at, last_attempt_at"
        );
        assert_eq!(DRAFT_SYNC_QUEUE_ITEM_ROW.reader().width(), 12);
        assert_eq!(DRAFT_SYNC_QUEUE_ITEM_ROW.insert_column_names().count(), 12);
    }

    #[test]
    fn every_column_of_a_fully_distinct_queue_item_survives_the_round_trip() {
        let db = setup_db();
        let written = an_item_with_no_two_columns_alike();
        db.queue_draft_sync(&written).unwrap();

        let read = db
            .get_draft_sync_item("id-column")
            .unwrap()
            .expect("the item this test just wrote");
        assert_eq!(read.id, written.id);
        assert_eq!(read.local_draft_id, written.local_draft_id);
        assert_eq!(read.operation, written.operation);
        assert_eq!(read.payload_json, written.payload_json);
        assert_eq!(read.server_id, written.server_id);
        assert_eq!(read.server_token, written.server_token);
        assert_eq!(read.transaction_id, written.transaction_id);
        assert_eq!(read.sync_status, written.sync_status);
        assert_eq!(read.retry_count, written.retry_count);
        assert_eq!(read.last_error, written.last_error);
        assert_eq!(read.created_at, written.created_at);
        assert_eq!(read.last_attempt_at, written.last_attempt_at);
    }

    #[test]
    fn queue_draft_sync_puts_each_value_in_the_column_that_carries_its_name() {
        let db = setup_db();
        db.queue_draft_sync(&an_item_with_no_two_columns_alike())
            .unwrap();

        let conn = db.connection();
        let conn = conn.lock();
        for (column, expected) in [
            ("id", "id-column"),
            ("local_draft_id", "local-draft-id-column"),
            ("operation", "CONVERT"),
            ("payload_json", r#"{"payload":"json-column"}"#),
            ("server_id", "server-id-column"),
            ("server_token", "server-token-column"),
            ("transaction_id", "transaction-id-column"),
            ("sync_status", "SYNCING"),
            ("last_error", "last-error-column"),
            ("created_at", "2026-08-24T10:00:00Z"),
            ("last_attempt_at", "2026-08-24T11:00:00Z"),
        ] {
            let matched: bool = conn
                .query_row(
                    &format!("SELECT {column} = ?1 FROM draft_sync_queue"),
                    [expected],
                    |row| row.get(0),
                )
                .unwrap();
            assert!(matched, "the `{column}` column does not hold `{expected}`");
        }

        // `retry_count` and the type it landed under: the writer used to bind it through
        // `.to_string()` into an `INTEGER` column, which only affinity rescued.
        let (count, kind): (i64, String) = conn
            .query_row(
                "SELECT retry_count, typeof(retry_count) FROM draft_sync_queue",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!((count, kind.as_str()), (3, "integer"));
    }

    #[test]
    fn a_second_write_of_the_same_queue_item_id_replaces_the_row() {
        let db = setup_db();
        let first = an_item_with_no_two_columns_alike();
        db.queue_draft_sync(&first).unwrap();
        db.queue_draft_sync(&DraftSyncQueueItem {
            last_error: Some("second-write".to_string()),
            ..first
        })
        .unwrap();

        let rows: i64 = db
            .select_scalar("SELECT COUNT(*) FROM draft_sync_queue", [])
            .unwrap();
        assert_eq!(rows, 1, "the second write inserted rather than replaced");
        assert_eq!(
            db.get_draft_sync_item("id-column")
                .unwrap()
                .unwrap()
                .last_error
                .as_deref(),
            Some("second-write")
        );
    }

    /// One nullable column, blanked, and what must still hold of its neighbours.
    ///
    /// A named struct rather than a tuple of two function pointers, for the reason
    /// `clippy::type_complexity` gives and `operators.rs` already settled: the three fields are
    /// what the case *is*.
    struct AbsentQueueColumn {
        column: &'static str,
        blank: fn(&mut DraftSyncQueueItem),
        assert_absent: fn(&DraftSyncQueueItem),
    }

    #[test]
    fn a_null_in_one_queue_column_reaches_that_columns_field_and_no_other() {
        let db = setup_db();
        let full = an_item_with_no_two_columns_alike();

        let cases = [
            AbsentQueueColumn {
                column: "server_id",
                blank: |row| row.server_id = None,
                assert_absent: |row| {
                    assert_eq!(row.server_id, None);
                    assert_eq!(row.server_token.as_deref(), Some("server-token-column"));
                },
            },
            AbsentQueueColumn {
                column: "server_token",
                blank: |row| row.server_token = None,
                assert_absent: |row| {
                    assert_eq!(row.server_token, None);
                    assert_eq!(row.server_id.as_deref(), Some("server-id-column"));
                },
            },
            AbsentQueueColumn {
                column: "transaction_id",
                blank: |row| row.transaction_id = None,
                assert_absent: |row| {
                    assert_eq!(row.transaction_id, None);
                    assert_eq!(row.sync_status, "SYNCING");
                },
            },
            AbsentQueueColumn {
                column: "last_error",
                blank: |row| row.last_error = None,
                assert_absent: |row| {
                    assert_eq!(row.last_error, None);
                    assert_eq!(row.retry_count, 3);
                },
            },
            AbsentQueueColumn {
                column: "last_attempt_at",
                blank: |row| row.last_attempt_at = None,
                assert_absent: |row| {
                    assert_eq!(row.last_attempt_at, None);
                    assert_eq!(row.created_at, "2026-08-24T10:00:00Z");
                },
            },
        ];

        for case in cases {
            let mut written = full.clone();
            (case.blank)(&mut written);
            db.queue_draft_sync(&written).unwrap();

            let stored: Option<String> = db
                .select_scalar(&format!("SELECT {} FROM draft_sync_queue", case.column), [])
                .unwrap();
            assert_eq!(stored, None, "`{}` was not written as NULL", case.column);

            let read = db
                .get_draft_sync_item("id-column")
                .unwrap()
                .expect("the row this iteration wrote");
            (case.assert_absent)(&read);
        }
    }

    #[test]
    fn every_reader_of_the_queue_returns_the_same_row() {
        let db = setup_db();
        db.queue_draft_sync(&DraftSyncQueueItem {
            sync_status: "PENDING".to_string(),
            ..an_item_with_no_two_columns_alike()
        })
        .unwrap();

        let by_id = db.get_draft_sync_item("id-column").unwrap().expect("by id");
        let pending = db.get_pending_draft_syncs(10).unwrap();
        let by_local = db
            .get_draft_sync_items_by_local_id("local-draft-id-column")
            .unwrap();

        assert_eq!(pending.len(), 1);
        assert_eq!(by_local.len(), 1);
        for (name, row) in [
            ("get_pending_draft_syncs", &pending[0]),
            ("get_draft_sync_items_by_local_id", &by_local[0]),
        ] {
            assert_eq!(row.id, by_id.id, "{name}");
            assert_eq!(row.payload_json, by_id.payload_json, "{name}");
            assert_eq!(row.transaction_id, by_id.transaction_id, "{name}");
            assert_eq!(row.retry_count, by_id.retry_count, "{name}");
        }

        // The fourth reader has the opposite predicate, so it must return nothing here — and it
        // does return something once the item has exhausted its retries. Both directions, because
        // "empty" alone is what a broken query also returns.
        assert!(db.get_failed_draft_syncs().unwrap().is_empty());
        db.queue_draft_sync(&DraftSyncQueueItem {
            retry_count: MAX_DRAFT_SYNC_RETRIES,
            ..an_item_with_no_two_columns_alike()
        })
        .unwrap();
        let failed = db.get_failed_draft_syncs().unwrap();
        assert_eq!(failed.len(), 1);
        assert_eq!(failed[0].id, by_id.id);
    }
}
