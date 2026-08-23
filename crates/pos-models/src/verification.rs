//! What happened when an operator entered their PIN.
//!
//! This replaced `PinVerificationResult`, deleted from `crates/pos-services/src/auth_service.rs`
//! by `auth-outcome-and-offline-lockout` — a `valid` boolean beside an operator id, a name and a
//! role, each a bare string, plus an optional message. All four were empty whenever the boolean
//! was false. That is a sum type spelled as a product, and every reader of it had to know which
//! fields the boolean made meaningful.
//!
//! [`PinVerification`] is that sum type spelled correctly, and it is **total**: "the till could
//! not find out" is a third case, not an error, so a `verify_pin` returning this returns it
//! directly rather than wrapping it in a `Result`. An offline till cannot decide, and that is
//! ordinary weather rather than a failure of the call.
//!
//! `AuthService::verify_pin` returns this, and `PinRefusal::consumes_an_attempt` is the only place
//! in the till that can spend an operator's lockout budget — which is what makes an `Undetermined`
//! *structurally incapable* of doing so.

use std::error::Error;
use std::fmt;
use std::num::NonZeroU8;

use chrono::{DateTime, Utc};
use thiserror::Error as ThisError;

use crate::operator::VerifiedOperator;
use crate::pin::PinLength;

// ============================================================================
// StoreFailure
// ============================================================================

/// What went wrong with the local store, in the shape a domain crate is allowed to hold.
///
/// # Why this is a struct with a boxed source rather than `LocalStoreUnavailable(rusqlite::Error)`
///
/// Naming `rusqlite::Error` here would make `pos-models` depend on `pos-db`'s database driver and
/// invert the dependency the whole approach rests on. The obvious repair — `impl
/// From<rusqlite::Error> for StoreFailure` written inside `pos-db` — **does not compile**: it is
/// `E0117`, because neither `From` nor `StoreFailure` is local to `pos-db`, and depending on a
/// crate confers no coherence standing over its types.
///
/// So the direction is reversed. `pos-models` publishes an inherent constructor, `pos-db` calls
/// it at its boundaries and boxes its own error as the source. `Box<dyn Error>` is std, so this
/// crate gains no dependency, and the underlying error is preserved rather than flattened into a
/// string — flattening is the error model this issue exists to remove.
///
/// A crate that wants nicer call sites may define its **own local** extension trait
/// (`trait IntoStoreFailure { fn into_store_failure(self, op: &'static str) -> StoreFailure; }`)
/// and implement it for its own error type. That is legal precisely because the trait is local.
#[derive(Debug, ThisError)]
#[error("{kind} while {operation}")]
pub struct StoreFailure {
    operation: &'static str,
    kind: StoreFailureKind,
    #[source]
    source: Option<Box<dyn Error + Send + Sync>>,
}

impl StoreFailure {
    /// Records a store failure.
    ///
    /// `operation` completes the sentence "… while {operation}", so phrase it as a gerund:
    /// `"reading the operator row"`, not `"read_operator"`. An error that cannot say what the
    /// till was doing is an error nobody can act on at three in the morning.
    pub const fn new(operation: &'static str, kind: StoreFailureKind) -> Self {
        Self {
            operation,
            kind,
            source: None,
        }
    }

    /// Attaches the underlying error, keeping it as an error rather than a message.
    pub fn caused_by(mut self, source: impl Error + Send + Sync + 'static) -> Self {
        self.source = Some(Box::new(source));
        self
    }

    /// What the till was doing when the store failed.
    pub const fn operation(&self) -> &'static str {
        self.operation
    }

    /// How the store failed.
    pub const fn kind(&self) -> StoreFailureKind {
        self.kind
    }
}

/// The ways the local store fails, from the point of view of a caller that has to decide what to
/// do next.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StoreFailureKind {
    /// The store could not be reached or opened at all.
    Unavailable,
    /// A statement ran and failed.
    QueryFailed,
    /// A row came back in a shape the caller could not read — a column holding a value outside
    /// the domain type it maps to, most often.
    RowUnreadable,
}

impl fmt::Display for StoreFailureKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Unavailable => "the local store was unavailable",
            Self::QueryFailed => "a query against the local store failed",
            Self::RowUnreadable => "a row in the local store could not be read",
        })
    }
}

// ============================================================================
// Authority and enrolment
// ============================================================================

/// When a locally stored credential stops being usable.
///
/// The `now` a caller checks against is a parameter rather than a `Utc::now()` inside this type:
/// which clock decides is a real question on a device someone else is holding, and a type that
/// reads the system clock quietly answers it for every caller.
///
/// An expiry is safe *here* in a way it is not on [`LockState`], and the asymmetry is the reason
/// only one of them has one. A credential that has expired fails **closed** — the till must reach
/// the platform — so an attacker who winds the clock forward only locks themselves out. A lockout
/// that expires fails **open**, so the same attacker unlocks the account. Same mechanism,
/// opposite consequence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CredentialExpiry(DateTime<Utc>);

impl CredentialExpiry {
    /// Records when the credential stops being usable.
    pub const fn at(instant: DateTime<Utc>) -> Self {
        Self(instant)
    }

    /// Whether the credential has expired as of `now`.
    pub fn has_passed(self, now: DateTime<Utc>) -> bool {
        now >= self.0
    }

    /// The instant itself.
    pub const fn instant(self) -> DateTime<Utc> {
        self.0
    }
}

impl fmt::Display for CredentialExpiry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0.to_rfc3339())
    }
}

/// Whether a locally stored credential may still be used to verify a PIN.
///
/// [`Self::Repudiated`] is **terminal**: a credential the platform has disowned must never fall
/// through to local verification, however unreachable the platform is. That is the whole reason
/// this is a type rather than a flag on a row — see [`Self::offline_authority`], which is the
/// only way to obtain an offline [`Authority`] and which returns `None` for a repudiated
/// credential.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EnrolmentState {
    /// The credential stands, and may verify a PIN while the platform is unreachable.
    Active,
    /// The platform has disowned this credential. No offline verification, ever.
    Repudiated,
}

/// Why the platform disowned this terminal's enrolment.
///
/// Both stop the till and neither may reach the local leg, so [`EnrolmentState`] folds them into
/// one — the decision is the same. They are still two, because they are **two different sentences
/// to the person standing at the drawer**, and only one of them has a remedy anybody there can
/// reach. A till that says "this device has been withdrawn" when an administrator could restore it
/// in thirty seconds sends someone home for the day.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, ThisError)]
pub enum Repudiation {
    /// The device was taken away. `POS_TERMINAL_GONE`, 403.
    ///
    /// Nothing at the till helps. The enrolment is over.
    #[error("this terminal has been withdrawn from the fleet")]
    Withdrawn,

    /// The terminal is enrolled and not active. `POS_TERMINAL_NOT_ACTIVE`, 403.
    ///
    /// Distinct from [`Self::Withdrawn`] precisely because it is recoverable: an administrator can
    /// reactivate it, and the person at the till has someone to call.
    #[error("this terminal is enrolled and not active; an administrator can reactivate it")]
    Suspended,
}

impl Repudiation {
    /// Whether anybody at the till can do something about this. Never true for
    /// [`Self::Withdrawn`].
    pub const fn has_a_remedy_at_the_till(self) -> bool {
        match self {
            Self::Suspended => true,
            Self::Withdrawn => false,
        }
    }
}

impl EnrolmentState {
    /// The authority a locally stored credential can confer.
    ///
    /// `Repudiated` confers none. Routing offline authority through this method is what makes
    /// "fell back to a repudiated credential because the network was down" something a caller has
    /// to write on purpose rather than something they can reach by accident.
    pub const fn offline_authority(self, not_after: CredentialExpiry) -> Option<Authority> {
        match self {
            Self::Active => Some(Authority::OfflineCredential { not_after }),
            Self::Repudiated => None,
        }
    }
}

/// Who decided that this PIN was correct.
///
/// A shift opened against a locally verified PIN is a different audit record from one the
/// platform verified, and the till uploads shifts. Collapsing the two would make an offline
/// decision indistinguishable from a server decision in the tenant's own books.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Authority {
    /// The platform verified the PIN.
    Platform,
    /// The till verified the PIN against a locally stored credential, valid until `not_after`.
    ///
    /// **Unconstructible in production as of schema v13**, which deleted `operators.pin_hash` —
    /// the till holds no credential to verify a PIN against, so no PIN is decided locally. That
    /// is a deliberate, temporary state of the world and not dead code: it is
    /// `offline-pin-verification-has-no-credential`'s target, and this variant is what stops the
    /// credential work from being able to journal a local decision as a platform one. **Do not
    /// tidy it away.**
    OfflineCredential {
        /// When the credential that authorised this stops being usable.
        not_after: CredentialExpiry,
    },
}

// ============================================================================
// Attempt counters and lock state
// ============================================================================

/// How many attempts an operator has left before the account locks.
///
/// Never zero. "Wrong PIN, and no attempts remain" is not a wrong-PIN outcome — it is
/// [`PinRefusal::Locked`], and a counter that could hold zero would let both spellings of the
/// same state exist at once.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AttemptsRemaining(NonZeroU8);

impl AttemptsRemaining {
    /// Builds a remaining-attempt count. `None` when none remain — the operator is locked, and
    /// the caller must say so with [`PinRefusal::Locked`] instead.
    pub const fn new(remaining: u8) -> Option<Self> {
        match NonZeroU8::new(remaining) {
            Some(remaining) => Some(Self(remaining)),
            None => None,
        }
    }

    /// The count.
    pub const fn get(self) -> u8 {
        self.0.get()
    }
}

impl fmt::Display for AttemptsRemaining {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// How many consecutive wrong PINs an operator has on record.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct FailedAttempts(u8);

impl FailedAttempts {
    /// No failures on record.
    pub const NONE: Self = Self(0);

    /// Records a failure count.
    pub const fn new(failures: u8) -> Self {
        Self(failures)
    }

    /// The count.
    pub const fn get(self) -> u8 {
        self.0
    }
}

impl fmt::Display for FailedAttempts {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Where an operator stands against their retry budget.
///
/// **There is no expiry field, and there must not be one.** PCI DSS v4.0 §8.3.4 permits a lockout
/// to end "after a minimum of 30 minutes *or until the user's identity is confirmed*". The till
/// takes the second branch, because the first is a timer read from the clock of whoever is
/// holding the device — an attacker who can set the date can end their own lockout. Identity
/// confirmation cannot be wound forward.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LockState {
    /// Not locked. `failures` consecutive wrong PINs are on record against the retry budget.
    Unlocked {
        /// Consecutive wrong PINs recorded so far.
        failures: FailedAttempts,
    },
    /// Locked until a supervisor or the platform confirms the operator's identity. Nothing else
    /// ends it — not a timer, and not a restart.
    LockedPendingIdentityConfirmation,
}

// ============================================================================
// The outcome
// ============================================================================

/// Why a PIN was refused.
///
/// Each variant is a distinct thing to say to the person at the till, and
/// [`Self::consumes_an_attempt`] is where the security-relevant distinction between them lives.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PinRefusal {
    /// The PIN did not match. This is the only refusal that spends the retry budget.
    WrongPin {
        /// Attempts left before the account locks. Never zero — see [`AttemptsRemaining`].
        attempts_remaining: AttemptsRemaining,
    },
    /// The account is locked and stays locked until someone confirms the operator's identity.
    /// Consumes no attempt: there is no budget left to spend.
    Locked,
    /// No operator with that identifier is known to this till. Consumes no attempt — there is no
    /// operator to charge it to, and charging an unknown identifier would let anyone exhaust a
    /// real operator's budget by guessing ids.
    OperatorUnknown,
    /// The operator exists but is not active in HR. Consumes no attempt: nothing the person at
    /// the till types could change the answer.
    OperatorInactive,
    /// The stored credential could not be read. **Consumes no attempt** — this is the till's
    /// fault, not the operator's, and spending their budget on it would lock people out of a
    /// terminal because of a corrupt row.
    ///
    /// **Nothing constructs this today, and that is correct rather than dead code.** Schema v13
    /// deleted `operators.pin_hash`, so there is no stored credential to be unreadable. This
    /// variant is `offline-pin-verification-has-no-credential`'s target: the moment a signed,
    /// terminal-bound credential exists, so does the state of holding a corrupt one. **Do not
    /// tidy it away** — deleting it would mean the next author has to re-derive that a corrupt
    /// credential must not cost an operator an attempt.
    CredentialUnreadable,
    /// The stored credential is past its expiry. **Consumes no attempt** — the operator must
    /// reach the platform, which no amount of retyping achieves.
    ///
    /// Unconstructed today, for the same reason as [`Self::CredentialUnreadable`] and kept for the
    /// same reason. See [`CredentialExpiry`] for why an expiry is safe here and not on
    /// [`LockState`].
    CredentialExpired,
    /// The stored credential was enrolled under a different PIN length than the tenant now
    /// requires. **Consumes no attempt**, and this is the variant that matters most: a tenant
    /// changing their policy from four digits to six would otherwise burn every operator's retry
    /// budget on PINs that were correct when they were set, and lock out the whole company.
    ///
    /// # Half of it is live and half is not
    ///
    /// The **online** half is constructed: the platform answers `POS_PIN_ROTATION_REQUIRED` and
    /// `AuthService::verify_pin` maps it here, carrying the required length out of the refusal's
    /// `details`. The **offline** half — a locally stored credential whose enrolled length no
    /// longer matches the policy — cannot exist yet, because schema v13 left the till no stored
    /// credential to have been enrolled under anything.
    ///
    /// That half is `offline-pin-verification-has-no-credential`'s, and it is the reason this
    /// variant is shaped to carry `expected` rather than being a bare marker: the till has to be
    /// able to say *six digits now* to someone whose four still work everywhere else.
    CredentialRequiresRotation {
        /// The length the tenant's policy now requires.
        expected: PinLength,
    },
}

impl PinRefusal {
    /// Whether this refusal spends one of the operator's remaining attempts.
    ///
    /// The match is exhaustive with no catch-all arm, on purpose. A new refusal variant fails to
    /// compile here, which forces whoever adds it to answer this question deliberately — and
    /// "does this count against the lockout counter" is exactly the question a default `_ =>
    /// true` would answer wrongly and silently.
    pub const fn consumes_an_attempt(self) -> bool {
        match self {
            Self::WrongPin { .. } => true,
            Self::Locked
            | Self::OperatorUnknown
            | Self::OperatorInactive
            | Self::CredentialUnreadable
            | Self::CredentialExpired
            | Self::CredentialRequiresRotation { .. } => false,
        }
    }
}

/// Why the till could not decide.
///
/// Not an error: an offline till is the normal condition this product is built for, and a caller
/// that has to distinguish "refused" from "could not ask" cannot be handed `Err` for both.
#[derive(Debug, ThisError)]
pub enum UndeterminedCause {
    /// The platform could not be reached, and no local credential settled it.
    #[error("the platform could not be reached, and no local credential could settle the PIN")]
    ServerUnreachable,

    /// The local store could not answer.
    #[error("the local store could not answer: {0}")]
    StoreUnavailable(#[from] StoreFailure),

    /// The stored terminal session was rejected and could not be renewed, so the till has no
    /// standing to ask the platform anything.
    #[error("the terminal session was rejected and could not be renewed")]
    ReauthFailed,

    /// The platform has disowned this terminal's enrolment.
    ///
    /// **Terminal, and it must never fall through to the local leg.** That is the one branch where
    /// the till would override a decision the server actually made: the platform was reached, it
    /// answered, and the answer was that this device is not one of theirs. Retrying does not help
    /// and neither does a local credential — which is why this is routed through
    /// [`EnrolmentState::offline_authority`], the only source of an offline [`Authority`], and why
    /// that method returns `None` for [`EnrolmentState::Repudiated`].
    ///
    /// Distinct from [`Self::ReauthFailed`], which is *"we could not renew"* — an answer about a
    /// session. This is an answer about the device.
    #[error("the platform has disowned this terminal: {0}")]
    EnrolmentRepudiated(#[source] Repudiation),

    /// The terminal has no `secretHash` on the platform, so no credential can be sealed for it.
    ///
    /// The enrolment is not disowned — the device is simply half-provisioned, and the remedy is to
    /// pair it again. **Not worth retrying**, which is what separates it from
    /// [`Self::ServerUnreachable`]: the answer will be the same every time until somebody pairs
    /// the device.
    #[error("this terminal is not fully provisioned; it must be paired again")]
    TerminalNotProvisioned,

    /// The platform answered, and the till could not read the answer.
    ///
    /// **Never fold this into [`Self::ServerUnreachable`].** A response that arrived and did not
    /// match the contract means the two systems disagree about the shape of an endpoint — one of
    /// them has a bug, and treating it as a network blip is how that disagreement survives to
    /// production, retried forever by a caller that thinks it is weather. It is the same
    /// distinction `pos_api::ApiFailure` draws between `Unreachable` and `Unreadable`, carried
    /// into the outcome so it survives the mapping.
    #[error("the platform answered, and the till could not read the answer")]
    ContractBreach {
        /// What could not be read. Kept as an error rather than flattened into a message.
        #[source]
        source: Box<dyn Error + Send + Sync>,
    },
}

impl UndeterminedCause {
    /// Records that the platform's answer could not be read.
    ///
    /// A constructor rather than a `From`, so the widening is written on purpose: almost every
    /// error in reach could be boxed into this, and only a contract breach belongs here.
    pub fn contract_breach(source: impl Error + Send + Sync + 'static) -> Self {
        Self::ContractBreach {
            source: Box::new(source),
        }
    }
}

/// What happened when an operator entered their PIN.
///
/// Total in all three directions: accepted, refused for a stated reason, or undecided for a
/// stated reason. There is deliberately no `is_accepted()` and no `operator()` shortcut — either
/// would let a caller act on the outcome without seeing the other two cases, which is precisely
/// what the `valid: bool` this replaces allowed.
#[derive(Debug)]
pub enum PinVerification {
    /// The PIN was correct, and `decided_by` records who decided that.
    Accepted {
        /// The operator, in the projection that carries no PIN material.
        operator: VerifiedOperator,
        /// Whether the platform or a local credential made this decision.
        decided_by: Authority,
    },
    /// The PIN was not accepted, for a reason the caller must handle.
    Refused(PinRefusal),
    /// The till could not find out.
    Undetermined(UndeterminedCause),
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::operator::{OperatorId, OperatorName, OperatorPermissions, OperatorRole};

    fn expiry(hour: u32) -> CredentialExpiry {
        CredentialExpiry::at(
            DateTime::parse_from_rfc3339(&format!("2026-08-22T{hour:02}:00:00Z"))
                .expect("a well-formed instant")
                .with_timezone(&Utc),
        )
    }

    fn boxed_source() -> std::num::ParseIntError {
        "not a number"
            .parse::<u8>()
            .expect_err("this must not parse")
    }

    #[test]
    fn verification_only_a_wrong_pin_spends_the_retry_budget() {
        // Every variant, listed. This is the canary the card asked for: `consumes_an_attempt` has
        // no catch-all arm, so a new refusal fails to compile there, and this test fails to
        // compile here until it is listed too.
        let spends = PinRefusal::WrongPin {
            attempts_remaining: AttemptsRemaining::new(2).expect("two is not zero"),
        };
        assert!(spends.consumes_an_attempt());

        for refusal in [
            PinRefusal::Locked,
            PinRefusal::OperatorUnknown,
            PinRefusal::OperatorInactive,
            PinRefusal::CredentialUnreadable,
            PinRefusal::CredentialExpired,
            PinRefusal::CredentialRequiresRotation {
                expected: PinLength::Six,
            },
        ] {
            assert!(
                !refusal.consumes_an_attempt(),
                "{refusal:?} must not spend an attempt"
            );
        }
    }

    #[test]
    fn verification_a_policy_length_change_cannot_lock_out_a_company() {
        // The scenario the rotation variant exists for: a tenant moves from four digits to six,
        // and every stored credential is suddenly the wrong length. If that spent an attempt,
        // three logins would lock every operator out of every terminal at once.
        let rotation = PinRefusal::CredentialRequiresRotation {
            expected: PinLength::Six,
        };

        assert!(!rotation.consumes_an_attempt());
    }

    #[test]
    fn verification_attempts_remaining_is_never_zero() {
        assert_eq!(AttemptsRemaining::new(0), None);
        assert_eq!(
            AttemptsRemaining::new(1).map(AttemptsRemaining::get),
            Some(1)
        );
    }

    #[test]
    fn verification_a_repudiated_credential_confers_no_offline_authority() {
        let not_after = expiry(23);

        assert_eq!(
            EnrolmentState::Active.offline_authority(not_after),
            Some(Authority::OfflineCredential { not_after })
        );
        assert_eq!(
            EnrolmentState::Repudiated.offline_authority(not_after),
            None
        );
    }

    #[test]
    fn verification_credential_expiry_is_judged_against_the_callers_clock() {
        let not_after = expiry(12);

        assert!(!not_after.has_passed(expiry(11).instant()));
        assert!(
            not_after.has_passed(expiry(12).instant()),
            "expiry is inclusive"
        );
        assert!(not_after.has_passed(expiry(13).instant()));
    }

    #[test]
    fn verification_store_failure_keeps_its_cause_as_an_error() {
        let failure =
            StoreFailure::new("reading the operator row", StoreFailureKind::RowUnreadable)
                .caused_by(boxed_source());

        // The cause is still an error, not a string flattened into a message.
        let source = failure.source().expect("the cause must survive");
        assert_eq!(source.to_string(), boxed_source().to_string());
        assert_eq!(failure.operation(), "reading the operator row");
        assert_eq!(failure.kind(), StoreFailureKind::RowUnreadable);
    }

    #[test]
    fn verification_store_failure_names_what_the_till_was_doing() {
        let failure = StoreFailure::new(
            "opening the credential store",
            StoreFailureKind::Unavailable,
        );

        assert_eq!(
            failure.to_string(),
            "the local store was unavailable while opening the credential store"
        );
        // Without a cause there is simply no cause, not an empty one.
        assert!(failure.source().is_none());
    }

    #[test]
    fn verification_undetermined_wraps_a_store_failure_without_flattening_it() {
        let cause = UndeterminedCause::from(
            StoreFailure::new("reading the operator row", StoreFailureKind::QueryFailed)
                .caused_by(boxed_source()),
        );

        assert!(matches!(cause, UndeterminedCause::StoreUnavailable(_)));
        // Two levels down, the driver's own error is still reachable.
        let store = cause.source().expect("the store failure");
        assert!(store.source().is_some(), "the driver error must survive");
    }

    #[test]
    fn verification_accepted_carries_the_operator_and_who_decided() {
        let operator = VerifiedOperator::from_verified_pin(
            OperatorId::new("op-1").unwrap(),
            OperatorName::new("Ahmed Hassan", Some("أحمد حسن")).unwrap(),
            OperatorRole::Supervisor,
            OperatorPermissions::none(),
        );
        let outcome = PinVerification::Accepted {
            operator,
            decided_by: Authority::OfflineCredential {
                not_after: expiry(23),
            },
        };

        match outcome {
            PinVerification::Accepted {
                operator,
                decided_by,
            } => {
                assert_eq!(operator.role(), OperatorRole::Supervisor);
                // An offline decision is distinguishable from a platform one, which is what the
                // uploaded shift record needs.
                assert!(matches!(decided_by, Authority::OfflineCredential { .. }));
            }
            PinVerification::Refused(_) | PinVerification::Undetermined(_) => {
                panic!("an accepted verification is not a refusal")
            }
        }
    }

    #[test]
    fn verification_lock_state_records_failures_only_while_unlocked() {
        let unlocked = LockState::Unlocked {
            failures: FailedAttempts::new(2),
        };
        match unlocked {
            LockState::Unlocked { failures } => assert_eq!(failures.get(), 2),
            LockState::LockedPendingIdentityConfirmation => panic!("this one is unlocked"),
        }

        // The locked case carries nothing — in particular no expiry, because a timer is read from
        // the clock of whoever holds the device.
        assert_eq!(FailedAttempts::NONE.get(), 0);
        assert_eq!(FailedAttempts::default(), FailedAttempts::NONE);
    }
}
