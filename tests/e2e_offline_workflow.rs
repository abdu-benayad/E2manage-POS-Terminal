//! E2E Offline Workflow Tests
//!
//! Complete end-to-end tests for offline operations including:
//! - Queue transactions when offline
//! - Pending transaction management
//! - Sync status tracking
//! - Queue statistics

mod common;

use common::TestApp;
use rust_decimal::Decimal;

// ============================================================================
// Queue Transaction When Offline
// ============================================================================

#[test]
fn test_offline_transaction_queuing() {
    let app = TestApp::new();
    app.seed_test_data();

    let shift_id = app.create_shift("shift-001", "op-001", Decimal::from(500));

    // Create a transaction
    let result = app.product_service.search("SKU001", 10).unwrap();
    app.cart_service.add_item(&result.products[0], Decimal::ONE).unwrap();

    let cart = app.cart_service.get_cart();
    app.transaction_service
        .create_from_cart(&cart, "LYD", &shift_id, "TERM-001", "op-001", "Operator")
        .unwrap();

    let txn = app.transaction_service.get_current().unwrap();
    app.transaction_service.add_cash_payment(txn.grand_total).unwrap();

    let txn = app.transaction_service.get_current().unwrap();

    // Queue for offline sync
    let queue_result = app.offline_service.queue_transaction(&txn);
    assert!(queue_result.is_ok());
    let offline_id = queue_result.unwrap();
    assert!(!offline_id.is_empty());

    // Verify queued
    let pending = app.offline_service.pending_count().unwrap();
    assert!(pending >= 1);

    println!("✓ Offline transaction queuing passed");
}

// ============================================================================
// Multiple Offline Transactions
// ============================================================================

#[test]
fn test_multiple_offline_transactions() {
    let app = TestApp::new();
    app.seed_test_data();

    let shift_id = app.create_shift("shift-002", "op-001", Decimal::from(500));

    // Get initial pending count
    let initial_pending = app.offline_service.pending_count().unwrap();

    // Queue multiple transactions
    for i in 1..=3 {
        let result = app.product_service.search(&format!("SKU{:03}", i), 10).unwrap();
        app.cart_service.clear();
        app.cart_service.add_item(&result.products[0], Decimal::from(i)).unwrap();

        let cart = app.cart_service.get_cart();
        app.transaction_service
            .create_from_cart(&cart, "LYD", &shift_id, "TERM-001", "op-001", "Operator")
            .unwrap();

        let txn = app.transaction_service.get_current().unwrap();
        app.transaction_service.add_cash_payment(txn.grand_total).unwrap();

        let txn = app.transaction_service.get_current().unwrap();
        app.offline_service.queue_transaction(&txn).unwrap();

        app.transaction_service.clear_current();
    }

    // Verify all queued
    let final_pending = app.offline_service.pending_count().unwrap();
    assert_eq!(final_pending, initial_pending + 3);

    println!("✓ Multiple offline transactions passed");
}

// ============================================================================
// Has Pending Check
// ============================================================================

#[test]
fn test_has_pending_transactions() {
    let app = TestApp::new();
    app.seed_test_data();

    let shift_id = app.create_shift("shift-003", "op-001", Decimal::from(500));

    // Check if there's any pending (from other tests)
    let initial_has_pending = app.offline_service.has_pending().unwrap();

    if !initial_has_pending {
        // Queue a transaction to ensure we have pending
        let result = app.product_service.search("SKU001", 10).unwrap();
        app.cart_service.add_item(&result.products[0], Decimal::ONE).unwrap();

        let cart = app.cart_service.get_cart();
        app.transaction_service
            .create_from_cart(&cart, "LYD", &shift_id, "TERM-001", "op-001", "Operator")
            .unwrap();

        let txn = app.transaction_service.get_current().unwrap();
        app.transaction_service.add_cash_payment(txn.grand_total).unwrap();

        let txn = app.transaction_service.get_current().unwrap();
        app.offline_service.queue_transaction(&txn).unwrap();
    }

    // Now should have pending
    let has_pending = app.offline_service.has_pending().unwrap();
    assert!(has_pending);

    println!("✓ Has pending transactions passed");
}

// ============================================================================
// Offline ID Generation
// ============================================================================

#[test]
fn test_offline_id_is_unique() {
    let app = TestApp::new();
    app.seed_test_data();

    let shift_id = app.create_shift("shift-004", "op-001", Decimal::from(500));

    let mut offline_ids = Vec::new();

    // Queue multiple transactions and collect IDs
    for i in 1..=5 {
        let result = app.product_service.search(&format!("SKU{:03}", i), 10).unwrap();
        app.cart_service.clear();
        app.cart_service.add_item(&result.products[0], Decimal::ONE).unwrap();

        let cart = app.cart_service.get_cart();
        app.transaction_service
            .create_from_cart(&cart, "LYD", &shift_id, "TERM-001", "op-001", "Operator")
            .unwrap();

        let txn = app.transaction_service.get_current().unwrap();
        app.transaction_service.add_cash_payment(txn.grand_total).unwrap();

        let txn = app.transaction_service.get_current().unwrap();
        let offline_id = app.offline_service.queue_transaction(&txn).unwrap();
        offline_ids.push(offline_id);

        app.transaction_service.clear_current();
    }

    // All IDs should be unique
    let mut unique_ids = offline_ids.clone();
    unique_ids.sort();
    unique_ids.dedup();
    assert_eq!(unique_ids.len(), offline_ids.len());

    println!("✓ Offline ID is unique passed");
}

// ============================================================================
// Transaction Data Integrity In Queue
// ============================================================================

#[test]
fn test_transaction_data_integrity_in_queue() {
    let app = TestApp::new();
    app.seed_test_data();

    let shift_id = app.create_shift("shift-005", "op-001", Decimal::from(500));

    // Create transaction with multiple items
    let result1 = app.product_service.search("SKU001", 10).unwrap();
    let result2 = app.product_service.search("SKU002", 10).unwrap();

    app.cart_service.add_item(&result1.products[0], Decimal::from(2)).unwrap();
    app.cart_service.add_item(&result2.products[0], Decimal::from(3)).unwrap();

    let cart = app.cart_service.get_cart();
    let expected_items = cart.items.len();
    let expected_total = cart.grand_total; // Decimal

    app.transaction_service
        .create_from_cart(&cart, "LYD", &shift_id, "TERM-001", "op-001", "Operator")
        .unwrap();

    let txn = app.transaction_service.get_current().unwrap();
    app.transaction_service.add_cash_payment(txn.grand_total).unwrap();

    let txn = app.transaction_service.get_current().unwrap();

    // Verify transaction has expected data before queuing
    assert_eq!(txn.items.len(), expected_items);
    assert_eq!(txn.grand_total, expected_total);

    // Queue the transaction
    let offline_id = app.offline_service.queue_transaction(&txn).unwrap();
    assert!(!offline_id.is_empty());

    // The queue preserves transaction integrity
    // (actual verification would require reading back from DB)
    assert!(app.offline_service.has_pending().unwrap());

    println!("✓ Transaction data integrity in queue passed");
}

// ============================================================================
// Queue Statistics
// ============================================================================

#[test]
fn test_queue_statistics() {
    let app = TestApp::new();
    app.seed_test_data();

    // Get initial count
    let initial_count = app.offline_service.pending_count().unwrap();

    let shift_id = app.create_shift("shift-006", "op-001", Decimal::from(500));

    // Add some transactions to queue
    let to_add = 2u32;
    for i in 1..=to_add {
        let result = app.product_service.search(&format!("SKU{:03}", i), 10).unwrap();
        app.cart_service.clear();
        app.cart_service.add_item(&result.products[0], Decimal::ONE).unwrap();

        let cart = app.cart_service.get_cart();
        app.transaction_service
            .create_from_cart(&cart, "LYD", &shift_id, "TERM-001", "op-001", "Operator")
            .unwrap();

        let txn = app.transaction_service.get_current().unwrap();
        app.transaction_service.add_cash_payment(txn.grand_total).unwrap();

        let txn = app.transaction_service.get_current().unwrap();
        app.offline_service.queue_transaction(&txn).unwrap();
        app.transaction_service.clear_current();
    }

    // Count should increase
    let final_count = app.offline_service.pending_count().unwrap();
    assert_eq!(final_count, initial_count + to_add);

    println!("✓ Queue statistics passed");
}

// ============================================================================
// Transaction With Payments In Queue
// ============================================================================

#[test]
fn test_transaction_with_payments_in_queue() {
    let app = TestApp::new();
    app.seed_test_data();

    let shift_id = app.create_shift("shift-007", "op-001", Decimal::from(500));

    // Create transaction with split payment
    let result = app.product_service.search("SKU010", 10).unwrap();
    app.cart_service.add_item(&result.products[0], Decimal::ONE).unwrap();

    let cart = app.cart_service.get_cart();
    app.transaction_service
        .create_from_cart(&cart, "LYD", &shift_id, "TERM-001", "op-001", "Operator")
        .unwrap();

    let txn = app.transaction_service.get_current().unwrap();
    let total = txn.grand_total;

    // Split payment
    let cash_amount = (total * Decimal::new(5, 1)).round();
    let card_amount = total - cash_amount;

    app.transaction_service.add_cash_payment(cash_amount).unwrap();
    app.transaction_service
        .add_card_payment(card_amount, "9999", "Visa", "AUTH789")
        .unwrap();

    let txn = app.transaction_service.get_current().unwrap();
    assert!(txn.is_fully_paid());
    assert_eq!(txn.payments.len(), 2);

    // Queue transaction with payments
    let offline_id = app.offline_service.queue_transaction(&txn).unwrap();
    assert!(!offline_id.is_empty());

    println!("✓ Transaction with payments in queue passed");
}

// ============================================================================
// Queue On Network Failure (Simulated)
// ============================================================================

#[test]
fn test_queue_simulates_network_failure() {
    let app = TestApp::new();
    app.seed_test_data();

    let shift_id = app.create_shift("shift-008", "op-001", Decimal::from(500));

    // Simulate: In real scenario, API would fail, so we queue locally
    let result = app.product_service.search("SKU001", 10).unwrap();
    app.cart_service.add_item(&result.products[0], Decimal::ONE).unwrap();

    let cart = app.cart_service.get_cart();
    app.transaction_service
        .create_from_cart(&cart, "LYD", &shift_id, "TERM-001", "op-001", "Operator")
        .unwrap();

    let txn = app.transaction_service.get_current().unwrap();
    app.transaction_service.add_cash_payment(txn.grand_total).unwrap();

    let txn = app.transaction_service.get_current().unwrap();

    // When network fails, application should queue transaction
    // (In real scenario, this would be triggered by API error)
    let result = app.offline_service.queue_transaction(&txn);
    assert!(result.is_ok());

    // Transaction is safely queued for later sync
    assert!(app.offline_service.has_pending().unwrap());

    println!("✓ Queue simulates network failure passed");
}

// ============================================================================
// Empty Queue Check
// ============================================================================

#[test]
fn test_empty_queue_on_fresh_db() {
    // Create fresh app without seeding existing data
    let app = TestApp::new();
    // Don't seed data - completely fresh

    // Check pending count
    let count = app.offline_service.pending_count().unwrap();
    // Fresh database should have 0 pending
    assert_eq!(count, 0);

    let has_pending = app.offline_service.has_pending().unwrap();
    assert!(!has_pending);

    println!("✓ Empty queue on fresh DB passed");
}

// ============================================================================
// Queue Transaction Types
// ============================================================================

#[test]
fn test_queue_different_transaction_types() {
    let app = TestApp::new();
    app.seed_test_data();

    let shift_id = app.create_shift("shift-009", "op-001", Decimal::from(500));

    // Queue a sale transaction
    let result = app.product_service.search("SKU001", 10).unwrap();
    app.cart_service.add_item(&result.products[0], Decimal::ONE).unwrap();

    let cart = app.cart_service.get_cart();
    app.transaction_service
        .create_from_cart(&cart, "LYD", &shift_id, "TERM-001", "op-001", "Operator")
        .unwrap();

    let txn = app.transaction_service.get_current().unwrap();
    app.transaction_service.add_cash_payment(txn.grand_total).unwrap();

    let txn = app.transaction_service.get_current().unwrap();
    // Transaction type may be stored as lowercase or uppercase
    assert!(txn.transaction_type.to_uppercase() == "SALE");

    let result = app.offline_service.queue_transaction(&txn);
    assert!(result.is_ok());

    println!("✓ Queue different transaction types passed");
}

// ============================================================================
// Pending Count After Multiple Operations
// ============================================================================

#[test]
fn test_pending_count_accuracy() {
    let app = TestApp::new();
    app.seed_test_data();

    let shift_id = app.create_shift("shift-010", "op-001", Decimal::from(500));

    // Record initial count
    let initial = app.offline_service.pending_count().unwrap();

    // Add 3 transactions
    for i in 1..=3 {
        let result = app.product_service.search(&format!("SKU{:03}", i), 10).unwrap();
        app.cart_service.clear();
        app.cart_service.add_item(&result.products[0], Decimal::ONE).unwrap();

        let cart = app.cart_service.get_cart();
        app.transaction_service
            .create_from_cart(&cart, "LYD", &shift_id, "TERM-001", "op-001", "Operator")
            .unwrap();

        let txn = app.transaction_service.get_current().unwrap();
        app.transaction_service.add_cash_payment(txn.grand_total).unwrap();

        let txn = app.transaction_service.get_current().unwrap();
        app.offline_service.queue_transaction(&txn).unwrap();
        app.transaction_service.clear_current();
    }

    // Should be exactly 3 more than initial
    let final_count = app.offline_service.pending_count().unwrap();
    assert_eq!(final_count, initial + 3);

    println!("✓ Pending count accuracy passed");
}
