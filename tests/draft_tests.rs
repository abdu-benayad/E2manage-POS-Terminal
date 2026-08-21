//! Draft Service Integration Tests
//!
//! Tests for draft/held order management including saving carts as drafts,
//! recalling drafts, listing by operator/shift, and draft lifecycle management.

mod common;

use common::*;
use e2manage_pos_terminal::services::draft_service::{
    Draft, DraftError, DraftService, DEFAULT_DRAFT_EXPIRY_HOURS, MAX_DRAFTS_PER_OPERATOR,
};
use rust_decimal::Decimal;
use std::sync::Arc;

// ============================================================================
// Test Setup
// ============================================================================

fn setup_draft_service() -> (DraftService, Arc<e2manage_pos_terminal::db::Database>) {
    let db = setup_test_db_arc();
    // Create a default operator for tests
    create_test_operator(&db, "op-1", "Test Operator");
    let service = DraftService::new(db.clone());
    (service, db)
}

fn setup_draft_service_with_multiple_operators(
) -> (DraftService, Arc<e2manage_pos_terminal::db::Database>) {
    let db = setup_test_db_arc();
    create_test_operator(&db, "op-1", "Operator One");
    create_test_operator(&db, "op-2", "Operator Two");
    let service = DraftService::new(db.clone());
    (service, db)
}

// ============================================================================
// Save Cart as Draft Tests
// ============================================================================

#[test]
fn test_save_cart_creates_draft() {
    let (service, _db) = setup_draft_service();
    let product = sample_product();
    let cart = create_cart_with_item(&product, Decimal::from(2));

    let result = service.save_cart(
        &cart,
        Some("Customer Order".to_string()),
        &op_id("op-1"),
        "shift-1",
    );
    assert!(result.is_ok());

    let draft = result.unwrap();
    assert_eq!(draft.name, Some("Customer Order".to_string()));
    assert_eq!(draft.item_count, 1);
    assert_eq!(draft.total_quantity, 2.0);
    assert!(draft.total_amount > 0.0);
    assert!(!draft.id.is_empty());
}

#[test]
fn test_save_cart_without_name() {
    let (service, _db) = setup_draft_service();
    let product = sample_product();
    let cart = create_cart_with_item(&product, Decimal::ONE);

    let draft = service
        .save_cart(&cart, None, &op_id("op-1"), "shift-1")
        .unwrap();

    assert!(draft.name.is_none());
    assert_eq!(draft.display_name(), "Unnamed Draft");
}

#[test]
fn test_save_empty_cart_fails() {
    let (service, _db) = setup_draft_service();
    let cart = e2manage_pos_terminal::models::Cart::new();

    let result = service.save_cart(&cart, None, &op_id("op-1"), "shift-1");
    assert!(matches!(result, Err(DraftError::EmptyCart)));
}

#[test]
fn test_save_cart_with_customer() {
    let (service, _db) = setup_draft_service();
    let cart = sample_cart();

    let draft = service
        .save_cart(&cart, None, &op_id("op-1"), "shift-1")
        .unwrap();

    assert!(draft.has_customer());
    assert_eq!(draft.customer_id, Some("cust-001".to_string()));
    assert_eq!(draft.customer_name, Some("Test Customer".to_string()));
}

#[test]
fn test_save_cart_preserves_operator_and_shift() {
    let (service, _db) = setup_draft_service();
    let product = sample_product();
    let cart = create_cart_with_item(&product, Decimal::ONE);

    let draft = service
        .save_cart(&cart, None, &op_id("op-1"), "shift-1")
        .unwrap();

    assert_eq!(draft.operator_id, Some(op_id("op-1")));
    assert_eq!(draft.shift_id, Some("shift-1".to_string()));
}

#[test]
fn test_save_cart_with_custom_expiry() {
    let (service, _db) = setup_draft_service();
    let product = sample_product();
    let cart = create_cart_with_item(&product, Decimal::ONE);

    // Save with 48 hour expiry
    let draft = service
        .save_cart_with_expiry(&cart, None, &op_id("op-1"), "shift-1", Some(48))
        .unwrap();

    assert!(draft.expires_at.is_some());
    // Should expire in roughly 48 hours (allow some margin for test execution)
    let time_until = draft.time_until_expiry().unwrap();
    assert!(time_until.num_hours() >= 47);
}

#[test]
fn test_save_cart_without_expiry() {
    let (service, _db) = setup_draft_service();
    let product = sample_product();
    let cart = create_cart_with_item(&product, Decimal::ONE);

    // Save with no expiry
    let draft = service
        .save_cart_with_expiry(&cart, None, &op_id("op-1"), "shift-1", None)
        .unwrap();

    assert!(draft.expires_at.is_none());
    assert!(!draft.is_expired());
}

// ============================================================================
// Recall Draft Tests
// ============================================================================

#[test]
fn test_recall_draft() {
    let (service, _db) = setup_draft_service();
    let product = create_product("p1", "Test Product", Decimal::from(100), Decimal::from(15));
    let cart = create_cart_with_item(&product, Decimal::from(3));

    let draft = service
        .save_cart(&cart, None, &op_id("op-1"), "shift-1")
        .unwrap();
    let recalled_cart = service.recall(&draft.id).unwrap();

    assert_eq!(recalled_cart.line_count(), 1);
    assert_eq!(recalled_cart.item_count(), Decimal::from(3));
}

#[test]
fn test_recall_preserves_customer() {
    let (service, _db) = setup_draft_service();
    let cart = sample_cart();

    let draft = service
        .save_cart(&cart, None, &op_id("op-1"), "shift-1")
        .unwrap();
    let recalled_cart = service.recall(&draft.id).unwrap();

    assert_eq!(recalled_cart.customer_id, Some("cust-001".to_string()));
    assert_eq!(
        recalled_cart.customer_name,
        Some("Test Customer".to_string())
    );
}

#[test]
fn test_recall_nonexistent_draft() {
    let (service, _db) = setup_draft_service();

    let result = service.recall("nonexistent-draft-id");
    assert!(matches!(result, Err(DraftError::NotFound(_))));
}

#[test]
fn test_recall_and_delete() {
    let (service, _db) = setup_draft_service();
    let product = sample_product();
    let cart = create_cart_with_item(&product, Decimal::ONE);

    let draft = service
        .save_cart(&cart, None, &op_id("op-1"), "shift-1")
        .unwrap();
    let draft_id = draft.id.clone();

    // Recall and delete atomically
    let recalled_cart = service.recall_and_delete(&draft_id).unwrap();
    assert_eq!(recalled_cart.line_count(), 1);

    // Draft should be deleted
    let result = service.get(&draft_id);
    assert!(matches!(result, Err(DraftError::NotFound(_))));
}

// ============================================================================
// Get Draft Tests
// ============================================================================

#[test]
fn test_get_draft() {
    let (service, _db) = setup_draft_service();
    let product = sample_product();
    let cart = create_cart_with_item(&product, Decimal::from(2));

    let saved_draft = service
        .save_cart(
            &cart,
            Some("My Draft".to_string()),
            &op_id("op-1"),
            "shift-1",
        )
        .unwrap();
    let fetched_draft = service.get(&saved_draft.id).unwrap();

    assert_eq!(fetched_draft.id, saved_draft.id);
    assert_eq!(fetched_draft.name, Some("My Draft".to_string()));
    assert_eq!(fetched_draft.item_count, 1);
}

#[test]
fn test_get_nonexistent_draft() {
    let (service, _db) = setup_draft_service();

    let result = service.get("nonexistent-id");
    assert!(matches!(result, Err(DraftError::NotFound(_))));
}

// ============================================================================
// List Drafts Tests
// ============================================================================

#[test]
fn test_list_by_operator() {
    let (service, _db) = setup_draft_service_with_multiple_operators();
    let product = sample_product();
    let cart = create_cart_with_item(&product, Decimal::ONE);

    // Create drafts for different operators
    service
        .save_cart(
            &cart,
            Some("Op1 Draft 1".to_string()),
            &op_id("op-1"),
            "shift-1",
        )
        .unwrap();
    service
        .save_cart(
            &cart,
            Some("Op1 Draft 2".to_string()),
            &op_id("op-1"),
            "shift-1",
        )
        .unwrap();
    service
        .save_cart(
            &cart,
            Some("Op2 Draft 1".to_string()),
            &op_id("op-2"),
            "shift-1",
        )
        .unwrap();

    let op1_drafts = service.list_by_operator(&op_id("op-1")).unwrap();
    let op2_drafts = service.list_by_operator(&op_id("op-2")).unwrap();

    assert_eq!(op1_drafts.len(), 2);
    assert_eq!(op2_drafts.len(), 1);
}

#[test]
fn test_list_by_shift() {
    let (service, _db) = setup_draft_service();
    let product = sample_product();
    let cart = create_cart_with_item(&product, Decimal::ONE);

    // Create drafts for different shifts
    service
        .save_cart(&cart, None, &op_id("op-1"), "shift-1")
        .unwrap();
    service
        .save_cart(&cart, None, &op_id("op-1"), "shift-1")
        .unwrap();
    service
        .save_cart(&cart, None, &op_id("op-1"), "shift-2")
        .unwrap();

    let shift1_drafts = service.list_by_shift("shift-1").unwrap();
    let shift2_drafts = service.list_by_shift("shift-2").unwrap();

    assert_eq!(shift1_drafts.len(), 2);
    assert_eq!(shift2_drafts.len(), 1);
}

#[test]
fn test_list_active_drafts() {
    let (service, _db) = setup_draft_service();
    let product = sample_product();
    let cart = create_cart_with_item(&product, Decimal::ONE);

    // Initially empty
    let drafts = service.list_active().unwrap();
    assert_eq!(drafts.len(), 0);

    // Add some drafts
    service
        .save_cart(&cart, None, &op_id("op-1"), "shift-1")
        .unwrap();
    service
        .save_cart(&cart, None, &op_id("op-1"), "shift-2")
        .unwrap();

    let drafts = service.list_active().unwrap();
    assert_eq!(drafts.len(), 2);
}

#[test]
fn test_list_by_operator_empty() {
    let (service, _db) = setup_draft_service();

    let drafts = service.list_by_operator(&op_id("op-1")).unwrap();
    assert!(drafts.is_empty());
}

// ============================================================================
// Count Drafts Tests
// ============================================================================

#[test]
fn test_count_active() {
    let (service, _db) = setup_draft_service();
    let product = sample_product();
    let cart = create_cart_with_item(&product, Decimal::ONE);

    assert_eq!(service.count_active().unwrap(), 0);

    service
        .save_cart(&cart, None, &op_id("op-1"), "shift-1")
        .unwrap();
    assert_eq!(service.count_active().unwrap(), 1);

    service
        .save_cart(&cart, None, &op_id("op-1"), "shift-1")
        .unwrap();
    assert_eq!(service.count_active().unwrap(), 2);
}

#[test]
fn test_count_by_operator() {
    let (service, _db) = setup_draft_service_with_multiple_operators();
    let product = sample_product();
    let cart = create_cart_with_item(&product, Decimal::ONE);

    assert_eq!(service.count_by_operator(&op_id("op-1")).unwrap(), 0);
    assert_eq!(service.count_by_operator(&op_id("op-2")).unwrap(), 0);

    service
        .save_cart(&cart, None, &op_id("op-1"), "shift-1")
        .unwrap();
    service
        .save_cart(&cart, None, &op_id("op-1"), "shift-1")
        .unwrap();
    service
        .save_cart(&cart, None, &op_id("op-2"), "shift-1")
        .unwrap();

    assert_eq!(service.count_by_operator(&op_id("op-1")).unwrap(), 2);
    assert_eq!(service.count_by_operator(&op_id("op-2")).unwrap(), 1);
}

// ============================================================================
// Delete Draft Tests
// ============================================================================

#[test]
fn test_delete_draft() {
    let (service, _db) = setup_draft_service();
    let product = sample_product();
    let cart = create_cart_with_item(&product, Decimal::ONE);

    let draft = service
        .save_cart(&cart, None, &op_id("op-1"), "shift-1")
        .unwrap();

    let deleted = service.delete(&draft.id).unwrap();
    assert!(deleted);

    // Verify deleted
    let result = service.get(&draft.id);
    assert!(matches!(result, Err(DraftError::NotFound(_))));
}

#[test]
fn test_delete_nonexistent_draft() {
    let (service, _db) = setup_draft_service();

    // Should return false but not error
    let deleted = service.delete("nonexistent-id").unwrap();
    assert!(!deleted);
}

#[test]
fn test_delete_updates_count() {
    let (service, _db) = setup_draft_service();
    let product = sample_product();
    let cart = create_cart_with_item(&product, Decimal::ONE);

    let draft = service
        .save_cart(&cart, None, &op_id("op-1"), "shift-1")
        .unwrap();
    assert_eq!(service.count_active().unwrap(), 1);

    service.delete(&draft.id).unwrap();
    assert_eq!(service.count_active().unwrap(), 0);
}

// ============================================================================
// Rename Draft Tests
// ============================================================================

#[test]
fn test_rename_draft() {
    let (service, _db) = setup_draft_service();
    let product = sample_product();
    let cart = create_cart_with_item(&product, Decimal::ONE);

    let draft = service
        .save_cart(
            &cart,
            Some("Original Name".to_string()),
            &op_id("op-1"),
            "shift-1",
        )
        .unwrap();

    let renamed = service
        .rename(&draft.id, Some("New Name".to_string()))
        .unwrap();
    assert_eq!(renamed.name, Some("New Name".to_string()));

    // Verify persistence
    let fetched = service.get(&draft.id).unwrap();
    assert_eq!(fetched.name, Some("New Name".to_string()));
}

#[test]
fn test_rename_to_none() {
    let (service, _db) = setup_draft_service();
    let product = sample_product();
    let cart = create_cart_with_item(&product, Decimal::ONE);

    let draft = service
        .save_cart(
            &cart,
            Some("Has Name".to_string()),
            &op_id("op-1"),
            "shift-1",
        )
        .unwrap();

    let renamed = service.rename(&draft.id, None).unwrap();
    assert!(renamed.name.is_none());
    assert_eq!(renamed.display_name(), "Unnamed Draft");
}

#[test]
fn test_rename_nonexistent_draft() {
    let (service, _db) = setup_draft_service();

    let result = service.rename("nonexistent-id", Some("New Name".to_string()));
    assert!(matches!(result, Err(DraftError::NotFound(_))));
}

// ============================================================================
// Draft Limit Tests
// ============================================================================

#[test]
fn test_draft_limit_per_operator() {
    let (service, _db) = setup_draft_service();
    let product = sample_product();
    let cart = create_cart_with_item(&product, Decimal::ONE);

    // Create maximum allowed drafts
    for i in 0..MAX_DRAFTS_PER_OPERATOR {
        service
            .save_cart(
                &cart,
                Some(format!("Draft {}", i)),
                &op_id("op-1"),
                "shift-1",
            )
            .unwrap();
    }

    // Try to create one more
    let result = service.save_cart(
        &cart,
        Some("Extra Draft".to_string()),
        &op_id("op-1"),
        "shift-1",
    );
    assert!(matches!(result, Err(DraftError::LimitExceeded(_))));
}

#[test]
fn test_draft_limit_independent_per_operator() {
    let (service, _db) = setup_draft_service_with_multiple_operators();
    let product = sample_product();
    let cart = create_cart_with_item(&product, Decimal::ONE);

    // Fill up op-1's limit
    for i in 0..MAX_DRAFTS_PER_OPERATOR {
        service
            .save_cart(
                &cart,
                Some(format!("Draft {}", i)),
                &op_id("op-1"),
                "shift-1",
            )
            .unwrap();
    }

    // op-2 should still be able to create drafts
    let result = service.save_cart(
        &cart,
        Some("Op2 Draft".to_string()),
        &op_id("op-2"),
        "shift-1",
    );
    assert!(result.is_ok());
}

#[test]
fn test_draft_limit_freed_after_delete() {
    let (service, _db) = setup_draft_service();
    let product = sample_product();
    let cart = create_cart_with_item(&product, Decimal::ONE);

    // Create maximum drafts
    let mut draft_ids = Vec::new();
    for i in 0..MAX_DRAFTS_PER_OPERATOR {
        let draft = service
            .save_cart(
                &cart,
                Some(format!("Draft {}", i)),
                &op_id("op-1"),
                "shift-1",
            )
            .unwrap();
        draft_ids.push(draft.id);
    }

    // Delete one
    service.delete(&draft_ids[0]).unwrap();

    // Should be able to create one more
    let result = service.save_cart(
        &cart,
        Some("New Draft".to_string()),
        &op_id("op-1"),
        "shift-1",
    );
    assert!(result.is_ok());
}

// ============================================================================
// Draft Model Tests
// ============================================================================

#[test]
fn test_draft_display_name() {
    let product = sample_product();
    let cart = create_cart_with_item(&product, Decimal::ONE);

    // With name
    let draft1 = Draft::from_cart(&cart, Some("My Order".to_string()), None, None, None);
    assert_eq!(draft1.display_name(), "My Order");

    // Without name
    let draft2 = Draft::from_cart(&cart, None, None, None, None);
    assert_eq!(draft2.display_name(), "Unnamed Draft");
}

#[test]
fn test_draft_has_customer() {
    let product = sample_product();

    // Cart without customer
    let cart1 = create_cart_with_item(&product, Decimal::ONE);
    let draft1 = Draft::from_cart(&cart1, None, None, None, None);
    assert!(!draft1.has_customer());

    // Cart with customer
    let cart2 = sample_cart();
    let draft2 = Draft::from_cart(&cart2, None, None, None, None);
    assert!(draft2.has_customer());
}

#[test]
fn test_draft_is_expired() {
    let product = sample_product();
    let cart = create_cart_with_item(&product, Decimal::ONE);

    // Fresh draft should not be expired
    let draft = Draft::from_cart(&cart, None, None, None, Some(DEFAULT_DRAFT_EXPIRY_HOURS));
    assert!(!draft.is_expired());
    assert!(draft.expires_at.is_some());

    // Draft without expiry never expires
    let draft2 = Draft::from_cart(&cart, None, None, None, None);
    assert!(!draft2.is_expired());
    assert!(draft2.expires_at.is_none());
}

#[test]
fn test_draft_time_until_expiry() {
    let product = sample_product();
    let cart = create_cart_with_item(&product, Decimal::ONE);

    // Draft with 24 hour expiry
    let draft = Draft::from_cart(&cart, None, None, None, Some(24));
    let time_until = draft.time_until_expiry().unwrap();
    assert!(time_until.num_hours() >= 23);

    // Draft without expiry
    let draft2 = Draft::from_cart(&cart, None, None, None, None);
    assert!(draft2.time_until_expiry().is_none());
}

#[test]
fn test_draft_from_cart_captures_totals() {
    let product = create_product("p1", "Test", Decimal::from(50), Decimal::from(10)); // 10% tax
    let cart = create_cart_with_item(&product, Decimal::from(4));

    let draft = Draft::from_cart(&cart, None, None, None, None);

    assert_eq!(draft.item_count, 1);
    assert_eq!(draft.total_quantity, 4.0);
    // 50 * 4 = 200 + 10% tax = 220
    assert!((draft.total_amount - 220.0).abs() < 0.01);
}

// ============================================================================
// Draft Error Display Tests
// ============================================================================

#[test]
fn test_draft_error_display() {
    assert_eq!(
        DraftError::NotFound("draft-123".to_string()).to_string(),
        "Draft not found: draft-123"
    );
    assert_eq!(
        DraftError::Expired("draft-456".to_string()).to_string(),
        "Draft has expired: draft-456"
    );
    assert_eq!(DraftError::EmptyCart.to_string(), "Cannot save empty cart");
    assert_eq!(
        DraftError::LimitExceeded(10).to_string(),
        "Maximum drafts limit reached (10)"
    );
}

// ============================================================================
// Constants Tests
// ============================================================================

#[test]
fn test_default_constants() {
    // Verify expected default values
    assert_eq!(DEFAULT_DRAFT_EXPIRY_HOURS, 24);
    assert_eq!(MAX_DRAFTS_PER_OPERATOR, 10);
}
