//! Write-outcome bridge — view-model conversion.
//!
//! Flattens [`PlatformSync`] into the shape a screen renders after a sale, a shift change or a
//! refund. The whole job of this module is a distinction the view must not be allowed to lose.
//!
//! # Two refusals, and rendering them alike is the defect
//!
//! Both arrive as `403`, and they mean opposite things to the person standing at the drawer:
//!
//! - `POS_SUPERVISOR_APPROVAL_REQUIRED` — a role **above** this one holds the capability, and the
//!   platform names which roles. The cashier can fetch one of those people and the write goes
//!   through.
//! - `POS_OPERATOR_CAPABILITY_DENIED` — **no** operator role holds it at all. Whoever is fetched
//!   is refused in turn.
//!
//! Rendering the second as "fetch a supervisor" wastes a trip and teaches the shop that the
//! prompt is noise, which is worse than a flat 403 because a flat 403 does not lie. So this bridge
//! does not produce a string: it produces a **sum type**, and the two live in different variants,
//! because a `message: String` field is exactly where the two would silently become one again.
//!
//! # No prose, and no local role table
//!
//! Nothing here contains a sentence. Arabic is the default locale, so the wording belongs to the
//! view layer, which has the locale; this module carries the *distinction* and the *data*.
//!
//! For the same reason [`WriteOutcomeModel::NeedsSomeoneMoreSenior`] carries the roles the
//! platform sent rather than any rule about which roles those are. A till that hard-codes "refunds
//! need a supervisor" is a second copy of the server's role table on the far side of a network
//! boundary, updated by a separate release train.

use pos_api::CapabilityStanding;
use pos_models::OperatorRole;
use pos_services::PlatformSync;

/// What a screen shows about a write the till has just made.
///
/// Six variants and no `bool`. The three that a `synced == false` used to cover are the three the
/// cashier needs told apart: it is safe on the till, someone can unblock it, or nobody here can.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WriteOutcomeModel {
    /// The platform holds it. There is nothing to tell anyone.
    Recorded,

    /// It is on this till and will go up when the network comes back.
    ///
    /// The reassuring case, and the only one that is ordinary. An offline-first till says so and
    /// carries on selling.
    HeldOnThisTill,

    /// A role above this operator's can authorise it, and these are the roles that can.
    ///
    /// `roles` is never empty when the platform named them, and is ordered lowest first, so a
    /// screen that shows only the first shows the *least* senior person who can help — the one
    /// most likely to be in the building. It **is** empty when the platform sent the code without
    /// readable details; the honest render for that is "someone with more authority", never a
    /// guessed role and never [`Self::NotSomethingATillCanDo`].
    NeedsSomeoneMoreSenior {
        /// The capability that was refused, when the platform named it.
        capability: Option<String>,
        /// Who holds it, lowest role first.
        roles: Vec<OperatorRole>,
    },

    /// No operator role holds this capability. Fetching anyone at the till cannot help.
    NotSomethingATillCanDo {
        /// The capability that was refused, when the platform named it.
        capability: Option<String>,
    },

    /// The platform refused for a reason that is not about the operator's authority.
    ///
    /// The code is the branchable fact; the message is the platform's own sentence, offered as a
    /// last resort for a screen that has nothing better to show. Do not branch on it — messages
    /// are translated, and this one may well arrive in Arabic.
    RefusedForAnotherReason {
        /// The machine code, in its wire spelling.
        code: String,
        /// The platform's message, for display only.
        message: String,
    },

    /// The platform answered and the till could not read the answer.
    ///
    /// Deliberately **not** merged with [`Self::HeldOnThisTill`]. Telling a cashier "it will sync
    /// later" is a promise the till cannot keep here: it does not know whether the platform
    /// recorded the write, so it can neither replay it nor declare it refused.
    Undetermined,
}

impl WriteOutcomeModel {
    /// Reads a [`PlatformSync`] for what to put on the screen.
    pub fn of(sync: &PlatformSync) -> Self {
        let refused = match sync {
            PlatformSync::Recorded(_) => return Self::Recorded,
            PlatformSync::Queued => return Self::HeldOnThisTill,
            PlatformSync::Undetermined => return Self::Undetermined,
            PlatformSync::Refused(refused) => refused,
        };

        match &refused.authority {
            CapabilityStanding::SupervisorHolds(approval) => Self::NeedsSomeoneMoreSenior {
                capability: approval.as_ref().map(|a| a.capability.as_str().to_string()),
                roles: approval
                    .as_ref()
                    .map(|a| a.held_by.iter().collect())
                    .unwrap_or_default(),
            },
            CapabilityStanding::NoOperatorRoleHolds(denial) => Self::NotSomethingATillCanDo {
                capability: denial.as_ref().map(|d| d.capability.as_str().to_string()),
            },
            CapabilityStanding::Unaffected => Self::RefusedForAnotherReason {
                code: refused.code.as_wire_str().to_string(),
                message: refused.message.clone(),
            },
        }
    }

    /// Whether the screen should offer to fetch someone.
    ///
    /// True for exactly one variant. Exhaustive with no catch-all: a seventh outcome has to answer
    /// this deliberately, because answering it wrongly is the entire defect this module exists to
    /// prevent.
    pub const fn offers_escalation(&self) -> bool {
        match self {
            Self::NeedsSomeoneMoreSenior { .. } => true,
            Self::Recorded
            | Self::HeldOnThisTill
            | Self::NotSomethingATillCanDo { .. }
            | Self::RefusedForAnotherReason { .. }
            | Self::Undetermined => false,
        }
    }

    /// Whether the till may carry on as though the write is safe.
    ///
    /// True for the two that are not a problem: the platform has it, or the till does and the
    /// queue will deliver it. Every refusal and the unreadable answer are false — a screen that
    /// treats them as fine is the flag this type replaced.
    pub const fn is_settled(&self) -> bool {
        match self {
            Self::Recorded | Self::HeldOnThisTill => true,
            Self::NeedsSomeoneMoreSenior { .. }
            | Self::NotSomethingATillCanDo { .. }
            | Self::RefusedForAnotherReason { .. }
            | Self::Undetermined => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pos_api::refusal_details::{
        CapabilityCode, HeldBy, OperatorCapabilityDeniedDetails, SupervisorApprovalRequiredDetails,
    };
    use pos_api::{ApiFailure, RefusalDetails, ServerErrorCode, StatusCode};

    fn capability(code: &str) -> CapabilityCode {
        CapabilityCode::new(code.to_string()).expect("a fixture capability is never blank")
    }

    fn sync_of(code: ServerErrorCode, details: Option<RefusalDetails>) -> PlatformSync {
        PlatformSync::of(Err(ApiFailure::Refused {
            status: StatusCode::FORBIDDEN,
            code,
            message: "غير مسموح".to_string(),
            details,
        }))
    }

    /// A cashier attempting a refund is told who can authorise it, lowest first.
    #[test]
    fn a_supervisor_refusal_names_the_roles_lowest_first() {
        let model = WriteOutcomeModel::of(&sync_of(
            ServerErrorCode::PosSupervisorApprovalRequired,
            Some(RefusalDetails::SupervisorApprovalRequired(
                SupervisorApprovalRequiredDetails {
                    capability: capability("POS_REFUND"),
                    held_by: HeldBy::new(vec![OperatorRole::Supervisor, OperatorRole::Manager])
                        .expect("a fixture role list is never empty"),
                },
            )),
        ));

        assert_eq!(
            model,
            WriteOutcomeModel::NeedsSomeoneMoreSenior {
                capability: Some("POS_REFUND".to_string()),
                roles: vec![OperatorRole::Supervisor, OperatorRole::Manager],
            }
        );
        assert!(model.offers_escalation());
        assert!(!model.is_settled());
    }

    /// The control, and the reason this file exists.
    ///
    /// The test above passes against a bridge that renders every 403 as "fetch a supervisor". This
    /// one does not: a denied capability must reach a different variant and must **not** offer
    /// escalation, because whoever the cashier fetches is refused in turn.
    #[test]
    fn a_denied_capability_does_not_send_anyone_to_fetch_a_supervisor() {
        let model = WriteOutcomeModel::of(&sync_of(
            ServerErrorCode::PosOperatorCapabilityDenied,
            Some(RefusalDetails::OperatorCapabilityDenied(
                OperatorCapabilityDeniedDetails {
                    capability: capability("POS_MANAGE"),
                },
            )),
        ));

        assert_eq!(
            model,
            WriteOutcomeModel::NotSomethingATillCanDo {
                capability: Some("POS_MANAGE".to_string()),
            }
        );
        assert!(!model.offers_escalation());
    }

    /// A supervisor code whose roles did not survive still offers escalation, with no roles named.
    ///
    /// Reachable rather than hypothetical: `RefusalDetails::read` answers `None` for a payload it
    /// cannot parse. The screen says "someone with more authority"; it does not guess a role, and
    /// it does not fall through to "not available at a till".
    #[test]
    fn a_supervisor_refusal_with_no_roles_still_offers_escalation() {
        let model = WriteOutcomeModel::of(&sync_of(
            ServerErrorCode::PosSupervisorApprovalRequired,
            None,
        ));

        assert_eq!(
            model,
            WriteOutcomeModel::NeedsSomeoneMoreSenior {
                capability: None,
                roles: vec![],
            }
        );
        assert!(model.offers_escalation());
    }

    /// The unknown-code fall-through, asserted rather than left implicit.
    ///
    /// `RefusalDetails::read` is total and yields `None` for a code this till has not been taught,
    /// so this arm is reached by every refusal that is not about authority — including a bare 403.
    #[test]
    fn a_code_this_till_does_not_know_is_shown_as_itself() {
        let model = WriteOutcomeModel::of(&sync_of(
            ServerErrorCode::from("POS_SOMETHING_NEW".to_string()),
            None,
        ));

        let WriteOutcomeModel::RefusedForAnotherReason { code, message } = model else {
            panic!("an unknown code must not be guessed into a capability refusal");
        };
        assert_eq!(code, "POS_SOMETHING_NEW");
        assert_eq!(
            message, "غير مسموح",
            "the platform's own sentence is carried for display, untranslated and unparsed"
        );
    }

    /// The two that are not a problem, and the one that must not be mistaken for them.
    #[test]
    fn only_recorded_and_queued_are_settled() {
        assert!(WriteOutcomeModel::of(&PlatformSync::Recorded("srv-1".to_string())).is_settled());
        assert!(WriteOutcomeModel::of(&PlatformSync::Queued).is_settled());
        assert!(
            !WriteOutcomeModel::of(&PlatformSync::Undetermined).is_settled(),
            "an unreadable answer is not a promise that the queue will deliver it"
        );
        assert_eq!(
            WriteOutcomeModel::of(&PlatformSync::Undetermined),
            WriteOutcomeModel::Undetermined
        );
    }
}
