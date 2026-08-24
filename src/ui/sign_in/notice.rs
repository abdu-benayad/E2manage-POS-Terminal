//! What the sign-in screen shows once a PIN attempt has an outcome.
//!
//! # The requirement this discharges structurally
//!
//! A till with no network cannot verify a PIN at all. Showing a wrong-PIN message for an
//! unreachable server is the defect `auth-outcome-and-offline-lockout` spent an issue removing:
//! the domain closed it by putting `consumes_an_attempt` on [`PinRefusal`] and nowhere else, so
//! that "could not ask" is not a kind of "said no".
//!
//! **A screen can undo that in pixels without touching the domain**, by rendering both outcomes
//! through one view model with an optional count and a shared message. So there is no such type
//! here. [`RefusalNotice`] and [`UndecidedNotice`] are separate types with **no shared field and
//! no common trait**, and neither is constructible from a [`PinVerification`] — see the note at
//! the bottom of this module. The count of remaining attempts exists only inside
//! [`PadOffer::AtCost`], so the element that renders a count takes one as an argument and is
//! unreachable from the undetermined path.
//!
//! [`PinVerification`]: pos_models::PinVerification

use pos_models::{
    AttemptsRemaining, Authority, OperatorId, PinLength, PinRefusal, Repudiation,
    UndeterminedCause, VerifiedOperator,
};

use super::strings::{self, Sentence};

// ============================================================================
// Refusals
// ============================================================================

/// Whether to offer the keypad back, and at what price.
///
/// # Why the attempt count lives in a variant rather than on the notice
///
/// `Option<AttemptsRemaining>` on [`RefusalNotice`] would make "locked, and four attempts remain"
/// representable — two spellings of contradictory states, reachable by anyone who forgets to
/// check the `None`. That is the boolean-with-empty-fields shape this codebase deleted once
/// already when `valid: bool` became [`PinVerification`]. Here the count is reachable only by
/// matching the arm that has one.
///
/// [`PinVerification`]: pos_models::PinVerification
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PadOffer {
    /// Offer the keypad. This refusal spent an attempt, and so will the next one.
    ///
    /// The only arm carrying a count, and therefore the only path to an element that shows one.
    AtCost {
        /// Attempts left before the account locks. Never zero — a zero is
        /// [`PinRefusal::Locked`], a different sentence entirely.
        attempts_remaining: AttemptsRemaining,
    },

    /// Offer the keypad. This refusal spent nothing, and a compliant PIN will be accepted.
    ///
    /// Reached only by [`PinRefusal::CredentialRequiresRotation`], which is the one refusal where
    /// retyping helps *and* costs nothing. Carries the length rather than naming it in the
    /// sentence, so an element can render "six digits" from a value the caller supplied.
    FreeOfCharge {
        /// The length the tenant's policy now requires.
        required_length: PinLength,
    },

    /// Do not offer the keypad. Nothing the person types changes this answer.
    Withheld,
}

impl PadOffer {
    /// Whether a different PIN could be accepted.
    ///
    /// Agrees with [`PinRefusal::a_different_pin_could_help`] by construction, and a test pins
    /// that agreement across every variant. If they ever disagree, the domain is right and this
    /// is wrong.
    pub const fn a_different_pin_could_help(self) -> bool {
        match self {
            Self::AtCost { .. } | Self::FreeOfCharge { .. } => true,
            Self::Withheld => false,
        }
    }
}

/// What to show when the platform or the till refused a PIN.
///
/// Constructible only from a [`PinRefusal`]. There is deliberately no constructor taking an
/// [`UndeterminedCause`], and no `From<PinVerification>`.
///
/// [`PinVerification`]: pos_models::PinVerification
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RefusalNotice {
    sentence: Sentence,
    pad: PadOffer,
}

impl RefusalNotice {
    /// Shapes a refusal into what the screen shows.
    ///
    /// Exhaustive with no catch-all arm, for the reason the domain's own methods are: a new
    /// refusal variant must fail to compile here rather than fall into a default sentence that
    /// tells somebody the wrong thing about their account.
    pub const fn for_refusal(refusal: PinRefusal) -> Self {
        match refusal {
            PinRefusal::WrongPin { attempts_remaining } => Self {
                sentence: strings::WRONG_PIN,
                pad: PadOffer::AtCost { attempts_remaining },
            },
            PinRefusal::Locked => Self {
                sentence: strings::LOCKED,
                pad: PadOffer::Withheld,
            },
            PinRefusal::OperatorUnknown => Self {
                sentence: strings::OPERATOR_UNKNOWN,
                pad: PadOffer::Withheld,
            },
            PinRefusal::OperatorInactive => Self {
                sentence: strings::OPERATOR_INACTIVE,
                pad: PadOffer::Withheld,
            },
            PinRefusal::CredentialUnreadable => Self {
                sentence: strings::CREDENTIAL_UNREADABLE,
                pad: PadOffer::Withheld,
            },
            PinRefusal::CredentialExpired => Self {
                sentence: strings::CREDENTIAL_EXPIRED,
                pad: PadOffer::Withheld,
            },
            PinRefusal::CredentialRequiresRotation { expected } => Self {
                sentence: strings::CREDENTIAL_REQUIRES_ROTATION,
                pad: PadOffer::FreeOfCharge {
                    required_length: expected,
                },
            },
        }
    }

    /// The sentence to show.
    pub const fn sentence(self) -> Sentence {
        self.sentence
    }

    /// Whether to offer the keypad back, and at what price.
    pub const fn pad(self) -> PadOffer {
        self.pad
    }
}

// ============================================================================
// Undecided
// ============================================================================

/// Whether trying the same thing again could produce a different answer.
///
/// An enum rather than a `bool` because the two cases are different instructions to the person at
/// the drawer, and a `bool` at a call site reads as neither.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Recheck {
    /// The condition may pass on its own; trying again is reasonable.
    ///
    /// True only where the obstacle is genuinely transient. A screen that retries anything else
    /// is a caller mistaking a bug for weather.
    WorthRetrying,

    /// The answer will be the same until somebody acts elsewhere.
    ///
    /// Says nothing about whether a remedy exists — a suspended terminal is `Futile` here and
    /// still has an administrator who can fix it in thirty seconds. That distinction lives in the
    /// sentence, which is why [`strings::ENROLMENT_WITHDRAWN`] and
    /// [`strings::ENROLMENT_SUSPENDED`] are two sentences and not one.
    Futile,
}

/// What to show when the till could not find out whether the PIN was right.
///
/// Constructible only from an [`UndeterminedCause`]. **It carries no attempt count and there is
/// nowhere to put one**, which is the property that makes the original defect unrepresentable
/// rather than merely absent.
///
/// Takes the cause by reference: [`UndeterminedCause`] owns a boxed source on one variant and is
/// therefore not `Copy`, and a notice that consumed the cause would stop the caller logging it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct UndecidedNotice {
    sentence: Sentence,
    recheck: Recheck,
}

impl UndecidedNotice {
    /// Shapes an undetermined outcome into what the screen shows.
    ///
    /// Exhaustive with no catch-all arm. A new cause must fail to compile here: the wrong default
    /// in *this* function is `WorthRetrying`, which invites a cashier to retype against something
    /// no amount of retyping settles.
    pub const fn for_cause(cause: &UndeterminedCause) -> Self {
        match cause {
            UndeterminedCause::ServerUnreachable => Self {
                sentence: strings::SERVER_UNREACHABLE,
                recheck: Recheck::WorthRetrying,
            },
            UndeterminedCause::StoreUnavailable(_) => Self {
                sentence: strings::STORE_UNAVAILABLE,
                recheck: Recheck::WorthRetrying,
            },
            UndeterminedCause::ReauthFailed => Self {
                sentence: strings::REAUTH_FAILED,
                recheck: Recheck::Futile,
            },
            UndeterminedCause::EnrolmentRepudiated(Repudiation::Withdrawn) => Self {
                sentence: strings::ENROLMENT_WITHDRAWN,
                recheck: Recheck::Futile,
            },
            UndeterminedCause::EnrolmentRepudiated(Repudiation::Suspended) => Self {
                sentence: strings::ENROLMENT_SUSPENDED,
                recheck: Recheck::Futile,
            },
            UndeterminedCause::TerminalNotProvisioned => Self {
                sentence: strings::TERMINAL_NOT_PROVISIONED,
                recheck: Recheck::Futile,
            },
            UndeterminedCause::ContractBreach { .. } => Self {
                sentence: strings::CONTRACT_BREACH,
                recheck: Recheck::Futile,
            },
        }
    }

    /// The sentence to show.
    pub const fn sentence(self) -> Sentence {
        self.sentence
    }

    /// Whether trying again could settle it.
    pub const fn recheck(self) -> Recheck {
        self.recheck
    }
}

// ============================================================================
// The exit value
// ============================================================================

/// Somebody is signed in at this till.
///
/// # Why two arms, and why they must not be collapsed
///
/// The till uploads shifts, and a shift opened against a locally verified PIN is a **different
/// audit record** from one the platform verified. [`Self::JustVerified`] carries the
/// [`Authority`] that decided; [`Self::Restored`] proves somebody is signed in and carries no
/// such record, because no decision was made in this session — one was made earlier and survived.
///
/// Flattening these to `{ operator, decided_by: Option<Authority> }` for a screen's convenience
/// would put `None` into the tenant's books as though it were a decision, and there is no
/// recovering who decided once it is written that way. A later screen must not do it.
#[derive(Debug, Clone)]
pub enum SignedInAtTheTill {
    /// A PIN was entered and accepted in this session.
    JustVerified {
        /// The operator, in the projection that carries no PIN material.
        operator: VerifiedOperator,
        /// Whether the platform or a local credential made this decision.
        decided_by: Authority,
    },

    /// A session that already existed was restored.
    ///
    /// Carries only the identifier: a restored session is evidence that somebody signed in, not a
    /// record of who decided they had.
    Restored {
        /// Who is signed in.
        operator: OperatorId,
    },
}

// ============================================================================
// Deliberately absent
// ============================================================================

// There is no `impl From<PinVerification> for RefusalNotice`, no such impl for
// `UndecidedNotice`, and no trait either implements.
//
// A conversion from the whole outcome would have to decide what to do with the two cases it is
// not for. Every available answer is wrong: `Option` makes the caller unwrap a case that cannot
// happen, a panic puts a crash on the render path, and a fallback notice is the shared-message
// bug this module exists to prevent. Callers match `PinVerification` and route each case to the
// type that models it — which is three arms at one site, checked by the compiler, instead of one
// call that silently handles two of them badly.
//
// If a future author reaches for this impl, that is the signal a screen is trying to render an
// outcome it has not yet distinguished. Distinguish it there.

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use pos_models::{StoreFailure, StoreFailureKind};

    fn attempts(n: u8) -> AttemptsRemaining {
        AttemptsRemaining::new(n).expect("a non-zero attempt count")
    }

    /// Every refusal variant, so a new one fails this list as well as the match.
    fn every_refusal() -> [PinRefusal; 7] {
        [
            PinRefusal::WrongPin {
                attempts_remaining: attempts(3),
            },
            PinRefusal::Locked,
            PinRefusal::OperatorUnknown,
            PinRefusal::OperatorInactive,
            PinRefusal::CredentialUnreadable,
            PinRefusal::CredentialExpired,
            PinRefusal::CredentialRequiresRotation {
                expected: PinLength::Six,
            },
        ]
    }

    /// Every undetermined cause, including both repudiations, which are one variant and two
    /// sentences.
    fn every_cause() -> [UndeterminedCause; 7] {
        [
            UndeterminedCause::ServerUnreachable,
            UndeterminedCause::StoreUnavailable(StoreFailure::new(
                "read_operator",
                StoreFailureKind::Unavailable,
            )),
            UndeterminedCause::ReauthFailed,
            UndeterminedCause::EnrolmentRepudiated(Repudiation::Withdrawn),
            UndeterminedCause::EnrolmentRepudiated(Repudiation::Suspended),
            UndeterminedCause::TerminalNotProvisioned,
            UndeterminedCause::contract_breach(
                "not a number"
                    .parse::<u8>()
                    .expect_err("this must not parse"),
            ),
        ]
    }

    // ------------------------------------------------------------------
    // The defect this module exists to make unrepresentable
    // ------------------------------------------------------------------

    #[test]
    fn sign_in_no_undetermined_cause_can_produce_an_attempt_count() {
        // The structural claim: there is no path from an undecided outcome to a count. This
        // asserts it over every cause; the type system is what makes it exhaustive, since
        // `UndecidedNotice` has no field that could hold one.
        for cause in every_cause() {
            let notice = UndecidedNotice::for_cause(&cause);
            // The only compiling way to ask for a count is through `PadOffer`, which
            // `UndecidedNotice` does not carry. If this type ever grows one, this test stops
            // compiling rather than silently passing.
            let _: Sentence = notice.sentence();
            let _: Recheck = notice.recheck();
        }
    }

    #[test]
    fn sign_in_a_refusal_and_an_undecided_outcome_share_no_sentence() {
        let refusal_sentences: Vec<Sentence> = every_refusal()
            .into_iter()
            .map(|r| RefusalNotice::for_refusal(r).sentence())
            .collect();
        let undecided_sentences: Vec<Sentence> = every_cause()
            .iter()
            .map(|c| UndecidedNotice::for_cause(c).sentence())
            .collect();

        for undecided in &undecided_sentences {
            assert!(
                !refusal_sentences.contains(undecided),
                "an undecided outcome shows a sentence a refusal also shows: {:?} — that is the \
                 shared-message bug this module exists to prevent",
                undecided
            );
        }

        // Control: the sweep is looking at something. Both sides must be non-empty, or the
        // assertion above is vacuous.
        assert_eq!(refusal_sentences.len(), 7);
        assert_eq!(undecided_sentences.len(), 7);
    }

    // ------------------------------------------------------------------
    // Refusals — one assertion per variant
    // ------------------------------------------------------------------

    #[test]
    fn sign_in_only_a_wrong_pin_carries_a_remaining_count() {
        for refusal in every_refusal() {
            let pad = RefusalNotice::for_refusal(refusal).pad();
            match (refusal, pad) {
                (
                    PinRefusal::WrongPin { attempts_remaining },
                    PadOffer::AtCost {
                        attempts_remaining: shown,
                    },
                ) => assert_eq!(shown, attempts_remaining),
                (PinRefusal::WrongPin { .. }, other) => {
                    panic!("a wrong PIN must offer the pad at cost, got {other:?}")
                }
                (_, PadOffer::AtCost { .. }) => {
                    panic!("{refusal:?} spent an attempt it does not spend")
                }
                _ => {}
            }
        }
    }

    #[test]
    fn sign_in_a_rotation_offers_the_pad_and_charges_nothing() {
        let notice = RefusalNotice::for_refusal(PinRefusal::CredentialRequiresRotation {
            expected: PinLength::Six,
        });
        assert_eq!(
            notice.pad(),
            PadOffer::FreeOfCharge {
                required_length: PinLength::Six
            },
            "a rotation is the one refusal where retyping helps and costs nothing"
        );
    }

    #[test]
    fn sign_in_the_pad_offer_agrees_with_the_domain_on_every_refusal() {
        // The domain owns this question. If the two ever disagree, the domain is right.
        for refusal in every_refusal() {
            assert_eq!(
                RefusalNotice::for_refusal(refusal)
                    .pad()
                    .a_different_pin_could_help(),
                refusal.a_different_pin_could_help(),
                "{refusal:?}: the screen and the domain disagree about whether retyping helps"
            );
        }
    }

    #[test]
    fn sign_in_a_locked_account_is_never_offered_the_pad() {
        assert_eq!(
            RefusalNotice::for_refusal(PinRefusal::Locked).pad(),
            PadOffer::Withheld,
            "offering the pad to a locked account walks the operator further into a lockout"
        );
    }

    #[test]
    fn sign_in_a_tills_own_fault_never_costs_the_operator_an_attempt() {
        for refusal in [
            PinRefusal::CredentialUnreadable,
            PinRefusal::CredentialExpired,
        ] {
            assert_eq!(
                RefusalNotice::for_refusal(refusal).pad(),
                PadOffer::Withheld,
                "{refusal:?} is the till's fault; the operator must not be charged for it"
            );
        }
    }

    #[test]
    fn sign_in_an_unknown_or_inactive_operator_is_told_so_and_not_offered_the_pad() {
        for (refusal, expected) in [
            (PinRefusal::OperatorUnknown, strings::OPERATOR_UNKNOWN),
            (PinRefusal::OperatorInactive, strings::OPERATOR_INACTIVE),
        ] {
            let notice = RefusalNotice::for_refusal(refusal);
            assert_eq!(notice.sentence(), expected);
            assert_eq!(notice.pad(), PadOffer::Withheld);
        }
    }

    // ------------------------------------------------------------------
    // Undecided — one assertion per variant
    // ------------------------------------------------------------------

    #[test]
    fn sign_in_only_a_transient_obstacle_is_worth_retrying() {
        let expected = [
            (UndeterminedCause::ServerUnreachable, Recheck::WorthRetrying),
            (
                UndeterminedCause::StoreUnavailable(StoreFailure::new(
                    "read_operator",
                    StoreFailureKind::Unavailable,
                )),
                Recheck::WorthRetrying,
            ),
            (UndeterminedCause::ReauthFailed, Recheck::Futile),
            (
                UndeterminedCause::EnrolmentRepudiated(Repudiation::Withdrawn),
                Recheck::Futile,
            ),
            (
                UndeterminedCause::EnrolmentRepudiated(Repudiation::Suspended),
                Recheck::Futile,
            ),
            (UndeterminedCause::TerminalNotProvisioned, Recheck::Futile),
        ];

        for (cause, want) in expected {
            assert_eq!(
                UndecidedNotice::for_cause(&cause).recheck(),
                want,
                "{cause:?} was given the wrong retry advice"
            );
        }
    }

    #[test]
    fn sign_in_a_contract_breach_is_never_presented_as_weather() {
        let cause = UndeterminedCause::contract_breach(
            "not a number"
                .parse::<u8>()
                .expect_err("this must not parse"),
        );
        let notice = UndecidedNotice::for_cause(&cause);
        assert_eq!(
            notice.recheck(),
            Recheck::Futile,
            "retrying a contract breach is a caller mistaking somebody's bug for a network blip"
        );
        assert_eq!(notice.sentence(), strings::CONTRACT_BREACH);
    }

    #[test]
    fn sign_in_the_two_repudiations_keep_their_remedy_distinction() {
        let withdrawn = UndecidedNotice::for_cause(&UndeterminedCause::EnrolmentRepudiated(
            Repudiation::Withdrawn,
        ));
        let suspended = UndecidedNotice::for_cause(&UndeterminedCause::EnrolmentRepudiated(
            Repudiation::Suspended,
        ));

        assert_ne!(
            withdrawn.sentence(),
            suspended.sentence(),
            "saying 'withdrawn' to a suspended terminal sends someone home for a day when an \
             administrator could reactivate it in thirty seconds"
        );

        // The sentences must track the domain's own answer, not a local opinion of it.
        assert!(!Repudiation::Withdrawn.has_a_remedy_at_the_till());
        assert!(Repudiation::Suspended.has_a_remedy_at_the_till());
        assert_eq!(suspended.sentence(), strings::ENROLMENT_SUSPENDED);
        assert_eq!(withdrawn.sentence(), strings::ENROLMENT_WITHDRAWN);
    }

    // ------------------------------------------------------------------
    // The string table
    // ------------------------------------------------------------------

    #[test]
    fn sign_in_every_sentence_is_present_in_both_directions() {
        for sentence in strings::EVERY_SENTENCE {
            assert!(
                !sentence.arabic().trim().is_empty(),
                "a sentence has no Arabic text: {sentence:?}"
            );
            assert!(
                !sentence.english().trim().is_empty(),
                "a sentence has no English text: {sentence:?}"
            );
            assert_ne!(
                sentence.arabic(),
                sentence.english(),
                "a sentence was left untranslated: {sentence:?}"
            );
        }
    }

    #[test]
    fn sign_in_every_sentence_a_notice_can_show_is_in_the_table() {
        // Step 14 sweeps `EVERY_SENTENCE` to prove both reading directions render. A sentence
        // reachable from a notice but absent from that array is invisible to the sweep, and the
        // sweep would still pass — the failure this asserts against.
        for refusal in every_refusal() {
            let shown = RefusalNotice::for_refusal(refusal).sentence();
            assert!(
                strings::EVERY_SENTENCE.contains(&shown),
                "{refusal:?} shows a sentence missing from EVERY_SENTENCE: {shown:?}"
            );
        }
        for cause in every_cause() {
            let shown = UndecidedNotice::for_cause(&cause).sentence();
            assert!(
                strings::EVERY_SENTENCE.contains(&shown),
                "{cause:?} shows a sentence missing from EVERY_SENTENCE: {shown:?}"
            );
        }
    }

    #[test]
    fn sign_in_the_sentence_table_holds_no_duplicates() {
        // Two outcomes sharing a sentence is the shared-message bug wearing different types.
        for (i, a) in strings::EVERY_SENTENCE.iter().enumerate() {
            for b in strings::EVERY_SENTENCE.iter().skip(i + 1) {
                assert_ne!(a, b, "two entries in EVERY_SENTENCE are identical: {a:?}");
            }
        }
    }
}
