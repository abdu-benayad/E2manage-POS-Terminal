//! Shared Draft Service
//!
//! Manages cloud-synced drafts that can be shared across terminals in the same
//! warehouse. Enables seamless handoff when customers need to switch terminals
//! due to hardware issues.
//!
//! ## Features
//!
//! - Create shared drafts (synced to backend with unique 6-char token)
//! - Recall drafts by token on any terminal in the warehouse
//! - Offline support with sync queue
//! - Automatic sync of pending operations
//!
//! ## Usage
//!
//! ```rust,ignore
//! use pos_services::SharedDraftService;
//!
//! let service = SharedDraftService::new(
//!     api, db, "wh-001".to_string(), "TERM-001".to_string()
//! );
//!
//! // Save cart as shared draft
//! let draft = service.save_shared_draft(&cart, Some("Ahmed's order"), "op-1", "shift-1").await?;
//! println!("Token: {}", draft.token); // e.g., "A1B2C3"
//!
//! // On another terminal, recall by token
//! let cart = service.recall_by_token("A1B2C3").await?;
//! ```

use anyhow::{anyhow, Result};
use chrono::Utc;
use rust_decimal::prelude::*;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use thiserror::Error;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use pos_api::cart::{
    CartItemDto, CartResponse, CreateAndParkCartRequest, CreateCartRequest,
    ParkedCartDetailResponse, ParkedCartResponse, ParkingCartItemDto, RecallCartRequest,
};
use pos_api::ApiClient;
use pos_db::draft_sync_queue::{DraftSyncOperation, DraftSyncQueueItem};
use pos_db::shared_drafts::{SharedDraftRow, SharedDraftSyncStatus};
use pos_db::Database;
use pos_models::cart::{Cart, CartItem};

// ============================================================================
// Error Types
// ============================================================================

/// Errors that can occur during shared draft operations
#[derive(Debug, Error)]
pub enum SharedDraftError {
    #[error("Draft not found: {0}")]
    NotFound(String),

    #[error("Draft has expired: {0}")]
    Expired(String),

    #[error("Cannot save empty cart")]
    EmptyCart,

    #[error("Invalid token format: {0}")]
    InvalidToken(String),

    #[error("Network error: {0}")]
    NetworkError(String),

    #[error("Failed to serialize cart: {0}")]
    SerializationError(String),

    #[error("Failed to deserialize cart: {0}")]
    DeserializationError(String),

    #[error("Database error: {0}")]
    DatabaseError(String),

    #[error("API error: {0}")]
    ApiError(String),
}

/// Result type for shared draft operations
pub type SharedDraftResult<T> = Result<T, SharedDraftError>;

// ============================================================================
// Shared Draft Model
// ============================================================================

/// A shared draft that can be accessed across terminals
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SharedDraft {
    /// Backend cart UUID
    pub id: String,
    /// Unique 6-character token for recall
    pub token: String,
    /// Display name
    pub name: Option<String>,
    /// The cart contents
    pub cart: Cart,
    /// Number of line items
    pub item_count: usize,
    /// Total amount
    pub total_amount: f64,
    /// Currency
    pub currency: String,
    /// Warehouse ID
    pub warehouse_id: String,
    /// Terminal device ID that created the draft
    pub device_id: Option<String>,
    /// Operator ID who created the draft
    pub operator_id: Option<String>,
    /// Operator name
    pub operator_name: Option<String>,
    /// When the draft was created
    pub created_at: String,
    /// When the draft expires
    pub expires_at: Option<String>,
    /// Whether this is a local-only draft (not yet synced)
    pub is_local_only: bool,
}

impl SharedDraft {
    /// Returns true if this is a local-only token (starts with "L")
    pub fn is_local_token(token: &str) -> bool {
        token.starts_with('L')
    }

    /// Returns the display name
    pub fn display_name(&self) -> &str {
        self.name.as_deref().unwrap_or("Shared Draft")
    }
}

/// Result of a sync queue processing operation
#[derive(Debug, Clone, Default)]
pub struct SyncQueueResult {
    /// Number of items successfully synced
    pub synced: u32,
    /// Number of items that failed
    pub failed: u32,
}

impl SyncQueueResult {
    pub fn has_activity(&self) -> bool {
        self.synced > 0 || self.failed > 0
    }
}

// ============================================================================
// Shared Draft Service
// ============================================================================

/// Service for managing cloud-synced shared drafts
pub struct SharedDraftService {
    api: Arc<ApiClient>,
    db: Arc<Database>,
    warehouse_id: String,
    device_id: String,
}

impl SharedDraftService {
    /// Creates a new shared draft service
    ///
    /// # Arguments
    ///
    /// * `api` - Shared API client
    /// * `db` - Shared database
    /// * `warehouse_id` - Current warehouse ID
    /// * `device_id` - Current terminal device ID
    pub fn new(
        api: Arc<ApiClient>,
        db: Arc<Database>,
        warehouse_id: String,
        device_id: String,
    ) -> Self {
        Self {
            api,
            db,
            warehouse_id,
            device_id,
        }
    }

    /// Saves a cart as a shared draft
    ///
    /// When online: Creates on backend, gets token, caches locally
    /// When offline: Saves locally with temp token, queues for sync
    ///
    /// # Arguments
    ///
    /// * `cart` - The cart to save
    /// * `name` - Optional display name
    /// * `operator_id` - Current operator ID
    /// * `operator_name` - Current operator name
    ///
    /// # Returns
    ///
    /// The created shared draft with token
    pub async fn save_shared_draft(
        &self,
        cart: &Cart,
        name: Option<String>,
        operator_id: &str,
        operator_name: &str,
    ) -> SharedDraftResult<SharedDraft> {
        // Validate cart is not empty
        if cart.is_empty() {
            return Err(SharedDraftError::EmptyCart);
        }

        // Build API request
        let items: Vec<CartItemDto> = cart
            .items
            .iter()
            .map(|item| CartItemDto {
                product_id: item.product_id.clone(),
                product_sku: item.sku.clone(),
                product_name: item.product_name.clone(),
                product_name_ar: item.product_name_ar.clone(),
                barcode: item.barcode.clone(),
                quantity: item.quantity.to_f64().unwrap_or(0.0),
                unit: format!("{:?}", item.unit),
                unit_price: item.unit_price.to_f64().unwrap_or(0.0),
                tax_rate: item.tax_rate.to_f64().unwrap_or(0.0),
                tax_inclusive: item.tax_inclusive,
                discount_percent: if item.discount_percent > Decimal::ZERO {
                    Some(item.discount_percent.to_f64().unwrap_or(0.0))
                } else {
                    None
                },
                discount_amount: if item.discount_amount > Decimal::ZERO {
                    Some(item.discount_amount.to_f64().unwrap_or(0.0))
                } else {
                    None
                },
                note: item.note.clone(),
                product_type: Some(item.product_type.clone()),
            })
            .collect();

        let request = CreateCartRequest {
            warehouse_id: self.warehouse_id.clone(),
            source: "POS".to_string(),
            device_id: Some(self.device_id.clone()),
            operator_id: Some(operator_id.to_string()),
            operator_name: Some(operator_name.to_string()),
            customer_id: cart.customer_id.clone(),
            customer_name: cart.customer_name.clone(),
            currency: "LYD".to_string(), // TODO: Get from config
            notes: cart.note.clone(),
            discount_percent: if cart.cart_discount_percent > Decimal::ZERO {
                Some(cart.cart_discount_percent.to_f64().unwrap_or(0.0))
            } else {
                None
            },
            discount_amount: if cart.cart_discount_amount > Decimal::ZERO {
                Some(cart.cart_discount_amount.to_f64().unwrap_or(0.0))
            } else {
                None
            },
            items,
        };

        // Try to create on backend
        match self.api.create_cart(&request).await {
            Ok(response) => {
                // Success - cache locally and return
                let draft = self.cache_from_response(&response, cart, false)?;
                info!("Created shared draft with token: {}", draft.token);
                Ok(draft)
            }
            Err(e) => {
                // Offline or error - create locally with temp token
                warn!(
                    "Failed to create shared draft online: {}, creating locally",
                    e
                );
                self.create_local_draft(cart, name, operator_id, operator_name, &request)
            }
        }
    }

    /// Creates a local-only draft when offline
    fn create_local_draft(
        &self,
        cart: &Cart,
        name: Option<String>,
        operator_id: &str,
        operator_name: &str,
        request: &CreateCartRequest,
    ) -> SharedDraftResult<SharedDraft> {
        let local_id = Uuid::new_v4().to_string();
        let temp_token = format!("L{}", local_id[..5].to_uppercase());
        let now = Utc::now().to_rfc3339();

        // Create shared draft row
        let items_json = serde_json::to_string(&cart.items)
            .map_err(|e| SharedDraftError::SerializationError(e.to_string()))?;

        let discount_json = if cart.cart_discount_percent > Decimal::ZERO
            || cart.cart_discount_amount > Decimal::ZERO
        {
            Some(
                serde_json::json!({
                    "percent": cart.cart_discount_percent.to_f64().unwrap_or(0.0),
                    "amount": cart.cart_discount_amount.to_f64().unwrap_or(0.0),
                })
                .to_string(),
            )
        } else {
            None
        };

        let draft_row = SharedDraftRow {
            id: local_id.clone(),
            token: temp_token.clone(),
            name: name.clone(),
            items_json,
            customer_id: cart.customer_id.clone(),
            customer_name: cart.customer_name.clone(),
            discount_json,
            notes: cart.note.clone(),
            item_count: cart.line_count() as i32,
            total_amount: cart.grand_total.to_f64().unwrap_or(0.0),
            currency: "LYD".to_string(),
            warehouse_id: self.warehouse_id.clone(),
            device_id: Some(self.device_id.clone()),
            operator_id: Some(operator_id.to_string()),
            operator_name: Some(operator_name.to_string()),
            created_at: now.clone(),
            expires_at: None,
            fetched_at: now.clone(),
            sync_status: "PENDING".to_string(),
        };

        // Save to cache
        self.db
            .save_shared_draft(&draft_row)
            .map_err(|e| SharedDraftError::DatabaseError(e.to_string()))?;

        // Queue for sync
        let payload = serde_json::to_string(request)
            .map_err(|e| SharedDraftError::SerializationError(e.to_string()))?;

        let queue_item =
            DraftSyncQueueItem::new_create(&Uuid::new_v4().to_string(), &local_id, &payload);

        self.db
            .queue_draft_sync(&queue_item)
            .map_err(|e| SharedDraftError::DatabaseError(e.to_string()))?;

        info!("Created local draft with temp token: {}", temp_token);

        Ok(SharedDraft {
            id: local_id,
            token: temp_token,
            name,
            cart: cart.clone(),
            item_count: cart.line_count(),
            total_amount: cart.grand_total.to_f64().unwrap_or(0.0),
            currency: "LYD".to_string(),
            warehouse_id: self.warehouse_id.clone(),
            device_id: Some(self.device_id.clone()),
            operator_id: Some(operator_id.to_string()),
            operator_name: Some(operator_name.to_string()),
            created_at: now,
            expires_at: None,
            is_local_only: true,
        })
    }

    /// Lists shared drafts for the current warehouse
    ///
    /// When online: Fetches from backend, updates cache
    /// When offline: Returns cached drafts
    pub async fn list_shared_drafts(&self) -> SharedDraftResult<Vec<SharedDraft>> {
        // Try to fetch from backend
        match self.api.list_pos_carts(&self.warehouse_id).await {
            Ok(response) => {
                // Update cache with fetched data
                let now = Utc::now().to_rfc3339();
                let mut drafts = Vec::new();

                for cart_response in response.carts {
                    let items_json = serde_json::to_string(&cart_response.items)
                        .unwrap_or_else(|_| "[]".to_string());

                    let discount_json = if cart_response.discount_percent.unwrap_or(0.0) > 0.0
                        || cart_response.discount_amount.unwrap_or(0.0) > 0.0
                    {
                        Some(
                            serde_json::json!({
                                "percent": cart_response.discount_percent.unwrap_or(0.0),
                                "amount": cart_response.discount_amount.unwrap_or(0.0),
                            })
                            .to_string(),
                        )
                    } else {
                        None
                    };

                    let draft_row = SharedDraftRow {
                        id: cart_response.id.clone(),
                        token: cart_response.token.clone(),
                        name: cart_response.name.clone(),
                        items_json: items_json.clone(),
                        customer_id: cart_response.customer_id.clone(),
                        customer_name: cart_response.customer_name.clone(),
                        discount_json,
                        notes: cart_response.notes.clone(),
                        item_count: cart_response.item_count,
                        total_amount: cart_response.total_amount,
                        currency: cart_response.currency.clone(),
                        warehouse_id: cart_response.warehouse_id.clone(),
                        device_id: cart_response.device_id.clone(),
                        operator_id: cart_response.operator_id.clone(),
                        operator_name: cart_response.operator_name.clone(),
                        created_at: cart_response.created_at.clone(),
                        expires_at: cart_response.expires_at.clone(),
                        fetched_at: now.clone(),
                        sync_status: "SYNCED".to_string(),
                    };

                    // Cache for offline access
                    let _ = self.db.save_shared_draft(&draft_row);

                    drafts.push(self.draft_from_row(&draft_row)?);
                }

                debug!("Listed {} shared drafts from backend", drafts.len());

                // Also include pending local drafts
                let local_drafts = self.get_pending_local_drafts()?;
                drafts.extend(local_drafts);

                Ok(drafts)
            }
            Err(e) => {
                // Offline - return cached
                warn!("Failed to fetch drafts from backend: {}, using cache", e);
                self.list_cached_drafts()
            }
        }
    }

    /// Gets pending local drafts not yet synced
    fn get_pending_local_drafts(&self) -> SharedDraftResult<Vec<SharedDraft>> {
        let rows = self
            .db
            .list_shared_drafts(&self.warehouse_id)
            .map_err(|e| SharedDraftError::DatabaseError(e.to_string()))?;

        rows.into_iter()
            .filter(|r| r.sync_status == "PENDING")
            .map(|r| self.draft_from_row(&r))
            .collect()
    }

    /// Lists drafts from local cache
    fn list_cached_drafts(&self) -> SharedDraftResult<Vec<SharedDraft>> {
        let rows = self
            .db
            .list_shared_drafts(&self.warehouse_id)
            .map_err(|e| SharedDraftError::DatabaseError(e.to_string()))?;

        rows.into_iter().map(|r| self.draft_from_row(&r)).collect()
    }

    /// Recalls a draft by its token
    ///
    /// When online: Fetches from backend
    /// When offline: Checks local cache
    ///
    /// # Arguments
    ///
    /// * `token` - The 6-character token
    ///
    /// # Returns
    ///
    /// The cart contents
    pub async fn recall_by_token(&self, token: &str) -> SharedDraftResult<Cart> {
        let token = token.trim().to_uppercase();

        // Validate token format
        if token.len() < 5 || token.len() > 8 {
            return Err(SharedDraftError::InvalidToken(token));
        }

        // Check if it's a local token
        if SharedDraft::is_local_token(&token) {
            return self.recall_local_draft(&token);
        }

        // Try to fetch from backend
        match self.api.get_cart_by_token(&token).await {
            Ok(response) => {
                let cart = self.cart_from_response(&response)?;
                info!("Recalled draft {} from backend", token);
                Ok(cart)
            }
            Err(e) => {
                // Try local cache
                warn!("Failed to recall from backend: {}, trying cache", e);
                self.recall_from_cache(&token)
            }
        }
    }

    /// Recalls a local-only draft
    fn recall_local_draft(&self, token: &str) -> SharedDraftResult<Cart> {
        let row = self
            .db
            .get_shared_draft_by_token(token)
            .map_err(|e| SharedDraftError::DatabaseError(e.to_string()))?
            .ok_or_else(|| SharedDraftError::NotFound(token.to_string()))?;

        self.cart_from_row(&row)
    }

    /// Recalls from local cache
    fn recall_from_cache(&self, token: &str) -> SharedDraftResult<Cart> {
        let row = self
            .db
            .get_shared_draft_by_token(token)
            .map_err(|e| SharedDraftError::DatabaseError(e.to_string()))?
            .ok_or_else(|| SharedDraftError::NotFound(token.to_string()))?;

        self.cart_from_row(&row)
    }

    /// Recalls a draft and marks it as converted
    ///
    /// Call this when completing a transaction from a recalled draft.
    ///
    /// # Arguments
    ///
    /// * `draft_id` - The draft/cart ID
    /// * `transaction_id` - The completed transaction ID
    ///
    /// # Returns
    ///
    /// The cart contents
    pub async fn recall_and_convert(
        &self,
        draft_id: &str,
        transaction_id: &str,
    ) -> SharedDraftResult<Cart> {
        // Get the draft first
        let row = self
            .db
            .get_shared_draft_by_id(draft_id)
            .map_err(|e| SharedDraftError::DatabaseError(e.to_string()))?
            .ok_or_else(|| SharedDraftError::NotFound(draft_id.to_string()))?;

        let cart = self.cart_from_row(&row)?;

        // Try to mark as converted on backend
        if row.sync_status == "SYNCED" {
            match self.api.convert_cart(draft_id, transaction_id).await {
                Ok(_) => {
                    // Remove from cache
                    let _ = self.db.delete_shared_draft(draft_id);
                    info!("Marked draft {} as converted", draft_id);
                }
                Err(e) => {
                    // Queue for later sync
                    warn!("Failed to convert on backend: {}, queueing", e);
                    self.queue_convert(draft_id, transaction_id)?;
                }
            }
        } else {
            // Local draft - queue convert operation
            self.queue_convert(draft_id, transaction_id)?;
        }

        Ok(cart)
    }

    /// Queues a convert operation for later sync
    fn queue_convert(&self, draft_id: &str, transaction_id: &str) -> SharedDraftResult<()> {
        let queue_item = DraftSyncQueueItem::new_convert(
            &Uuid::new_v4().to_string(),
            draft_id,
            draft_id,
            transaction_id,
        );

        self.db
            .queue_draft_sync(&queue_item)
            .map_err(|e| SharedDraftError::DatabaseError(e.to_string()))?;

        // Update local status
        let _ = self
            .db
            .update_shared_draft_status(draft_id, SharedDraftSyncStatus::PendingConvert);

        Ok(())
    }

    /// Deletes/cancels a shared draft
    ///
    /// # Arguments
    ///
    /// * `draft_id` - The draft/cart ID
    pub async fn delete_draft(&self, draft_id: &str) -> SharedDraftResult<bool> {
        // Get the draft to check sync status
        let row = self
            .db
            .get_shared_draft_by_id(draft_id)
            .map_err(|e| SharedDraftError::DatabaseError(e.to_string()))?;

        if let Some(row) = row {
            if row.sync_status == "SYNCED" {
                // Try to delete on backend
                match self.api.delete_cart(draft_id).await {
                    Ok(_) => {
                        let _ = self.db.delete_shared_draft(draft_id);
                        info!("Deleted draft {} from backend", draft_id);
                        return Ok(true);
                    }
                    Err(e) => {
                        warn!("Failed to delete on backend: {}, queueing", e);
                        self.queue_delete(draft_id)?;
                    }
                }
            } else {
                // Local draft - just delete it and queue operations
                self.queue_delete(draft_id)?;
            }

            // Remove from local cache
            self.db
                .delete_shared_draft(draft_id)
                .map_err(|e| SharedDraftError::DatabaseError(e.to_string()))?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Queues a delete operation for later sync
    fn queue_delete(&self, draft_id: &str) -> SharedDraftResult<()> {
        let queue_item =
            DraftSyncQueueItem::new_delete(&Uuid::new_v4().to_string(), draft_id, draft_id);

        self.db
            .queue_draft_sync(&queue_item)
            .map_err(|e| SharedDraftError::DatabaseError(e.to_string()))?;

        Ok(())
    }

    /// Processes the draft sync queue
    ///
    /// Called during sync cycles to sync pending operations.
    pub async fn process_sync_queue(&self) -> SharedDraftResult<SyncQueueResult> {
        let mut result = SyncQueueResult::default();

        // Get pending items
        let items = self
            .db
            .get_pending_draft_syncs(10)
            .map_err(|e| SharedDraftError::DatabaseError(e.to_string()))?;

        if items.is_empty() {
            return Ok(result);
        }

        debug!("Processing {} draft sync queue items", items.len());

        for item in items {
            let _ = self.db.mark_draft_sync_syncing(&item.id);

            // Previously the operation column was parsed with a silent fallback to `Create`,
            // so a queue row with an unrecognised operation was executed as a cart create.
            let operation = match item.operation_type() {
                Ok(operation) => operation,
                Err(e) => {
                    error!(
                        "Draft sync item {} has an unreadable operation: {e}",
                        item.id
                    );
                    let _ = self.db.mark_draft_sync_failed(&item.id, &e.to_string());
                    result.failed += 1;
                    continue;
                }
            };

            match operation {
                DraftSyncOperation::Create => match self.process_create_sync(&item).await {
                    Ok(_) => result.synced += 1,
                    Err(e) => {
                        let _ = self.db.mark_draft_sync_failed(&item.id, &e.to_string());
                        result.failed += 1;
                    }
                },
                DraftSyncOperation::Convert => match self.process_convert_sync(&item).await {
                    Ok(_) => result.synced += 1,
                    Err(e) => {
                        let _ = self.db.mark_draft_sync_failed(&item.id, &e.to_string());
                        result.failed += 1;
                    }
                },
                DraftSyncOperation::Delete => match self.process_delete_sync(&item).await {
                    Ok(_) => result.synced += 1,
                    Err(e) => {
                        let _ = self.db.mark_draft_sync_failed(&item.id, &e.to_string());
                        result.failed += 1;
                    }
                },
            }
        }

        // Cleanup completed items
        let _ = self.db.cleanup_completed_draft_syncs();

        info!(
            "Draft sync queue processed: {} synced, {} failed",
            result.synced, result.failed
        );

        Ok(result)
    }

    /// Processes a CREATE sync operation
    async fn process_create_sync(&self, item: &DraftSyncQueueItem) -> Result<()> {
        let request: CreateCartRequest = serde_json::from_str(&item.payload_json)?;

        let response = self.api.create_cart(&request).await?;

        // Update local draft with server info
        if let Ok(Some(mut row)) = self.db.get_shared_draft_by_id(&item.local_draft_id) {
            row.id = response.id.clone();
            row.token = response.token.clone();
            row.sync_status = "SYNCED".to_string();
            let _ = self.db.save_shared_draft(&row);
        }

        // Mark queue item complete
        self.db
            .mark_draft_sync_complete(&item.id, Some(&response.id), Some(&response.token))
            .map_err(|e| anyhow!(e.to_string()))?;

        info!("Synced create draft, new token: {}", response.token);
        Ok(())
    }

    /// Processes a CONVERT sync operation
    async fn process_convert_sync(&self, item: &DraftSyncQueueItem) -> Result<()> {
        let server_id = item
            .server_id
            .as_ref()
            .ok_or_else(|| anyhow!("Missing server_id for convert"))?;
        let transaction_id = item
            .transaction_id
            .as_ref()
            .ok_or_else(|| anyhow!("Missing transaction_id for convert"))?;

        self.api.convert_cart(server_id, transaction_id).await?;

        // Remove from local cache
        let _ = self.db.delete_shared_draft(&item.local_draft_id);

        // Mark complete
        self.db
            .mark_draft_sync_complete(&item.id, None, None)
            .map_err(|e| anyhow!(e.to_string()))?;

        info!("Synced convert draft {}", server_id);
        Ok(())
    }

    /// Processes a DELETE sync operation
    async fn process_delete_sync(&self, item: &DraftSyncQueueItem) -> Result<()> {
        if let Some(server_id) = &item.server_id {
            // Try to delete on backend (may already be deleted)
            match self.api.delete_cart(server_id).await {
                Ok(_) => {}
                Err(e) => {
                    // If 404, it's already deleted - that's fine
                    if !e.to_string().contains("404") {
                        return Err(e);
                    }
                }
            }
        }

        // Mark complete
        self.db
            .mark_draft_sync_complete(&item.id, None, None)
            .map_err(|e| anyhow!(e.to_string()))?;

        info!("Synced delete draft");
        Ok(())
    }

    /// Gets the count of pending sync operations
    pub fn pending_sync_count(&self) -> SharedDraftResult<i64> {
        self.db
            .count_pending_draft_syncs()
            .map_err(|e| SharedDraftError::DatabaseError(e.to_string()))
    }

    // ========================================================================
    // Helper Methods
    // ========================================================================

    /// Creates a SharedDraft from API response
    fn cache_from_response(
        &self,
        response: &CartResponse,
        original_cart: &Cart,
        is_local: bool,
    ) -> SharedDraftResult<SharedDraft> {
        let now = Utc::now().to_rfc3339();

        // Create cache row
        let items_json = serde_json::to_string(&original_cart.items)
            .map_err(|e| SharedDraftError::SerializationError(e.to_string()))?;

        let discount_json = if original_cart.cart_discount_percent > Decimal::ZERO
            || original_cart.cart_discount_amount > Decimal::ZERO
        {
            Some(
                serde_json::json!({
                    "percent": original_cart.cart_discount_percent.to_f64().unwrap_or(0.0),
                    "amount": original_cart.cart_discount_amount.to_f64().unwrap_or(0.0),
                })
                .to_string(),
            )
        } else {
            None
        };

        let row = SharedDraftRow {
            id: response.id.clone(),
            token: response.token.clone(),
            name: response.name.clone(),
            items_json,
            customer_id: original_cart.customer_id.clone(),
            customer_name: original_cart.customer_name.clone(),
            discount_json,
            notes: original_cart.note.clone(),
            item_count: original_cart.line_count() as i32,
            total_amount: original_cart.grand_total.to_f64().unwrap_or(0.0),
            currency: response.currency.clone(),
            warehouse_id: response.warehouse_id.clone(),
            device_id: response.device_id.clone(),
            operator_id: response.operator_id.clone(),
            operator_name: response.operator_name.clone(),
            created_at: response.created_at.clone(),
            expires_at: response.expires_at.clone(),
            fetched_at: now,
            sync_status: if is_local {
                "PENDING".to_string()
            } else {
                "SYNCED".to_string()
            },
        };

        self.db
            .save_shared_draft(&row)
            .map_err(|e| SharedDraftError::DatabaseError(e.to_string()))?;

        Ok(SharedDraft {
            id: response.id.clone(),
            token: response.token.clone(),
            name: response.name.clone(),
            cart: original_cart.clone(),
            item_count: original_cart.line_count(),
            total_amount: original_cart.grand_total.to_f64().unwrap_or(0.0),
            currency: response.currency.clone(),
            warehouse_id: response.warehouse_id.clone(),
            device_id: response.device_id.clone(),
            operator_id: response.operator_id.clone(),
            operator_name: response.operator_name.clone(),
            created_at: response.created_at.clone(),
            expires_at: response.expires_at.clone(),
            is_local_only: is_local,
        })
    }

    /// Creates a Cart from API response
    fn cart_from_response(&self, response: &CartResponse) -> SharedDraftResult<Cart> {
        let mut cart = Cart::new();

        // Convert items
        for item_dto in &response.items {
            let item = CartItem {
                id: Uuid::new_v4().to_string(),
                product_id: item_dto.product_id.clone(),
                product_name: item_dto.product_name.clone(),
                product_name_ar: item_dto.product_name_ar.clone(),
                sku: item_dto.product_sku.clone(),
                barcode: item_dto.barcode.clone(),
                quantity: Decimal::from_f64(item_dto.quantity).unwrap_or(Decimal::ZERO),
                unit: pos_models::product::ProductUnit::Unit, // Default
                unit_price: Decimal::from_f64(item_dto.unit_price).unwrap_or(Decimal::ZERO),
                tax_rate: Decimal::from_f64(item_dto.tax_rate).unwrap_or(Decimal::ZERO),
                tax_inclusive: item_dto.tax_inclusive,
                discount_amount: Decimal::from_f64(item_dto.discount_amount.unwrap_or(0.0))
                    .unwrap_or(Decimal::ZERO),
                discount_percent: Decimal::from_f64(item_dto.discount_percent.unwrap_or(0.0))
                    .unwrap_or(Decimal::ZERO),
                note: item_dto.note.clone(),
                product_type: pos_models::product::ProductType::default(),
                track_inventory: true,
                line_subtotal: Decimal::ZERO,
                line_tax: Decimal::ZERO,
                line_total: Decimal::ZERO,
                line_discount: Decimal::ZERO,
            };
            cart.items.push(item);
        }

        // Set cart-level fields
        cart.customer_id = response.customer_id.clone();
        cart.customer_name = response.customer_name.clone();
        cart.note = response.notes.clone();
        cart.cart_discount_percent =
            Decimal::from_f64(response.discount_percent.unwrap_or(0.0)).unwrap_or(Decimal::ZERO);
        cart.cart_discount_amount =
            Decimal::from_f64(response.discount_amount.unwrap_or(0.0)).unwrap_or(Decimal::ZERO);

        // Recalculate
        for item in &mut cart.items {
            item.recalculate();
        }
        cart.recalculate();

        Ok(cart)
    }

    /// Creates a Cart from a database row
    fn cart_from_row(&self, row: &SharedDraftRow) -> SharedDraftResult<Cart> {
        let items: Vec<CartItem> = serde_json::from_str(&row.items_json)
            .map_err(|e| SharedDraftError::DeserializationError(e.to_string()))?;

        let mut cart = Cart::new();
        cart.items = items;
        cart.customer_id = row.customer_id.clone();
        cart.customer_name = row.customer_name.clone();
        cart.note = row.notes.clone();

        // Parse discount
        if let Some(ref discount_json) = row.discount_json {
            if let Ok(discount) = serde_json::from_str::<serde_json::Value>(discount_json) {
                if let Some(percent) = discount.get("percent").and_then(|v| v.as_f64()) {
                    cart.cart_discount_percent =
                        Decimal::from_f64(percent).unwrap_or(Decimal::ZERO);
                }
                if let Some(amount) = discount.get("amount").and_then(|v| v.as_f64()) {
                    cart.cart_discount_amount = Decimal::from_f64(amount).unwrap_or(Decimal::ZERO);
                }
            }
        }

        cart.recalculate();
        Ok(cart)
    }

    /// Creates a SharedDraft from a database row
    fn draft_from_row(&self, row: &SharedDraftRow) -> SharedDraftResult<SharedDraft> {
        let cart = self.cart_from_row(row)?;

        Ok(SharedDraft {
            id: row.id.clone(),
            token: row.token.clone(),
            name: row.name.clone(),
            cart,
            item_count: row.item_count as usize,
            total_amount: row.total_amount,
            currency: row.currency.clone(),
            warehouse_id: row.warehouse_id.clone(),
            device_id: row.device_id.clone(),
            operator_id: row.operator_id.clone(),
            operator_name: row.operator_name.clone(),
            created_at: row.created_at.clone(),
            expires_at: row.expires_at.clone(),
            is_local_only: row.sync_status == "PENDING",
        })
    }

    // ========================================================================
    // Parking API Methods (new parking endpoints)
    // ========================================================================

    /// Parks the current cart using the new parking API
    ///
    /// This is the preferred method for parking carts. It uses the new
    /// `/api/pos/parking` endpoint which provides:
    /// - POS tokens (P001, P002...) for easy verbal communication
    /// - Soft lock on recall to prevent conflicts
    /// - Warehouse-scoped visibility
    ///
    /// # Arguments
    ///
    /// * `cart` - The cart to park
    /// * `display_name` - Optional display name (e.g., "Blue shirt customer")
    /// * `operator_id` - Current operator ID
    /// * `operator_name` - Current operator name
    ///
    /// # Returns
    ///
    /// The parked cart response with POS token and items
    pub async fn park_cart(
        &self,
        cart: &Cart,
        display_name: Option<String>,
        operator_id: &str,
        operator_name: &str,
    ) -> SharedDraftResult<ParkedCartDetailResponse> {
        if cart.is_empty() {
            return Err(SharedDraftError::EmptyCart);
        }

        // Build parking items
        let items: Vec<ParkingCartItemDto> = cart
            .items
            .iter()
            .map(|item| ParkingCartItemDto {
                product_id: item.product_id.clone(),
                product_sku: item.sku.clone(),
                product_name: item.product_name.clone(),
                product_name_ar: item.product_name_ar.clone(),
                barcode: item.barcode.clone(),
                quantity: item.quantity.to_f64().unwrap_or(0.0),
                unit_price: item.unit_price.to_f64().unwrap_or(0.0),
                tax_rate: if item.tax_rate > Decimal::ZERO {
                    Some(item.tax_rate.to_f64().unwrap_or(0.0))
                } else {
                    None
                },
                discount_percent: if item.discount_percent > Decimal::ZERO {
                    Some(item.discount_percent.to_f64().unwrap_or(0.0))
                } else {
                    None
                },
                discount_amount: if item.discount_amount > Decimal::ZERO {
                    Some(item.discount_amount.to_f64().unwrap_or(0.0))
                } else {
                    None
                },
                notes: item.note.clone(),
            })
            .collect();

        let request = CreateAndParkCartRequest {
            warehouse_id: self.warehouse_id.clone(),
            display_name,
            operator_id: operator_id.to_string(),
            operator_name: operator_name.to_string(),
            terminal_id: self.device_id.clone(),
            customer_id: cart.customer_id.clone(),
            customer_name: cart.customer_name.clone(),
            customer_phone: None, // Cart doesn't have phone
            currency: "LYD".to_string(),
            notes: cart.note.clone(),
            items,
            expiry_minutes: Some(240), // 4 hours default
        };

        match self.api.create_and_park_cart(&request).await {
            Ok(response) => {
                info!("Cart parked with POS token: {}", response.pos_token);
                Ok(response)
            }
            Err(e) => {
                error!("Failed to park cart: {}", e);
                Err(SharedDraftError::ApiError(e.to_string()))
            }
        }
    }

    /// Lists parked carts using the new parking API
    ///
    /// Returns all parked and recalled carts for the warehouse.
    pub async fn list_parked_carts(&self) -> SharedDraftResult<Vec<ParkedCartResponse>> {
        match self
            .api
            .list_parked_carts(&self.warehouse_id, None, false)
            .await
        {
            Ok(carts) => {
                debug!("Listed {} parked carts", carts.len());
                Ok(carts)
            }
            Err(e) => {
                warn!("Failed to list parked carts: {}", e);
                Err(SharedDraftError::ApiError(e.to_string()))
            }
        }
    }

    /// Recalls a parked cart by POS token (P001, P002...)
    ///
    /// Sets the cart to RECALLED status with soft lock.
    ///
    /// # Arguments
    ///
    /// * `pos_token` - The POS token (e.g., "P001")
    /// * `operator_id` - Current operator ID
    /// * `operator_name` - Current operator name
    /// * `force` - Force recall even if locked by another terminal
    ///
    /// # Returns
    ///
    /// The recalled cart with items, converted to a Cart
    pub async fn recall_parked_cart(
        &self,
        pos_token: &str,
        operator_id: &str,
        operator_name: &str,
        force: bool,
    ) -> SharedDraftResult<Cart> {
        let request = RecallCartRequest {
            operator_id: operator_id.to_string(),
            operator_name: operator_name.to_string(),
            terminal_id: self.device_id.clone(),
            force,
        };

        match self
            .api
            .recall_parked_cart_by_token(pos_token, &request)
            .await
        {
            Ok(response) => {
                let cart = self.cart_from_parked_response(&response)?;
                info!("Recalled parked cart {}", pos_token);
                Ok(cart)
            }
            Err(e) => {
                error!("Failed to recall parked cart {}: {}", pos_token, e);
                Err(SharedDraftError::ApiError(e.to_string()))
            }
        }
    }

    /// Recalls a parked cart by ID
    pub async fn recall_parked_cart_by_id(
        &self,
        cart_id: &str,
        operator_id: &str,
        operator_name: &str,
        force: bool,
    ) -> SharedDraftResult<Cart> {
        let request = RecallCartRequest {
            operator_id: operator_id.to_string(),
            operator_name: operator_name.to_string(),
            terminal_id: self.device_id.clone(),
            force,
        };

        match self.api.recall_parked_cart(cart_id, &request).await {
            Ok(response) => {
                let cart = self.cart_from_parked_response(&response)?;
                info!("Recalled parked cart by ID {}", cart_id);
                Ok(cart)
            }
            Err(e) => {
                error!("Failed to recall parked cart {}: {}", cart_id, e);
                Err(SharedDraftError::ApiError(e.to_string()))
            }
        }
    }

    /// Releases a recalled cart back to PARKED status
    ///
    /// Use when operator decides not to complete the cart.
    pub async fn release_parked_cart(&self, cart_id: &str) -> SharedDraftResult<()> {
        match self.api.release_parked_cart(cart_id, &self.device_id).await {
            Ok(_) => {
                info!("Released parked cart {}", cart_id);
                Ok(())
            }
            Err(e) => {
                error!("Failed to release parked cart {}: {}", cart_id, e);
                Err(SharedDraftError::ApiError(e.to_string()))
            }
        }
    }

    /// Voids/cancels a parked cart
    pub async fn void_parked_cart(&self, cart_id: &str) -> SharedDraftResult<()> {
        match self.api.void_parked_cart(cart_id).await {
            Ok(_) => {
                info!("Voided parked cart {}", cart_id);
                Ok(())
            }
            Err(e) => {
                error!("Failed to void parked cart {}: {}", cart_id, e);
                Err(SharedDraftError::ApiError(e.to_string()))
            }
        }
    }

    /// Converts a parked cart detail response to a Cart
    fn cart_from_parked_response(
        &self,
        response: &ParkedCartDetailResponse,
    ) -> SharedDraftResult<Cart> {
        let mut cart = Cart::new();

        for item in &response.items {
            let cart_item = CartItem {
                id: item.id.clone(),
                product_id: item.product_id.clone(),
                product_name: item.product_name.clone(),
                product_name_ar: item.product_name_ar.clone(),
                sku: item.product_sku.clone(),
                barcode: item.barcode.clone(),
                quantity: Decimal::from_f64(item.quantity).unwrap_or(Decimal::ZERO),
                unit: pos_models::product::ProductUnit::Unit,
                unit_price: Decimal::from_f64(item.unit_price).unwrap_or(Decimal::ZERO),
                tax_rate: Decimal::from_f64(item.tax_rate.unwrap_or(0.0)).unwrap_or(Decimal::ZERO),
                tax_inclusive: false, // Assume tax-exclusive for now
                discount_amount: Decimal::from_f64(item.discount_amount).unwrap_or(Decimal::ZERO),
                discount_percent: Decimal::from_f64(item.discount_percent.unwrap_or(0.0))
                    .unwrap_or(Decimal::ZERO),
                note: item.notes.clone(),
                line_subtotal: Decimal::from_f64(item.line_subtotal).unwrap_or(Decimal::ZERO),
                line_tax: Decimal::from_f64(item.tax_amount).unwrap_or(Decimal::ZERO),
                line_total: Decimal::from_f64(item.line_total).unwrap_or(Decimal::ZERO),
                line_discount: Decimal::from_f64(item.discount_amount).unwrap_or(Decimal::ZERO),
                product_type: pos_models::product::ProductType::default(),
                track_inventory: true,
            };
            cart.items.push(cart_item);
        }

        cart.customer_id = response.customer_id.clone();
        cart.customer_name = response.customer_name.clone();
        cart.note = response.notes.clone();
        cart.recalculate();

        Ok(cart)
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use pos_models::product::{Product, ProductUnit};
    use rust_decimal::Decimal;

    fn create_test_cart() -> Cart {
        let product = Product {
            id: "prod-001".to_string(),
            sku: "SKU001".to_string(),
            barcode: Some("1234567890".to_string()),
            name: "Test Product".to_string(),
            name_ar: Some("منتج تجريبي".to_string()),
            price: Decimal::from(10),
            tax_rate: Decimal::from(15),
            tax_inclusive: false,
            unit: ProductUnit::Unit,
            ..Default::default()
        };

        let mut cart = Cart::new();
        cart.items.push(CartItem::new(&product, Decimal::from(2)));
        cart.customer_id = Some("cust-001".to_string());
        cart.customer_name = Some("Test Customer".to_string());
        cart.recalculate();
        cart
    }

    #[test]
    fn test_shared_draft_is_local_token() {
        assert!(SharedDraft::is_local_token("L12345"));
        assert!(SharedDraft::is_local_token("LABCDE"));
        assert!(!SharedDraft::is_local_token("A1B2C3"));
        assert!(!SharedDraft::is_local_token("XYZ123"));
    }

    #[test]
    fn test_sync_queue_result() {
        let mut result = SyncQueueResult::default();
        assert!(!result.has_activity());

        result.synced = 1;
        assert!(result.has_activity());
    }

    #[test]
    fn test_shared_draft_display_name() {
        let cart = create_test_cart();
        let draft = SharedDraft {
            id: "id-1".to_string(),
            token: "ABC123".to_string(),
            name: Some("Test Draft".to_string()),
            cart: cart.clone(),
            item_count: 1,
            total_amount: 23.0,
            currency: "LYD".to_string(),
            warehouse_id: "wh-1".to_string(),
            device_id: None,
            operator_id: None,
            operator_name: None,
            created_at: "2025-01-01T00:00:00Z".to_string(),
            expires_at: None,
            is_local_only: false,
        };

        assert_eq!(draft.display_name(), "Test Draft");

        let draft_no_name = SharedDraft {
            name: None,
            ..draft
        };
        assert_eq!(draft_no_name.display_name(), "Shared Draft");
    }
}
