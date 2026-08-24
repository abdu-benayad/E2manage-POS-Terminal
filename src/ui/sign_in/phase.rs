//! The sign-in screen is one state machine, not four screens.
//!
//! [`AuthPhase`] arms carry exactly what the phase needs, so the states worth ruling out cannot be
//! written down: there is no `PinEntry` without an operator, no `OperatorSelect` without a
//! terminal session, and no phase carrying a `String` that says which screen we are on.
//!
//! # The invariant this module exists to establish
//!
//! `AuthService::verify_pin` reaches `accepted_by_platform`, which calls
//! `OperatorSignIn::record_and_present` — writing the session row and setting the operator token
//! on the API client — **before it returns**. The effect precedes the answer.
//!
//! So discarding a "stale" verification answer discards the *answer* and keeps the *effect*: the
//! screen shows nobody signed in, the till is signed in, every subsequent write is attributed to
//! that operator, and the next launch's restore probe signs them in with no PIN ever completed.
//!
//! Two consequences, both structural rather than remembered:
//!
//! 1. [`PinEntryStanding::Verifying`] offers no way out. No cancel enquiry exists, and no answer
//!    other than that verification's own moves out of it.
//! 2. [`advance`] folds an `Accepted` verification **in every phase**, not only in `PinEntry`. If
//!    the answer reaches a screen that has moved on, the till is still signed in, and the only
//!    safe thing a screen can do is agree.

use chrono::{DateTime, Utc};
use pos_api::PairingStatus;
use pos_db::OperatorRow;
use pos_models::{
    EnteredDigits, HardwareEnrolment, OperatorId, OperatorName, OperatorRole, PinPolicy,
    PinVerification,
};
use pos_services::TerminalSession;
use std::time::Duration;

use super::enquiry::{AuthAnswer, AuthEnquiry, EnquiryId, PairingCode, PendingEnquiry};
use super::notice::{RefusalNotice, SignedInAtTheTill, UndecidedNotice};

/// How long to wait before polling the pairing status again.
///
/// Non-zero by construction. A pending poll that folds to an immediate re-poll issues one request
/// per frame — thousands a minute against the platform, for a code a human is reading off this
/// screen and typing somewhere else.
pub const PAIRING_POLL_INTERVAL: Duration = Duration::from_secs(3);

// ============================================================================
// Operator card
// ============================================================================

/// One operator, as the selection list shows them.
///
/// Carries [`OperatorName`] rather than a rendered string because that type holds both scripts,
/// and an Arabic-locale list needs the Arabic one. Flattening to a `String` here would decide the
/// locale at the wrong layer — in a bridge that must not know which direction the screen reads.
#[derive(Debug, Clone)]
pub struct OperatorCard {
    id: OperatorId,
    name: OperatorName,
    role: OperatorRole,
}

impl OperatorCard {
    /// The operators worth offering, out of everything the till has synced.
    ///
    /// **Inactive operators are filtered out here rather than refused later.** An inactive
    /// operator's PIN can never be accepted — `PinRefusal::OperatorInactive` consumes no attempt
    /// precisely because nothing they type could change the answer — so listing them offers a
    /// door that is locked, and the person at the till learns that only after typing a PIN.
    pub fn roster(rows: &[OperatorRow]) -> Vec<Self> {
        rows.iter()
            .filter(|row| row.is_active)
            .map(|row| Self {
                id: row.id.clone(),
                name: row.name.clone(),
                role: row.role,
            })
            .collect()
    }

    /// Who this is.
    pub const fn id(&self) -> &OperatorId {
        &self.id
    }

    /// Their name, in both scripts the store keeps.
    pub const fn name(&self) -> &OperatorName {
        &self.name
    }

    /// Their POS role.
    pub const fn role(&self) -> OperatorRole {
        self.role
    }
}

// ============================================================================
// PIN entry
// ============================================================================

/// Where PIN entry stands.
#[derive(Debug)]
pub enum PinEntryStanding {
    /// Digits are being typed. The buffer zeroizes when it drops.
    Entering(EnteredDigits),

    /// A verification is in flight.
    ///
    /// # There is deliberately no way out of this state, and a cancel must not be added
    ///
    /// The UI reads better with a cancel button, and somebody will want one. It cannot be built
    /// safely at this layer, and the reason is an ordering rather than a preference.
    ///
    /// `AuthService::verify_pin` → `accepted_by_platform` → `OperatorSignIn::record_and_present`
    /// writes the operator session row and sets `X-Operator-Token` on the shared API client
    /// **before** `verify_pin` returns. A cancel can therefore only cancel the screen's interest
    /// in the answer, never the effect. The result is a till that is signed in while showing a
    /// sign-in screen: subsequent writes carry that operator's token, and the next launch's
    /// restore probe finds a valid session and signs them in without a PIN ever having completed.
    ///
    /// Carries no digits, so there is no buffer to hand back to an entering state, and no
    /// affordance for one to be wired to.
    ///
    /// **The wait is already bounded** — the API client's own 30-second request timeout produces
    /// `UndeterminedCause::ServerUnreachable`, which folds to [`Self::Undecided`] and returns the
    /// cashier to entry. An unbounded wait would be a different argument; this one is not.
    ///
    /// A cancel becomes safe only when the platform's decision and the till's recording of it are
    /// separable — i.e. when `verify_pin` returns a decision the caller then commits. Until then
    /// `sign_in_a_verification_in_flight_offers_no_way_out` fails if this changes.
    Verifying {
        /// The enquiry whose answer is awaited.
        awaiting: EnquiryId,
    },

    /// The PIN was refused, with what to show and whether to offer the pad back.
    Refused(RefusalNotice),

    /// The till could not find out.
    ///
    /// Never [`Self::Refused`] for an undecided outcome — showing a wrong-PIN message for an
    /// unreachable server is the defect `auth-outcome-and-offline-lockout` removed, and the two
    /// notices share no type so no render path can confuse them.
    Undecided(UndecidedNotice),
}

// ============================================================================
// The phase
// ============================================================================

/// Which part of signing in the till is on.
///
/// One enum, never a string, and never four independent screens with a flag between them.
#[derive(Debug)]
pub enum AuthPhase {
    /// Awaiting the start-up probe: is there a terminal session, and is anybody already signed in?
    Splash,

    /// The start-up probe could not answer.
    ///
    /// **Not in the task's original four**, and added because the alternative was worse: a failed
    /// probe with nowhere to go produces a silent retry loop behind an unchanging splash, which is
    /// the "retried forever by a caller that thinks it is weather" failure that
    /// `UndeterminedCause::ContractBreach` exists to name. Reuses [`UndecidedNotice`], which
    /// already carries both the sentence and whether retrying is worth it.
    Stalled(UndecidedNotice),

    /// This till is not paired. A code is on screen, waiting for somebody to approve it.
    Pairing {
        /// The code the platform issued.
        code: PairingCode,
        /// When it stops being valid.
        expires_at: DateTime<Utc>,
        /// Whether approving this code replaces a working terminal.
        ///
        /// Three states, never a boolean. `Undetermined` until the first status poll answers,
        /// because the pairing-*request* response is byte-identical for a first enrolment and a
        /// re-pair and carries no signal to read.
        enrolment: HardwareEnrolment,
        /// Whether a status poll is already outstanding.
        ///
        /// The fold emits no new poll while this is true, which is how a single-flight poll is
        /// enforced without a stamp: the pairing poll is effectful but repeatable, so the cost of
        /// a duplicate is load rather than a lost effect.
        poll_in_flight: bool,
    },

    /// The till is enrolled. Somebody must say who they are.
    ///
    /// Carrying the [`TerminalSession`] is what makes "choosing an operator on an unenrolled till"
    /// unrepresentable.
    OperatorSelect {
        /// This till's session with the platform.
        session: TerminalSession,
        /// Who can sign in here.
        operators: Vec<OperatorCard>,
    },

    /// An operator has been chosen and is entering their PIN.
    ///
    /// Carrying the session, the operator and the policy together is what makes "PIN entry with
    /// nobody selected" and "PIN entry before its rules were known" unrepresentable.
    PinEntry {
        /// This till's session with the platform.
        session: TerminalSession,
        /// Whose PIN is being entered.
        operator: OperatorCard,
        /// The tenant's rules, carried since login rather than looked up.
        policy: PinPolicy,
        /// Where entry stands.
        standing: PinEntryStanding,
    },

    /// Somebody is signed in. The screen is done.
    ///
    /// **Not in the task's original four**, and required: the fold has to be able to express the
    /// exit, and `SignedInAtTheTill` is the value task 06 built for it.
    SignedIn(SignedInAtTheTill),
}

// ============================================================================
// The fold
// ============================================================================

/// Advances the screen by one answer.
///
/// Total: every phase/answer pair has an arm, and there is no panic path. Returns the enquiries to
/// dispatch — unstamped, because ids are minted at dispatch by the driver, which is the only thing
/// that knows an enquiry actually left.
///
/// # An accepted verification is folded in every phase
///
/// The first arm is not defensive programming. `record_and_present` has already run by the time
/// this answer exists, so a screen that has moved on is a screen that is *wrong*: the till holds
/// the session either way. Agreeing is the only outcome that does not leave the UI and the API
/// client disagreeing about who is signed in.
pub fn advance(phase: AuthPhase, answer: AuthAnswer) -> (AuthPhase, Vec<PendingEnquiry>) {
    // An accepted PIN binds regardless of where the screen thinks it is. See the module note.
    if let AuthAnswer::PinVerified {
        outcome: PinVerification::Accepted {
            operator,
            decided_by,
        },
        ..
    } = answer
    {
        return (
            AuthPhase::SignedIn(SignedInAtTheTill::JustVerified {
                operator,
                decided_by,
            }),
            Vec::new(),
        );
    }

    match (phase, answer) {
        // ---- Splash -------------------------------------------------------
        (AuthPhase::Splash, AuthAnswer::SessionRestored { outcome, .. }) => match outcome {
            Ok(Some(operator)) => (
                AuthPhase::SignedIn(SignedInAtTheTill::Restored { operator }),
                Vec::new(),
            ),
            // Nobody signed in is not a failure; the terminal-session half of the probe decides
            // what happens next.
            Ok(None) => (AuthPhase::Splash, Vec::new()),
            Err(_) => (
                AuthPhase::Stalled(UndecidedNotice::for_startup_probe_failure()),
                Vec::new(),
            ),
        },

        (AuthPhase::Splash, AuthAnswer::TerminalSessionOpened { outcome, .. }) => match outcome {
            Ok(Some(session)) => (
                AuthPhase::OperatorSelect {
                    session,
                    operators: Vec::new(),
                },
                vec![PendingEnquiry::now(AuthEnquiry::LoadOperators)],
            ),
            Ok(None) => (
                AuthPhase::Splash,
                vec![PendingEnquiry::now(AuthEnquiry::RequestPairingCode)],
            ),
            Err(_) => (
                AuthPhase::Stalled(UndecidedNotice::for_startup_probe_failure()),
                Vec::new(),
            ),
        },

        (AuthPhase::Splash, AuthAnswer::PairingCodeRequested { outcome, .. }) => match outcome {
            Ok(state) => {
                let code = PairingCode::new(state.pairing_code);
                (
                    AuthPhase::Pairing {
                        code: code.clone(),
                        expires_at: state.expires_at,
                        enrolment: state.enrolment,
                        poll_in_flight: true,
                    },
                    vec![PendingEnquiry::after(
                        PAIRING_POLL_INTERVAL,
                        AuthEnquiry::PairingStatus { code },
                    )],
                )
            }
            Err(_) => (
                AuthPhase::Stalled(UndecidedNotice::for_startup_probe_failure()),
                Vec::new(),
            ),
        },

        // ---- Pairing ------------------------------------------------------
        (
            AuthPhase::Pairing {
                code, expires_at, ..
            },
            AuthAnswer::PairingStatusRead { outcome, .. },
        ) => match outcome {
            Ok(state) => match state.status {
                // The poll that completed the pairing has already registered this terminal and
                // logged it in. Restoring is how the screen catches up with what happened.
                PairingStatus::Completed => (
                    AuthPhase::Pairing {
                        code,
                        expires_at: state.expires_at,
                        enrolment: state.enrolment,
                        poll_in_flight: false,
                    },
                    vec![PendingEnquiry::now(AuthEnquiry::RestoreSession)],
                ),
                PairingStatus::Pending => (
                    AuthPhase::Pairing {
                        code: code.clone(),
                        expires_at: state.expires_at,
                        enrolment: state.enrolment,
                        poll_in_flight: true,
                    },
                    vec![PendingEnquiry::after(
                        PAIRING_POLL_INTERVAL,
                        AuthEnquiry::PairingStatus { code },
                    )],
                ),
                // A dead code is replaced, not polled. The enrolment answer the platform gave is
                // carried across: it is a fact about this hardware, not about this code.
                PairingStatus::Expired | PairingStatus::Cancelled => (
                    AuthPhase::Pairing {
                        code,
                        expires_at: state.expires_at,
                        enrolment: state.enrolment,
                        poll_in_flight: false,
                    },
                    vec![PendingEnquiry::now(AuthEnquiry::RequestPairingCode)],
                ),
            },
            // A failed poll says nothing about the enrolment, so the carried value is left alone
            // rather than reset to `Undetermined` — which would spell "the poll failed" as
            // "nobody has said", losing an answer the platform already gave.
            Err(_) => (
                AuthPhase::Pairing {
                    code: code.clone(),
                    expires_at,
                    enrolment: HardwareEnrolment::Undetermined,
                    poll_in_flight: true,
                },
                vec![PendingEnquiry::after(
                    PAIRING_POLL_INTERVAL,
                    AuthEnquiry::PairingStatus { code },
                )],
            ),
        },

        (AuthPhase::Pairing { .. }, AuthAnswer::PairingCodeRequested { outcome, .. }) => {
            match outcome {
                Ok(state) => {
                    let code = PairingCode::new(state.pairing_code);
                    (
                        AuthPhase::Pairing {
                            code: code.clone(),
                            expires_at: state.expires_at,
                            enrolment: state.enrolment,
                            poll_in_flight: true,
                        },
                        vec![PendingEnquiry::after(
                            PAIRING_POLL_INTERVAL,
                            AuthEnquiry::PairingStatus { code },
                        )],
                    )
                }
                Err(_) => (
                    AuthPhase::Stalled(UndecidedNotice::for_startup_probe_failure()),
                    Vec::new(),
                ),
            }
        }

        (AuthPhase::Pairing { .. }, AuthAnswer::TerminalSessionOpened { outcome, .. }) => {
            match outcome {
                Ok(Some(session)) => (
                    AuthPhase::OperatorSelect {
                        session,
                        operators: Vec::new(),
                    },
                    vec![PendingEnquiry::now(AuthEnquiry::LoadOperators)],
                ),
                // Pairing said completed and no session exists: the two disagree, and inventing a
                // session here is the one thing that must not happen.
                Ok(None) | Err(_) => (
                    AuthPhase::Stalled(UndecidedNotice::for_startup_probe_failure()),
                    Vec::new(),
                ),
            }
        }

        (
            AuthPhase::Pairing {
                code,
                expires_at,
                enrolment,
                poll_in_flight,
            },
            AuthAnswer::SessionRestored { outcome, .. },
        ) => match outcome {
            Ok(Some(operator)) => (
                AuthPhase::SignedIn(SignedInAtTheTill::Restored { operator }),
                Vec::new(),
            ),
            // Paired but nobody signed in: pick up the terminal session the completed poll wrote.
            Ok(None) => (
                AuthPhase::Pairing {
                    code,
                    expires_at,
                    enrolment,
                    poll_in_flight,
                },
                Vec::new(),
            ),
            Err(_) => (
                AuthPhase::Stalled(UndecidedNotice::for_startup_probe_failure()),
                Vec::new(),
            ),
        },

        // ---- Operator selection -------------------------------------------
        (
            AuthPhase::OperatorSelect { session, operators },
            AuthAnswer::OperatorsLoaded { outcome, .. },
        ) => match outcome {
            Ok(rows) => (
                AuthPhase::OperatorSelect {
                    session,
                    operators: OperatorCard::roster(&rows),
                },
                Vec::new(),
            ),
            // A failed local read leaves the list as it was rather than emptying it: an empty
            // roster and a roster that could not be read are different things to show.
            Err(_) => (AuthPhase::OperatorSelect { session, operators }, Vec::new()),
        },

        // ---- PIN entry -----------------------------------------------------
        (
            AuthPhase::PinEntry {
                session,
                operator,
                policy,
                ..
            },
            AuthAnswer::PinVerified { outcome, .. },
        ) => {
            // `Accepted` was handled above, in every phase. What remains is a refusal or an
            // undecided outcome, and the two land in standings that share no type.
            let standing = match outcome {
                PinVerification::Refused(refusal) => {
                    PinEntryStanding::Refused(RefusalNotice::for_refusal(refusal))
                }
                PinVerification::Undetermined(cause) => {
                    PinEntryStanding::Undecided(UndecidedNotice::for_cause(&cause))
                }
                PinVerification::Accepted { .. } => unreachable!(
                    "an accepted verification is folded before this match; see `advance`"
                ),
            };
            (
                AuthPhase::PinEntry {
                    session,
                    operator,
                    policy,
                    standing,
                },
                Vec::new(),
            )
        }

        // ---- Everything else ------------------------------------------------
        // An answer that does not belong to the current phase changes nothing. Written as one arm
        // rather than omitted, so the function is total without a panic path, and enumerated above
        // it rather than as a catch-all over answers, so a new *phase* still has to be considered.
        (phase, _) => (phase, Vec::new()),
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use pos_api::SessionToken;
    use pos_models::{
        AttemptsRemaining, Authority, LockoutPeriod, MaxAttempts, NameScript, OfflineWindow,
        PinLength, PinRefusal, RequiredPinLength, SessionLifetime, StoreFailure, StoreFailureKind,
        UndeterminedCause, VerifiedOperator,
    };
    use pos_services::PairingState;
    use std::collections::HashSet;

    use crate::ui::sign_in::enquiry::EnquiryIds;

    fn ids() -> EnquiryIds {
        EnquiryIds::new()
    }

    fn an_id() -> EnquiryId {
        ids().mint()
    }

    fn operator_id(raw: &str) -> OperatorId {
        OperatorId::new(raw).expect("a well-formed operator id")
    }

    fn session() -> TerminalSession {
        TerminalSession {
            terminal_id: "t-1".into(),
            terminal_code: "TILL-01".into(),
            hardware_id: "hw-1".into(),
            session_token: SessionToken::new("a-token").expect("a non-blank token"),
            company_id: "c-1".into(),
            branch_id: None,
            locale: "ar".into(),
            currency: "LYD".into(),
            tax_rate: 0.0,
            tax_inclusive: true,
            sector: "RETAIL".into(),
            features: Vec::new(),
        }
    }

    fn policy() -> PinPolicy {
        PinPolicy::new(
            RequiredPinLength::Exactly(PinLength::Four),
            MaxAttempts::new(3).expect("a non-zero budget"),
            LockoutPeriod::from_minutes(15).expect("fifteen is not negative"),
            SessionLifetime::from_hours(8).expect("eight is positive"),
            OfflineWindow::from_hours(72).expect("seventy-two is not negative"),
        )
    }

    fn card() -> OperatorCard {
        OperatorCard {
            id: operator_id("op-1"),
            name: OperatorName::new("Sara", Some("سارة")).expect("a well-formed name"),
            role: OperatorRole::Cashier,
        }
    }

    fn pairing_state(status: PairingStatus, enrolment: HardwareEnrolment) -> PairingState {
        PairingState {
            pairing_code: "ABC123".into(),
            expires_at: DateTime::parse_from_rfc3339("2026-08-24T23:00:00Z")
                .expect("a well-formed instant")
                .with_timezone(&Utc),
            status,
            hardware_id: "hw-1".into(),
            enrolment,
        }
    }

    fn pairing(enrolment: HardwareEnrolment, poll_in_flight: bool) -> AuthPhase {
        AuthPhase::Pairing {
            code: PairingCode::new("ABC123"),
            expires_at: DateTime::parse_from_rfc3339("2026-08-24T23:00:00Z")
                .expect("a well-formed instant")
                .with_timezone(&Utc),
            enrolment,
            poll_in_flight,
        }
    }

    fn verifying(awaiting: EnquiryId) -> AuthPhase {
        AuthPhase::PinEntry {
            session: session(),
            operator: card(),
            policy: policy(),
            standing: PinEntryStanding::Verifying { awaiting },
        }
    }

    fn verified_operator() -> VerifiedOperator {
        VerifiedOperator::from_verified_pin(
            operator_id("op-1"),
            OperatorName::new("Sara", Some("سارة")).expect("a well-formed name"),
            OperatorRole::Cashier,
            pos_models::OperatorPermissions::none(),
        )
    }

    /// Every answer shape, for exhaustive sweeps. Ids are deliberately foreign to any phase under
    /// test unless a test overrides them.
    fn every_answer(id: EnquiryId) -> Vec<AuthAnswer> {
        vec![
            AuthAnswer::SessionRestored {
                id,
                outcome: Ok(Some(operator_id("op-9"))),
            },
            AuthAnswer::SessionRestored {
                id,
                outcome: Ok(None),
            },
            AuthAnswer::SessionRestored {
                id,
                outcome: Err(anyhow::anyhow!("probe failed")),
            },
            AuthAnswer::PairingCodeRequested {
                id,
                outcome: Ok(pairing_state(
                    PairingStatus::Pending,
                    HardwareEnrolment::NotEnrolled,
                )),
            },
            AuthAnswer::PairingCodeRequested {
                id,
                outcome: Err(anyhow::anyhow!("no code")),
            },
            AuthAnswer::PairingStatusRead {
                id,
                outcome: Ok(pairing_state(
                    PairingStatus::Completed,
                    HardwareEnrolment::AlreadyEnrolled,
                )),
            },
            AuthAnswer::PairingStatusRead {
                id,
                outcome: Err(anyhow::anyhow!("poll failed")),
            },
            AuthAnswer::TerminalSessionOpened {
                id,
                outcome: Ok(Some(session())),
            },
            AuthAnswer::TerminalSessionOpened {
                id,
                outcome: Ok(None),
            },
            AuthAnswer::TerminalSessionOpened {
                id,
                outcome: Err(anyhow::anyhow!("no session")),
            },
            AuthAnswer::OperatorsLoaded {
                id,
                outcome: Ok(Vec::new()),
            },
            AuthAnswer::OperatorsLoaded {
                id,
                outcome: Err(anyhow::anyhow!("read failed")),
            },
            AuthAnswer::PinVerified {
                id,
                outcome: PinVerification::Refused(PinRefusal::Locked),
            },
            AuthAnswer::PinVerified {
                id,
                outcome: PinVerification::Undetermined(UndeterminedCause::ServerUnreachable),
            },
        ]
    }

    // ==================================================================
    // 1. THE CANARY
    // ==================================================================

    /// **If a cancel affordance is ever added, this test must fail.**
    ///
    /// `record_and_present` writes the operator session row and sets `X-Operator-Token` before
    /// `verify_pin` returns, so leaving `Verifying` without folding the verification's own answer
    /// produces a till that is signed in behind a sign-in screen.
    #[test]
    fn sign_in_a_verification_in_flight_offers_no_way_out() {
        let awaiting = an_id();
        let foreign = {
            let mut minter = ids();
            minter.mint();
            minter.mint()
        };

        for answer in every_answer(foreign) {
            let is_a_verification = matches!(answer, AuthAnswer::PinVerified { .. });
            let (next, enquiries) = advance(verifying(awaiting), answer);

            match next {
                AuthPhase::PinEntry {
                    standing: PinEntryStanding::Verifying { awaiting: still },
                    ..
                } => {
                    assert!(
                        !is_a_verification,
                        "a verification answer must move out of Verifying, not sit in it"
                    );
                    assert_eq!(
                        still, awaiting,
                        "the awaited id changed underneath the wait"
                    );
                }
                AuthPhase::PinEntry { .. } | AuthPhase::SignedIn(_) => assert!(
                    is_a_verification,
                    "only a verification's own answer may leave Verifying — this one was not one"
                ),
                other => panic!(
                    "a non-verification answer moved PIN entry to a different phase entirely: \
                     {other:?}"
                ),
            }

            assert!(
                enquiries.is_empty(),
                "a waiting verification must not emit enquiries; a cancel would arrive as one"
            );
        }
    }

    #[test]
    fn sign_in_a_verifying_standing_carries_no_digits_to_hand_back() {
        // Structural half of the canary: there is no buffer in `Verifying`, so no back
        // affordance can be wired to one. If a field is ever added, this stops compiling.
        match verifying(an_id()) {
            AuthPhase::PinEntry {
                standing: PinEntryStanding::Verifying { awaiting },
                ..
            } => {
                let _: EnquiryId = awaiting;
            }
            other => panic!("expected a verifying PIN entry, got {other:?}"),
        }
    }

    // ==================================================================
    // 2. The three PIN outcomes
    // ==================================================================

    #[test]
    fn sign_in_an_undecided_outcome_never_lands_in_refused() {
        // The defect `auth-outcome-and-offline-lockout` removed, asserted at the fold.
        let causes = [
            UndeterminedCause::ServerUnreachable,
            UndeterminedCause::ReauthFailed,
            UndeterminedCause::TerminalNotProvisioned,
            UndeterminedCause::StoreUnavailable(StoreFailure::new(
                "read",
                StoreFailureKind::Unavailable,
            )),
        ];

        for cause in causes {
            let (next, _) = advance(
                verifying(an_id()),
                AuthAnswer::PinVerified {
                    id: an_id(),
                    outcome: PinVerification::Undetermined(cause),
                },
            );
            match next {
                AuthPhase::PinEntry {
                    standing: PinEntryStanding::Undecided(_),
                    ..
                } => {}
                AuthPhase::PinEntry {
                    standing: PinEntryStanding::Refused(_),
                    ..
                } => panic!("an undecided outcome was rendered as a refusal"),
                other => panic!("an undecided outcome left PIN entry: {other:?}"),
            }
        }
    }

    #[test]
    fn sign_in_a_refusal_lands_in_refused_with_the_domains_own_verdict() {
        let (next, _) = advance(
            verifying(an_id()),
            AuthAnswer::PinVerified {
                id: an_id(),
                outcome: PinVerification::Refused(PinRefusal::WrongPin {
                    attempts_remaining: AttemptsRemaining::new(2).expect("non-zero"),
                }),
            },
        );
        match next {
            AuthPhase::PinEntry {
                standing: PinEntryStanding::Refused(notice),
                ..
            } => {
                assert!(notice.pad().a_different_pin_could_help());
            }
            other => panic!("a refusal left PIN entry: {other:?}"),
        }
    }

    // ==================================================================
    // 5. Acceptance
    // ==================================================================

    #[test]
    fn sign_in_an_accepted_verification_carries_the_authority_unchanged() {
        for authority in [
            Authority::Platform,
            Authority::OfflineCredential {
                not_after: pos_models::CredentialExpiry::at(
                    DateTime::parse_from_rfc3339("2026-08-25T00:00:00Z")
                        .expect("a well-formed instant")
                        .with_timezone(&Utc),
                ),
            },
        ] {
            let (next, _) = advance(
                verifying(an_id()),
                AuthAnswer::PinVerified {
                    id: an_id(),
                    outcome: PinVerification::Accepted {
                        operator: verified_operator(),
                        decided_by: authority,
                    },
                },
            );
            match next {
                AuthPhase::SignedIn(SignedInAtTheTill::JustVerified { decided_by, .. }) => {
                    assert_eq!(decided_by, authority, "the audit record was rewritten");
                }
                other => panic!("an accepted PIN did not sign anybody in: {other:?}"),
            }
        }
    }

    /// The invariant in its strongest form: an accepted answer binds wherever it lands.
    #[test]
    fn sign_in_an_accepted_verification_binds_in_every_phase() {
        // `record_and_present` already ran. A screen that has moved on is a screen that is wrong,
        // and the till holds the session either way.
        let phases = vec![
            AuthPhase::Splash,
            AuthPhase::Stalled(UndecidedNotice::for_startup_probe_failure()),
            pairing(HardwareEnrolment::Undetermined, true),
            AuthPhase::OperatorSelect {
                session: session(),
                operators: Vec::new(),
            },
            verifying(an_id()),
        ];

        for phase in phases {
            let described = format!("{phase:?}");
            let (next, enquiries) = advance(
                phase,
                AuthAnswer::PinVerified {
                    id: an_id(),
                    outcome: PinVerification::Accepted {
                        operator: verified_operator(),
                        decided_by: Authority::Platform,
                    },
                },
            );
            assert!(
                matches!(
                    next,
                    AuthPhase::SignedIn(SignedInAtTheTill::JustVerified { .. })
                ),
                "an accepted PIN was dropped in phase {described}: the till would be signed in \
                 behind a sign-in screen"
            );
            assert!(enquiries.is_empty());
        }
    }

    #[test]
    fn sign_in_a_restored_session_is_a_different_record_from_a_fresh_verification() {
        let (next, _) = advance(
            AuthPhase::Splash,
            AuthAnswer::SessionRestored {
                id: an_id(),
                outcome: Ok(Some(operator_id("op-7"))),
            },
        );
        match next {
            AuthPhase::SignedIn(SignedInAtTheTill::Restored { operator }) => {
                assert_eq!(operator, operator_id("op-7"));
            }
            other => panic!("a restored session produced {other:?}"),
        }
    }

    // ==================================================================
    // 3. The pairing poll is single-flight and never immediate
    // ==================================================================

    #[test]
    fn sign_in_a_pending_poll_folds_to_exactly_one_further_poll_after_a_delay() {
        let (next, enquiries) = advance(
            pairing(HardwareEnrolment::Undetermined, true),
            AuthAnswer::PairingStatusRead {
                id: an_id(),
                outcome: Ok(pairing_state(
                    PairingStatus::Pending,
                    HardwareEnrolment::NotEnrolled,
                )),
            },
        );

        assert_eq!(
            enquiries.len(),
            1,
            "a pending poll must produce one re-poll"
        );
        assert!(
            matches!(enquiries[0].asking, AuthEnquiry::PairingStatus { .. }),
            "the re-poll must be a status poll"
        );
        assert!(
            enquiries[0].run_after > Duration::ZERO,
            "an immediate re-poll is one request per frame against the platform"
        );
        assert!(matches!(
            next,
            AuthPhase::Pairing {
                poll_in_flight: true,
                ..
            }
        ));
    }

    #[test]
    fn sign_in_a_completed_pairing_stops_polling_and_catches_up_with_the_effect() {
        // The completed poll already registered this terminal and logged it in.
        let (next, enquiries) = advance(
            pairing(HardwareEnrolment::Undetermined, true),
            AuthAnswer::PairingStatusRead {
                id: an_id(),
                outcome: Ok(pairing_state(
                    PairingStatus::Completed,
                    HardwareEnrolment::AlreadyEnrolled,
                )),
            },
        );

        assert!(
            !enquiries
                .iter()
                .any(|e| matches!(e.asking, AuthEnquiry::PairingStatus { .. })),
            "a completed pairing must stop polling"
        );
        assert!(
            enquiries
                .iter()
                .any(|e| matches!(e.asking, AuthEnquiry::RestoreSession)),
            "the screen must catch up with the session the poll already created"
        );
        assert!(matches!(
            next,
            AuthPhase::Pairing {
                poll_in_flight: false,
                ..
            }
        ));
    }

    #[test]
    fn sign_in_an_expired_code_is_replaced_rather_than_polled() {
        for status in [PairingStatus::Expired, PairingStatus::Cancelled] {
            let (_, enquiries) = advance(
                pairing(HardwareEnrolment::NotEnrolled, true),
                AuthAnswer::PairingStatusRead {
                    id: an_id(),
                    outcome: Ok(pairing_state(
                        status.clone(),
                        HardwareEnrolment::NotEnrolled,
                    )),
                },
            );
            assert!(
                enquiries
                    .iter()
                    .any(|e| matches!(e.asking, AuthEnquiry::RequestPairingCode)),
                "{status:?} must ask for a new code"
            );
            assert!(
                !enquiries
                    .iter()
                    .any(|e| matches!(e.asking, AuthEnquiry::PairingStatus { .. })),
                "{status:?} must not keep polling a dead code"
            );
        }
    }

    #[test]
    fn sign_in_no_fold_ever_emits_an_immediate_pairing_poll() {
        // Swept rather than asserted at one site: a zero delay anywhere is the frame-rate
        // request storm, and the arm that introduces it need not be the one under test.
        let phases = || {
            vec![
                AuthPhase::Splash,
                pairing(HardwareEnrolment::Undetermined, false),
                pairing(HardwareEnrolment::AlreadyEnrolled, true),
            ]
        };
        let mut seen_polls = 0usize;

        for phase in phases() {
            for answer in every_answer(an_id()) {
                let (_, enquiries) = advance(phase_clone(&phase), answer);
                for enquiry in &enquiries {
                    if matches!(enquiry.asking, AuthEnquiry::PairingStatus { .. }) {
                        seen_polls += 1;
                        assert!(
                            enquiry.run_after >= PAIRING_POLL_INTERVAL,
                            "a pairing poll was scheduled with too short a delay: {:?}",
                            enquiry.run_after
                        );
                    }
                }
            }
        }

        // Control: the sweep found polls to check. Without this, deleting every poll would pass.
        assert!(
            seen_polls > 0,
            "the sweep saw no pairing polls at all, so it proved nothing"
        );
    }

    fn phase_clone(phase: &AuthPhase) -> AuthPhase {
        match phase {
            AuthPhase::Splash => AuthPhase::Splash,
            AuthPhase::Stalled(_) => {
                AuthPhase::Stalled(UndecidedNotice::for_startup_probe_failure())
            }
            AuthPhase::Pairing {
                code,
                expires_at,
                enrolment,
                poll_in_flight,
            } => AuthPhase::Pairing {
                code: code.clone(),
                expires_at: *expires_at,
                enrolment: *enrolment,
                poll_in_flight: *poll_in_flight,
            },
            AuthPhase::OperatorSelect { .. } => AuthPhase::OperatorSelect {
                session: session(),
                operators: Vec::new(),
            },
            AuthPhase::PinEntry { .. } => verifying(an_id()),
            AuthPhase::SignedIn(_) => AuthPhase::SignedIn(SignedInAtTheTill::Restored {
                operator: operator_id("op-1"),
            }),
        }
    }

    // ==================================================================
    // 4. Enrolment stays three-state
    // ==================================================================

    #[test]
    fn sign_in_all_three_enrolment_states_survive_the_fold_distinctly() {
        let mut seen = HashSet::new();

        for reported in [
            HardwareEnrolment::AlreadyEnrolled,
            HardwareEnrolment::NotEnrolled,
            HardwareEnrolment::Undetermined,
        ] {
            let (next, _) = advance(
                pairing(HardwareEnrolment::Undetermined, true),
                AuthAnswer::PairingStatusRead {
                    id: an_id(),
                    outcome: Ok(pairing_state(PairingStatus::Pending, reported)),
                },
            );
            match next {
                AuthPhase::Pairing { enrolment, .. } => {
                    assert_eq!(
                        enrolment, reported,
                        "the platform said {reported:?} and the fold stored {enrolment:?}"
                    );
                    seen.insert(format!("{enrolment:?}"));
                }
                other => panic!("a pending poll left the pairing phase: {other:?}"),
            }
        }

        assert_eq!(seen.len(), 3, "the three states did not stay three");
    }

    #[test]
    fn sign_in_undetermined_never_becomes_not_enrolled() {
        // The specific collapse the type exists to prevent: "nobody has said" spelled as "no".
        let (next, _) = advance(
            pairing(HardwareEnrolment::Undetermined, true),
            AuthAnswer::PairingStatusRead {
                id: an_id(),
                outcome: Ok(pairing_state(
                    PairingStatus::Pending,
                    HardwareEnrolment::Undetermined,
                )),
            },
        );
        match next {
            AuthPhase::Pairing { enrolment, .. } => assert_eq!(
                enrolment,
                HardwareEnrolment::Undetermined,
                "an unanswered enrolment question was answered by the fold"
            ),
            other => panic!("expected a pairing phase, got {other:?}"),
        }
    }

    // ==================================================================
    // 6. A hopeless refusal does not reopen the pad
    // ==================================================================

    #[test]
    fn sign_in_a_refusal_no_pin_can_fix_does_not_return_to_entering() {
        let hopeless = [
            PinRefusal::Locked,
            PinRefusal::OperatorUnknown,
            PinRefusal::OperatorInactive,
            PinRefusal::CredentialUnreadable,
            PinRefusal::CredentialExpired,
        ];

        for refusal in hopeless {
            assert!(
                !refusal.a_different_pin_could_help(),
                "{refusal:?} is in the hopeless list but the domain disagrees"
            );

            let (next, enquiries) = advance(
                verifying(an_id()),
                AuthAnswer::PinVerified {
                    id: an_id(),
                    outcome: PinVerification::Refused(refusal),
                },
            );

            match next {
                AuthPhase::PinEntry {
                    standing: PinEntryStanding::Refused(notice),
                    ..
                } => {
                    assert!(
                        !notice.pad().a_different_pin_could_help(),
                        "{refusal:?} offered the pad back"
                    );
                }
                AuthPhase::PinEntry {
                    standing: PinEntryStanding::Entering(_),
                    ..
                } => panic!("{refusal:?} returned to an entering state"),
                other => panic!("{refusal:?} produced {other:?}"),
            }
            assert!(enquiries.is_empty(), "{refusal:?} emitted a retry enquiry");
        }
    }

    // ==================================================================
    // Roster
    // ==================================================================

    #[test]
    fn sign_in_the_roster_offers_no_door_that_is_locked() {
        // An inactive operator's PIN can never be accepted, so listing them means the person at
        // the till learns that only after typing one.
        let rows = vec![
            OperatorRow {
                id: operator_id("op-active"),
                code: "A1".into(),
                employee_id: None,
                employee_number: None,
                name: OperatorName::new("Sara", Some("سارة")).expect("a well-formed name"),
                role: OperatorRole::Cashier,
                department: None,
                position: None,
                permissions: None,
                is_active: true,
            },
            OperatorRow {
                id: operator_id("op-inactive"),
                code: "A2".into(),
                employee_id: None,
                employee_number: None,
                name: OperatorName::new("Omar", Some("عمر")).expect("a well-formed name"),
                role: OperatorRole::Cashier,
                department: None,
                position: None,
                permissions: None,
                is_active: false,
            },
        ];

        let roster = OperatorCard::roster(&rows);
        assert_eq!(roster.len(), 1, "an inactive operator was offered");
        assert_eq!(roster[0].id(), &operator_id("op-active"));

        // Control: the filter is filtering, not emptying. Both scripts survive for the RTL list.
        assert_eq!(roster[0].name().arabic(), Some("سارة"));
        assert_eq!(roster[0].name().in_script(NameScript::Latin), "Sara");
    }

    #[test]
    fn sign_in_loading_operators_fills_the_roster_without_leaving_the_phase() {
        let (next, enquiries) = advance(
            AuthPhase::OperatorSelect {
                session: session(),
                operators: Vec::new(),
            },
            AuthAnswer::OperatorsLoaded {
                id: an_id(),
                outcome: Ok(Vec::new()),
            },
        );
        assert!(matches!(next, AuthPhase::OperatorSelect { .. }));
        assert!(enquiries.is_empty());
    }

    // ==================================================================
    // Totality
    // ==================================================================

    #[test]
    fn sign_in_the_fold_is_total_over_every_phase_and_answer() {
        // No panic path. Runs every answer against every phase; a missing arm would abort here
        // rather than in a till.
        let mut folded = 0usize;
        for phase_seed in [
            AuthPhase::Splash,
            AuthPhase::Stalled(UndecidedNotice::for_startup_probe_failure()),
            pairing(HardwareEnrolment::Undetermined, true),
            AuthPhase::OperatorSelect {
                session: session(),
                operators: Vec::new(),
            },
            verifying(an_id()),
            AuthPhase::SignedIn(SignedInAtTheTill::Restored {
                operator: operator_id("op-1"),
            }),
        ] {
            for answer in every_answer(an_id()) {
                let _ = advance(phase_clone(&phase_seed), answer);
                folded += 1;
            }
        }

        // Control: 6 phases x 14 answers. If either list is silently truncated this drops.
        assert_eq!(
            folded, 84,
            "the totality sweep did not cover what it claims"
        );
    }
}
