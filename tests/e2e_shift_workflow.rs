//! E2E Shift Workflow Tests
//!
//! Complete end-to-end tests for shift operations including:
//! - Start shift with opening cash
//! - Complete transactions during shift
//! - Close shift with cash count
//! - Shift summaries and reports

mod common;

use common::{op_id, op_name, TestApp};
use e2manage_pos_terminal::db::OperatorRow;
use e2manage_pos_terminal::models::{
    LockoutPeriod, MaxAttempts, NameScript, OfflineWindow, Pin, PinLength, PinPolicy,
    PinVerification, RequiredPinLength, SessionLifetime, UndeterminedCause,
};
use rust_decimal::Decimal;

/// The fixtures' PIN and the tenant policy they run under.
///
/// `Pin::parse` takes no policy — a tenant's length rule governs minting, and the till has no
/// minting door — so the two are separate concerns and are built separately here.
fn fixture_pin() -> Pin {
    Pin::parse("1234").expect("four ASCII digits are a platform-legal PIN")
}

fn fixture_policy() -> PinPolicy {
    PinPolicy::new(
        RequiredPinLength::Exactly(PinLength::Four),
        MaxAttempts::new(3).expect("three is not zero"),
        LockoutPeriod::from_minutes(30).expect("thirty is not negative"),
        SessionLifetime::from_hours(12).expect("twelve is positive"),
        OfflineWindow::from_hours(24).expect("twenty-four is not negative"),
    )
}

/// The operator this till holds, after asserting what a PIN entry offline now answers.
///
/// **These workflows used to begin by logging in offline, and they no longer can.** The platform
/// withdrew `pinHash`, schema v13 dropped the column, and a till with no network holds nothing to
/// check a PIN against until `offline-pin-verification-has-no-credential` lands. So the login step
/// asserts the honest outcome instead of a fabricated success — and asserts that it costs the
/// operator nothing, which is the whole point: the same situation previously answered `WrongPin`
/// and locked the cashier out for trying.
///
/// The identity these tests need comes from the store, which still holds it. What is gone is the
/// *verification*, not the operator.
fn operator_after_an_offline_pin_entry(app: &TestApp, id: &str) -> OperatorRow {
    let outcome = app
        .auth_service
        .verify_pin_sync(&op_id(id), &fixture_pin(), &fixture_policy());

    match outcome {
        PinVerification::Undetermined(UndeterminedCause::ServerUnreachable) => {}
        PinVerification::Refused(refusal) => panic!(
            "a synced, active operator is not refused offline, and {refusal:?} \
             {} spend their budget",
            if refusal.consumes_an_attempt() {
                "would"
            } else {
                "would not"
            }
        ),
        other => panic!("this till cannot verify a PIN offline any more, got {other:?}"),
    }

    app.db
        .get_operator_by_id(&op_id(id))
        .expect("the store answers")
        .expect("the fixture seeded this operator")
}

// ============================================================================
// Complete Shift Workflow (Sync DB Operations)
// ============================================================================

#[test]
fn test_complete_shift_workflow_sync() {
    let app = TestApp::new();
    app.seed_test_data();

    // 1. LOGIN (verify PIN)
    let operator = operator_after_an_offline_pin_entry(&app, "op-001");
    let operator_id = operator.id.clone();
    let operator_name = operator.name.recorded_in(NameScript::Arabic);

    // 2. START SHIFT with opening float (direct DB for sync test)
    let opening_cash = Decimal::from(500);
    let shift = app
        .db
        .start_shift(
            "shift-001",
            "SHIFT-001",
            &operator_id,
            Some("TERM-01"),
            opening_cash,
        )
        .expect("Failed to start shift");

    assert_eq!(shift.status, "ACTIVE");
    assert_eq!(shift.opening_cash, opening_cash);

    // 3. PERFORM TRANSACTIONS
    let mut total_cash_sales = Decimal::ZERO;

    // Transaction 1: Cash sale
    let result = app.product_service.search("SKU001", 10).unwrap();
    app.cart_service
        .add_item(&result.products[0], Decimal::from(2))
        .unwrap();
    let cart = app.cart_service.get_cart();

    app.transaction_service
        .create_from_cart(
            &cart,
            "LYD",
            &shift.id,
            "TERM-001",
            &operator_id,
            &operator_name,
        )
        .unwrap();
    let txn = app.transaction_service.get_current().unwrap();
    let txn1_total = txn.grand_total;
    app.transaction_service
        .add_cash_payment(txn1_total)
        .unwrap();
    total_cash_sales += txn1_total;

    app.cart_service.clear();
    app.transaction_service.clear_current();

    // Transaction 2: Card sale
    let result = app.product_service.search("SKU005", 10).unwrap();
    app.cart_service
        .add_item(&result.products[0], Decimal::ONE)
        .unwrap();
    let cart = app.cart_service.get_cart();

    app.transaction_service
        .create_from_cart(
            &cart,
            "LYD",
            &shift.id,
            "TERM-001",
            &operator_id,
            &operator_name,
        )
        .unwrap();
    let txn = app.transaction_service.get_current().unwrap();
    let txn2_total = txn.grand_total;
    app.transaction_service
        .add_card_payment(txn2_total, "4242", "Visa", "AUTH456")
        .unwrap();

    app.cart_service.clear();
    app.transaction_service.clear_current();

    // 4. EXPECTED CASH = opening + cash sales
    let expected_cash = opening_cash + total_cash_sales;

    // 5. END SHIFT with cash count (direct DB for sync test)
    let counted_cash = expected_cash; // Perfect count
    let close_result = app
        .db
        .end_shift(&shift.id, counted_cash, expected_cash, Some("End of day"));
    assert!(close_result.is_ok());

    // Verify shift is closed
    let closed_shift = app.db.get_shift_by_id(&shift.id).unwrap().unwrap();
    assert_eq!(closed_shift.status, "CLOSED");
    assert!(closed_shift.ended_at.is_some());

    println!("✓ Complete shift workflow (sync) passed");
}

// ============================================================================
// Shift With Variance
// ============================================================================

#[test]
fn test_shift_with_cash_variance() {
    let app = TestApp::new();
    app.seed_test_data();

    let opening_cash = Decimal::from(500);
    let shift = app
        .db
        .start_shift(
            "shift-002",
            "SHIFT-002",
            &op_id("op-001"),
            Some("TERM-01"),
            opening_cash,
        )
        .unwrap();

    // Simulate a cash sale
    let cash_sale = Decimal::from(50);
    let expected_cash = opening_cash + cash_sale;

    // Count is short by 10
    let counted_cash = expected_cash - Decimal::from(10);
    let variance = counted_cash - expected_cash;

    app.db
        .end_shift(
            &shift.id,
            counted_cash,
            expected_cash,
            Some("Short by 10 - miscounted earlier"),
        )
        .unwrap();

    let closed_shift = app.db.get_shift_by_id(&shift.id).unwrap().unwrap();
    assert_eq!(closed_shift.status, "CLOSED");

    // Variance should be recorded (negative means short)
    let actual_variance = closed_shift.closing_cash.unwrap_or(Decimal::ZERO)
        - closed_shift.expected_cash.unwrap_or(Decimal::ZERO);
    assert_eq!(actual_variance, variance);

    println!("✓ Shift with cash variance passed");
}

// ============================================================================
// Multiple Shifts (Sequential)
// ============================================================================

#[test]
fn test_multiple_sequential_shifts() {
    let app = TestApp::new();
    app.seed_test_data();

    // Shift 1: Morning
    let shift1 = app
        .db
        .start_shift(
            "shift-003a",
            "SHIFT-M001",
            &op_id("op-001"),
            Some("TERM-01"),
            Decimal::from(500),
        )
        .unwrap();

    app.db
        .end_shift(
            &shift1.id,
            Decimal::from(500),
            Decimal::from(500),
            Some("Morning shift"),
        )
        .unwrap();

    // Shift 2: Afternoon (after shift 1 closed)
    let shift2 = app
        .db
        .start_shift(
            "shift-003b",
            "SHIFT-A001",
            &op_id("op-001"),
            Some("TERM-01"),
            Decimal::from(600),
        )
        .unwrap();

    app.db
        .end_shift(
            &shift2.id,
            Decimal::from(600),
            Decimal::from(600),
            Some("Afternoon shift"),
        )
        .unwrap();

    // Both shifts should be closed
    let s1 = app.db.get_shift_by_id(&shift1.id).unwrap().unwrap();
    let s2 = app.db.get_shift_by_id(&shift2.id).unwrap().unwrap();

    assert_eq!(s1.status, "CLOSED");
    assert_eq!(s2.status, "CLOSED");

    println!("✓ Multiple sequential shifts passed");
}

// ============================================================================
// Get Current Active Shift
// ============================================================================

#[test]
fn test_get_current_active_shift() {
    let app = TestApp::new();
    app.seed_test_data();

    // No active shift initially
    let current = app.db.get_current_active_shift();
    assert!(current.is_ok());
    // May be None or may have a previous shift - check by starting fresh

    // Start a shift
    let shift = app
        .db
        .start_shift(
            "shift-004",
            "SHIFT-004",
            &op_id("op-001"),
            Some("TERM-01"),
            Decimal::from(500),
        )
        .unwrap();

    // Should find active shift
    let current = app.db.get_current_active_shift().unwrap();
    assert!(current.is_some());
    let current = current.unwrap();
    assert_eq!(current.id, shift.id);
    assert_eq!(current.status, "ACTIVE");

    println!("✓ Get current active shift passed");
}

// ============================================================================
// Shift Start Validation
// ============================================================================

#[test]
fn test_shift_requires_operator() {
    let app = TestApp::new();
    app.seed_test_data();

    // Start shift with valid operator
    let result = app.db.start_shift(
        "shift-005",
        "SHIFT-005",
        &op_id("op-001"),
        Some("TERM-01"),
        Decimal::from(500),
    );
    assert!(result.is_ok());

    let shift = result.unwrap();
    assert_eq!(shift.operator_id, op_id("op-001"));

    println!("✓ Shift requires operator passed");
}

// ============================================================================
// Shift With Zero Opening Cash
// ============================================================================

#[test]
fn test_shift_with_zero_opening_cash() {
    let app = TestApp::new();
    app.seed_test_data();

    // Zero float is valid for card-only terminals
    let shift = app
        .db
        .start_shift(
            "shift-006",
            "SHIFT-006",
            &op_id("op-001"),
            Some("TERM-01"),
            Decimal::ZERO,
        )
        .unwrap();

    assert_eq!(shift.opening_cash, Decimal::ZERO);
    assert_eq!(shift.status, "ACTIVE");

    println!("✓ Shift with zero opening cash passed");
}

// ============================================================================
// Shift Lookup By ID
// ============================================================================

#[test]
fn test_shift_lookup_by_id() {
    let app = TestApp::new();
    app.seed_test_data();

    let shift = app
        .db
        .start_shift(
            "shift-007",
            "SHIFT-007",
            &op_id("op-001"),
            Some("TERM-01"),
            Decimal::from(500),
        )
        .unwrap();

    // Lookup by ID
    let found = app.db.get_shift_by_id(&shift.id).unwrap();
    assert!(found.is_some());
    let found = found.unwrap();
    assert_eq!(found.id, shift.id);
    assert_eq!(found.shift_number, "SHIFT-007");

    // Lookup non-existent
    let not_found = app.db.get_shift_by_id("nonexistent").unwrap();
    assert!(not_found.is_none());

    println!("✓ Shift lookup by ID passed");
}

// ============================================================================
// Shift Timestamps
// ============================================================================

#[test]
fn test_shift_timestamps() {
    let app = TestApp::new();
    app.seed_test_data();

    // Start shift
    let shift = app
        .db
        .start_shift(
            "shift-008",
            "SHIFT-008",
            &op_id("op-001"),
            Some("TERM-01"),
            Decimal::from(500),
        )
        .unwrap();

    assert!(!shift.started_at.is_empty());
    assert!(shift.ended_at.is_none());

    // Close shift
    app.db
        .end_shift(&shift.id, Decimal::from(500), Decimal::from(500), None)
        .unwrap();

    let closed = app.db.get_shift_by_id(&shift.id).unwrap().unwrap();
    assert!(closed.ended_at.is_some());

    println!("✓ Shift timestamps passed");
}

// ============================================================================
// Transaction During Shift
// ============================================================================

#[test]
fn test_transactions_associated_with_shift() {
    let app = TestApp::new();
    app.seed_test_data();

    let shift = app
        .db
        .start_shift(
            "shift-009",
            "SHIFT-009",
            &op_id("op-001"),
            Some("TERM-01"),
            Decimal::from(500),
        )
        .unwrap();

    // Create transactions
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
        app.transaction_service
            .create_from_cart(
                &cart,
                "LYD",
                &shift.id,
                "TERM-001",
                &op_id("op-001"),
                &op_name("Operator"),
            )
            .unwrap();

        let txn = app.transaction_service.get_current().unwrap();
        app.transaction_service
            .add_cash_payment(txn.grand_total)
            .unwrap();

        // Verify transaction has shift ID
        let txn = app.transaction_service.get_current().unwrap();
        assert_eq!(txn.shift_id, shift.id);

        app.transaction_service.clear_current();
    }

    println!("✓ Transactions associated with shift passed");
}

// ============================================================================
// Shift Status Transitions
// ============================================================================

#[test]
fn test_shift_status_transitions() {
    let app = TestApp::new();
    app.seed_test_data();

    // Start: ACTIVE
    let shift = app
        .db
        .start_shift(
            "shift-010",
            "SHIFT-010",
            &op_id("op-001"),
            Some("TERM-01"),
            Decimal::from(500),
        )
        .unwrap();
    assert_eq!(shift.status, "ACTIVE");

    // Close: CLOSED
    app.db
        .end_shift(&shift.id, Decimal::from(500), Decimal::from(500), None)
        .unwrap();

    let closed = app.db.get_shift_by_id(&shift.id).unwrap().unwrap();
    assert_eq!(closed.status, "CLOSED");

    println!("✓ Shift status transitions passed");
}

// ============================================================================
// Shift Terminal Association
// ============================================================================

#[test]
fn test_shift_terminal_association() {
    let app = TestApp::new();
    app.seed_test_data();

    let terminal_id = "TERM-99";
    let shift = app
        .db
        .start_shift(
            "shift-011",
            "SHIFT-011",
            &op_id("op-001"),
            Some(terminal_id),
            Decimal::from(500),
        )
        .unwrap();

    assert_eq!(shift.terminal_id, Some(terminal_id.to_string()));

    println!("✓ Shift terminal association passed");
}

// ============================================================================
// Shift With Notes
// ============================================================================

#[test]
fn test_shift_close_with_notes() {
    let app = TestApp::new();
    app.seed_test_data();

    let shift = app
        .db
        .start_shift(
            "shift-012",
            "SHIFT-012",
            &op_id("op-001"),
            Some("TERM-01"),
            Decimal::from(500),
        )
        .unwrap();

    let note = "End of day - all registers balanced";
    app.db
        .end_shift(
            &shift.id,
            Decimal::from(500),
            Decimal::from(500),
            Some(note),
        )
        .unwrap();

    let closed = app.db.get_shift_by_id(&shift.id).unwrap().unwrap();
    assert!(closed.notes.is_some());
    assert!(closed.notes.unwrap().contains("registers balanced"));

    println!("✓ Shift close with notes passed");
}
