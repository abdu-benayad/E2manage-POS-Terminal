//! E2E Sale Workflow Tests
//!
//! Complete end-to-end tests for sale transactions including:
//! - Cash sale workflow
//! - Split payment workflow
//! - Card payment workflow
//! - Multiple item transactions

mod common;

use common::TestApp;
use e2manage_pos_terminal::models::transaction::TransactionStatus;
use rust_decimal::Decimal;

// ============================================================================
// Complete Cash Sale Workflow
// ============================================================================

#[test]
fn test_complete_cash_sale_workflow() {
    let app = TestApp::new();
    app.seed_test_data();

    // 1. VERIFY OPERATOR PIN (simulates login)
    let pin_result = app.auth_service.verify_pin_sync("op-001", "1234");
    assert!(pin_result.is_ok());
    let pin_result = pin_result.unwrap();
    assert!(pin_result.valid);
    assert_eq!(pin_result.operator_name, "أحمد محمد");
    assert_eq!(pin_result.operator_role, "cashier");

    // 2. CREATE SHIFT (using direct DB to avoid async)
    let shift_id = app.create_shift("shift-001", "op-001", Decimal::from(500));
    assert!(!shift_id.is_empty());

    // 3. SEARCH PRODUCT
    let search_result = app.product_service.search("SKU001", 10);
    assert!(search_result.is_ok());
    let search_result = search_result.unwrap();
    assert_eq!(search_result.products.len(), 1);
    let product = &search_result.products[0];
    assert_eq!(product.sku, "SKU001");

    // 4. ADD TO CART
    let add_result = app.cart_service.add_item(product, Decimal::from(2));
    assert!(add_result.is_ok());

    let cart = app.cart_service.get_cart();
    assert_eq!(cart.items.len(), 1);
    assert_eq!(cart.items[0].quantity, Decimal::from(2));
    assert!(cart.grand_total > Decimal::ZERO);

    // 5. CREATE TRANSACTION
    let txn_result = app.transaction_service.create_from_cart(
        &cart,
        "LYD",
        &shift_id,
        "TERM-001",
        "op-001",
        "أحمد محمد",
    );
    assert!(txn_result.is_ok());

    let txn = app.transaction_service.get_current();
    assert!(txn.is_some());
    let txn = txn.unwrap();
    assert_eq!(txn.items.len(), 1);
    assert_eq!(txn.status, TransactionStatus::Draft);

    // 6. PROCESS CASH PAYMENT
    let pay_amount = txn.grand_total + Decimal::from(5); // Pay with extra for change
    let pay_result = app.transaction_service.add_cash_payment(pay_amount);
    assert!(pay_result.is_ok());

    // Verify payment applied
    let txn = app.transaction_service.get_current().unwrap();
    assert!(txn.is_fully_paid());
    assert_eq!(txn.change_due(), Decimal::from(5));

    // 7. VERIFY TRANSACTION STATE
    assert!(!txn.transaction_number.is_empty());
    assert_eq!(txn.payments.len(), 1);
    assert_eq!(txn.operator_id, "op-001");
    assert_eq!(txn.shift_id, shift_id);

    // 8. CLEAR CART FOR NEXT CUSTOMER
    app.cart_service.clear();
    assert!(app.cart_service.is_empty());

    println!("✓ Complete cash sale workflow passed");
}

// ============================================================================
// Split Payment Workflow
// ============================================================================

#[test]
fn test_split_payment_workflow() {
    let app = TestApp::new();
    app.seed_test_data();

    // Setup: Login and create shift
    let pin_result = app.auth_service.verify_pin_sync("op-001", "1234").unwrap();
    assert!(pin_result.valid);

    let shift_id = app.create_shift("shift-002", "op-001", Decimal::from(500));

    // Add multiple products to cart (use SKU to avoid FTS5 hyphen issue)
    let result1 = app.product_service.search("SKU001", 10).unwrap();
    let result2 = app.product_service.search("SKU002", 10).unwrap();
    let result3 = app.product_service.search("SKU003", 10).unwrap();

    app.cart_service
        .add_item(&result1.products[0], Decimal::ONE)
        .unwrap();
    app.cart_service
        .add_item(&result2.products[0], Decimal::ONE)
        .unwrap();
    app.cart_service
        .add_item(&result3.products[0], Decimal::ONE)
        .unwrap();

    let cart = app.cart_service.get_cart();
    assert_eq!(cart.items.len(), 3);

    // Create transaction
    app.transaction_service
        .create_from_cart(&cart, "LYD", &shift_id, "TERM-001", "op-001", "Operator")
        .unwrap();

    let txn = app.transaction_service.get_current().unwrap();
    let total = txn.grand_total;

    // Split payment: 60% cash, 40% card
    let cash_amount = (total * Decimal::new(6, 1)).round();
    let card_amount = total - cash_amount;

    // First payment: Cash
    app.transaction_service
        .add_cash_payment(cash_amount)
        .unwrap();

    let txn = app.transaction_service.get_current().unwrap();
    assert!(!txn.is_fully_paid()); // Not fully paid yet

    // Second payment: Card
    app.transaction_service
        .add_card_payment(card_amount, "4242", "Visa", "AUTH123")
        .unwrap();

    let txn = app.transaction_service.get_current().unwrap();
    assert!(txn.is_fully_paid());
    assert_eq!(txn.payments.len(), 2);

    println!("✓ Split payment workflow passed");
}

// ============================================================================
// Card Payment Workflow
// ============================================================================

#[test]
fn test_card_only_payment_workflow() {
    let app = TestApp::new();
    app.seed_test_data();

    let shift_id = app.create_shift("shift-003", "op-001", Decimal::from(500));

    // Search and add product
    let result = app.product_service.search("SKU005", 10).unwrap();
    app.cart_service
        .add_item(&result.products[0], Decimal::ONE)
        .unwrap();

    let cart = app.cart_service.get_cart();

    // Create transaction
    app.transaction_service
        .create_from_cart(&cart, "LYD", &shift_id, "TERM-001", "op-001", "Operator")
        .unwrap();

    let txn = app.transaction_service.get_current().unwrap();
    let total = txn.grand_total;

    // Pay entire amount with card
    app.transaction_service
        .add_card_payment(total, "5678", "MasterCard", "AUTH456")
        .unwrap();

    let txn = app.transaction_service.get_current().unwrap();
    assert!(txn.is_fully_paid());
    assert_eq!(txn.payments.len(), 1);
    assert_eq!(txn.change_due(), Decimal::ZERO); // No change for card

    println!("✓ Card only payment workflow passed");
}

// ============================================================================
// Multiple Items Transaction
// ============================================================================

#[test]
fn test_multiple_items_transaction() {
    let app = TestApp::new();
    app.seed_test_data();

    let shift_id = app.create_shift("shift-004", "op-001", Decimal::from(500));

    // Add products with different quantities (use SKU to avoid FTS5 hyphen issue)
    for i in 1..=5 {
        let result = app
            .product_service
            .search(&format!("SKU{:03}", i), 10)
            .unwrap();
        if !result.products.is_empty() {
            app.cart_service
                .add_item(&result.products[0], Decimal::from(i))
                .unwrap();
        }
    }

    let cart = app.cart_service.get_cart();
    assert_eq!(cart.items.len(), 5);

    // Verify cart totals
    assert!(cart.subtotal > Decimal::ZERO);
    assert!(cart.tax_total > Decimal::ZERO);
    assert!(cart.grand_total > cart.subtotal); // With tax

    // Create and pay transaction
    app.transaction_service
        .create_from_cart(&cart, "LYD", &shift_id, "TERM-001", "op-001", "Operator")
        .unwrap();

    let txn = app.transaction_service.get_current().unwrap();
    app.transaction_service
        .add_cash_payment(txn.grand_total)
        .unwrap();

    let txn = app.transaction_service.get_current().unwrap();
    assert!(txn.is_fully_paid());
    assert_eq!(txn.items.len(), 5);

    println!("✓ Multiple items transaction passed");
}

// ============================================================================
// Barcode Lookup Workflow
// ============================================================================

#[test]
fn test_barcode_lookup_and_sale() {
    let app = TestApp::new();
    app.seed_test_data();

    let shift_id = app.create_shift("shift-005", "op-001", Decimal::from(500));

    // Lookup by barcode (barcode format: 1000000000001 for prod-001)
    let result = app.product_service.lookup_barcode("1000000000001");
    assert!(result.is_ok());
    let product = result.unwrap();
    assert!(product.is_some());
    let product = product.unwrap();
    assert_eq!(product.id, "prod-001");

    // Add to cart and process sale
    app.cart_service.add_item(&product, Decimal::ONE).unwrap();
    let cart = app.cart_service.get_cart();

    app.transaction_service
        .create_from_cart(&cart, "LYD", &shift_id, "TERM-001", "op-001", "Operator")
        .unwrap();

    let txn = app.transaction_service.get_current().unwrap();
    app.transaction_service
        .add_cash_payment(txn.grand_total)
        .unwrap();

    assert!(app
        .transaction_service
        .get_current()
        .unwrap()
        .is_fully_paid());

    println!("✓ Barcode lookup and sale passed");
}

// ============================================================================
// Discount Application Workflow
// ============================================================================

#[test]
fn test_discount_application_workflow() {
    let app = TestApp::new();
    app.seed_test_data();

    let shift_id = app.create_shift("shift-006", "op-001", Decimal::from(500));

    // Add product
    let result = app.product_service.search("SKU001", 10).unwrap();
    app.cart_service
        .add_item(&result.products[0], Decimal::ONE)
        .unwrap();

    let cart_before = app.cart_service.get_cart();
    let original_total = cart_before.grand_total;

    // Apply 10% cart discount
    app.cart_service
        .apply_cart_discount(Decimal::from(10))
        .unwrap();

    let cart_after = app.cart_service.get_cart();
    assert!(cart_after.grand_total < original_total);
    assert!(cart_after.discount_total > Decimal::ZERO);

    // Complete transaction
    app.transaction_service
        .create_from_cart(
            &cart_after,
            "LYD",
            &shift_id,
            "TERM-001",
            "op-001",
            "Operator",
        )
        .unwrap();

    let txn = app.transaction_service.get_current().unwrap();
    app.transaction_service
        .add_cash_payment(txn.grand_total)
        .unwrap();

    assert!(app
        .transaction_service
        .get_current()
        .unwrap()
        .is_fully_paid());

    println!("✓ Discount application workflow passed");
}

// ============================================================================
// Add Same Item Multiple Times
// ============================================================================

#[test]
fn test_add_same_item_increases_quantity() {
    let app = TestApp::new();
    app.seed_test_data();

    let result = app.product_service.search("SKU001", 10).unwrap();
    let product = &result.products[0];

    // Add same product multiple times
    app.cart_service.add_item(product, Decimal::ONE).unwrap();
    app.cart_service
        .add_item(product, Decimal::from(2))
        .unwrap();
    app.cart_service.add_item(product, Decimal::ONE).unwrap();

    let cart = app.cart_service.get_cart();

    // Should consolidate into single item with quantity 4
    assert_eq!(cart.items.len(), 1);
    assert_eq!(cart.items[0].quantity, Decimal::from(4));

    println!("✓ Add same item increases quantity passed");
}

// ============================================================================
// Update Item Quantity
// ============================================================================

#[test]
fn test_update_item_quantity() {
    let app = TestApp::new();
    app.seed_test_data();

    let result = app.product_service.search("SKU001", 10).unwrap();
    let product = &result.products[0];

    app.cart_service
        .add_item(product, Decimal::from(2))
        .unwrap();
    let cart = app.cart_service.get_cart();
    let item_id = &cart.items[0].id;

    // Update quantity
    app.cart_service
        .update_quantity(item_id, Decimal::from(5))
        .unwrap();

    let cart = app.cart_service.get_cart();
    assert_eq!(cart.items[0].quantity, Decimal::from(5));

    println!("✓ Update item quantity passed");
}

// ============================================================================
// Remove Item From Cart
// ============================================================================

#[test]
fn test_remove_item_from_cart() {
    let app = TestApp::new();
    app.seed_test_data();

    // Add two products (use SKU to avoid FTS5 hyphen issue)
    let result1 = app.product_service.search("SKU001", 10).unwrap();
    let result2 = app.product_service.search("SKU002", 10).unwrap();

    app.cart_service
        .add_item(&result1.products[0], Decimal::ONE)
        .unwrap();
    app.cart_service
        .add_item(&result2.products[0], Decimal::ONE)
        .unwrap();

    let cart = app.cart_service.get_cart();
    assert_eq!(cart.items.len(), 2);

    // Remove first item
    let item_id = cart.items[0].id.clone();
    app.cart_service.remove_item(&item_id).unwrap();

    let cart = app.cart_service.get_cart();
    assert_eq!(cart.items.len(), 1);

    println!("✓ Remove item from cart passed");
}
