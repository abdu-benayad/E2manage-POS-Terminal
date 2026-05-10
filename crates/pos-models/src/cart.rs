//! Cart Domain Models
//!
//! This module contains the cart-related domain models for the POS terminal.
//! Cart represents the current shopping cart with items, discounts, and totals.

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::product::{Product, ProductType, ProductUnit};

fn default_true() -> bool {
    true
}

/// A single item in the shopping cart
///
/// Represents a product with quantity, pricing, and discount information.
/// All calculations (subtotal, tax, total) are performed when the item is modified.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CartItem {
    /// Unique identifier for this cart item (UUID)
    pub id: String,
    /// Reference to the product ID
    pub product_id: String,
    /// Product name (English)
    pub product_name: String,
    /// Product name (Arabic) - optional
    pub product_name_ar: Option<String>,
    /// Product SKU
    pub sku: String,
    /// Barcode (optional)
    pub barcode: Option<String>,
    /// Quantity in cart
    pub quantity: Decimal,
    /// Unit of measure
    pub unit: ProductUnit,
    /// Unit price (before tax)
    pub unit_price: Decimal,
    /// Tax rate percentage (e.g., 15.0 for 15%)
    pub tax_rate: Decimal,
    /// Whether tax is inclusive in the unit price
    pub tax_inclusive: bool,
    /// Fixed discount amount per unit
    pub discount_amount: Decimal,
    /// Discount percentage (0-100)
    pub discount_percent: Decimal,
    /// Product type classification (Phase 3 Track H)
    #[serde(default)]
    pub product_type: ProductType,
    /// Whether this product tracks inventory
    #[serde(default = "default_true")]
    pub track_inventory: bool,
    /// Optional note/comment for this item
    pub note: Option<String>,
    /// Calculated: subtotal before tax (after discount)
    pub line_subtotal: Decimal,
    /// Calculated: tax amount for this line
    pub line_tax: Decimal,
    /// Calculated: total after tax
    pub line_total: Decimal,
    /// Calculated: total discount applied
    pub line_discount: Decimal,
}

impl CartItem {
    /// Creates a new cart item from a product with the given quantity
    ///
    /// # Arguments
    /// * `product` - The product to add to cart
    /// * `quantity` - The initial quantity
    ///
    /// # Example
    /// ```rust,ignore
    /// let item = CartItem::new(&product, Decimal::from(2));
    /// ```
    pub fn new(product: &Product, quantity: Decimal) -> Self {
        let mut item = Self {
            id: Uuid::new_v4().to_string(),
            product_id: product.id.clone(),
            product_name: product.name.clone(),
            product_name_ar: product.name_ar.clone(),
            sku: product.sku.clone(),
            barcode: product.barcode.clone(),
            quantity,
            unit: product.unit.clone(),
            unit_price: product.price,
            tax_rate: product.tax_rate,
            tax_inclusive: product.tax_inclusive,
            discount_amount: Decimal::ZERO,
            discount_percent: Decimal::ZERO,
            product_type: product.product_type.clone(),
            track_inventory: product.track_inventory,
            note: None,
            line_subtotal: Decimal::ZERO,
            line_tax: Decimal::ZERO,
            line_total: Decimal::ZERO,
            line_discount: Decimal::ZERO,
        };
        item.recalculate();
        item
    }

    /// Recalculates all line totals based on current values
    ///
    /// This should be called whenever quantity, price, or discount changes.
    /// Handles both tax-inclusive and tax-exclusive pricing.
    pub fn recalculate(&mut self) {
        let hundred = Decimal::from(100);

        // Calculate gross amount (before discount)
        let gross = self.unit_price * self.quantity;

        // Calculate discount (percent takes priority over fixed amount)
        let discount = if self.discount_percent > Decimal::ZERO {
            gross * (self.discount_percent / hundred)
        } else {
            self.discount_amount * self.quantity
        };
        self.line_discount = discount;

        // Calculate subtotal after discount
        let subtotal_after_discount = gross - discount;

        // Calculate tax based on whether price is tax-inclusive
        if self.tax_inclusive {
            // Price already includes tax, extract it
            let net = subtotal_after_discount / (Decimal::ONE + self.tax_rate / hundred);
            self.line_tax = subtotal_after_discount - net;
            self.line_subtotal = net;
            self.line_total = subtotal_after_discount;
        } else {
            // Price is tax-exclusive, add tax
            self.line_subtotal = subtotal_after_discount;
            self.line_tax = subtotal_after_discount * (self.tax_rate / hundred);
            self.line_total = subtotal_after_discount + self.line_tax;
        }
    }

    /// Sets a new quantity and recalculates
    pub fn set_quantity(&mut self, qty: Decimal) {
        self.quantity = qty.max(Decimal::ZERO);
        self.recalculate();
    }

    /// Increments quantity by 1
    pub fn increment(&mut self) {
        self.quantity += Decimal::ONE;
        self.recalculate();
    }

    /// Decrements quantity by 1 (minimum 0)
    pub fn decrement(&mut self) {
        if self.quantity > Decimal::ONE {
            self.quantity -= Decimal::ONE;
            self.recalculate();
        }
    }

    /// Applies a percentage discount to this item
    ///
    /// # Arguments
    /// * `percent` - Discount percentage (0-100)
    pub fn apply_discount_percent(&mut self, percent: Decimal) {
        self.discount_percent = percent.max(Decimal::ZERO).min(Decimal::from(100));
        self.discount_amount = Decimal::ZERO;
        self.recalculate();
    }

    /// Applies a fixed amount discount per unit
    ///
    /// # Arguments
    /// * `amount` - Fixed discount amount per unit
    pub fn apply_discount_amount(&mut self, amount: Decimal) {
        self.discount_amount = amount.max(Decimal::ZERO);
        self.discount_percent = Decimal::ZERO;
        self.recalculate();
    }

    /// Clears all discounts from this item
    pub fn clear_discount(&mut self) {
        self.discount_amount = Decimal::ZERO;
        self.discount_percent = Decimal::ZERO;
        self.recalculate();
    }

    /// Sets a note for this item
    pub fn set_note(&mut self, note: Option<String>) {
        self.note = note;
    }

    /// Returns the display name based on locale
    pub fn display_name(&self, locale: &str) -> &str {
        if locale == "ar" {
            self.product_name_ar
                .as_deref()
                .unwrap_or(&self.product_name)
        } else {
            &self.product_name
        }
    }

    /// Returns true if this item has any discount applied
    pub fn has_discount(&self) -> bool {
        self.discount_percent > Decimal::ZERO || self.discount_amount > Decimal::ZERO
    }

    /// Returns the discount label for display (e.g., "-10%" or "-5.00")
    pub fn discount_label(&self) -> String {
        if self.discount_percent > Decimal::ZERO {
            format!("-{}%", self.discount_percent.round_dp(0))
        } else if self.discount_amount > Decimal::ZERO {
            format!("-{}", self.discount_amount.round_dp(3))
        } else {
            String::new()
        }
    }

    /// Returns the original line total before any discounts
    pub fn original_line_total(&self) -> Decimal {
        if self.tax_inclusive {
            self.unit_price * self.quantity
        } else {
            self.unit_price * self.quantity * (Decimal::ONE + self.tax_rate / Decimal::from(100))
        }
    }
}

/// The shopping cart containing all items and totals
///
/// Maintains a list of items and calculates aggregate totals.
/// Supports cart-level discounts in addition to item-level discounts.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Cart {
    /// List of items in the cart
    pub items: Vec<CartItem>,
    /// Associated customer ID (optional)
    pub customer_id: Option<String>,
    /// Associated customer name (optional)
    pub customer_name: Option<String>,
    /// Customer's credit balance (for display)
    pub customer_balance: Decimal,
    /// Calculated: sum of all line subtotals
    pub subtotal: Decimal,
    /// Calculated: sum of all line taxes
    pub tax_total: Decimal,
    /// Calculated: total discount (items + cart)
    pub discount_total: Decimal,
    /// Calculated: final total after tax and discount
    pub grand_total: Decimal,
    /// Cart-level discount percentage
    pub cart_discount_percent: Decimal,
    /// Cart-level fixed discount amount
    pub cart_discount_amount: Decimal,
    /// Optional note for the entire cart/order
    pub note: Option<String>,
}

impl Default for Cart {
    fn default() -> Self {
        Self {
            items: Vec::new(),
            customer_id: None,
            customer_name: None,
            customer_balance: Decimal::ZERO,
            subtotal: Decimal::ZERO,
            tax_total: Decimal::ZERO,
            discount_total: Decimal::ZERO,
            grand_total: Decimal::ZERO,
            cart_discount_percent: Decimal::ZERO,
            cart_discount_amount: Decimal::ZERO,
            note: None,
        }
    }
}

impl Cart {
    /// Creates a new empty cart
    pub fn new() -> Self {
        Self::default()
    }

    /// Recalculates all cart totals
    ///
    /// Sums up all item totals and applies cart-level discount.
    pub fn recalculate(&mut self) {
        // Sum item-level totals
        let items_subtotal: Decimal = self.items.iter().map(|i| i.line_subtotal).sum();
        let items_tax: Decimal = self.items.iter().map(|i| i.line_tax).sum();
        let items_discount: Decimal = self.items.iter().map(|i| i.line_discount).sum();

        // Calculate cart-level discount
        let cart_discount = if self.cart_discount_percent > Decimal::ZERO {
            items_subtotal * (self.cart_discount_percent / Decimal::from(100))
        } else {
            self.cart_discount_amount
        };

        self.subtotal = items_subtotal - cart_discount;
        self.tax_total = items_tax;
        self.discount_total = items_discount + cart_discount;
        self.grand_total = self.subtotal + self.tax_total;

        // Ensure grand_total doesn't go negative
        if self.grand_total.is_sign_negative() {
            self.grand_total = Decimal::ZERO;
        }
    }

    /// Returns the total number of items (sum of quantities)
    pub fn item_count(&self) -> Decimal {
        self.items.iter().map(|i| i.quantity).sum()
    }

    /// Returns the number of unique line items
    pub fn line_count(&self) -> usize {
        self.items.len()
    }

    /// Returns true if the cart is empty
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Returns true if the cart has any discount applied
    pub fn has_discount(&self) -> bool {
        self.discount_total > Decimal::ZERO
    }

    /// Returns true if a customer is associated with the cart
    pub fn has_customer(&self) -> bool {
        self.customer_id.is_some()
    }

    /// Finds an item by its ID
    pub fn find_item(&self, item_id: &str) -> Option<&CartItem> {
        self.items.iter().find(|i| i.id == item_id)
    }

    /// Finds an item by its ID (mutable)
    pub fn find_item_mut(&mut self, item_id: &str) -> Option<&mut CartItem> {
        self.items.iter_mut().find(|i| i.id == item_id)
    }

    /// Finds an item by product ID
    pub fn find_by_product(&self, product_id: &str) -> Option<&CartItem> {
        self.items.iter().find(|i| i.product_id == product_id)
    }

    /// Finds an item by product ID (mutable)
    pub fn find_by_product_mut(&mut self, product_id: &str) -> Option<&mut CartItem> {
        self.items.iter_mut().find(|i| i.product_id == product_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_product() -> Product {
        Product {
            id: "prod-001".to_string(),
            sku: "SKU001".to_string(),
            barcode: Some("1234567890123".to_string()),
            name: "Test Product".to_string(),
            name_ar: Some("منتج تجريبي".to_string()),
            price: Decimal::from(10),
            tax_rate: Decimal::from(15),
            tax_inclusive: false,
            unit: ProductUnit::Unit,
            ..Default::default()
        }
    }

    fn create_test_product_tax_inclusive() -> Product {
        Product {
            id: "prod-002".to_string(),
            sku: "SKU002".to_string(),
            name: "Tax Inclusive Product".to_string(),
            price: Decimal::from(115),
            tax_rate: Decimal::from(15),
            tax_inclusive: true,
            unit: ProductUnit::Unit,
            ..Default::default()
        }
    }

    #[test]
    fn test_cart_item_new() {
        let product = create_test_product();
        let item = CartItem::new(&product, Decimal::from(2));

        assert_eq!(item.product_id, "prod-001");
        assert_eq!(item.quantity, Decimal::from(2));
        assert_eq!(item.unit_price, Decimal::from(10));
        assert_eq!(item.line_subtotal, Decimal::from(20));
        assert_eq!(item.line_tax, Decimal::from(3)); // 20 * 15% = 3
        assert_eq!(item.line_total, Decimal::from(23));
    }

    #[test]
    fn test_cart_item_tax_inclusive() {
        let product = create_test_product_tax_inclusive();
        let item = CartItem::new(&product, Decimal::ONE);

        // 115 inclusive at 15% means net = 115/1.15 = 100, tax = 15
        assert_eq!(item.line_subtotal, Decimal::from(100));
        assert_eq!(item.line_tax, Decimal::from(15));
        assert_eq!(item.line_total, Decimal::from(115));
    }

    #[test]
    fn test_cart_item_set_quantity() {
        let product = create_test_product();
        let mut item = CartItem::new(&product, Decimal::ONE);

        item.set_quantity(Decimal::from(5));

        assert_eq!(item.quantity, Decimal::from(5));
        assert_eq!(item.line_subtotal, Decimal::from(50));
        assert_eq!(item.line_tax, Decimal::new(75, 1)); // 7.5
        assert_eq!(item.line_total, Decimal::new(575, 1)); // 57.5
    }

    #[test]
    fn test_cart_item_increment_decrement() {
        let product = create_test_product();
        let mut item = CartItem::new(&product, Decimal::from(2));

        item.increment();
        assert_eq!(item.quantity, Decimal::from(3));

        item.decrement();
        assert_eq!(item.quantity, Decimal::from(2));

        // Cannot go below 1
        item.set_quantity(Decimal::ONE);
        item.decrement();
        assert_eq!(item.quantity, Decimal::ONE);
    }

    #[test]
    fn test_cart_item_discount_percent() {
        let product = create_test_product();
        let mut item = CartItem::new(&product, Decimal::from(2));

        item.apply_discount_percent(Decimal::from(10));

        // Gross: 20, Discount: 2, Subtotal: 18, Tax: 2.7, Total: 20.7
        assert_eq!(item.line_discount, Decimal::from(2));
        assert_eq!(item.line_subtotal, Decimal::from(18));
        assert_eq!(item.line_tax, Decimal::new(27, 1)); // 2.7
        assert_eq!(item.line_total, Decimal::new(207, 1)); // 20.7
        assert!(item.has_discount());
        assert_eq!(item.discount_label(), "-10%");
    }

    #[test]
    fn test_cart_item_discount_amount() {
        let product = create_test_product();
        let mut item = CartItem::new(&product, Decimal::from(2));

        item.apply_discount_amount(Decimal::ONE); // 1.0 per unit

        // Gross: 20, Discount: 2 (1*2), Subtotal: 18, Tax: 2.7, Total: 20.7
        assert_eq!(item.line_discount, Decimal::from(2));
        assert_eq!(item.line_subtotal, Decimal::from(18));
    }

    #[test]
    fn test_cart_item_clear_discount() {
        let product = create_test_product();
        let mut item = CartItem::new(&product, Decimal::from(2));

        item.apply_discount_percent(Decimal::from(10));
        assert!(item.has_discount());

        item.clear_discount();
        assert!(!item.has_discount());
        assert_eq!(item.line_total, Decimal::from(23));
    }

    #[test]
    fn test_cart_item_display_name() {
        let product = create_test_product();
        let item = CartItem::new(&product, Decimal::ONE);

        assert_eq!(item.display_name("en"), "Test Product");
        assert_eq!(item.display_name("ar"), "منتج تجريبي");
    }

    #[test]
    fn test_cart_item_note() {
        let product = create_test_product();
        let mut item = CartItem::new(&product, Decimal::ONE);

        item.set_note(Some("No onions".to_string()));
        assert_eq!(item.note, Some("No onions".to_string()));

        item.set_note(None);
        assert_eq!(item.note, None);
    }

    #[test]
    fn test_cart_new() {
        let cart = Cart::new();

        assert!(cart.is_empty());
        assert_eq!(cart.item_count(), Decimal::ZERO);
        assert_eq!(cart.line_count(), 0);
        assert_eq!(cart.grand_total, Decimal::ZERO);
    }

    #[test]
    fn test_cart_recalculate() {
        let product1 = create_test_product();
        let product2 = Product {
            id: "prod-002".to_string(),
            sku: "SKU002".to_string(),
            name: "Product 2".to_string(),
            price: Decimal::from(20),
            tax_rate: Decimal::from(15),
            tax_inclusive: false,
            unit: ProductUnit::Unit,
            ..Default::default()
        };

        let mut cart = Cart::new();
        cart.items.push(CartItem::new(&product1, Decimal::from(2))); // 23.0 total
        cart.items.push(CartItem::new(&product2, Decimal::ONE)); // 23.0 total
        cart.recalculate();

        assert_eq!(cart.line_count(), 2);
        assert_eq!(cart.item_count(), Decimal::from(3));
        assert_eq!(cart.subtotal, Decimal::from(40)); // 20 + 20
        assert_eq!(cart.tax_total, Decimal::from(6)); // 3 + 3
        assert_eq!(cart.grand_total, Decimal::from(46));
    }

    #[test]
    fn test_cart_discount_percent() {
        let product = create_test_product();
        let mut cart = Cart::new();
        cart.items.push(CartItem::new(&product, Decimal::from(10))); // 100 subtotal, 15 tax
        cart.cart_discount_percent = Decimal::from(10);
        cart.recalculate();

        // Subtotal: 100, Cart discount: 10, Net subtotal: 90, Tax: 15, Total: 105
        assert_eq!(cart.discount_total, Decimal::from(10));
        assert_eq!(cart.subtotal, Decimal::from(90));
        assert_eq!(cart.grand_total, Decimal::from(105));
    }

    #[test]
    fn test_cart_discount_amount() {
        let product = create_test_product();
        let mut cart = Cart::new();
        cart.items.push(CartItem::new(&product, Decimal::from(10))); // 100 subtotal, 15 tax
        cart.cart_discount_amount = Decimal::from(5);
        cart.recalculate();

        assert_eq!(cart.discount_total, Decimal::from(5));
        assert_eq!(cart.subtotal, Decimal::from(95));
        assert_eq!(cart.grand_total, Decimal::from(110));
    }

    #[test]
    fn test_cart_customer() {
        let mut cart = Cart::new();

        assert!(!cart.has_customer());

        cart.customer_id = Some("cust-001".to_string());
        cart.customer_name = Some("John Doe".to_string());
        cart.customer_balance = Decimal::from(500);

        assert!(cart.has_customer());
        assert_eq!(cart.customer_name, Some("John Doe".to_string()));
    }

    #[test]
    fn test_cart_find_item() {
        let product = create_test_product();
        let mut cart = Cart::new();
        let item = CartItem::new(&product, Decimal::ONE);
        let item_id = item.id.clone();
        cart.items.push(item);

        assert!(cart.find_item(&item_id).is_some());
        assert!(cart.find_item("nonexistent").is_none());
        assert!(cart.find_by_product("prod-001").is_some());
        assert!(cart.find_by_product("nonexistent").is_none());
    }

    #[test]
    fn test_cart_combined_discounts() {
        let product = create_test_product();
        let mut cart = Cart::new();

        let mut item = CartItem::new(&product, Decimal::from(10));
        item.apply_discount_percent(Decimal::from(10)); // Item gets 10% off
        cart.items.push(item);

        cart.cart_discount_percent = Decimal::from(5); // Additional 5% off cart
        cart.recalculate();

        // Item: 100 gross, 10% off = 90 subtotal
        // Cart: 90 subtotal, 5% off = 4.5 cart discount, 85.5 net subtotal
        // Tax on 90 (item level) = 13.5
        // Total = 85.5 + 13.5 = 99.0
        assert_eq!(cart.discount_total, Decimal::new(145, 1)); // 14.5
        assert_eq!(cart.subtotal, Decimal::new(855, 1)); // 85.5
        assert_eq!(cart.grand_total, Decimal::from(99));
    }

    #[test]
    fn test_cart_negative_total_protection() {
        let product = create_test_product();
        let mut cart = Cart::new();
        cart.items.push(CartItem::new(&product, Decimal::ONE));
        cart.cart_discount_amount = Decimal::from(1000); // Discount larger than cart value
        cart.recalculate();

        assert!(cart.grand_total >= Decimal::ZERO);
    }

    #[test]
    fn test_cart_item_original_total() {
        let product = create_test_product();
        let mut item = CartItem::new(&product, Decimal::from(2));

        let original = item.original_line_total();
        assert_eq!(original, Decimal::from(23)); // 20 + 3 tax

        item.apply_discount_percent(Decimal::from(10));
        let still_original = item.original_line_total();
        assert_eq!(still_original, Decimal::from(23)); // Should be same
    }
}
