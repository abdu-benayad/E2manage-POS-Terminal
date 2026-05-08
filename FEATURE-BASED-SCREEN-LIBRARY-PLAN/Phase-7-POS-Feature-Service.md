# Phase 7: POS - Feature Service

**Time:** 1.5 hours
**Type:** POS Terminal (Rust)

Create FeatureService for screen access checks and feature navigation.

---

## Pre-Flight Checklist

- [ ] Phase 6 completed (DB schema ready)
- [ ] `pos-services` crate exists
- [ ] Database operations work

---

## 1. Tests First (TDD)

**File:** `crates/pos-services/src/feature_service_tests.rs`

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use pos_db::Database;
    use pos_models::{Feature, FeatureScreen};
    use std::sync::Arc;
    use tempfile::tempdir;

    fn setup_test_service() -> FeatureService {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let db = Arc::new(Database::new(db_path.to_str().unwrap()).unwrap());
        FeatureService::new(db)
    }

    fn seed_features(db: &Database) {
        // Core feature (always enabled)
        db.upsert_feature(&Feature {
            feature_id: "checkout".to_string(),
            name: "Checkout".to_string(),
            is_core: true,
            is_enabled: true,
            ..Default::default()
        }).unwrap();

        db.upsert_feature_screen(&FeatureScreen {
            feature_id: "checkout".to_string(),
            screen_id: "checkout".to_string(),
            name: "Checkout".to_string(),
            is_entry_point: true,
            ..Default::default()
        }).unwrap();

        // Optional enabled feature
        db.upsert_feature(&Feature {
            feature_id: "returns".to_string(),
            name: "Returns".to_string(),
            is_core: false,
            is_enabled: true,
            ..Default::default()
        }).unwrap();

        db.upsert_feature_screen(&FeatureScreen {
            feature_id: "returns".to_string(),
            screen_id: "return-entry".to_string(),
            name: "Return Entry".to_string(),
            is_entry_point: true,
            next_screen: Some("return-items".to_string()),
            ..Default::default()
        }).unwrap();

        db.upsert_feature_screen(&FeatureScreen {
            feature_id: "returns".to_string(),
            screen_id: "return-items".to_string(),
            name: "Return Items".to_string(),
            is_entry_point: false,
            next_screen: Some("refund".to_string()),
            ..Default::default()
        }).unwrap();

        // Disabled feature
        db.upsert_feature(&Feature {
            feature_id: "drafts".to_string(),
            name: "Drafts".to_string(),
            is_core: false,
            is_enabled: false,
            ..Default::default()
        }).unwrap();

        db.upsert_feature_screen(&FeatureScreen {
            feature_id: "drafts".to_string(),
            screen_id: "save-draft".to_string(),
            name: "Save Draft".to_string(),
            ..Default::default()
        }).unwrap();
    }

    #[test]
    fn test_is_screen_enabled_for_enabled_feature() {
        let service = setup_test_service();
        seed_features(&service.db);

        assert!(service.is_screen_enabled("checkout").unwrap());
        assert!(service.is_screen_enabled("return-entry").unwrap());
    }

    #[test]
    fn test_is_screen_disabled_for_disabled_feature() {
        let service = setup_test_service();
        seed_features(&service.db);

        assert!(!service.is_screen_enabled("save-draft").unwrap());
    }

    #[test]
    fn test_is_screen_disabled_for_unknown_screen() {
        let service = setup_test_service();
        seed_features(&service.db);

        assert!(!service.is_screen_enabled("unknown-screen").unwrap());
    }

    #[test]
    fn test_get_enabled_screen_ids() {
        let service = setup_test_service();
        seed_features(&service.db);

        let screens = service.get_enabled_screen_ids().unwrap();

        assert!(screens.contains(&"checkout".to_string()));
        assert!(screens.contains(&"return-entry".to_string()));
        assert!(!screens.contains(&"save-draft".to_string()));
    }

    #[test]
    fn test_get_next_screen() {
        let service = setup_test_service();
        seed_features(&service.db);

        let next = service.get_next_screen("return-entry").unwrap();
        assert_eq!(next, Some("return-items".to_string()));

        let next = service.get_next_screen("return-items").unwrap();
        assert_eq!(next, Some("refund".to_string()));
    }

    #[test]
    fn test_get_feature_entry_screen() {
        let service = setup_test_service();
        seed_features(&service.db);

        let entry = service.get_feature_entry_screen("returns").unwrap();
        assert_eq!(entry, Some("return-entry".to_string()));
    }

    #[test]
    fn test_is_feature_enabled() {
        let service = setup_test_service();
        seed_features(&service.db);

        assert!(service.is_feature_enabled("checkout").unwrap());
        assert!(service.is_feature_enabled("returns").unwrap());
        assert!(!service.is_feature_enabled("drafts").unwrap());
    }

    #[test]
    fn test_get_screen_feature() {
        let service = setup_test_service();
        seed_features(&service.db);

        let feature = service.get_screen_feature("return-entry").unwrap();
        assert_eq!(feature, Some("returns".to_string()));
    }
}
```

**Run (expect fail):**

```bash
cd e2manage-pos-terminal
cargo test --package pos-services feature_service
```

---

## 2. Service Implementation

**File:** `crates/pos-services/src/feature_service.rs`

```rust
use anyhow::{Result, Context};
use pos_db::Database;
use pos_models::{Feature, FeatureScreen};
use std::sync::Arc;
use tracing::{debug, warn};

/// Service for managing feature access and navigation
pub struct FeatureService {
    pub(crate) db: Arc<Database>,
}

impl FeatureService {
    pub fn new(db: Arc<Database>) -> Self {
        Self { db }
    }

    /// Check if a screen is accessible (its feature is enabled)
    pub fn is_screen_enabled(&self, screen_id: &str) -> Result<bool> {
        self.db.is_screen_enabled(screen_id)
            .context("Failed to check screen enabled status")
    }

    /// Check if a feature is enabled
    pub fn is_feature_enabled(&self, feature_id: &str) -> Result<bool> {
        match self.db.get_feature(feature_id)? {
            Some(feature) => Ok(feature.is_enabled),
            None => {
                debug!("Feature {} not found, treating as disabled", feature_id);
                Ok(false)
            }
        }
    }

    /// Get all enabled screen IDs
    pub fn get_enabled_screen_ids(&self) -> Result<Vec<String>> {
        let features = self.db.get_enabled_features()?;
        let mut screen_ids = Vec::new();

        for feature in features {
            let screens = self.db.get_feature_screens(&feature.feature_id)?;
            for screen in screens {
                screen_ids.push(screen.screen_id);
            }
        }

        Ok(screen_ids)
    }

    /// Get next screen in navigation flow
    pub fn get_next_screen(&self, current_screen: &str) -> Result<Option<String>> {
        let features = self.db.get_enabled_features()?;

        for feature in features {
            let screens = self.db.get_feature_screens(&feature.feature_id)?;
            for screen in screens {
                if screen.screen_id == current_screen {
                    return Ok(screen.next_screen);
                }
            }
        }

        Ok(None)
    }

    /// Get entry point screen for a feature
    pub fn get_feature_entry_screen(&self, feature_id: &str) -> Result<Option<String>> {
        let screens = self.db.get_feature_screens(feature_id)?;
        for screen in screens {
            if screen.is_entry_point {
                return Ok(Some(screen.screen_id));
            }
        }
        Ok(None)
    }

    /// Get feature ID for a screen
    pub fn get_screen_feature(&self, screen_id: &str) -> Result<Option<String>> {
        let features = self.db.get_enabled_features()?;

        for feature in features {
            let screens = self.db.get_feature_screens(&feature.feature_id)?;
            for screen in screens {
                if screen.screen_id == screen_id {
                    return Ok(Some(feature.feature_id));
                }
            }
        }

        // Also check disabled features
        let all_features = self.get_all_features()?;
        for feature in all_features {
            let screens = self.db.get_feature_screens(&feature.feature_id)?;
            for screen in screens {
                if screen.screen_id == screen_id {
                    return Ok(Some(feature.feature_id));
                }
            }
        }

        Ok(None)
    }

    /// Get all features (enabled and disabled)
    pub fn get_all_features(&self) -> Result<Vec<Feature>> {
        // Query all features including disabled ones
        let conn = &self.db.conn;
        let mut stmt = conn.prepare(
            "SELECT * FROM features ORDER BY display_order"
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

    /// Get enabled features
    pub fn get_enabled_features(&self) -> Result<Vec<Feature>> {
        self.db.get_enabled_features()
    }

    /// Validate navigation target
    pub fn can_navigate_to(&self, target_screen: &str) -> Result<bool> {
        if !self.is_screen_enabled(target_screen)? {
            warn!("Navigation blocked: screen {} is disabled", target_screen);
            return Ok(false);
        }
        Ok(true)
    }
}
```

---

## 3. Export Service

**File:** `crates/pos-services/src/lib.rs`

Add:

```rust
mod feature_service;
pub use feature_service::FeatureService;

#[cfg(test)]
mod feature_service_tests;
```

---

## 4. Error Type (if needed)

**File:** `crates/pos-services/src/feature_service.rs`

Add at top:

```rust
use thiserror::Error;

#[derive(Debug, Error)]
pub enum FeatureError {
    #[error("Feature not found: {0}")]
    NotFound(String),

    #[error("Screen disabled: {0}")]
    ScreenDisabled(String),

    #[error("Database error: {0}")]
    Database(#[from] anyhow::Error),
}

pub type FeatureResult<T> = Result<T, FeatureError>;
```

---

## 5. Verification

```bash
cd e2manage-pos-terminal

# Run tests
cargo test --package pos-services feature_service -- --nocapture

# Check compilation
cargo check --package pos-services

# Full build
cargo build
```

---

## Success Criteria

- [ ] Tests pass
- [ ] `is_screen_enabled` works correctly
- [ ] `get_enabled_screen_ids` returns only enabled screens
- [ ] `get_next_screen` returns correct navigation target
- [ ] `get_feature_entry_screen` finds entry points
- [ ] `can_navigate_to` blocks disabled screens
- [ ] `cargo build` succeeds

---

## Rollback

```bash
rm crates/pos-services/src/feature_service.rs
rm crates/pos-services/src/feature_service_tests.rs
# Remove exports from lib.rs
```

---

## Next Phase

Read and follow **Phase-8-POS-Sync-Features.md**
