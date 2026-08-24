//! What the till can read when the platform refuses a request for being malformed.
//!
//! The answer today is **nothing**, and it is not a missing field — the envelope does not parse.
//! These tests pin that, and pin the shape that fixes it, so the day the platform migrates its
//! Zod path onto `ApiError` the till can tell that it worked rather than assuming.
//!
//! Written 2026-08-24 while answering a platform lane's question — *does the till read the
//! `errors` array, which our migration deletes?* The literal answer is no: `ApiErrorResponse.errors`
//! has no production reader. The useful answer turned out to be the opposite of the question.
//!
//! # These fixtures are captured, not composed — and they are still fossils
//!
//! Every literal here was read off `error-handler.middleware.ts` and `api-error.type.ts` rather
//! than imagined, which is the difference between this file and the test it was written to correct.
//! That is necessary and it is **not sufficient.** A captured literal freezes the day it was
//! captured, and the platform lane's own version of this problem is instructive: a Zod 3→4 upgrade
//! rewrote `"Required"` into `"Invalid input: expected string, received undefined"` with no code
//! changed and nothing red. A frozen fixture passes straight through an upgrade like that.
//!
//! So the load-bearing assertion in this file is the **negative** one —
//! `the_platforms_current_validation_body_does_not_parse_at_all`. It is what would have caught the
//! dead path, and it stays pointed at whatever the platform sends next. The positives are records of
//! an agreement, dated, and they should be re-derived rather than trusted once that migration lands.
//! The till cannot verify a producer's body from here; the pact is the instrument that can, and the
//! interaction for this surface is deliberately unwritten until the surface is repaired.

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

/// The migrated shape the platform lane committed to, and what the till does with it unchanged.
///
/// Agreed 2026-08-24 with `e2manage-platform-ae`, who is migrating the Zod path onto `ApiError`:
///
/// ```json
/// { "error": { "code": "VALIDATION_ERROR", "message": "…",
///     "details": { "fields": [ { "path": "pin", "code": "invalid_type", "message": "…" } ] } } }
/// ```
///
/// **Measured, so the answer to "does the till need work for this" is not a guess: no.** The
/// envelope parses, the code is [`ServerErrorCode::ValidationError`] and reads as recognised, and
/// the `fields` payload degrades to `details: None` because [`pos_api::RefusalDetails`] models no
/// variant for that code. `RefusalDetails::read` is total by construction — an unreadable figure
/// must not cost the till the refusal it travelled on — so the drop is the designed behaviour and
/// not a silent loss to repair.
///
/// # Why no `RefusalDetails` variant is being added ahead of it
///
/// Because the producer has not shipped it. A typed variant written now would be a till-side model
/// of a payload no server sends — the same defect as the test this file exists to correct, arrived
/// at from the opposite direction and with better intentions. When it lands, `fields` becomes worth
/// typing and the pact interaction becomes worth writing, in that order.
///
/// # If it is ever typed, key it on `code` and never on `message`
///
/// Not a preference. `path` and `code` are Zod's issue code and location; `message` is prose the
/// producer explicitly reserves the right to let a dependency rewrite — and did: the 3→4 upgrade
/// changed the message text with no code change anywhere, and a middleware predicate matching on
/// that text silently began answering 404 where it had answered 400. That defect is the reason this
/// migration exists. A consumer keying on `message` inherits it across a patch bump.
#[test]
fn the_migrated_validation_shape_needs_no_change_on_this_side() {
    let migrated = r#"{"success":false,"message":"Validation failed","error":{"code":"VALIDATION_ERROR","message":"Validation failed","details":{"fields":[{"path":"pin","code":"invalid_type","message":"Invalid input: expected string, received undefined"}]}}}"#;

    let envelope: ApiErrorResponse =
        serde_json::from_str(migrated).expect("the agreed migrated shape must parse");
    let detail = envelope.error.expect("the nested error object");

    assert_eq!(detail.code, ServerErrorCode::ValidationError);
    assert!(detail.code.is_recognised());
    assert!(
        detail.details.is_none(),
        "`fields` is expected to drop until a RefusalDetails variant models it. If this now reads \
         Some, someone typed it — write the pact interaction in the same change"
    );
}

/// The control for the test above: dropping an unmodelled `details` is general, not special-casing.
///
/// Without this, `details.is_none()` above is consistent with the payload being rejected, ignored,
/// or mishandled specifically for `VALIDATION_ERROR`. A different code carrying a `details` object
/// the till models no variant for behaves identically, which is what makes the drop a property of
/// `RefusalDetails::read` rather than a fact about validation.
#[test]
fn an_unmodelled_details_payload_drops_for_any_code() {
    let other = r#"{"success":false,"message":"x","error":{"code":"NOT_FOUND","message":"x","details":{"anything":1}}}"#;

    let envelope: ApiErrorResponse = serde_json::from_str(other).expect("must parse");
    let detail = envelope.error.expect("the nested error object");

    assert_eq!(detail.code, ServerErrorCode::NotFound);
    assert!(detail.details.is_none());
}
