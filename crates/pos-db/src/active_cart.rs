//! Active Cart Repository
//!
//! Handles persistence of the active shopping cart for crash recovery.
//! Ensures cart data is not lost on app crash or power failure.

use rusqlite::{OptionalExtension, Result as SqliteResult};

use super::Database;

impl Database {
    /// Saves the active cart to the database
    ///
    /// Uses INSERT OR REPLACE to ensure only one active cart exists (id=1).
    /// This should be called after every cart mutation for crash recovery.
    ///
    /// # Arguments
    /// * `operator_id` - The operator who owns this cart (optional)
    /// * `cart_json` - The cart serialized as JSON
    pub fn save_active_cart(&self, operator_id: Option<&str>, cart_json: &str) -> SqliteResult<()> {
        self.execute(
            r#"INSERT OR REPLACE INTO active_cart (id, operator_id, cart_json, updated_at)
               VALUES (1, ?1, ?2, datetime('now'))"#,
            &[&operator_id, &cart_json],
        )?;
        Ok(())
    }

    /// Gets the active cart from the database
    ///
    /// Returns the operator_id and cart_json if a cart exists.
    /// Used on startup to restore a cart that was in progress before a crash.
    pub fn get_active_cart(&self) -> SqliteResult<Option<(Option<String>, String)>> {
        let conn = self.connection();
        let conn = conn.lock();
        conn.query_row(
            "SELECT operator_id, cart_json FROM active_cart WHERE id = 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
    }

    /// Clears the active cart from the database
    ///
    /// Should be called after a transaction is completed or the cart is intentionally cleared.
    pub fn clear_active_cart(&self) -> SqliteResult<()> {
        self.execute("DELETE FROM active_cart WHERE id = 1", &[])?;
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

    #[test]
    fn test_save_and_get_active_cart() {
        let db = setup_db();

        let cart_json = r#"{"items":[{"product_id":"p1","quantity":2}],"grand_total":50.0}"#;
        db.save_active_cart(Some("op-1"), cart_json).unwrap();

        let result = db.get_active_cart().unwrap();
        assert!(result.is_some());

        let (operator_id, json) = result.unwrap();
        assert_eq!(operator_id, Some("op-1".to_string()));
        assert_eq!(json, cart_json);
    }

    #[test]
    fn test_save_active_cart_without_operator() {
        let db = setup_db();

        let cart_json = r#"{"items":[]}"#;
        db.save_active_cart(None, cart_json).unwrap();

        let result = db.get_active_cart().unwrap();
        assert!(result.is_some());

        let (operator_id, _) = result.unwrap();
        assert_eq!(operator_id, None);
    }

    #[test]
    fn test_save_replaces_existing_cart() {
        let db = setup_db();

        // Save first cart
        db.save_active_cart(Some("op-1"), r#"{"v":1}"#).unwrap();

        // Save second cart - should replace
        db.save_active_cart(Some("op-2"), r#"{"v":2}"#).unwrap();

        let result = db.get_active_cart().unwrap();
        let (operator_id, json) = result.unwrap();
        assert_eq!(operator_id, Some("op-2".to_string()));
        assert_eq!(json, r#"{"v":2}"#);
    }

    #[test]
    fn test_clear_active_cart() {
        let db = setup_db();

        db.save_active_cart(Some("op-1"), r#"{"items":[]}"#)
            .unwrap();
        assert!(db.get_active_cart().unwrap().is_some());

        db.clear_active_cart().unwrap();
        assert!(db.get_active_cart().unwrap().is_none());
    }

    #[test]
    fn test_get_empty_active_cart() {
        let db = setup_db();

        let result = db.get_active_cart().unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_clear_nonexistent_cart() {
        let db = setup_db();

        // Should not error when clearing non-existent cart
        db.clear_active_cart().unwrap();
    }
}
