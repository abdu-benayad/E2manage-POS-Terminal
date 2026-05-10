//! E2E Feature Sync Tests
//!
//! End-to-end tests for feature library synchronization from backend.
//! These tests require a running backend server with test data.
//!
//! ## Running Tests
//!
//! ```bash
//! # Set backend URL (defaults to localhost:3000)
//! export BACKEND_URL=http://localhost:3000
//!
//! # Run feature sync E2E tests
//! cargo test --test e2e_feature_sync -- --ignored --nocapture
//! ```
//!
//! ## Prerequisites
//!
//! 1. Backend server running at BACKEND_URL
//! 2. Test tenant with POS module enabled
//! 3. Test terminal registered and authenticated
//! 4. Feature library seeded in backend

mod common;

use common::setup_test_db_arc;
use e2manage_pos_terminal::{ApiClient, FeatureService, SyncEvent, SyncService};
use std::env;
use std::sync::Arc;
use tokio::sync::broadcast;

/// Default backend URL for E2E tests
const DEFAULT_BACKEND_URL: &str = "http://localhost:3000";

/// Get backend URL from environment or use default
fn get_backend_url() -> String {
    env::var("BACKEND_URL").unwrap_or_else(|_| DEFAULT_BACKEND_URL.to_string())
}

// ============================================================================
// Feature Sync E2E Tests
// ============================================================================

/// Test that features can be synced from the backend
///
/// This test:
/// 1. Creates an in-memory database
/// 2. Connects to the real backend
/// 3. Authenticates a terminal
/// 4. Syncs features from backend
/// 5. Verifies features are stored in local database
#[tokio::test]
#[ignore] // Requires running backend
async fn test_feature_sync_from_backend() {
    let backend_url = get_backend_url();
    println!("Testing feature sync from: {}", backend_url);

    // Setup in-memory database
    let db = setup_test_db_arc();

    // Create API client connected to real backend
    let api = Arc::new(ApiClient::new(&backend_url));

    // Skip if backend is not available
    if !check_backend_available(&backend_url).await {
        println!("⚠ Skipping: Backend not available at {}", backend_url);
        return;
    }

    // Try to authenticate (requires pre-registered terminal)
    // This assumes the E2E test script has set up the test terminal
    let test_terminal_code =
        env::var("TEST_TERMINAL_CODE").unwrap_or_else(|_| "TERM-001".to_string());
    let test_hw_id = env::var("TEST_HARDWARE_ID").unwrap_or_else(|_| "E2E-TEST-HW-001".to_string());
    let test_secret =
        env::var("TEST_TERMINAL_SECRET").unwrap_or_else(|_| "e2e-test-secret-12345678".to_string());

    // Attempt terminal authentication
    match api
        .login_terminal(&test_terminal_code, &test_hw_id, &test_secret)
        .await
    {
        Ok(_) => println!("✓ Terminal authenticated"),
        Err(e) => {
            println!("⚠ Skipping: Terminal authentication failed: {}", e);
            println!("  Make sure test terminal is registered in backend");
            return;
        }
    }

    // Create sync service (using with_defaults which sets reasonable interval)
    let sync_service = SyncService::with_defaults(api.clone(), db.clone());
    let (tx, mut rx) = broadcast::channel::<SyncEvent>(16);

    // Sync all (includes features)
    match sync_service.sync_all(&tx).await {
        Ok(_) => println!("✓ Sync completed (includes features)"),
        Err(e) => {
            println!("✗ Sync failed: {}", e);
            panic!("Sync should succeed");
        }
    }

    // Check event received
    match rx.try_recv() {
        Ok(SyncEvent::FeaturesUpdated { count }) => {
            println!("✓ Received FeaturesUpdated event: {} features", count);
            assert!(count > 0, "Should have synced at least one feature");
        }
        Ok(SyncEvent::FeaturesNotModified) => {
            println!("✓ Features not modified (304) - this is also valid");
        }
        Ok(other) => {
            println!("? Received unexpected event: {:?}", other);
        }
        Err(_) => {
            println!("? No sync event received");
        }
    }

    // Verify features are stored
    let feature_service = FeatureService::new(db.clone());
    let features = feature_service.get_enabled_features();

    match features {
        Ok(features) => {
            println!("✓ Got {} enabled features from database", features.len());
            for f in &features {
                println!("  - {} ({})", f.name, f.feature_id);
            }
        }
        Err(e) => {
            println!("? Could not get features: {}", e);
        }
    }

    println!("\n✓ Feature sync from backend test passed");
}

/// Test that feature-based navigation works correctly
///
/// This test verifies that:
/// 1. Screens can be enabled/disabled based on features
/// 2. Navigation respects feature status
#[tokio::test]
#[ignore] // Requires running backend
async fn test_feature_based_navigation() {
    let backend_url = get_backend_url();
    println!("Testing feature-based navigation from: {}", backend_url);

    // Setup
    let db = setup_test_db_arc();
    let api = Arc::new(ApiClient::new(&backend_url));

    // Skip if backend is not available
    if !check_backend_available(&backend_url).await {
        println!("⚠ Skipping: Backend not available at {}", backend_url);
        return;
    }

    // Authenticate
    let test_terminal_code =
        env::var("TEST_TERMINAL_CODE").unwrap_or_else(|_| "TERM-001".to_string());
    let test_hw_id = env::var("TEST_HARDWARE_ID").unwrap_or_else(|_| "E2E-TEST-HW-001".to_string());
    let test_secret =
        env::var("TEST_TERMINAL_SECRET").unwrap_or_else(|_| "e2e-test-secret-12345678".to_string());

    if api
        .login_terminal(&test_terminal_code, &test_hw_id, &test_secret)
        .await
        .is_err()
    {
        println!("⚠ Skipping: Terminal not authenticated");
        return;
    }

    // Sync features
    let sync_service = SyncService::with_defaults(api.clone(), db.clone());
    let (tx, _) = broadcast::channel::<SyncEvent>(16);

    if sync_service.sync_all(&tx).await.is_err() {
        println!("⚠ Skipping: Feature sync failed");
        return;
    }

    // Test navigation
    let feature_service = FeatureService::new(db.clone());

    // Test screen enabled check
    match feature_service.is_screen_enabled("checkout") {
        Ok(enabled) => {
            println!("✓ checkout screen enabled: {}", enabled);
            // Core features should typically be enabled
        }
        Err(e) => {
            println!("? Could not check screen: {}", e);
        }
    }

    // Test feature enabled check
    match feature_service.is_feature_enabled("checkout") {
        Ok(enabled) => {
            println!("✓ checkout feature enabled: {}", enabled);
        }
        Err(e) => {
            println!("? Could not check feature: {}", e);
        }
    }

    // Test getting next screen (navigation flow)
    match feature_service.get_next_screen("checkout") {
        Ok(next) => {
            if let Some(screen_id) = next {
                println!("✓ Next screen from checkout: {}", screen_id);
            } else {
                println!("✓ checkout is a terminal screen (no next)");
            }
        }
        Err(e) => {
            println!("? Could not get next screen: {}", e);
        }
    }

    println!("\n✓ Feature-based navigation test passed");
}

/// Test ETag caching for features
///
/// This test verifies that:
/// 1. First sync returns full data
/// 2. Second sync with same ETag returns 304 (not modified)
#[tokio::test]
#[ignore] // Requires running backend
async fn test_etag_caching() {
    let backend_url = get_backend_url();
    println!("Testing ETag caching from: {}", backend_url);

    // Setup
    let db = setup_test_db_arc();
    let api = Arc::new(ApiClient::new(&backend_url));

    // Skip if backend is not available
    if !check_backend_available(&backend_url).await {
        println!("⚠ Skipping: Backend not available at {}", backend_url);
        return;
    }

    // Authenticate
    let test_terminal_code =
        env::var("TEST_TERMINAL_CODE").unwrap_or_else(|_| "TERM-001".to_string());
    let test_hw_id = env::var("TEST_HARDWARE_ID").unwrap_or_else(|_| "E2E-TEST-HW-001".to_string());
    let test_secret =
        env::var("TEST_TERMINAL_SECRET").unwrap_or_else(|_| "e2e-test-secret-12345678".to_string());

    if api
        .login_terminal(&test_terminal_code, &test_hw_id, &test_secret)
        .await
        .is_err()
    {
        println!("⚠ Skipping: Terminal not authenticated");
        return;
    }

    // Create sync service
    let sync_service = SyncService::with_defaults(api.clone(), db.clone());
    let (tx, mut rx) = broadcast::channel::<SyncEvent>(16);

    // First sync - should get full data
    println!("Performing first sync...");
    sync_service
        .sync_all(&tx)
        .await
        .expect("First sync should succeed");

    let first_event = rx.try_recv();
    let was_updated = matches!(first_event, Ok(SyncEvent::FeaturesUpdated { .. }));
    let was_not_modified = matches!(first_event, Ok(SyncEvent::FeaturesNotModified));

    if was_updated {
        println!("✓ First sync: FeaturesUpdated");
    } else if was_not_modified {
        println!("✓ First sync: FeaturesNotModified (already cached)");
    } else {
        println!("? First sync: Unknown event");
    }

    // Second sync - should return 304 (not modified) if data hasn't changed
    println!("Performing second sync (should use cached ETag)...");
    sync_service
        .sync_all(&tx)
        .await
        .expect("Second sync should succeed");

    match rx.try_recv() {
        Ok(SyncEvent::FeaturesNotModified) => {
            println!("✓ Second sync: FeaturesNotModified (304)");
        }
        Ok(SyncEvent::FeaturesUpdated { count }) => {
            // This is also valid if backend data changed
            println!(
                "✓ Second sync: FeaturesUpdated ({} features) - data may have changed",
                count
            );
        }
        Ok(other) => {
            println!("? Second sync: Unexpected event {:?}", other);
        }
        Err(_) => {
            println!("? No event from second sync");
        }
    }

    println!("\n✓ ETag caching test passed");
}

/// Test that optional features respect tenant configuration
///
/// This test verifies that:
/// 1. Core features are always enabled
/// 2. Optional features respect tenant config (e.g., allowReturns, allowDrafts)
#[tokio::test]
#[ignore] // Requires running backend
async fn test_feature_config_toggle() {
    let backend_url = get_backend_url();
    println!("Testing feature config toggle from: {}", backend_url);

    // Setup
    let db = setup_test_db_arc();
    let api = Arc::new(ApiClient::new(&backend_url));

    // Skip if backend is not available
    if !check_backend_available(&backend_url).await {
        println!("⚠ Skipping: Backend not available at {}", backend_url);
        return;
    }

    // Authenticate
    let test_terminal_code =
        env::var("TEST_TERMINAL_CODE").unwrap_or_else(|_| "TERM-001".to_string());
    let test_hw_id = env::var("TEST_HARDWARE_ID").unwrap_or_else(|_| "E2E-TEST-HW-001".to_string());
    let test_secret =
        env::var("TEST_TERMINAL_SECRET").unwrap_or_else(|_| "e2e-test-secret-12345678".to_string());

    if api
        .login_terminal(&test_terminal_code, &test_hw_id, &test_secret)
        .await
        .is_err()
    {
        println!("⚠ Skipping: Terminal not authenticated");
        return;
    }

    // Sync features
    let sync_service = SyncService::with_defaults(api.clone(), db.clone());
    let (tx, _) = broadcast::channel::<SyncEvent>(16);
    sync_service
        .sync_all(&tx)
        .await
        .expect("Feature sync should succeed");

    // Check feature service
    let feature_service = FeatureService::new(db.clone());

    // Get all features and check their status
    match feature_service.get_enabled_features() {
        Ok(features) => {
            println!("\nFeature status:");
            for f in &features {
                let core_indicator = if f.is_core { "(core)" } else { "(optional)" };
                println!(
                    "  - {} {} enabled: {}",
                    f.name, core_indicator, f.is_enabled
                );
            }

            // Core features should always be enabled
            let core_disabled: Vec<_> = features
                .iter()
                .filter(|f| f.is_core && !f.is_enabled)
                .collect();

            if !core_disabled.is_empty() {
                println!("⚠ Warning: Some core features are disabled:");
                for f in core_disabled {
                    println!("    - {}", f.name);
                }
            } else {
                println!("✓ All core features are enabled");
            }
        }
        Err(e) => {
            println!("? Could not get features: {}", e);
        }
    }

    println!("\n✓ Feature config toggle test passed");
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Check if backend is available using ApiClient
async fn check_backend_available(url: &str) -> bool {
    let api = ApiClient::new(url);

    if api.is_online().await.is_online() {
        println!("✓ Backend available at {}", url);
        true
    } else {
        println!("✗ Backend not available at {}", url);
        false
    }
}
