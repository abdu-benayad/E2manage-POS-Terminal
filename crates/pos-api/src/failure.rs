//! How a call to the platform failed.
//!
//! These types live in `pos-api` and not in `pos-models` because they name `reqwest::Error` and
//! `serde_json::Error`. Unlike `pos_models::StoreFailure`, there is no orphan problem here: this
//! crate owns the `reqwest` dependency, so `Self` is local and a `From` impl would be legal. That
//! asymmetry is worth understanding rather than copying — and, as [`ApiFailure::unreachable`]
//! explains, the blanket `From` is still the wrong thing to write.
//!
//! # The distinction this file exists to preserve
//!
//! `handle_response` (`crates/pos-api/src/client.rs:514-554`) flattens every non-2xx response
//! *and* every parse failure into one `anyhow!("API Error ({}): {}")`. `AuthService::verify_pin`
//! (`crates/pos-services/src/auth_service.rs:263-268`) then treats any error at all as grounds to
//! fall back to offline verification:
//!
//! ```text
//! Err(e) => { warn!("Online PIN verification failed, trying offline: {}", e); }
//! ```
//!
//! So a response that *arrived* and could not be read is currently indistinguishable from a
//! network that is down, and both silently downgrade an authentication decision. A body that does
//! not match the contract is a **bug in one of the two systems** — it should be logged and
//! alerted as one, not counted as weather.
//!
//! Nothing here is wired in yet; `auth-outcome-and-offline-lockout` replaces `handle_response`.

use std::fmt;

use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use thiserror::Error;

// ============================================================================
// ServerErrorCode
// ============================================================================

/// The machine-readable code from the error envelope's `error.code`.
///
/// # This code carries less than it looks like it does
///
/// The platform derives it from the HTTP status through a fixed table — `errorCodeFor`
/// (`wadi-dms-api/src/shared/types/api-error.type.ts:75-86`) maps 400/401/403/404/409/422/500 and
/// nothing else. So today the code is a restatement of the status, and four different 401s from
/// the verify-PIN endpoint are structurally identical. Filed as
/// `e2manage/issue/api-error-code-is-derived-from-status`.
///
/// The till therefore models the codes it can see and **never infers a specific refusal from
/// one**. `Unrecognised` is a state, not a parse failure: unlike a closed server enum such as
/// `OperatorRole`, this set is open by construction (`errorCodeFor` already returns
/// `UNKNOWN_ERROR` for anything outside its table), so "a code this till has not been taught" is
/// a thing that legitimately arrives and must be representable.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(from = "String", into = "String")]
pub enum ServerErrorCode {
    /// `BAD_REQUEST` — HTTP 400.
    BadRequest,
    /// `UNAUTHORIZED` — HTTP 401.
    Unauthorized,
    /// `FORBIDDEN` — HTTP 403.
    Forbidden,
    /// `NOT_FOUND` — HTTP 404.
    NotFound,
    /// `CONFLICT` — HTTP 409.
    Conflict,
    /// `VALIDATION_ERROR` — HTTP 422.
    ValidationError,
    /// `INTERNAL_ERROR` — HTTP 500.
    InternalError,
    /// A code this till does not model, carried verbatim.
    ///
    /// Includes the platform's own `UNKNOWN_ERROR`. Treat it as "no information", never as a
    /// particular refusal.
    Unrecognised(String),
}

impl ServerErrorCode {
    /// The spelling the platform uses.
    pub fn as_wire_str(&self) -> &str {
        match self {
            Self::BadRequest => "BAD_REQUEST",
            Self::Unauthorized => "UNAUTHORIZED",
            Self::Forbidden => "FORBIDDEN",
            Self::NotFound => "NOT_FOUND",
            Self::Conflict => "CONFLICT",
            Self::ValidationError => "VALIDATION_ERROR",
            Self::InternalError => "INTERNAL_ERROR",
            Self::Unrecognised(code) => code,
        }
    }

    /// Whether this code tells the till anything beyond "the request was refused".
    ///
    /// False for [`Self::Unrecognised`]. A caller that branches on the code should ask this
    /// first, so that "we do not know why" cannot be read as "not one of the reasons we handle".
    pub const fn is_recognised(&self) -> bool {
        !matches!(self, Self::Unrecognised(_))
    }
}

impl From<String> for ServerErrorCode {
    fn from(code: String) -> Self {
        match code.as_str() {
            "BAD_REQUEST" => Self::BadRequest,
            "UNAUTHORIZED" => Self::Unauthorized,
            "FORBIDDEN" => Self::Forbidden,
            "NOT_FOUND" => Self::NotFound,
            "CONFLICT" => Self::Conflict,
            "VALIDATION_ERROR" => Self::ValidationError,
            "INTERNAL_ERROR" => Self::InternalError,
            _ => Self::Unrecognised(code),
        }
    }
}

impl From<ServerErrorCode> for String {
    fn from(code: ServerErrorCode) -> Self {
        match code {
            ServerErrorCode::Unrecognised(code) => code,
            recognised => recognised.as_wire_str().to_string(),
        }
    }
}

impl fmt::Display for ServerErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_wire_str())
    }
}

// ============================================================================
// ApiFailure
// ============================================================================

/// Why a call to the platform did not produce an answer.
///
/// Three cases that must stay three, because the correct response to each is different: retry
/// later, act on what the server said, or raise a bug.
#[derive(Debug, Error)]
pub enum ApiFailure {
    /// The server answered, and the answer was no.
    ///
    /// This is the platform working correctly and declining — an expired session, a wrong PIN, a
    /// terminal that is not registered. The till should act on it, not retry it.
    #[error("the server refused the request with {status} ({code}): {message}")]
    Refused {
        /// The HTTP status the server returned.
        status: StatusCode,
        /// The machine code from the envelope. See [`ServerErrorCode`] for what it is worth.
        code: ServerErrorCode,
        /// The human-readable message from the envelope.
        message: String,
    },

    /// The server could not be reached at all.
    ///
    /// The only case that is ordinary weather, and the only one an offline-first till should
    /// silently absorb.
    #[error("the server could not be reached")]
    Unreachable(#[source] reqwest::Error),

    /// The server answered, and the answer did not match the contract.
    ///
    /// **Never fold this into [`Self::Unreachable`].** A body that arrived and could not be read
    /// means the till and the platform disagree about the shape of an endpoint — one of them has
    /// a bug, and treating it as a network blip is how the disagreement survives to production.
    #[error("the server answered, but its response did not match the contract")]
    Unreadable(#[source] serde_json::Error),
}

impl ApiFailure {
    /// Records that the server could not be reached.
    ///
    /// There is deliberately **no `impl From<reqwest::Error> for ApiFailure`**, even though this
    /// crate owns `reqwest` and the orphan rule would permit one. `reqwest` folds body-decode
    /// failures into the same `Error` type as connection failures — `is_decode()` is the only
    /// thing that separates them — so a blanket conversion would route "the response arrived and
    /// did not match the contract" straight into [`Self::Unreachable`], rebuilding inside this
    /// enum the exact conflation it exists to end.
    ///
    /// Callers therefore fetch the body and decode it with `serde_json` themselves, which keeps
    /// the two failures in two types, and reach this constructor by name. A conversion that has
    /// to be written is a conversion that gets read.
    pub const fn unreachable(error: reqwest::Error) -> Self {
        Self::Unreachable(error)
    }

    /// Whether retrying this call later could plausibly succeed unchanged.
    ///
    /// Exhaustive with no catch-all arm: a new failure case has to answer this deliberately.
    pub const fn is_transient(&self) -> bool {
        match self {
            Self::Unreachable(_) => true,
            Self::Refused { .. } | Self::Unreadable(_) => false,
        }
    }
}

impl From<serde_json::Error> for ApiFailure {
    /// The one safe blanket conversion: a `serde_json::Error` can only mean the bytes in hand did
    /// not match the contract.
    fn from(error: serde_json::Error) -> Self {
        Self::Unreadable(error)
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::ApiErrorResponse;

    /// The envelope `respondWithApiError` writes, captured verbatim.
    const REFUSAL_ENVELOPE: &str = r#"{
        "success": false,
        "message": "Invalid PIN",
        "error": { "code": "UNAUTHORIZED", "message": "Invalid PIN" }
    }"#;

    #[test]
    fn error_envelope_finds_the_code_where_the_server_nests_it() {
        // Before this task the till declared `errorCode` at the top level, so this field had
        // always deserialized to `None` against a correct server.
        let envelope: ApiErrorResponse =
            serde_json::from_str(REFUSAL_ENVELOPE).expect("the captured envelope");

        let detail = envelope.error.expect("the nested error object");
        assert_eq!(detail.code, ServerErrorCode::Unauthorized);
        assert_eq!(envelope.message, "Invalid PIN");
    }

    #[test]
    fn error_envelope_without_an_error_object_still_reads() {
        // Not every path through the platform goes via `respondWithApiError`.
        let envelope: ApiErrorResponse =
            serde_json::from_str(r#"{"success":false,"message":"Bad Request"}"#)
                .expect("a bare envelope");

        assert!(envelope.error.is_none());
        assert!(envelope.errors.is_none());
    }

    #[test]
    fn error_envelope_carries_per_field_validation_failures() {
        let json = r#"{"message":"Validation failed","errors":[{"field":"pin"}]}"#;
        let envelope: ApiErrorResponse = serde_json::from_str(json).expect("a 422 envelope");

        assert_eq!(envelope.errors.expect("the errors array").len(), 1);
    }

    #[test]
    fn server_error_code_round_trips_every_code_the_platform_emits() {
        // `errorCodeFor` (`api-error.type.ts:75-86`) emits exactly these, plus UNKNOWN_ERROR.
        for wire in [
            "BAD_REQUEST",
            "UNAUTHORIZED",
            "FORBIDDEN",
            "NOT_FOUND",
            "CONFLICT",
            "VALIDATION_ERROR",
            "INTERNAL_ERROR",
        ] {
            let code = ServerErrorCode::from(wire.to_string());
            assert!(code.is_recognised(), "{wire} must be modelled");
            assert_eq!(code.as_wire_str(), wire);
            assert_eq!(String::from(code.clone()), wire);
            assert_eq!(
                serde_json::from_str::<ServerErrorCode>(&format!("\"{wire}\"")).unwrap(),
                code
            );
        }
    }

    #[test]
    fn server_error_code_keeps_an_unmodelled_code_verbatim() {
        // The platform's own fallback, and anything a later release adds.
        for wire in ["UNKNOWN_ERROR", "PIN_LOCKED", ""] {
            let code = ServerErrorCode::from(wire.to_string());

            assert_eq!(code, ServerErrorCode::Unrecognised(wire.to_string()));
            assert!(
                !code.is_recognised(),
                "`{wire}` must not read as a refusal the till understands"
            );
            assert_eq!(code.as_wire_str(), wire);
        }
    }

    #[test]
    fn api_failure_refused_names_the_status_the_code_and_the_message() {
        let failure = ApiFailure::Refused {
            status: StatusCode::UNAUTHORIZED,
            code: ServerErrorCode::Unauthorized,
            message: "Invalid PIN".to_string(),
        };

        assert_eq!(
            failure.to_string(),
            "the server refused the request with 401 Unauthorized (UNAUTHORIZED): Invalid PIN"
        );
    }

    #[test]
    fn api_failure_unreadable_keeps_the_parse_error_as_its_source() {
        use std::error::Error as _;

        let parse_error = serde_json::from_str::<ApiErrorResponse>("{ not json")
            .expect_err("this must not parse");
        let failure = ApiFailure::from(parse_error);

        assert!(matches!(failure, ApiFailure::Unreadable(_)));
        assert!(
            failure.source().is_some(),
            "the parse error must survive, not be flattened into the message"
        );
        // The message must read like a bug report, not like weather.
        assert_eq!(
            failure.to_string(),
            "the server answered, but its response did not match the contract"
        );
    }

    #[test]
    fn api_failure_only_unreachable_is_worth_retrying_unchanged() {
        // The distinction `handle_response` currently destroys: a contract breach is not a retry.
        let unreadable =
            ApiFailure::from(serde_json::from_str::<ApiErrorResponse>("{ not json").unwrap_err());
        let refused = ApiFailure::Refused {
            status: StatusCode::UNAUTHORIZED,
            code: ServerErrorCode::Unauthorized,
            message: "Invalid PIN".to_string(),
        };

        assert!(!unreadable.is_transient());
        assert!(!refused.is_transient());
    }
}
