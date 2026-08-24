//! The six till writes keep their refusal.
//!
//! Every one of them used to go through [`ApiClient::post`] or [`ApiClient::get`], which widen
//! `ApiFailure` into `anyhow::Error`. That widening is the right default for a caller that only
//! reports the failure, and the wrong one for these six: a sale the platform *refused* and a sale
//! it never heard about are different events, and the offline queue must replay only the second.
//!
//! # These tests do not downcast, and that is the assertion
//!
//! `transport_failures.rs` downcasts out of `anyhow` on purpose — it pins that the legacy surface
//! is kept by conversion rather than by a second error path. Here the opposite property is being
//! pinned: these six signatures return [`pos_api::ApiResult`], so the refusal is in the type and
//! there is nothing to downcast. If a future edit widens one of them back, this file stops
//! compiling, which is a louder failure than an assertion.
//!
//! # The control
//!
//! Each fixture mounts exactly one route. A method that asked for a different path gets wiremock's
//! unmatched-request 404 with an empty body, which reads as [`ApiFailure::Unreadable`] and fails
//! the `Refused` assertion — so a repointed path that nobody updated here goes red rather than
//! green. `a_path_nobody_mounted_is_not_a_refusal` proves that control fires.

use pos_api::{
    ApiClient, ApiFailure, CreateReturnRequest, CreateTransactionRequest, EndShiftRequest,
    RefusalDetails, ReturnItemRequest, ServerErrorCode, ServerShiftId, StartShiftRequest,
    VoidTransactionRequest,
};
use pos_models::OperatorId;
use rust_decimal::Decimal;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

// ============================================================================
// Fixtures
// ============================================================================

fn operator() -> OperatorId {
    OperatorId::new("op-1").expect("a fixture id is never blank")
}

fn a_sale() -> CreateTransactionRequest {
    CreateTransactionRequest {
        transaction_number: "TXN-1".to_string(),
        transaction_type: "SALE".to_string(),
        items: vec![],
        payments: vec![],
        subtotal: Decimal::ONE,
        tax_total: Decimal::ZERO,
        discount_total: Decimal::ZERO,
        grand_total: Decimal::ONE,
        currency: "LYD".to_string(),
        customer_id: None,
        customer_name: None,
        shift_id: Some(ServerShiftId::new("srv-shift-1").expect("a fixture id is never blank")),
        terminal_id: "TERM-001".to_string(),
        operator_id: operator(),
        note: None,
        created_at: "2026-08-24T09:00:00Z".to_string(),
        completed_at: None,
    }
}

fn an_opening() -> StartShiftRequest {
    StartShiftRequest {
        shift_number: "SH-1".to_string(),
        operator_id: operator(),
        terminal_id: "TERM-001".to_string(),
        opening_cash: Decimal::ZERO,
        currency: "LYD".to_string(),
        started_at: "2026-08-24T09:00:00Z".to_string(),
    }
}

fn a_closing() -> EndShiftRequest {
    EndShiftRequest {
        closing_cash: Decimal::ZERO,
        expected_cash: Decimal::ZERO,
        variance: Decimal::ZERO,
        note: None,
        ended_at: "2026-08-24T17:00:00Z".to_string(),
        denomination_breakdown: None,
    }
}

fn a_refund() -> CreateReturnRequest {
    CreateReturnRequest {
        original_transaction_id: "txn-1".to_string(),
        items: vec![ReturnItemRequest {
            original_item_id: "item-1".to_string(),
            quantity: Decimal::ONE,
            reason: "RETURN".to_string(),
        }],
        refund_method: "CASH".to_string(),
        refund_amount: Decimal::ONE,
        operator_id: operator(),
        terminal_id: "TERM-001".to_string(),
        shift_id: Some(ServerShiftId::new("srv-shift-1").expect("a fixture id is never blank")),
    }
}

/// The platform's refusal envelope, nested exactly as it arrives on the wire.
fn refusal_body(code: &str, details: Option<serde_json::Value>) -> serde_json::Value {
    let mut error = serde_json::json!({ "code": code, "message": "refused" });
    if let Some(details) = details {
        error["details"] = details;
    }
    serde_json::json!({ "success": false, "message": "refused", "error": error })
}

async fn answering(
    verb: &str,
    route: &str,
    status: u16,
    body: serde_json::Value,
) -> (MockServer, ApiClient) {
    let server = MockServer::start().await;
    Mock::given(method(verb))
        .and(path(route))
        .respond_with(ResponseTemplate::new(status).set_body_json(body))
        .mount(&server)
        .await;
    let client = ApiClient::new(&server.uri());
    (server, client)
}

/// The code a refusal carried, or a panic naming what arrived instead.
///
/// Takes `ApiFailure` by value rather than `anyhow::Error`, which is the point of the file.
#[track_caller]
fn code_of(failure: ApiFailure) -> ServerErrorCode {
    match failure {
        ApiFailure::Refused { code, .. } => code,
        other => panic!("expected a refusal the caller can branch on, got: {other:?}"),
    }
}

// ============================================================================
// The six
// ============================================================================

#[tokio::test]
async fn a_refused_sale_keeps_its_code() {
    let (server, client) = answering(
        "POST",
        "/api/pos/till/transactions",
        403,
        refusal_body("POS_OPERATOR_CAPABILITY_DENIED", None),
    )
    .await;
    let failure = client
        .create_transaction(&a_sale())
        .await
        .expect_err("this fixture refuses");
    drop(server);

    assert_eq!(
        code_of(failure),
        ServerErrorCode::PosOperatorCapabilityDenied
    );
}

#[tokio::test]
async fn a_refused_void_keeps_its_code() {
    let (server, client) = answering(
        "POST",
        "/api/pos/till/transactions/txn-1/void",
        403,
        refusal_body("POS_SUPERVISOR_APPROVAL_REQUIRED", None),
    )
    .await;
    let failure = client
        .void_transaction(
            "txn-1",
            &VoidTransactionRequest {
                reason: "mis-scan".to_string(),
            },
        )
        .await
        .expect_err("this fixture refuses");
    drop(server);

    assert_eq!(
        code_of(failure),
        ServerErrorCode::PosSupervisorApprovalRequired
    );
}

#[tokio::test]
async fn a_refused_receipt_lookup_keeps_its_code() {
    let (server, client) = answering(
        "GET",
        "/api/pos/till/transactions/by-receipt/R-1",
        403,
        refusal_body("POS_OPERATOR_SESSION_EXPIRED", None),
    )
    .await;
    let failure = client
        .get_transaction_by_receipt("R-1")
        .await
        .expect_err("this fixture refuses");
    drop(server);

    assert_eq!(code_of(failure), ServerErrorCode::PosOperatorSessionExpired);
}

#[tokio::test]
async fn a_refused_shift_opening_keeps_its_code() {
    let (server, client) = answering(
        "POST",
        "/api/pos/till/shifts/start",
        403,
        refusal_body("POS_OPERATOR_CAPABILITY_DENIED", None),
    )
    .await;
    let failure = client
        .start_shift(&an_opening())
        .await
        .expect_err("this fixture refuses");
    drop(server);

    assert_eq!(
        code_of(failure),
        ServerErrorCode::PosOperatorCapabilityDenied
    );
}

#[tokio::test]
async fn a_refused_shift_closing_keeps_its_code() {
    let (server, client) = answering(
        "POST",
        "/api/pos/till/shifts/shift-1/end",
        403,
        refusal_body("POS_SUPERVISOR_APPROVAL_REQUIRED", None),
    )
    .await;
    let failure = client
        .end_shift("shift-1", &a_closing())
        .await
        .expect_err("this fixture refuses");
    drop(server);

    assert_eq!(
        code_of(failure),
        ServerErrorCode::PosSupervisorApprovalRequired
    );
}

#[tokio::test]
async fn a_refused_return_keeps_its_code_and_the_roles_that_can_supply_it() {
    let (server, client) = answering(
        "POST",
        "/api/pos/till/returns",
        403,
        refusal_body(
            "POS_SUPERVISOR_APPROVAL_REQUIRED",
            Some(serde_json::json!({
                "capability": "POS_REFUND",
                "heldBy": ["SUPERVISOR", "MANAGER"]
            })),
        ),
    )
    .await;
    let failure = client
        .create_return(&a_refund())
        .await
        .expect_err("this fixture refuses");
    drop(server);

    // A refund is the worked example: a cashier is refused here, and the only useful thing the
    // till can say is *who to fetch*. The code alone does not carry that; the details do, and
    // they must survive the same door the code does.
    let ApiFailure::Refused { code, details, .. } = failure else {
        panic!("expected a refusal");
    };
    assert_eq!(code, ServerErrorCode::PosSupervisorApprovalRequired);
    let Some(RefusalDetails::SupervisorApprovalRequired(approval)) = details else {
        panic!("the roles that can supply a refund must survive the transport");
    };
    assert_eq!(approval.capability.as_str(), "POS_REFUND");
    assert_eq!(approval.held_by.len(), 2);
}

// ============================================================================
// The controls
// ============================================================================

/// A receipt the platform does not hold is a 404 **refusal**, not an unreadable answer.
///
/// The read path's whole reason for having its own door: `Ok(None)` in `ReturnService` is reached
/// from this code and from nothing else.
#[tokio::test]
async fn a_missing_receipt_is_a_refusal_carrying_not_found() {
    let (server, client) = answering(
        "GET",
        "/api/pos/till/transactions/by-receipt/R-404",
        404,
        refusal_body("NOT_FOUND", None),
    )
    .await;
    let failure = client
        .get_transaction_by_receipt("R-404")
        .await
        .expect_err("this fixture refuses");
    drop(server);

    assert_eq!(code_of(failure), ServerErrorCode::NotFound);
}

/// The control for every test above: a route the fixture never mounted does **not** read as a
/// refusal.
///
/// Without this, all seven assertions above would pass against a client that asked for entirely
/// the wrong path, because "wiremock answered 404" and "the platform refused" would be the same
/// reading. They are not: an unmounted route answers with an empty body, which is a contract
/// breach rather than a refusal.
#[tokio::test]
async fn a_path_nobody_mounted_is_not_a_refusal() {
    let (server, client) = answering(
        "POST",
        "/api/pos/some-other-route",
        403,
        refusal_body("POS_OPERATOR_CAPABILITY_DENIED", None),
    )
    .await;
    let failure = client
        .create_transaction(&a_sale())
        .await
        .expect_err("nothing is mounted at the route this method calls");
    drop(server);

    assert!(
        matches!(failure, ApiFailure::Unreadable(_)),
        "an unmounted route must not be mistaken for a refusal, got: {failure:?}"
    );
}
