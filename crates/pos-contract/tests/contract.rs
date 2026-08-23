//! What the till reads from the E2Manage platform, declared as a Pact contract.
//!
//! Each test here adds one interaction to `pacts/e2manage-pos-terminal-wadi-dms-api.json`.
//! The platform replays that file against its real POS routes, so **an interaction is a
//! claim the platform's suite will enforce** — add one only for a surface the till reads
//! correctly today, per `project/till/doc/till-consumer-surface-audit`.
//!
//! Two rules that decide what a matcher should be:
//!
//! - A value the till **branches on** gets a literal. `error.code` deserialises into
//!   [`ServerErrorCode`], so its spelling is load-bearing.
//! - A value the till merely **carries** gets `like!`. Pinning a `message` would pin prose
//!   and turn every copy edit into a failed build.
//!
//! Responses are parsed with the till's own types rather than with hand-written JSON
//! assertions. A contract that restates the DTO instead of using it records what the test
//! author believed, not what the till does.

//! # Never declare an empty JSON request body
//!
//! An interaction declaring `json_body(json_pattern!({}))` records `"body": {}` with a
//! `content-type` header, and the provider verification then **hangs for 30 seconds and
//! reports `error sending request`** — measured twice, against two different databases,
//! while the same route answered `supertest` in milliseconds. A route that ignores its
//! request body must declare no body at all.
//!
//! Worth the paragraph because the failure gives no hint of its cause: it reads as the
//! provider being unreachable, not as anything about the contract.

use pact_consumer::prelude::*;
use pos_api::{ApiErrorResponse, ServerErrorCode};

/// The nested error envelope, which every refusal the till handles is carried in.
///
/// This is the highest-value pin available. `ApiClient::handle_response` parses **every**
/// non-2xx response on all 41 endpoints the till calls into [`ApiErrorResponse`], so the
/// envelope's shape is the one contract failure that would break the till everywhere at
/// once rather than on one route.
///
/// It is pinned here on `pairing/status` because that route is one of the two the audit
/// found accurate, and because an unknown pairing code is a refusal that needs no fixture
/// beyond its own absence.
///
/// Pinned: top-level `message` (required by `ApiErrorResponse`) and the nested
/// `error.{code, message}`. Deliberately **not** pinned: `success`, which the platform
/// sends and the till's error path never reads. Pinning what the consumer does not read is
/// how a contract starts failing for changes that harm nobody.
///
/// # Why the message is a literal, against the usual rule
///
/// `notFoundMiddleware` (`error-handler.middleware.ts:365-375`) answers an **unregistered**
/// route with the same status and the same `error.code: "NOT_FOUND"`. With `like!` on the
/// message, deleting this route outright would still satisfy the contract — so the
/// mutation that proves the harness reaches the real provider would pass while proving
/// nothing.
///
/// The prose is therefore the only thing distinguishing *the handler ran and found no such
/// code* from *the route is gone*. That makes it a discriminator rather than decoration,
/// which is the one case where pinning a message earns its cost. It is thrown at
/// `pairing.handler.ts:271`.
///
/// # A note on CSRF, which decides what may be pinned at all
///
/// `validateCsrfToken` is mounted on `/api` (`app.ts:303`) ahead of the POS module
/// (registered at `:370`), and `saveUninitialized: true` (`:250`) means `req.session`
/// always exists, so the middleware's `if (!req.session) return next()` escape never
/// fires. Its exemption list (`csrf.middleware.ts:104-110`) covers only
/// `terminals/{pairing,authenticate,login,refresh,logout}` and `fleet/heartbeat`.
///
/// The till holds no cookie jar and sends no `x-csrf-token`, so **every unsafe-method
/// request it makes to a non-exempt route is refused 403 with a flat
/// `{success, error: "..."}` body** that `ApiErrorResponse` cannot even parse. An
/// interaction pinning such a route would record traffic the till never successfully
/// makes.
///
/// This one is safe twice over: `GET` is a safe method, and `terminals/pairing` is exempt
/// anyway.
#[tokio::test]
async fn a_refusal_carries_the_nested_error_envelope() {
    // Long enough to pass `pairingCodeParamSchema` (6..=20 chars) so the route answers
    // with the refusal under test rather than a validation error.
    const ABSENT_PAIRING_CODE: &str = "PACTNOSUCHCODE";

    let pact = PactBuilder::new("e2manage-pos-terminal", "wadi-dms-api")
        .with_output_dir("./pacts")
        .interaction(
            "a pairing-status request for a code that does not exist",
            "",
            |mut i| {
                i.given("no pairing request exists for the code PACTNOSUCHCODE");
                i.request.get().path(format!(
                    "/api/pos/terminals/pairing/status/{ABSENT_PAIRING_CODE}"
                ));
                i.response
                    .status(404)
                    .header("content-type", "application/json")
                    .json_body(json_pattern!({
                        // Literal, not `like!` — see the note above: it is what separates
                        // this refusal from `notFoundMiddleware`'s identical-looking one.
                        "message": "Pairing code not found",
                        "error": {
                            "code": "NOT_FOUND",
                            "message": "Pairing code not found",
                        }
                    }));
                i
            },
        )
        .start_mock_server(None, None);

    let url = format!(
        "{}api/pos/terminals/pairing/status/{ABSENT_PAIRING_CODE}",
        pact.url()
    );
    let response = reqwest::Client::new()
        .get(&url)
        .send()
        .await
        .expect("the mock server did not answer");

    assert_eq!(response.status().as_u16(), 404);

    let refusal: ApiErrorResponse = response
        .json()
        .await
        .expect("the till's ApiErrorResponse could not parse the refusal");

    let detail = refusal
        .error
        .expect("the refusal carried no `error` object, so the till learns nothing from it");
    assert_eq!(detail.code, ServerErrorCode::NotFound);
    assert!(
        detail.code.is_recognised(),
        "a code the till cannot model is not worth pinning"
    );
}

/// The pairing handshake's first step: the till asks for a code to display.
///
/// **The first 2xx interaction in this contract**, and it matters more than its size
/// suggests. Every refusal pinned so far travels the shared error serialiser, so until this
/// existed the contract proved the real error path runs and said nothing about any
/// controller's success path.
///
/// The till reads this through `post_envelope`, which unwraps `{success, message, data}` and
/// hands back `data` — so `data.{pairingCode, expiresAt, hardwareId}` is the till's actual
/// expectation and the envelope around it is incidental. Both are pinned: the envelope
/// because `post_envelope` requires `success` to be present and true before it will unwrap,
/// and the payload because that is what `RequestPairingResponse` deserialises.
///
/// `pairingCode` and `expiresAt` are `like!` — they are minted per request and the till only
/// displays them. `hardwareId` is a literal because the platform echoes back what was sent,
/// and an echo that stops echoing is a real change: the till uses it to confirm the code it
/// is showing belongs to this device.
#[tokio::test]
async fn a_pairing_request_returns_a_code_to_display() {
    const HARDWARE_ID: &str = "pact-hardware-id";

    let pact = PactBuilder::new("e2manage-pos-terminal", "wadi-dms-api")
        .with_output_dir("./pacts")
        .interaction(
            "a pairing request from an unpaired terminal",
            "",
            |mut i| {
                i.given(format!(
                    "no pairing request exists for the hardware id {HARDWARE_ID}"
                ));
                i.request
                    .post()
                    .path("/api/pos/terminals/pairing/request")
                    .header("content-type", "application/json")
                    .json_body(json_pattern!({ "hardwareId": HARDWARE_ID }));
                i.response
                    .status(200)
                    .header("content-type", "application/json")
                    .json_body(json_pattern!({
                        "success": true,
                        "data": {
                            "pairingCode": like!("ABC123"),
                            "expiresAt": like!("2026-08-23T12:00:00.000Z"),
                            "hardwareId": HARDWARE_ID,
                        }
                    }));
                i
            },
        )
        .start_mock_server(None, None);

    let url = format!("{}api/pos/terminals/pairing/request", pact.url());
    let response = reqwest::Client::new()
        .post(&url)
        .json(&serde_json::json!({ "hardwareId": HARDWARE_ID }))
        .send()
        .await
        .expect("the mock server did not answer");

    assert_eq!(response.status().as_u16(), 200);
    let body: serde_json::Value = response.json().await.expect("the response was not json");
    assert_eq!(body["success"], true);
    assert_eq!(body["data"]["hardwareId"], HARDWARE_ID);
}

/// A terminal-authenticated route reached with no `X-Terminal-Token` at all.
///
/// Pinned on `logout` because it is both CSRF-exempt and unconditionally
/// terminal-authenticated (`terminal.controller.ts:113`), so the refusal is reachable with no
/// fixture beyond sending nothing.
///
/// The code is a literal. The till does not yet branch on `POS_TERMINAL_*` — `ServerErrorCode`
/// models seven generic codes and carries the rest as `Unrecognised`, whose contract is "no
/// information" — but it does read the spelling verbatim, and
/// `auth-outcome-and-offline-lockout` is about to start branching on it. Pinning it now is
/// what makes that safe: a rename between here and there would otherwise be silent, and
/// silent renames of refusal codes are the thing this whole contract exists to stop.
#[tokio::test]
async fn a_terminal_route_without_a_token_says_the_token_is_missing() {
    let pact = PactBuilder::new("e2manage-pos-terminal", "wadi-dms-api")
        .with_output_dir("./pacts")
        .interaction(
            "a terminal-authenticated request with no token",
            "",
            |mut i| {
                i.given("no terminal state is required");
                // No request body and no `content-type`: the route ignores the body, and a
                // declared-but-empty JSON body deadlocks the verification. See the module note.
                i.request.post().path("/api/pos/terminals/logout");
                i.response
                    .status(401)
                    .header("content-type", "application/json")
                    .json_body(json_pattern!({
                        "message": like!("Terminal token is missing"),
                        "error": {
                            "code": "POS_TERMINAL_TOKEN_MISSING",
                            "message": like!("Terminal token is missing"),
                        }
                    }));
                i
            },
        )
        .start_mock_server(None, None);

    let url = format!("{}api/pos/terminals/logout", pact.url());
    let response = reqwest::Client::new()
        .post(&url)
        .send()
        .await
        .expect("the mock server did not answer");

    assert_eq!(response.status().as_u16(), 401);
    let refusal: ApiErrorResponse = response
        .json()
        .await
        .expect("the till's ApiErrorResponse could not parse the refusal");
    let detail = refusal
        .error
        .expect("the refusal carried no `error` object");
    assert_eq!(
        detail.code,
        ServerErrorCode::Unrecognised("POS_TERMINAL_TOKEN_MISSING".to_string()),
        "the till carries POS_* codes verbatim until it models them"
    );
    assert!(
        !detail.code.is_recognised(),
        "an unmodelled code must read as `no information`, never as a particular refusal"
    );
}

/// The same route reached with a token the platform cannot verify.
///
/// A distinct branch from the missing-token case above and a distinct code
/// (`terminal-auth.middleware.ts:103` versus `:66`), which is the platform being careful:
/// "you sent nothing" and "what you sent is not valid" are different facts and the till will
/// eventually act on them differently — one is a bug, the other is an expired install.
///
/// Reachable with no fixture: any string that is not a live session token produces it.
#[tokio::test]
async fn a_terminal_route_with_an_unverifiable_token_says_the_token_is_invalid() {
    const NOT_A_SESSION_TOKEN: &str = "pact-not-a-real-terminal-token";

    let pact = PactBuilder::new("e2manage-pos-terminal", "wadi-dms-api")
        .with_output_dir("./pacts")
        .interaction(
            "a terminal-authenticated request with an unverifiable token",
            "",
            |mut i| {
                i.given("no terminal session exists for the token pact-not-a-real-terminal-token");
                i.request
                    .post()
                    .path("/api/pos/terminals/logout")
                    .header("X-Terminal-Token", NOT_A_SESSION_TOKEN);
                i.response
                    .status(401)
                    .header("content-type", "application/json")
                    .json_body(json_pattern!({
                        "message": like!("Invalid terminal token"),
                        "error": {
                            "code": "POS_TERMINAL_TOKEN_INVALID",
                            "message": like!("Invalid terminal token"),
                        }
                    }));
                i
            },
        )
        .start_mock_server(None, None);

    let url = format!("{}api/pos/terminals/logout", pact.url());
    let response = reqwest::Client::new()
        .post(&url)
        .header("X-Terminal-Token", NOT_A_SESSION_TOKEN)
        .send()
        .await
        .expect("the mock server did not answer");

    assert_eq!(response.status().as_u16(), 401);
    let refusal: ApiErrorResponse = response
        .json()
        .await
        .expect("the till's ApiErrorResponse could not parse the refusal");
    let detail = refusal
        .error
        .expect("the refusal carried no `error` object");
    assert_eq!(
        detail.code,
        ServerErrorCode::Unrecognised("POS_TERMINAL_TOKEN_INVALID".to_string())
    );
}
