//! Three failures that used to be one string.
//!
//! `handle_response` flattened every non-2xx **and** every parse failure into a single
//! `anyhow!("API Error ({}): {}")`, and `handle_request_error` turned every reqwest error into one
//! of three more strings. Nothing downstream could branch on any of it — which is why
//! `pos-services::sync_service::is_auth_error` recovers the status by substring-matching `"401"`.
//!
//! These tests pin the three cases apart, through the real client and a real socket.
//!
//! # Why they downcast
//!
//! Every public method on `ApiClient` still returns `anyhow::Result<T>`, deliberately: `ApiFailure`
//! is `Error + Send + Sync + 'static`, so `?` converts for free and the ~30 public signatures did
//! not have to change. Downcasting is therefore not a test convenience — it asserts the
//! architectural claim, that the legacy surface is kept **by conversion rather than by a second
//! path**. If a future edit rebuilds a parallel string error, the downcast returns `None` and these
//! fail.

use pos_api::{ApiClient, ApiFailure, RefusalDetails, ServerErrorCode};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// The `ApiFailure` behind an `anyhow::Error`, or a failure naming what was found instead.
fn failure_of(error: &anyhow::Error) -> &ApiFailure {
    error.downcast_ref::<ApiFailure>().unwrap_or_else(|| {
        panic!("the transport must surface an `ApiFailure` through `anyhow`, got: {error:#}")
    })
}

async fn server_answering(status: u16, body: serde_json::Value) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/pos/sync/catalog/delta"))
        .respond_with(ResponseTemplate::new(status).set_body_json(body))
        .mount(&server)
        .await;
    server
}

async fn drive(server: &MockServer) -> anyhow::Error {
    ApiClient::new(&server.uri())
        .get_catalog_delta("2026-08-23T09:00:00.000Z")
        .await
        .expect_err("this fixture answers a failure")
}

/// A 401 carrying a well-formed error envelope keeps its status **and** its machine code.
///
/// Both were previously destroyed: the status survived only as digits inside a message string, and
/// the code was never read at all, because `ApiErrorResponse` declared `error_code` at the top
/// level where the platform nests it under `error.code`.
#[tokio::test]
async fn a_refusal_keeps_its_status_and_its_code() {
    let server = server_answering(
        401,
        serde_json::json!({
            "success": false,
            "message": "Session expired",
            "error": { "code": "UNAUTHORIZED", "message": "Session expired" }
        }),
    )
    .await;

    let error = drive(&server).await;

    match failure_of(&error) {
        ApiFailure::Refused {
            status,
            code,
            message,
            details,
        } => {
            assert_eq!(status.as_u16(), 401);
            assert_eq!(*code, ServerErrorCode::Unauthorized);
            assert!(code.is_recognised());
            assert_eq!(message, "Session expired");
            // `UNAUTHORIZED` is one of the status-derived codes and the catalogue gives it no
            // payload, so an absent `details` here is the contract, not a gap.
            assert!(details.is_none());
        }
        other => panic!("a 401 with a well-formed envelope must be `Refused`, got: {other:?}"),
    }
}

/// A body that arrived and does not match the contract is `Unreadable` — never `Unreachable`.
///
/// This is the distinction the old code destroyed and the one that matters most: folding it into
/// "the network was down" is how a disagreement between the till and the platform about an
/// endpoint's shape survives to production, retried forever by a caller that thinks it is weather.
#[tokio::test]
async fn a_body_that_does_not_match_the_contract_is_unreadable() {
    let server = server_answering(200, serde_json::json!({ "totally": "unexpected" })).await;

    let error = drive(&server).await;

    assert!(
        matches!(failure_of(&error), ApiFailure::Unreadable(_)),
        "a 200 whose body does not match must be `Unreadable`, got: {:?}",
        failure_of(&error)
    );
}

/// A non-2xx whose body cannot be parsed is **also** `Unreadable`, not a `Refused` with a guessed
/// code.
///
/// The live case is a CSRF 403: `{success, error: "..."}` is flat, while `ApiErrorResponse` wants a
/// top-level `message` and an `error` object. Inventing a code here would be a fabrication the
/// caller then branches on.
#[tokio::test]
async fn an_unparseable_refusal_is_not_given_a_guessed_code() {
    let server = server_answering(
        403,
        serde_json::json!({ "success": false, "error": "invalid csrf token" }),
    )
    .await;

    let error = drive(&server).await;

    assert!(
        matches!(failure_of(&error), ApiFailure::Unreadable(_)),
        "an unparseable error body must not become a `Refused`, got: {:?}",
        failure_of(&error)
    );
}

/// A base URL with nothing behind it.
///
/// Binding to port 0 lets the OS pick a free one, and dropping the listener closes it
/// synchronously. **Not a dropped `MockServer`** — that shuts down on a background task, so the
/// port can still answer for a moment afterwards, and the first version of this test caught its
/// own fixture doing exactly that: wiremock's unmatched-request reply arrived and read as
/// `Unreadable`. Picking a port number by hand would occasionally hit something real.
fn closed_port_uri() -> String {
    let listener =
        std::net::TcpListener::bind("127.0.0.1:0").expect("the OS can assign a loopback port");
    let addr = listener
        .local_addr()
        .expect("a bound listener has an address");
    drop(listener);
    format!("http://{addr}")
}

/// Nothing listening is `Unreachable` — the one case that is ordinary weather.
#[tokio::test]
async fn a_closed_port_is_unreachable() {
    let uri = closed_port_uri();

    let error = ApiClient::new(&uri)
        .get_catalog_delta("2026-08-23T09:00:00.000Z")
        .await
        .expect_err("nothing is listening on that port");

    assert!(
        matches!(failure_of(&error), ApiFailure::Unreachable(_)),
        "a closed port must be `Unreachable`, got: {:?}",
        failure_of(&error)
    );
}

/// `is_transient()` is true for exactly one of the three, and that is the whole point of keeping
/// them apart: only a server that was not there is worth asking again unchanged.
#[tokio::test]
async fn only_unreachable_is_worth_retrying() {
    let refused = server_answering(
        401,
        serde_json::json!({
            "success": false,
            "message": "Session expired",
            "error": { "code": "UNAUTHORIZED", "message": "Session expired" }
        }),
    )
    .await;
    let unreadable = server_answering(200, serde_json::json!({ "totally": "unexpected" })).await;
    let closed = closed_port_uri();

    let refused = drive(&refused).await;
    let unreadable = drive(&unreadable).await;
    let unreachable = ApiClient::new(&closed)
        .get_catalog_delta("2026-08-23T09:00:00.000Z")
        .await
        .expect_err("nothing is listening on that port");

    assert!(
        !failure_of(&refused).is_transient(),
        "a refusal is an answer"
    );
    assert!(
        !failure_of(&unreadable).is_transient(),
        "a contract breach is a bug, and retrying it just repeats it"
    );
    assert!(
        failure_of(&unreachable).is_transient(),
        "a server that was not there may be there later"
    );
}

/// A refusal's figures survive the transport, over a real socket.
///
/// The unit tests in `refusal_details` read the `error` object directly. This one proves the
/// wiring in between: `handle_response` → `refusal_from_body` → `ApiFailure::Refused`. Before this
/// task the till deserialized this exact envelope successfully and dropped `details` on the floor,
/// which is why asserting it at the type level would not have caught anything.
///
/// **This is half of acceptance row 16.** `lockedUntil` reaches the outcome. The other half —
/// that nothing stores it as an unlock timer — is `tests/guards.rs::a_lockout_notice_is_never_stored`,
/// because "no code does X" is a claim about the tree and not about a value.
#[tokio::test]
async fn a_lockout_refusal_carries_its_instant_all_the_way_out() {
    let server = server_answering(
        401,
        serde_json::json!({
            "success": false,
            "message": "Operator locked",
            "error": {
                "code": "POS_OPERATOR_LOCKED",
                "message": "Operator locked",
                "details": { "lockedUntil": "2026-08-23T14:32:00.000Z" }
            }
        }),
    )
    .await;

    let error = drive(&server).await;

    match failure_of(&error) {
        ApiFailure::Refused { code, details, .. } => {
            assert_eq!(*code, ServerErrorCode::PosOperatorLocked);
            let Some(RefusalDetails::OperatorLocked(locked)) = details else {
                panic!("the lockout instant must reach the caller, got {details:?}");
            };
            assert_eq!(
                locked.locked_until.instant_to_render().to_rfc3339(),
                "2026-08-23T14:32:00+00:00"
            );
        }
        other => panic!("a 401 with a well-formed envelope must be `Refused`, got: {other:?}"),
    }

    // The message is the server's sentence, not the figure. A number concatenated into a
    // translated string cannot be parsed, compared or filtered by the client that needs it —
    // which is the whole reason the platform sends `details` as fields.
    assert!(
        !error.to_string().contains("14:32"),
        "the instant travels as a field, never inside the message: {error}"
    );
}

/// A wrong PIN carries how many tries are left, and a zero is read as the lock it means.
#[tokio::test]
async fn a_wrong_pin_carries_its_remaining_attempts() {
    for (remaining, expected) in [
        (
            2,
            RefusalDetails::PinInvalid(pos_api::PinInvalidDetails {
                attempts_remaining: pos_models::AttemptsRemaining::new(2).expect("2 is not 0"),
            }),
        ),
        // The server contradicting its own partition: the attempt that empties the budget is
        // supposed to answer POS_OPERATOR_LOCKED.
        (0, RefusalDetails::PinBudgetExhausted),
    ] {
        let server = server_answering(
            401,
            serde_json::json!({
                "success": false,
                "message": "Invalid PIN",
                "error": {
                    "code": "POS_PIN_INVALID",
                    "message": "Invalid PIN",
                    "details": { "attemptsRemaining": remaining }
                }
            }),
        )
        .await;

        let error = drive(&server).await;

        match failure_of(&error) {
            ApiFailure::Refused { details, .. } => {
                assert_eq!(
                    details.as_ref(),
                    Some(&expected),
                    "for {remaining} remaining"
                )
            }
            other => panic!("a 401 must be `Refused`, got: {other:?}"),
        }
    }
}
