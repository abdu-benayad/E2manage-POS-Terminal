//! E2E Draft/Hold Workflow Tests
//!
//! Complete end-to-end tests for draft (hold) operations including:
//! - Save cart as draft
//! - Recall draft
//! - List drafts
//! - Delete drafts
//! - Draft expiry handling

mod common;

use common::TestApp;
use rust_decimal::Decimal;

// ============================================================================
// Hold and Recall Workflow
// ============================================================================

#[test]
fn test_hold_and_recall_workflow() {
    let app = TestApp::new();
    app.seed_test_data();

    // Create shift for operator
    let shift_id = app.create_shift("shift-001", "op-001", Decimal::from(500));

    // Add items to cart
    let result1 = app.product_service.search("SKU001", 10).unwrap();
    let result2 = app.product_service.search("SKU002", 10).unwrap();

    app.cart_service
        .add_item(&result1.products[0], Decimal::from(2))
        .unwrap();
    app.cart_service
        .add_item(&result2.products[0], Decimal::ONE)
        .unwrap();

    let cart = app.cart_service.get_cart();
    let original_total = cart.grand_total;
    let original_items_count = cart.items.len();

    // 1. SAVE AS DRAFT
    let draft = app.draft_service.save_cart(
        &cart,
        Some("Customer Order #1".to_string()),
        "op-001",
        &shift_id,
    );
    assert!(draft.is_ok());
    let draft = draft.unwrap();
    let draft_id = draft.id.clone();

    assert_eq!(draft.name, Some("Customer Order #1".to_string()));
    assert!(!draft.id.is_empty());

    // 2. CLEAR CART FOR NEXT CUSTOMER
    app.cart_service.clear();
    assert!(app.cart_service.is_empty());

    // 3. PROCESS ANOTHER CUSTOMER
    let result3 = app.product_service.search("SKU005", 10).unwrap();
    app.cart_service
        .add_item(&result3.products[0], Decimal::from(3))
        .unwrap();
    assert_eq!(app.cart_service.get_cart().items.len(), 1);
    app.cart_service.clear();

    // 4. RECALL DRAFT
    let recalled = app.draft_service.get(&draft_id);
    assert!(recalled.is_ok());
    let recalled = recalled.unwrap();
    assert_eq!(recalled.name, Some("Customer Order #1".to_string()));
    assert_eq!(recalled.cart.items.len(), original_items_count);
    assert_eq!(recalled.cart.grand_total, original_total);

    // 5. RESTORE TO CART (simulate - verify data integrity)
    for item in &recalled.cart.items {
        assert!(item.quantity > Decimal::ZERO);
        assert!(!item.product_id.is_empty());
    }

    // 6. DELETE DRAFT AFTER USE
    let delete_result = app.draft_service.delete(&draft_id);
    assert!(delete_result.is_ok());

    // Verify draft is deleted
    let get_result = app.draft_service.get(&draft_id);
    assert!(get_result.is_err()); // Should not find deleted draft

    println!("✓ Hold and recall workflow passed");
}

// ============================================================================
// Multiple Drafts Management
// ============================================================================

#[test]
fn test_multiple_drafts_for_operator() {
    let app = TestApp::new();
    app.seed_test_data();

    let shift_id = app.create_shift("shift-002", "op-001", Decimal::from(500));

    // Create multiple drafts
    let mut draft_ids = Vec::new();

    for i in 1..=3 {
        let result = app
            .product_service
            .search(&format!("SKU{:03}", i), 10)
            .unwrap();
        app.cart_service.clear();
        app.cart_service
            .add_item(&result.products[0], Decimal::from(i))
            .unwrap();

        let cart = app.cart_service.get_cart();
        let draft = app
            .draft_service
            .save_cart(&cart, Some(format!("Draft {}", i)), "op-001", &shift_id)
            .unwrap();
        draft_ids.push(draft.id);
    }

    // List all drafts for operator
    let drafts = app.draft_service.list_by_operator("op-001").unwrap();
    assert_eq!(drafts.len(), 3);

    // Each draft should have unique ID
    for (i, id) in draft_ids.iter().enumerate() {
        let draft = app.draft_service.get(id).unwrap();
        assert_eq!(draft.name, Some(format!("Draft {}", i + 1)));
    }

    println!("✓ Multiple drafts for operator passed");
}

// ============================================================================
// Draft Without Name
// ============================================================================

#[test]
fn test_draft_without_name() {
    let app = TestApp::new();
    app.seed_test_data();

    let shift_id = app.create_shift("shift-003", "op-001", Decimal::from(500));

    let result = app.product_service.search("SKU001", 10).unwrap();
    app.cart_service
        .add_item(&result.products[0], Decimal::ONE)
        .unwrap();

    let cart = app.cart_service.get_cart();

    // Save draft without name
    let draft = app
        .draft_service
        .save_cart(&cart, None, "op-001", &shift_id)
        .unwrap();

    assert!(draft.name.is_none());
    assert!(!draft.id.is_empty());

    println!("✓ Draft without name passed");
}

// ============================================================================
// Empty Cart Cannot Be Saved
// ============================================================================

#[test]
fn test_empty_cart_cannot_be_saved() {
    let app = TestApp::new();
    app.seed_test_data();

    let shift_id = app.create_shift("shift-004", "op-001", Decimal::from(500));

    // Cart is empty
    let cart = app.cart_service.get_cart();
    assert!(cart.is_empty());

    // Try to save empty cart
    let result = app
        .draft_service
        .save_cart(&cart, Some("Empty".to_string()), "op-001", &shift_id);

    assert!(result.is_err());

    println!("✓ Empty cart cannot be saved passed");
}

// ============================================================================
// Draft Preserves Cart State
// ============================================================================

#[test]
fn test_draft_preserves_cart_state() {
    let app = TestApp::new();
    app.seed_test_data();

    let shift_id = app.create_shift("shift-005", "op-001", Decimal::from(500));

    // Build a complex cart
    let result1 = app.product_service.search("SKU001", 10).unwrap();
    let result2 = app.product_service.search("SKU002", 10).unwrap();

    app.cart_service
        .add_item(&result1.products[0], Decimal::from(2))
        .unwrap();
    app.cart_service
        .add_item(&result2.products[0], Decimal::from(3))
        .unwrap();

    // Apply discount
    app.cart_service
        .apply_cart_discount(Decimal::from(10))
        .unwrap();

    // Set customer
    app.cart_service.set_customer(
        Some("cust-001".to_string()),
        Some("Test Customer".to_string()),
        Decimal::ZERO,
    );

    let cart = app.cart_service.get_cart();

    // Capture state before saving
    let expected_items = cart.items.len();
    let expected_subtotal = cart.subtotal;
    let expected_discount = cart.discount_total;
    let expected_total = cart.grand_total;
    let expected_customer = cart.customer_name.clone();

    // Save as draft
    let draft = app
        .draft_service
        .save_cart(&cart, Some("Complex Cart".to_string()), "op-001", &shift_id)
        .unwrap();

    // Clear and recall
    app.cart_service.clear();
    let recalled = app.draft_service.get(&draft.id).unwrap();

    // Verify all state preserved
    assert_eq!(recalled.cart.items.len(), expected_items);
    assert_eq!(recalled.cart.subtotal, expected_subtotal);
    assert_eq!(recalled.cart.discount_total, expected_discount);
    assert_eq!(recalled.cart.grand_total, expected_total);
    assert_eq!(recalled.cart.customer_name, expected_customer);

    println!("✓ Draft preserves cart state passed");
}

// ============================================================================
// List All Active Drafts
// ============================================================================

#[test]
fn test_list_all_active_drafts() {
    let app = TestApp::new();
    app.seed_test_data();

    let shift_id = app.create_shift("shift-006", "op-001", Decimal::from(500));

    // Create drafts
    for i in 1..=3 {
        let result = app
            .product_service
            .search(&format!("SKU{:03}", i), 10)
            .unwrap();
        app.cart_service.clear();
        app.cart_service
            .add_item(&result.products[0], Decimal::ONE)
            .unwrap();

        let cart = app.cart_service.get_cart();
        app.draft_service
            .save_cart(
                &cart,
                Some(format!("Active Draft {}", i)),
                "op-001",
                &shift_id,
            )
            .unwrap();
    }

    // List all active drafts
    let drafts = app.draft_service.list_active().unwrap();
    assert!(drafts.len() >= 3); // At least our 3 drafts

    println!("✓ List all active drafts passed");
}

// ============================================================================
// Delete Draft
// ============================================================================

#[test]
fn test_delete_draft() {
    let app = TestApp::new();
    app.seed_test_data();

    let shift_id = app.create_shift("shift-007", "op-001", Decimal::from(500));

    // Create a draft
    let result = app.product_service.search("SKU001", 10).unwrap();
    app.cart_service
        .add_item(&result.products[0], Decimal::ONE)
        .unwrap();

    let cart = app.cart_service.get_cart();
    let draft = app
        .draft_service
        .save_cart(&cart, Some("To Delete".to_string()), "op-001", &shift_id)
        .unwrap();
    let draft_id = draft.id.clone();

    // Verify exists
    assert!(app.draft_service.get(&draft_id).is_ok());

    // Delete
    let delete_result = app.draft_service.delete(&draft_id);
    assert!(delete_result.is_ok());
    assert!(delete_result.unwrap()); // Should return true

    // Verify deleted
    assert!(app.draft_service.get(&draft_id).is_err());

    println!("✓ Delete draft passed");
}

// ============================================================================
// Draft Count Statistics
// ============================================================================

#[test]
fn test_draft_count_statistics() {
    let app = TestApp::new();
    app.seed_test_data();

    let shift_id = app.create_shift("shift-008", "op-001", Decimal::from(500));

    // Initial count
    let initial_count = app.draft_service.count_by_operator("op-001").unwrap_or(0);

    // Create drafts
    for i in 1..=2 {
        let result = app
            .product_service
            .search(&format!("SKU{:03}", i), 10)
            .unwrap();
        app.cart_service.clear();
        app.cart_service
            .add_item(&result.products[0], Decimal::ONE)
            .unwrap();

        let cart = app.cart_service.get_cart();
        app.draft_service
            .save_cart(&cart, None, "op-001", &shift_id)
            .unwrap();
    }

    // Count should increase
    let new_count = app.draft_service.count_by_operator("op-001").unwrap_or(0);
    assert_eq!(new_count, initial_count + 2);

    println!("✓ Draft count statistics passed");
}

// ============================================================================
// Draft with Item Discount
// ============================================================================

#[test]
fn test_draft_with_item_discount() {
    let app = TestApp::new();
    app.seed_test_data();

    let shift_id = app.create_shift("shift-009", "op-001", Decimal::from(500));

    // Add product and apply item discount
    let result = app.product_service.search("SKU001", 10).unwrap();
    app.cart_service
        .add_item(&result.products[0], Decimal::from(2))
        .unwrap();

    let cart = app.cart_service.get_cart();
    let item_id = &cart.items[0].id;
    app.cart_service
        .apply_item_discount(item_id, Decimal::from(15))
        .unwrap();

    let cart = app.cart_service.get_cart();

    // Save as draft
    let draft = app
        .draft_service
        .save_cart(
            &cart,
            Some("Item Discount Draft".to_string()),
            "op-001",
            &shift_id,
        )
        .unwrap();

    // Recall and verify discount preserved
    let recalled = app.draft_service.get(&draft.id).unwrap();
    assert!(recalled.cart.discount_total > Decimal::ZERO);

    println!("✓ Draft with item discount passed");
}
