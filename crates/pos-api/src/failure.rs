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
//! `handle_response` used to flatten every non-2xx response *and* every parse failure into one
//! `anyhow!("API Error ({}): {}")`, and `AuthService::verify_pin` then read any error at all as
//! grounds to fall back to offline verification:
//!
//! ```text
//! Err(e) => { warn!("Online PIN verification failed, trying offline: {}", e); }
//! ```
//!
//! So a response that *arrived* and could not be read was indistinguishable from a network that
//! was down, and both silently downgraded an authentication decision. A body that does not match
//! the contract is a **bug in one of the two systems** — it is logged and alerted as one, not
//! counted as weather.
//!
//! `handle_response` now builds these three, `verify_pin` branches on them, and only
//! [`ApiFailure::Unreachable`] reaches the local leg. [`TerminalStanding`],
//! [`OperatorSessionRefusal`] and [`CapabilityStanding`], further down, read a refusal for what it
//! says about the terminal, about the operator's session, and about the operator's authority to
//! make this particular write.

use std::fmt;

use crate::refusal_details::{
    OperatorCapabilityDeniedDetails, RefusalDetails, SupervisorApprovalRequiredDetails,
};
use pos_models::{EnrolmentState, Repudiation};
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

/// A call to the platform whose failure keeps its type.
///
/// The alias exists so a signature can say `ApiResult<T>` rather than
/// `std::result::Result<T, ApiFailure>` in modules that already have `anyhow::Result` in scope —
/// which is most of them, and where a bare `Result<T>` therefore silently means the widened one.
/// Naming the typed door is the point: the six till write methods return this, and the ~30 other
/// public signatures on the client still return `anyhow::Result`, so the difference has to be
/// legible at a glance.
pub type ApiResult<T> = std::result::Result<T, ApiFailure>;

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

// ============================================================================
// TerminalStanding
// ============================================================================

/// What a refusal says about **this terminal's** standing with the platform, as opposed to about
/// the request it refused.
///
/// # Why four answers and not one 401
///
/// `SyncService::is_auth_error` recovers the status by substring-matching `"401"` in a message
/// string, and everything that matches takes one branch: re-authenticate, and if that fails,
/// declare the terminal unregistered. Four genuinely different situations arrive that way —
///
/// | server answer | what it means | what the till should do |
/// | --- | --- | --- |
/// | 403 `POS_TERMINAL_GONE` | the device was taken away | stop, permanently |
/// | 403 `POS_TERMINAL_NOT_ACTIVE` | enrolled and not active | stop; an administrator can fix it |
/// | 401 `POS_TERMINAL_SESSION_EXPIRED` / `_TOKEN_INVALID` | the session lapsed | refresh once, retry |
/// | 409 `POS_TERMINAL_NOT_PROVISIONED` | no `secretHash` to seal with | pair again; do not retry |
///
/// — and only the third is worth a retry. Two of the four are terminal, and the two terminal ones
/// are different sentences to a human.
///
/// # This is what makes Decision 2 implementable without a server change
///
/// Design v6 held that a de-enrolled terminal answers an indistinguishable 401, because
/// de-enrolment revokes sessions and session-revoked is tested first. The second clause was false:
/// nothing revoked a session on de-enrolment, so a de-enrolled terminal still holds an unrevoked
/// one, reaches the status check, and answers **403** — a different status *and* a different code.
///
/// The order is still wrong at `terminal-auth.middleware.ts:76`, which tests `revokedAt` before
/// `terminal.status` with a comment recording it as known-wrong. So a de-enrolled terminal whose
/// session *also* expired reports as expired — which is why "expired, and the refresh was refused"
/// is treated as repudiation by the caller rather than as a second chance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TerminalStanding {
    /// The refusal was not about the terminal. Whatever it says, it says about the request.
    Unaffected,
    /// The session lapsed. The enrolment stands; refresh **once** and retry.
    ///
    /// Once, not in a loop: a retry loop against a server that keeps answering 401 is a lockout
    /// amplifier, and the endpoint behind it counts attempts.
    SessionLapsed,
    /// Enrolled, and with no `secretHash` on the platform. Pair the device again.
    NotProvisioned,
    /// The platform has disowned this terminal.
    Repudiated(Repudiation),
}

impl TerminalStanding {
    /// Reads a refusal for what it says about the terminal.
    ///
    /// Only [`ApiFailure::Refused`] can say anything: an unreachable server has made no claim
    /// about the enrolment, and a body that could not be read is a bug rather than a verdict.
    /// Both answer [`Self::Unaffected`], which is the honest reading — *this failure tells you
    /// nothing about the device*.
    pub fn of(failure: &ApiFailure) -> Self {
        let ApiFailure::Refused { code, .. } = failure else {
            return Self::Unaffected;
        };

        match code {
            ServerErrorCode::PosTerminalGone => Self::Repudiated(Repudiation::Withdrawn),
            ServerErrorCode::PosTerminalNotActive => Self::Repudiated(Repudiation::Suspended),
            ServerErrorCode::PosTerminalSessionExpired
            | ServerErrorCode::PosTerminalTokenInvalid => Self::SessionLapsed,
            ServerErrorCode::PosTerminalNotProvisioned => Self::NotProvisioned,
            _ => Self::Unaffected,
        }
    }

    /// The enrolment this standing implies.
    ///
    /// `None` when the refusal said nothing about the terminal. Routing through
    /// [`EnrolmentState::offline_authority`] from here is what makes "fell back to a repudiated
    /// enrolment because the network was down" a thing a caller has to write on purpose.
    pub const fn enrolment(self) -> Option<EnrolmentState> {
        match self {
            Self::Unaffected => None,
            Self::SessionLapsed | Self::NotProvisioned => Some(EnrolmentState::Active),
            Self::Repudiated(_) => Some(EnrolmentState::Repudiated),
        }
    }

    /// The repudiation this standing carries, if the platform disowned the terminal.
    ///
    /// An accessor rather than a `matches!` at each call site, because "is this terminal finished"
    /// is asked in more than one place and the answer must not drift between them.
    pub const fn repudiation(self) -> Option<Repudiation> {
        match self {
            Self::Repudiated(repudiation) => Some(repudiation),
            Self::Unaffected | Self::SessionLapsed | Self::NotProvisioned => None,
        }
    }

    /// Whether the till should refresh its session and try this request again.
    ///
    /// True for exactly one of the four. Exhaustive with no catch-all arm: a fifth standing has to
    /// answer this deliberately.
    pub const fn deserves_one_retry(self) -> bool {
        match self {
            Self::SessionLapsed => true,
            Self::Unaffected | Self::NotProvisioned | Self::Repudiated(_) => false,
        }
    }
}

// ============================================================================
// OperatorSessionRefusal
// ============================================================================

/// Why the platform would not accept the operator session this till presented.
///
/// `attendedOperatorAuthMiddleware` refuses in seven distinguishable ways and every one of them
/// arrives as a 401. Four are about the **session**; three are about the **operator**, and the
/// difference decides whether entering a PIN again can plausibly help.
///
/// # The two that are folded, and the two that are not
///
/// `INVALID` covers both an unknown token and one **bound to another terminal** — folded by the
/// platform on purpose, so that a refusal cannot confirm to whoever holds a stolen token that it
/// is live somewhere. Mirroring that fold here is not laziness; splitting it locally would be
/// inventing a distinction the wire deliberately does not carry.
///
/// `REVOKED` and `EXPIRED` are *not* folded, because they are different sentences: expired is
/// "sign in again", revoked is "something happened to your account". The platform tests revoked
/// **before** expired, so a session that is both reports as revoked. That ordering lives on the
/// server; nothing here re-derives it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Error)]
pub enum OperatorSessionRefusal {
    /// `POS_OPERATOR_SESSION_REQUIRED` — no session was presented.
    #[error("no operator is signed in at this till")]
    NotPresented,

    /// `POS_OPERATOR_SESSION_INVALID` — unknown, or issued to another terminal.
    #[error("this operator session is not one the platform will honour here")]
    NotHonoured,

    /// `POS_OPERATOR_SESSION_EXPIRED` — the twelve hours are up.
    #[error("this operator session has expired; sign in again")]
    Lapsed,

    /// `POS_OPERATOR_SESSION_REVOKED` — a PIN reset, a deactivation, or a rotation took it away.
    #[error("this operator session has been revoked")]
    Revoked,

    /// `POS_OPERATOR_INACTIVE` — the operator is re-read on every presentation, so employment
    /// status ends a live session mid-shift.
    #[error("this operator is no longer active")]
    OperatorInactive,

    /// `POS_OPERATOR_LOCKED` — locked elsewhere, while this session was open.
    #[error("this operator is locked")]
    OperatorLocked,

    /// `POS_OPERATOR_NOT_FOUND` — the operator behind a session the platform itself issued.
    #[error("the platform does not know this operator")]
    OperatorUnknown,
}

impl OperatorSessionRefusal {
    /// Reads a refusal for what it says about the operator's session.
    ///
    /// `None` when the refusal was about something else — including every non-`Refused` failure,
    /// because an unreachable server has made no claim about anybody's session.
    pub fn of(failure: &ApiFailure) -> Option<Self> {
        let ApiFailure::Refused { code, .. } = failure else {
            return None;
        };

        match code {
            ServerErrorCode::PosOperatorSessionRequired => Some(Self::NotPresented),
            ServerErrorCode::PosOperatorSessionInvalid => Some(Self::NotHonoured),
            ServerErrorCode::PosOperatorSessionExpired => Some(Self::Lapsed),
            ServerErrorCode::PosOperatorSessionRevoked => Some(Self::Revoked),
            ServerErrorCode::PosOperatorInactive => Some(Self::OperatorInactive),
            ServerErrorCode::PosOperatorLocked => Some(Self::OperatorLocked),
            ServerErrorCode::PosOperatorNotFound => Some(Self::OperatorUnknown),
            _ => None,
        }
    }

    /// Whether the session this till is holding must be thrown away.
    ///
    /// False for exactly one: [`Self::NotPresented`] means there was nothing to hold. Every other
    /// refusal is the platform declining a credential the till still has, and keeping it would
    /// mean presenting it again on the next request — the shape that turns one refusal into a
    /// steady stream of them.
    pub const fn discards_the_held_session(self) -> bool {
        match self {
            Self::NotPresented => false,
            Self::NotHonoured
            | Self::Lapsed
            | Self::Revoked
            | Self::OperatorInactive
            | Self::OperatorLocked
            | Self::OperatorUnknown => true,
        }
    }

    /// Whether entering a PIN again can plausibly fix this.
    ///
    /// False for the three that are about the operator rather than the session: a locked,
    /// deactivated or unknown operator will be refused at the PIN too, and a till that answers
    /// "please sign in again" to a locked cashier sends them round a loop that spends their
    /// lockout budget on every pass.
    pub const fn a_pin_can_fix_it(self) -> bool {
        match self {
            Self::NotPresented | Self::NotHonoured | Self::Lapsed | Self::Revoked => true,
            Self::OperatorInactive | Self::OperatorLocked | Self::OperatorUnknown => false,
        }
    }
}

#[cfg(test)]
mod operator_session_refusal_tests {
    use super::*;

    fn refusal(code: ServerErrorCode) -> ApiFailure {
        ApiFailure::Refused {
            status: StatusCode::UNAUTHORIZED,
            code,
            message: "refused".to_string(),
            details: None,
        }
    }

    /// The seven, each read from its own code, as a table.
    #[test]
    fn each_operator_code_gets_its_own_refusal() {
        let table = [
            (
                ServerErrorCode::PosOperatorSessionRequired,
                OperatorSessionRefusal::NotPresented,
            ),
            (
                ServerErrorCode::PosOperatorSessionInvalid,
                OperatorSessionRefusal::NotHonoured,
            ),
            (
                ServerErrorCode::PosOperatorSessionExpired,
                OperatorSessionRefusal::Lapsed,
            ),
            (
                ServerErrorCode::PosOperatorSessionRevoked,
                OperatorSessionRefusal::Revoked,
            ),
            (
                ServerErrorCode::PosOperatorInactive,
                OperatorSessionRefusal::OperatorInactive,
            ),
            (
                ServerErrorCode::PosOperatorLocked,
                OperatorSessionRefusal::OperatorLocked,
            ),
            (
                ServerErrorCode::PosOperatorNotFound,
                OperatorSessionRefusal::OperatorUnknown,
            ),
        ];

        for (code, expected) in table {
            assert_eq!(
                OperatorSessionRefusal::of(&refusal(code.clone())),
                Some(expected),
                "for {code:?}"
            );
        }

        // A refusal about the PIN, not about a session. `None` and not a default — this is what
        // keeps `verify-pin`'s own refusals out of the sign-out path.
        assert_eq!(
            OperatorSessionRefusal::of(&refusal(ServerErrorCode::PosPinInvalid)),
            None
        );
        // And a terminal-level refusal, which `TerminalStanding` answers instead.
        assert_eq!(
            OperatorSessionRefusal::of(&refusal(ServerErrorCode::PosTerminalGone)),
            None
        );
    }

    /// Exactly one refusal leaves the held session in place, and exactly three cannot be fixed by
    /// entering a PIN.
    #[test]
    fn the_two_questions_a_caller_asks_have_different_answers() {
        assert!(!OperatorSessionRefusal::NotPresented.discards_the_held_session());
        for refusal in [
            OperatorSessionRefusal::NotHonoured,
            OperatorSessionRefusal::Lapsed,
            OperatorSessionRefusal::Revoked,
            OperatorSessionRefusal::OperatorInactive,
            OperatorSessionRefusal::OperatorLocked,
            OperatorSessionRefusal::OperatorUnknown,
        ] {
            assert!(
                refusal.discards_the_held_session(),
                "{refusal:?} is the platform declining a credential the till still holds"
            );
        }

        // The three that are about the operator rather than the session. Sending a locked cashier
        // back to the PIN pad spends their lockout budget on every pass.
        for refusal in [
            OperatorSessionRefusal::OperatorInactive,
            OperatorSessionRefusal::OperatorLocked,
            OperatorSessionRefusal::OperatorUnknown,
        ] {
            assert!(!refusal.a_pin_can_fix_it(), "{refusal:?}");
        }
        for refusal in [
            OperatorSessionRefusal::NotPresented,
            OperatorSessionRefusal::NotHonoured,
            OperatorSessionRefusal::Lapsed,
            OperatorSessionRefusal::Revoked,
        ] {
            assert!(refusal.a_pin_can_fix_it(), "{refusal:?}");
        }
    }
}

#[cfg(test)]
mod terminal_standing_tests {
    use super::*;

    fn refusal(status: u16, code: ServerErrorCode) -> ApiFailure {
        ApiFailure::Refused {
            status: StatusCode::from_u16(status).expect("a real status"),
            code,
            message: "refused".to_string(),
            details: None,
        }
    }

    /// The four-way table, read from the machine code the platform sends.
    ///
    /// Written as data rather than as four tests because the point is that the four answers are
    /// *different*: a mapping collapsed onto one answer is the defect, and a table shows the
    /// collapse where four separate assertions would each still pass.
    #[test]
    fn each_terminal_code_gets_its_own_standing() {
        let table = [
            (
                403,
                ServerErrorCode::PosTerminalGone,
                TerminalStanding::Repudiated(Repudiation::Withdrawn),
            ),
            (
                403,
                ServerErrorCode::PosTerminalNotActive,
                TerminalStanding::Repudiated(Repudiation::Suspended),
            ),
            (
                401,
                ServerErrorCode::PosTerminalSessionExpired,
                TerminalStanding::SessionLapsed,
            ),
            (
                401,
                ServerErrorCode::PosTerminalTokenInvalid,
                TerminalStanding::SessionLapsed,
            ),
            (
                409,
                ServerErrorCode::PosTerminalNotProvisioned,
                TerminalStanding::NotProvisioned,
            ),
            // About the PIN, not about the device — and the reason the standing is consulted
            // before the refusal is mapped.
            (
                401,
                ServerErrorCode::PosPinInvalid,
                TerminalStanding::Unaffected,
            ),
            (
                401,
                ServerErrorCode::PosOperatorLocked,
                TerminalStanding::Unaffected,
            ),
        ];

        for (status, code, expected) in table {
            assert_eq!(
                TerminalStanding::of(&refusal(status, code.clone())),
                expected,
                "for {code:?}"
            );
        }
    }

    /// Exactly one of the four is worth asking again, and only the two repudiations forbid the
    /// local leg. `enrolment()` is `None` for `Unaffected` because a refusal about the PIN says
    /// nothing about the enrolment — reading it as `Active` would be an assertion nobody made.
    #[test]
    fn only_a_lapsed_session_is_worth_a_retry() {
        assert!(TerminalStanding::SessionLapsed.deserves_one_retry());
        for standing in [
            TerminalStanding::Unaffected,
            TerminalStanding::NotProvisioned,
            TerminalStanding::Repudiated(Repudiation::Withdrawn),
            TerminalStanding::Repudiated(Repudiation::Suspended),
        ] {
            assert!(
                !standing.deserves_one_retry(),
                "{standing:?} must not be retried"
            );
        }

        assert_eq!(TerminalStanding::Unaffected.enrolment(), None);
        assert_eq!(
            TerminalStanding::SessionLapsed.enrolment(),
            Some(EnrolmentState::Active)
        );
        assert_eq!(
            TerminalStanding::NotProvisioned.enrolment(),
            Some(EnrolmentState::Active)
        );
        assert_eq!(
            TerminalStanding::Repudiated(Repudiation::Suspended).enrolment(),
            Some(EnrolmentState::Repudiated)
        );

        // The property the local leg depends on: a repudiated enrolment confers no offline
        // authority, whatever the expiry says.
        let not_after = pos_models::CredentialExpiry::at(
            chrono::DateTime::parse_from_rfc3339("2099-01-01T00:00:00Z")
                .expect("a literal instant")
                .with_timezone(&chrono::Utc),
        );
        assert_eq!(
            EnrolmentState::Repudiated.offline_authority(not_after),
            None
        );
    }

    /// `repudiation()` is the accessor the sync loop asks before it tries to renew anything.
    #[test]
    fn only_a_repudiated_standing_carries_a_repudiation() {
        assert_eq!(
            TerminalStanding::Repudiated(Repudiation::Withdrawn).repudiation(),
            Some(Repudiation::Withdrawn)
        );
        assert_eq!(TerminalStanding::Unaffected.repudiation(), None);
        assert_eq!(TerminalStanding::SessionLapsed.repudiation(), None);
        assert_eq!(TerminalStanding::NotProvisioned.repudiation(), None);
    }
}

// ============================================================================
// CapabilityStanding
// ============================================================================

/// What a refusal says about the operator's authority to make **this** write.
///
/// # Why two answers and not one 403
///
/// All three till-facing refusals of this kind are `403`, and the till must branch on the code
/// rather than the status, because the two that matter mean opposite things to the person standing
/// at the drawer:
///
/// | code | means | the honest sentence |
/// | --- | --- | --- |
/// | `POS_SUPERVISOR_APPROVAL_REQUIRED` | a role **above** this one holds it | fetch one of these people |
/// | `POS_OPERATOR_CAPABILITY_DENIED` | **no** operator role holds it at all | not available at a till |
///
/// Collapsing them into one "forbidden" is a defect rather than a simplification. Rendering
/// *denied* as "fetch a supervisor" sends a cashier to fetch someone who is refused in turn: it
/// wastes a trip and teaches the shop that the prompt is noise, which is worse than a flat 403,
/// because a flat 403 at least does not lie.
///
/// # The till reads `heldBy`; it never carries a role table
///
/// [`Self::SupervisorHolds`] carries the roles the platform named, lowest first. A client that
/// hard-codes "refunds need a supervisor" is a second copy of the role table on the far side of a
/// network boundary, updated by a separate release train — the arrangement that let the platform's
/// CSRF exemption list drift from the guards it described. A refusal that names the roles is the
/// server holding the ladder; a client that infers them is the ladder copied.
///
/// The worked example is a refund. `CASHIER_CAPABILITIES` is `['POS_READ', 'POS_CREATE']`
/// (`operator-capabilities.ts:66`) and `POS_REFUND` sits at SUPERVISOR (`:76-80`), deliberately —
/// a refund moves money out of the drawer against an already-paid sale. An unattended terminal
/// holds the same two capabilities (`:136-137`) and is refused at the same gate with the same
/// code, one tier further down.
///
/// # `POS_ATTRIBUTION_REQUIRES_OPERATOR` is deliberately absent
///
/// It is unreachable from a till, so an arm for it would be dead code. `buildPosRouter` wires
/// `requirePosCapability` **ahead** of the handler (`pos-route-table.ts:124-130`), and every route
/// that reaches `requirePrincipal` declares a capability an unattended terminal does not hold —
/// returns `POS_REFUND`, void `POS_VOID`, and the z-report is back-office-audienced. So the
/// capability gate refuses first and the attribution check is never reached. It goes live only if
/// someone relaxes `POS_REFUND` on the returns row or adds a till-audienced route at `POS_CREATE`;
/// that is a property of three capability declarations, not of this enum.
///
/// # The open-set arm, stated rather than discovered
///
/// Like [`TerminalStanding::of`] and [`OperatorSessionRefusal::of`], [`Self::of`] ends in a
/// catch-all, so a `ServerErrorCode` variant added later reads as [`Self::Unaffected`] here with
/// no compiler complaint. That is the deliberate trade the two neighbours already make — the code
/// set is open by construction (`errorCodeFor` returns `UNKNOWN_ERROR` for anything outside its
/// table) so exhaustiveness here would be a fiction. `refusal_details::RefusalDetails::parse` is
/// the one that *is* exhaustive, and it is where a new code gets caught.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CapabilityStanding {
    /// The refusal was not about the operator's authority. Whatever it says, it says about the
    /// request.
    Unaffected,

    /// A role above this operator holds what was refused.
    ///
    /// The payload is `None` when the platform sent the code without readable details — a contract
    /// breach [`RefusalDetails::read`] has already logged, and one that still leaves an honest
    /// sentence: *someone with more authority can do this*. **Do not fabricate the roles, and do
    /// not read a missing list as [`Self::NoOperatorRoleHolds`].** The first invents a person for
    /// a cashier to go and find; the second sends them away from help that exists.
    SupervisorHolds(Option<SupervisorApprovalRequiredDetails>),

    /// No operator role holds it at all, so escalating at the till cannot help.
    ///
    /// The person who can do this is signed into the admin UI, not standing at the drawer. The
    /// payload names the capability when the platform sent it.
    NoOperatorRoleHolds(Option<OperatorCapabilityDeniedDetails>),
}

impl CapabilityStanding {
    /// Reads a refusal for what it says about the operator's authority.
    ///
    /// Only [`ApiFailure::Refused`] can say anything: an unreachable server has made no claim
    /// about anybody's authority, and a body that could not be read is a bug rather than a
    /// verdict. Both answer [`Self::Unaffected`] — *this failure tells you nothing about what the
    /// operator may do*.
    pub fn of(failure: &ApiFailure) -> Self {
        let ApiFailure::Refused { code, details, .. } = failure else {
            return Self::Unaffected;
        };

        match code {
            ServerErrorCode::PosSupervisorApprovalRequired => {
                Self::SupervisorHolds(match details {
                    Some(RefusalDetails::SupervisorApprovalRequired(approval)) => {
                        Some(approval.clone())
                    }
                    _ => None,
                })
            }
            ServerErrorCode::PosOperatorCapabilityDenied => {
                Self::NoOperatorRoleHolds(match details {
                    Some(RefusalDetails::OperatorCapabilityDenied(denial)) => Some(denial.clone()),
                    _ => None,
                })
            }
            _ => Self::Unaffected,
        }
    }

    /// Whether fetching someone more senior, here, now, can get this write through.
    ///
    /// True for exactly one of the three. Exhaustive with no catch-all arm: a fourth standing has
    /// to answer this deliberately, because answering it wrongly is the whole defect — a `true`
    /// here on a denied capability is what sends a cashier to fetch a person who is refused in
    /// turn.
    pub const fn escalating_at_the_till_can_help(&self) -> bool {
        match self {
            Self::SupervisorHolds(_) => true,
            Self::Unaffected | Self::NoOperatorRoleHolds(_) => false,
        }
    }
}

#[cfg(test)]
mod capability_standing_tests {
    use super::*;

    fn refusal(code: ServerErrorCode, details: Option<RefusalDetails>) -> ApiFailure {
        ApiFailure::Refused {
            status: StatusCode::FORBIDDEN,
            code,
            message: "refused".to_string(),
            details,
        }
    }

    fn supervisor_details() -> RefusalDetails {
        RefusalDetails::SupervisorApprovalRequired(SupervisorApprovalRequiredDetails {
            capability: crate::refusal_details::CapabilityCode::new("POS_REFUND".to_string())
                .expect("a fixture capability is never blank"),
            held_by: crate::refusal_details::HeldBy::new(vec![
                pos_models::OperatorRole::Supervisor,
                pos_models::OperatorRole::Manager,
            ])
            .expect("a fixture role list is never empty"),
        })
    }

    /// The two codes get two standings, and the roles survive.
    #[test]
    fn a_supervisor_refusal_names_who_can_supply_it() {
        let standing = CapabilityStanding::of(&refusal(
            ServerErrorCode::PosSupervisorApprovalRequired,
            Some(supervisor_details()),
        ));

        let CapabilityStanding::SupervisorHolds(Some(approval)) = standing else {
            panic!("expected a named supervisor standing, got: {standing:?}");
        };
        assert_eq!(approval.capability.as_str(), "POS_REFUND");
        assert_eq!(
            approval.held_by.lowest(),
            pos_models::OperatorRole::Supervisor
        );
    }

    /// The control for the test above, and the reason this type exists.
    ///
    /// A denial and an approval-required are both 403 and both about a capability. If the two
    /// collapsed onto one standing, the test above would pass against a till that renders every
    /// 403 as "fetch a supervisor" — which is precisely the defect.
    #[test]
    fn a_denied_capability_is_not_a_supervisor_refusal() {
        let standing = CapabilityStanding::of(&refusal(
            ServerErrorCode::PosOperatorCapabilityDenied,
            Some(RefusalDetails::OperatorCapabilityDenied(
                OperatorCapabilityDeniedDetails {
                    capability: crate::refusal_details::CapabilityCode::new(
                        "POS_MANAGE".to_string(),
                    )
                    .expect("a fixture capability is never blank"),
                },
            )),
        ));

        let CapabilityStanding::NoOperatorRoleHolds(Some(denial)) = &standing else {
            panic!("expected a denial, got: {standing:?}");
        };
        assert_eq!(denial.capability.as_str(), "POS_MANAGE");
        assert!(!standing.escalating_at_the_till_can_help());
    }

    /// A supervisor code whose `heldBy` did not survive is still a supervisor code.
    ///
    /// `RefusalDetails::read` answers `None` for a details payload it cannot parse, so this state
    /// is reachable rather than hypothetical. Reading it as a denial would send a cashier away
    /// from help that exists; inventing a role list would put a name in front of them that the
    /// platform never sent.
    #[test]
    fn a_supervisor_refusal_with_no_roles_is_still_escalatable() {
        let standing = CapabilityStanding::of(&refusal(
            ServerErrorCode::PosSupervisorApprovalRequired,
            None,
        ));

        assert_eq!(standing, CapabilityStanding::SupervisorHolds(None));
        assert!(standing.escalating_at_the_till_can_help());
    }

    /// Details of the wrong shape beside the right code do not silently become the right shape.
    #[test]
    fn details_belonging_to_another_code_are_not_borrowed() {
        let standing = CapabilityStanding::of(&refusal(
            ServerErrorCode::PosOperatorCapabilityDenied,
            Some(supervisor_details()),
        ));

        assert_eq!(standing, CapabilityStanding::NoOperatorRoleHolds(None));
    }

    /// Every failure that is not a refusal says nothing about authority.
    ///
    /// The `Unaffected` half of the partition, and the one that keeps the offline path honest: a
    /// server nobody reached has not denied anybody anything.
    #[test]
    fn only_a_refusal_can_speak_to_authority() {
        assert_eq!(
            CapabilityStanding::of(&refusal(ServerErrorCode::Forbidden, None)),
            CapabilityStanding::Unaffected,
            "a bare 403 carries no capability verdict — that is why the codes exist"
        );
        assert_eq!(
            CapabilityStanding::of(&ApiFailure::Unreadable(
                serde_json::from_str::<u8>("{").expect_err("this is not a u8")
            )),
            CapabilityStanding::Unaffected
        );
        assert!(!CapabilityStanding::Unaffected.escalating_at_the_till_can_help());
    }
}
