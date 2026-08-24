//! Features Repository
//!
//! Handles feature and feature screen data storage and retrieval
//! for the Feature-Based Screen Library.

use pos_models::{Feature, FeatureScreen};
use rusqlite::{params, Result as SqliteResult};

use super::Database;
use crate::column;
use crate::projection::{self, OnConflict};
use crate::row_mapping;

row_mapping! {
    /// Every column of `features` this till reads or writes, declared once.
    ///
    /// # `Update`, and why `Replace` here would destroy data
    ///
    /// `feature_screens` declares `FOREIGN KEY (feature_id) REFERENCES features(feature_id)
    /// ON DELETE CASCADE` (`schema.rs:478`), and `PRAGMA foreign_keys` is `ON`
    /// (`connection.rs:25`, `:44`). `INSERT OR REPLACE` is **delete-then-insert**, not an upsert of
    /// the named columns — so re-syncing a feature that already exists would cascade-delete every
    /// one of its screens. The disposition is not a style choice on this table; it is the
    /// difference between a catalogue refresh and losing the screen library.
    ///
    /// The `set` list is copied from the shipped `DO UPDATE SET` clause rather than assumed to be
    /// every column: `feature_id` is the conflict key and is not reassigned.
    pub const FEATURE_ROW: RowMapping<Feature> = for "features" {
        feature_id,
        name,
        name_ar         via column::OPTIONAL_TEXT,
        config_key      via column::OPTIONAL_TEXT,
        is_core,
        is_enabled,
        icon            via column::OPTIONAL_TEXT,
        display_order,
        updated_at,
    } on_conflict OnConflict::Update {
        key: &["feature_id"],
        set: &[
            "name",
            "name_ar",
            "config_key",
            "is_core",
            "is_enabled",
            "icon",
            "display_order",
            "updated_at",
        ],
    };
}

row_mapping! {
    /// Every column of `feature_screens` this till reads or writes, declared once.
    ///
    /// # `Update` again, and here for a second reason
    ///
    /// `id` is `INTEGER PRIMARY KEY AUTOINCREMENT` (`schema.rs:470`) and the natural key is
    /// `UNIQUE(feature_id, screen_id)` (`:479`). Delete-and-reinsert would assign a **new `id`**;
    /// `DO UPDATE` preserves it. `id` is not in the projection at all — the store owns it, and
    /// `FeatureScreen` has no field for it.
    pub const FEATURE_SCREEN_ROW: RowMapping<FeatureScreen> = for "feature_screens" {
        feature_id,
        screen_id,
        name,
        name_ar         via column::OPTIONAL_TEXT,
        is_entry_point,
        next_screen     via column::OPTIONAL_TEXT,
        display_order,
    } on_conflict OnConflict::Update {
        key: &["feature_id", "screen_id"],
        set: &[
            "name",
            "name_ar",
            "is_entry_point",
            "next_screen",
            "display_order",
        ],
    };
}

impl Database {
    /// Inserts a feature, or updates it in place if it is already known.
    ///
    /// In place: see [`FEATURE_ROW`] on why a delete-and-reinsert would take the screens with it.
    pub fn upsert_feature(&self, feature: &Feature) -> SqliteResult<()> {
        self.insert(&FEATURE_ROW, feature)?;
        Ok(())
    }

    /// Inserts a feature screen, or updates it in place if it is already known.
    ///
    /// In place: see [`FEATURE_SCREEN_ROW`] on why a delete-and-reinsert would renumber it.
    pub fn upsert_feature_screen(&self, screen: &FeatureScreen) -> SqliteResult<()> {
        self.insert(&FEATURE_SCREEN_ROW, screen)?;
        Ok(())
    }

    /// Gets a feature by ID.
    pub fn get_feature(&self, feature_id: &str) -> SqliteResult<Option<Feature>> {
        self.select_one(
            FEATURE_ROW.reader(),
            "FROM features WHERE feature_id = ?1",
            params![feature_id],
        )
    }

    /// Gets the enabled features, in display order.
    pub fn get_enabled_features(&self) -> SqliteResult<Vec<Feature>> {
        self.select_all(
            FEATURE_ROW.reader(),
            "FROM features WHERE is_enabled = 1 ORDER BY display_order",
            [],
        )
    }

    /// Gets every feature, enabled or not, in display order.
    pub fn get_all_features(&self) -> SqliteResult<Vec<Feature>> {
        self.select_all(
            FEATURE_ROW.reader(),
            "FROM features ORDER BY display_order",
            [],
        )
    }

    /// Gets one feature's screens, in display order.
    pub fn get_feature_screens(&self, feature_id: &str) -> SqliteResult<Vec<FeatureScreen>> {
        self.select_all(
            FEATURE_SCREEN_ROW.reader(),
            "FROM feature_screens WHERE feature_id = ?1 ORDER BY display_order",
            params![feature_id],
        )
    }

    /// Gets a screen by its screen ID.
    pub fn get_screen(&self, screen_id: &str) -> SqliteResult<Option<FeatureScreen>> {
        self.select_one(
            FEATURE_SCREEN_ROW.reader(),
            "FROM feature_screens WHERE screen_id = ?1",
            params![screen_id],
        )
    }

    /// Checks if a screen is enabled (i.e., its parent feature is enabled)
    pub fn is_screen_enabled(&self, screen_id: &str) -> SqliteResult<bool> {
        let count: i64 = self.select_scalar(
            "SELECT COUNT(*) FROM feature_screens fs \
             INNER JOIN features f ON fs.feature_id = f.feature_id \
             WHERE fs.screen_id = ?1 AND f.is_enabled = 1",
            params![screen_id],
        )?;

        Ok(count > 0)
    }

    /// Checks if a feature is enabled
    pub fn is_feature_enabled(&self, feature_id: &str) -> SqliteResult<bool> {
        let count: i64 = self.select_scalar(
            "SELECT COUNT(*) FROM features WHERE feature_id = ?1 AND is_enabled = 1",
            params![feature_id],
        )?;

        Ok(count > 0)
    }

    /// Gets all enabled screen IDs
    pub fn get_enabled_screen_ids(&self) -> SqliteResult<Vec<String>> {
        self.select_scalars(
            "SELECT fs.screen_id FROM feature_screens fs \
             INNER JOIN features f ON fs.feature_id = f.feature_id \
             WHERE f.is_enabled = 1 \
             ORDER BY f.display_order, fs.display_order",
            [],
        )
    }

    /// Gets a feature's entry-point screen.
    pub fn get_feature_entry_screen(
        &self,
        feature_id: &str,
    ) -> SqliteResult<Option<FeatureScreen>> {
        self.select_one(
            FEATURE_SCREEN_ROW.reader(),
            "FROM feature_screens WHERE feature_id = ?1 AND is_entry_point = 1",
            params![feature_id],
        )
    }

    /// Gets the feature ID for a screen
    pub fn get_screen_feature(&self, screen_id: &str) -> SqliteResult<Option<String>> {
        self.select_optional_scalar(
            "SELECT feature_id FROM feature_screens WHERE screen_id = ?1",
            params![screen_id],
        )
    }

    /// Clears all features and screens (for sync refresh)
    pub fn clear_features(&self) -> SqliteResult<()> {
        let conn = self.connection();
        let conn = conn.lock();

        // Due to ON DELETE CASCADE, deleting features will also delete screens
        conn.execute("DELETE FROM feature_screens", [])?;
        conn.execute("DELETE FROM features", [])?;
        Ok(())
    }

    /// Replaces the whole screen library with `features` and `screens`, atomically.
    ///
    /// # This carried a third and fourth hand-written column list
    ///
    /// Its own nine-column and seven-column `INSERT`s, distinct from
    /// [`Database::upsert_feature`]'s and [`Database::upsert_feature_screen`]'s, and they were
    /// **plain `INSERT`s with no conflict clause** where those two upsert. The difference is not
    /// observable here — both deletes run first, so nothing is left to conflict with — and going
    /// through the mappings makes the four statements two and leaves the disposition to the table,
    /// which is where it belongs. If a delete ever stops happening, an upsert is also the better
    /// failure: a constraint error part-way through a catalogue is not.
    ///
    pub fn sync_features(
        &self,
        features: &[Feature],
        screens: &[FeatureScreen],
    ) -> SqliteResult<()> {
        let conn = self.connection();
        let conn = conn.lock();

        let transaction = conn.unchecked_transaction()?;

        // Screens first. `feature_screens` references `features` with `ON DELETE CASCADE`, so the
        // second delete would take them anyway — but doing it explicitly keeps the order readable
        // and does not rest on the cascade being on.
        conn.execute("DELETE FROM feature_screens", [])?;
        conn.execute("DELETE FROM features", [])?;

        for feature in features {
            projection::write(&conn, &FEATURE_ROW, feature)?;
        }
        for screen in screens {
            projection::write(&conn, &FEATURE_SCREEN_ROW, screen)?;
        }

        transaction.commit()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::migrations::run_migrations;

    fn setup_test_db() -> Database {
        let db = Database::in_memory().unwrap();
        {
            let conn = db.connection();
            let conn = conn.lock();
            run_migrations(&conn).unwrap();
        }
        db
    }

    #[test]
    fn test_create_features_table() {
        let db = setup_test_db();
        let conn = db.connection();
        let conn = conn.lock();

        // Tables should be created during migration
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='features'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='feature_screens'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn test_upsert_feature() {
        let db = setup_test_db();

        let feature = Feature {
            feature_id: "returns".to_string(),
            name: "Returns".to_string(),
            name_ar: Some("المرتجعات".to_string()),
            config_key: Some("allowReturns".to_string()),
            is_core: false,
            is_enabled: true,
            icon: Some("rotate-ccw".to_string()),
            display_order: 60,
            updated_at: "2024-01-01T00:00:00Z".to_string(),
        };

        db.upsert_feature(&feature).unwrap();

        let loaded = db.get_feature("returns").unwrap();
        assert!(loaded.is_some());
        let loaded = loaded.unwrap();
        assert_eq!(loaded.name, "Returns");
        assert_eq!(loaded.name_ar, Some("المرتجعات".to_string()));
        assert!(loaded.is_enabled);
    }

    #[test]
    fn test_upsert_feature_update() {
        let db = setup_test_db();

        let feature = Feature {
            feature_id: "returns".to_string(),
            name: "Returns".to_string(),
            is_enabled: true,
            updated_at: "2024-01-01T00:00:00Z".to_string(),
            ..Default::default()
        };

        db.upsert_feature(&feature).unwrap();

        // Update the feature
        let updated_feature = Feature {
            feature_id: "returns".to_string(),
            name: "Returns Updated".to_string(),
            is_enabled: false,
            updated_at: "2024-01-02T00:00:00Z".to_string(),
            ..Default::default()
        };

        db.upsert_feature(&updated_feature).unwrap();

        let loaded = db.get_feature("returns").unwrap().unwrap();
        assert_eq!(loaded.name, "Returns Updated");
        assert!(!loaded.is_enabled);
    }

    #[test]
    fn test_get_enabled_features() {
        let db = setup_test_db();

        db.upsert_feature(&Feature {
            feature_id: "checkout".to_string(),
            name: "Checkout".to_string(),
            is_core: true,
            is_enabled: true,
            display_order: 10,
            updated_at: "2024-01-01T00:00:00Z".to_string(),
            ..Default::default()
        })
        .unwrap();

        db.upsert_feature(&Feature {
            feature_id: "returns".to_string(),
            name: "Returns".to_string(),
            is_core: false,
            is_enabled: false,
            display_order: 20,
            updated_at: "2024-01-01T00:00:00Z".to_string(),
            ..Default::default()
        })
        .unwrap();

        let enabled = db.get_enabled_features().unwrap();
        assert_eq!(enabled.len(), 1);
        assert_eq!(enabled[0].feature_id, "checkout");
    }

    #[test]
    fn test_upsert_feature_screen() {
        let db = setup_test_db();

        db.upsert_feature(&Feature {
            feature_id: "returns".to_string(),
            name: "Returns".to_string(),
            is_enabled: true,
            updated_at: "2024-01-01T00:00:00Z".to_string(),
            ..Default::default()
        })
        .unwrap();

        let screen = FeatureScreen {
            feature_id: "returns".to_string(),
            screen_id: "return-entry".to_string(),
            name: "Return Entry".to_string(),
            name_ar: None,
            is_entry_point: true,
            next_screen: Some("return-items".to_string()),
            display_order: 10,
        };

        db.upsert_feature_screen(&screen).unwrap();

        let screens = db.get_feature_screens("returns").unwrap();
        assert_eq!(screens.len(), 1);
        assert!(screens[0].is_entry_point);
        assert_eq!(screens[0].next_screen, Some("return-items".to_string()));
    }

    #[test]
    fn test_is_screen_enabled() {
        let db = setup_test_db();

        db.upsert_feature(&Feature {
            feature_id: "returns".to_string(),
            name: "Returns".to_string(),
            is_enabled: true,
            updated_at: "2024-01-01T00:00:00Z".to_string(),
            ..Default::default()
        })
        .unwrap();

        db.upsert_feature_screen(&FeatureScreen {
            feature_id: "returns".to_string(),
            screen_id: "return-entry".to_string(),
            name: "Return Entry".to_string(),
            ..Default::default()
        })
        .unwrap();

        assert!(db.is_screen_enabled("return-entry").unwrap());
        assert!(!db.is_screen_enabled("nonexistent-screen").unwrap());
    }

    #[test]
    fn test_is_screen_enabled_when_feature_disabled() {
        let db = setup_test_db();

        db.upsert_feature(&Feature {
            feature_id: "returns".to_string(),
            name: "Returns".to_string(),
            is_enabled: false, // Feature is disabled
            updated_at: "2024-01-01T00:00:00Z".to_string(),
            ..Default::default()
        })
        .unwrap();

        db.upsert_feature_screen(&FeatureScreen {
            feature_id: "returns".to_string(),
            screen_id: "return-entry".to_string(),
            name: "Return Entry".to_string(),
            ..Default::default()
        })
        .unwrap();

        // Screen should not be enabled since feature is disabled
        assert!(!db.is_screen_enabled("return-entry").unwrap());
    }

    #[test]
    fn test_is_feature_enabled() {
        let db = setup_test_db();

        db.upsert_feature(&Feature {
            feature_id: "checkout".to_string(),
            name: "Checkout".to_string(),
            is_enabled: true,
            updated_at: "2024-01-01T00:00:00Z".to_string(),
            ..Default::default()
        })
        .unwrap();

        db.upsert_feature(&Feature {
            feature_id: "returns".to_string(),
            name: "Returns".to_string(),
            is_enabled: false,
            updated_at: "2024-01-01T00:00:00Z".to_string(),
            ..Default::default()
        })
        .unwrap();

        assert!(db.is_feature_enabled("checkout").unwrap());
        assert!(!db.is_feature_enabled("returns").unwrap());
        assert!(!db.is_feature_enabled("nonexistent").unwrap());
    }

    #[test]
    fn test_get_enabled_screen_ids() {
        let db = setup_test_db();

        // Setup checkout feature (enabled)
        db.upsert_feature(&Feature {
            feature_id: "checkout".to_string(),
            name: "Checkout".to_string(),
            is_enabled: true,
            display_order: 10,
            updated_at: "2024-01-01T00:00:00Z".to_string(),
            ..Default::default()
        })
        .unwrap();

        db.upsert_feature_screen(&FeatureScreen {
            feature_id: "checkout".to_string(),
            screen_id: "checkout-main".to_string(),
            name: "Checkout".to_string(),
            display_order: 10,
            ..Default::default()
        })
        .unwrap();

        // Setup returns feature (disabled)
        db.upsert_feature(&Feature {
            feature_id: "returns".to_string(),
            name: "Returns".to_string(),
            is_enabled: false,
            display_order: 20,
            updated_at: "2024-01-01T00:00:00Z".to_string(),
            ..Default::default()
        })
        .unwrap();

        db.upsert_feature_screen(&FeatureScreen {
            feature_id: "returns".to_string(),
            screen_id: "return-entry".to_string(),
            name: "Return Entry".to_string(),
            display_order: 10,
            ..Default::default()
        })
        .unwrap();

        let enabled_screens = db.get_enabled_screen_ids().unwrap();
        assert_eq!(enabled_screens.len(), 1);
        assert!(enabled_screens.contains(&"checkout-main".to_string()));
        assert!(!enabled_screens.contains(&"return-entry".to_string()));
    }

    #[test]
    fn test_get_feature_entry_screen() {
        let db = setup_test_db();

        db.upsert_feature(&Feature {
            feature_id: "returns".to_string(),
            name: "Returns".to_string(),
            is_enabled: true,
            updated_at: "2024-01-01T00:00:00Z".to_string(),
            ..Default::default()
        })
        .unwrap();

        db.upsert_feature_screen(&FeatureScreen {
            feature_id: "returns".to_string(),
            screen_id: "return-entry".to_string(),
            name: "Return Entry".to_string(),
            is_entry_point: true,
            ..Default::default()
        })
        .unwrap();

        db.upsert_feature_screen(&FeatureScreen {
            feature_id: "returns".to_string(),
            screen_id: "return-items".to_string(),
            name: "Return Items".to_string(),
            is_entry_point: false,
            ..Default::default()
        })
        .unwrap();

        let entry = db.get_feature_entry_screen("returns").unwrap();
        assert!(entry.is_some());
        assert_eq!(entry.unwrap().screen_id, "return-entry");
    }

    #[test]
    fn test_get_screen_feature() {
        let db = setup_test_db();

        db.upsert_feature(&Feature {
            feature_id: "returns".to_string(),
            name: "Returns".to_string(),
            is_enabled: true,
            updated_at: "2024-01-01T00:00:00Z".to_string(),
            ..Default::default()
        })
        .unwrap();

        db.upsert_feature_screen(&FeatureScreen {
            feature_id: "returns".to_string(),
            screen_id: "return-entry".to_string(),
            name: "Return Entry".to_string(),
            ..Default::default()
        })
        .unwrap();

        let feature_id = db.get_screen_feature("return-entry").unwrap();
        assert_eq!(feature_id, Some("returns".to_string()));

        let feature_id = db.get_screen_feature("nonexistent").unwrap();
        assert!(feature_id.is_none());
    }

    #[test]
    fn test_clear_features() {
        let db = setup_test_db();

        db.upsert_feature(&Feature {
            feature_id: "checkout".to_string(),
            name: "Checkout".to_string(),
            is_enabled: true,
            updated_at: "2024-01-01T00:00:00Z".to_string(),
            ..Default::default()
        })
        .unwrap();

        db.upsert_feature_screen(&FeatureScreen {
            feature_id: "checkout".to_string(),
            screen_id: "checkout-main".to_string(),
            name: "Checkout".to_string(),
            ..Default::default()
        })
        .unwrap();

        // Verify data exists
        assert!(db.get_feature("checkout").unwrap().is_some());

        // Clear all features
        db.clear_features().unwrap();

        // Verify data is gone
        assert!(db.get_feature("checkout").unwrap().is_none());
        let screens = db.get_feature_screens("checkout").unwrap();
        assert!(screens.is_empty());
    }

    #[test]
    fn test_sync_features() {
        let db = setup_test_db();

        // Initial sync
        let features = vec![
            Feature {
                feature_id: "checkout".to_string(),
                name: "Checkout".to_string(),
                is_enabled: true,
                display_order: 10,
                updated_at: "2024-01-01T00:00:00Z".to_string(),
                ..Default::default()
            },
            Feature {
                feature_id: "returns".to_string(),
                name: "Returns".to_string(),
                is_enabled: false,
                display_order: 20,
                updated_at: "2024-01-01T00:00:00Z".to_string(),
                ..Default::default()
            },
        ];

        let screens = vec![
            FeatureScreen {
                feature_id: "checkout".to_string(),
                screen_id: "checkout-main".to_string(),
                name: "Checkout".to_string(),
                is_entry_point: true,
                display_order: 10,
                ..Default::default()
            },
            FeatureScreen {
                feature_id: "returns".to_string(),
                screen_id: "return-entry".to_string(),
                name: "Return Entry".to_string(),
                is_entry_point: true,
                display_order: 10,
                ..Default::default()
            },
        ];

        db.sync_features(&features, &screens).unwrap();

        // Verify data
        let all_features = db.get_all_features().unwrap();
        assert_eq!(all_features.len(), 2);

        let enabled = db.get_enabled_features().unwrap();
        assert_eq!(enabled.len(), 1);
        assert_eq!(enabled[0].feature_id, "checkout");
    }

    // ------------------------------------------------------------------------------------------
    // `FEATURE_ROW` and `FEATURE_SCREEN_ROW`. The three patterns from task 04, plus the one this
    // table needs that no other does: the second write must not take the children with it.
    // ------------------------------------------------------------------------------------------

    fn a_feature_with_no_two_columns_alike() -> Feature {
        Feature {
            feature_id: "feature-id-column".to_string(),
            name: "name-column".to_string(),
            name_ar: Some("name-ar-column".to_string()),
            config_key: Some("config-key-column".to_string()),
            // Not the column defaults (`is_core` 0, `is_enabled` 1): a value that never reached the
            // store would otherwise read back as one that did.
            is_core: true,
            is_enabled: false,
            icon: Some("icon-column".to_string()),
            display_order: 7,
            updated_at: "2026-08-24T10:00:00Z".to_string(),
        }
    }

    fn a_screen_with_no_two_columns_alike() -> FeatureScreen {
        FeatureScreen {
            feature_id: "feature-id-column".to_string(),
            screen_id: "screen-id-column".to_string(),
            name: "screen-name-column".to_string(),
            name_ar: Some("screen-name-ar-column".to_string()),
            is_entry_point: true,
            next_screen: Some("next-screen-column".to_string()),
            display_order: 9,
        }
    }

    #[test]
    fn neither_mapping_can_delete_and_reinsert() {
        // The property this whole task is about, asserted on the rendered SQL rather than trusted.
        // `INSERT OR REPLACE` on `features` cascade-deletes the screens; on `feature_screens` it
        // renumbers the `AUTOINCREMENT` id.
        for statement in [
            FEATURE_ROW.insert_statement(),
            FEATURE_SCREEN_ROW.insert_statement(),
        ] {
            assert!(
                !statement.contains("OR REPLACE"),
                "a delete-and-reinsert reached a table with children: {statement}"
            );
            assert!(statement.contains("ON CONFLICT"), "{statement}");
            assert!(statement.contains("DO UPDATE SET"), "{statement}");
        }
        assert!(FEATURE_ROW
            .insert_statement()
            .contains("ON CONFLICT(feature_id) DO UPDATE SET"));
        assert!(FEATURE_SCREEN_ROW
            .insert_statement()
            .contains("ON CONFLICT(feature_id, screen_id) DO UPDATE SET"));
        // The conflict key is not among the reassigned columns, in either.
        assert!(!FEATURE_ROW
            .insert_statement()
            .contains("feature_id = excluded.feature_id"));
        assert!(!FEATURE_SCREEN_ROW
            .insert_statement()
            .contains("screen_id = excluded.screen_id"));
    }

    #[test]
    fn re_syncing_a_feature_keeps_its_screens() {
        // **The test a single write cannot fail.** Under `INSERT OR REPLACE` the first write
        // passes, the screens are there, and everything looks correct — the cascade only fires on
        // the *second* write of the same `feature_id`, which is what every catalogue refresh does.
        let db = setup_test_db();
        db.upsert_feature(&a_feature_with_no_two_columns_alike())
            .unwrap();
        db.upsert_feature_screen(&a_screen_with_no_two_columns_alike())
            .unwrap();
        assert_eq!(
            db.get_feature_screens("feature-id-column").unwrap().len(),
            1
        );

        // The same feature again, as a re-sync sends it, with one column changed.
        db.upsert_feature(&Feature {
            name: "renamed".to_string(),
            ..a_feature_with_no_two_columns_alike()
        })
        .unwrap();

        let screens = db.get_feature_screens("feature-id-column").unwrap();
        assert_eq!(
            screens.len(),
            1,
            "re-syncing the feature cascade-deleted its screens"
        );
        assert_eq!(screens[0].screen_id, "screen-id-column");
        assert_eq!(
            db.get_feature("feature-id-column").unwrap().unwrap().name,
            "renamed",
            "the update did not take"
        );
    }

    #[test]
    fn re_syncing_a_screen_keeps_the_row_it_was_given() {
        // The second reason for `Update`: `id` is `AUTOINCREMENT`, so delete-and-reinsert hands
        // the screen a new one. Nothing in `FeatureScreen` carries `id`, so this reads it directly.
        let db = setup_test_db();
        db.upsert_feature(&a_feature_with_no_two_columns_alike())
            .unwrap();
        db.upsert_feature_screen(&a_screen_with_no_two_columns_alike())
            .unwrap();
        let first_id: i64 = db
            .select_scalar(
                "SELECT id FROM feature_screens WHERE screen_id = ?1",
                ["screen-id-column"],
            )
            .unwrap();

        db.upsert_feature_screen(&FeatureScreen {
            name: "renamed".to_string(),
            ..a_screen_with_no_two_columns_alike()
        })
        .unwrap();

        let second_id: i64 = db
            .select_scalar(
                "SELECT id FROM feature_screens WHERE screen_id = ?1",
                ["screen-id-column"],
            )
            .unwrap();
        assert_eq!(second_id, first_id, "the screen was renumbered");
        let rows: i64 = db
            .select_scalar("SELECT COUNT(*) FROM feature_screens", [])
            .unwrap();
        assert_eq!(rows, 1, "the second upsert inserted a duplicate");
        assert_eq!(
            db.get_screen("screen-id-column").unwrap().unwrap().name,
            "renamed"
        );
    }

    #[test]
    fn the_feature_mappings_name_every_column_in_the_order_they_read_them() {
        assert_eq!(
            FEATURE_ROW.reader().select_list(),
            "feature_id, name, name_ar, config_key, is_core, is_enabled, icon, display_order, \
             updated_at"
        );
        assert_eq!(FEATURE_ROW.reader().width(), 9);
        assert_eq!(
            FEATURE_SCREEN_ROW.reader().select_list(),
            "feature_id, screen_id, name, name_ar, is_entry_point, next_screen, display_order"
        );
        assert_eq!(FEATURE_SCREEN_ROW.reader().width(), 7);
        // `id` is the store's and is in neither projection nor either insert.
        assert!(!FEATURE_SCREEN_ROW.insert_column_names().any(|c| c == "id"));
    }

    #[test]
    fn every_column_of_a_fully_distinct_feature_survives_the_round_trip() {
        let db = setup_test_db();
        let written = a_feature_with_no_two_columns_alike();
        db.upsert_feature(&written).unwrap();

        let read = db.get_feature("feature-id-column").unwrap().unwrap();
        assert_eq!(read.feature_id, written.feature_id);
        assert_eq!(read.name, written.name);
        assert_eq!(read.name_ar, written.name_ar);
        assert_eq!(read.config_key, written.config_key);
        assert_eq!(read.is_core, written.is_core);
        assert_eq!(read.is_enabled, written.is_enabled);
        assert_eq!(read.icon, written.icon);
        assert_eq!(read.display_order, written.display_order);
        assert_eq!(read.updated_at, written.updated_at);
    }

    #[test]
    fn every_column_of_a_fully_distinct_screen_survives_the_round_trip() {
        let db = setup_test_db();
        db.upsert_feature(&a_feature_with_no_two_columns_alike())
            .unwrap();
        let written = a_screen_with_no_two_columns_alike();
        db.upsert_feature_screen(&written).unwrap();

        let read = db.get_screen("screen-id-column").unwrap().unwrap();
        assert_eq!(read.feature_id, written.feature_id);
        assert_eq!(read.screen_id, written.screen_id);
        assert_eq!(read.name, written.name);
        assert_eq!(read.name_ar, written.name_ar);
        assert_eq!(read.is_entry_point, written.is_entry_point);
        assert_eq!(read.next_screen, written.next_screen);
        assert_eq!(read.display_order, written.display_order);
    }

    /// Reads every column of the single stored feature and screen back **by name**.
    fn assert_every_feature_column_holds_its_own_value(db: &Database) {
        let conn = db.connection();
        let conn = conn.lock();
        for (table, column, expected) in [
            ("features", "feature_id", "feature-id-column"),
            ("features", "name", "name-column"),
            ("features", "name_ar", "name-ar-column"),
            ("features", "config_key", "config-key-column"),
            ("features", "icon", "icon-column"),
            ("features", "updated_at", "2026-08-24T10:00:00Z"),
            ("feature_screens", "feature_id", "feature-id-column"),
            ("feature_screens", "screen_id", "screen-id-column"),
            ("feature_screens", "name", "screen-name-column"),
            ("feature_screens", "name_ar", "screen-name-ar-column"),
            ("feature_screens", "next_screen", "next-screen-column"),
        ] {
            let matched: bool = conn
                .query_row(
                    &format!("SELECT {column} = ?1 FROM {table}"),
                    [expected],
                    |row| row.get(0),
                )
                .unwrap();
            assert!(matched, "`{table}.{column}` does not hold `{expected}`");
        }
        // The flags and orders. `is_core` is 1 where `is_enabled` is 0, so a swap between the two
        // adjacent booleans is visible; two equal flags would hide it.
        for (table, column, expected) in [
            ("features", "is_core", 1_i64),
            ("features", "is_enabled", 0),
            ("features", "display_order", 7),
            ("feature_screens", "is_entry_point", 1),
            ("feature_screens", "display_order", 9),
        ] {
            let stored: i64 = conn
                .query_row(&format!("SELECT {column} FROM {table}"), [], |row| {
                    row.get(0)
                })
                .unwrap();
            assert_eq!(stored, expected, "`{table}.{column}`");
        }
    }

    #[test]
    fn upsert_puts_each_value_in_the_column_that_carries_its_name() {
        let db = setup_test_db();
        db.upsert_feature(&a_feature_with_no_two_columns_alike())
            .unwrap();
        db.upsert_feature_screen(&a_screen_with_no_two_columns_alike())
            .unwrap();
        assert_every_feature_column_holds_its_own_value(&db);
    }

    #[test]
    fn sync_features_puts_each_value_in_the_column_that_carries_its_name() {
        // The second writer. It carried its own two column lists, and unlike the upserts they were
        // plain `INSERT`s.
        let db = setup_test_db();
        db.sync_features(
            &[a_feature_with_no_two_columns_alike()],
            &[a_screen_with_no_two_columns_alike()],
        )
        .unwrap();
        assert_every_feature_column_holds_its_own_value(&db);
    }

    #[test]
    fn a_null_in_one_feature_column_reaches_that_columns_field_and_no_other() {
        let db = setup_test_db();
        let full = a_feature_with_no_two_columns_alike();
        for (column, blank, absent) in [
            (
                "name_ar",
                (|f: &mut Feature| f.name_ar = None) as fn(&mut Feature),
                (|f: &Feature| {
                    assert_eq!(f.name_ar, None);
                    assert_eq!(f.config_key.as_deref(), Some("config-key-column"));
                }) as fn(&Feature),
            ),
            (
                "config_key",
                |f: &mut Feature| f.config_key = None,
                |f: &Feature| {
                    assert_eq!(f.config_key, None);
                    assert_eq!(f.icon.as_deref(), Some("icon-column"));
                },
            ),
            (
                "icon",
                |f: &mut Feature| f.icon = None,
                |f: &Feature| {
                    assert_eq!(f.icon, None);
                    assert_eq!(f.display_order, 7);
                },
            ),
        ] {
            let mut written = full.clone();
            blank(&mut written);
            db.upsert_feature(&written).unwrap();

            let stored: Option<String> = db
                .select_scalar(&format!("SELECT {column} FROM features"), [])
                .unwrap();
            assert_eq!(stored, None, "`{column}` was not written as NULL");
            absent(&db.get_feature("feature-id-column").unwrap().unwrap());
        }
    }

    #[test]
    fn every_reader_of_the_feature_tables_returns_the_same_row() {
        let db = setup_test_db();
        db.upsert_feature(&Feature {
            is_enabled: true,
            ..a_feature_with_no_two_columns_alike()
        })
        .unwrap();
        db.upsert_feature_screen(&a_screen_with_no_two_columns_alike())
            .unwrap();

        let one = db.get_feature("feature-id-column").unwrap().unwrap();
        let enabled = db.get_enabled_features().unwrap();
        let all = db.get_all_features().unwrap();
        assert_eq!(enabled.len(), 1);
        assert_eq!(all.len(), 1);
        for (name, feature) in [
            ("get_enabled_features", &enabled[0]),
            ("get_all_features", &all[0]),
        ] {
            assert_eq!(feature.feature_id, one.feature_id, "{name}");
            assert_eq!(feature.config_key, one.config_key, "{name}");
            assert_eq!(feature.is_core, one.is_core, "{name}");
            assert_eq!(feature.display_order, one.display_order, "{name}");
        }

        let screen = db.get_screen("screen-id-column").unwrap().unwrap();
        let by_feature = db.get_feature_screens("feature-id-column").unwrap();
        let entry = db
            .get_feature_entry_screen("feature-id-column")
            .unwrap()
            .expect("the entry-point screen");
        assert_eq!(by_feature.len(), 1);
        for (name, s) in [
            ("get_feature_screens", &by_feature[0]),
            ("get_feature_entry_screen", &entry),
        ] {
            assert_eq!(s.screen_id, screen.screen_id, "{name}");
            assert_eq!(s.next_screen, screen.next_screen, "{name}");
            assert_eq!(s.is_entry_point, screen.is_entry_point, "{name}");
            assert_eq!(s.display_order, screen.display_order, "{name}");
        }
    }
}
