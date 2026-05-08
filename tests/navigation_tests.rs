//! Navigation Tests (TDD for Phase 9)
//!
//! Tests for the Navigator struct that integrates FeatureService
//! into navigation to block disabled screens.

use e2manage_pos_terminal::db::Database;
use e2manage_pos_terminal::services::FeatureService;
use pos_models::{Feature, FeatureScreen};
use std::sync::Arc;

mod common;
use common::*;

fn setup_test_navigation() -> (Arc<Database>, FeatureService) {
    let db = setup_test_db_arc();
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
    })
    .unwrap();

    db.upsert_feature_screen(&FeatureScreen {
        feature_id: "checkout".to_string(),
        screen_id: "checkout".to_string(),
        name: "Checkout".to_string(),
        is_entry_point: true,
        ..Default::default()
    })
    .unwrap();

    // Optional disabled feature
    db.upsert_feature(&Feature {
        feature_id: "drafts".to_string(),
        name: "Drafts".to_string(),
        is_core: false,
        is_enabled: false,
        ..Default::default()
    })
    .unwrap();

    db.upsert_feature_screen(&FeatureScreen {
        feature_id: "drafts".to_string(),
        screen_id: "save-draft".to_string(),
        name: "Save Draft".to_string(),
        ..Default::default()
    })
    .unwrap();

    // Optional enabled feature with flow
    db.upsert_feature(&Feature {
        feature_id: "returns".to_string(),
        name: "Returns".to_string(),
        is_core: false,
        is_enabled: true,
        ..Default::default()
    })
    .unwrap();

    db.upsert_feature_screen(&FeatureScreen {
        feature_id: "returns".to_string(),
        screen_id: "return-entry".to_string(),
        name: "Return Entry".to_string(),
        is_entry_point: true,
        next_screen: Some("return-items".to_string()),
        display_order: 1,
        ..Default::default()
    })
    .unwrap();

    db.upsert_feature_screen(&FeatureScreen {
        feature_id: "returns".to_string(),
        screen_id: "return-items".to_string(),
        name: "Return Items".to_string(),
        is_entry_point: false,
        next_screen: Some("refund".to_string()),
        display_order: 2,
        ..Default::default()
    })
    .unwrap();

    db.upsert_feature_screen(&FeatureScreen {
        feature_id: "returns".to_string(),
        screen_id: "refund".to_string(),
        name: "Refund".to_string(),
        is_entry_point: false,
        display_order: 3,
        ..Default::default()
    })
    .unwrap();
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

// Navigator-specific tests
use e2manage_pos_terminal::ui::navigation::{Navigator, NavigationResult};

#[test]
fn test_navigator_navigate_to_success() {
    let (db, service) = setup_test_navigation();
    seed_navigation_features(&db);

    let mut navigator = Navigator::new(service);

    match navigator.navigate_to("checkout") {
        NavigationResult::Success { screen_id } => {
            assert_eq!(screen_id, "checkout");
            assert_eq!(navigator.current_screen(), "checkout");
        }
        _ => panic!("Expected Success"),
    }
}

#[test]
fn test_navigator_navigate_to_blocked() {
    let (db, service) = setup_test_navigation();
    seed_navigation_features(&db);

    let mut navigator = Navigator::new(service);

    match navigator.navigate_to("save-draft") {
        NavigationResult::Blocked { reason } => {
            assert!(reason.contains("drafts") || reason.contains("disabled"));
        }
        _ => panic!("Expected Blocked"),
    }
}

#[test]
fn test_navigator_navigate_to_not_found() {
    let (db, service) = setup_test_navigation();
    seed_navigation_features(&db);

    let mut navigator = Navigator::new(service);

    match navigator.navigate_to("nonexistent-screen") {
        NavigationResult::NotFound { screen_id } => {
            assert_eq!(screen_id, "nonexistent-screen");
        }
        _ => panic!("Expected NotFound"),
    }
}

#[test]
fn test_navigator_navigate_next() {
    let (db, service) = setup_test_navigation();
    seed_navigation_features(&db);

    let mut navigator = Navigator::new(service);

    // Go to return-entry first
    navigator.navigate_to("return-entry");

    // Navigate next should go to return-items
    match navigator.navigate_next() {
        NavigationResult::Success { screen_id } => {
            assert_eq!(screen_id, "return-items");
        }
        _ => panic!("Expected Success for next screen"),
    }

    // Navigate next again should go to refund
    match navigator.navigate_next() {
        NavigationResult::Success { screen_id } => {
            assert_eq!(screen_id, "refund");
        }
        _ => panic!("Expected Success for refund screen"),
    }

    // Navigate next from refund should return NotFound (no more screens)
    match navigator.navigate_next() {
        NavigationResult::NotFound { .. } => {}
        _ => panic!("Expected NotFound at end of flow"),
    }
}

#[test]
fn test_navigator_navigate_to_feature() {
    let (db, service) = setup_test_navigation();
    seed_navigation_features(&db);

    let mut navigator = Navigator::new(service);

    match navigator.navigate_to_feature("returns") {
        NavigationResult::Success { screen_id } => {
            assert_eq!(screen_id, "return-entry");
        }
        _ => panic!("Expected Success"),
    }
}

#[test]
fn test_navigator_get_available_screens() {
    let (db, service) = setup_test_navigation();
    seed_navigation_features(&db);

    let navigator = Navigator::new(service);
    let available = navigator.get_available_screens();

    assert!(available.contains(&"checkout".to_string()));
    assert!(available.contains(&"return-entry".to_string()));
    assert!(!available.contains(&"save-draft".to_string()));
}

#[test]
fn test_navigator_is_screen_available() {
    let (db, service) = setup_test_navigation();
    seed_navigation_features(&db);

    let navigator = Navigator::new(service);

    assert!(navigator.is_screen_available("checkout"));
    assert!(navigator.is_screen_available("return-entry"));
    assert!(!navigator.is_screen_available("save-draft"));
    assert!(!navigator.is_screen_available("nonexistent"));
}
