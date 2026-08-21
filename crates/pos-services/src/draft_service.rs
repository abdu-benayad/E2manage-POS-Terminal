//! Draft Service
//!
//! Higher-level service for managing draft/held orders.
//! Wraps database operations and provides business logic for draft management.
//!
//! ## Features
//!
//! - Save cart as draft with optional name
//! - Recall drafts to cart
//! - List drafts by operator or shift
//! - Auto-expiration support
//! - Cart serialization/deserialization
//!
//! ## Usage
//!
//! ```rust,ignore
//! use crate::services::DraftService;
//! use std::sync::Arc;
//!
//! let service = DraftService::new(Arc::new(db));
//!
//! // Save current cart as draft
//! let draft = service.save_cart(&cart, Some("Ahmed's order"), "op-1", "shift-1")?;
//! println!("Saved draft: {} ({})", draft.name, draft.id);
//!
//! // Get all drafts for current shift
//! let drafts = service.list_by_shift("shift-1")?;
//! for d in drafts {
//!     println!("{}: {} items", d.display_name(), d.item_count);
//! }
//!
//! // Recall draft to cart
//! let cart = service.recall("draft-123")?;
//!
//! // Delete draft after use
//! service.delete("draft-123")?;
//! ```

use chrono::{DateTime, Duration, Utc};
use rust_decimal::prelude::*;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use thiserror::Error;
use uuid::Uuid;

use pos_db::drafts::DraftRow;
use pos_db::Database;
use pos_models::cart::{Cart, CartItem};

/// Default expiration time for drafts (24 hours)
pub const DEFAULT_DRAFT_EXPIRY_HOURS: i64 = 24;

/// Maximum number of drafts per operator
pub const MAX_DRAFTS_PER_OPERATOR: usize = 10;

// ============================================================================
// Error Types
// ============================================================================

/// Errors that can occur during draft operations
#[derive(Debug, Error)]
pub enum DraftError {
    #[error("Draft not found: {0}")]
    NotFound(String),

    #[error("Draft has expired: {0}")]
    Expired(String),

    #[error("Cannot save empty cart")]
    EmptyCart,

    #[error("Maximum drafts limit reached ({0})")]
    LimitExceeded(usize),

    #[error("Failed to serialize cart: {0}")]
    SerializationError(String),

    #[error("Failed to deserialize cart: {0}")]
    DeserializationError(String),

    #[error("Database error: {0}")]
    DatabaseError(String),
}

/// Result type for draft operations
pub type DraftResult<T> = Result<T, DraftError>;

// ============================================================================
// Draft Model
// ============================================================================

/// A draft/held order
///
/// Represents a cart that has been saved for later recall.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Draft {
    /// Unique identifier
    pub id: String,
    /// Optional display name (e.g., "Ahmed's order")
    pub name: Option<String>,
    /// The held cart with all items
    pub cart: Cart,
    /// Number of items in the draft
    pub item_count: usize,
    /// Total quantity of all items
    pub total_quantity: f64,
    /// Grand total amount
    pub total_amount: f64,
    /// Customer ID (if associated)
    pub customer_id: Option<String>,
    /// Customer name for display
    pub customer_name: Option<String>,
    /// Operator who created the draft
    pub operator_id: Option<String>,
    /// Shift when draft was created
    pub shift_id: Option<String>,
    /// When the draft was created
    pub created_at: DateTime<Utc>,
    /// When the draft expires (None = never)
    pub expires_at: Option<DateTime<Utc>>,
}

impl Draft {
    /// Creates a new draft from a cart
    pub fn from_cart(
        cart: &Cart,
        name: Option<String>,
        operator_id: Option<String>,
        shift_id: Option<String>,
        expiry_hours: Option<i64>,
    ) -> Self {
        let now = Utc::now();
        let expires_at = expiry_hours.map(|h| now + Duration::hours(h));

        Self {
            id: Uuid::new_v4().to_string(),
            name,
            cart: cart.clone(),
            item_count: cart.line_count(),
            total_quantity: cart.item_count().to_f64().unwrap_or(0.0),
            total_amount: cart.grand_total.to_f64().unwrap_or(0.0),
            customer_id: cart.customer_id.clone(),
            customer_name: cart.customer_name.clone(),
            operator_id,
            shift_id,
            created_at: now,
            expires_at,
        }
    }

    /// Returns the display name (uses auto-generated name if no custom name)
    pub fn display_name(&self) -> &str {
        self.name.as_deref().unwrap_or("Unnamed Draft")
    }

    /// Returns true if the draft has expired
    pub fn is_expired(&self) -> bool {
        if let Some(expires_at) = self.expires_at {
            Utc::now() > expires_at
        } else {
            false
        }
    }

    /// Returns the time until expiration (or None if expired/never expires)
    pub fn time_until_expiry(&self) -> Option<Duration> {
        self.expires_at.map(|expires_at| {
            let now = Utc::now();
            if expires_at > now {
                expires_at - now
            } else {
                Duration::zero()
            }
        })
    }

    /// Returns true if the draft has a customer associated
    pub fn has_customer(&self) -> bool {
        self.customer_id.is_some()
    }

    /// Converts to database row format
    fn to_db_row(&self) -> DraftResult<DraftRow> {
        let items_json = serde_json::to_string(&self.cart.items)
            .map_err(|e| DraftError::SerializationError(e.to_string()))?;

        let discount_json = if self.cart.cart_discount_percent > Decimal::ZERO
            || self.cart.cart_discount_amount > Decimal::ZERO
        {
            Some(
                serde_json::json!({
                    "percent": self.cart.cart_discount_percent.to_f64().unwrap_or(0.0),
                    "amount": self.cart.cart_discount_amount.to_f64().unwrap_or(0.0),
                })
                .to_string(),
            )
        } else {
            None
        };

        Ok(DraftRow {
            id: self.id.clone(),
            name: self.name.clone(),
            items_json,
            customer_id: self.customer_id.clone(),
            customer_name: self.customer_name.clone(),
            discount_json,
            notes: self.cart.note.clone(),
            created_at: self.created_at.to_rfc3339(),
            expires_at: self.expires_at.map(|dt| dt.to_rfc3339()),
            operator_id: self.operator_id.clone(),
            shift_id: self.shift_id.clone(),
        })
    }

    /// Creates from database row
    fn from_db_row(row: DraftRow) -> DraftResult<Self> {
        // Parse items
        let items: Vec<CartItem> = serde_json::from_str(&row.items_json)
            .map_err(|e| DraftError::DeserializationError(e.to_string()))?;

        // Build cart
        let mut cart = Cart::new();
        cart.items = items;
        cart.customer_id = row.customer_id.clone();
        cart.customer_name = row.customer_name.clone();
        cart.note = row.notes.clone();

        // Parse discount
        if let Some(discount_json) = &row.discount_json {
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

        // Parse timestamps
        let created_at = DateTime::parse_from_rfc3339(&row.created_at)
            .map_err(|e| DraftError::DeserializationError(e.to_string()))?
            .with_timezone(&Utc);

        let expires_at = row.expires_at.as_ref().and_then(|s| {
            DateTime::parse_from_rfc3339(s)
                .map(|dt| dt.with_timezone(&Utc))
                .ok()
        });

        Ok(Self {
            id: row.id,
            name: row.name,
            item_count: cart.line_count(),
            total_quantity: cart.item_count().to_f64().unwrap_or(0.0),
            total_amount: cart.grand_total.to_f64().unwrap_or(0.0),
            cart,
            customer_id: row.customer_id,
            customer_name: row.customer_name,
            operator_id: row.operator_id,
            shift_id: row.shift_id,
            created_at,
            expires_at,
        })
    }
}

// ============================================================================
// Draft Service
// ============================================================================

/// Service for managing draft/held orders
pub struct DraftService {
    db: Arc<Database>,
}

impl DraftService {
    /// Creates a new draft service
    pub fn new(db: Arc<Database>) -> Self {
        Self { db }
    }

    /// Saves a cart as a draft
    ///
    /// # Arguments
    /// * `cart` - The cart to save
    /// * `name` - Optional display name for the draft
    /// * `operator_id` - ID of the operator creating the draft
    /// * `shift_id` - ID of the current shift
    ///
    /// # Returns
    /// The created draft
    pub fn save_cart(
        &self,
        cart: &Cart,
        name: Option<String>,
        operator_id: &str,
        shift_id: &str,
    ) -> DraftResult<Draft> {
        self.save_cart_with_expiry(
            cart,
            name,
            operator_id,
            shift_id,
            Some(DEFAULT_DRAFT_EXPIRY_HOURS),
        )
    }

    /// Saves a cart as a draft with custom expiry
    pub fn save_cart_with_expiry(
        &self,
        cart: &Cart,
        name: Option<String>,
        operator_id: &str,
        shift_id: &str,
        expiry_hours: Option<i64>,
    ) -> DraftResult<Draft> {
        // Validate cart is not empty
        if cart.is_empty() {
            return Err(DraftError::EmptyCart);
        }

        // Check draft limit for operator
        let existing_drafts = self.list_by_operator(operator_id)?;
        if existing_drafts.len() >= MAX_DRAFTS_PER_OPERATOR {
            return Err(DraftError::LimitExceeded(MAX_DRAFTS_PER_OPERATOR));
        }

        // Create draft
        let draft = Draft::from_cart(
            cart,
            name,
            Some(operator_id.to_string()),
            Some(shift_id.to_string()),
            expiry_hours,
        );

        // Save to database
        let db_row = draft.to_db_row()?;
        self.db
            .save_draft(&db_row)
            .map_err(|e| DraftError::DatabaseError(e.to_string()))?;

        Ok(draft)
    }

    /// Recalls a draft by ID
    ///
    /// Returns the cart from the draft. Does NOT delete the draft.
    pub fn recall(&self, draft_id: &str) -> DraftResult<Cart> {
        let draft = self.get(draft_id)?;

        if draft.is_expired() {
            return Err(DraftError::Expired(draft_id.to_string()));
        }

        Ok(draft.cart)
    }

    /// Recalls and deletes a draft
    ///
    /// Atomically recalls the cart and removes the draft.
    pub fn recall_and_delete(&self, draft_id: &str) -> DraftResult<Cart> {
        let cart = self.recall(draft_id)?;
        self.delete(draft_id)?;
        Ok(cart)
    }

    /// Gets a draft by ID
    pub fn get(&self, draft_id: &str) -> DraftResult<Draft> {
        let row = self
            .db
            .get_draft_by_id(draft_id)
            .map_err(|e| DraftError::DatabaseError(e.to_string()))?
            .ok_or_else(|| DraftError::NotFound(draft_id.to_string()))?;

        Draft::from_db_row(row)
    }

    /// Lists all active (non-expired) drafts
    pub fn list_active(&self) -> DraftResult<Vec<Draft>> {
        let rows = self
            .db
            .get_active_drafts()
            .map_err(|e| DraftError::DatabaseError(e.to_string()))?;

        rows.into_iter().map(Draft::from_db_row).collect()
    }

    /// Lists drafts for a specific operator
    pub fn list_by_operator(&self, operator_id: &str) -> DraftResult<Vec<Draft>> {
        let rows = self
            .db
            .get_drafts_by_operator(operator_id)
            .map_err(|e| DraftError::DatabaseError(e.to_string()))?;

        rows.into_iter().map(Draft::from_db_row).collect()
    }

    /// Lists drafts for a specific shift
    pub fn list_by_shift(&self, shift_id: &str) -> DraftResult<Vec<Draft>> {
        let rows = self
            .db
            .get_drafts_by_shift(shift_id)
            .map_err(|e| DraftError::DatabaseError(e.to_string()))?;

        rows.into_iter().map(Draft::from_db_row).collect()
    }

    /// Counts active drafts
    pub fn count_active(&self) -> DraftResult<i64> {
        self.db
            .get_draft_count()
            .map_err(|e| DraftError::DatabaseError(e.to_string()))
    }

    /// Counts drafts for an operator
    pub fn count_by_operator(&self, operator_id: &str) -> DraftResult<usize> {
        Ok(self.list_by_operator(operator_id)?.len())
    }

    /// Deletes a draft
    pub fn delete(&self, draft_id: &str) -> DraftResult<bool> {
        self.db
            .delete_draft(draft_id)
            .map_err(|e| DraftError::DatabaseError(e.to_string()))
    }

    /// Deletes all expired drafts
    pub fn cleanup_expired(&self) -> DraftResult<usize> {
        self.db
            .cleanup_expired_drafts()
            .map_err(|e| DraftError::DatabaseError(e.to_string()))
    }

    /// Updates a draft's name
    pub fn rename(&self, draft_id: &str, new_name: Option<String>) -> DraftResult<Draft> {
        let mut draft = self.get(draft_id)?;
        draft.name = new_name;

        let db_row = draft.to_db_row()?;
        self.db
            .save_draft(&db_row)
            .map_err(|e| DraftError::DatabaseError(e.to_string()))?;

        Ok(draft)
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use pos_db::migrations::run_migrations;
    use pos_models::product::{Product, ProductUnit};
    use rust_decimal::Decimal;

    fn setup_db() -> Database {
        let db = Database::in_memory().unwrap();
        {
            let conn = db.connection();
            let conn = conn.lock();
            run_migrations(&conn).unwrap();
        }
        db
    }

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

    fn create_operator(db: &Database, id: &str) {
        use pos_db::operators::OperatorRow;
        let op = OperatorRow {
            id: id.to_string(),
            code: format!("C{}", id),
            name: "Test Operator".to_string(),
            pin_hash: "hash".to_string(),
            ..Default::default()
        };
        db.save_operator(&op).unwrap();
    }

    #[test]
    fn test_save_cart_creates_draft() {
        let db = Arc::new(setup_db());
        create_operator(&db, "op-1");
        let service = DraftService::new(db);

        let cart = create_test_cart();
        let draft = service
            .save_cart(&cart, Some("Test Draft".to_string()), "op-1", "shift-1")
            .unwrap();

        assert_eq!(draft.name, Some("Test Draft".to_string()));
        assert_eq!(draft.item_count, 1);
        assert_eq!(draft.customer_name, Some("Test Customer".to_string()));
        assert!(!draft.id.is_empty());
    }

    #[test]
    fn test_save_empty_cart_fails() {
        let db = Arc::new(setup_db());
        let service = DraftService::new(db);

        let cart = Cart::new();
        let result = service.save_cart(&cart, None, "op-1", "shift-1");

        assert!(matches!(result, Err(DraftError::EmptyCart)));
    }

    #[test]
    fn test_recall_draft() {
        let db = Arc::new(setup_db());
        create_operator(&db, "op-1");
        let service = DraftService::new(db);

        let cart = create_test_cart();
        let draft = service.save_cart(&cart, None, "op-1", "shift-1").unwrap();

        let recalled_cart = service.recall(&draft.id).unwrap();

        assert_eq!(recalled_cart.line_count(), 1);
        assert_eq!(
            recalled_cart.customer_name,
            Some("Test Customer".to_string())
        );
    }

    #[test]
    fn test_recall_and_delete() {
        let db = Arc::new(setup_db());
        create_operator(&db, "op-1");
        let service = DraftService::new(db);

        let cart = create_test_cart();
        let draft = service.save_cart(&cart, None, "op-1", "shift-1").unwrap();
        let draft_id = draft.id.clone();

        let _recalled = service.recall_and_delete(&draft_id).unwrap();

        // Draft should be deleted
        let result = service.get(&draft_id);
        assert!(matches!(result, Err(DraftError::NotFound(_))));
    }

    #[test]
    fn test_list_by_operator() {
        let db = Arc::new(setup_db());
        create_operator(&db, "op-1");
        create_operator(&db, "op-2");
        let service = DraftService::new(db);

        let cart = create_test_cart();

        // Create drafts for different operators
        service
            .save_cart(&cart, Some("Draft 1".to_string()), "op-1", "shift-1")
            .unwrap();
        service
            .save_cart(&cart, Some("Draft 2".to_string()), "op-1", "shift-1")
            .unwrap();
        service
            .save_cart(&cart, Some("Draft 3".to_string()), "op-2", "shift-1")
            .unwrap();

        let op1_drafts = service.list_by_operator("op-1").unwrap();
        let op2_drafts = service.list_by_operator("op-2").unwrap();

        assert_eq!(op1_drafts.len(), 2);
        assert_eq!(op2_drafts.len(), 1);
    }

    #[test]
    fn test_list_by_shift() {
        let db = Arc::new(setup_db());
        create_operator(&db, "op-1");
        let service = DraftService::new(db);

        let cart = create_test_cart();

        // Create drafts for different shifts
        service.save_cart(&cart, None, "op-1", "shift-1").unwrap();
        service.save_cart(&cart, None, "op-1", "shift-1").unwrap();
        service.save_cart(&cart, None, "op-1", "shift-2").unwrap();

        let shift1_drafts = service.list_by_shift("shift-1").unwrap();
        let shift2_drafts = service.list_by_shift("shift-2").unwrap();

        assert_eq!(shift1_drafts.len(), 2);
        assert_eq!(shift2_drafts.len(), 1);
    }

    #[test]
    fn test_delete_draft() {
        let db = Arc::new(setup_db());
        create_operator(&db, "op-1");
        let service = DraftService::new(db);

        let cart = create_test_cart();
        let draft = service.save_cart(&cart, None, "op-1", "shift-1").unwrap();

        let deleted = service.delete(&draft.id).unwrap();
        assert!(deleted);

        let result = service.get(&draft.id);
        assert!(matches!(result, Err(DraftError::NotFound(_))));
    }

    #[test]
    fn test_rename_draft() {
        let db = Arc::new(setup_db());
        create_operator(&db, "op-1");
        let service = DraftService::new(db);

        let cart = create_test_cart();
        let draft = service.save_cart(&cart, None, "op-1", "shift-1").unwrap();

        let renamed = service
            .rename(&draft.id, Some("New Name".to_string()))
            .unwrap();
        assert_eq!(renamed.name, Some("New Name".to_string()));

        // Verify persistence
        let fetched = service.get(&draft.id).unwrap();
        assert_eq!(fetched.name, Some("New Name".to_string()));
    }

    #[test]
    fn test_draft_count() {
        let db = Arc::new(setup_db());
        create_operator(&db, "op-1");
        let service = DraftService::new(db);

        let cart = create_test_cart();

        assert_eq!(service.count_active().unwrap(), 0);

        service.save_cart(&cart, None, "op-1", "shift-1").unwrap();
        service.save_cart(&cart, None, "op-1", "shift-1").unwrap();

        assert_eq!(service.count_active().unwrap(), 2);
    }

    #[test]
    fn test_draft_display_name() {
        let db = Arc::new(setup_db());
        create_operator(&db, "op-1");
        let service = DraftService::new(db);

        let cart = create_test_cart();

        // Draft with name
        let draft1 = service
            .save_cart(&cart, Some("My Draft".to_string()), "op-1", "shift-1")
            .unwrap();
        assert_eq!(draft1.display_name(), "My Draft");

        // Draft without name
        let draft2 = service.save_cart(&cart, None, "op-1", "shift-1").unwrap();
        assert_eq!(draft2.display_name(), "Unnamed Draft");
    }

    #[test]
    fn test_draft_expiry() {
        let cart = create_test_cart();

        // Draft with expiry
        let draft = Draft::from_cart(&cart, None, Some("op-1".to_string()), None, Some(24));
        assert!(draft.expires_at.is_some());
        assert!(!draft.is_expired());
        assert!(draft.time_until_expiry().unwrap() > Duration::hours(23));

        // Draft without expiry
        let draft2 = Draft::from_cart(&cart, None, Some("op-1".to_string()), None, None);
        assert!(draft2.expires_at.is_none());
        assert!(!draft2.is_expired());
    }

    #[test]
    fn test_draft_limit() {
        let db = Arc::new(setup_db());
        create_operator(&db, "op-1");
        let service = DraftService::new(db);

        let cart = create_test_cart();

        // Create max drafts
        for i in 0..MAX_DRAFTS_PER_OPERATOR {
            service
                .save_cart(&cart, Some(format!("Draft {}", i)), "op-1", "shift-1")
                .unwrap();
        }

        // Try to create one more
        let result = service.save_cart(&cart, Some("Extra Draft".to_string()), "op-1", "shift-1");
        assert!(matches!(result, Err(DraftError::LimitExceeded(_))));
    }

    #[test]
    fn test_draft_cart_discount_preserved() {
        let db = Arc::new(setup_db());
        create_operator(&db, "op-1");
        let service = DraftService::new(db);

        let mut cart = create_test_cart();
        cart.cart_discount_percent = Decimal::from(10);
        cart.recalculate();
        let original_total = cart.grand_total;

        let draft = service.save_cart(&cart, None, "op-1", "shift-1").unwrap();
        let recalled = service.recall(&draft.id).unwrap();

        assert_eq!(recalled.cart_discount_percent, Decimal::from(10));
        assert_eq!(recalled.grand_total, original_total);
    }

    #[test]
    fn test_draft_from_cart_item_details() {
        let cart = create_test_cart();
        let draft = Draft::from_cart(&cart, Some("Test".to_string()), None, None, None);

        assert_eq!(draft.item_count, 1);
        assert!((draft.total_quantity - 2.0).abs() < f64::EPSILON);
        assert!(draft.total_amount > 0.0);
    }
}
