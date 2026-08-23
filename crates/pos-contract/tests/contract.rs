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
use pos_api::client::ApiEnvelope;
use pos_api::{ApiErrorResponse, LoginTerminalResponse, ServerErrorCode};

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

/// A terminal logging in, and the payload it gets back.
///
/// **This is the interaction the repair exists to make possible.** `POST
/// /api/pos/terminals/login` was undeserialisable by the till for an unknown length of
/// time: `LoginTerminalResponse` required `tenantId` and the platform stopped sending it,
/// so serde refused the whole body across four production call sites. A drifted surface
/// cannot be pinned — an interaction would have failed this contract for a change the
/// platform made correctly — so `till-consumer-contract-against-the-platform` excluded it
/// deliberately and `terminal-login-requires-a-tenant-id-the-platform-never-sends` deleted
/// the field. This is the pin that stops it happening again silently.
///
/// It is reachable at all for two reasons, both measured rather than assumed:
///
/// - `terminals/login` is one of the six CSRF-exempt prefixes (`csrf.middleware.ts:105`),
///   so a cookieless POST from the till is real traffic and not a 403. Almost nothing else
///   the till writes is.
/// - `/login` is an alias for `/authenticate` on the same handler
///   (`terminal.controller.ts:76-77`), and `authenticateTerminalSchema` is a non-strict
///   `z.object`, so the `terminalCode` the till sends alongside `hardwareId` and `secret`
///   is stripped rather than refused.
///
/// # What is pinned, and the one omission worth explaining
///
/// Exactly the fields `LoginTerminalResponse` requires — `sessionToken`, `terminalId`,
/// `terminalCode`, `companyId`, `config` — plus the two `config` values the till actually
/// consumes. All of them `like!`: every one is minted per login or comes from the fixture's
/// own company, and the till carries rather than branches on them. `locale` is hardcoded
/// `'ar'` server-side and `currency` is resolved from the company, so pinning either
/// literally would pin a property of the fixture instead of a property of the contract.
///
/// **`expiresAt` is deliberately not pinned, and it is the omission a later reader will try
/// to correct.** It is `#[serde(default)] Option<String>` and nothing reads it:
/// `AuthService::login` builds `TerminalSession` without it and `save_terminal_config` has
/// no column for it. Pinning a field the till never consumes is this issue's own defect
/// running the other way — a value carried for nobody. The pin arrives *with* its consumer,
/// when `auth-outcome-and-offline-lockout` gives `SessionToken` a real expiry.
///
/// `features`, `branchId` and `receiptConfig` are out for the same reason plus their own:
/// `features` depends on what the fixture's company has seeded, and `branchId` is `null`
/// for a terminal with no location, so both would pin the fixture.
///
/// # Why it needs a fixture, when nothing else here does
///
/// The four interactions above establish absences — an unknown code, a missing token, a
/// token that resolves to nothing. This one needs a company and an `ACTIVE` terminal whose
/// `secret_hash` is the bcrypt of a secret the request presents, which makes its provider
/// state the first create-shaped handler on the other side. That is the cost of pinning a
/// success payload, and it is the reason the audit's `logout` 200 stays unpinned: a fixture
/// is worth building for a route the till actually calls in production.
///
/// The secret is 16 characters at minimum because `authenticateTerminalSchema`
/// (`terminal.validator.ts:43-53`) refuses a shorter one at 400 — which would pin a
/// validation error while looking like a login.
#[tokio::test]
async fn a_terminal_login_returns_a_session_the_till_can_read() {
    const HARDWARE_ID: &str = "pact-login-hardware-id";
    const TERMINAL_CODE: &str = "TERM-PACT-LOGIN";
    // At least 16 characters, or the request never reaches the handler under test.
    const SECRET: &str = "pact-terminal-secret-0001";

    let pact = PactBuilder::new("e2manage-pos-terminal", "wadi-dms-api")
        .with_output_dir("./pacts")
        .interaction(
            "a login from a paired terminal presenting its secret",
            "",
            |mut i| {
                i.given(format!(
                    "a paired terminal with the hardware id {HARDWARE_ID} and a known secret"
                ));
                i.request
                    .post()
                    .path("/api/pos/terminals/login")
                    .header("content-type", "application/json")
                    .json_body(json_pattern!({
                        "terminalCode": TERMINAL_CODE,
                        "hardwareId": HARDWARE_ID,
                        "secret": SECRET,
                    }));
                i.response
                    .status(200)
                    .header("content-type", "application/json")
                    .json_body(json_pattern!({
                        "success": true,
                        "data": {
                            "sessionToken": like!("2f8a1c9e4b7d0a63f5e2c8b1a4d7e0f3"),
                            "terminalId": like!("TERM-E2E-123456"),
                            "terminalCode": like!("TERM-E2E-123456"),
                            "companyId": like!("3f1b9c02-5d7e-4a18-9c3b-2e6f8a10d45c"),
                            "config": {
                                "locale": like!("ar"),
                                "currency": like!("LYD"),
                                "taxConfig": {
                                    "defaultRate": like!(0.0),
                                    "taxInclusive": like!(false),
                                },
                            },
                        }
                    }));
                i
            },
        )
        .start_mock_server(None, None);

    let url = format!("{}api/pos/terminals/login", pact.url());
    let response = reqwest::Client::new()
        .post(&url)
        .json(&serde_json::json!({
            "terminalCode": TERMINAL_CODE,
            "hardwareId": HARDWARE_ID,
            "secret": SECRET,
        }))
        .send()
        .await
        .expect("the mock server did not answer");

    assert_eq!(response.status().as_u16(), 200);

    // The exact pair `post_envelope` uses (`client.rs:387-404`), rather than hand-written
    // JSON assertions: this is the deserialisation that was failing in production, so the
    // contract is only meaningful if it is the one being exercised here.
    let envelope: ApiEnvelope<LoginTerminalResponse> = response
        .json()
        .await
        .expect("the till's LoginTerminalResponse could not parse the login payload");

    assert!(
        envelope.success,
        "`post_envelope` refuses to unwrap `data` unless `success` is present and true"
    );
    let login = envelope
        .data
        .expect("the envelope carried no `data`, so the till gets no session at all");

    for (field, value) in [
        ("sessionToken", &login.session_token),
        ("terminalId", &login.terminal_id),
        ("terminalCode", &login.terminal_code),
        ("companyId", &login.company_id),
    ] {
        assert!(
            !value.is_empty(),
            "`{field}` deserialised empty; a required field that arrives blank is the failure mode `#[serde(default)]` would have introduced here, and the reason it was refused"
        );
    }
}
