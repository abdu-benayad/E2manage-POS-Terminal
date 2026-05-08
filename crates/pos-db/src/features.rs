//! Features Repository
//!
//! Handles feature and feature screen data storage and retrieval
//! for the Feature-Based Screen Library.

use pos_models::{Feature, FeatureScreen};
use rusqlite::{params, OptionalExtension, Result as SqliteResult};

use super::Database;

impl Database {
    /// Upserts a feature (insert or update)
    pub fn upsert_feature(&self, feature: &Feature) -> SqliteResult<()> {
        let conn = self.connection();
        let conn = conn.lock();

        conn.execute(
            r#"
            INSERT INTO features (
                feature_id, name, name_ar, config_key, is_core,
                is_enabled, icon, display_order, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
            ON CONFLICT(feature_id) DO UPDATE SET
                name = excluded.name,
                name_ar = excluded.name_ar,
                config_key = excluded.config_key,
                is_core = excluded.is_core,
                is_enabled = excluded.is_enabled,
                icon = excluded.icon,
                display_order = excluded.display_order,
                updated_at = excluded.updated_at
            "#,
            params![
                feature.feature_id,
                feature.name,
                feature.name_ar,
                feature.config_key,
                feature.is_core as i32,
                feature.is_enabled as i32,
                feature.icon,
                feature.display_order,
                feature.updated_at,
            ],
        )?;
        Ok(())
    }

    /// Upserts a feature screen (insert or update)
    pub fn upsert_feature_screen(&self, screen: &FeatureScreen) -> SqliteResult<()> {
        let conn = self.connection();
        let conn = conn.lock();

        conn.execute(
            r#"
            INSERT INTO feature_screens (
                feature_id, screen_id, name, name_ar,
                is_entry_point, next_screen, display_order
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            ON CONFLICT(feature_id, screen_id) DO UPDATE SET
                name = excluded.name,
                name_ar = excluded.name_ar,
                is_entry_point = excluded.is_entry_point,
                next_screen = excluded.next_screen,
                display_order = excluded.display_order
            "#,
            params![
                screen.feature_id,
                screen.screen_id,
                screen.name,
                screen.name_ar,
                screen.is_entry_point as i32,
                screen.next_screen,
                screen.display_order,
            ],
        )?;
        Ok(())
    }

    /// Gets a feature by ID
    pub fn get_feature(&self, feature_id: &str) -> SqliteResult<Option<Feature>> {
        let conn = self.connection();
        let conn = conn.lock();

        conn.query_row(
            "SELECT feature_id, name, name_ar, config_key, is_core, is_enabled, icon, display_order, updated_at
             FROM features WHERE feature_id = ?1",
            params![feature_id],
            |row| {
                Ok(Feature {
                    feature_id: row.get(0)?,
                    name: row.get(1)?,
                    name_ar: row.get(2)?,
                    config_key: row.get(3)?,
                    is_core: row.get::<_, i32>(4)? != 0,
                    is_enabled: row.get::<_, i32>(5)? != 0,
                    icon: row.get(6)?,
                    display_order: row.get(7)?,
                    updated_at: row.get(8)?,
                })
            },
        )
        .optional()
    }

    /// Gets all enabled features ordered by display_order
    pub fn get_enabled_features(&self) -> SqliteResult<Vec<Feature>> {
        let conn = self.connection();
        let conn = conn.lock();

        let mut stmt = conn.prepare(
            "SELECT feature_id, name, name_ar, config_key, is_core, is_enabled, icon, display_order, updated_at
             FROM features WHERE is_enabled = 1 ORDER BY display_order",
        )?;

        let features = stmt
            .query_map([], |row| {
                Ok(Feature {
                    feature_id: row.get(0)?,
                    name: row.get(1)?,
                    name_ar: row.get(2)?,
                    config_key: row.get(3)?,
                    is_core: row.get::<_, i32>(4)? != 0,
                    is_enabled: row.get::<_, i32>(5)? != 0,
                    icon: row.get(6)?,
                    display_order: row.get(7)?,
                    updated_at: row.get(8)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(features)
    }

    /// Gets all features (including disabled) ordered by display_order
    pub fn get_all_features(&self) -> SqliteResult<Vec<Feature>> {
        let conn = self.connection();
        let conn = conn.lock();

        let mut stmt = conn.prepare(
            "SELECT feature_id, name, name_ar, config_key, is_core, is_enabled, icon, display_order, updated_at
             FROM features ORDER BY display_order",
        )?;

        let features = stmt
            .query_map([], |row| {
                Ok(Feature {
                    feature_id: row.get(0)?,
                    name: row.get(1)?,
                    name_ar: row.get(2)?,
                    config_key: row.get(3)?,
                    is_core: row.get::<_, i32>(4)? != 0,
                    is_enabled: row.get::<_, i32>(5)? != 0,
                    icon: row.get(6)?,
                    display_order: row.get(7)?,
                    updated_at: row.get(8)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(features)
    }

    /// Gets all screens for a feature ordered by display_order
    pub fn get_feature_screens(&self, feature_id: &str) -> SqliteResult<Vec<FeatureScreen>> {
        let conn = self.connection();
        let conn = conn.lock();

        let mut stmt = conn.prepare(
            "SELECT feature_id, screen_id, name, name_ar, is_entry_point, next_screen, display_order
             FROM feature_screens WHERE feature_id = ?1 ORDER BY display_order",
        )?;

        let screens = stmt
            .query_map(params![feature_id], |row| {
                Ok(FeatureScreen {
                    feature_id: row.get(0)?,
                    screen_id: row.get(1)?,
                    name: row.get(2)?,
                    name_ar: row.get(3)?,
                    is_entry_point: row.get::<_, i32>(4)? != 0,
                    next_screen: row.get(5)?,
                    display_order: row.get(6)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(screens)
    }

    /// Gets a screen by its screen_id
    pub fn get_screen(&self, screen_id: &str) -> SqliteResult<Option<FeatureScreen>> {
        let conn = self.connection();
        let conn = conn.lock();

        conn.query_row(
            "SELECT feature_id, screen_id, name, name_ar, is_entry_point, next_screen, display_order
             FROM feature_screens WHERE screen_id = ?1",
            params![screen_id],
            |row| {
                Ok(FeatureScreen {
                    feature_id: row.get(0)?,
                    screen_id: row.get(1)?,
                    name: row.get(2)?,
                    name_ar: row.get(3)?,
                    is_entry_point: row.get::<_, i32>(4)? != 0,
                    next_screen: row.get(5)?,
                    display_order: row.get(6)?,
                })
            },
        )
        .optional()
    }

    /// Checks if a screen is enabled (i.e., its parent feature is enabled)
    pub fn is_screen_enabled(&self, screen_id: &str) -> SqliteResult<bool> {
        let conn = self.connection();
        let conn = conn.lock();

        let count: i64 = conn.query_row(
            r#"
            SELECT COUNT(*) FROM feature_screens fs
            INNER JOIN features f ON fs.feature_id = f.feature_id
            WHERE fs.screen_id = ?1 AND f.is_enabled = 1
            "#,
            params![screen_id],
            |row| row.get(0),
        )?;

        Ok(count > 0)
    }

    /// Checks if a feature is enabled
    pub fn is_feature_enabled(&self, feature_id: &str) -> SqliteResult<bool> {
        let conn = self.connection();
        let conn = conn.lock();

        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM features WHERE feature_id = ?1 AND is_enabled = 1",
            params![feature_id],
            |row| row.get(0),
        )?;

        Ok(count > 0)
    }

    /// Gets all enabled screen IDs
    pub fn get_enabled_screen_ids(&self) -> SqliteResult<Vec<String>> {
        let conn = self.connection();
        let conn = conn.lock();

        let mut stmt = conn.prepare(
            r#"
            SELECT fs.screen_id FROM feature_screens fs
            INNER JOIN features f ON fs.feature_id = f.feature_id
            WHERE f.is_enabled = 1
            ORDER BY f.display_order, fs.display_order
            "#,
        )?;

        let screen_ids = stmt
            .query_map([], |row| row.get(0))?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(screen_ids)
    }

    /// Gets the entry point screen for a feature
    pub fn get_feature_entry_screen(&self, feature_id: &str) -> SqliteResult<Option<FeatureScreen>> {
        let conn = self.connection();
        let conn = conn.lock();

        conn.query_row(
            "SELECT feature_id, screen_id, name, name_ar, is_entry_point, next_screen, display_order
             FROM feature_screens WHERE feature_id = ?1 AND is_entry_point = 1",
            params![feature_id],
            |row| {
                Ok(FeatureScreen {
                    feature_id: row.get(0)?,
                    screen_id: row.get(1)?,
                    name: row.get(2)?,
                    name_ar: row.get(3)?,
                    is_entry_point: row.get::<_, i32>(4)? != 0,
                    next_screen: row.get(5)?,
                    display_order: row.get(6)?,
                })
            },
        )
        .optional()
    }

    /// Gets the feature ID for a screen
    pub fn get_screen_feature(&self, screen_id: &str) -> SqliteResult<Option<String>> {
        let conn = self.connection();
        let conn = conn.lock();

        conn.query_row(
            "SELECT feature_id FROM feature_screens WHERE screen_id = ?1",
            params![screen_id],
            |row| row.get(0),
        )
        .optional()
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

    /// Bulk upserts features and their screens in a transaction
    pub fn sync_features(&self, features: &[Feature], screens: &[FeatureScreen]) -> SqliteResult<()> {
        let conn = self.connection();
        let conn = conn.lock();

        let tx = conn.unchecked_transaction()?;

        // Clear existing data
        tx.execute("DELETE FROM feature_screens", [])?;
        tx.execute("DELETE FROM features", [])?;

        // Insert features
        for feature in features {
            tx.execute(
                r#"
                INSERT INTO features (
                    feature_id, name, name_ar, config_key, is_core,
                    is_enabled, icon, display_order, updated_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
                "#,
                params![
                    feature.feature_id,
                    feature.name,
                    feature.name_ar,
                    feature.config_key,
                    feature.is_core as i32,
                    feature.is_enabled as i32,
                    feature.icon,
                    feature.display_order,
                    feature.updated_at,
                ],
            )?;
        }

        // Insert screens
        for screen in screens {
            tx.execute(
                r#"
                INSERT INTO feature_screens (
                    feature_id, screen_id, name, name_ar,
                    is_entry_point, next_screen, display_order
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                "#,
                params![
                    screen.feature_id,
                    screen.screen_id,
                    screen.name,
                    screen.name_ar,
                    screen.is_entry_point as i32,
                    screen.next_screen,
                    screen.display_order,
                ],
            )?;
        }

        tx.commit()?;
        Ok(())
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
}
