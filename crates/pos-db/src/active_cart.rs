//! Active Cart Repository
//!
//! Handles persistence of the active shopping cart for crash recovery.
//! Ensures cart data is not lost on app crash or power failure.

use rusqlite::Result as SqliteResult;
use serde::{Deserialize, Serialize};

use pos_models::OperatorId;

use super::Database;
use crate::column;
use crate::projection::OnConflict;
use crate::row_mapping;

/// The one cart the till keeps on disk, so a crash does not lose what the cashier has rung up.
///
/// This used to be `(Option<OperatorId>, String)` — a row shape with no name, read positionally at
/// one end and destructured positionally at the other, across a crate boundary. Two `String`-ish
/// members in a tuple is exactly the arrangement where swapping them costs nothing and is caught
/// by nothing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActiveCart {
    /// The operator who owns this cart, when one was signed in.
    pub operator_id: Option<OperatorId>,
    /// The cart itself, as `serde_json` left it. Opaque to this crate.
    pub cart_json: String,
}

row_mapping! {
    /// Every column of `active_cart`, declared once.
    ///
    /// `id` is `managed` rather than a field, and that is the singleton constraint expressed
    /// rather than remembered: the table says `id INTEGER PRIMARY KEY CHECK (id = 1)`, so there is
    /// one active cart and no caller has a choice about which. An `id` field on [`ActiveCart`]
    /// would be a socket accepting a number the schema refuses.
    ///
    /// With `id` fixed at 1, `OnConflict::Replace` is what makes saving the cart idempotent — the
    /// second save overwrites the first instead of failing on the primary key.
    pub const ACTIVE_CART_ROW: RowMapping<ActiveCart> = for "active_cart" {
        operator_id     via column::OPTIONAL_OPERATOR_ID,
        cart_json,
        managed "id" = "1",
        managed "updated_at" = "datetime('now')",
    } on_conflict OnConflict::Replace;
}

impl Database {
    /// Saves the active cart to the database
    ///
    /// Replaces whatever was there: there is exactly one active cart (`id = 1`). Call after every
    /// cart mutation, so a crash costs at most the last one.
    pub fn save_active_cart(&self, cart: &ActiveCart) -> SqliteResult<()> {
        self.insert(&ACTIVE_CART_ROW, cart)?;
        Ok(())
    }

    /// Gets the active cart from the database
    ///
    /// Used on startup to restore a cart that was in progress before a crash.
    pub fn get_active_cart(&self) -> SqliteResult<Option<ActiveCart>> {
        self.select_one(
            ACTIVE_CART_ROW.reader(),
            "FROM active_cart WHERE id = 1",
            [],
        )
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
    use crate::projection::scalar;

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

    #[test]
    fn test_save_and_get_active_cart() {
        let db = setup_db();

        let cart_json = r#"{"items":[{"product_id":"p1","quantity":2}],"grand_total":50.0}"#;
        db.save_active_cart(&ActiveCart {
            operator_id: Some(op_id("op-1")),
            cart_json: cart_json.to_string(),
        })
        .unwrap();

        let restored = db.get_active_cart().unwrap().expect("a saved cart");
        assert_eq!(restored.operator_id, Some(op_id("op-1")));
        assert_eq!(restored.cart_json, cart_json);
    }

    #[test]
    fn test_save_active_cart_without_operator() {
        let db = setup_db();

        db.save_active_cart(&ActiveCart {
            operator_id: None,
            cart_json: r#"{"items":[]}"#.to_string(),
        })
        .unwrap();

        let restored = db.get_active_cart().unwrap().expect("a saved cart");
        assert_eq!(restored.operator_id, None);
    }

    #[test]
    fn test_save_replaces_existing_cart() {
        let db = setup_db();

        db.save_active_cart(&ActiveCart {
            operator_id: Some(op_id("op-1")),
            cart_json: r#"{"v":1}"#.to_string(),
        })
        .unwrap();

        // The second save must replace rather than fail: `id` is fixed at 1 by the mapping, so
        // every save collides with the previous one on the primary key.
        db.save_active_cart(&ActiveCart {
            operator_id: Some(op_id("op-2")),
            cart_json: r#"{"v":2}"#.to_string(),
        })
        .unwrap();

        let restored = db.get_active_cart().unwrap().expect("a saved cart");
        assert_eq!(restored.operator_id, Some(op_id("op-2")));
        assert_eq!(restored.cart_json, r#"{"v":2}"#);

        // …and it replaces rather than accumulating.
        let rows: i64 = {
            let conn = db.connection();
            let conn = conn.lock();
            scalar(&conn, "SELECT COUNT(*) FROM active_cart", []).unwrap()
        };
        assert_eq!(rows, 1);
    }

    #[test]
    fn test_clear_active_cart() {
        let db = setup_db();

        db.save_active_cart(&ActiveCart {
            operator_id: Some(op_id("op-1")),
            cart_json: r#"{"items":[]}"#.to_string(),
        })
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

    // ------------------------------------------------------------------------------------------
    // `ACTIVE_CART_ROW`.
    // ------------------------------------------------------------------------------------------

    #[test]
    fn the_active_cart_mapping_reads_two_columns_and_writes_four() {
        assert_eq!(
            ACTIVE_CART_ROW.reader().select_list(),
            "operator_id, cart_json"
        );
        assert_eq!(ACTIVE_CART_ROW.reader().width(), 2);
        assert_eq!(
            ACTIVE_CART_ROW
                .insert_column_names()
                .collect::<Vec<_>>()
                .join(", "),
            "operator_id, cart_json, id, updated_at",
            "`id` and `updated_at` are the store's, and must still appear in the INSERT"
        );
    }

    #[test]
    fn saving_the_cart_puts_each_value_in_the_column_that_carries_its_name() {
        let db = setup_db();
        db.save_active_cart(&ActiveCart {
            operator_id: Some(op_id("operator-id-column")),
            cart_json: r#"{"cart":"json-column"}"#.to_string(),
        })
        .unwrap();

        let conn = db.connection();
        let conn = conn.lock();
        for (column, expected) in [
            ("operator_id", "operator-id-column"),
            ("cart_json", r#"{"cart":"json-column"}"#),
        ] {
            let matched: bool = scalar(
                &conn,
                &format!("SELECT {column} = ?1 FROM active_cart"),
                [expected],
            )
            .unwrap();
            assert!(matched, "the `{column}` column does not hold `{expected}`");
        }

        // `id` is the mapping's, not the caller's: the schema says `CHECK (id = 1)`.
        let id: i64 = scalar(&conn, "SELECT id FROM active_cart", []).unwrap();
        assert_eq!(id, 1);

        // …and `updated_at` is `NOT NULL`, so the store must have written it.
        let stamped: bool = scalar(
            &conn,
            "SELECT updated_at IS NOT NULL AND updated_at <> '' FROM active_cart",
            [],
        )
        .unwrap();
        assert!(stamped, "the store did not stamp `updated_at`");
    }

    /// A `NULL` operator reaches `operator_id` and leaves `cart_json` alone.
    ///
    /// Two columns is too few for a table of cases, but it is exactly the width at which a swap
    /// is cheapest: both sides are text, and an unsigned-in cart is the ordinary state at
    /// start-of-day rather than an edge case.
    #[test]
    fn a_null_operator_reaches_that_field_and_leaves_the_cart_alone() {
        let db = setup_db();
        db.save_active_cart(&ActiveCart {
            operator_id: None,
            cart_json: r#"{"cart":"json-column"}"#.to_string(),
        })
        .unwrap();

        let restored = db.get_active_cart().unwrap().expect("a saved cart");
        assert_eq!(restored.operator_id, None);
        assert_eq!(restored.cart_json, r#"{"cart":"json-column"}"#);
    }
}
