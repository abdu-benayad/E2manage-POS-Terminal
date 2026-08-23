//! API Integration Tests
//!
//! Tests the POS terminal's communication with the E2Manage backend API.
//!
//! These tests require a running backend server and are marked with #[ignore].
//! Run them explicitly with: `cargo test --test api_integration -- --ignored`
//!
//! ## Configuration
//!
//! Set the backend URL via environment variable:
//! ```bash
//! BACKEND_URL=http://localhost:3000 cargo test --test api_integration -- --ignored
//! ```
//!
//! ## Prerequisites
//!
//! 1. Backend server running at BACKEND_URL
//! 2. Test tenant with POS module enabled
//! 3. Test user with POS permissions
//! 4. Test terminal registered

use reqwest::{Client, StatusCode};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::env;
use std::time::Duration;

/// Default backend URL for integration tests
const DEFAULT_BACKEND_URL: &str = "http://localhost:3000";

/// Get backend URL from environment or use default
fn get_backend_url() -> String {
    env::var("BACKEND_URL").unwrap_or_else(|_| DEFAULT_BACKEND_URL.to_string())
}

// ============================================================================
// API Response Envelope
// ============================================================================

/// Standard API response wrapper from the backend
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiResponse<T> {
    pub success: bool,
    #[serde(default)]
    pub message: Option<String>,
    pub data: Option<T>,
}

/// Error response from the backend
///
/// The machine code is nested under `error.code`; a top-level `errorCode` is a field the platform
/// has never sent.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiError {
    pub message: String,
    #[serde(default)]
    pub error: Option<ApiErrorDetail>,
}

#[derive(Debug, Deserialize)]
pub struct ApiErrorDetail {
    pub code: String,
    pub message: String,
}

// ============================================================================
// Test Infrastructure
// ============================================================================

/// Integration test client for backend API
pub struct TestClient {
    client: Client,
    base_url: String,
    auth_token: Option<String>,
    csrf_token: Option<String>,
}

impl Default for TestClient {
    fn default() -> Self {
        Self::new()
    }
}

impl TestClient {
    /// Create a new test client
    pub fn new() -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .expect("Failed to create HTTP client");

        Self {
            client,
            base_url: get_backend_url(),
            auth_token: None,
            csrf_token: None,
        }
    }

    /// Set authentication token
    pub fn with_auth(mut self, token: &str) -> Self {
        self.auth_token = Some(token.to_string());
        self
    }

    /// Fetch CSRF token from server
    pub async fn fetch_csrf_token(&mut self) -> Result<String, String> {
        let url = format!("{}/api/csrf-token", self.base_url);

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| format!("Failed to fetch CSRF token: {}", e))?;

        if !response.status().is_success() {
            return Err(format!("CSRF fetch failed: {}", response.status()));
        }

        #[derive(Deserialize)]
        struct CsrfResponse {
            token: String,
        }

        let csrf: CsrfResponse = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse CSRF response: {}", e))?;

        self.csrf_token = Some(csrf.token.clone());
        Ok(csrf.token)
    }

    /// Make a GET request
    pub async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<ApiResponse<T>, String> {
        let url = format!("{}{}", self.base_url, path);

        let mut request = self.client.get(&url);

        if let Some(ref token) = self.auth_token {
            request = request.header("Authorization", format!("Bearer {}", token));
        }

        let response = request
            .send()
            .await
            .map_err(|e| format!("Request failed: {}", e))?;

        let status = response.status();

        if status == StatusCode::NOT_MODIFIED {
            return Ok(ApiResponse {
                success: true,
                message: Some("Not modified".to_string()),
                data: None,
            });
        }

        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(format!("HTTP {}: {}", status, error_text));
        }

        response
            .json()
            .await
            .map_err(|e| format!("Failed to parse response: {}", e))
    }

    /// Make a POST request
    pub async fn post<T: DeserializeOwned, B: Serialize>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<ApiResponse<T>, String> {
        let url = format!("{}{}", self.base_url, path);

        let mut request = self.client.post(&url).json(body);

        if let Some(ref token) = self.auth_token {
            request = request.header("Authorization", format!("Bearer {}", token));
        }

        if let Some(ref csrf) = self.csrf_token {
            request = request.header("x-csrf-token", csrf);
        }

        let response = request
            .send()
            .await
            .map_err(|e| format!("Request failed: {}", e))?;

        let status = response.status();

        if !status.is_success() && status != StatusCode::ACCEPTED {
            let error_text = response.text().await.unwrap_or_default();
            return Err(format!("HTTP {}: {}", status, error_text));
        }

        response
            .json()
            .await
            .map_err(|e| format!("Failed to parse response: {}", e))
    }

    /// Check if backend is reachable
    pub async fn is_online(&self) -> bool {
        let url = format!("{}/api/health", self.base_url);

        match self
            .client
            .get(&url)
            .timeout(Duration::from_secs(5))
            .send()
            .await
        {
            Ok(response) => response.status().is_success(),
            Err(_) => false,
        }
    }
}

// ============================================================================
// Health Check Tests
// ============================================================================

#[tokio::test]
#[ignore = "Requires running backend"]
async fn test_backend_health() {
    let client = TestClient::new();

    let is_online = client.is_online().await;

    assert!(
        is_online,
        "Backend should be online at {}",
        get_backend_url()
    );
    println!("✓ Backend health check passed");
}

// ============================================================================
// Sync Status Tests
// ============================================================================

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SyncStatus {
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub server_time: String,
    #[serde(default)]
    pub version: String,
}

#[tokio::test]
#[ignore = "Requires running backend with auth"]
async fn test_sync_status_requires_auth() {
    let client = TestClient::new();

    // Without auth, should get 401
    let result: Result<ApiResponse<SyncStatus>, _> = client.get("/api/pos/sync/status").await;

    assert!(result.is_err(), "Should require authentication");
    let error = result.unwrap_err();
    assert!(error.contains("401"), "Should return 401 Unauthorized");

    println!("✓ Sync status requires auth passed");
}

// ============================================================================
// Catalog Sync Tests (Structure Validation)
// ============================================================================

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct CatalogProduct {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub sku: Option<String>,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub name_ar: Option<String>,
    #[serde(default)]
    pub price: f64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogCategory {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub name_ar: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct CatalogResponse {
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub products: Option<Vec<CatalogProduct>>,
    #[serde(default)]
    pub categories: Option<Vec<CatalogCategory>>,
    #[serde(default)]
    pub last_updated: Option<String>,
    #[serde(default)]
    pub not_modified: bool,
}

#[tokio::test]
#[ignore = "Requires running backend with test data"]
async fn test_catalog_structure() {
    // This test needs a valid auth token - get from environment
    let auth_token =
        env::var("TEST_AUTH_TOKEN").expect("TEST_AUTH_TOKEN must be set for catalog tests");

    let client = TestClient::new().with_auth(&auth_token);

    let result: Result<ApiResponse<CatalogResponse>, _> = client
        .get("/api/pos/sync/catalog?includeCategories=true")
        .await;

    assert!(
        result.is_ok(),
        "Catalog request should succeed: {:?}",
        result.err()
    );

    let response = result.unwrap();
    assert!(response.success, "Response should indicate success");

    let catalog = response.data.expect("Response should have data");

    // Validate structure
    assert!(!catalog.version.is_empty(), "Version should not be empty");

    if let Some(products) = &catalog.products {
        for product in products {
            assert!(!product.id.is_empty(), "Product ID should not be empty");
            assert!(!product.name.is_empty(), "Product name should not be empty");
        }
    }

    println!("✓ Catalog structure validation passed");
    println!("  - Version: {}", catalog.version);
    println!(
        "  - Products: {}",
        catalog.products.map(|p| p.len()).unwrap_or(0)
    );
    println!(
        "  - Categories: {}",
        catalog.categories.map(|c| c.len()).unwrap_or(0)
    );
}

// ============================================================================
// ETag Caching Tests
// ============================================================================

#[tokio::test]
#[ignore = "Requires running backend with test data"]
async fn test_catalog_etag_caching() {
    let auth_token =
        env::var("TEST_AUTH_TOKEN").expect("TEST_AUTH_TOKEN must be set for catalog tests");

    // First request - get initial catalog
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .unwrap();

    let base_url = get_backend_url();
    let url = format!("{}/api/pos/sync/catalog", base_url);

    let response1 = client
        .get(&url)
        .header("Authorization", format!("Bearer {}", auth_token))
        .send()
        .await
        .expect("First request should succeed");

    assert!(response1.status().is_success());

    // Get ETag from response
    let etag = response1
        .headers()
        .get("etag")
        .expect("Response should have ETag header")
        .to_str()
        .unwrap()
        .to_string();

    println!("First request ETag: {}", etag);

    // Parse first response to verify structure
    let _body1: ApiResponse<CatalogResponse> = response1.json().await.unwrap();

    // Second request - with If-None-Match header
    let response2 = client
        .get(&url)
        .header("Authorization", format!("Bearer {}", auth_token))
        .header("If-None-Match", &etag)
        .send()
        .await
        .expect("Second request should succeed");

    // Should get 304 Not Modified
    assert_eq!(
        response2.status(),
        StatusCode::NOT_MODIFIED,
        "Second request with matching ETag should return 304"
    );

    println!("✓ ETag caching test passed");
    println!("  - First request: 200 OK with ETag");
    println!("  - Second request: 304 Not Modified");
}

// ============================================================================
// Products Sync Tests
// ============================================================================

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ProductsSyncResponse {
    #[serde(default)]
    pub sync_type: String,
    #[serde(default)]
    pub products: Vec<CatalogProduct>,
    #[serde(default)]
    pub total_count: i64,
    #[serde(default)]
    pub synced_at: String,
    #[serde(default)]
    pub has_more: bool,
}

#[tokio::test]
#[ignore = "Requires running backend with test data"]
async fn test_products_sync_full() {
    let auth_token = env::var("TEST_AUTH_TOKEN").expect("TEST_AUTH_TOKEN must be set");

    let client = TestClient::new().with_auth(&auth_token);

    let result: Result<ApiResponse<ProductsSyncResponse>, _> =
        client.get("/api/pos/sync/products").await;

    assert!(
        result.is_ok(),
        "Products sync should succeed: {:?}",
        result.err()
    );

    let response = result.unwrap();
    assert!(response.success);

    let data = response.data.expect("Should have data");
    assert_eq!(
        data.sync_type, "FULL",
        "Should be full sync without lastSync param"
    );

    println!("✓ Products full sync passed");
    println!("  - Sync type: {}", data.sync_type);
    println!("  - Products: {}", data.products.len());
    println!("  - Total: {}", data.total_count);
}

#[tokio::test]
#[ignore = "Requires running backend with test data"]
async fn test_products_sync_incremental() {
    let auth_token = env::var("TEST_AUTH_TOKEN").expect("TEST_AUTH_TOKEN must be set");

    let client = TestClient::new().with_auth(&auth_token);

    // Use a date from 1 hour ago
    let last_sync = chrono::Utc::now() - chrono::Duration::hours(1);
    let last_sync_str = last_sync.to_rfc3339();

    let result: Result<ApiResponse<ProductsSyncResponse>, _> = client
        .get(&format!(
            "/api/pos/sync/products?lastSync={}",
            last_sync_str
        ))
        .await;

    assert!(
        result.is_ok(),
        "Incremental sync should succeed: {:?}",
        result.err()
    );

    let response = result.unwrap();
    assert!(response.success);

    let data = response.data.expect("Should have data");
    assert_eq!(
        data.sync_type, "INCREMENTAL",
        "Should be incremental sync with lastSync param"
    );

    println!("✓ Products incremental sync passed");
    println!("  - Sync type: {}", data.sync_type);
    println!("  - Changed products: {}", data.products.len());
}

// ============================================================================
// Offline Upload Tests
// ============================================================================

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OfflineTransaction {
    pub local_id: String,
    pub terminal_id: String,
    pub transaction_type: String,
    pub items: Vec<OfflineTransactionItem>,
    pub payments: Vec<OfflinePayment>,
    pub subtotal: f64,
    pub tax_total: f64,
    pub grand_total: f64,
    pub created_at: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OfflineTransactionItem {
    pub product_id: String,
    pub product_name: String,
    pub quantity: f64,
    pub unit_price: f64,
    pub line_total: f64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OfflinePayment {
    pub payment_type: String,
    pub amount: f64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UploadRequest {
    pub transactions: Vec<OfflineTransaction>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct UploadResponse {
    #[serde(default)]
    pub received: i32,
    #[serde(default)]
    pub queued: i32,
    #[serde(default)]
    pub duplicates: i32,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub queue_ids: Vec<String>,
}

#[tokio::test]
#[ignore = "Requires running backend with test terminal"]
async fn test_offline_upload() {
    let auth_token = env::var("TEST_AUTH_TOKEN").expect("TEST_AUTH_TOKEN must be set");
    let terminal_id = env::var("TEST_TERMINAL_ID").expect("TEST_TERMINAL_ID must be set");

    let mut client = TestClient::new().with_auth(&auth_token);

    // Fetch CSRF token first
    client
        .fetch_csrf_token()
        .await
        .expect("Should fetch CSRF token");

    // Create test transaction
    let local_id = uuid::Uuid::new_v4().to_string();
    let transaction = OfflineTransaction {
        local_id: local_id.clone(),
        terminal_id: terminal_id.clone(),
        transaction_type: "SALE".to_string(),
        items: vec![OfflineTransactionItem {
            product_id: "test-product-001".to_string(),
            product_name: "Test Product".to_string(),
            quantity: 2.0,
            unit_price: 10.0,
            line_total: 20.0,
        }],
        payments: vec![OfflinePayment {
            payment_type: "CASH".to_string(),
            amount: 23.0, // 20 + 15% tax
        }],
        subtotal: 20.0,
        tax_total: 3.0,
        grand_total: 23.0,
        created_at: chrono::Utc::now().to_rfc3339(),
    };

    let request = UploadRequest {
        transactions: vec![transaction],
    };

    let result: Result<ApiResponse<UploadResponse>, _> =
        client.post("/api/pos/sync/upload", &request).await;

    assert!(result.is_ok(), "Upload should succeed: {:?}", result.err());

    let response = result.unwrap();
    assert!(response.success);

    let data = response.data.expect("Should have data");
    assert_eq!(data.received, 1);
    assert_eq!(data.queued, 1);
    assert_eq!(data.status, "queued");

    println!("✓ Offline upload test passed");
    println!("  - Local ID: {}", local_id);
    println!("  - Queue ID: {:?}", data.queue_ids.first());
}

// ============================================================================
// Terminal Authentication Tests
// ============================================================================

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalAuthRequest {
    pub hardware_id: String,
    pub secret: String,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct TerminalAuthResponse {
    #[serde(default)]
    pub session_token: String,
    #[serde(default)]
    pub terminal_id: String,
    #[serde(default)]
    pub terminal_code: String,
    #[serde(default)]
    pub company_id: String,
}

#[tokio::test]
#[ignore = "Requires registered test terminal"]
async fn test_terminal_authentication() {
    let hardware_id = env::var("TEST_HARDWARE_ID").expect("TEST_HARDWARE_ID must be set");
    let secret = env::var("TEST_TERMINAL_SECRET").expect("TEST_TERMINAL_SECRET must be set");

    let mut client = TestClient::new();

    // Fetch CSRF token
    client
        .fetch_csrf_token()
        .await
        .expect("Should fetch CSRF token");

    let request = TerminalAuthRequest {
        hardware_id,
        secret,
    };

    let result: Result<ApiResponse<TerminalAuthResponse>, _> =
        client.post("/api/pos/terminals/login", &request).await;

    assert!(
        result.is_ok(),
        "Terminal auth should succeed: {:?}",
        result.err()
    );

    let response = result.unwrap();
    assert!(response.success);

    let data = response.data.expect("Should have data");
    assert!(!data.session_token.is_empty());
    assert!(!data.terminal_id.is_empty());

    println!("✓ Terminal authentication test passed");
    println!("  - Terminal ID: {}", data.terminal_id);
    println!("  - Terminal Code: {}", data.terminal_code);
}

// ============================================================================
// JSON Serialization Compatibility Tests
// ============================================================================

/// Test that Rust serialization matches what backend expects
mod serialization_tests {
    use e2manage_pos_terminal::api::auth::{
        HeartbeatRequest, LoginTerminalRequest, VerifyPinRequest,
    };
    use e2manage_pos_terminal::models::OperatorId;

    #[test]
    fn test_login_request_json_format() {
        let request = LoginTerminalRequest {
            terminal_code: "TERM-001".to_string(),
            hardware_id: "TEST-HW-001".to_string(),
            secret: "secret-key".to_string(),
        };

        let json = serde_json::to_string(&request).unwrap();

        // Backend expects camelCase
        assert!(
            json.contains("hardwareId"),
            "Should use camelCase: {}",
            json
        );
        assert!(!json.contains("hardware_id"), "Should not use snake_case");
    }

    #[test]
    fn test_heartbeat_request_json_format() {
        let request = HeartbeatRequest {
            uptime_seconds: 3600,
            cpu_percent: 25.5,
            memory_mb: 512,
            disk_free_mb: 1024,
            offline_txn_count: 3,
            app_version: "1.0.0".to_string(),
            current_shift_id: None,
            current_operator_id: Some(OperatorId::new("op-001").unwrap()),
        };

        let json = serde_json::to_string(&request).unwrap();

        // Check camelCase
        assert!(json.contains("uptimeSeconds"), "Should use camelCase");
        assert!(json.contains("cpuPercent"), "Should use camelCase");
        assert!(json.contains("currentOperatorId"), "Should use camelCase");

        // None values should be skipped
        assert!(!json.contains("currentShiftId"), "None should be skipped");
    }

    #[test]
    fn test_verify_pin_request_json_format() {
        let request = VerifyPinRequest {
            operator_id: OperatorId::new("op-001").unwrap(),
            pin: "1234".to_string(),
        };

        let json = serde_json::to_string(&request).unwrap();

        assert!(json.contains("operatorId"), "Should use camelCase");
        assert!(json.contains("\"1234\""), "PIN should be string");
    }
}

/// Test that backend responses can be deserialized by Rust
mod deserialization_tests {
    use super::*;

    #[test]
    fn test_catalog_response_deserialization() {
        // Simulate backend response format
        let json = r#"{
            "success": true,
            "message": "Catalog retrieved",
            "data": {
                "version": "abc123",
                "products": [
                    {
                        "id": "prod-001",
                        "code": "SKU001",
                        "sku": "SKU001",
                        "name": "Test Product",
                        "nameAr": "منتج تجريبي",
                        "price": 10.50,
                        "taxRate": 15.0,
                        "stockQuantity": 100,
                        "isActive": true
                    }
                ],
                "categories": [
                    {
                        "id": "cat-001",
                        "name": "Food",
                        "nameAr": "طعام"
                    }
                ],
                "lastUpdated": "2025-01-01T00:00:00Z"
            }
        }"#;

        let response: ApiResponse<CatalogResponse> = serde_json::from_str(json).unwrap();

        assert!(response.success);
        let data = response.data.unwrap();
        assert_eq!(data.version, "abc123");
        assert_eq!(data.products.as_ref().unwrap().len(), 1);
        assert_eq!(data.products.as_ref().unwrap()[0].name, "Test Product");
    }

    #[test]
    fn test_upload_response_deserialization() {
        let json = r#"{
            "success": true,
            "message": "Transactions queued",
            "data": {
                "received": 2,
                "queued": 2,
                "duplicates": 0,
                "queueIds": ["queue-001", "queue-002"],
                "status": "queued"
            }
        }"#;

        let response: ApiResponse<UploadResponse> = serde_json::from_str(json).unwrap();

        assert!(response.success);
        let data = response.data.unwrap();
        assert_eq!(data.received, 2);
        assert_eq!(data.queued, 2);
        assert_eq!(data.queue_ids.len(), 2);
    }

    #[test]
    fn test_304_not_modified_handling() {
        // 304 responses have no body, just headers
        // The client should handle this gracefully
        let response = ApiResponse::<CatalogResponse> {
            success: true,
            message: Some("Not modified".to_string()),
            data: None,
        };

        assert!(response.success);
        assert!(response.data.is_none());
    }
}

// ============================================================================
// Contract Validation Tests
// ============================================================================

/// These tests validate that the Rust DTOs match backend API contract
mod contract_tests {
    use e2manage_pos_terminal::api::sync::{CategoryDto, OperatorDto, ProductDto};
    use e2manage_pos_terminal::models::OperatorRole;

    #[test]
    fn test_product_dto_matches_backend() {
        // Backend sends this format
        let backend_json = r#"{
            "id": "prod-001",
            "sku": "SKU001",
            "barcode": "1234567890123",
            "name": "Test Product",
            "nameAr": "منتج",
            "price": 25.50,
            "cost": 15.00,
            "taxRate": 15.0,
            "taxInclusive": false,
            "categoryId": "cat-001",
            "categoryName": "Food",
            "unit": "UNIT",
            "stockQty": 100,
            "minStock": 10,
            "allowNegativeStock": false,
            "isWeighable": false,
            "isSerialized": false,
            "isActive": true
        }"#;

        let product: ProductDto = serde_json::from_str(backend_json).unwrap();

        assert_eq!(product.id, "prod-001");
        assert_eq!(product.sku, "SKU001");
        assert_eq!(product.price, 25.50);
        assert_eq!(product.tax_rate, 15.0);
        assert!(product.is_active);
    }

    #[test]
    fn test_category_dto_matches_backend() {
        let backend_json = r##"{
            "id": "cat-001",
            "parentId": null,
            "name": "Food",
            "nameAr": "طعام",
            "color": "#FF5722",
            "icon": "food",
            "displayOrder": 1,
            "isActive": true
        }"##;

        let category: CategoryDto = serde_json::from_str(backend_json).unwrap();

        assert_eq!(category.id, "cat-001");
        assert_eq!(category.name, "Food");
        assert_eq!(category.color, Some(String::from("#FF5722")));
    }

    #[test]
    fn test_operator_dto_matches_backend() {
        let backend_json = r#"{
            "id": "op-001",
            "employeeNumber": "C001",
            "name": "Ahmed",
            "nameAr": "أحمد",
            "pinHash": "$2b$12$abcdef...",
            "role": "CASHIER",
            "avatarUrl": null,
            "permissions": {"canVoid":true},
            "isActive": true
        }"#;

        let operator: OperatorDto = serde_json::from_str(backend_json).unwrap();

        assert_eq!(operator.id.as_str(), "op-001");
        assert_eq!(operator.employee_number, Some("C001".to_string()));
        assert_eq!(operator.role, OperatorRole::Cashier);
        assert!(operator.is_active);
    }
}
