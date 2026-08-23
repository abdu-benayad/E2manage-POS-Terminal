//! `POST /api/pos/sync/operators/verify-pin`, over a real socket.
//!
//! Two defects met on this route, and both were invisible to a unit test of the DTO:
//!
//! - The response was read **raw** while the controller wraps it in `{success, data}`. Repaired
//!   before this issue; asserted here because nothing else does.
//! - The DTO required `valid: bool`, which the server has never sent, so a **correct** PIN
//!   produced a deserialization failure — and `AuthService::verify_pin` absorbed that as grounds
//!   to fall back to offline verification. A correct PIN, online, silently taking the offline
//!   path is the shape of this whole issue.
//!
//! The route is `/api/pos/sync/operators/verify-pin` (`sync.controller.ts:207`). The shorter
//! `/api/pos/sync/verify-pin` is a 404, and the mocks below match the full path precisely so a
//! future edit that shortens it fails here rather than in a shop.

use pos_api::{ApiClient, ApiFailure, RefusalDetails, ServerErrorCode};
use pos_models::{OperatorId, OperatorRole, Pin};
use wiremock::matchers::{body_json, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const VERIFY_PATH: &str = "/api/pos/sync/operators/verify-pin";

async fn server_answering(status: u16, body: serde_json::Value) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(VERIFY_PATH))
        .respond_with(ResponseTemplate::new(status).set_body_json(body))
        .mount(&server)
        .await;
    server
}

fn operator() -> OperatorId {
    OperatorId::new("op-001").expect("a non-blank id")
}

fn pin() -> Pin {
    Pin::parse("1234").expect("four ASCII digits are a platform-legal PIN")
}

/// A refusal envelope as `respondWithApiError` writes it, with its typed `details`.
fn refusal(code: &str, message: &str, details: serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "success": false,
        "message": message,
        "error": { "code": code, "message": message, "details": details }
    })
}

/// A correct PIN online produces an answer, not a deserialization failure.
///
/// The regression test for the `valid` requirement. It asserts a field from *inside* `data`, which
/// is what distinguishes "the envelope was unwrapped and the body read" from "something returned
/// a default".
#[tokio::test]
async fn a_correct_pin_reads_as_the_affirmative_answer_a_200_already_is() {
    let server = server_answering(
        200,
        serde_json::json!({
            "success": true,
            "message": "PIN verified",
            "data": {
                "session": { "token": "op-sess-abc", "expiresAt": "2026-08-23T22:00:00.000Z" },
                "operatorId": "op-001",
                "employeeId": "emp-77",
                "employeeNumber": "EMP001",
                "name": "Sara Haddad",
                "nameAr": null,
                "role": "SUPERVISOR",
                "permissions": { "canVoid": true }
            }
        }),
    )
    .await;

    let verified = ApiClient::new(&server.uri())
        .verify_operator_pin(&operator(), &pin())
        .await
        .expect("a 200 is the affirmative answer");

    let session = verified
        .session
        .expect("a till presents a terminal token, so the server mints a session");
    assert_eq!(session.token().expose(), "op-sess-abc");
    assert_eq!(verified.employee_number, "EMP001");
    assert_eq!(verified.role, OperatorRole::Supervisor);
}

/// The request reaches the full path with the field names the server reads.
///
/// `body_json` matches exactly, so a renamed or dropped field fails to match and the mock answers
/// its unmatched-request reply — which surfaces here as a failure rather than as a green test
/// against a server that was never asked what this claims.
#[tokio::test]
async fn the_request_names_the_operator_and_the_pin_in_camel_case() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(VERIFY_PATH))
        .and(body_json(serde_json::json!({
            "operatorId": "op-001",
            "pin": "1234"
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "success": true,
            "data": {
                "operatorId": "op-001",
                "employeeId": "emp-77",
                "employeeNumber": "EMP001",
                "name": "Sara Haddad",
                "role": "CASHIER"
            }
        })))
        .mount(&server)
        .await;

    ApiClient::new(&server.uri())
        .verify_operator_pin(&operator(), &pin())
        .await
        .expect("the body must match what the controller's validator reads");

    assert_eq!(
        server.received_requests().await.unwrap_or_default().len(),
        1,
        "the request must actually have gone out"
    );
}

/// The three refusals this route can answer stay three, each with its own figures.
///
/// Before `ApiFailure` they were one `anyhow!("API Error (401): …")` string, and before `details`
/// they carried no figures at all. A cashier who mistyped, a cashier who is locked out, and a
/// cashier whose correct PIN is now the wrong length are three different things to say — and only
/// the first of them costs an attempt.
#[tokio::test]
async fn the_refusals_this_route_can_answer_stay_distinguishable() {
    let wrong_pin = server_answering(
        401,
        refusal(
            "POS_PIN_INVALID",
            "Invalid PIN",
            serde_json::json!({ "attemptsRemaining": 2 }),
        ),
    )
    .await;
    let locked = server_answering(
        401,
        refusal(
            "POS_OPERATOR_LOCKED",
            "Operator locked",
            serde_json::json!({ "lockedUntil": "2026-08-23T14:32:00.000Z" }),
        ),
    )
    .await;
    // 403, not 401, and deliberately: the credential was right and is nonetheless disallowed. A
    // till that classified by status alone would tell a cashier they mistyped a PIN they typed
    // correctly.
    let rotation = server_answering(
        403,
        refusal(
            "POS_PIN_ROTATION_REQUIRED",
            "PIN rotation required",
            serde_json::json!({ "requiredLength": 6 }),
        ),
    )
    .await;

    let mut seen = Vec::new();
    for server in [&wrong_pin, &locked, &rotation] {
        let failure = ApiClient::new(&server.uri())
            .verify_operator_pin(&operator(), &pin())
            .await
            .expect_err("each of these fixtures refuses");

        let ApiFailure::Refused {
            status,
            code,
            details,
            ..
        } = failure
        else {
            panic!("a refusal with a well-formed envelope must be `Refused`, got {failure:?}");
        };
        assert!(
            !ApiFailure::Refused {
                status,
                code: code.clone(),
                message: String::new(),
                details: None,
            }
            .is_transient(),
            "a refusal is an answer, never weather"
        );
        seen.push((status.as_u16(), code, details));
    }

    let attempts_left = pos_models::AttemptsRemaining::new(2).expect("2 is not 0");
    assert_eq!(seen[0].0, 401);
    assert_eq!(seen[0].1, ServerErrorCode::PosPinInvalid);
    assert!(matches!(
        seen[0].2,
        Some(RefusalDetails::PinInvalid(d)) if d.attempts_remaining == attempts_left
    ));

    assert_eq!(seen[1].0, 401);
    assert_eq!(seen[1].1, ServerErrorCode::PosOperatorLocked);
    assert!(matches!(seen[1].2, Some(RefusalDetails::OperatorLocked(_))));

    assert_eq!(seen[2].0, 403);
    assert_eq!(seen[2].1, ServerErrorCode::PosPinRotationRequired);
    assert!(matches!(
        seen[2].2,
        Some(RefusalDetails::PinRotationRequired(d))
            if d.required_length == pos_models::PinLength::Six
    ));

    // The two 401s are the pair a status-based classifier folds together. Said explicitly, because
    // "they are distinguishable" is the whole claim of this test.
    assert_eq!(seen[0].0, seen[1].0);
    assert_ne!(seen[0].1, seen[1].1);
    assert_ne!(seen[0].2, seen[1].2);
}
