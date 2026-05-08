//! Sync State Repository
//!
//! Tracks synchronization state for each resource type (products, operators, etc.)

use chrono::{DateTime, Utc};
use rusqlite::{OptionalExtension, Result as SqliteResult};
use serde::{Deserialize, Serialize};

use super::Database;

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

impl Database {
    /// Gets sync state for a resource
    pub fn get_sync_state(&self, resource: SyncResource) -> SqliteResult<Option<SyncState>> {
        let conn = self.connection();
        let conn = conn.lock();

        conn.query_row(
            "SELECT resource, etag, version, last_sync, record_count FROM sync_state WHERE resource = ?1",
            [resource.as_str()],
            |row| {
                Ok(SyncState {
                    resource: row.get(0)?,
                    etag: row.get(1)?,
                    version: row.get(2)?,
                    last_sync: row.get(3)?,
                    record_count: row.get(4)?,
                })
            },
        )
        .optional()
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
        let now = Utc::now().to_rfc3339();

        self.execute(
            r#"INSERT OR REPLACE INTO sync_state (resource, etag, version, last_sync, record_count)
               VALUES (?1, ?2, ?3, ?4, ?5)"#,
            &[
                &resource.as_str(),
                &etag,
                &version,
                &Some(now.as_str()),
                &record_count,
            ],
        )?;
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
        let conn = self.connection();
        let conn = conn.lock();

        let mut stmt =
            conn.prepare("SELECT resource, etag, version, last_sync, record_count FROM sync_state")?;

        let rows = stmt.query_map([], |row| {
            Ok(SyncState {
                resource: row.get(0)?,
                etag: row.get(1)?,
                version: row.get(2)?,
                last_sync: row.get(3)?,
                record_count: row.get(4)?,
            })
        })?;

        rows.collect()
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
}
