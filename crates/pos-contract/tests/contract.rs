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
