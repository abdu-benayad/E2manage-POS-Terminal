//! Database Integration Tests
//!
//! Tests for database operations including CRUD for products, operators,
//! shifts, drafts, and offline transactions.

mod common;

use common::*;
use e2manage_pos_terminal::db::{
    OfflineTransactionRow, OperatorRow, ProductRow, ShiftStatus, SyncStatus,
};
use e2manage_pos_terminal::models::OperatorRole;
use rust_decimal::Decimal;
use std::str::FromStr;

// ============================================================================
// Database Initialization Tests
// ============================================================================

#[test]
fn test_database_initialization() {
    let db = setup_test_db();
    // Verify connection works by getting a lock
    let conn = db.connection();
    let _guard = conn.lock();
    // If we get here without panic, connection is working
}

#[test]
fn test_database_empty_state() {
    let db = setup_test_db();

    // Product count should be 0
    let count = db.get_product_count().unwrap();
    assert_eq!(count, 0);

    // Operator list should be empty
    let operators = db.get_operators().unwrap();
    assert!(operators.is_empty());
}

// ============================================================================
// Product CRUD Tests
// ============================================================================

#[test]
fn test_product_create_and_read() {
    let db = setup_test_db();
    let product = sample_product_row();

    // Create
    db.save_product(&product).unwrap();

    // Read by ID
    let found = db.get_product_by_id(&product.id).unwrap();
    assert!(found.is_some());
    let found = found.unwrap();
    assert_eq!(found.name, "Test Product");
    assert_eq!(found.price, Decimal::from(10));
    assert_eq!(found.sku, "SKU001");
}

#[test]
fn test_product_by_barcode() {
    let db = setup_test_db();
    let product = sample_product_row();

    db.save_product(&product).unwrap();

    let found = db.get_product_by_barcode("1234567890123").unwrap();
    assert!(found.is_some());
    let found = found.unwrap();
    assert_eq!(found.name, "Test Product");
}

#[test]
fn test_product_by_barcode_not_found() {
    let db = setup_test_db();

    let found = db.get_product_by_barcode("nonexistent").unwrap();
    assert!(found.is_none());
}

#[test]
fn test_product_search() {
    let db = setup_test_db();

    // Create multiple products
    create_test_products(&db, 10);

    // Search by name
    let results = db.search_products("Product", 50).unwrap();
    assert_eq!(results.len(), 10);

    // Search by specific name
    let results = db.search_products("Product 5", 50).unwrap();
    assert!(!results.is_empty());
}

#[test]
fn test_product_search_by_sku() {
    let db = setup_test_db();

    create_test_products(&db, 10);

    let results = db.search_products("SKU005", 50).unwrap();
    assert_eq!(results.len(), 1);
}

#[test]
fn test_product_search_limit() {
    let db = setup_test_db();

    create_test_products(&db, 20);

    // Search with limit
    let results = db.search_products("Product", 5).unwrap();
    assert_eq!(results.len(), 5);
}

#[test]
fn test_product_update() {
    let db = setup_test_db();
    let mut product = sample_product_row();

    db.save_product(&product).unwrap();

    // Update price
    product.price = Decimal::from(25);
    product.name = "Updated Product".to_string();
    db.save_product(&product).unwrap();

    let found = db.get_product_by_id(&product.id).unwrap().unwrap();
    assert_eq!(found.price, Decimal::from(25));
    assert_eq!(found.name, "Updated Product");
}

#[test]
fn test_product_count() {
    let db = setup_test_db();

    assert_eq!(db.get_product_count().unwrap(), 0);

    create_test_products(&db, 5);

    assert_eq!(db.get_product_count().unwrap(), 5);
}

// ============================================================================
// Operator CRUD Tests
// ============================================================================

#[test]
fn test_operator_create_and_read() {
    let db = setup_test_db();
    let operator = sample_operator_row();

    db.save_operator(&operator).unwrap();

    let found = db.get_operator_by_id(&operator.id).unwrap();
    assert!(found.is_some());
    let found = found.unwrap();
    assert_eq!(found.name.latin(), "Test Operator");
    // The Arabic name round-trips too. `OperatorRow` is the only place in the workspace that
    // holds it, and nothing asserted it survived the store until both columns became one value.
    assert_eq!(found.name.arabic(), Some("مشغل تجريبي"));
    assert_eq!(found.code, "OP001");
}

#[test]
fn test_operator_list() {
    let db = setup_test_db();

    create_test_operator(&db, "op-1", "Operator One");
    create_test_operator(&db, "op-2", "Operator Two");
    create_test_operator(&db, "op-3", "Operator Three");

    let operators = db.get_operators().unwrap();
    assert_eq!(operators.len(), 3);
}

#[test]
fn test_operator_by_code() {
    let db = setup_test_db();

    let operator = OperatorRow {
        id: op_id("op-test"),
        code: "CODE123".to_string(),
        name: op_latin_name("Test Operator"),
        ..sample_operator_row()
    };
    db.save_operator(&operator).unwrap();

    let found = db.get_operator_by_id(&op_id("op-test")).unwrap();
    assert!(found.is_some());
    let found = found.unwrap();
    assert_eq!(found.name.latin(), "Test Operator");
    // An absent Arabic name comes back absent, rather than as the Latin one written twice.
    assert_eq!(found.name.arabic(), None);
}

#[test]
fn test_operator_update() {
    let db = setup_test_db();
    let mut operator = sample_operator_row();

    db.save_operator(&operator).unwrap();

    operator.name = op_latin_name("Updated Name");
    operator.role = OperatorRole::Manager;
    db.save_operator(&operator).unwrap();

    let found = db.get_operator_by_id(&operator.id).unwrap().unwrap();
    assert_eq!(found.name.latin(), "Updated Name");
    assert_eq!(found.role, OperatorRole::Manager);
}

// ============================================================================
// Shift Lifecycle Tests
// ============================================================================

#[test]
fn test_shift_start() {
    let db = setup_test_db();

    // Create operator first (foreign key)
    create_test_operator(&db, "op-1", "Test Operator");

    let shift = db
        .start_shift(
            "shift-1",
            "SHIFT-001",
            &op_id("op-1"),
            Some("TERM-01"),
            Decimal::from(100),
        )
        .unwrap();

    assert_eq!(shift.id, "shift-1");
    assert_eq!(shift.shift_number, "SHIFT-001");
    assert_eq!(shift.operator_id, op_id("op-1"));
    assert_eq!(shift.opening_cash, Decimal::from(100));
    assert_eq!(shift.status, ShiftStatus::Active.as_str());
}

#[test]
fn test_shift_get_by_id() {
    let db = setup_test_db();
    create_test_operator(&db, "op-1", "Test Operator");

    db.start_shift(
        "shift-1",
        "SHIFT-001",
        &op_id("op-1"),
        Some("TERM-01"),
        Decimal::from(100),
    )
    .unwrap();

    let found = db.get_shift_by_id("shift-1").unwrap();
    assert!(found.is_some());
    assert_eq!(found.unwrap().shift_number, "SHIFT-001");
}

#[test]
fn test_shift_end() {
    let db = setup_test_db();
    create_test_operator(&db, "op-1", "Test Operator");

    db.start_shift(
        "shift-1",
        "SHIFT-001",
        &op_id("op-1"),
        Some("TERM-01"),
        Decimal::from(100),
    )
    .unwrap();

    // End shift with counted cash
    db.end_shift(
        "shift-1",
        Decimal::from(250),
        Decimal::from(200),
        Some("Good day"),
    )
    .unwrap();

    let shift = db.get_shift_by_id("shift-1").unwrap().unwrap();
    assert_eq!(shift.status, ShiftStatus::Closed.as_str());
    assert_eq!(shift.closing_cash, Some(Decimal::from(250)));
    assert_eq!(shift.expected_cash, Some(Decimal::from(200)));
    assert_eq!(shift.variance, Some(Decimal::from(50))); // 250 - 200
    assert_eq!(shift.notes, Some("Good day".to_string()));
}

#[test]
fn test_shift_active() {
    let db = setup_test_db();
    create_test_operator(&db, "op-1", "Test Operator");

    db.start_shift(
        "shift-1",
        "SHIFT-001",
        &op_id("op-1"),
        Some("TERM-01"),
        Decimal::from(100),
    )
    .unwrap();

    let active = db.get_active_shift(&op_id("op-1")).unwrap();
    assert!(active.is_some());
    assert_eq!(active.unwrap().id, "shift-1");

    // End the shift
    db.end_shift("shift-1", Decimal::from(100), Decimal::from(100), None)
        .unwrap();

    let active = db.get_active_shift(&op_id("op-1")).unwrap();
    assert!(active.is_none());
}

// ============================================================================
// Draft Tests
// ============================================================================

#[test]
fn test_draft_create() {
    let db = setup_test_db();
    create_test_operator(&db, "op-1", "Test Operator");

    let cart_json = r#"[{"product_id":"p1","qty":2}]"#;
    let draft = db
        .create_draft(
            "d1",
            cart_json,
            Some(&op_id("op-1")),
            Some("shift-1"),
            None,
            None,
            Some(8),
        )
        .unwrap();

    assert!(!draft.id.is_empty());
    assert!(draft.name.is_some());
}

#[test]
fn test_draft_get() {
    let db = setup_test_db();
    create_test_operator(&db, "op-1", "Test Operator");

    let cart_json = r#"[{"product_id":"p1","qty":2}]"#;
    let draft = db
        .create_draft(
            "d1",
            cart_json,
            Some(&op_id("op-1")),
            Some("shift-1"),
            None,
            None,
            Some(8),
        )
        .unwrap();

    let found = db.get_draft_by_id(&draft.id).unwrap();
    assert!(found.is_some());
}

#[test]
fn test_draft_list_by_shift() {
    let db = setup_test_db();
    create_test_operator(&db, "op-1", "Test Operator");

    // Create multiple drafts for same shift
    db.create_draft(
        "d1",
        r#"[]"#,
        Some(&op_id("op-1")),
        Some("shift-1"),
        None,
        None,
        Some(8),
    )
    .unwrap();
    db.create_draft(
        "d2",
        r#"[]"#,
        Some(&op_id("op-1")),
        Some("shift-1"),
        None,
        None,
        Some(8),
    )
    .unwrap();
    db.create_draft(
        "d3",
        r#"[]"#,
        Some(&op_id("op-1")),
        Some("shift-2"),
        None,
        None,
        Some(8),
    )
    .unwrap();

    let shift1_drafts = db.get_drafts_by_shift("shift-1").unwrap();
    assert_eq!(shift1_drafts.len(), 2);

    let shift2_drafts = db.get_drafts_by_shift("shift-2").unwrap();
    assert_eq!(shift2_drafts.len(), 1);
}

#[test]
fn test_draft_delete() {
    let db = setup_test_db();
    create_test_operator(&db, "op-1", "Test Operator");

    let draft = db
        .create_draft(
            "d1",
            r#"[]"#,
            Some(&op_id("op-1")),
            Some("shift-1"),
            None,
            None,
            Some(8),
        )
        .unwrap();
    let draft_id = draft.id.clone();

    // Delete
    db.delete_draft(&draft_id).unwrap();

    let found = db.get_draft_by_id(&draft_id).unwrap();
    assert!(found.is_none());
}

// ============================================================================
// Offline Transaction Queue Tests
// ============================================================================

#[test]
fn test_offline_transaction_save() {
    let db = setup_test_db();

    // Create operator and shift first (foreign keys)
    create_test_operator(&db, "op-001", "Test Operator");
    db.start_shift(
        "shift-001",
        "SHIFT-001",
        &op_id("op-001"),
        Some("TERM-01"),
        Decimal::from(100),
    )
    .unwrap();

    let txn = OfflineTransactionRow {
        offline_id: "off-001".to_string(),
        transaction_number: Some("TXN-001".to_string()),
        transaction_type: "SALE".to_string(),
        items_json: r#"[{"product_id":"p1","qty":2}]"#.to_string(),
        payments_json: r#"[{"method":"CASH","amount":100}]"#.to_string(),
        subtotal: Decimal::from(100),
        tax_total: Decimal::from(15),
        discount_total: Decimal::ZERO,
        grand_total: Decimal::from(115),
        customer_id: None,
        customer_name: None,
        shift_id: Some("shift-001".to_string()),
        operator_id: Some(op_id("op-001")),
        terminal_id: Some("TERM-01".to_string()),
        receipt_number: Some("REC-001".to_string()),
        notes: None,
        created_at: chrono::Utc::now().to_rfc3339(),
        sync_status: SyncStatus::Pending.as_str().to_string(),
        server_id: None,
        retry_count: 0,
        last_error: None,
        last_retry_at: None,
        catalog_etag: None,
    };

    db.save_offline_transaction(&txn).unwrap();

    let found = db.get_offline_transaction("off-001").unwrap();
    assert!(found.is_some());
    let found = found.unwrap();
    assert_eq!(found.grand_total, Decimal::from(115));
}

#[test]
fn test_offline_transaction_pending_count() {
    let db = setup_test_db();

    // Create pending transactions
    for i in 0..5 {
        let txn = OfflineTransactionRow {
            offline_id: format!("off-{:03}", i),
            transaction_number: Some(format!("TXN-{:03}", i)),
            transaction_type: "SALE".to_string(),
            items_json: "[]".to_string(),
            payments_json: "[]".to_string(),
            subtotal: Decimal::from(100),
            tax_total: Decimal::from(15),
            discount_total: Decimal::ZERO,
            grand_total: Decimal::from(115),
            customer_id: None,
            customer_name: None,
            shift_id: None,
            operator_id: None,
            terminal_id: None,
            receipt_number: None,
            notes: None,
            created_at: chrono::Utc::now().to_rfc3339(),
            sync_status: SyncStatus::Pending.as_str().to_string(),
            server_id: None,
            retry_count: 0,
            last_error: None,
            last_retry_at: None,
            catalog_etag: None,
        };
        db.save_offline_transaction(&txn).unwrap();
    }

    let count = db.get_pending_transaction_count().unwrap();
    assert_eq!(count, 5);
}

#[test]
fn test_offline_transaction_mark_synced() {
    let db = setup_test_db();

    let txn = OfflineTransactionRow {
        offline_id: "off-001".to_string(),
        transaction_number: Some("TXN-001".to_string()),
        transaction_type: "SALE".to_string(),
        items_json: "[]".to_string(),
        payments_json: "[]".to_string(),
        subtotal: Decimal::from(100),
        tax_total: Decimal::from(15),
        discount_total: Decimal::ZERO,
        grand_total: Decimal::from(115),
        customer_id: None,
        customer_name: None,
        shift_id: None,
        operator_id: None,
        terminal_id: None,
        receipt_number: None,
        notes: None,
        created_at: chrono::Utc::now().to_rfc3339(),
        sync_status: SyncStatus::Pending.as_str().to_string(),
        server_id: None,
        retry_count: 0,
        last_error: None,
        last_retry_at: None,
        catalog_etag: None,
    };
    db.save_offline_transaction(&txn).unwrap();

    // Initially 1 pending
    assert_eq!(db.get_pending_transaction_count().unwrap(), 1);

    // Mark synced
    db.mark_transaction_synced("off-001", "server-txn-001")
        .unwrap();

    // Now 0 pending
    assert_eq!(db.get_pending_transaction_count().unwrap(), 0);

    // Verify server_id was set
    let found = db.get_offline_transaction("off-001").unwrap().unwrap();
    assert_eq!(found.server_id, Some("server-txn-001".to_string()));
    assert_eq!(found.sync_status, SyncStatus::Synced.as_str());
}

#[test]
fn test_offline_transaction_mark_failed() {
    let db = setup_test_db();

    let txn = OfflineTransactionRow {
        offline_id: "off-001".to_string(),
        transaction_number: None,
        transaction_type: "SALE".to_string(),
        items_json: "[]".to_string(),
        payments_json: "[]".to_string(),
        subtotal: Decimal::from(100),
        tax_total: Decimal::from(15),
        discount_total: Decimal::ZERO,
        grand_total: Decimal::from(115),
        customer_id: None,
        customer_name: None,
        shift_id: None,
        operator_id: None,
        terminal_id: None,
        receipt_number: None,
        notes: None,
        created_at: chrono::Utc::now().to_rfc3339(),
        sync_status: SyncStatus::Pending.as_str().to_string(),
        server_id: None,
        retry_count: 0,
        last_error: None,
        last_retry_at: None,
        catalog_etag: None,
    };
    db.save_offline_transaction(&txn).unwrap();

    // Mark failed
    db.mark_transaction_failed("off-001", "Network error")
        .unwrap();

    let found = db.get_offline_transaction("off-001").unwrap().unwrap();
    assert_eq!(found.sync_status, SyncStatus::Failed.as_str());
    assert_eq!(found.last_error, Some("Network error".to_string()));
    assert_eq!(found.retry_count, 1);
}

#[test]
fn test_offline_transaction_get_pending() {
    let db = setup_test_db();

    // Create some pending and one synced
    for i in 0..3 {
        let txn = OfflineTransactionRow {
            offline_id: format!("off-{:03}", i),
            transaction_number: None,
            transaction_type: "SALE".to_string(),
            items_json: "[]".to_string(),
            payments_json: "[]".to_string(),
            subtotal: Decimal::from(100),
            tax_total: Decimal::from(15),
            discount_total: Decimal::ZERO,
            grand_total: Decimal::from(115),
            customer_id: None,
            customer_name: None,
            shift_id: None,
            operator_id: None,
            terminal_id: None,
            receipt_number: None,
            notes: None,
            created_at: chrono::Utc::now().to_rfc3339(),
            sync_status: SyncStatus::Pending.as_str().to_string(),
            server_id: None,
            retry_count: 0,
            last_error: None,
            last_retry_at: None,
            catalog_etag: None,
        };
        db.save_offline_transaction(&txn).unwrap();
    }

    // Mark one as synced
    db.mark_transaction_synced("off-001", "server-1").unwrap();

    // Get pending (should return 2)
    let pending = db.get_pending_transactions(10).unwrap();
    assert_eq!(pending.len(), 2);
}

#[test]
fn test_offline_transaction_by_shift() {
    let db = setup_test_db();

    // Create operators and shifts first (foreign keys)
    create_test_operator(&db, "op-1", "Test Operator");
    db.start_shift(
        "shift-1",
        "SHIFT-001",
        &op_id("op-1"),
        Some("TERM-01"),
        Decimal::from(100),
    )
    .unwrap();
    db.start_shift(
        "shift-2",
        "SHIFT-002",
        &op_id("op-1"),
        Some("TERM-01"),
        Decimal::from(100),
    )
    .unwrap();

    // Create transactions for different shifts
    for i in 0..3 {
        let txn = OfflineTransactionRow {
            offline_id: format!("off-{:03}", i),
            transaction_number: None,
            transaction_type: "SALE".to_string(),
            items_json: "[]".to_string(),
            payments_json: "[]".to_string(),
            subtotal: Decimal::from(100),
            tax_total: Decimal::from(15),
            discount_total: Decimal::ZERO,
            grand_total: Decimal::from(115),
            customer_id: None,
            customer_name: None,
            shift_id: Some("shift-1".to_string()),
            operator_id: None,
            terminal_id: None,
            receipt_number: None,
            notes: None,
            created_at: chrono::Utc::now().to_rfc3339(),
            sync_status: SyncStatus::Pending.as_str().to_string(),
            server_id: None,
            retry_count: 0,
            last_error: None,
            last_retry_at: None,
            catalog_etag: None,
        };
        db.save_offline_transaction(&txn).unwrap();
    }

    // Add one for different shift
    let txn = OfflineTransactionRow {
        offline_id: "off-other".to_string(),
        transaction_number: None,
        transaction_type: "SALE".to_string(),
        items_json: "[]".to_string(),
        payments_json: "[]".to_string(),
        subtotal: Decimal::from(100),
        tax_total: Decimal::from(15),
        discount_total: Decimal::ZERO,
        grand_total: Decimal::from(115),
        customer_id: None,
        customer_name: None,
        shift_id: Some("shift-2".to_string()),
        operator_id: None,
        terminal_id: None,
        receipt_number: None,
        notes: None,
        created_at: chrono::Utc::now().to_rfc3339(),
        sync_status: SyncStatus::Pending.as_str().to_string(),
        server_id: None,
        retry_count: 0,
        last_error: None,
        last_retry_at: None,
        catalog_etag: None,
    };
    db.save_offline_transaction(&txn).unwrap();

    let shift1_txns = db.get_transactions_by_shift("shift-1").unwrap();
    assert_eq!(shift1_txns.len(), 3);

    let shift2_txns = db.get_transactions_by_shift("shift-2").unwrap();
    assert_eq!(shift2_txns.len(), 1);
}

// ============================================================================
// Full Workflow Test
// ============================================================================

#[test]
fn test_full_database_workflow() {
    let db = setup_test_db();

    // 1. Create products
    let product = ProductRow {
        id: "p1".to_string(),
        sku: "SKU001".to_string(),
        barcode: Some("1234567890123".to_string()),
        name: "Test Product".to_string(),
        name_ar: Some("منتج تجريبي".to_string()),
        price: Decimal::from_str("10.99").unwrap(),
        tax_rate: Decimal::from(15),
        tax_inclusive: false,
        category_id: None,
        stock_qty: 100,
        is_active: true,
        unit: "unit".to_string(),
        ..Default::default()
    };
    db.save_product(&product).unwrap();

    // 2. Create operator
    create_test_operator(&db, "op1", "Ahmed");

    // 3. Start shift
    let shift = db
        .start_shift(
            "s1",
            "SHIFT-001",
            &op_id("op1"),
            Some("TERM-01"),
            Decimal::from(100),
        )
        .unwrap();
    assert_eq!(shift.status, ShiftStatus::Active.as_str());

    // 4. Create a draft
    let draft = db
        .create_draft(
            "d1",
            r#"[{"product_id":"p1","qty":2}]"#,
            Some(&op_id("op1")),
            Some("s1"),
            None,
            None,
            Some(8),
        )
        .unwrap();
    assert!(draft.name.is_some());

    // 5. Search products
    let found = db.search_products("Test", 10).unwrap();
    assert_eq!(found.len(), 1);

    // 6. Get by barcode
    let by_barcode = db.get_product_by_barcode("1234567890123").unwrap();
    assert!(by_barcode.is_some());

    // 7. Delete draft
    db.delete_draft(&draft.id).unwrap();

    // 8. End shift
    db.end_shift(
        "s1",
        Decimal::from(250),
        Decimal::from(200),
        Some("Good day"),
    )
    .unwrap();
    let ended = db.get_shift_by_id("s1").unwrap().unwrap();
    assert_eq!(ended.status, ShiftStatus::Closed.as_str());
    assert_eq!(ended.variance, Some(Decimal::from(50)));
}
