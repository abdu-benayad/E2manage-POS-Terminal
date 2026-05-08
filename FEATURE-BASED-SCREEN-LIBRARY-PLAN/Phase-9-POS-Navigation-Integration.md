# Phase 9: POS - Navigation Integration

**Time:** 1.5 hours
**Type:** POS Terminal (Rust)

Integrate FeatureService into navigation to block disabled screens.

---

## Pre-Flight Checklist

- [ ] Phase 8 completed (sync works)
- [ ] FeatureService available
- [ ] Main.rs navigation logic exists

---

## 1. Tests First (TDD)

**File:** `tests/navigation_tests.rs`

```rust
use e2manage_pos_terminal::*;
use pos_db::Database;
use pos_models::{Feature, FeatureScreen};
use pos_services::FeatureService;
use std::sync::Arc;
use tempfile::tempdir;

fn setup_test_navigation() -> (Arc<Database>, FeatureService) {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let db = Arc::new(Database::new(db_path.to_str().unwrap()).unwrap());
    let service = FeatureService::new(db.clone());
    (db, service)
}

fn seed_navigation_features(db: &Database) {
    // Core enabled feature
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

    // Optional disabled feature
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

    // Optional enabled feature with flow
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
        display_order: 1,
        ..Default::default()
    }).unwrap();

    db.upsert_feature_screen(&FeatureScreen {
        feature_id: "returns".to_string(),
        screen_id: "return-items".to_string(),
        name: "Return Items".to_string(),
        is_entry_point: false,
        next_screen: Some("refund".to_string()),
        display_order: 2,
        ..Default::default()
    }).unwrap();

    db.upsert_feature_screen(&FeatureScreen {
        feature_id: "returns".to_string(),
        screen_id: "refund".to_string(),
        name: "Refund".to_string(),
        is_entry_point: false,
        display_order: 3,
        ..Default::default()
    }).unwrap();
}

#[test]
fn test_navigate_to_enabled_screen_allowed() {
    let (db, service) = setup_test_navigation();
    seed_navigation_features(&db);

    assert!(service.can_navigate_to("checkout").unwrap());
    assert!(service.can_navigate_to("return-entry").unwrap());
}

#[test]
fn test_navigate_to_disabled_screen_blocked() {
    let (db, service) = setup_test_navigation();
    seed_navigation_features(&db);

    assert!(!service.can_navigate_to("save-draft").unwrap());
}

#[test]
fn test_navigate_to_unknown_screen_blocked() {
    let (db, service) = setup_test_navigation();
    seed_navigation_features(&db);

    assert!(!service.can_navigate_to("nonexistent-screen").unwrap());
}

#[test]
fn test_get_next_screen_in_flow() {
    let (db, service) = setup_test_navigation();
    seed_navigation_features(&db);

    let next = service.get_next_screen("return-entry").unwrap();
    assert_eq!(next, Some("return-items".to_string()));

    let next = service.get_next_screen("return-items").unwrap();
    assert_eq!(next, Some("refund".to_string()));

    let next = service.get_next_screen("refund").unwrap();
    assert_eq!(next, None); // End of flow
}

#[test]
fn test_get_enabled_screen_ids_excludes_disabled() {
    let (db, service) = setup_test_navigation();
    seed_navigation_features(&db);

    let screens = service.get_enabled_screen_ids().unwrap();

    assert!(screens.contains(&"checkout".to_string()));
    assert!(screens.contains(&"return-entry".to_string()));
    assert!(!screens.contains(&"save-draft".to_string()));
}
```

**Run (expect fail/pass depending on previous phases):**

```bash
cargo test --test navigation_tests
```

---

## 2. Navigation Module

**File:** `src/ui/navigation.rs`

```rust
use anyhow::{Result, Context};
use pos_services::FeatureService;
use tracing::{info, warn, debug};

/// Navigation result
#[derive(Debug, Clone)]
pub enum NavigationResult {
    Success { screen_id: String },
    Blocked { reason: String },
    NotFound { screen_id: String },
}

/// Handle screen navigation with feature checks
pub struct Navigator {
    feature_service: FeatureService,
    current_screen: String,
}

impl Navigator {
    pub fn new(feature_service: FeatureService) -> Self {
        Self {
            feature_service,
            current_screen: "splash".to_string(),
        }
    }

    /// Navigate to a screen with feature validation
    pub fn navigate_to(&mut self, screen_id: &str) -> NavigationResult {
        debug!("Navigation requested: {} -> {}", self.current_screen, screen_id);

        // Check if screen exists and is enabled
        match self.feature_service.is_screen_enabled(screen_id) {
            Ok(true) => {
                info!("Navigating to: {}", screen_id);
                self.current_screen = screen_id.to_string();
                NavigationResult::Success {
                    screen_id: screen_id.to_string(),
                }
            }
            Ok(false) => {
                // Check if screen exists but feature is disabled
                match self.feature_service.get_screen_feature(screen_id) {
                    Ok(Some(feature_id)) => {
                        warn!(
                            "Navigation blocked: screen {} disabled (feature {} is off)",
                            screen_id, feature_id
                        );
                        NavigationResult::Blocked {
                            reason: format!("Feature '{}' is disabled", feature_id),
                        }
                    }
                    Ok(None) => {
                        warn!("Navigation blocked: screen {} not found", screen_id);
                        NavigationResult::NotFound {
                            screen_id: screen_id.to_string(),
                        }
                    }
                    Err(e) => {
                        warn!("Navigation error: {}", e);
                        NavigationResult::Blocked {
                            reason: format!("Error: {}", e),
                        }
                    }
                }
            }
            Err(e) => {
                warn!("Failed to check screen status: {}", e);
                NavigationResult::Blocked {
                    reason: format!("Error checking screen: {}", e),
                }
            }
        }
    }

    /// Navigate to next screen in flow
    pub fn navigate_next(&mut self) -> NavigationResult {
        match self.feature_service.get_next_screen(&self.current_screen) {
            Ok(Some(next)) => self.navigate_to(&next),
            Ok(None) => {
                debug!("No next screen defined for: {}", self.current_screen);
                NavigationResult::NotFound {
                    screen_id: "".to_string(),
                }
            }
            Err(e) => NavigationResult::Blocked {
                reason: format!("Error: {}", e),
            },
        }
    }

    /// Navigate to feature entry point
    pub fn navigate_to_feature(&mut self, feature_id: &str) -> NavigationResult {
        match self.feature_service.get_feature_entry_screen(feature_id) {
            Ok(Some(entry)) => self.navigate_to(&entry),
            Ok(None) => {
                warn!("Feature {} has no entry point", feature_id);
                NavigationResult::NotFound {
                    screen_id: feature_id.to_string(),
                }
            }
            Err(e) => NavigationResult::Blocked {
                reason: format!("Error: {}", e),
            },
        }
    }

    /// Get current screen
    pub fn current_screen(&self) -> &str {
        &self.current_screen
    }

    /// Get all navigable screens (for menu building)
    pub fn get_available_screens(&self) -> Vec<String> {
        self.feature_service
            .get_enabled_screen_ids()
            .unwrap_or_default()
    }

    /// Check if a screen button should be visible
    pub fn is_screen_available(&self, screen_id: &str) -> bool {
        self.feature_service
            .is_screen_enabled(screen_id)
            .unwrap_or(false)
    }
}
```

---

## 3. Integration with Main

**File:** `src/main.rs`

Add to initialization:

```rust
use crate::ui::navigation::{Navigator, NavigationResult};

// In main() or App initialization:
let feature_service = FeatureService::new(db.clone());
let navigator = Navigator::new(feature_service);

// Store in app state (Arc<Mutex<Navigator>> or similar)
```

Replace direct screen setting with navigation check:

```rust
// BEFORE (direct):
self.window.set_current_screen(screen_id.into());

// AFTER (with feature check):
match self.navigator.lock().unwrap().navigate_to(screen_id) {
    NavigationResult::Success { screen_id } => {
        self.window.set_current_screen(screen_id.into());
    }
    NavigationResult::Blocked { reason } => {
        warn!("Navigation blocked: {}", reason);
        // Optionally show error to user
        self.show_toast(&format!("Feature disabled: {}", reason));
    }
    NavigationResult::NotFound { screen_id } => {
        error!("Screen not found: {}", screen_id);
    }
}
```

---

## 4. UI Visibility Callbacks

For Slint UI, expose navigation checks:

```rust
// In UI bridge setup:
let nav_clone = navigator.clone();
window.on_is_feature_enabled(move |feature_id: SharedString| {
    nav_clone.lock().unwrap()
        .feature_service
        .is_feature_enabled(&feature_id)
        .unwrap_or(false)
});

let nav_clone = navigator.clone();
window.on_can_navigate_to(move |screen_id: SharedString| {
    nav_clone.lock().unwrap()
        .is_screen_available(&screen_id)
});
```

---

## 5. Slint Bindings (Example)

**File:** `ui/main.slint`

```slint
// Add callback declarations
export component MainWindow {
    // Callbacks for feature checks
    callback is_feature_enabled(string) -> bool;
    callback can_navigate_to(string) -> bool;

    // Conditional button visibility
    if root.can_navigate_to("return-entry"): Button {
        text: "Returns";
        clicked => { root.navigate("return-entry"); }
    }

    if root.is_feature_enabled("drafts"): Button {
        text: "Drafts";
        clicked => { root.navigate("save-draft"); }
    }
}
```

---

## 6. Verification

```bash
cd e2manage-pos-terminal

# Run navigation tests
cargo test --test navigation_tests -- --nocapture

# Run all tests
cargo test

# Check compilation
cargo check

# Full build
cargo build

# Manual testing
cargo run
# Try navigating to disabled features
```

---

## Success Criteria

- [ ] Tests pass
- [ ] Navigation checks feature status before allowing
- [ ] Disabled screens return Blocked result
- [ ] Unknown screens return NotFound result
- [ ] Flow navigation (next_screen) works
- [ ] UI buttons hidden for disabled features
- [ ] `cargo build` succeeds

---

## Rollback

```bash
rm src/ui/navigation.rs
rm tests/navigation_tests.rs
# Revert main.rs changes
git checkout -- src/main.rs
```

---

## Next Phase

Read and follow **Phase-10-Frontend-Feature-Config-UI.md**
