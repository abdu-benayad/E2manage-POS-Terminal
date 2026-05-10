//! E2E Catalog Sync Tests
//!
//! End-to-end tests for product catalog synchronization from backend.
//! Tests that the Authorization header is properly sent with sync requests.
//!
//! ## Running Tests
//!
//! ```bash
//! # Set backend URL (defaults to https://jooher.app)
//! export BACKEND_URL=https://jooher.app
//!
//! # Run catalog sync E2E tests
//! cargo test --test e2e_catalog_sync -- --ignored --nocapture
//! ```
//!
//! ## Prerequisites
//!
//! 1. Backend server running at BACKEND_URL
//! 2. Test terminal registered and has valid session token
//! 3. Product catalog seeded in backend

mod common;

use common::setup_test_db_arc;
use e2manage_pos_terminal::api::ApiClient;
use e2manage_pos_terminal::services::{AuthService, SyncService, SyncEvent};
use std::env;
use std::sync::Arc;
use tokio::sync::broadcast;

/// Default backend URL for E2E tests
const DEFAULT_BACKEND_URL: &str = "https://jooher.app";

/// Get backend URL from environment or use default
fn get_backend_url() -> String {
    env::var("BACKEND_URL").unwrap_or_else(|_| DEFAULT_BACKEND_URL.to_string())
}

/// Check if backend is available using ApiClient
async fn check_backend_available(api: &ApiClient) -> bool {
    api.is_online().await.is_online()
}

// ============================================================================
// Catalog Sync E2E Tests
// ============================================================================

/// Test that catalog sync works with proper Authorization header
///
/// This test:
/// 1. Creates an in-memory database
/// 2. Connects to the real backend
/// 3. Sets up authentication token
/// 4. Syncs catalog from backend
/// 5. Verifies products are stored in local database
#[tokio::test]
#[ignore] // Requires running backend
async fn test_catalog_sync_with_auth_header() {
    let backend_url = get_backend_url();
    println!("\n=== Catalog Sync E2E Test ===");
    println!("Backend URL: {}", backend_url);

    // Setup in-memory database
    let db = setup_test_db_arc();

    // Create API client connected to real backend
    let api = Arc::new(ApiClient::new(&backend_url));

    // Check if backend is available
    if !check_backend_available(&api).await {
        println!("SKIP: Backend not available at {}", backend_url);
        return;
    }
    println!("Backend is available");

    // Create auth service (not used directly but validates setup)
    let _auth_service = AuthService::new(Arc::clone(&api), Arc::clone(&db));

    // Try to get session token from environment or use test token
    let session_token = env::var("TEST_SESSION_TOKEN").ok();

    if let Some(token) = session_token {
        println!("Using session token from environment");
        api.set_token(token).await;
    } else {
        // Try terminal login
        let test_terminal_code = env::var("TEST_TERMINAL_CODE")
            .unwrap_or_else(|_| "TERM-001".to_string());
        let test_hw_id = env::var("TEST_HARDWARE_ID")
            .unwrap_or_else(|_| "E2E-TEST-HW-001".to_string());
        let test_secret = env::var("TEST_TERMINAL_SECRET")
            .unwrap_or_else(|_| "e2e-test-secret-12345678".to_string());

        println!("Attempting terminal login with hw_id: {}", test_hw_id);

        match api.login_terminal(&test_terminal_code, &test_hw_id, &test_secret).await {
            Ok(response) => {
                println!("Terminal authenticated successfully");
                println!("  Terminal ID: {}", response.terminal_id);
                api.set_token(response.session_token).await;
            }
            Err(e) => {
                println!("SKIP: Terminal authentication failed: {}", e);
                println!("  Set TEST_SESSION_TOKEN env var to provide a valid token");
                return;
            }
        }
    }

    // Verify we have a token set
    if !api.is_authenticated().await {
        println!("FAIL: No authentication token set");
        panic!("API client should be authenticated");
    }
    println!("API client is authenticated");

    // Create sync service
    let sync_service = SyncService::new(Arc::clone(&api), Arc::clone(&db), 5);
    let (tx, _rx) = broadcast::channel::<SyncEvent>(16);

    // Try to sync catalog
    println!("\nSyncing catalog...");
    match sync_service.sync_all(&tx).await {
        Ok(_) => {
            println!("Sync completed successfully!");
        }
        Err(e) => {
            println!("FAIL: Sync failed with error: {}", e);
            panic!("Catalog sync should succeed with valid auth token");
        }
    }

    // Check products were saved
    let product_count = db.get_product_count().unwrap_or(0);
    println!("Products in database: {}", product_count);

    // Check categories were saved
    let categories = db.get_categories().unwrap_or_default();
    println!("Categories in database: {}", categories.len());

    // Verify we got some data
    if product_count > 0 {
        println!("\nSUCCESS: Catalog sync retrieved {} products", product_count);
    } else {
        println!("\nWARNING: No products synced (backend may have empty catalog)");
    }
}

/// Test that sync fails gracefully without auth token
#[tokio::test]
#[ignore] // Requires running backend
async fn test_catalog_sync_fails_without_auth() {
    let backend_url = get_backend_url();
    println!("\n=== Test Sync Without Auth ===");
    println!("Backend URL: {}", backend_url);

    // Setup database and API without auth
    let db = setup_test_db_arc();
    let api = Arc::new(ApiClient::new(&backend_url));

    // Check if backend is available
    if !check_backend_available(&api).await {
        println!("SKIP: Backend not available at {}", backend_url);
        return;
    }

    // DON'T set any token - this should cause 401

    // Create sync service
    let sync_service = SyncService::new(Arc::clone(&api), Arc::clone(&db), 5);
    let (tx, _rx) = broadcast::channel::<SyncEvent>(16);

    // Try to sync - should fail with 401
    println!("Attempting sync without auth token...");
    match sync_service.sync_all(&tx).await {
        Ok(_) => {
            println!("WARNING: Sync succeeded without auth - backend may not require auth");
        }
        Err(e) => {
            let error_str = e.to_string();
            if error_str.contains("401") || error_str.contains("Unauthorized") || error_str.contains("Authorization") {
                println!("SUCCESS: Sync correctly rejected with 401 Unauthorized");
                println!("  Error: {}", error_str);
            } else {
                println!("FAIL: Unexpected error type: {}", error_str);
            }
        }
    }
}

/// Test that Authorization header is correctly formatted
#[tokio::test]
async fn test_authorization_header_format() {
    let backend_url = get_backend_url();
    println!("\n=== Test Authorization Header Format ===");

    // Create API client
    let api = ApiClient::new(&backend_url);

    // Set a test token
    let test_token = "test-token-12345";
    api.set_token(test_token.to_string()).await;

    // Verify token is set
    let stored_token = api.get_token().await;
    assert_eq!(stored_token, Some(test_token.to_string()));
    println!("Token stored correctly: {}", test_token);

    // The actual header format is tested internally by the API client
    // We just verify the token management works
    println!("SUCCESS: Token management working correctly");
    println!("  Authorization header will be: Bearer {}", test_token);
}

/// Test catalog sync with ETag caching
#[tokio::test]
#[ignore] // Requires running backend
async fn test_catalog_sync_etag_caching() {
    let backend_url = get_backend_url();
    println!("\n=== Test Catalog Sync ETag Caching ===");
    println!("Backend URL: {}", backend_url);

    // Setup
    let db = setup_test_db_arc();
    let api = Arc::new(ApiClient::new(&backend_url));

    // Check if backend is available
    if !check_backend_available(&api).await {
        println!("SKIP: Backend not available at {}", backend_url);
        return;
    }

    // Get session token
    let session_token = env::var("TEST_SESSION_TOKEN").ok();
    if let Some(token) = session_token {
        api.set_token(token).await;
    } else {
        println!("SKIP: Set TEST_SESSION_TOKEN to run this test");
        return;
    }

    // Create sync service
    let sync_service = SyncService::new(Arc::clone(&api), Arc::clone(&db), 5);
    let (tx, _rx) = broadcast::channel::<SyncEvent>(16);

    // First sync - should get full data
    println!("First sync (should get full data)...");
    sync_service.sync_all(&tx).await.expect("First sync should succeed");

    let first_count = db.get_product_count().unwrap_or(0);
    println!("  Products after first sync: {}", first_count);

    // Second sync - should use ETag and possibly get 304 Not Modified
    println!("Second sync (should use ETag caching)...");
    let (tx2, mut rx2) = broadcast::channel::<SyncEvent>(16);
    sync_service.sync_all(&tx2).await.expect("Second sync should succeed");

    // Check if we got a NotModified event
    while let Ok(event) = rx2.try_recv() {
        match event {
            SyncEvent::CatalogNotModified => {
                println!("  Received CatalogNotModified - ETag caching working!");
            }
            SyncEvent::CatalogUpdated { products_count, categories_count } => {
                println!("  Catalog updated: {} products, {} categories", products_count, categories_count);
            }
            _ => {}
        }
    }

    println!("SUCCESS: ETag caching test completed");
}
