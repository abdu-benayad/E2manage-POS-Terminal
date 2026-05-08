//! Cart Bridge - Slint UI Integration
//!
//! Provides types and utilities for bridging the Rust CartService
//! with the Slint UI layer. Converts cart data to Slint-compatible
//! types that can be displayed in the UI.
//!
//! ## Example
//!
//! ```rust,ignore
//! use e2manage_pos_terminal::services::CartService;
//! use e2manage_pos_terminal::ui::CartBridge;
//! use std::sync::Arc;
//!
//! let cart_service = Arc::new(CartService::new());
//! let bridge = CartBridge::new(Arc::clone(&cart_service));
//!
//! // Get items for display
//! let items = bridge.get_items_model("en");
//! for item in items {
//!     println!("{}: {} x {}", item.name, item.quantity, item.unit_price);
//! }
//!
//! // Get totals
//! let totals = bridge.get_totals("LYD", "en");
//! println!("Total: {}", totals.grand_total);
//! ```

use crate::services::cart_service::CartService;
use crate::utils::formatting;
use rust_decimal::Decimal;
use rust_decimal::prelude::*;
use std::sync::Arc;

/// Cart item model for Slint UI
///
/// This struct contains all the data needed to display a cart item
/// in the Slint UI. All monetary values are pre-formatted as strings.
#[derive(Clone, Debug, Default)]
pub struct CartItemModel {
    /// Unique cart item ID
    pub id: String,
    /// Product ID reference
    pub product_id: String,
    /// Product name (localized)
    pub name: String,
    /// Product name in Arabic
    pub name_ar: String,
    /// Product SKU
    pub sku: String,
    /// Current quantity
    pub quantity: i32,
    /// Decimal quantity (for weighable items, Slint needs f64)
    pub quantity_decimal: f64,
    /// Unit of measure label
    pub unit: String,
    /// Formatted unit price
    pub unit_price: String,
    /// Formatted line total
    pub line_total: String,
    /// Whether item has a discount
    pub has_discount: bool,
    /// Discount label (e.g., "-10%")
    pub discount_label: String,
    /// Formatted discount amount
    pub discount_amount: String,
    /// Formatted original price (before discount)
    pub original_price: String,
    /// Optional note on the item
    pub note: String,
    /// Whether this item has a note
    pub has_note: bool,
}

/// Cart totals for Slint UI
///
/// Contains all the cart-level totals needed for display.
#[derive(Clone, Debug, Default)]
pub struct CartTotals {
    /// Formatted subtotal (before tax)
    pub subtotal: String,
    /// Formatted tax total
    pub tax_total: String,
    /// Formatted total discount
    pub discount_total: String,
    /// Formatted grand total
    pub grand_total: String,
    /// Whether cart has any discount
    pub has_discount: bool,
    /// Total item count
    pub item_count: i32,
    /// Number of line items
    pub line_count: i32,
    /// Associated customer name
    pub customer_name: String,
    /// Customer balance
    pub customer_balance: String,
    /// Whether a customer is associated
    pub has_customer: bool,
    /// Cart-level discount percent (Slint needs f64)
    pub cart_discount_percent: f64,
    /// Cart-level discount amount
    pub cart_discount_amount: String,
}

/// Customer model for Slint UI
#[derive(Clone, Debug, Default)]
pub struct CustomerModel {
    /// Customer ID
    pub id: String,
    /// Customer name
    pub name: String,
    /// Formatted balance
    pub balance: String,
}

/// Bridge between Rust CartService and Slint UI
///
/// Provides methods to convert cart data to Slint-compatible types.
/// The bridge holds an Arc reference to the cart service, allowing
/// it to be shared across threads.
pub struct CartBridge {
    service: Arc<CartService>,
}

impl CartBridge {
    /// Creates a new cart bridge with the given cart service
    pub fn new(service: Arc<CartService>) -> Self {
        Self { service }
    }

    /// Gets the cart items formatted for Slint display
    ///
    /// # Arguments
    /// * `locale` - The locale code for name localization ("ar" or "en")
    /// * `currency` - Currency symbol to use (default "LYD")
    pub fn get_items_model(&self, locale: &str, currency: &str) -> Vec<CartItemModel> {
        let cart = self.service.get_cart();
        cart.items
            .iter()
            .map(|item| CartItemModel {
                id: item.id.clone(),
                product_id: item.product_id.clone(),
                name: item.display_name(locale).to_string(),
                name_ar: item.product_name_ar.clone().unwrap_or_default(),
                sku: item.sku.clone(),
                quantity: item.quantity.to_i32().unwrap_or(0),
                // Slint UI needs f64 for decimal quantities (weighable items)
                quantity_decimal: item.quantity.to_f64().unwrap_or(0.0),
                unit: item.unit.label(locale).to_string(),
                unit_price: formatting::format_currency(item.unit_price, currency, locale),
                line_total: formatting::format_currency(item.line_total, currency, locale),
                has_discount: item.has_discount(),
                discount_label: item.discount_label(),
                discount_amount: formatting::format_currency(item.line_discount, currency, locale),
                original_price: formatting::format_currency(item.original_line_total(), currency, locale),
                note: item.note.clone().unwrap_or_default(),
                has_note: item.note.is_some(),
            })
            .collect()
    }

    /// Gets the cart items formatted with default settings
    ///
    /// Uses "ar" locale. Currency should be passed from terminal config.
    pub fn get_items_model_default(&self, currency: &str) -> Vec<CartItemModel> {
        self.get_items_model("ar", currency)
    }

    /// Gets the cart totals formatted for Slint display
    ///
    /// # Arguments
    /// * `currency` - Currency code to use (e.g., "LYD", "USD")
    /// * `locale` - Locale for formatting (e.g., "ar", "en")
    pub fn get_totals(&self, currency: &str, locale: &str) -> CartTotals {
        let cart = self.service.get_cart();
        CartTotals {
            subtotal: formatting::format_currency(cart.subtotal, currency, locale),
            tax_total: formatting::format_currency(cart.tax_total, currency, locale),
            discount_total: formatting::format_currency(cart.discount_total, currency, locale),
            grand_total: formatting::format_currency(cart.grand_total, currency, locale),
            has_discount: cart.has_discount(),
            item_count: cart.item_count().to_i32().unwrap_or(0),
            line_count: cart.line_count() as i32,
            customer_name: cart.customer_name.clone().unwrap_or_default(),
            customer_balance: formatting::format_currency(cart.customer_balance, currency, locale),
            has_customer: cart.has_customer(),
            // Slint UI needs f64 for numeric display
            cart_discount_percent: cart.cart_discount_percent.to_f64().unwrap_or(0.0),
            cart_discount_amount: formatting::format_currency(cart.cart_discount_amount, currency, locale),
        }
    }

    /// Gets the cart totals with Arabic locale. Currency should be passed from terminal config.
    pub fn get_totals_default(&self, currency: &str) -> CartTotals {
        self.get_totals(currency, "ar")
    }

    /// Gets customer information if set
    pub fn get_customer(&self, currency: &str, locale: &str) -> Option<CustomerModel> {
        let cart = self.service.get_cart();
        if cart.has_customer() {
            Some(CustomerModel {
                id: cart.customer_id.unwrap_or_default(),
                name: cart.customer_name.unwrap_or_default(),
                balance: formatting::format_currency(cart.customer_balance, currency, locale),
            })
        } else {
            None
        }
    }

    /// Checks if the cart is empty
    pub fn is_empty(&self) -> bool {
        self.service.is_empty()
    }

    /// Gets the grand total as a Decimal
    pub fn grand_total(&self) -> Decimal {
        self.service.grand_total()
    }

    /// Gets the item count
    pub fn item_count(&self) -> i32 {
        self.service.item_count().to_i32().unwrap_or(0)
    }

    /// Gets the line count
    pub fn line_count(&self) -> i32 {
        self.service.line_count() as i32
    }

    /// Gets a reference to the underlying cart service
    pub fn service(&self) -> &Arc<CartService> {
        &self.service
    }
}

impl Clone for CartBridge {
    fn clone(&self) -> Self {
        Self {
            service: Arc::clone(&self.service),
        }
    }
}

/// Formats a Decimal quantity value for display.
///
/// For integers, shows without decimal. For decimals, shows 3 places.
pub fn format_quantity_decimal(qty: Decimal) -> String {
    if qty.fract().is_zero() {
        format!("{}", qty.round_dp(0))
    } else {
        format!("{:.3}", qty.round_dp(3))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::product::{Product, ProductUnit};

    fn create_test_product(id: &str, name: &str, price: Decimal) -> Product {
        Product {
            id: id.to_string(),
            sku: format!("SKU-{}", id),
            name: name.to_string(),
            name_ar: Some(format!("{}عربي", name)),
            price,
            tax_rate: Decimal::from(15),
            tax_inclusive: false,
            unit: ProductUnit::Unit,
            ..Default::default()
        }
    }

    #[test]
    fn test_cart_bridge_new() {
        let service = Arc::new(CartService::new());
        let bridge = CartBridge::new(Arc::clone(&service));

        assert!(bridge.is_empty());
        assert_eq!(bridge.item_count(), 0);
    }

    #[test]
    fn test_get_items_model() {
        let service = Arc::new(CartService::new());
        let product = create_test_product("p1", "Test Product", Decimal::from(10));
        service.add_item(&product, Decimal::from(2)).unwrap();

        let bridge = CartBridge::new(service);
        let items = bridge.get_items_model("en", "LYD");

        assert_eq!(items.len(), 1);
        assert_eq!(items[0].name, "Test Product");
        assert_eq!(items[0].quantity, 2);
        assert_eq!(items[0].unit_price, "10.000 LYD");
    }

    #[test]
    fn test_get_items_model_arabic() {
        let service = Arc::new(CartService::new());
        let product = create_test_product("p1", "Test Product", Decimal::from(10));
        service.add_item(&product, Decimal::ONE).unwrap();

        let bridge = CartBridge::new(service);
        let items = bridge.get_items_model("ar", "LYD");

        assert_eq!(items[0].name, "Test Productعربي");
    }

    #[test]
    fn test_get_totals() {
        let service = Arc::new(CartService::new());
        let product = create_test_product("p1", "Test Product", Decimal::from(100));
        service.add_item(&product, Decimal::ONE).unwrap();

        let bridge = CartBridge::new(service);
        let totals = bridge.get_totals("LYD", "en");

        assert_eq!(totals.subtotal, "100.000 LYD");
        assert_eq!(totals.tax_total, "15.000 LYD");
        assert_eq!(totals.grand_total, "115.000 LYD");
        assert_eq!(totals.item_count, 1);
        assert!(!totals.has_discount);
    }

    #[test]
    fn test_get_totals_with_discount() {
        let service = Arc::new(CartService::new());
        let product = create_test_product("p1", "Test Product", Decimal::from(100));
        service.add_item(&product, Decimal::ONE).unwrap();
        service.apply_cart_discount(Decimal::from(10)).unwrap();

        let bridge = CartBridge::new(service);
        let totals = bridge.get_totals("LYD", "en");

        assert!(totals.has_discount);
        assert_eq!(totals.cart_discount_percent, 10.0);
    }

    #[test]
    fn test_get_customer() {
        let service = Arc::new(CartService::new());
        service.set_customer(
            Some("cust-001".to_string()),
            Some("John Doe".to_string()),
            Decimal::from(500),
        );

        let bridge = CartBridge::new(service);
        let customer = bridge.get_customer("LYD", "en").unwrap();

        assert_eq!(customer.id, "cust-001");
        assert_eq!(customer.name, "John Doe");
        assert_eq!(customer.balance, "500.000 LYD");
    }

    #[test]
    fn test_get_customer_none() {
        let service = Arc::new(CartService::new());
        let bridge = CartBridge::new(service);

        assert!(bridge.get_customer("LYD", "en").is_none());
    }

    #[test]
    fn test_format_quantity_decimal() {
        assert_eq!(format_quantity_decimal(Decimal::from(5)), "5");
        assert_eq!(format_quantity_decimal(Decimal::new(25, 1)), "2.500");
        assert_eq!(format_quantity_decimal(Decimal::new(1123, 3)), "1.123");
    }

    #[test]
    fn test_grand_total_returns_decimal() {
        let service = Arc::new(CartService::new());
        let product = create_test_product("p1", "Test Product", Decimal::from(100));
        service.add_item(&product, Decimal::ONE).unwrap();

        let bridge = CartBridge::new(service);
        assert_eq!(bridge.grand_total(), Decimal::from(115));
    }

    #[test]
    fn test_item_with_discount() {
        let service = Arc::new(CartService::new());
        let product = create_test_product("p1", "Test Product", Decimal::from(100));
        let item_id = service.add_item(&product, Decimal::ONE).unwrap();
        service.apply_item_discount(&item_id, Decimal::from(10)).unwrap();

        let bridge = CartBridge::new(service);
        let items = bridge.get_items_model("en", "LYD");

        assert!(items[0].has_discount);
        assert_eq!(items[0].discount_label, "-10%");
    }

    #[test]
    fn test_item_with_note() {
        let service = Arc::new(CartService::new());
        let product = create_test_product("p1", "Test Product", Decimal::from(10));
        let item_id = service.add_item(&product, Decimal::ONE).unwrap();
        service
            .set_item_note(&item_id, Some("No ice".to_string()))
            .unwrap();

        let bridge = CartBridge::new(service);
        let items = bridge.get_items_model("en", "LYD");

        assert!(items[0].has_note);
        assert_eq!(items[0].note, "No ice");
    }

    #[test]
    fn test_bridge_clone_shares_service() {
        let service = Arc::new(CartService::new());
        let bridge1 = CartBridge::new(Arc::clone(&service));
        let bridge2 = bridge1.clone();

        let product = create_test_product("p1", "Test Product", Decimal::from(10));
        service.add_item(&product, Decimal::ONE).unwrap();

        assert_eq!(bridge1.item_count(), 1);
        assert_eq!(bridge2.item_count(), 1);
    }
}
