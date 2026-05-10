//! Cart Service - Shopping Cart Business Logic
//!
//! This service manages the shopping cart state and provides operations
//! for adding, updating, and removing items. The cart is thread-safe
//! and can be accessed from both Rust backend and Slint UI.
//!
//! ## Example
//!
//! ```rust,ignore
//! use e2manage_pos_terminal::services::CartService;
//! use e2manage_pos_terminal::models::Product;
//! use std::sync::Arc;
//!
//! let cart_service = Arc::new(CartService::new());
//!
//! // Add product to cart
//! let product = get_product("prod-001");
//! cart_service.add_item(&product, 2.0)?;
//!
//! // Get cart snapshot
//! let cart = cart_service.get_cart();
//! println!("Total: ${:.2}", cart.grand_total);
//!
//! // Apply discount
//! cart_service.apply_cart_discount(10.0)?;
//!
//! // Clear cart
//! cart_service.clear();
//! ```

use parking_lot::RwLock;
use pos_db::Database;
use pos_models::cart::{Cart, CartItem};
use pos_models::product::Product;
use rust_decimal::Decimal;
use std::sync::Arc;
use thiserror::Error;
use tracing::{debug, info, warn};

/// Errors that can occur during cart operations
#[derive(Error, Debug)]
pub enum CartError {
    #[error("Item not found: {0}")]
    ItemNotFound(String),

    #[error("Product not found: {0}")]
    ProductNotFound(String),

    #[error("Invalid quantity: {0}")]
    InvalidQuantity(String),

    #[error("Invalid discount: {0}")]
    InvalidDiscount(String),

    #[error("Cart is empty")]
    EmptyCart,

    #[error("Operation failed: {0}")]
    OperationFailed(String),
}

/// Result type for cart operations
pub type CartResult<T> = Result<T, CartError>;

/// Cart service providing thread-safe cart operations
///
/// This service uses RwLock to allow concurrent reads and exclusive writes.
/// All operations that modify the cart automatically recalculate totals.
///
/// When created with `with_persistence()`, cart changes are automatically
/// saved to SQLite for crash recovery.
pub struct CartService {
    cart: Arc<RwLock<Cart>>,
    /// Optional database for cart persistence (crash recovery)
    db: Option<Arc<Database>>,
    /// Current operator ID for persistence tracking
    operator_id: Arc<RwLock<Option<String>>>,
}

impl CartService {
    /// Creates a new cart service with an empty cart (no persistence)
    pub fn new() -> Self {
        Self {
            cart: Arc::new(RwLock::new(Cart::new())),
            db: None,
            operator_id: Arc::new(RwLock::new(None)),
        }
    }

    /// Creates a cart service with database persistence for crash recovery
    ///
    /// When persistence is enabled, every cart mutation is automatically
    /// saved to SQLite. On startup, call `restore()` to recover a cart
    /// that was in progress before a crash.
    pub fn with_persistence(db: Arc<Database>) -> Self {
        Self {
            cart: Arc::new(RwLock::new(Cart::new())),
            db: Some(db),
            operator_id: Arc::new(RwLock::new(None)),
        }
    }

    /// Creates a cart service with an existing cart (no persistence)
    pub fn with_cart(cart: Cart) -> Self {
        Self {
            cart: Arc::new(RwLock::new(cart)),
            db: None,
            operator_id: Arc::new(RwLock::new(None)),
        }
    }

    /// Sets the current operator ID for persistence tracking
    pub fn set_operator(&self, operator_id: Option<String>) {
        *self.operator_id.write() = operator_id;
    }

    /// Persists the current cart to SQLite (if persistence is enabled)
    ///
    /// This is called automatically after every cart mutation.
    /// Silent failures are logged but don't interrupt cart operations.
    fn persist(&self) {
        if let Some(db) = &self.db {
            let cart = self.cart.read();
            match serde_json::to_string(&*cart) {
                Ok(json) => {
                    let op_id = self.operator_id.read();
                    if let Err(e) = db.save_active_cart(op_id.as_deref(), &json) {
                        warn!("Failed to persist cart: {}", e);
                    }
                }
                Err(e) => {
                    warn!("Failed to serialize cart for persistence: {}", e);
                }
            }
        }
    }

    /// Restores a cart from SQLite (crash recovery)
    ///
    /// Call this on startup to recover a cart that was in progress before
    /// an app crash or power failure. Returns true if a non-empty cart
    /// was restored, false otherwise.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let cart_service = CartService::with_persistence(db);
    /// if cart_service.restore() {
    ///     info!("Recovered cart from previous session");
    /// }
    /// ```
    pub fn restore(&self) -> bool {
        if let Some(db) = &self.db {
            match db.get_active_cart() {
                Ok(Some((op_id, json))) => {
                    match serde_json::from_str::<Cart>(&json) {
                        Ok(cart) => {
                            if !cart.is_empty() {
                                *self.cart.write() = cart;
                                // Restore operator ID if available
                                if let Some(op) = op_id {
                                    *self.operator_id.write() = Some(op);
                                }
                                info!(
                                    "Restored cart with {} items from previous session",
                                    self.line_count()
                                );
                                return true;
                            }
                        }
                        Err(e) => {
                            warn!("Failed to deserialize recovered cart: {}", e);
                        }
                    }
                }
                Ok(None) => {
                    debug!("No active cart to restore");
                }
                Err(e) => {
                    warn!("Failed to query active cart: {}", e);
                }
            }
        }
        false
    }

    /// Adds a product to the cart
    ///
    /// If the product already exists in the cart, increments the quantity.
    /// Otherwise, creates a new cart item.
    ///
    /// # Arguments
    /// * `product` - The product to add
    /// * `quantity` - The quantity to add (must be positive)
    pub fn add_item(&self, product: &Product, quantity: Decimal) -> CartResult<String> {
        if quantity <= Decimal::ZERO {
            return Err(CartError::InvalidQuantity(
                "Quantity must be positive".to_string(),
            ));
        }

        let mut cart = self.cart.write();

        // Check if product already in cart - find index first
        let existing_idx = cart.items.iter().position(|i| i.product_id == product.id);

        let item_id = if let Some(idx) = existing_idx {
            let new_qty = cart.items[idx].quantity + quantity;
            cart.items[idx].set_quantity(new_qty);
            let item_id = cart.items[idx].id.clone();
            cart.recalculate();
            info!(
                "Updated {} x {} in cart (new qty: {})",
                quantity, product.name, new_qty
            );
            item_id
        } else {
            let item = CartItem::new(product, quantity);
            let item_id = item.id.clone();
            cart.items.push(item);
            cart.recalculate();
            info!("Added {} x {} to cart", quantity, product.name);
            item_id
        };
        drop(cart); // Release write lock before persist
        self.persist();
        Ok(item_id)
    }

    /// Updates the quantity of an existing cart item
    ///
    /// If quantity is 0, the item is removed from the cart.
    ///
    /// # Arguments
    /// * `item_id` - The cart item ID to update
    /// * `quantity` - The new quantity
    pub fn update_quantity(&self, item_id: &str, quantity: Decimal) -> CartResult<()> {
        if quantity.is_sign_negative() {
            return Err(CartError::InvalidQuantity(
                "Quantity cannot be negative".to_string(),
            ));
        }

        let mut cart = self.cart.write();

        if quantity.is_zero() {
            let original_len = cart.items.len();
            cart.items.retain(|i| i.id != item_id);
            if cart.items.len() == original_len {
                return Err(CartError::ItemNotFound(item_id.to_string()));
            }
            cart.recalculate();
            debug!("Removed item {} from cart", item_id);
            drop(cart);
            self.persist();
            return Ok(());
        }

        if let Some(item) = cart.find_item_mut(item_id) {
            item.set_quantity(quantity);
            cart.recalculate();
            debug!("Updated item {} quantity to {}", item_id, quantity);
            drop(cart);
            self.persist();
            Ok(())
        } else {
            Err(CartError::ItemNotFound(item_id.to_string()))
        }
    }

    /// Increments the quantity of an item by 1
    pub fn increment(&self, item_id: &str) -> CartResult<()> {
        let mut cart = self.cart.write();

        // Find index first to avoid borrow issues
        let idx = cart.items.iter().position(|i| i.id == item_id);

        if let Some(idx) = idx {
            cart.items[idx].increment();
            let new_qty = cart.items[idx].quantity;
            cart.recalculate();
            debug!("Incremented item {} to {}", item_id, new_qty);
            drop(cart);
            self.persist();
            Ok(())
        } else {
            Err(CartError::ItemNotFound(item_id.to_string()))
        }
    }

    /// Decrements the quantity of an item by 1
    ///
    /// If quantity would go to 0 or below, removes the item.
    pub fn decrement(&self, item_id: &str) -> CartResult<()> {
        let mut cart = self.cart.write();

        // Find index first to avoid borrow issues
        let idx = cart.items.iter().position(|i| i.id == item_id);

        if let Some(idx) = idx {
            if cart.items[idx].quantity <= Decimal::ONE {
                // Need to remove the item
                cart.items.remove(idx);
                cart.recalculate();
                info!("Removed item {} from cart (quantity reached 0)", item_id);
                drop(cart);
                self.persist();
                return Ok(());
            }
            cart.items[idx].decrement();
            let new_qty = cart.items[idx].quantity;
            cart.recalculate();
            debug!("Decremented item {} to {}", item_id, new_qty);
            drop(cart);
            self.persist();
            Ok(())
        } else {
            Err(CartError::ItemNotFound(item_id.to_string()))
        }
    }

    /// Removes an item from the cart
    pub fn remove_item(&self, item_id: &str) -> CartResult<()> {
        let mut cart = self.cart.write();

        let original_len = cart.items.len();
        cart.items.retain(|i| i.id != item_id);

        if cart.items.len() == original_len {
            return Err(CartError::ItemNotFound(item_id.to_string()));
        }

        cart.recalculate();
        info!("Removed item {} from cart", item_id);
        drop(cart);
        self.persist();
        Ok(())
    }

    /// Applies a percentage discount to a specific item
    ///
    /// # Arguments
    /// * `item_id` - The cart item ID
    /// * `percent` - Discount percentage (0-100)
    pub fn apply_item_discount(&self, item_id: &str, percent: Decimal) -> CartResult<()> {
        if percent.is_sign_negative() || percent > Decimal::from(100) {
            return Err(CartError::InvalidDiscount(
                "Discount must be between 0 and 100".to_string(),
            ));
        }

        let mut cart = self.cart.write();

        if let Some(item) = cart.find_item_mut(item_id) {
            item.apply_discount_percent(percent);
            cart.recalculate();
            info!("Applied {}% discount to item {}", percent, item_id);
            drop(cart);
            self.persist();
            Ok(())
        } else {
            Err(CartError::ItemNotFound(item_id.to_string()))
        }
    }

    /// Applies a fixed amount discount to a specific item (per unit)
    pub fn apply_item_discount_amount(&self, item_id: &str, amount: Decimal) -> CartResult<()> {
        if amount.is_sign_negative() {
            return Err(CartError::InvalidDiscount(
                "Discount amount cannot be negative".to_string(),
            ));
        }

        let mut cart = self.cart.write();

        if let Some(item) = cart.find_item_mut(item_id) {
            item.apply_discount_amount(amount);
            cart.recalculate();
            info!("Applied {} fixed discount to item {}", amount, item_id);
            drop(cart);
            self.persist();
            Ok(())
        } else {
            Err(CartError::ItemNotFound(item_id.to_string()))
        }
    }

    /// Clears the discount from a specific item
    pub fn clear_item_discount(&self, item_id: &str) -> CartResult<()> {
        let mut cart = self.cart.write();

        if let Some(item) = cart.find_item_mut(item_id) {
            item.clear_discount();
            cart.recalculate();
            info!("Cleared discount from item {}", item_id);
            drop(cart);
            self.persist();
            Ok(())
        } else {
            Err(CartError::ItemNotFound(item_id.to_string()))
        }
    }

    /// Sets a note on a specific item
    pub fn set_item_note(&self, item_id: &str, note: Option<String>) -> CartResult<()> {
        let mut cart = self.cart.write();

        if let Some(item) = cart.find_item_mut(item_id) {
            item.set_note(note);
            debug!("Set note on item {}", item_id);
            Ok(())
        } else {
            Err(CartError::ItemNotFound(item_id.to_string()))
        }
    }

    /// Applies a percentage discount to the entire cart
    ///
    /// Clears any fixed amount discount.
    pub fn apply_cart_discount(&self, percent: Decimal) -> CartResult<()> {
        if percent.is_sign_negative() || percent > Decimal::from(100) {
            return Err(CartError::InvalidDiscount(
                "Discount must be between 0 and 100".to_string(),
            ));
        }

        let mut cart = self.cart.write();
        cart.cart_discount_percent = percent;
        cart.cart_discount_amount = Decimal::ZERO;
        cart.recalculate();
        info!("Applied {}% cart discount", percent);
        drop(cart);
        self.persist();
        Ok(())
    }

    /// Applies a fixed amount discount to the entire cart
    ///
    /// Clears any percentage discount.
    pub fn apply_cart_discount_amount(&self, amount: Decimal) -> CartResult<()> {
        if amount.is_sign_negative() {
            return Err(CartError::InvalidDiscount(
                "Discount amount cannot be negative".to_string(),
            ));
        }

        let mut cart = self.cart.write();
        cart.cart_discount_amount = amount;
        cart.cart_discount_percent = Decimal::ZERO;
        cart.recalculate();
        info!("Applied {} fixed cart discount", amount);
        drop(cart);
        self.persist();
        Ok(())
    }

    /// Clears cart-level discount
    pub fn clear_cart_discount(&self) -> CartResult<()> {
        let mut cart = self.cart.write();
        cart.cart_discount_percent = Decimal::ZERO;
        cart.cart_discount_amount = Decimal::ZERO;
        cart.recalculate();
        info!("Cleared cart discount");
        drop(cart);
        self.persist();
        Ok(())
    }

    /// Sets the customer for the cart
    pub fn set_customer(
        &self,
        customer_id: Option<String>,
        customer_name: Option<String>,
        balance: Decimal,
    ) {
        {
            let mut cart = self.cart.write();
            cart.customer_id = customer_id.clone();
            cart.customer_name = customer_name.clone();
            cart.customer_balance = balance;
        }

        if let Some(name) = &customer_name {
            info!("Set customer: {} (balance: {})", name, balance);
        } else {
            info!("Cleared customer from cart");
        }
        self.persist();
    }

    /// Removes the customer from the cart
    pub fn clear_customer(&self) {
        self.set_customer(None, None, Decimal::ZERO);
    }

    /// Sets a note for the entire cart/order
    pub fn set_cart_note(&self, note: Option<String>) {
        {
            let mut cart = self.cart.write();
            cart.note = note;
        }
        self.persist();
    }

    /// Clears all items from the cart
    ///
    /// Resets to a fresh empty cart but preserves customer if set.
    pub fn clear(&self) {
        {
            let mut cart = self.cart.write();
            let customer_id = cart.customer_id.clone();
            let customer_name = cart.customer_name.clone();
            let customer_balance = cart.customer_balance;

            *cart = Cart::new();

            // Optionally preserve customer
            cart.customer_id = customer_id;
            cart.customer_name = customer_name;
            cart.customer_balance = customer_balance;
        }
        info!("Cart cleared");
        self.persist();
    }

    /// Clears everything including customer
    ///
    /// Also clears the persisted cart from the database.
    pub fn clear_all(&self) {
        {
            let mut cart = self.cart.write();
            *cart = Cart::new();
        }
        info!("Cart fully cleared (including customer)");
        // Clear from database as well
        if let Some(db) = &self.db {
            if let Err(e) = db.clear_active_cart() {
                warn!("Failed to clear persisted cart: {}", e);
            }
        }
    }

    /// Returns a snapshot of the current cart state
    ///
    /// This is a clone, so modifications to the returned cart
    /// won't affect the service's cart.
    pub fn get_cart(&self) -> Cart {
        self.cart.read().clone()
    }

    /// Returns true if the cart is empty
    pub fn is_empty(&self) -> bool {
        self.cart.read().is_empty()
    }

    /// Returns the total number of items in the cart
    pub fn item_count(&self) -> Decimal {
        self.cart.read().item_count()
    }

    /// Returns the number of line items in the cart
    pub fn line_count(&self) -> usize {
        self.cart.read().line_count()
    }

    /// Returns the cart grand total
    pub fn grand_total(&self) -> Decimal {
        self.cart.read().grand_total
    }

    /// Checks if a product is already in the cart
    pub fn has_product(&self, product_id: &str) -> bool {
        self.cart.read().find_by_product(product_id).is_some()
    }

    /// Gets the quantity of a product in the cart
    pub fn get_product_quantity(&self, product_id: &str) -> Decimal {
        self.cart
            .read()
            .find_by_product(product_id)
            .map(|item| item.quantity)
            .unwrap_or(Decimal::ZERO)
    }

    /// Validates that the cart is ready for checkout
    ///
    /// Returns Ok if valid, or an error describing the issue.
    pub fn validate_for_checkout(&self) -> CartResult<()> {
        let cart = self.cart.read();

        if cart.is_empty() {
            return Err(CartError::EmptyCart);
        }

        if cart.grand_total <= Decimal::ZERO {
            return Err(CartError::OperationFailed(
                "Cart total must be positive".to_string(),
            ));
        }

        // Could add more validations here:
        // - Check stock availability
        // - Check customer credit limits
        // - Validate discounts

        Ok(())
    }

    /// Replaces the entire cart with a new cart
    ///
    /// Useful for recalling held orders/drafts.
    pub fn replace_cart(&self, new_cart: Cart) {
        let line_count = {
            let mut cart = self.cart.write();
            *cart = new_cart;
            cart.recalculate();
            cart.line_count()
        };
        info!("Cart replaced with {} items", line_count);
        self.persist();
    }
}

impl Default for CartService {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for CartService {
    fn clone(&self) -> Self {
        Self {
            cart: Arc::clone(&self.cart),
            db: self.db.clone(),
            operator_id: Arc::clone(&self.operator_id),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pos_models::product::ProductUnit;

    fn create_test_product(id: &str, name: &str, price: Decimal) -> Product {
        Product {
            id: id.to_string(),
            sku: format!("SKU-{}", id),
            name: name.to_string(),
            price,
            tax_rate: Decimal::from(15),
            tax_inclusive: false,
            unit: ProductUnit::Unit,
            ..Default::default()
        }
    }

    #[test]
    fn test_cart_service_new() {
        let service = CartService::new();
        assert!(service.is_empty());
        assert_eq!(service.item_count(), Decimal::ZERO);
    }

    #[test]
    fn test_add_item() {
        let service = CartService::new();
        let product = create_test_product("p1", "Test Product", Decimal::from(10));

        let result = service.add_item(&product, Decimal::from(2));
        assert!(result.is_ok());

        assert!(!service.is_empty());
        assert_eq!(service.item_count(), Decimal::from(2));
        assert_eq!(service.line_count(), 1);
    }

    #[test]
    fn test_add_same_product_twice() {
        let service = CartService::new();
        let product = create_test_product("p1", "Test Product", Decimal::from(10));

        service.add_item(&product, Decimal::from(2)).unwrap();
        service.add_item(&product, Decimal::from(3)).unwrap();

        assert_eq!(service.item_count(), Decimal::from(5));
        assert_eq!(service.line_count(), 1); // Should be combined
    }

    #[test]
    fn test_add_different_products() {
        let service = CartService::new();
        let product1 = create_test_product("p1", "Product 1", Decimal::from(10));
        let product2 = create_test_product("p2", "Product 2", Decimal::from(20));

        service.add_item(&product1, Decimal::ONE).unwrap();
        service.add_item(&product2, Decimal::ONE).unwrap();

        assert_eq!(service.item_count(), Decimal::from(2));
        assert_eq!(service.line_count(), 2);
    }

    #[test]
    fn test_add_item_invalid_quantity() {
        let service = CartService::new();
        let product = create_test_product("p1", "Test Product", Decimal::from(10));

        let result = service.add_item(&product, Decimal::ZERO);
        assert!(matches!(result, Err(CartError::InvalidQuantity(_))));

        let result = service.add_item(&product, Decimal::from(-1));
        assert!(matches!(result, Err(CartError::InvalidQuantity(_))));
    }

    #[test]
    fn test_update_quantity() {
        let service = CartService::new();
        let product = create_test_product("p1", "Test Product", Decimal::from(10));
        let item_id = service.add_item(&product, Decimal::from(2)).unwrap();

        service.update_quantity(&item_id, Decimal::from(5)).unwrap();

        assert_eq!(service.item_count(), Decimal::from(5));
    }

    #[test]
    fn test_update_quantity_to_zero_removes() {
        let service = CartService::new();
        let product = create_test_product("p1", "Test Product", Decimal::from(10));
        let item_id = service.add_item(&product, Decimal::from(2)).unwrap();

        service.update_quantity(&item_id, Decimal::ZERO).unwrap();

        assert!(service.is_empty());
    }

    #[test]
    fn test_increment_decrement() {
        let service = CartService::new();
        let product = create_test_product("p1", "Test Product", Decimal::from(10));
        let item_id = service.add_item(&product, Decimal::from(2)).unwrap();

        service.increment(&item_id).unwrap();
        assert_eq!(service.item_count(), Decimal::from(3));

        service.decrement(&item_id).unwrap();
        assert_eq!(service.item_count(), Decimal::from(2));
    }

    #[test]
    fn test_decrement_to_zero_removes() {
        let service = CartService::new();
        let product = create_test_product("p1", "Test Product", Decimal::from(10));
        let item_id = service.add_item(&product, Decimal::ONE).unwrap();

        service.decrement(&item_id).unwrap();

        assert!(service.is_empty());
    }

    #[test]
    fn test_remove_item() {
        let service = CartService::new();
        let product = create_test_product("p1", "Test Product", Decimal::from(10));
        let item_id = service.add_item(&product, Decimal::from(2)).unwrap();

        service.remove_item(&item_id).unwrap();

        assert!(service.is_empty());
    }

    #[test]
    fn test_remove_nonexistent_item() {
        let service = CartService::new();

        let result = service.remove_item("nonexistent");
        assert!(matches!(result, Err(CartError::ItemNotFound(_))));
    }

    #[test]
    fn test_apply_item_discount() {
        let service = CartService::new();
        let product = create_test_product("p1", "Test Product", Decimal::from(100));
        let item_id = service.add_item(&product, Decimal::ONE).unwrap();

        service
            .apply_item_discount(&item_id, Decimal::from(10))
            .unwrap();

        let cart = service.get_cart();
        let item = cart.find_item(&item_id).unwrap();
        assert_eq!(item.line_discount, Decimal::from(10));
    }

    #[test]
    fn test_apply_invalid_discount() {
        let service = CartService::new();
        let product = create_test_product("p1", "Test Product", Decimal::from(100));
        let item_id = service.add_item(&product, Decimal::ONE).unwrap();

        let result = service.apply_item_discount(&item_id, Decimal::from(-5));
        assert!(matches!(result, Err(CartError::InvalidDiscount(_))));

        let result = service.apply_item_discount(&item_id, Decimal::from(150));
        assert!(matches!(result, Err(CartError::InvalidDiscount(_))));
    }

    #[test]
    fn test_apply_cart_discount() {
        let service = CartService::new();
        let product = create_test_product("p1", "Test Product", Decimal::from(100));
        service.add_item(&product, Decimal::ONE).unwrap();

        service.apply_cart_discount(Decimal::from(10)).unwrap();

        let cart = service.get_cart();
        assert_eq!(cart.discount_total, Decimal::from(10));
    }

    #[test]
    fn test_clear_cart_discount() {
        let service = CartService::new();
        let product = create_test_product("p1", "Test Product", Decimal::from(100));
        service.add_item(&product, Decimal::ONE).unwrap();
        service.apply_cart_discount(Decimal::from(10)).unwrap();

        service.clear_cart_discount().unwrap();

        let cart = service.get_cart();
        assert_eq!(cart.cart_discount_percent, Decimal::ZERO);
    }

    #[test]
    fn test_set_customer() {
        let service = CartService::new();

        service.set_customer(
            Some("cust-001".to_string()),
            Some("John Doe".to_string()),
            Decimal::from(500),
        );

        let cart = service.get_cart();
        assert_eq!(cart.customer_id, Some("cust-001".to_string()));
        assert_eq!(cart.customer_name, Some("John Doe".to_string()));
        assert_eq!(cart.customer_balance, Decimal::from(500));
    }

    #[test]
    fn test_clear_customer() {
        let service = CartService::new();
        service.set_customer(
            Some("cust-001".to_string()),
            Some("John Doe".to_string()),
            Decimal::from(500),
        );

        service.clear_customer();

        let cart = service.get_cart();
        assert_eq!(cart.customer_id, None);
    }

    #[test]
    fn test_clear_preserves_customer() {
        let service = CartService::new();
        let product = create_test_product("p1", "Test Product", Decimal::from(10));
        service.add_item(&product, Decimal::from(2)).unwrap();
        service.set_customer(
            Some("cust-001".to_string()),
            Some("John Doe".to_string()),
            Decimal::from(500),
        );

        service.clear();

        let cart = service.get_cart();
        assert!(cart.is_empty());
        assert_eq!(cart.customer_id, Some("cust-001".to_string()));
    }

    #[test]
    fn test_clear_all() {
        let service = CartService::new();
        let product = create_test_product("p1", "Test Product", Decimal::from(10));
        service.add_item(&product, Decimal::from(2)).unwrap();
        service.set_customer(
            Some("cust-001".to_string()),
            Some("John Doe".to_string()),
            Decimal::from(500),
        );

        service.clear_all();

        let cart = service.get_cart();
        assert!(cart.is_empty());
        assert_eq!(cart.customer_id, None);
    }

    #[test]
    fn test_has_product() {
        let service = CartService::new();
        let product = create_test_product("p1", "Test Product", Decimal::from(10));

        assert!(!service.has_product("p1"));

        service.add_item(&product, Decimal::ONE).unwrap();

        assert!(service.has_product("p1"));
        assert!(!service.has_product("p2"));
    }

    #[test]
    fn test_get_product_quantity() {
        let service = CartService::new();
        let product = create_test_product("p1", "Test Product", Decimal::from(10));

        assert_eq!(service.get_product_quantity("p1"), Decimal::ZERO);

        service.add_item(&product, Decimal::from(3)).unwrap();

        assert_eq!(service.get_product_quantity("p1"), Decimal::from(3));
    }

    #[test]
    fn test_grand_total() {
        let service = CartService::new();
        let product = create_test_product("p1", "Test Product", Decimal::from(10));
        service.add_item(&product, Decimal::from(2)).unwrap();

        // 20 + 3 (15% tax) = 23
        assert_eq!(service.grand_total(), Decimal::from(23));
    }

    #[test]
    fn test_validate_for_checkout() {
        let service = CartService::new();

        // Empty cart should fail
        let result = service.validate_for_checkout();
        assert!(matches!(result, Err(CartError::EmptyCart)));

        // Add item, should pass
        let product = create_test_product("p1", "Test Product", Decimal::from(10));
        service.add_item(&product, Decimal::ONE).unwrap();

        let result = service.validate_for_checkout();
        assert!(result.is_ok());
    }

    #[test]
    fn test_replace_cart() {
        let service = CartService::new();
        let product = create_test_product("p1", "Original", Decimal::from(10));
        service.add_item(&product, Decimal::ONE).unwrap();

        let mut new_cart = Cart::new();
        new_cart.items.push(CartItem::new(
            &create_test_product("p2", "Replaced", Decimal::from(20)),
            Decimal::from(5),
        ));

        service.replace_cart(new_cart);

        assert_eq!(service.line_count(), 1);
        assert_eq!(service.item_count(), Decimal::from(5));

        let cart = service.get_cart();
        assert_eq!(cart.items[0].product_name, "Replaced");
    }

    #[test]
    fn test_set_item_note() {
        let service = CartService::new();
        let product = create_test_product("p1", "Test Product", Decimal::from(10));
        let item_id = service.add_item(&product, Decimal::ONE).unwrap();

        service
            .set_item_note(&item_id, Some("Extra spicy".to_string()))
            .unwrap();

        let cart = service.get_cart();
        let item = cart.find_item(&item_id).unwrap();
        assert_eq!(item.note, Some("Extra spicy".to_string()));
    }

    #[test]
    fn test_cart_service_clone_shares_state() {
        let service1 = CartService::new();
        let service2 = service1.clone();

        let product = create_test_product("p1", "Test Product", Decimal::from(10));
        service1.add_item(&product, Decimal::from(2)).unwrap();

        // Both should see the same cart
        assert_eq!(service2.item_count(), Decimal::from(2));
    }

    #[test]
    fn test_with_cart() {
        let mut cart = Cart::new();
        let product = create_test_product("p1", "Test Product", Decimal::from(10));
        cart.items.push(CartItem::new(&product, Decimal::from(3)));
        cart.recalculate();

        let service = CartService::with_cart(cart);

        assert_eq!(service.item_count(), Decimal::from(3));
    }
}
