# Phase 6: POS - DB Schema Features

**Time:** 1 hour
**Type:** POS Terminal (Rust)

Add SQLite tables for features and feature_screens in the POS terminal application.

---

## Pre-Flight Checklist

- [ ] Phase 5 completed (backend API ready)
- [ ] `pos-db` crate exists
- [ ] Database module compiles

---

## 1. Tests First (TDD)

**File:** `crates/pos-db/src/features_tests.rs`

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::Database;
    use tempfile::tempdir;

    fn setup_test_db() -> Database {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        Database::new(db_path.to_str().unwrap()).unwrap()
    }

    #[test]
    fn test_create_features_table() {
        let db = setup_test_db();
        // Tables should be created during migration
        let count: i64 = db.conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='features'",
            [],
            |row| row.get(0),
        ).unwrap();
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
            updated_at: chrono::Utc::now().to_rfc3339(),
        };

        db.upsert_feature(&feature).unwrap();

        let loaded = db.get_feature("returns").unwrap();
        assert!(loaded.is_some());
        assert_eq!(loaded.unwrap().name, "Returns");
    }

    #[test]
    fn test_get_enabled_features() {
        let db = setup_test_db();

        db.upsert_feature(&Feature {
            feature_id: "checkout".to_string(),
            name: "Checkout".to_string(),
            is_core: true,
            is_enabled: true,
            ..Default::default()
        }).unwrap();

        db.upsert_feature(&Feature {
            feature_id: "returns".to_string(),
            name: "Returns".to_string(),
            is_core: false,
            is_enabled: false,
            ..Default::default()
        }).unwrap();

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
            ..Default::default()
        }).unwrap();

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
    }

    #[test]
    fn test_is_screen_enabled() {
        let db = setup_test_db();

        db.upsert_feature(&Feature {
            feature_id: "returns".to_string(),
            is_enabled: true,
            ..Default::default()
        }).unwrap();

        db.upsert_feature_screen(&FeatureScreen {
            feature_id: "returns".to_string(),
            screen_id: "return-entry".to_string(),
            ..Default::default()
        }).unwrap();

        assert!(db.is_screen_enabled("return-entry").unwrap());
        assert!(!db.is_screen_enabled("nonexistent-screen").unwrap());
    }
}
```

**Run (expect fail):**

```bash
cd e2manage-pos-terminal
cargo test --package pos-db features_tests
```

---

## 2. Models

**File:** `crates/pos-models/src/feature.rs`

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Feature {
    pub feature_id: String,
    pub name: String,
    #[serde(default)]
    pub name_ar: Option<String>,
    #[serde(default)]
    pub config_key: Option<String>,
    #[serde(default)]
    pub is_core: bool,
    #[serde(default)]
    pub is_enabled: bool,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub display_order: i32,
    #[serde(default)]
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct FeatureScreen {
    pub feature_id: String,
    pub screen_id: String,
    pub name: String,
    #[serde(default)]
    pub name_ar: Option<String>,
    #[serde(default)]
    pub is_entry_point: bool,
    #[serde(default)]
    pub next_screen: Option<String>,
    #[serde(default)]
    pub display_order: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeatureDto {
    pub feature_id: String,
    pub name: String,
    #[serde(default)]
    pub name_ar: Option<String>,
    #[serde(default)]
    pub config_key: Option<String>,
    #[serde(default)]
    pub is_core: bool,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub display_order: i32,
    #[serde(default)]
    pub screens: Vec<FeatureScreenDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeatureScreenDto {
    pub screen_id: String,
    pub name: String,
    #[serde(default)]
    pub name_ar: Option<String>,
    #[serde(default)]
    pub is_entry_point: bool,
    #[serde(default)]
    pub next_screen: Option<String>,
    #[serde(default)]
    pub display_order: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeaturesResponse {
    pub features: Vec<FeatureDto>,
    pub version: String,
    pub synced_at: String,
}
```

**Update lib.rs:**

```rust
// In crates/pos-models/src/lib.rs
mod feature;
pub use feature::*;
```

---

## 3. Database Schema & Operations

**File:** `crates/pos-db/src/features.rs`

```rust
use anyhow::Result;
use rusqlite::{params, Connection};
use pos_models::{Feature, FeatureScreen};

pub const FEATURES_SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS features (
    feature_id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    name_ar TEXT,
    config_key TEXT,
    is_core INTEGER DEFAULT 0,
    is_enabled INTEGER DEFAULT 1,
    icon TEXT,
    display_order INTEGER DEFAULT 100,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS feature_screens (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    feature_id TEXT NOT NULL,
    screen_id TEXT NOT NULL,
    name TEXT NOT NULL,
    name_ar TEXT,
    is_entry_point INTEGER DEFAULT 0,
    next_screen TEXT,
    display_order INTEGER DEFAULT 100,
    FOREIGN KEY (feature_id) REFERENCES features(feature_id),
    UNIQUE(feature_id, screen_id)
);

CREATE INDEX IF NOT EXISTS idx_feature_screens_screen_id
ON feature_screens(screen_id);
"#;

impl super::Database {
    pub fn init_features_schema(&self) -> Result<()> {
        self.conn.execute_batch(FEATURES_SCHEMA)?;
        Ok(())
    }

    pub fn upsert_feature(&self, feature: &Feature) -> Result<()> {
        self.conn.execute(
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

    pub fn upsert_feature_screen(&self, screen: &FeatureScreen) -> Result<()> {
        self.conn.execute(
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

    pub fn get_feature(&self, feature_id: &str) -> Result<Option<Feature>> {
        let mut stmt = self.conn.prepare(
            "SELECT * FROM features WHERE feature_id = ?1"
        )?;

        let mut rows = stmt.query(params![feature_id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(Feature {
                feature_id: row.get(0)?,
                name: row.get(1)?,
                name_ar: row.get(2)?,
                config_key: row.get(3)?,
                is_core: row.get::<_, i32>(4)? != 0,
                is_enabled: row.get::<_, i32>(5)? != 0,
                icon: row.get(6)?,
                display_order: row.get(7)?,
                updated_at: row.get(8)?,
            }))
        } else {
            Ok(None)
        }
    }

    pub fn get_enabled_features(&self) -> Result<Vec<Feature>> {
        let mut stmt = self.conn.prepare(
            "SELECT * FROM features WHERE is_enabled = 1 ORDER BY display_order"
        )?;

        let features = stmt.query_map([], |row| {
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
        })?.collect::<Result<Vec<_>, _>>()?;

        Ok(features)
    }

    pub fn get_feature_screens(&self, feature_id: &str) -> Result<Vec<FeatureScreen>> {
        let mut stmt = self.conn.prepare(
            "SELECT * FROM feature_screens WHERE feature_id = ?1 ORDER BY display_order"
        )?;

        let screens = stmt.query_map(params![feature_id], |row| {
            Ok(FeatureScreen {
                feature_id: row.get(1)?,
                screen_id: row.get(2)?,
                name: row.get(3)?,
                name_ar: row.get(4)?,
                is_entry_point: row.get::<_, i32>(5)? != 0,
                next_screen: row.get(6)?,
                display_order: row.get(7)?,
            })
        })?.collect::<Result<Vec<_>, _>>()?;

        Ok(screens)
    }

    pub fn is_screen_enabled(&self, screen_id: &str) -> Result<bool> {
        let count: i64 = self.conn.query_row(
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

    pub fn clear_features(&self) -> Result<()> {
        self.conn.execute("DELETE FROM feature_screens", [])?;
        self.conn.execute("DELETE FROM features", [])?;
        Ok(())
    }
}
```

**Update lib.rs:**

```rust
// In crates/pos-db/src/lib.rs
mod features;
pub use features::*;
```

---

## 4. Migration Integration

Add to existing migration runner:

```rust
// In Database::new() or migrate()
self.init_features_schema()?;
```

---

## 5. Verification

```bash
cd e2manage-pos-terminal

# Run tests
cargo test --package pos-db features

# Check compilation
cargo check --package pos-db

# Full build
cargo build
```

---

## Success Criteria

- [ ] Tests pass
- [ ] `features` table created with correct schema
- [ ] `feature_screens` table created with correct schema
- [ ] CRUD operations work
- [ ] `is_screen_enabled` query joins correctly
- [ ] `cargo build` succeeds

---

## Rollback

```bash
rm crates/pos-db/src/features.rs
rm crates/pos-models/src/feature.rs
# Remove exports from lib.rs files
```

---

## Next Phase

Read and follow **Phase-7-POS-Feature-Service.md**
