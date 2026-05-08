# Phase 8: POS - Sync Features

**Time:** 1 hour
**Type:** POS Terminal (Rust)

Add feature synchronization to SyncService, fetching features from backend API.

---

## Pre-Flight Checklist

- [ ] Phase 7 completed (FeatureService exists)
- [ ] Backend API `/api/pos/features/terminal` ready
- [ ] SyncService exists in pos-services

---

## 1. Tests First (TDD)

**File:** `crates/pos-services/src/sync_service_features_tests.rs`

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use pos_api::MockApiClient;
    use pos_db::Database;
    use pos_models::{FeatureDto, FeatureScreenDto, FeaturesResponse};
    use std::sync::Arc;
    use tempfile::tempdir;
    use tokio::sync::broadcast;

    fn setup_test_deps() -> (Arc<Database>, Arc<MockApiClient>) {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let db = Arc::new(Database::new(db_path.to_str().unwrap()).unwrap());
        let api = Arc::new(MockApiClient::new());
        (db, api)
    }

    fn mock_features_response() -> FeaturesResponse {
        FeaturesResponse {
            features: vec![
                FeatureDto {
                    feature_id: "checkout".to_string(),
                    name: "Checkout".to_string(),
                    name_ar: Some("نقطة البيع".to_string()),
                    is_core: true,
                    enabled: true,
                    screens: vec![
                        FeatureScreenDto {
                            screen_id: "checkout".to_string(),
                            name: "Checkout".to_string(),
                            is_entry_point: true,
                            ..Default::default()
                        },
                    ],
                    ..Default::default()
                },
                FeatureDto {
                    feature_id: "returns".to_string(),
                    name: "Returns".to_string(),
                    is_core: false,
                    enabled: true,
                    screens: vec![
                        FeatureScreenDto {
                            screen_id: "return-entry".to_string(),
                            name: "Return Entry".to_string(),
                            is_entry_point: true,
                            next_screen: Some("return-items".to_string()),
                            ..Default::default()
                        },
                        FeatureScreenDto {
                            screen_id: "return-items".to_string(),
                            name: "Return Items".to_string(),
                            is_entry_point: false,
                            ..Default::default()
                        },
                    ],
                    ..Default::default()
                },
            ],
            version: "abc123".to_string(),
            synced_at: chrono::Utc::now().to_rfc3339(),
        }
    }

    #[tokio::test]
    async fn test_sync_features_stores_in_db() {
        let (db, api) = setup_test_deps();

        api.set_features_response(mock_features_response());

        let service = SyncService::new(db.clone(), api);
        let (tx, _rx) = broadcast::channel(16);

        service.sync_features(&tx).await.unwrap();

        // Verify features stored
        let features = db.get_enabled_features().unwrap();
        assert_eq!(features.len(), 2);

        let checkout = db.get_feature("checkout").unwrap();
        assert!(checkout.is_some());
        assert!(checkout.unwrap().is_core);
    }

    #[tokio::test]
    async fn test_sync_features_stores_screens() {
        let (db, api) = setup_test_deps();

        api.set_features_response(mock_features_response());

        let service = SyncService::new(db.clone(), api);
        let (tx, _rx) = broadcast::channel(16);

        service.sync_features(&tx).await.unwrap();

        // Verify screens stored
        let screens = db.get_feature_screens("returns").unwrap();
        assert_eq!(screens.len(), 2);

        let entry = screens.iter().find(|s| s.is_entry_point);
        assert!(entry.is_some());
        assert_eq!(entry.unwrap().screen_id, "return-entry");
    }

    #[tokio::test]
    async fn test_sync_features_sends_event() {
        let (db, api) = setup_test_deps();

        api.set_features_response(mock_features_response());

        let service = SyncService::new(db.clone(), api);
        let (tx, mut rx) = broadcast::channel(16);

        service.sync_features(&tx).await.unwrap();

        let event = rx.recv().await.unwrap();
        match event {
            SyncEvent::FeaturesUpdated { count } => {
                assert_eq!(count, 2);
            }
            _ => panic!("Expected FeaturesUpdated event"),
        }
    }

    #[tokio::test]
    async fn test_sync_features_clears_old_data() {
        let (db, api) = setup_test_deps();

        // Pre-seed old data
        db.upsert_feature(&pos_models::Feature {
            feature_id: "old-feature".to_string(),
            name: "Old".to_string(),
            is_enabled: true,
            ..Default::default()
        }).unwrap();

        api.set_features_response(mock_features_response());

        let service = SyncService::new(db.clone(), api);
        let (tx, _rx) = broadcast::channel(16);

        service.sync_features(&tx).await.unwrap();

        // Old feature should be gone
        let old = db.get_feature("old-feature").unwrap();
        assert!(old.is_none());
    }

    #[tokio::test]
    async fn test_sync_features_handles_etag() {
        let (db, api) = setup_test_deps();

        api.set_features_response(mock_features_response());
        api.set_next_etag("\"abc123\"".to_string());

        let service = SyncService::new(db.clone(), api);
        let (tx, _rx) = broadcast::channel(16);

        // First sync
        service.sync_features(&tx).await.unwrap();

        // Second sync with same ETag should return early
        api.set_return_not_modified(true);
        let result = service.sync_features(&tx).await;

        assert!(result.is_ok());
        // No duplicate data should be inserted
    }
}
```

**Run (expect fail):**

```bash
cargo test --package pos-services sync_service_features
```

---

## 2. API Client Extension

**File:** `crates/pos-api/src/features.rs`

```rust
use anyhow::Result;
use pos_models::FeaturesResponse;

impl super::ApiClient {
    /// Fetch features for this terminal
    pub async fn get_features(&self, etag: Option<&str>) -> Result<Option<FeaturesResponse>> {
        let mut request = self.client
            .get(format!("{}/api/pos/features/terminal", self.base_url))
            .header("X-Terminal-Token", &self.session_token);

        if let Some(tag) = etag {
            request = request.header("If-None-Match", tag);
        }

        let response = request.send().await?;

        // Handle 304 Not Modified
        if response.status() == reqwest::StatusCode::NOT_MODIFIED {
            return Ok(None);
        }

        if !response.status().is_success() {
            anyhow::bail!("Failed to fetch features: {}", response.status());
        }

        let features_response: FeaturesResponse = response.json().await?;
        Ok(Some(features_response))
    }
}
```

**Update lib.rs:**

```rust
// In crates/pos-api/src/lib.rs
mod features;
```

---

## 3. SyncService Extension

**File:** `crates/pos-services/src/sync_service.rs`

Add to SyncService:

```rust
use pos_models::{Feature, FeatureScreen, FeaturesResponse};

/// Sync event types
#[derive(Debug, Clone)]
pub enum SyncEvent {
    // ... existing events ...
    FeaturesUpdated { count: usize },
    FeaturesSyncSkipped { reason: String },
}

impl SyncService {
    /// Sync features from backend
    pub async fn sync_features(
        &self,
        tx: &broadcast::Sender<SyncEvent>,
    ) -> Result<()> {
        info!("Syncing features...");

        // Get current ETag from last sync
        let current_etag = self.get_features_etag()?;

        // Fetch from API
        let response = self.api
            .get_features(current_etag.as_deref())
            .await?;

        match response {
            None => {
                // 304 Not Modified
                debug!("Features unchanged (ETag match)");
                let _ = tx.send(SyncEvent::FeaturesSyncSkipped {
                    reason: "unchanged".to_string(),
                });
                return Ok(());
            }
            Some(features_response) => {
                self.store_features(&features_response)?;

                // Store new ETag
                self.set_features_etag(&features_response.version)?;

                let count = features_response.features.len();
                info!("Synced {} features", count);

                let _ = tx.send(SyncEvent::FeaturesUpdated { count });
            }
        }

        Ok(())
    }

    fn store_features(&self, response: &FeaturesResponse) -> Result<()> {
        // Clear old data (full sync strategy)
        self.db.clear_features()?;

        for feature_dto in &response.features {
            // Convert DTO to model
            let feature = Feature {
                feature_id: feature_dto.feature_id.clone(),
                name: feature_dto.name.clone(),
                name_ar: feature_dto.name_ar.clone(),
                config_key: feature_dto.config_key.clone(),
                is_core: feature_dto.is_core,
                is_enabled: feature_dto.enabled,
                icon: feature_dto.icon.clone(),
                display_order: feature_dto.display_order,
                updated_at: chrono::Utc::now().to_rfc3339(),
            };

            self.db.upsert_feature(&feature)?;

            // Store screens
            for screen_dto in &feature_dto.screens {
                let screen = FeatureScreen {
                    feature_id: feature_dto.feature_id.clone(),
                    screen_id: screen_dto.screen_id.clone(),
                    name: screen_dto.name.clone(),
                    name_ar: screen_dto.name_ar.clone(),
                    is_entry_point: screen_dto.is_entry_point,
                    next_screen: screen_dto.next_screen.clone(),
                    display_order: screen_dto.display_order,
                };

                self.db.upsert_feature_screen(&screen)?;
            }
        }

        Ok(())
    }

    fn get_features_etag(&self) -> Result<Option<String>> {
        // Store in settings table or dedicated sync_state table
        self.db.get_setting("features_etag")
    }

    fn set_features_etag(&self, etag: &str) -> Result<()> {
        self.db.set_setting("features_etag", etag)
    }
}
```

---

## 4. Add to Sync Cycle

In the main sync loop, add features sync:

```rust
impl SyncService {
    pub async fn run_sync_cycle(&self) -> Result<()> {
        let (tx, _rx) = broadcast::channel(32);

        // Existing syncs...
        self.sync_catalog(&tx).await?;
        self.sync_operators(&tx).await?;

        // NEW: Sync features
        self.sync_features(&tx).await?;

        // ...rest of sync
        Ok(())
    }
}
```

---

## 5. Verification

```bash
cd e2manage-pos-terminal

# Run tests
cargo test --package pos-services sync_service_features -- --nocapture

# Run all sync tests
cargo test --package pos-services sync

# Check compilation
cargo check

# Full build
cargo build
```

---

## Success Criteria

- [ ] Tests pass
- [ ] Features fetched from backend API
- [ ] Features and screens stored in SQLite
- [ ] ETag caching works (304 returns None)
- [ ] SyncEvent::FeaturesUpdated emitted
- [ ] Sync clears old features before storing
- [ ] `cargo build` succeeds

---

## Rollback

```bash
rm crates/pos-api/src/features.rs
# Revert sync_service.rs changes
git checkout -- crates/pos-services/src/sync_service.rs
```

---

## Next Phase

Read and follow **Phase-9-POS-Navigation-Integration.md**
