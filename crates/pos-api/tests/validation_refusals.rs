//! What the till can read when the platform refuses a request for being malformed.
//!
//! The answer today is **nothing**, and it is not a missing field — the envelope does not parse.
//! These tests pin that, and pin the shape that fixes it, so the day the platform migrates its
//! Zod path onto `ApiError` the till can tell that it worked rather than assuming.
//!
//! Written 2026-08-24 while answering a platform lane's question — *does the till read the
//! `errors` array, which our migration deletes?* The literal answer is no: `ApiErrorResponse.errors`
//! has no production reader. The useful answer turned out to be the opposite of the question.

use pos_api::{ApiErrorResponse, ServerErrorCode};

/// The platform's **current** validation refusal is unreadable to the till.
///
/// `error-handler.middleware.ts:129-132` answers a `ZodError` with
/// `res.status(400).json({ success, message, error: message, errors })` — where `error` is the
/// joined issue **string**, not the nested object every other refusal carries.
/// [`pos_api::ApiErrorDetail`] deserialises through a struct, so a string there is a type error and
/// the whole envelope is rejected. The till sees `ApiFailure::Unreadable`: *the server answered and
/// the answer did not match the contract* — which is exactly right, and means the `errors` array
/// never reaches a caller regardless of what it contains.
///
/// **So the till's exposure to that array being deleted is nil, for a reason worth more than the
/// reassurance:** this path has never worked. A malformed request is currently indistinguishable
/// from a contract breach, which is the same conflation `ApiFailure` exists to prevent everywhere
/// else.
#[test]
fn the_platforms_current_validation_body_does_not_parse_at_all() {
    let today = r#"{"success":false,"message":"pin: Required","error":"pin: Required","errors":[{"code":"invalid_type","path":["pin"],"message":"Required"}]}"#;

    let outcome = serde_json::from_str::<ApiErrorResponse>(today);

    assert!(
        outcome.is_err(),
        "the current 400 body parsed, so either the platform migrated its Zod path or \
         `ApiErrorDetail` became lenient. Both change what this file documents — read \
         `an_apierror_shaped_validation_body_is_readable` and retire this test deliberately"
    );
}

/// The shape that fixes it, pinned so the migration can be recognised when it lands.
///
/// Any refusal routed through `respondWithApiError` (`api-error.type.ts:162-173`) has this shape.
/// The till already models the code — [`ServerErrorCode::ValidationError`] exists and
/// `is_recognised()` answers true — so nothing on this side needs to change for a migrated
/// validation refusal to become actionable. That is the whole cost of the fix, from here: zero.
#[test]
fn an_apierror_shaped_validation_body_is_readable() {
    let migrated = r#"{"success":false,"message":"Validation failed","error":{"code":"VALIDATION_ERROR","message":"Validation failed"}}"#;

    let envelope: ApiErrorResponse =
        serde_json::from_str(migrated).expect("an ApiError-shaped refusal must parse");
    let detail = envelope.error.expect("the nested error object");

    assert_eq!(detail.code, ServerErrorCode::ValidationError);
    assert!(
        detail.code.is_recognised(),
        "the till must not read a migrated validation refusal as `no information`"
    );
}

/// The control, without which the test above reads as "the parser is broken".
///
/// A refusal the till demonstrably does handle, through the same parser, so
/// `the_platforms_current_validation_body_does_not_parse_at_all` is a fact about that body rather
/// than about `ApiErrorResponse`.
#[test]
fn the_same_parser_reads_an_ordinary_refusal() {
    let ordinary = r#"{"success":false,"message":"Terminal token is missing","error":{"code":"POS_TERMINAL_TOKEN_MISSING","message":"Terminal token is missing"}}"#;

    let envelope: ApiErrorResponse =
        serde_json::from_str(ordinary).expect("an ordinary refusal must parse");

    assert_eq!(
        envelope.error.expect("the nested error object").code,
        ServerErrorCode::PosTerminalTokenMissing
    );
}

/// `errors` is readable **only** on a body the platform does not send, which is why nobody noticed.
///
/// `failure.rs::error_envelope_carries_per_field_validation_failures` feeds
/// `{"message":…,"errors":[…]}` — no `error` key at all. That parses, because the field is
/// `#[serde(default)]`, and it asserts the array survives. Both true, and about a body no branch of
/// `error-handler.middleware.ts` emits: every validation path sets `error` to the joined string.
///
/// A test can be correct about an invented input indefinitely. This one records the difference, so
/// the array's readability is not mistaken for the refusal's.
#[test]
fn the_errors_array_survives_only_when_no_error_key_is_present() {
    let invented = r#"{"message":"Validation failed","errors":[{"field":"pin"}]}"#;
    let envelope: ApiErrorResponse = serde_json::from_str(invented).expect("this shape does parse");
    assert!(envelope.error.is_none());
    assert_eq!(envelope.errors.expect("the errors array").len(), 1);

    // The half that matters: add the `error` string the platform actually sends, and it stops
    // parsing. The array's presence was never the deciding field.
    let as_sent =
        r#"{"message":"Validation failed","error":"Validation failed","errors":[{"field":"pin"}]}"#;
    assert!(
        serde_json::from_str::<ApiErrorResponse>(as_sent).is_err(),
        "the platform's actual pairing of `error` and `errors` must not parse"
    );
}
