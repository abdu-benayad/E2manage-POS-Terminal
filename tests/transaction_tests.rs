//! Transaction Service Integration Tests
//!
//! Tests for transaction processing including creating transactions from carts,
//! adding payments, completing transactions, and void operations.

mod common;

use common::*;
use e2manage_pos_terminal::models::transaction::TransactionStatus;
use e2manage_pos_terminal::services::cart_service::CartService;
use e2manage_pos_terminal::services::transaction_service::{
    ShiftTotals, TransactionError, TransactionService,
};
use rust_decimal::Decimal;

// ============================================================================
// Test Setup
// ============================================================================

fn setup_transaction_service() -> TransactionService {
    let db = setup_test_db_arc();
    let api = setup_mock_api_arc();
    TransactionService::new(api, db)
}

// ============================================================================
// Create Transaction Tests
// ============================================================================

#[test]
fn test_create_transaction_from_cart() {
    let service = setup_transaction_service();
    let cart_service = CartService::new();
    let product = sample_product();

    cart_service.add_item(&product, Decimal::from(2)).unwrap();
    let cart = cart_service.get_cart();

    let result = service.create_from_cart(&cart, "LYD", "shift-1", "TERM-001", "op-1", "Ahmed");
    assert!(result.is_ok());

    let txn = service.get_current();
    assert!(txn.is_some());
    let txn = txn.unwrap();
    assert_eq!(txn.items.len(), 1);
    assert_eq!(txn.status, TransactionStatus::Draft);
    assert_eq!(txn.shift_id, "shift-1");
    assert_eq!(txn.operator_id, "op-1");
}

#[test]
fn test_create_from_empty_cart_fails() {
    let service = setup_transaction_service();
    let cart = e2manage_pos_terminal::models::Cart::new();

    let result = service.create_from_cart(&cart, "LYD", "shift-1", "TERM-001", "op-1", "Ahmed");
    assert!(matches!(result, Err(TransactionError::EmptyTransaction)));
}

#[test]
fn test_get_current_empty() {
    let service = setup_transaction_service();
    assert!(service.get_current().is_none());
}

#[test]
fn test_clear_current() {
    let service = setup_transaction_service();
    let cart_service = CartService::new();
    let product = sample_product();

    cart_service.add_item(&product, Decimal::ONE).unwrap();
    service
        .create_from_cart(
            &cart_service.get_cart(),
            "LYD",
            "shift-1",
            "TERM-001",
            "op-1",
            "Ahmed",
        )
        .unwrap();

    assert!(service.get_current().is_some());

    service.clear_current();
    assert!(service.get_current().is_none());
}

// ============================================================================
// Payment Tests
// ============================================================================

#[test]
fn test_add_cash_payment() {
    let service = setup_transaction_service();
    let cart_service = CartService::new();
    let product = create_product("p1", "Test", Decimal::from(100), Decimal::from(15));

    cart_service.add_item(&product, Decimal::ONE).unwrap();
    service
        .create_from_cart(
            &cart_service.get_cart(),
            "LYD",
            "shift-1",
            "TERM-001",
            "op-1",
            "Ahmed",
        )
        .unwrap();

    // Grand total should be 115 (100 + 15% tax)
    let result = service.add_cash_payment(Decimal::from(115));
    assert!(result.is_ok());
    assert!(service.is_fully_paid());
}

#[test]
fn test_add_card_payment() {
    let service = setup_transaction_service();
    let cart_service = CartService::new();
    let product = create_product("p1", "Test", Decimal::from(100), Decimal::from(15));

    cart_service.add_item(&product, Decimal::ONE).unwrap();
    service
        .create_from_cart(
            &cart_service.get_cart(),
            "LYD",
            "shift-1",
            "TERM-001",
            "op-1",
            "Ahmed",
        )
        .unwrap();

    let result = service.add_card_payment(Decimal::from(115), "1234", "VISA", "AUTH123");
    assert!(result.is_ok());
    assert!(service.is_fully_paid());
}

#[test]
fn test_split_payment() {
    let service = setup_transaction_service();
    let cart_service = CartService::new();
    let product = create_product("p1", "Test", Decimal::from(100), Decimal::ZERO); // No tax for simplicity

    cart_service.add_item(&product, Decimal::ONE).unwrap();
    service
        .create_from_cart(
            &cart_service.get_cart(),
            "LYD",
            "shift-1",
            "TERM-001",
            "op-1",
            "Ahmed",
        )
        .unwrap();

    // Split: 60 cash + 40 card = 100 total
    service.add_cash_payment(Decimal::from(60)).unwrap();
    assert!(!service.is_fully_paid());
    assert_eq!(service.remaining(), Decimal::from(40));

    service
        .add_card_payment(Decimal::from(40), "5678", "MASTERCARD", "AUTH456")
        .unwrap();
    assert!(service.is_fully_paid());
    assert_eq!(service.remaining(), Decimal::ZERO);
}

#[test]
fn test_change_calculation() {
    let service = setup_transaction_service();
    let cart_service = CartService::new();
    let product = create_product("p1", "Test", Decimal::from(45), Decimal::ZERO); // No tax

    cart_service.add_item(&product, Decimal::ONE).unwrap();
    service
        .create_from_cart(
            &cart_service.get_cart(),
            "LYD",
            "shift-1",
            "TERM-001",
            "op-1",
            "Ahmed",
        )
        .unwrap();

    // Pay 50 for 45 total
    service.add_cash_payment(Decimal::from(50)).unwrap();

    assert!(service.is_fully_paid());
    assert_eq!(service.change_due(), Decimal::from(5));
}

#[test]
fn test_add_payment_no_transaction() {
    let service = setup_transaction_service();

    let result = service.add_cash_payment(Decimal::from(100));
    assert!(matches!(result, Err(TransactionError::NotFound)));
}

#[test]
fn test_remaining_and_change() {
    let service = setup_transaction_service();
    let cart_service = CartService::new();
    let product = create_product("p1", "Test", Decimal::from(100), Decimal::ZERO);

    cart_service.add_item(&product, Decimal::ONE).unwrap();
    service
        .create_from_cart(
            &cart_service.get_cart(),
            "LYD",
            "shift-1",
            "TERM-001",
            "op-1",
            "Ahmed",
        )
        .unwrap();

    // Initially, remaining = grand total
    assert_eq!(service.remaining(), Decimal::from(100));
    assert_eq!(service.change_due(), Decimal::ZERO);

    // Partial payment
    service.add_cash_payment(Decimal::from(30)).unwrap();
    assert_eq!(service.remaining(), Decimal::from(70));
    assert_eq!(service.change_due(), Decimal::ZERO);

    // Full payment with overpay
    service.add_cash_payment(Decimal::from(80)).unwrap();
    assert_eq!(service.remaining(), Decimal::ZERO);
    assert_eq!(service.change_due(), Decimal::from(10));
}

// ============================================================================
// Void Transaction Tests
// ============================================================================

#[test]
fn test_void_transaction() {
    let service = setup_transaction_service();
    let cart_service = CartService::new();
    let product = sample_product();

    cart_service.add_item(&product, Decimal::ONE).unwrap();
    service
        .create_from_cart(
            &cart_service.get_cart(),
            "LYD",
            "shift-1",
            "TERM-001",
            "op-1",
            "Ahmed",
        )
        .unwrap();

    let result = service.void("Customer changed mind");
    assert!(result.is_ok());
    assert!(service.get_current().is_none());
}

#[test]
fn test_void_no_transaction() {
    let service = setup_transaction_service();

    let result = service.void("No transaction to void");
    assert!(matches!(result, Err(TransactionError::NotFound)));
}

// ============================================================================
// Transaction Accessors Tests
// ============================================================================

#[test]
fn test_is_fully_paid_initial() {
    let service = setup_transaction_service();
    let cart_service = CartService::new();
    let product = sample_product();

    cart_service.add_item(&product, Decimal::ONE).unwrap();
    service
        .create_from_cart(
            &cart_service.get_cart(),
            "LYD",
            "shift-1",
            "TERM-001",
            "op-1",
            "Ahmed",
        )
        .unwrap();

    assert!(!service.is_fully_paid());
}

#[test]
fn test_pending_count() {
    let service = setup_transaction_service();

    let count = service.pending_count().unwrap();
    assert_eq!(count, 0);
}

// ============================================================================
// ShiftTotals Tests
// ============================================================================

#[test]
fn test_shift_totals_default() {
    let totals = ShiftTotals::default();

    assert_eq!(totals.sales_count, 0);
    assert_eq!(totals.sales_total, Decimal::ZERO);
    assert_eq!(totals.return_count, 0);
    assert_eq!(totals.return_total, Decimal::ZERO);
    assert_eq!(totals.cash_total, Decimal::ZERO);
    assert_eq!(totals.card_total, Decimal::ZERO);
}

#[test]
fn test_shift_totals_net_calculations() {
    let mut totals = ShiftTotals::default();
    totals.sales_total = Decimal::from(1000);
    totals.cash_total = Decimal::from(600);
    totals.card_total = Decimal::from(400);
    totals.return_total = Decimal::from(100);

    // net_cash = cash_total - return_total = 600 - 100 = 500
    assert_eq!(totals.net_cash(), Decimal::from(500));

    // net_sales = sales_total - return_total = 1000 - 100 = 900
    assert_eq!(totals.net_sales(), Decimal::from(900));

    // total_payments = cash + card + wallet + credit + other
    assert_eq!(totals.total_payments(), Decimal::from(1000));
}

// ============================================================================
// Transaction Error Tests
// ============================================================================

#[test]
fn test_transaction_error_display() {
    assert_eq!(
        TransactionError::EmptyTransaction.to_string(),
        "Transaction has no items"
    );
    assert_eq!(
        TransactionError::NotFullyPaid.to_string(),
        "Transaction not fully paid"
    );
    assert_eq!(
        TransactionError::AlreadyCompleted.to_string(),
        "Transaction already completed"
    );
    assert_eq!(
        TransactionError::NotFound.to_string(),
        "Transaction not found"
    );
}
