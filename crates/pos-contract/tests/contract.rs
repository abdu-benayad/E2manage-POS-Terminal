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
//!
//! # Regeneration MERGES into the artifact; it does not replace it
//!
//! "Byte-stable regeneration" holds only while nothing changes. When an interaction's
//! `description` or its `given` changes, the writer **adds** the new form and leaves the old one
//! behind — so editing two interactions took the artifact from seven to nine, both stale copies
//! looking exactly like real coverage, and the platform would have verified expectations the till
//! no longer has.
//!
//! **Delete `pacts/e2manage-pos-terminal-wadi-dms-api.json` and re-run whenever you edit an
//! existing interaction.** Adding a new one is safe; changing one is not.

use pact_consumer::prelude::*;
use pos_api::Enveloped;
use pos_api::{
    ApiErrorResponse, HeartbeatRequest, HeartbeatResponse, LoginTerminalResponse, PairingStatus,
    PairingStatusResponse, RefreshResponse, ServerErrorCode,
};
use pos_models::HardwareEnrolment;

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
/// The till reads this through `Enveloped<T>`, which unwraps `{success, message, data}` and
/// hands back `data` — so `data.{pairingCode, expiresAt, hardwareId}` is the till's actual
/// expectation and the envelope around it is incidental. Both are pinned: the envelope
/// because `Enveloped` refuses a body whose `success` is absent or false, and the payload
/// because that is what `RequestPairingResponse` deserialises.
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

/// The one enrolment signal the till can read: `isRePair` on the pairing-status poll.
///
/// # Why the status route and not the pairing *request*
///
/// The obvious place to pin *"the platform knows this hardware"* is the request route — post a
/// pairing request from already-enrolled hardware and assert the answer says so. **That
/// interaction cannot come out differently, so it is not worth writing.**
/// `pairing.handler.ts:38-42` builds `RequestPairingResult` from three fields, and
/// `terminal.controller.ts:667-675` sends exactly those three, so the 200 is byte-identical for
/// a first enrolment and a re-pair. An expectation there would hold whatever the platform did
/// with the flag, while its name reported that the flag was pinned — worse than no pin, because
/// a named check retires the question.
///
/// `terminal.controller.ts:703` is the **sole** `isRePair` occurrence in that controller, so the
/// status poll is the only place this claim is falsifiable. That is the whole reason for the
/// choice, and it is why the request-side pin stays deferred rather than written weakly.
///
/// # What is a literal and what is not
///
/// `status` and `isRePair` are literals because together they *are* the claim: a request that is
/// still pending and reports itself a re-pair. Splitting them would pin two facts neither of
/// which is the one the till acts on. `pairingCode` is a literal because the provider state
/// chose it. `expiresAt` is `like!` — the fixture mints it as `now + 1 hour`, so any literal
/// would be a timestamp that is wrong by the time it is read.
///
/// # The sibling this is only load-bearing beside
///
/// [`HardwareEnrolment`] defaults to `Undetermined` when `isRePair` is absent, deliberately: a
/// server predating the platform's re-pair change asserts no negative. That default is exactly
/// what makes a *vanished* field quiet here — this interaction alone cannot tell a renamed field
/// from a removed one. It is paired with
/// `pos-api/src/auth.rs::an_absent_is_re_pair_is_undetermined_not_a_denial`, which pins the
/// absence against a `true` positive control, and with the platform's own state handler
/// (`provider-states.ts`), which writes `isRePair: true` on the row.
///
/// Verified by mutation rather than assumed: flipping the fixture's `isRePair` to `false` turns
/// this interaction red on the platform's `test:contracts:till`.
#[tokio::test]
async fn a_pending_re_pair_says_so_on_the_status_poll() {
    // Chosen by the platform's provider state, not here — see `PACT_REPAIR_PAIRING_CODE`.
    const REPAIR_PAIRING_CODE: &str = "PACTREPAIRCODE";

    let pact = PactBuilder::new("e2manage-pos-terminal", "wadi-dms-api")
        .with_output_dir("./pacts")
        .interaction(
            "a pairing-status request for a pending re-pair",
            "",
            |mut i| {
                i.given(format!(
                    "a pending re-pair request with the code {REPAIR_PAIRING_CODE}"
                ));
                i.request.get().path(format!(
                    "/api/pos/terminals/pairing/status/{REPAIR_PAIRING_CODE}"
                ));
                i.response
                    .status(200)
                    .header("content-type", "application/json")
                    .json_body(json_pattern!({
                        "success": true,
                        "data": {
                            "status": "PENDING",
                            "pairingCode": REPAIR_PAIRING_CODE,
                            "expiresAt": like!("2026-08-24T12:00:00.000Z"),
                            "isRePair": true,
                        }
                    }));
                i
            },
        )
        .start_mock_server(None, None);

    let url = format!(
        "{}api/pos/terminals/pairing/status/{REPAIR_PAIRING_CODE}",
        pact.url()
    );
    let response = reqwest::Client::new()
        .get(&url)
        .send()
        .await
        .expect("the mock server did not answer");

    assert_eq!(response.status().as_u16(), 200);

    // `Enveloped<PairingStatusResponse>` is the till's actual read path
    // (`auth.rs::check_pairing_status`), not a restatement of it. The two assertions below are
    // about wiring that has no other test on this route: `PENDING` reaching a manual
    // `Deserialize` impl, and `isRePair` reaching a field named `enrolment` — which only works
    // because of an explicit `rename`, `rename_all = "camelCase"` being a no-op on a
    // single-word name.
    let status: PairingStatusResponse = response
        .json::<Enveloped<PairingStatusResponse>>()
        .await
        .expect("the till's PairingStatusResponse could not parse the status payload")
        .into_inner();

    assert_eq!(status.status, PairingStatus::Pending);
    assert_eq!(
        status.enrolment,
        HardwareEnrolment::AlreadyEnrolled,
        "the platform said this pairing request is a re-pair and the till read it otherwise"
    );
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
    // Modelled as of `auth-outcome-and-offline-lockout` task 03. This assertion read
    // `Unrecognised("POS_TERMINAL_TOKEN_MISSING")` with the note "the till carries POS_* codes
    // verbatim until it models them" — which was true when it was written and stopped being true
    // the day the till modelled all 32. `crates/pos-contract` is `[workspace] exclude`d, so
    // `cargo test --workspace` could not report it; `cd crates/pos-contract && cargo test` is
    // what answers.
    assert_eq!(detail.code, ServerErrorCode::PosTerminalTokenMissing);
    assert!(
        detail.code.is_recognised(),
        "a code the till models must not read as `no information`"
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
    // See the note on the missing-token case above: modelled as of task 03.
    assert_eq!(detail.code, ServerErrorCode::PosTerminalTokenInvalid);
    assert!(detail.code.is_recognised());
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

    // `Enveloped<LoginTerminalResponse>` is exactly what the till's `login_terminal` reads
    // (`auth.rs:384`), rather than hand-written JSON assertions: this is the deserialisation
    // that was failing in production, so the contract is only meaningful if it is the one
    // being exercised here.
    //
    // It replaces a manual `ApiEnvelope` read plus two assertions, because `Enveloped` now
    // performs both checks itself — a body whose `success` is absent or false, or which
    // carries no `data`, fails to deserialise rather than reaching an assertion. Folding them
    // into the type is the point of the type; restating them here would test the restatement.
    let login: LoginTerminalResponse = response
        .json::<Enveloped<LoginTerminalResponse>>()
        .await
        .expect("the till's LoginTerminalResponse could not parse the login payload")
        .into_inner();

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

/// The session token the two authenticated 200s present in `X-Terminal-Token`.
///
/// A **fixed** value, because the provider state has to be able to create a session that matches
/// it: `POS_TerminalSession` stores `SHA-256(token)` and never the token, so the fixture hashes
/// this exact string into `tokenHash`. A token minted per run could not be named in the artifact.
///
/// `terminal-auth.middleware.ts:32,76` reads `x-terminal-token` and **nowhere else** — not the
/// `Authorization` header the till also sets — so that is the header pinned here.
const PACT_SESSION_TOKEN: &str = "pact-terminal-session-token-0001";

/// The fleet heartbeat, which is the surface `till-api-client-disagrees-with-the-served-contract`
/// repaired and therefore the one it earns the right to pin.
///
/// # Why the request body is pinned, against the usual instinct to pin only responses
///
/// The defect here was not a field name in the reply. The till serialised its metrics **flat**
/// while `fleet.controller.ts:197` reads `req.body.metrics`, so the handler saw `undefined`, fell
/// back to `{}`, and — no validator on the route, every field in `terminal-heartbeat.handler.ts`
/// optional-with-guard — answered **200 having recorded nothing**. Every till would have reported
/// itself online with zero telemetry, indistinguishable from success in any manual check.
///
/// A pact that declared only the response would pass against exactly that broken payload. So the
/// nesting is the contract, and `uptime` is pinned as a literal key inside it: it is
/// `TerminalMetricsDto`'s one **required** field (`fleet.dto.ts:21`), and the till spelled it
/// `uptimeSeconds`, which the platform has no field for.
///
/// The metric *values* are `like!` — they are measurements, and pinning a number would fail the
/// contract for a terminal that happened to be up longer.
#[tokio::test]
async fn a_heartbeat_reports_metrics_where_the_platform_reads_them() {
    let pact = PactBuilder::new("e2manage-pos-terminal", "wadi-dms-api")
        .with_output_dir("./pacts")
        .interaction(
            "a heartbeat from an authenticated terminal reporting its metrics",
            "",
            |mut i| {
                i.given(format!(
                    "an active terminal whose session token is {PACT_SESSION_TOKEN}"
                ));
                i.request
                    .post()
                    .path("/api/pos/fleet/heartbeat")
                    .header("content-type", "application/json")
                    .header("X-Terminal-Token", PACT_SESSION_TOKEN)
                    .json_body(json_pattern!({
                        // The nesting IS the contract. See the doc comment.
                        "metrics": {
                            "uptime": like!(3600),
                            "cpuPercent": like!(12.5),
                            "memoryMb": like!(512),
                            "diskFreeMb": like!(20480),
                            "offlineTxnCount": like!(0),
                            "appVersion": like!("1.2.3"),
                        }
                    }));
                i.response
                    .status(200)
                    .header("content-type", "application/json")
                    .json_body(json_pattern!({
                        "success": true,
                        "data": {
                            "acknowledged": like!(true),
                            "serverTime": like!("2026-08-23T10:00:00.000Z"),
                            "commands": [],
                        }
                    }));
                i
            },
        )
        .start_mock_server(None, None);

    let metrics = HeartbeatRequest {
        uptime_seconds: 3600,
        cpu_percent: 12.5,
        memory_mb: 512,
        disk_free_mb: 20480,
        offline_txn_count: 0,
        app_version: "1.2.3".to_string(),
        current_shift_id: None,
        current_operator_id: None,
    };

    // Serialised through the till's own wrapper rather than a hand-written JSON object, so that a
    // regression which un-nests the body fails here instead of passing against a restatement.
    let body = serde_json::json!({ "metrics": metrics });

    let url = format!("{}api/pos/fleet/heartbeat", pact.url());
    let response = reqwest::Client::new()
        .post(&url)
        .header("X-Terminal-Token", PACT_SESSION_TOKEN)
        .json(&body)
        .send()
        .await
        .expect("the mock server did not answer");

    assert_eq!(response.status().as_u16(), 200);

    let heartbeat: HeartbeatResponse = response
        .json::<Enveloped<HeartbeatResponse>>()
        .await
        .expect("the till's HeartbeatResponse could not parse the heartbeat payload")
        .into_inner();

    assert!(
        heartbeat.acknowledged,
        "`acknowledged` is the only field the till branches on; a heartbeat that is not acknowledged is not a heartbeat"
    );
    assert!(
        !heartbeat.server_time.is_empty(),
        "`serverTime` arrived blank — it is required, and a required field that deserialises empty is the failure `#[serde(default)]` would have hidden"
    );
}

/// A refreshed terminal session.
///
/// Pinned because the repair was real: the till read this enveloped route with the raw `post`, so
/// it looked for `sessionToken` at the top level of `{success, message, data}` and never found it.
/// The route is `terminalAuthMiddleware` and one of the six CSRF-exempt prefixes
/// (`csrf.middleware.ts:105`), which is what makes it reachable enough to pin at all.
///
/// **No request body is declared.** The till posts `&()`, and the route ignores its body — and
/// declaring an empty one records `"body": {}` plus a content-type, which deadlocks provider
/// verification for 30 s with `error sending request`. That failure reads as an unreachable
/// provider and says nothing about the contract; the module docs above carry the full note.
#[tokio::test]
async fn a_token_refresh_returns_a_session_the_till_can_read() {
    let pact = PactBuilder::new("e2manage-pos-terminal", "wadi-dms-api")
        .with_output_dir("./pacts")
        .interaction(
            "a token refresh from a terminal holding a valid session",
            "",
            |mut i| {
                i.given(format!(
                    "an active terminal whose session token is {PACT_SESSION_TOKEN}"
                ));
                i.request
                    .post()
                    .path("/api/pos/terminals/refresh")
                    .header("X-Terminal-Token", PACT_SESSION_TOKEN);
                i.response
                    .status(200)
                    .header("content-type", "application/json")
                    .json_body(json_pattern!({
                        "success": true,
                        "data": {
                            "sessionToken": like!("9c4e1a7f2b8d0e63a5f1c9b4d7e2a0f8"),
                        }
                    }));
                i
            },
        )
        .start_mock_server(None, None);

    let url = format!("{}api/pos/terminals/refresh", pact.url());
    let response = reqwest::Client::new()
        .post(&url)
        .header("X-Terminal-Token", PACT_SESSION_TOKEN)
        .send()
        .await
        .expect("the mock server did not answer");

    assert_eq!(response.status().as_u16(), 200);

    let refreshed: RefreshResponse = response
        .json::<Enveloped<RefreshResponse>>()
        .await
        .expect("the till's RefreshResponse could not parse the refresh payload")
        .into_inner();

    // A blank token can no longer reach this line: `RefreshResponse.session_token` is a
    // `SessionToken`, whose `Deserialize` refuses one, so the `.json()` above is where a blank
    // fails now — the `is_empty()` check that used to stand here has moved into the type. The
    // property still needs saying, because "the till parsed it" is not obviously the same claim
    // as "the token is usable", and this is the file that documents the contract.
    assert!(
        !refreshed.session_token.expose().trim().is_empty(),
        "a refresh that returns a blank token silently unauthenticates the till on its next request"
    );
}

/// The artifact's interaction count, derived from the artifact rather than stated in prose.
///
/// # Why a count is worth a test
///
/// Every figure about this contract that was written down instead of computed has gone stale.
/// The issue that added the last two interactions opened by asserting the pact pinned "four"
/// when it pinned five, and by putting the till's surface at "36 paths" when it is 41 — that one
/// came from a grep that counted a doc-comment placeholder and could not see a path assembled
/// base-URL-first. A number quoted from a document is a claim rotting quietly; a number recomputed
/// from the artifact is a fact.
///
/// So this asserts the two things a person forgets in opposite directions: that adding an
/// interaction here without recording it leaves the coverage table wrong, and that the artifact on
/// disk is the one this crate just wrote rather than a stale copy.
///

// ============================================================================
// The six till write routes
// ============================================================================

/// The six routes the platform opened for this till, pinned by their **guard** rather than by
/// their payload.
///
/// # What is pinned, and why it is not the write itself
///
/// The obvious interaction — *a till posts a sale and the platform records it* — cannot be
/// written honestly today. It would require a provider state that enrols a terminal, signs an
/// operator in, opens a shift and seeds a catalogue product and a company currency, and it would
/// record a **guess** about the success payload: no run of `till/issue/…routes-built-for-it`
/// task 07 has happened, because no current platform is reachable from the till's machine. An
/// expectation the platform has not met fails that repository's suite for a change it made
/// correctly, which is the one thing this contract must never do.
///
/// What *can* be pinned with no fixture at all is the thing this issue is actually about: **these
/// six paths exist, and they sit behind `terminalAuth`.** Reaching one with no `X-Terminal-Token`
/// answers 401 `POS_TERMINAL_TOKEN_MISSING`, from the first guard in the chain
/// (`terminal-auth.middleware.ts:66`), before any handler, body or database is involved.
///
/// # Why that is the highest-value pin available here
///
/// The failure this issue exists to prevent is **silent**. When the till's paths drifted before,
/// the symptom was a 404. Here the back-office mounts still exist and still refuse, so a till
/// calling the wrong door produces no new error — the write just keeps failing exactly as it did
/// while nobody noticed.
///
/// This interaction is the instrument that ends that. An unmounted route does not answer
/// `POS_TERMINAL_TOKEN_MISSING`; it reaches `notFoundMiddleware`
/// (`error-handler.middleware.ts:365-375`) and answers `NOT_FOUND`. So if the platform moves,
/// renames or unmounts any of the six, **its own suite fails in the pull request that does it** —
/// which is precisely what nothing did when it added them.
///
/// # The state handler already exists
///
/// `'no terminal state is required'` is registered at `provider-states.ts:121`. An artifact naming
/// a `given` with no handler fails the platform's verification immediately
/// (`till.provider.pact.e2e.test.ts:114-123`), so reusing a registered state is what makes these
/// six deliverable without a platform-side change.
///
/// # No request bodies
///
/// Declared deliberately, not overlooked. `terminalAuth` refuses before the body is read, and an
/// interaction declaring `json_body(json_pattern!({}))` records `"body": {}` with a content-type
/// and deadlocks verification for 30 seconds — see the module header.
async fn a_till_write_route_without_a_terminal_token(
    description: &'static str,
    given: &'static str,
    verb: TillVerb,
    path: &'static str,
) {
    let pact = PactBuilder::new("e2manage-pos-terminal", "wadi-dms-api")
        .with_output_dir("./pacts")
        .interaction(description, "", |mut i| {
            i.given(given);
            match verb {
                TillVerb::Get => i.request.get().path(path),
                TillVerb::Post => i.request.post().path(path),
            };
            i.response
                .status(401)
                .header("content-type", "application/json")
                .json_body(json_pattern!({
                    "message": like!("Terminal token is missing"),
                    "error": {
                        // Literal: the till branches on this, and it is what separates "the
                        // route exists and its guard refused" from "the route is not mounted".
                        "code": "POS_TERMINAL_TOKEN_MISSING",
                        "message": like!("Terminal token is missing"),
                    }
                }));
            i
        })
        .start_mock_server(None, None);

    let url = format!("{}{}", pact.url(), path.trim_start_matches('/'));
    let client = reqwest::Client::new();
    let response = match verb {
        TillVerb::Get => client.get(&url),
        TillVerb::Post => client.post(&url),
    }
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
    assert_eq!(detail.code, ServerErrorCode::PosTerminalTokenMissing);
    assert!(
        detail.code.is_recognised(),
        "a code the till models must not read as `no information`"
    );
}

/// Which method a till write uses. Two of the six are not POST.
#[derive(Clone, Copy)]
enum TillVerb {
    Get,
    Post,
}

/// `POST /api/pos/till/transactions` — the sale.
#[tokio::test]
async fn the_till_sale_route_exists_behind_terminal_auth() {
    a_till_write_route_without_a_terminal_token(
        "a till sale with no terminal token",
        "no terminal state is required",
        TillVerb::Post,
        "/api/pos/till/transactions",
    )
    .await;
}

/// `POST /api/pos/till/transactions/{id}/void`.
#[tokio::test]
async fn the_till_void_route_exists_behind_terminal_auth() {
    a_till_write_route_without_a_terminal_token(
        "a till void with no terminal token",
        "no terminal state is required",
        TillVerb::Post,
        "/api/pos/till/transactions/pact-no-such-transaction/void",
    )
    .await;
}

/// `GET /api/pos/till/transactions/by-receipt/{n}` — the one read of the six.
#[tokio::test]
async fn the_till_receipt_lookup_route_exists_behind_terminal_auth() {
    a_till_write_route_without_a_terminal_token(
        "a till receipt lookup with no terminal token",
        "no terminal state is required",
        TillVerb::Get,
        "/api/pos/till/transactions/by-receipt/PACTNOSUCHRECEIPT",
    )
    .await;
}

/// `POST /api/pos/till/shifts/start`.
#[tokio::test]
async fn the_till_shift_opening_route_exists_behind_terminal_auth() {
    a_till_write_route_without_a_terminal_token(
        "a till shift opening with no terminal token",
        "no terminal state is required",
        TillVerb::Post,
        "/api/pos/till/shifts/start",
    )
    .await;
}

/// `POST /api/pos/till/shifts/{id}/end`.
#[tokio::test]
async fn the_till_shift_closing_route_exists_behind_terminal_auth() {
    a_till_write_route_without_a_terminal_token(
        "a till shift closing with no terminal token",
        "no terminal state is required",
        TillVerb::Post,
        "/api/pos/till/shifts/pact-no-such-shift/end",
    )
    .await;
}

/// `POST /api/pos/till/returns`.
///
/// The route a cashier is refused on for a *different* reason once a terminal token is present —
/// `POS_REFUND` sits at SUPERVISOR. That refusal is the one the till now renders as its own thing
/// (`CapabilityStanding::SupervisorHolds`), and it is **not** pinned here: reaching it needs an
/// enrolled terminal and a signed-in cashier, which is a provider state the platform does not
/// register. Coverage grows one interaction per repaired surface, and that surface has not been
/// exercised against a live server yet.
#[tokio::test]
async fn the_till_return_route_exists_behind_terminal_auth() {
    a_till_write_route_without_a_terminal_token(
        "a till return with no terminal token",
        "no terminal state is required",
        TillVerb::Post,
        "/api/pos/till/returns",
    )
    .await;
}

/// **`EXPECTED` is meant to be edited.** Raising it is the moment you update
/// `e2manage/doc/pos-till-server-contract`'s coverage table and copy the artifact into the
/// platform. That is the whole point: the edit is the reminder.
///
/// # What this test can and cannot see
///
/// It reads the artifact **as it stands on disk**, and the interactions above write theirs
/// concurrently with it, so within a single `cargo test` it is reading the *previous* run's file.
/// That is a real limit and not a defect to paper over: it makes this a check on the artifact **as
/// committed**, which is the thing the platform actually verifies against. A stale committed
/// artifact is exactly what it exists to catch.
///
/// The consequence to know: after editing an interaction, one run is not enough. Delete the
/// artifact, run, run again — the second run is the one that reports the truth.
#[test]
fn the_artifact_pins_exactly_the_interactions_this_crate_declares() {
    /// Raise this in the same commit that adds an interaction, updates the coverage table in
    /// `e2manage/doc/pos-till-server-contract`, and copies the artifact to the platform.
    const EXPECTED: usize = 14;

    let path = std::path::Path::new("./pacts/e2manage-pos-terminal-wadi-dms-api.json");
    let Ok(text) = std::fs::read_to_string(path) else {
        // The artifact is written by the interactions above. Absent means the suite has not run
        // yet in this tree, which is a state to report rather than a failure to assert on.
        eprintln!(
            "{} is absent; run `cargo test` in this crate to regenerate it, then re-run",
            path.display()
        );
        return;
    };

    let artifact: serde_json::Value =
        serde_json::from_str(&text).expect("the pact artifact is not valid JSON");
    let interactions = artifact["interactions"]
        .as_array()
        .expect("the pact artifact has no `interactions` array")
        .len();

    assert_eq!(
        interactions, EXPECTED,
        "the artifact pins {interactions} interactions and this crate expects {EXPECTED}.\n\
         If you ADDED one: raise EXPECTED here, add its row to `e2manage/doc/pos-till-server-contract`'s \
         coverage table, and copy the artifact to \
         `wadi-dms-api/src/modules/pos/__tests__/contracts/pacts/` — `scripts/till-pact.mjs sync` \
         at the platform monorepo root performs that copy and `… check` notices when it was \
         skipped, but neither runs on its own and `ci.sh` renders the check's exit 3 as \
         \"could not check\" rather than gating. Until someone runs it the platform is verifying \
         the till's PREVIOUS expectations.\n\
         If you did NOT: an interaction was lost, and the platform's suite has stopped checking \
         something the till depends on."
    );
}
