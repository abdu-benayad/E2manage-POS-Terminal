//! Cart Service Integration Tests
//!
//! Tests for shopping cart operations including adding items,
//! updating quantities, applying discounts, and calculating totals.

mod common;

use common::*;
use e2manage_pos_terminal::services::cart_service::CartService;
use rust_decimal::Decimal;

// ============================================================================
// Cart Initialization Tests
// ============================================================================

#[test]
fn test_cart_service_new() {
    let service = CartService::new();
    assert!(service.is_empty());
    assert_eq!(service.item_count(), Decimal::ZERO);
    assert_eq!(service.line_count(), 0);
}

// ============================================================================
// Add Item Tests
// ============================================================================

#[test]
fn test_add_item_to_cart() {
    let service = CartService::new();
    let product = sample_product();

    let result = service.add_item(&product, Decimal::from(2));
    assert!(result.is_ok());

    assert!(!service.is_empty());
    assert_eq!(service.item_count(), Decimal::from(2));
    assert_eq!(service.line_count(), 1);
}

#[test]
fn test_add_same_product_combines() {
    let service = CartService::new();
    let product = sample_product();

    service.add_item(&product, Decimal::from(2)).unwrap();
    service.add_item(&product, Decimal::from(3)).unwrap();

    assert_eq!(service.item_count(), Decimal::from(5));
    assert_eq!(service.line_count(), 1); // Should combine into one line
}

#[test]
fn test_add_different_products() {
    let service = CartService::new();
    let product1 = create_product("p1", "Product One", Decimal::from(10), Decimal::from(15));
    let product2 = create_product("p2", "Product Two", Decimal::from(20), Decimal::from(15));

    service.add_item(&product1, Decimal::ONE).unwrap();
    service.add_item(&product2, Decimal::ONE).unwrap();

    assert_eq!(service.item_count(), Decimal::from(2));
    assert_eq!(service.line_count(), 2);
}

#[test]
fn test_add_item_invalid_quantity_zero() {
    let service = CartService::new();
    let product = sample_product();

    let result = service.add_item(&product, Decimal::ZERO);
    assert!(result.is_err());
}

#[test]
fn test_add_item_invalid_quantity_negative() {
    let service = CartService::new();
    let product = sample_product();

    let result = service.add_item(&product, Decimal::from(-1));
    assert!(result.is_err());
}

// ============================================================================
// Cart Totals Calculation Tests
// ============================================================================

#[test]
fn test_cart_totals_calculation() {
    let service = CartService::new();
    let product = create_product("p1", "Test Product", Decimal::from(100), Decimal::from(15));

    service.add_item(&product, Decimal::from(2)).unwrap();

    let cart = service.get_cart();

    // Subtotal: 2 * 100 = 200
    assert_eq!(cart.subtotal, Decimal::from(200));

    // Tax: 200 * 0.15 = 30
    assert_eq!(cart.tax_total, Decimal::from(30));

    // Grand total: 200 + 30 = 230
    assert_eq!(cart.grand_total, Decimal::from(230));
}

#[test]
fn test_cart_totals_multiple_items() {
    let service = CartService::new();
    let product1 = create_product("p1", "Product One", Decimal::from(50), Decimal::from(10)); // 10% tax
    let product2 = create_product("p2", "Product Two", Decimal::from(100), Decimal::from(15)); // 15% tax

    service.add_item(&product1, Decimal::from(2)).unwrap(); // 100 + 10 tax = 110
    service.add_item(&product2, Decimal::ONE).unwrap(); // 100 + 15 tax = 115

    let cart = service.get_cart();

    // Subtotal: (50 * 2) + 100 = 200
    assert_eq!(cart.subtotal, Decimal::from(200));

    // Tax: (100 * 0.10) + (100 * 0.15) = 10 + 15 = 25
    assert_eq!(cart.tax_total, Decimal::from(25));

    // Grand total: 200 + 25 = 225
    assert_eq!(cart.grand_total, Decimal::from(225));
}

// ============================================================================
// Update Quantity Tests
// ============================================================================

#[test]
fn test_update_quantity() {
    let service = CartService::new();
    let product = sample_product();
    let item_id = service.add_item(&product, Decimal::from(2)).unwrap();

    service.update_quantity(&item_id, Decimal::from(5)).unwrap();

    assert_eq!(service.item_count(), Decimal::from(5));
}

#[test]
fn test_update_quantity_to_zero_removes() {
    let service = CartService::new();
    let product = sample_product();
    let item_id = service.add_item(&product, Decimal::from(2)).unwrap();

    service.update_quantity(&item_id, Decimal::ZERO).unwrap();

    assert!(service.is_empty());
}

#[test]
fn test_update_quantity_verifies_line_total() {
    let service = CartService::new();
    // Use zero tax so line_total = unit_price * quantity exactly
    let product = create_product("p1", "Test Item", Decimal::from(10), Decimal::ZERO);
    let item_id = service.add_item(&product, Decimal::ONE).unwrap();

    service.update_quantity(&item_id, Decimal::from(5)).unwrap();

    let cart = service.get_cart();
    let item = cart.items.iter().find(|i| i.id == item_id).unwrap();
    assert_eq!(item.quantity, Decimal::from(5));
    // line_total = unit_price * quantity = 10 * 5 = 50
    assert_eq!(item.line_total, Decimal::from(50));
}

#[test]
fn test_update_quantity_negative_errors() {
    let service = CartService::new();
    let product = sample_product();
    let item_id = service.add_item(&product, Decimal::from(2)).unwrap();

    let result = service.update_quantity(&item_id, Decimal::from(-1));
    assert!(result.is_err());

    // Cart should remain unchanged
    assert_eq!(service.item_count(), Decimal::from(2));
}

#[test]
fn test_update_quantity_nonexistent_item_errors() {
    let service = CartService::new();
    let product = sample_product();
    service.add_item(&product, Decimal::from(2)).unwrap();

    let result = service.update_quantity("nonexistent-id", Decimal::from(5));
    assert!(result.is_err());
}

#[test]
fn test_increment_decrement() {
    let service = CartService::new();
    let product = sample_product();
    let item_id = service.add_item(&product, Decimal::from(2)).unwrap();

    service.increment(&item_id).unwrap();
    assert_eq!(service.item_count(), Decimal::from(3));

    service.decrement(&item_id).unwrap();
    assert_eq!(service.item_count(), Decimal::from(2));
}

#[test]
fn test_decrement_to_zero_removes() {
    let service = CartService::new();
    let product = sample_product();
    let item_id = service.add_item(&product, Decimal::ONE).unwrap();

    service.decrement(&item_id).unwrap();

    assert!(service.is_empty());
}

// ============================================================================
// Remove Item Tests
// ============================================================================

#[test]
fn test_remove_item() {
    let service = CartService::new();
    let product = sample_product();
    let item_id = service.add_item(&product, Decimal::from(2)).unwrap();

    service.remove_item(&item_id).unwrap();

    assert!(service.is_empty());
}

#[test]
fn test_remove_nonexistent_item() {
    let service = CartService::new();

    let result = service.remove_item("nonexistent");
    assert!(result.is_err());
}

// ============================================================================
// Discount Tests
// ============================================================================

#[test]
fn test_apply_cart_discount_percentage() {
    let service = CartService::new();
    let product = create_product("p1", "Test Product", Decimal::from(100), Decimal::ZERO); // No tax for simplicity

    service.add_item(&product, Decimal::ONE).unwrap();
    service.apply_cart_discount(Decimal::from(10)).unwrap(); // 10% discount

    let cart = service.get_cart();
    // Original: 100, discount: 10, final: 90
    assert_eq!(cart.subtotal, Decimal::from(90));
    assert_eq!(cart.discount_total, Decimal::from(10));
}

#[test]
fn test_apply_item_discount() {
    let service = CartService::new();
    let product = create_product("p1", "Test Product", Decimal::from(100), Decimal::ZERO);
    let item_id = service.add_item(&product, Decimal::ONE).unwrap();

    service.apply_item_discount(&item_id, Decimal::from(20)).unwrap(); // 20% discount on item

    let cart = service.get_cart();
    assert_eq!(cart.discount_total, Decimal::from(20));
}

// ============================================================================
// Clear Cart Tests
// ============================================================================

#[test]
fn test_clear_cart() {
    let service = CartService::new();
    let product = sample_product();

    service.add_item(&product, Decimal::from(5)).unwrap();
    assert!(!service.is_empty());

    service.clear();
    assert!(service.is_empty());
    assert_eq!(service.item_count(), Decimal::ZERO);
}

// ============================================================================
// Customer Assignment Tests
// ============================================================================

#[test]
fn test_set_customer() {
    let service = CartService::new();
    let product = sample_product();
    service.add_item(&product, Decimal::ONE).unwrap();

    service.set_customer(Some("cust-001".to_string()), Some("Test Customer".to_string()), Decimal::ZERO);

    let cart = service.get_cart();
    assert_eq!(cart.customer_id, Some("cust-001".to_string()));
    assert_eq!(cart.customer_name, Some("Test Customer".to_string()));
}

#[test]
fn test_clear_customer() {
    let service = CartService::new();
    let product = sample_product();
    service.add_item(&product, Decimal::ONE).unwrap();

    service.set_customer(Some("cust-001".to_string()), Some("Test Customer".to_string()), Decimal::from(1000));
    service.set_customer(None, None, Decimal::ZERO);

    let cart = service.get_cart();
    assert!(cart.customer_id.is_none());
    assert!(cart.customer_name.is_none());
}

// ============================================================================
// Get Cart State Tests
// ============================================================================

#[test]
fn test_get_cart_returns_snapshot() {
    let service = CartService::new();
    let product = sample_product();
    service.add_item(&product, Decimal::from(2)).unwrap();

    let cart = service.get_cart();
    assert_eq!(cart.items.len(), 1);
    assert_eq!(cart.items[0].quantity, Decimal::from(2));
}

#[test]
fn test_grand_total_accessor() {
    let service = CartService::new();
    let product = create_product("p1", "Test", Decimal::from(100), Decimal::from(15));
    service.add_item(&product, Decimal::ONE).unwrap();

    // 100 + 15% tax = 115
    assert_eq!(service.grand_total(), Decimal::from(115));
}

// ============================================================================
// Cart Clone Tests
// ============================================================================

#[test]
fn test_cart_service_clone() {
    let service = CartService::new();
    let product = sample_product();
    service.add_item(&product, Decimal::from(2)).unwrap();

    let cloned = service.clone();

    // Both should reference the same cart state
    assert_eq!(service.item_count(), cloned.item_count());
    assert_eq!(service.grand_total(), cloned.grand_total());

    // Changes through one should reflect in the other
    service.add_item(&product, Decimal::ONE).unwrap();
    assert_eq!(cloned.item_count(), Decimal::from(3));
}

// ============================================================================
// Void Last Item Tests
// ============================================================================

#[test]
fn test_void_last_item_removes_last() {
    let service = CartService::new();
    let p1 = create_product("p1", "Apple", Decimal::from(5), Decimal::ZERO);
    let p2 = create_product("p2", "Banana", Decimal::from(8), Decimal::ZERO);
    let p3 = create_product("p3", "Cherry", Decimal::from(12), Decimal::ZERO);

    service.add_item(&p1, Decimal::ONE).unwrap();
    service.add_item(&p2, Decimal::ONE).unwrap();
    service.add_item(&p3, Decimal::ONE).unwrap();
    assert_eq!(service.line_count(), 3);

    // Void the last item (Cherry)
    let cart = service.get_cart();
    let last_id = cart.items.last().unwrap().id.clone();
    service.remove_item(&last_id).unwrap();

    // Should have 2 items remaining in original order
    assert_eq!(service.line_count(), 2);
    let cart = service.get_cart();
    assert_eq!(cart.items[0].product_name, "Apple");
    assert_eq!(cart.items[1].product_name, "Banana");
}

#[test]
fn test_void_last_item_empty_cart() {
    let service = CartService::new();

    // Empty cart — no items to void, verify it's a no-op
    let cart = service.get_cart();
    assert!(cart.items.last().is_none());
    assert!(service.is_empty());
}

#[test]
fn test_void_last_item_single_item_empties_cart() {
    let service = CartService::new();
    let product = create_product("p1", "Solo Item", Decimal::from(25), Decimal::ZERO);

    service.add_item(&product, Decimal::ONE).unwrap();
    assert_eq!(service.line_count(), 1);

    // Void the only item
    let cart = service.get_cart();
    let last_id = cart.items.last().unwrap().id.clone();
    service.remove_item(&last_id).unwrap();

    assert!(service.is_empty());
    assert_eq!(service.line_count(), 0);
    assert_eq!(service.item_count(), Decimal::ZERO);
}
