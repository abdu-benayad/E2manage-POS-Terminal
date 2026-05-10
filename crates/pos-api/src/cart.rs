//! Cart API - Backend cart/draft communication
//!
//! Handles cloud-synced drafts (carts) that can be shared across terminals.
//! Enables seamless handoff when customers need to switch terminals.
//!
//! ## Features
//!
//! - Create carts from POS terminal with unique 6-char tokens
//! - List carts for the same warehouse
//! - Recall carts by token
//! - Mark carts as converted (completed transaction)
//! - Delete/cancel carts
//!
//! ## Usage
//!
//! ```rust,ignore
//! use pos_api::{ApiClient, cart::*};
//!
//! // Create a shared draft
//! let request = CreateCartRequest {
//!     warehouse_id: "wh-001".to_string(),
//!     source: "POS".to_string(),
//!     device_id: Some("TERM-001".to_string()),
//!     customer_id: None,
//!     customer_name: None,
//!     currency: "LYD".to_string(),
//!     notes: None,
//!     items: vec![CartItemDto { ... }],
//! };
//! let cart = client.create_cart(&request).await?;
//! println!("Draft token: {}", cart.token);
//!
//! // Recall by token on another terminal
//! let cart = client.get_cart_by_token("A1B2C3").await?;
//! ```

use anyhow::Result;
use pos_models::product::ProductType;
use serde::{Deserialize, Serialize};

use super::ApiClient;

// ============================================================================
// Request DTOs
// ============================================================================

/// Request to create a new cart (shared draft)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateCartRequest {
    /// Warehouse/store ID
    pub warehouse_id: String,
    /// Source of the cart (always "POS" for terminal)
    pub source: String,
    /// Terminal device ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_id: Option<String>,
    /// Operator/cashier ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub operator_id: Option<String>,
    /// Operator name for display
    #[serde(skip_serializing_if = "Option::is_none")]
    pub operator_name: Option<String>,
    /// Customer ID (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub customer_id: Option<String>,
    /// Customer name (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub customer_name: Option<String>,
    /// Currency code (e.g., "LYD")
    pub currency: String,
    /// Optional notes
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    /// Cart-level discount percentage
    #[serde(skip_serializing_if = "Option::is_none")]
    pub discount_percent: Option<f64>,
    /// Cart-level fixed discount amount
    #[serde(skip_serializing_if = "Option::is_none")]
    pub discount_amount: Option<f64>,
    /// Cart items
    pub items: Vec<CartItemDto>,
}

/// Cart item data for API requests/responses
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CartItemDto {
    /// Product ID
    pub product_id: String,
    /// Product SKU
    pub product_sku: String,
    /// Product name
    pub product_name: String,
    /// Product name in Arabic
    #[serde(skip_serializing_if = "Option::is_none")]
    pub product_name_ar: Option<String>,
    /// Barcode
    #[serde(skip_serializing_if = "Option::is_none")]
    pub barcode: Option<String>,
    /// Quantity
    pub quantity: f64,
    /// Unit of measure
    #[serde(default = "default_unit")]
    pub unit: String,
    /// Unit price
    pub unit_price: f64,
    /// Tax rate percentage
    #[serde(default)]
    pub tax_rate: f64,
    /// Whether price is tax-inclusive
    #[serde(default)]
    pub tax_inclusive: bool,
    /// Discount percentage on this item
    #[serde(skip_serializing_if = "Option::is_none")]
    pub discount_percent: Option<f64>,
    /// Discount amount per unit
    #[serde(skip_serializing_if = "Option::is_none")]
    pub discount_amount: Option<f64>,
    /// Item note
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// Product type (e.g., PHYSICAL_GOOD, SERVICE, BUNDLE)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub product_type: Option<ProductType>,
}

fn default_unit() -> String {
    "UNIT".to_string()
}

/// Request to convert a cart to a transaction
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConvertCartRequest {
    /// Transaction ID that the cart was converted to
    pub transaction_id: String,
}

// ============================================================================
// Parking API DTOs
// ============================================================================

/// Request to create and park a cart in one operation
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateAndParkCartRequest {
    /// Warehouse ID
    pub warehouse_id: String,
    /// Optional display name (e.g., "Blue shirt customer")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    /// Operator ID
    pub operator_id: String,
    /// Operator name
    pub operator_name: String,
    /// Terminal ID
    pub terminal_id: String,
    /// Customer ID (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub customer_id: Option<String>,
    /// Customer name (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub customer_name: Option<String>,
    /// Customer phone (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub customer_phone: Option<String>,
    /// Currency (default: LYD)
    #[serde(default = "default_currency")]
    pub currency: String,
    /// Cart notes
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    /// Cart items
    pub items: Vec<ParkingCartItemDto>,
    /// Expiry in minutes (default: 240 = 4 hours)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expiry_minutes: Option<i32>,
}

/// Cart item for parking API
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParkingCartItemDto {
    pub product_id: String,
    pub product_sku: String,
    pub product_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub product_name_ar: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub barcode: Option<String>,
    pub quantity: f64,
    pub unit_price: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tax_rate: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub discount_percent: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub discount_amount: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

/// Request to park a cart
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ParkCartRequest {
    /// Optional display name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    /// Operator ID
    pub operator_id: String,
    /// Operator name
    pub operator_name: String,
    /// Terminal ID
    pub terminal_id: String,
    /// Expiry in minutes
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expiry_minutes: Option<i32>,
}

/// Request to recall a parked cart
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecallCartRequest {
    /// Operator ID
    pub operator_id: String,
    /// Operator name
    pub operator_name: String,
    /// Terminal ID
    pub terminal_id: String,
    /// Force recall even if locked by another terminal
    #[serde(default)]
    pub force: bool,
}

/// Parked cart response (list view)
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParkedCartResponse {
    pub id: String,
    pub pos_token: String,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub customer_id: Option<String>,
    #[serde(default)]
    pub customer_name: Option<String>,
    #[serde(default)]
    pub customer_phone: Option<String>,
    pub item_count: i32,
    pub subtotal: f64,
    pub tax_amount: f64,
    pub discount_amount: f64,
    pub total_amount: f64,
    pub currency: String,
    pub status: String,
    pub is_recalled: bool,
    pub is_expired: bool,
    #[serde(default)]
    pub parked_by_operator_name: Option<String>,
    #[serde(default)]
    pub parked_by_terminal_id: Option<String>,
    #[serde(default)]
    pub parked_at: Option<String>,
    #[serde(default)]
    pub recalled_by_operator_name: Option<String>,
    #[serde(default)]
    pub recalled_by_terminal_id: Option<String>,
    #[serde(default)]
    pub recalled_at: Option<String>,
    #[serde(default)]
    pub expires_at: Option<String>,
    pub created_at: String,
}

/// Parked cart item in detail response
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParkedCartItemResponse {
    pub id: String,
    pub product_id: String,
    pub product_sku: String,
    pub product_name: String,
    #[serde(default)]
    pub product_name_ar: Option<String>,
    #[serde(default)]
    pub barcode: Option<String>,
    pub quantity: f64,
    pub unit_price: f64,
    #[serde(default)]
    pub discount_percent: Option<f64>,
    pub discount_amount: f64,
    #[serde(default)]
    pub tax_rate: Option<f64>,
    pub tax_amount: f64,
    pub line_subtotal: f64,
    pub line_total: f64,
    #[serde(default)]
    pub notes: Option<String>,
}

/// Parked cart detail response (includes items)
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParkedCartDetailResponse {
    pub id: String,
    pub pos_token: String,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub customer_id: Option<String>,
    #[serde(default)]
    pub customer_name: Option<String>,
    #[serde(default)]
    pub customer_phone: Option<String>,
    pub item_count: i32,
    pub subtotal: f64,
    pub tax_amount: f64,
    pub discount_amount: f64,
    pub total_amount: f64,
    pub currency: String,
    pub status: String,
    pub is_recalled: bool,
    pub is_expired: bool,
    #[serde(default)]
    pub parked_by_operator_name: Option<String>,
    #[serde(default)]
    pub parked_by_terminal_id: Option<String>,
    #[serde(default)]
    pub parked_at: Option<String>,
    #[serde(default)]
    pub recalled_by_operator_name: Option<String>,
    #[serde(default)]
    pub recalled_by_terminal_id: Option<String>,
    #[serde(default)]
    pub recalled_at: Option<String>,
    #[serde(default)]
    pub expires_at: Option<String>,
    pub created_at: String,
    pub items: Vec<ParkedCartItemResponse>,
    #[serde(default)]
    pub notes: Option<String>,
}

// ============================================================================
// Response DTOs
// ============================================================================

/// Cart response from backend
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CartResponse {
    /// Cart UUID
    pub id: String,
    /// Unique 6-character token for recall (e.g., "A1B2C3")
    pub token: String,
    /// Display name (optional)
    #[serde(default)]
    pub name: Option<String>,
    /// Cart source
    #[serde(default)]
    pub source: String,
    /// Cart status
    pub status: String,
    /// Number of line items
    #[serde(default)]
    pub item_count: i32,
    /// Total quantity of all items
    #[serde(default)]
    pub total_quantity: f64,
    /// Total amount
    #[serde(default)]
    pub total_amount: f64,
    /// Currency
    #[serde(default = "default_currency")]
    pub currency: String,
    /// Warehouse ID
    pub warehouse_id: String,
    /// Device/terminal ID
    #[serde(default)]
    pub device_id: Option<String>,
    /// Operator ID
    #[serde(default)]
    pub operator_id: Option<String>,
    /// Operator name
    #[serde(default)]
    pub operator_name: Option<String>,
    /// Customer ID
    #[serde(default)]
    pub customer_id: Option<String>,
    /// Customer name
    #[serde(default)]
    pub customer_name: Option<String>,
    /// Cart-level discount percentage
    #[serde(default)]
    pub discount_percent: Option<f64>,
    /// Cart-level fixed discount amount
    #[serde(default)]
    pub discount_amount: Option<f64>,
    /// Notes
    #[serde(default)]
    pub notes: Option<String>,
    /// Cart items
    #[serde(default)]
    pub items: Vec<CartItemDto>,
    /// When the cart was created
    pub created_at: String,
    /// When the cart expires
    #[serde(default)]
    pub expires_at: Option<String>,
    /// Last updated timestamp
    #[serde(default)]
    pub updated_at: Option<String>,
}

fn default_currency() -> String {
    "LYD".to_string()
}

/// Cart list response
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CartListResponse {
    /// List of carts
    pub carts: Vec<CartResponse>,
    /// Total count
    #[serde(default)]
    pub total: i32,
}

// ============================================================================
// API Client Implementation
// ============================================================================

impl ApiClient {
    /// Creates a new cart (shared draft) on the backend
    ///
    /// The backend generates a unique 6-character token that can be used
    /// to recall the cart on any terminal in the same warehouse.
    ///
    /// # Arguments
    ///
    /// * `request` - Cart creation request with items and metadata
    ///
    /// # Returns
    ///
    /// Created cart with generated token
    pub async fn create_cart(&self, request: &CreateCartRequest) -> Result<CartResponse> {
        self.post_envelope("/api/carts", request).await
    }

    /// Lists all POS carts for a warehouse
    ///
    /// Filters carts by source=POS to only show terminal-created carts.
    ///
    /// # Arguments
    ///
    /// * `warehouse_id` - Warehouse to list carts for
    ///
    /// # Returns
    ///
    /// List of carts for the warehouse
    pub async fn list_pos_carts(&self, warehouse_id: &str) -> Result<CartListResponse> {
        let url = format!(
            "/api/carts?warehouseId={}&source=POS&status=ACTIVE",
            urlencoding::encode(warehouse_id)
        );
        self.get_envelope(&url).await
    }

    /// Gets a cart by its token
    ///
    /// Used to recall a draft created on another terminal.
    ///
    /// # Arguments
    ///
    /// * `token` - The 6-character token
    ///
    /// # Returns
    ///
    /// The cart with all items
    pub async fn get_cart_by_token(&self, token: &str) -> Result<CartResponse> {
        let url = format!("/api/carts/token/{}", urlencoding::encode(token));
        self.get_envelope(&url).await
    }

    /// Gets a cart by ID
    ///
    /// # Arguments
    ///
    /// * `id` - The cart UUID
    ///
    /// # Returns
    ///
    /// The cart with all items
    pub async fn get_cart_by_id(&self, id: &str) -> Result<CartResponse> {
        let url = format!("/api/carts/{}", urlencoding::encode(id));
        self.get_envelope(&url).await
    }

    /// Marks a cart as converted (completed transaction)
    ///
    /// Called after a transaction is completed from this cart.
    /// The cart will be marked as CONVERTED and won't appear in active lists.
    ///
    /// # Arguments
    ///
    /// * `id` - Cart UUID
    /// * `transaction_id` - The transaction ID it was converted to
    ///
    /// # Returns
    ///
    /// Updated cart response
    pub async fn convert_cart(&self, id: &str, transaction_id: &str) -> Result<CartResponse> {
        let url = format!("/api/carts/{}/convert", urlencoding::encode(id));
        let request = ConvertCartRequest {
            transaction_id: transaction_id.to_string(),
        };
        self.post_envelope(&url, &request).await
    }

    /// Deletes/cancels a cart
    ///
    /// Removes the cart from the system. Use when a draft is no longer needed.
    ///
    /// # Arguments
    ///
    /// * `id` - Cart UUID to delete
    pub async fn delete_cart(&self, id: &str) -> Result<()> {
        let url = format!("/api/carts/{}", urlencoding::encode(id));
        let _: serde_json::Value = self.delete(&url).await?;
        Ok(())
    }

    // ========================================================================
    // Cart Parking API (POS-specific endpoints)
    // ========================================================================

    /// Creates and parks a new cart
    ///
    /// Creates a cart with items and immediately parks it with a POS token (P001, P002...).
    /// Use this for the "Hold" / "Park" functionality.
    ///
    /// # Arguments
    ///
    /// * `request` - Cart creation request with parking metadata
    ///
    /// # Returns
    ///
    /// Parked cart with POS token and items
    pub async fn create_and_park_cart(
        &self,
        request: &CreateAndParkCartRequest,
    ) -> Result<ParkedCartDetailResponse> {
        self.post_envelope("/api/pos/parking", request).await
    }

    /// Lists parked carts for a warehouse
    ///
    /// Returns all parked and recalled carts for the warehouse.
    ///
    /// # Arguments
    ///
    /// * `warehouse_id` - Warehouse to list parked carts for
    /// * `search` - Optional search term (display name, customer name, POS token)
    /// * `include_expired` - Whether to include expired carts
    ///
    /// # Returns
    ///
    /// List of parked carts
    pub async fn list_parked_carts(
        &self,
        warehouse_id: &str,
        search: Option<&str>,
        include_expired: bool,
    ) -> Result<Vec<ParkedCartResponse>> {
        let mut url = format!(
            "/api/pos/parking?warehouseId={}&includeExpired={}",
            urlencoding::encode(warehouse_id),
            include_expired
        );
        if let Some(s) = search {
            url.push_str(&format!("&search={}", urlencoding::encode(s)));
        }
        self.get_envelope(&url).await
    }

    /// Gets a parked cart by POS token (P001, P002...)
    ///
    /// # Arguments
    ///
    /// * `pos_token` - The POS token (e.g., "P001")
    ///
    /// # Returns
    ///
    /// The parked cart with items
    pub async fn get_parked_cart_by_token(
        &self,
        pos_token: &str,
    ) -> Result<ParkedCartDetailResponse> {
        let url = format!("/api/pos/parking/token/{}", urlencoding::encode(pos_token));
        self.get_envelope(&url).await
    }

    /// Gets a parked cart by ID
    ///
    /// # Arguments
    ///
    /// * `id` - The cart UUID
    ///
    /// # Returns
    ///
    /// The parked cart with items
    pub async fn get_parked_cart_by_id(&self, id: &str) -> Result<ParkedCartDetailResponse> {
        let url = format!("/api/pos/parking/{}", urlencoding::encode(id));
        self.get_envelope(&url).await
    }

    /// Recalls a parked cart
    ///
    /// Sets the cart to RECALLED status with soft lock.
    /// If already recalled by another terminal, use `force: true` after timeout.
    ///
    /// # Arguments
    ///
    /// * `id` - Cart UUID to recall
    /// * `request` - Recall request with operator/terminal info
    ///
    /// # Returns
    ///
    /// The recalled cart with items
    pub async fn recall_parked_cart(
        &self,
        id: &str,
        request: &RecallCartRequest,
    ) -> Result<ParkedCartDetailResponse> {
        let url = format!("/api/pos/parking/{}/recall", urlencoding::encode(id));
        self.post_envelope(&url, request).await
    }

    /// Recalls a parked cart by POS token
    ///
    /// # Arguments
    ///
    /// * `pos_token` - The POS token (e.g., "P001")
    /// * `request` - Recall request with operator/terminal info
    ///
    /// # Returns
    ///
    /// The recalled cart with items
    pub async fn recall_parked_cart_by_token(
        &self,
        pos_token: &str,
        request: &RecallCartRequest,
    ) -> Result<ParkedCartDetailResponse> {
        let url = format!(
            "/api/pos/parking/token/{}/recall",
            urlencoding::encode(pos_token)
        );
        self.post_envelope(&url, request).await
    }

    /// Releases a recalled cart back to PARKED status
    ///
    /// Use when operator decides not to complete the cart.
    ///
    /// # Arguments
    ///
    /// * `id` - Cart UUID to release
    /// * `terminal_id` - Current terminal ID (must match recalling terminal)
    pub async fn release_parked_cart(
        &self,
        id: &str,
        terminal_id: &str,
    ) -> Result<ParkedCartDetailResponse> {
        let url = format!("/api/pos/parking/{}/release", urlencoding::encode(id));
        let body = serde_json::json!({ "terminalId": terminal_id });
        self.post_envelope(&url, &body).await
    }

    /// Re-parks a recalled cart after modifications
    ///
    /// # Arguments
    ///
    /// * `id` - Cart UUID to re-park
    /// * `request` - Park request with updated metadata
    pub async fn repark_cart(
        &self,
        id: &str,
        request: &ParkCartRequest,
    ) -> Result<ParkedCartDetailResponse> {
        let url = format!("/api/pos/parking/{}/repark", urlencoding::encode(id));
        self.post_envelope(&url, request).await
    }

    /// Parks an existing DRAFT cart
    ///
    /// # Arguments
    ///
    /// * `id` - Cart UUID to park
    /// * `request` - Park request with metadata
    pub async fn park_cart(
        &self,
        id: &str,
        request: &ParkCartRequest,
    ) -> Result<ParkedCartDetailResponse> {
        let url = format!("/api/pos/parking/{}/park", urlencoding::encode(id));
        self.post_envelope(&url, request).await
    }

    /// Voids/cancels a parked cart
    ///
    /// # Arguments
    ///
    /// * `id` - Cart UUID to void
    pub async fn void_parked_cart(&self, id: &str) -> Result<()> {
        let url = format!("/api/pos/parking/{}", urlencoding::encode(id));
        let _: serde_json::Value = self.delete(&url).await?;
        Ok(())
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_cart_request_serialization() {
        let request = CreateCartRequest {
            warehouse_id: "wh-001".to_string(),
            source: "POS".to_string(),
            device_id: Some("TERM-001".to_string()),
            operator_id: Some("op-001".to_string()),
            operator_name: Some("Ahmed".to_string()),
            customer_id: None,
            customer_name: None,
            currency: "LYD".to_string(),
            notes: None,
            discount_percent: None,
            discount_amount: None,
            items: vec![CartItemDto {
                product_id: "prod-001".to_string(),
                product_sku: "SKU001".to_string(),
                product_name: "Test Product".to_string(),
                product_name_ar: Some("منتج تجريبي".to_string()),
                barcode: Some("123456".to_string()),
                quantity: 2.0,
                unit: "UNIT".to_string(),
                unit_price: 10.0,
                tax_rate: 15.0,
                tax_inclusive: false,
                discount_percent: None,
                discount_amount: None,
                note: None,
                product_type: None,
            }],
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("warehouseId"));
        assert!(json.contains("TERM-001"));
        assert!(json.contains("prod-001"));
    }

    #[test]
    fn test_cart_response_deserialization() {
        let json = r#"{
            "id": "cart-123",
            "token": "A1B2C3",
            "name": "Hold #1",
            "source": "POS",
            "status": "ACTIVE",
            "itemCount": 2,
            "totalQuantity": 5.0,
            "totalAmount": 25.5,
            "currency": "LYD",
            "warehouseId": "wh-001",
            "deviceId": "TERM-001",
            "operatorId": "op-001",
            "operatorName": "Ahmed",
            "createdAt": "2025-01-01T00:00:00Z",
            "items": [
                {
                    "productId": "prod-001",
                    "productSku": "SKU001",
                    "productName": "Test",
                    "quantity": 2.0,
                    "unitPrice": 10.0
                }
            ]
        }"#;

        let response: CartResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.id, "cart-123");
        assert_eq!(response.token, "A1B2C3");
        assert_eq!(response.status, "ACTIVE");
        assert_eq!(response.items.len(), 1);
    }

    #[test]
    fn test_cart_list_response_deserialization() {
        let json = r#"{
            "carts": [
                {
                    "id": "cart-1",
                    "token": "ABC123",
                    "status": "ACTIVE",
                    "warehouseId": "wh-001",
                    "createdAt": "2025-01-01T00:00:00Z"
                }
            ],
            "total": 1
        }"#;

        let response: CartListResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.carts.len(), 1);
        assert_eq!(response.total, 1);
    }

    #[test]
    fn test_cart_item_dto_defaults() {
        let json = r#"{
            "productId": "prod-001",
            "productSku": "SKU001",
            "productName": "Test",
            "quantity": 1.0,
            "unitPrice": 10.0
        }"#;

        let item: CartItemDto = serde_json::from_str(json).unwrap();
        assert_eq!(item.unit, "UNIT");
        assert_eq!(item.tax_rate, 0.0);
        assert!(!item.tax_inclusive);
        // product_type is optional — absent from JSON means None
        assert!(item.product_type.is_none());
    }

    #[test]
    fn test_cart_item_dto_product_type_present_in_json() {
        let item = CartItemDto {
            product_id: "prod-service".to_string(),
            product_sku: "SVC001".to_string(),
            product_name: "Consulting".to_string(),
            product_name_ar: None,
            barcode: None,
            quantity: 1.0,
            unit: "HOUR".to_string(),
            unit_price: 100.0,
            tax_rate: 0.0,
            tax_inclusive: false,
            discount_percent: None,
            discount_amount: None,
            note: None,
            product_type: Some(ProductType::Service),
        };

        let json = serde_json::to_string(&item).unwrap();
        assert!(
            json.contains("\"productType\""),
            "productType field must be present when set"
        );
        assert!(
            json.contains("SERVICE"),
            "productType must serialize to SERVICE"
        );

        // Round-trip
        let deserialized: CartItemDto = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.product_type, Some(ProductType::Service));
    }

    #[test]
    fn test_cart_item_dto_product_type_omitted_when_none() {
        let item = CartItemDto {
            product_id: "prod-001".to_string(),
            product_sku: "SKU001".to_string(),
            product_name: "Test".to_string(),
            product_name_ar: None,
            barcode: None,
            quantity: 1.0,
            unit: "UNIT".to_string(),
            unit_price: 10.0,
            tax_rate: 0.0,
            tax_inclusive: false,
            discount_percent: None,
            discount_amount: None,
            note: None,
            product_type: None,
        };

        let json = serde_json::to_string(&item).unwrap();
        assert!(
            !json.contains("productType"),
            "productType must be omitted when None"
        );
    }

    #[test]
    fn test_cart_item_dto_deserializes_product_type_from_json() {
        let json = r#"{
            "productId": "prod-001",
            "productSku": "SKU001",
            "productName": "Test",
            "quantity": 1.0,
            "unitPrice": 10.0,
            "productType": "PHYSICAL_GOOD"
        }"#;

        let item: CartItemDto = serde_json::from_str(json).unwrap();
        assert_eq!(item.product_type, Some(ProductType::PhysicalGood));
    }
}
