//! What the platform did with a record the till handed it.
//!
//! # The shape this replaces
//!
//! Three services reported the same thing the same wrong way: a `synced: bool`, in two cases
//! beside an `Option<String>` server id that is `None` exactly when the boolean is false. This
//! repo has already deleted that shape once by name — `PinVerificationResult { valid: bool,
//! operator_name: String, … }` became [`pos_models::PinVerification`] for it — and the conventions
//! state the rule: *a boolean plus fields that are empty when it is false is a sum type spelled
//! wrong.*
//!
//! It was not merely inelegant. `false` was reached from four situations that demand four
//! different things of the till:
//!
//! | situation | what the till must do |
//! | --- | --- |
//! | the platform recorded it | nothing; keep its identifier |
//! | nobody answered | keep the local copy and let the queue replay it |
//! | the platform refused | tell the cashier what it said; **never** replay |
//! | the answer was unreadable | neither; a human has to look |
//!
//! Collapsing the last three into one `false` is what let a refused sale sit in the offline queue
//! being retried forever against a server that had already declined it — and what left the cashier
//! with no sentence to read.
//!
//! # Why the unreadable case is not folded into either neighbour
//!
//! [`Self::Undetermined`] is the honest answer to `ApiFailure::Unreadable` and it is genuinely
//! different from both neighbours. The platform answered, so "nobody was there" is false. The till
//! could not read the answer, so "it refused" is a claim nobody can support. And replaying is not
//! obviously safe: if the platform *did* record the sale, a replay double-posts it; if it did not,
//! not replaying loses it. Undetermined is what the till knows, and saying so is the same
//! discipline `PinVerification::Undetermined` already applies to a PIN nobody could check.

use pos_api::{ApiFailure, CapabilityStanding, ServerErrorCode};

/// What became of a record the till handed to the platform.
///
/// Constructed from an [`ApiResult`](pos_api::ApiResult) with [`Self::of`], so the mapping from
/// the three transport failures lives in one place rather than being re-decided at each of the
/// six write sites.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlatformSync {
    /// The platform recorded it, under this identifier.
    ///
    /// A bare `String` because that is what `pos-db` stores in `server_id` and what every consumer
    /// downstream still passes around; a newtype confined to this crate would buy a runtime parse
    /// at the boundary rather than a compiler refusal. `TerminalId` and `CompanyId` are owed the
    /// same treatment and for the same reason are not being taken here.
    Recorded(String),

    /// Nobody answered. The local copy stands and the offline queue will replay it.
    ///
    /// The only one of the four that is ordinary weather, and the only one an offline-first till
    /// absorbs silently.
    Queued,

    /// The platform answered, and the answer was no. Replaying will not change it.
    Refused(WriteRefused),

    /// The platform answered and the till could not read the answer.
    ///
    /// A contract breach, already logged at `error!` by the transport. Not a refusal, not weather,
    /// and not safe to replay — see the module header.
    Undetermined,
}

impl PlatformSync {
    /// Reads the outcome of one write.
    ///
    /// Exhaustive over [`ApiFailure`] with no catch-all arm: a fourth transport failure has to
    /// answer this deliberately rather than inheriting whichever neighbour it happens to resemble.
    pub fn of(outcome: Result<String, ApiFailure>) -> Self {
        match outcome {
            Ok(server_id) => Self::Recorded(server_id),
            Err(ApiFailure::Unreachable(_)) => Self::Queued,
            Err(ApiFailure::Unreadable(_)) => Self::Undetermined,
            Err(failure @ ApiFailure::Refused { .. }) => Self::Refused(WriteRefused::of(&failure)),
        }
    }

    /// The platform's identifier for the record, when it made one.
    pub fn server_id(&self) -> Option<&str> {
        match self {
            Self::Recorded(id) => Some(id),
            Self::Queued | Self::Refused(_) | Self::Undetermined => None,
        }
    }

    /// Whether the offline queue should try this record again.
    ///
    /// True for exactly one of the four, and the one that matters is [`Self::Refused`] answering
    /// **false**: a decision the platform has already made does not become a different decision on
    /// the tenth attempt, and a queue that keeps presenting it is how a refused sale becomes a
    /// steady stream of refusals.
    pub const fn deserves_a_replay(&self) -> bool {
        match self {
            Self::Queued => true,
            Self::Recorded(_) | Self::Refused(_) | Self::Undetermined => false,
        }
    }

    /// Whether the platform holds this record.
    ///
    /// The narrow reading of the `synced` flag this type replaces, kept because callers really do
    /// ask it — but kept as a question about [`Self::Recorded`] alone, so that "not recorded" can
    /// no longer be mistaken for "not refused".
    pub const fn is_recorded(&self) -> bool {
        matches!(self, Self::Recorded(_))
    }
}

/// A refusal, in the form the till can act on and show.
///
/// A projection of `ApiFailure::Refused` rather than the failure itself, because it has to be
/// `Clone` to sit in a result a caller passes around, and `ApiFailure` carries `reqwest::Error`
/// and `serde_json::Error`, neither of which clone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriteRefused {
    /// The machine code. **Branch on this, never on [`Self::message`]** — messages are translated,
    /// arrive in Arabic when the fallback locale is used, and are not a contract.
    pub code: ServerErrorCode,

    /// What the code says about the operator's authority to make this write.
    ///
    /// Derived from the code and its details once, here, rather than re-derived at each render
    /// site — which is what keeps two screens from disagreeing about whether fetching a supervisor
    /// would help.
    pub authority: CapabilityStanding,

    /// The platform's own sentence. For a log and for a last-resort display; never for a branch.
    pub message: String,
}

impl WriteRefused {
    /// Reads a refusal. Anything that is not one yields a `Forbidden`-coded value with an
    /// `Unaffected` standing, which is unreachable through [`PlatformSync::of`] and exists so this
    /// constructor is total.
    fn of(failure: &ApiFailure) -> Self {
        let authority = CapabilityStanding::of(failure);
        match failure {
            ApiFailure::Refused { code, message, .. } => Self {
                code: code.clone(),
                authority,
                message: message.clone(),
            },
            other => Self {
                code: ServerErrorCode::Forbidden,
                authority,
                message: other.to_string(),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pos_api::refusal_details::{HeldBy, SupervisorApprovalRequiredDetails};
    use pos_api::{CapabilityCode, RefusalDetails, StatusCode};
    use pos_models::OperatorRole;

    fn refused(code: ServerErrorCode, details: Option<RefusalDetails>) -> PlatformSync {
        PlatformSync::of(Err(ApiFailure::Refused {
            status: StatusCode::FORBIDDEN,
            code,
            message: "refused".to_string(),
            details,
        }))
    }

    /// The four answers are four, and the pair that used to be one `false` is the point.
    #[test]
    fn a_refusal_and_an_unanswered_request_are_not_the_same_outcome() {
        let queued = PlatformSync::of(Err(ApiFailure::Unreadable(
            serde_json::from_str::<u8>("{").expect_err("this is not a u8"),
        )));
        assert_eq!(queued, PlatformSync::Undetermined);

        let refusal = refused(ServerErrorCode::PosOperatorCapabilityDenied, None);
        assert!(!refusal.is_recorded());
        assert!(!refusal.deserves_a_replay());

        assert_eq!(
            PlatformSync::of(Ok("srv-1".to_string())).server_id(),
            Some("srv-1")
        );
    }

    /// A refusal must never be replayed, and this is the assertion that says so.
    ///
    /// The behaviour the `synced: bool` hid: a refused sale went into the queue looking exactly
    /// like an unsent one, and was retried against a server that had already declined it. Only
    /// `Queued` earns a replay.
    #[test]
    fn only_an_unanswered_request_earns_a_replay() {
        assert!(PlatformSync::Queued.deserves_a_replay());
        assert!(!PlatformSync::Recorded("srv-1".to_string()).deserves_a_replay());
        assert!(!PlatformSync::Undetermined.deserves_a_replay());
        assert!(!refused(ServerErrorCode::Forbidden, None).deserves_a_replay());
    }

    /// The capability standing rides along, so the render does not have to re-derive it.
    ///
    /// Re-deriving it at each screen is how two screens come to disagree about whether fetching a
    /// supervisor would help.
    #[test]
    fn a_refusal_carries_its_capability_standing() {
        let sync = refused(
            ServerErrorCode::PosSupervisorApprovalRequired,
            Some(RefusalDetails::SupervisorApprovalRequired(
                SupervisorApprovalRequiredDetails {
                    capability: CapabilityCode::new("POS_REFUND".to_string())
                        .expect("a fixture capability is never blank"),
                    held_by: HeldBy::new(vec![OperatorRole::Supervisor])
                        .expect("a fixture role list is never empty"),
                },
            )),
        );

        let PlatformSync::Refused(refused) = sync else {
            panic!("expected a refusal");
        };
        assert_eq!(refused.code, ServerErrorCode::PosSupervisorApprovalRequired);
        assert!(refused.authority.escalating_at_the_till_can_help());
    }

    /// The control for the test above: a refusal that is not about authority does not acquire one.
    #[test]
    fn a_refusal_about_something_else_offers_no_escalation() {
        let PlatformSync::Refused(refused) = refused(ServerErrorCode::ValidationError, None) else {
            panic!("expected a refusal");
        };
        assert!(!refused.authority.escalating_at_the_till_can_help());
    }
}
