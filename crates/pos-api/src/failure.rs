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

use crate::refusal_details::RefusalDetails;
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

    // ---- PIN and operator ----
    /// `POS_PIN_REQUEST_INVALID` — HTTP 400.
    ///
    /// The request body was not a PIN verification request.
    PosPinRequestInvalid,
    /// `POS_OPERATOR_NOT_FOUND` — HTTP 404.
    ///
    /// No operator with that id in this company.
    PosOperatorNotFound,
    /// `POS_OPERATOR_INACTIVE` — HTTP 401.
    ///
    /// The employee behind the operator profile is not `ACTIVE`.
    PosOperatorInactive,
    /// `POS_OPERATOR_LOCKED` — HTTP 401.
    ///
    /// Locked out; no PIN was compared and no attempt spent.
    ///
    /// Carries a `details` payload (`OperatorLockedDetails`) — see task 04.
    PosOperatorLocked,
    /// `POS_PIN_INVALID` — HTTP 401.
    ///
    /// Wrong PIN. The only refusal on this path that spends the budget.
    ///
    /// Carries a `details` payload (`PinInvalidDetails`) — see task 04.
    PosPinInvalid,
    /// `POS_PIN_ROTATION_REQUIRED` — HTTP 403.
    ///
    /// The PIN was CORRECT and is the wrong length for the rule now in force.
    ///
    /// 403 rather than 401 because the credential is right and nonetheless disallowed. The verdict
    /// is computed only after bcrypt succeeds, deliberately: deciding it earlier is a free oracle
    /// on the required length.
    ///
    /// Carries a `details` payload (`PinRotationRequiredDetails`) — see task 04.
    PosPinRotationRequired,
    /// `POS_PIN_POLICY_VIOLATION` — HTTP 400.
    ///
    /// A PIN being MINTED breaks the tenant's length rule.
    ///
    /// A minting door, so enforcement here is correct — unlike verification, which compares and
    /// nothing else.
    ///
    /// Carries a `details` payload (`PinPolicyViolationDetails`) — see task 04.
    PosPinPolicyViolation,
    /// `POS_PIN_UNCHANGED` — HTTP 400.
    ///
    /// A rotation offered the PIN that is already set.
    PosPinUnchanged,

    // ---- operator session — the credential a verified PIN mints ----
    /// `POS_OPERATOR_SESSION_REQUIRED` — HTTP 401.
    ///
    /// No `X-Operator-Token` was presented to a route that requires one.
    PosOperatorSessionRequired,
    /// `POS_OPERATOR_SESSION_INVALID` — HTTP 401.
    ///
    /// Unknown token, **or one bound to another terminal**.
    ///
    /// Folded server-side on purpose: telling the two apart would let a refusal confirm that a
    /// stolen token is live. Do not add a till-side heuristic that tries.
    PosOperatorSessionInvalid,
    /// `POS_OPERATOR_SESSION_EXPIRED` — HTTP 401.
    ///
    /// Twelve hours elapsed. The server decides this; the till's clock is not authoritative.
    PosOperatorSessionExpired,
    /// `POS_OPERATOR_SESSION_REVOKED` — HTTP 401.
    ///
    /// Taken away — a PIN reset, a deactivation, a rotation. Tested before expiry server-side.
    PosOperatorSessionRevoked,

    // ---- capability — which principal may do what at a till ----
    /// `POS_SUPERVISOR_APPROVAL_REQUIRED` — HTTP 403.
    ///
    /// Some operator role holds this capability, so a person at this till can supply it.
    ///
    /// Carries a `details` payload (`SupervisorApprovalRequiredDetails`) — see task 04.
    PosSupervisorApprovalRequired,
    /// `POS_OPERATOR_CAPABILITY_DENIED` — HTTP 403.
    ///
    /// No operator role holds it, so escalating at the till cannot help.
    ///
    /// Carries a `details` payload (`OperatorCapabilityDeniedDetails`) — see task 04.
    PosOperatorCapabilityDenied,

    // ---- terminal — the device credential ----
    /// `POS_TERMINAL_TOKEN_MISSING` — HTTP 401.
    ///
    /// No `X-Terminal-Token` at all.
    PosTerminalTokenMissing,
    /// `POS_TERMINAL_TOKEN_INVALID` — HTTP 401.
    ///
    /// A terminal token that does not resolve.
    PosTerminalTokenInvalid,
    /// `POS_TERMINAL_SESSION_EXPIRED` — HTTP 401.
    ///
    /// The terminal session lapsed. Refresh once and retry.
    PosTerminalSessionExpired,
    /// `POS_TERMINAL_SESSION_REVOKED` — HTTP 401.
    ///
    /// The terminal session was revoked.
    PosTerminalSessionRevoked,
    /// `POS_TERMINAL_NOT_ACTIVE` — HTTP 403.
    ///
    /// The terminal is enrolled and not active. Recoverable by an administrator.
    PosTerminalNotActive,
    /// `POS_TERMINAL_GONE` — HTTP 403.
    ///
    /// The device was taken away. Distinct from `NOT_ACTIVE`: only one has a remedy at the till.
    PosTerminalGone,
    /// `POS_TERMINAL_NOT_PROVISIONED` — HTTP 409.
    ///
    /// No `secretHash`, so no offline credential can be sealed for it. Re-pair.
    PosTerminalNotProvisioned,
    /// `POS_TERMINAL_AUTH_FAILED` — HTTP 401.
    ///
    /// Terminal authentication failed.
    PosTerminalAuthFailed,
    /// `POS_TERMINAL_AUTH_REQUIRED` — HTTP 401.
    ///
    /// The route needs a terminal credential and none was offered.
    PosTerminalAuthRequired,
    /// `POS_COMPANY_INACTIVE` — HTTP 401.
    ///
    /// The tenant this terminal belongs to is not active.
    PosCompanyInactive,
    /// `POS_TERMINAL_NOT_FOUND` — HTTP 404.
    ///
    /// Fleet administration: the named terminals do not exist.
    ///
    /// Carries a `details` payload (`TerminalsNotFoundDetails`) — see task 04.
    PosTerminalNotFound,
    /// `POS_TERMINAL_ACTION_NOT_ALLOWED` — HTTP 409.
    ///
    /// Fleet administration: the action does not apply in the terminal's current state.
    PosTerminalActionNotAllowed,

    // ---- offline failure reports ----
    /// `POS_OFFLINE_REPORT_NO_CREDENTIAL` — HTTP 403.
    ///
    /// No live credential for this (operator, terminal) pair.
    ///
    /// **A revoked credential and one that never existed answer identically.** Deliberate
    /// server-side; do not try to tell them apart here.
    PosOfflineReportNoCredential,
    /// `POS_OFFLINE_REPORT_EXPIRED` — HTTP 403.
    ///
    /// The credential the report is charged against is past its `notAfter`.
    ///
    /// Carries a `details` payload (`OfflineReportExpiredDetails`) — see task 04.
    PosOfflineReportExpired,
    /// `POS_OFFLINE_REPORT_OVER_BUDGET` — HTTP 400.
    ///
    /// More failures claimed than the credential's own budget allowed.
    ///
    /// Carries a `details` payload (`OfflineReportOverBudgetDetails`) — see task 04.
    PosOfflineReportOverBudget,

    // ---- fleet commands ----
    /// `POS_COMMAND_TYPE_INVALID` — HTTP 400.
    ///
    /// Fleet administration: unknown command type.
    PosCommandTypeInvalid,
    /// `POS_COMMAND_NOT_PENDING` — HTTP 409.
    ///
    /// Fleet administration: the command is no longer pending.
    ///
    /// Carries a `details` payload (`CommandNotPendingDetails`) — see task 04.
    PosCommandNotPending,
    /// `POS_COMMAND_NOT_FOR_TERMINAL` — HTTP 404.
    ///
    /// Fleet administration: the command belongs to a different terminal.
    PosCommandNotForTerminal,

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
            Self::PosPinRequestInvalid => "POS_PIN_REQUEST_INVALID",
            Self::PosOperatorNotFound => "POS_OPERATOR_NOT_FOUND",
            Self::PosOperatorInactive => "POS_OPERATOR_INACTIVE",
            Self::PosOperatorLocked => "POS_OPERATOR_LOCKED",
            Self::PosPinInvalid => "POS_PIN_INVALID",
            Self::PosPinRotationRequired => "POS_PIN_ROTATION_REQUIRED",
            Self::PosPinPolicyViolation => "POS_PIN_POLICY_VIOLATION",
            Self::PosPinUnchanged => "POS_PIN_UNCHANGED",
            Self::PosOperatorSessionRequired => "POS_OPERATOR_SESSION_REQUIRED",
            Self::PosOperatorSessionInvalid => "POS_OPERATOR_SESSION_INVALID",
            Self::PosOperatorSessionExpired => "POS_OPERATOR_SESSION_EXPIRED",
            Self::PosOperatorSessionRevoked => "POS_OPERATOR_SESSION_REVOKED",
            Self::PosSupervisorApprovalRequired => "POS_SUPERVISOR_APPROVAL_REQUIRED",
            Self::PosOperatorCapabilityDenied => "POS_OPERATOR_CAPABILITY_DENIED",
            Self::PosTerminalTokenMissing => "POS_TERMINAL_TOKEN_MISSING",
            Self::PosTerminalTokenInvalid => "POS_TERMINAL_TOKEN_INVALID",
            Self::PosTerminalSessionExpired => "POS_TERMINAL_SESSION_EXPIRED",
            Self::PosTerminalSessionRevoked => "POS_TERMINAL_SESSION_REVOKED",
            Self::PosTerminalNotActive => "POS_TERMINAL_NOT_ACTIVE",
            Self::PosTerminalGone => "POS_TERMINAL_GONE",
            Self::PosTerminalNotProvisioned => "POS_TERMINAL_NOT_PROVISIONED",
            Self::PosTerminalAuthFailed => "POS_TERMINAL_AUTH_FAILED",
            Self::PosTerminalAuthRequired => "POS_TERMINAL_AUTH_REQUIRED",
            Self::PosCompanyInactive => "POS_COMPANY_INACTIVE",
            Self::PosTerminalNotFound => "POS_TERMINAL_NOT_FOUND",
            Self::PosTerminalActionNotAllowed => "POS_TERMINAL_ACTION_NOT_ALLOWED",
            Self::PosOfflineReportNoCredential => "POS_OFFLINE_REPORT_NO_CREDENTIAL",
            Self::PosOfflineReportExpired => "POS_OFFLINE_REPORT_EXPIRED",
            Self::PosOfflineReportOverBudget => "POS_OFFLINE_REPORT_OVER_BUDGET",
            Self::PosCommandTypeInvalid => "POS_COMMAND_TYPE_INVALID",
            Self::PosCommandNotPending => "POS_COMMAND_NOT_PENDING",
            Self::PosCommandNotForTerminal => "POS_COMMAND_NOT_FOR_TERMINAL",
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
            "POS_PIN_REQUEST_INVALID" => Self::PosPinRequestInvalid,
            "POS_OPERATOR_NOT_FOUND" => Self::PosOperatorNotFound,
            "POS_OPERATOR_INACTIVE" => Self::PosOperatorInactive,
            "POS_OPERATOR_LOCKED" => Self::PosOperatorLocked,
            "POS_PIN_INVALID" => Self::PosPinInvalid,
            "POS_PIN_ROTATION_REQUIRED" => Self::PosPinRotationRequired,
            "POS_PIN_POLICY_VIOLATION" => Self::PosPinPolicyViolation,
            "POS_PIN_UNCHANGED" => Self::PosPinUnchanged,
            "POS_OPERATOR_SESSION_REQUIRED" => Self::PosOperatorSessionRequired,
            "POS_OPERATOR_SESSION_INVALID" => Self::PosOperatorSessionInvalid,
            "POS_OPERATOR_SESSION_EXPIRED" => Self::PosOperatorSessionExpired,
            "POS_OPERATOR_SESSION_REVOKED" => Self::PosOperatorSessionRevoked,
            "POS_SUPERVISOR_APPROVAL_REQUIRED" => Self::PosSupervisorApprovalRequired,
            "POS_OPERATOR_CAPABILITY_DENIED" => Self::PosOperatorCapabilityDenied,
            "POS_TERMINAL_TOKEN_MISSING" => Self::PosTerminalTokenMissing,
            "POS_TERMINAL_TOKEN_INVALID" => Self::PosTerminalTokenInvalid,
            "POS_TERMINAL_SESSION_EXPIRED" => Self::PosTerminalSessionExpired,
            "POS_TERMINAL_SESSION_REVOKED" => Self::PosTerminalSessionRevoked,
            "POS_TERMINAL_NOT_ACTIVE" => Self::PosTerminalNotActive,
            "POS_TERMINAL_GONE" => Self::PosTerminalGone,
            "POS_TERMINAL_NOT_PROVISIONED" => Self::PosTerminalNotProvisioned,
            "POS_TERMINAL_AUTH_FAILED" => Self::PosTerminalAuthFailed,
            "POS_TERMINAL_AUTH_REQUIRED" => Self::PosTerminalAuthRequired,
            "POS_COMPANY_INACTIVE" => Self::PosCompanyInactive,
            "POS_TERMINAL_NOT_FOUND" => Self::PosTerminalNotFound,
            "POS_TERMINAL_ACTION_NOT_ALLOWED" => Self::PosTerminalActionNotAllowed,
            "POS_OFFLINE_REPORT_NO_CREDENTIAL" => Self::PosOfflineReportNoCredential,
            "POS_OFFLINE_REPORT_EXPIRED" => Self::PosOfflineReportExpired,
            "POS_OFFLINE_REPORT_OVER_BUDGET" => Self::PosOfflineReportOverBudget,
            "POS_COMMAND_TYPE_INVALID" => Self::PosCommandTypeInvalid,
            "POS_COMMAND_NOT_PENDING" => Self::PosCommandNotPending,
            "POS_COMMAND_NOT_FOR_TERMINAL" => Self::PosCommandNotForTerminal,
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
        /// The figures the refusal carried beside its code — how many attempts are left, when a
        /// lock lifts, which length is now required. See [`RefusalDetails`].
        ///
        /// **Not in the `Display` string**, deliberately. These are values a caller branches on
        /// and a screen renders; folding them into a message would be the same mistake as
        /// spelling a status code with digits inside a sentence, which is what this enum exists
        /// to undo.
        details: Option<RefusalDetails>,
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

// ============================================================================
// Every recognised code, for the round-trip guard
// ============================================================================

impl ServerErrorCode {
    /// Every variant except [`Self::Unrecognised`], which is not a code but the absence of one.
    ///
    /// **Extend this when you add a variant.** `as_wire_str` is exhaustive with no catch-all, so a
    /// new variant already fails to compile there — that is the reminder. This list is what gives
    /// `the_three_spellings_agree` something to iterate, and the honest limitation is that a
    /// variant added to the enum and to both matches but *not* here is simply untested rather than
    /// reported. Rust has no reflection to close that without a derive dependency.
    #[cfg(test)]
    const ALL_RECOGNISED: &'static [Self] = &[
        Self::BadRequest,
        Self::Unauthorized,
        Self::Forbidden,
        Self::NotFound,
        Self::Conflict,
        Self::ValidationError,
        Self::InternalError,
        Self::PosPinRequestInvalid,
        Self::PosOperatorNotFound,
        Self::PosOperatorInactive,
        Self::PosOperatorLocked,
        Self::PosPinInvalid,
        Self::PosPinRotationRequired,
        Self::PosPinPolicyViolation,
        Self::PosPinUnchanged,
        Self::PosOperatorSessionRequired,
        Self::PosOperatorSessionInvalid,
        Self::PosOperatorSessionExpired,
        Self::PosOperatorSessionRevoked,
        Self::PosSupervisorApprovalRequired,
        Self::PosOperatorCapabilityDenied,
        Self::PosTerminalTokenMissing,
        Self::PosTerminalTokenInvalid,
        Self::PosTerminalSessionExpired,
        Self::PosTerminalSessionRevoked,
        Self::PosTerminalNotActive,
        Self::PosTerminalGone,
        Self::PosTerminalNotProvisioned,
        Self::PosTerminalAuthFailed,
        Self::PosTerminalAuthRequired,
        Self::PosCompanyInactive,
        Self::PosTerminalNotFound,
        Self::PosTerminalActionNotAllowed,
        Self::PosOfflineReportNoCredential,
        Self::PosOfflineReportExpired,
        Self::PosOfflineReportOverBudget,
        Self::PosCommandTypeInvalid,
        Self::PosCommandNotPending,
        Self::PosCommandNotForTerminal,
    ];
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::ApiErrorResponse;

    /// `as_wire_str`, `From<String>`, `From<ServerErrorCode> for String` and serde must agree.
    ///
    /// Hand-written matches over the same 39 codes are that many chances to typo one, and a typo
    /// is invisible: the code simply stops being recognised and starts arriving as
    /// `Unrecognised`, which reads exactly like a code the platform added and this till has not
    /// modelled yet. That is the failure this test exists to make loud.
    ///
    /// **Serde is asserted rather than inferred.** `#[serde(from = "String", into = "String")]`
    /// means deserialisation currently routes through the same two impls — but that is one
    /// attribute away from not being true, and serde is the spelling that runs in production:
    /// `ApiErrorDetail.code` is deserialised, never parsed by hand.
    #[test]
    fn the_spellings_agree_including_the_one_serde_uses() {
        for code in ServerErrorCode::ALL_RECOGNISED {
            let wire = code.as_wire_str();

            assert_eq!(
                ServerErrorCode::from(wire.to_string()),
                *code,
                "`{wire}` does not parse back to the variant that spells it — `From<String>` is \
                 missing an arm, so this code arrives as `Unrecognised` and reads like one the \
                 platform invented"
            );

            assert_eq!(
                String::from(code.clone()),
                wire,
                "`{wire}` does not survive `From<ServerErrorCode> for String`"
            );

            assert!(
                code.is_recognised(),
                "`{wire}` is in ALL_RECOGNISED and reports itself unrecognised"
            );

            let json = format!("\"{wire}\"");
            assert_eq!(
                serde_json::from_str::<ServerErrorCode>(&json).expect("a code is a JSON string"),
                *code,
                "`{wire}` does not deserialise to the variant that spells it — and serde is the \
                 path a real refusal takes"
            );
            assert_eq!(
                serde_json::to_string(code).expect("a code serialises as its wire string"),
                json
            );
        }
    }

    /// No two variants claim the same wire spelling.
    ///
    /// A duplicate would make `From<String>` silently pick whichever arm comes first, so one of
    /// the two variants could be constructed by name and never by parsing — a difference nothing
    /// else here would notice.
    #[test]
    fn no_two_codes_share_a_spelling() {
        let mut seen = std::collections::BTreeSet::new();
        for code in ServerErrorCode::ALL_RECOGNISED {
            assert!(
                seen.insert(code.as_wire_str()),
                "two variants both spell themselves `{}`",
                code.as_wire_str()
            );
        }
    }

    /// The whole point of the task: POS codes stop landing in `Unrecognised`.
    ///
    /// Acceptance row 13's other half is below — an unmodelled code still lands there and is still
    /// reported as carrying no information.
    #[test]
    fn the_pos_codes_are_no_longer_unrecognised() {
        for wire in [
            "POS_PIN_INVALID",
            "POS_OPERATOR_LOCKED",
            "POS_PIN_ROTATION_REQUIRED",
            "POS_OPERATOR_SESSION_REQUIRED",
            "POS_TERMINAL_GONE",
            "POS_TERMINAL_NOT_PROVISIONED",
            "POS_SUPERVISOR_APPROVAL_REQUIRED",
            "POS_OFFLINE_REPORT_OVER_BUDGET",
        ] {
            let code = ServerErrorCode::from(wire.to_string());
            assert!(
                code.is_recognised(),
                "`{wire}` still lands in `Unrecognised`"
            );
            assert_eq!(code.as_wire_str(), wire);
        }
    }

    /// Acceptance row 13. A code this till does not model is carried verbatim and reports itself
    /// as no information — never as a particular refusal.
    ///
    /// `Unrecognised` stays for a measured reason: the platform's catalog is explicitly not an
    /// inventory of every code the API emits, and 35+ more live in hand-rolled responses.
    #[test]
    fn an_unmodelled_code_is_carried_and_never_read_as_a_refusal() {
        let code = ServerErrorCode::from("POS_SOMETHING_SHIPPED_ON_TUESDAY".to_string());

        assert!(!code.is_recognised());
        assert_eq!(code.as_wire_str(), "POS_SOMETHING_SHIPPED_ON_TUESDAY");
        assert!(!ServerErrorCode::ALL_RECOGNISED.contains(&code));
    }

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
    fn server_error_code_round_trips_the_status_derived_codes() {
        // `errorCodeFor` (`api-error.type.ts:75-86`) emits exactly these, plus UNKNOWN_ERROR.
        //
        // Named for what it covers. It used to be called "every code the platform emits", which
        // was true while these seven were the only ones modelled and stopped being true the day
        // the POS catalogue's 32 were added — a test whose name overstates its scope is how a gap
        // stays invisible. `the_spellings_agree_including_the_one_serde_uses` is the exhaustive
        // one.
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
            details: None,
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
            details: None,
        };

        assert!(!unreadable.is_transient());
        assert!(!refused.is_transient());
    }
}
