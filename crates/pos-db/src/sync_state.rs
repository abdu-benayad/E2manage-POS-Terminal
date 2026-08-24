//! Sync State Repository
//!
//! Tracks synchronization state for each resource type (products, operators, etc.)

use chrono::{DateTime, Utc};
use rusqlite::{OptionalExtension, Result as SqliteResult};
use serde::{Deserialize, Serialize};

use super::Database;
use crate::projection::OnConflict;
use crate::row_mapping;

/// Known resource types for sync
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SyncResource {
    Products,
    Categories,
    Operators,
    Customers,
    PaymentMethods,
    Screens,
    Features,
}

impl SyncResource {
    pub fn as_str(&self) -> &'static str {
        match self {
            SyncResource::Products => "products",
            SyncResource::Categories => "categories",
            SyncResource::Operators => "operators",
            SyncResource::Customers => "customers",
            SyncResource::PaymentMethods => "payment_methods",
            SyncResource::Screens => "screens",
            SyncResource::Features => "features",
        }
    }
}

impl std::fmt::Display for SyncResource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Sync state row from database
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncState {
    pub resource: String,
    pub etag: Option<String>,
    pub version: Option<String>,
    pub last_sync: Option<String>,
    pub record_count: i64,
}

row_mapping! {
    /// Every column of `sync_state`, declared once.
    ///
    /// Two identical five-column readers stood at `get_sync_state` and `get_all_sync_states`,
    /// which is the two-copies-of-one-shape case in its smallest form: nothing made them agree,
    /// and nothing would have said so if they stopped.
    ///
    /// `last_sync` is **not** `managed "last_sync" = "datetime('now')"`, and the difference is not
    /// cosmetic. The caller writes `Utc::now().to_rfc3339()`; SQLite's `datetime('now')` renders
    /// `YYYY-MM-DD HH:MM:SS`, which [`SyncState::last_sync_time`] parses as RFC 3339 and gets
    /// `None` for — turning every synced resource into a never-synced one. So the column stays a
    /// bound parameter and the timestamp stays the caller's.
    pub const SYNC_STATE_ROW: RowMapping<SyncState> = for "sync_state" {
        resource,
        etag,
        version,
        last_sync,
        record_count,
    } on_conflict OnConflict::Replace;
}

impl Database {
    /// Gets sync state for a resource
    pub fn get_sync_state(&self, resource: SyncResource) -> SqliteResult<Option<SyncState>> {
        self.select_one(
            SYNC_STATE_ROW.reader(),
            "FROM sync_state WHERE resource = ?1",
            [resource.as_str()],
        )
    }

    /// Gets ETag for a resource (for conditional GET)
    pub fn get_etag(&self, resource: SyncResource) -> SqliteResult<Option<String>> {
        let conn = self.connection();
        let conn = conn.lock();

        conn.query_row(
            "SELECT etag FROM sync_state WHERE resource = ?1",
            [resource.as_str()],
            |row| row.get(0),
        )
        .optional()
    }

    /// Updates sync state for a resource
    pub fn update_sync_state(
        &self,
        resource: SyncResource,
        etag: Option<&str>,
        version: Option<&str>,
        record_count: i64,
    ) -> SqliteResult<()> {
        let state = SyncState {
            resource: resource.as_str().to_string(),
            etag: etag.map(String::from),
            version: version.map(String::from),
            last_sync: Some(Utc::now().to_rfc3339()),
            record_count,
        };

        self.insert(&SYNC_STATE_ROW, &state)?;
        Ok(())
    }

    /// Clears sync state for a resource
    pub fn clear_sync_state(&self, resource: SyncResource) -> SqliteResult<()> {
        self.execute(
            "DELETE FROM sync_state WHERE resource = ?1",
            &[&resource.as_str()],
        )?;
        Ok(())
    }

    /// Clears all sync state
    pub fn clear_all_sync_state(&self) -> SqliteResult<()> {
        self.execute("DELETE FROM sync_state", &[])?;
        Ok(())
    }

    /// Clears all tenant-specific data from the local database.
    ///
    /// Called during re-pairing to ensure no stale data from a previous
    /// tenant remains. Preserves terminal_registration and settings.
    pub fn clear_tenant_data(&self) -> SqliteResult<()> {
        let conn = self.connection();
        let conn = conn.lock();
        conn.execute_batch(
            r#"
            DELETE FROM products;
            DELETE FROM categories;
            DELETE FROM operators;
            DELETE FROM customers;
            DELETE FROM payment_methods;
            DELETE FROM features;
            DELETE FROM feature_screens;
            DELETE FROM screens;
            DELETE FROM sync_state;
            DELETE FROM drafts;
            DELETE FROM shared_drafts;
            DELETE FROM draft_sync_queue;
            DELETE FROM active_cart;
            DELETE FROM shifts;
            DELETE FROM offline_transactions;
            DELETE FROM z_reports;
            DELETE FROM day_closures;
            DELETE FROM print_queue;
            DELETE FROM terminal_config;
            "#,
        )?;
        Ok(())
    }

    /// Gets all sync states
    pub fn get_all_sync_states(&self) -> SqliteResult<Vec<SyncState>> {
        self.select_all(SYNC_STATE_ROW.reader(), "FROM sync_state", [])
    }

    /// Checks if a resource needs sync (never synced or older than interval)
    pub fn needs_sync(&self, resource: SyncResource, max_age_minutes: i64) -> SqliteResult<bool> {
        let state = self.get_sync_state(resource)?;

        match state {
            None => Ok(true), // Never synced
            Some(state) => {
                let last_sync = state
                    .last_sync
                    .as_ref()
                    .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                    .map(|dt| dt.with_timezone(&Utc));

                match last_sync {
                    None => Ok(true),
                    Some(last) => {
                        let age = Utc::now().signed_duration_since(last);
                        Ok(age.num_minutes() >= max_age_minutes)
                    }
                }
            }
        }
    }
}

impl SyncState {
    /// Parses last_sync as DateTime
    pub fn last_sync_time(&self) -> Option<DateTime<Utc>> {
        self.last_sync
            .as_ref()
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&Utc))
    }

    /// Returns human-readable "time ago" string
    pub fn time_ago(&self) -> String {
        match self.last_sync_time() {
            None => "never".to_string(),
            Some(last) => {
                let duration = Utc::now().signed_duration_since(last);

                if duration.num_days() > 0 {
                    format!("{} days ago", duration.num_days())
                } else if duration.num_hours() > 0 {
                    format!("{} hours ago", duration.num_hours())
                } else if duration.num_minutes() > 0 {
                    format!("{} minutes ago", duration.num_minutes())
                } else {
                    "just now".to_string()
                }
            }
        }
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
    fn test_sync_state_crud() {
        let db = setup_db();

        // Initially no state
        let state = db.get_sync_state(SyncResource::Products).unwrap();
        assert!(state.is_none());

        // Update state
        db.update_sync_state(
            SyncResource::Products,
            Some("\"abc123\""),
            Some("v1.0"),
            100,
        )
        .unwrap();

        // Get state
        let state = db.get_sync_state(SyncResource::Products).unwrap();
        assert!(state.is_some());
        let state = state.unwrap();
        assert_eq!(state.etag, Some("\"abc123\"".to_string()));
        assert_eq!(state.record_count, 100);

        // Clear state
        db.clear_sync_state(SyncResource::Products).unwrap();
        let state = db.get_sync_state(SyncResource::Products).unwrap();
        assert!(state.is_none());
    }

    #[test]
    fn test_get_etag() {
        let db = setup_db();

        db.update_sync_state(SyncResource::Products, Some("\"etag123\""), None, 50)
            .unwrap();

        let etag = db.get_etag(SyncResource::Products).unwrap();
        assert_eq!(etag, Some("\"etag123\"".to_string()));

        let etag = db.get_etag(SyncResource::Categories).unwrap();
        assert!(etag.is_none());
    }

    #[test]
    fn test_needs_sync() {
        let db = setup_db();

        // Never synced - needs sync
        assert!(db.needs_sync(SyncResource::Products, 10).unwrap());

        // Just synced - doesn't need sync
        db.update_sync_state(SyncResource::Products, Some("etag"), None, 100)
            .unwrap();
        assert!(!db.needs_sync(SyncResource::Products, 10).unwrap());

        // With 0 max age - always needs sync
        assert!(db.needs_sync(SyncResource::Products, 0).unwrap());
    }

    #[test]
    fn test_all_sync_states() {
        let db = setup_db();

        db.update_sync_state(SyncResource::Products, Some("p1"), None, 100)
            .unwrap();
        db.update_sync_state(SyncResource::Categories, Some("c1"), None, 10)
            .unwrap();
        db.update_sync_state(SyncResource::Operators, Some("o1"), None, 5)
            .unwrap();

        let states = db.get_all_sync_states().unwrap();
        assert_eq!(states.len(), 3);
    }

    // ------------------------------------------------------------------------------------------
    // `SYNC_STATE_ROW`.
    // ------------------------------------------------------------------------------------------

    /// A sync state whose every column holds a value found nowhere else in the row.
    fn a_sync_state_with_no_two_columns_alike() -> SyncState {
        SyncState {
            resource: "resource-column".to_string(),
            etag: Some("etag-column".to_string()),
            version: Some("version-column".to_string()),
            last_sync: Some("2026-08-24T10:00:00Z".to_string()),
            record_count: 4242,
        }
    }

    #[test]
    fn the_sync_state_mapping_names_every_column_it_writes_in_the_order_it_reads_them() {
        assert_eq!(
            SYNC_STATE_ROW.reader().select_list(),
            "resource, etag, version, last_sync, record_count"
        );
        assert_eq!(SYNC_STATE_ROW.reader().width(), 5);
        assert_eq!(SYNC_STATE_ROW.insert_column_names().count(), 5);
    }

    #[test]
    fn every_column_of_a_fully_distinct_sync_state_survives_the_round_trip() {
        let db = setup_db();
        let written = a_sync_state_with_no_two_columns_alike();
        db.insert(&SYNC_STATE_ROW, &written).unwrap();

        let read = db
            .select_all(SYNC_STATE_ROW.reader(), "FROM sync_state", [])
            .unwrap();
        assert_eq!(read.len(), 1);
        let read = &read[0];
        assert_eq!(read.resource, written.resource);
        assert_eq!(read.etag, written.etag);
        assert_eq!(read.version, written.version);
        assert_eq!(read.last_sync, written.last_sync);
        assert_eq!(read.record_count, written.record_count);
    }

    #[test]
    fn update_sync_state_puts_each_value_in_the_column_that_carries_its_name() {
        let db = setup_db();
        db.update_sync_state(
            SyncResource::PaymentMethods,
            Some("etag-column"),
            Some("version-column"),
            4242,
        )
        .unwrap();

        let conn = db.connection();
        let conn = conn.lock();
        for (column, expected) in [
            ("resource", "payment_methods"),
            ("etag", "etag-column"),
            ("version", "version-column"),
        ] {
            let matched: bool = conn
                .query_row(
                    &format!("SELECT {column} = ?1 FROM sync_state"),
                    [expected],
                    |row| row.get(0),
                )
                .unwrap();
            assert!(matched, "the `{column}` column does not hold `{expected}`");
        }

        let count: i64 = conn
            .query_row("SELECT record_count FROM sync_state", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 4242);
    }

    /// The stamp `update_sync_state` writes must be the one [`SyncState::last_sync_time`] reads.
    ///
    /// This is why `last_sync` is a bound parameter and not `managed "last_sync" =
    /// "datetime('now')"`. SQLite renders `YYYY-MM-DD HH:MM:SS`, which the RFC 3339 parser
    /// answers `None` for — and `needs_sync` reads that `None` as *never synced*, so every
    /// resource would re-sync on every poll, forever, with nothing failing.
    #[test]
    fn the_last_sync_stamp_is_readable_by_the_parser_that_has_to_read_it() {
        let db = setup_db();
        db.update_sync_state(SyncResource::Products, None, None, 1)
            .unwrap();

        let state = db
            .get_sync_state(SyncResource::Products)
            .unwrap()
            .expect("the state this test just wrote");
        assert!(
            state.last_sync_time().is_some(),
            "`last_sync` was written in a format `last_sync_time` cannot parse: {:?}",
            state.last_sync
        );

        // The control: the parser does answer `None` for the shape `datetime('now')` produces, so
        // the assertion above discriminates rather than passing on anything non-empty.
        let sqlite_shaped = SyncState {
            last_sync: Some("2026-08-24 10:00:00".to_string()),
            ..a_sync_state_with_no_two_columns_alike()
        };
        assert_eq!(sqlite_shaped.last_sync_time(), None);
    }

    #[test]
    fn a_second_write_of_the_same_resource_replaces_the_row() {
        let db = setup_db();
        db.update_sync_state(SyncResource::Products, Some("first"), None, 1)
            .unwrap();
        db.update_sync_state(SyncResource::Products, Some("second"), None, 2)
            .unwrap();

        let rows: i64 = db
            .select_scalar("SELECT COUNT(*) FROM sync_state", [])
            .unwrap();
        assert_eq!(rows, 1, "the second write inserted rather than replaced");

        let state = db.get_sync_state(SyncResource::Products).unwrap().unwrap();
        assert_eq!(state.etag.as_deref(), Some("second"));
        assert_eq!(state.record_count, 2);
    }

    /// One nullable sync-state column, blanked, and what must still hold of its neighbours.
    struct AbsentSyncStateColumn {
        column: &'static str,
        blank: fn(&mut SyncState),
        assert_absent: fn(&SyncState),
    }

    #[test]
    fn a_null_in_one_sync_state_column_reaches_that_columns_field_and_no_other() {
        let db = setup_db();
        let full = a_sync_state_with_no_two_columns_alike();

        let cases = [
            AbsentSyncStateColumn {
                column: "etag",
                blank: |row| row.etag = None,
                assert_absent: |row| {
                    assert_eq!(row.etag, None);
                    assert_eq!(row.version.as_deref(), Some("version-column"));
                },
            },
            AbsentSyncStateColumn {
                column: "version",
                blank: |row| row.version = None,
                assert_absent: |row| {
                    assert_eq!(row.version, None);
                    assert_eq!(row.etag.as_deref(), Some("etag-column"));
                },
            },
            AbsentSyncStateColumn {
                column: "last_sync",
                blank: |row| row.last_sync = None,
                assert_absent: |row| {
                    assert_eq!(row.last_sync, None);
                    assert_eq!(row.version.as_deref(), Some("version-column"));
                },
            },
        ];

        for case in cases {
            let mut blanked = full.clone();
            (case.blank)(&mut blanked);
            db.insert(&SYNC_STATE_ROW, &blanked).unwrap();

            let read = db
                .select_one(
                    SYNC_STATE_ROW.reader(),
                    "FROM sync_state WHERE resource = ?1",
                    ["resource-column"],
                )
                .unwrap()
                .unwrap_or_else(|| panic!("the state written with `{}` blank", case.column));
            (case.assert_absent)(&read);
        }
    }
}
