//! The seam between an immediate-mode screen and services that take time.
//!
//! Pure data: no async, no toolkit, no service handles. A frame produces an [`AuthEnquiry`], the
//! driver runs it, and an [`AuthAnswer`] comes back to be folded. Keeping the seam as values is
//! what lets step 08's fold be a pure function and step 12's driver be the only thing that knows
//! about tasks.
//!
//! # Two properties this module exists to make structural
//!
//! **An answer is matched to its question by [`EnquiryId`], never by shape.** Two pairing-status
//! polls in flight produce two identical-looking answers; without an id the fold cannot tell the
//! answer to the question it is still waiting for from the answer to one it abandoned.
//!
//! **Most of these enquiries have already changed the world by the time an answer exists.** See
//! [`Discardable`]. That is the property step 08 keys on, and getting it backwards was a real
//! defect in this issue's design: the first draft proposed stamping answers and discarding stale
//! ones, which does nothing when the effect landed before the answer was produced.

use std::num::NonZeroU64;
use std::time::Duration;

use pos_db::OperatorRow;
use pos_models::{EnteredDigits, OperatorId, PinVerification};
use pos_services::{PairingState, TerminalSession};

// ============================================================================
// Identity
// ============================================================================

/// Which enquiry an answer belongs to.
///
/// `NonZeroU64` so a zero-initialised field cannot pass for a real id — there is no
/// `Default`, and the only way to obtain one is [`EnquiryIds::mint`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EnquiryId(NonZeroU64);

impl EnquiryId {
    /// The raw value, for logging. Not a constructor in either direction.
    pub const fn get(self) -> u64 {
        self.0.get()
    }
}

/// Mints [`EnquiryId`]s in sequence.
///
/// A value the driver owns rather than a global counter: a process-wide `AtomicU64` would make
/// ids shared state between every screen that ever asks anything, and make a test's ids depend on
/// what other tests had run. One screen, one minter, ids that start at 1 every time.
#[derive(Debug)]
pub struct EnquiryIds(u64);

impl EnquiryIds {
    /// A fresh sequence. The first [`mint`](Self::mint) returns 1.
    pub const fn new() -> Self {
        Self(0)
    }

    /// The next id.
    ///
    /// Saturates rather than wrapping. Wrapping would reissue a live id after 2^64 enquiries and
    /// silently match an answer to the wrong question; saturating reissues the last id instead,
    /// which is equally wrong but cannot be reached without 2^64 enquiries in one session, and
    /// this way the failure is a stuck counter rather than a collision that looks fine.
    pub fn mint(&mut self) -> EnquiryId {
        self.0 = self.0.saturating_add(1);
        EnquiryId(NonZeroU64::new(self.0).expect("saturating_add from 0 never yields 0"))
    }
}

impl Default for EnquiryIds {
    fn default() -> Self {
        Self::new()
    }
}

/// The code the platform issued for this pairing attempt.
///
/// A newtype because the till hands it straight back to the platform, and a bare `String`
/// parameter beside a hardware id and a terminal code is three unmarked sockets that accept each
/// other.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PairingCode(String);

impl PairingCode {
    /// Wraps a code the platform issued.
    pub fn new(code: impl Into<String>) -> Self {
        Self(code.into())
    }

    /// The code, for the request that carries it.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

// ============================================================================
// What the screen asks
// ============================================================================

/// Whether an answer can be thrown away without leaving a trace.
///
/// # Why this is not a `bool` on the driver
///
/// This issue's design first proposed stamping each answer and discarding the stale ones. That is
/// backwards for every enquiry here except one: **the effect lands before the answer exists**, so
/// discarding the answer keeps the effect and loses only the screen's knowledge of it. A screen
/// that abandons a PIN verification does not un-mint the operator session the platform just
/// created — it simply stops knowing that it has one.
///
/// So the property is a fact about the enquiry, declared here, and step 08's fold must accept any
/// [`Self::Never`] answer whether or not the screen has moved on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Discardable {
    /// Nothing has happened yet. Dropping the answer costs nothing.
    Freely,

    /// An effect has already landed, and dropping the answer does not undo it.
    ///
    /// Names the effect, so the reason is legible at the point a fold decides what to do rather
    /// than only in this file.
    Never {
        /// What has already happened by the time the answer arrives.
        effect: &'static str,
    },
}

/// What the screen has asked the services to find out.
///
/// Not `Clone`: [`Self::VerifyPin`] owns an [`EnteredDigits`], which zeroizes on drop and must not
/// be duplicated into a buffer nothing will zeroize.
#[derive(Debug)]
pub enum AuthEnquiry {
    /// Is somebody already signed in at this till?
    ///
    /// **Not effect-free.** `OperatorSignIn::restore` presents the held operator token to the API
    /// client before returning, so by the time an answer exists the client is authenticated as
    /// that operator.
    RestoreSession,

    /// Ask the platform for a pairing code.
    ///
    /// **Not effect-free.** The platform records a pairing attempt against this hardware id, and
    /// the code it issues has an expiry running from the moment it was created — asking twice
    /// invalidates nothing but does start a second clock.
    RequestPairingCode,

    /// Has anybody approved the pairing code yet?
    ///
    /// **Not effect-free, and this is the one that surprises.** A successful status poll does not
    /// merely report approval: it registers the terminal locally and logs it in. The answer that
    /// says "approved" arrives at a till that is already enrolled and holding a session.
    PairingStatus {
        /// The code being polled.
        code: PairingCode,
    },

    /// Read the operators this till knows about.
    ///
    /// **Effect-free** — the only one here. A local read that changes nothing, which is why a
    /// stale answer to it can simply be dropped.
    LoadOperators,

    /// Check a PIN against the platform, or locally if the platform cannot be reached.
    ///
    /// **Not effect-free.** A correct PIN mints an operator session on the platform and
    /// `record_and_present` stores and presents it before the outcome is returned. A wrong PIN
    /// spends an attempt out of the operator's budget. Neither is undone by ignoring the answer.
    VerifyPin {
        /// Whose PIN is being checked.
        operator: OperatorId,
        /// The digits, in the buffer that zeroizes when it drops.
        digits: EnteredDigits,
    },
}

impl AuthEnquiry {
    /// Whether an answer to this enquiry may be discarded.
    ///
    /// Exhaustive with no catch-all arm. A new enquiry must fail to compile here, because the
    /// wrong default is [`Discardable::Freely`] — which silently drops the record of an effect
    /// that has already happened.
    pub const fn discardable(&self) -> Discardable {
        match self {
            Self::LoadOperators => Discardable::Freely,
            Self::RestoreSession => Discardable::Never {
                effect: "the held operator token has been presented to the API client",
            },
            Self::RequestPairingCode => Discardable::Never {
                effect: "the platform has recorded a pairing attempt and started an expiry clock",
            },
            Self::PairingStatus { .. } => Discardable::Never {
                effect: "an approved poll registers this terminal locally and logs it in",
            },
            Self::VerifyPin { .. } => Discardable::Never {
                effect: "a correct PIN has minted and stored an operator session; a wrong one has \
                         spent an attempt",
            },
        }
    }
}

/// An enquiry the fold has asked for, before an id has been stamped on it.
///
/// The fold produces these; the driver turns each into a [`DispatchedEnquiry`] by minting an id.
/// Ids are stamped at dispatch rather than in the fold because an id records that an enquiry
/// actually left, and only the driver knows that.
#[derive(Debug)]
pub struct PendingEnquiry {
    /// How long to wait before running it.
    pub run_after: Duration,
    /// What is being asked.
    pub asking: AuthEnquiry,
}

impl PendingEnquiry {
    /// Run it as soon as the driver can.
    ///
    /// Spelled out rather than defaulted: see [`DispatchedEnquiry`] for why an omitted delay is
    /// the failure this type is shaped against.
    pub const fn now(asking: AuthEnquiry) -> Self {
        Self {
            run_after: Duration::ZERO,
            asking,
        }
    }

    /// Run it after a wait.
    pub const fn after(run_after: Duration, asking: AuthEnquiry) -> Self {
        Self { run_after, asking }
    }

    /// Stamps this for dispatch.
    pub fn dispatch(self, ids: &mut EnquiryIds) -> DispatchedEnquiry {
        DispatchedEnquiry::new(ids, self.run_after, self.asking)
    }
}

/// An enquiry that has been handed to the driver.
///
/// # Why the delay is a field and not a fourth type
///
/// A pending pairing poll with no delay folds to another poll on the very next frame. At sixty
/// frames a second that is 3,600 requests a minute against the platform, for a code a human is
/// reading off a screen and typing somewhere else. Carrying [`Self::run_after`] on every enquiry
/// means the fold cannot produce an immediate re-poll by omission — it has to name a duration,
/// and [`Duration::ZERO`] is a thing somebody wrote on purpose.
#[derive(Debug)]
pub struct DispatchedEnquiry {
    /// The id an answer must carry to be matched to this.
    pub id: EnquiryId,
    /// How long to wait before running it.
    pub run_after: Duration,
    /// What is being asked.
    pub asking: AuthEnquiry,
}

impl DispatchedEnquiry {
    /// Stamps an enquiry for dispatch.
    pub fn new(ids: &mut EnquiryIds, run_after: Duration, asking: AuthEnquiry) -> Self {
        Self {
            id: ids.mint(),
            run_after,
            asking,
        }
    }

    /// Whether an answer to this may be discarded. Delegates to [`AuthEnquiry::discardable`].
    pub const fn discardable(&self) -> Discardable {
        self.asking.discardable()
    }
}

// ============================================================================
// What comes back
// ============================================================================

/// What the services found out.
///
/// Every arm carries the service's own outcome, including its failure. Nothing is flattened into
/// a string on the way through, and there is deliberately no error type declared in this module:
/// an error invented here would have to be built by discarding what the service actually said.
///
/// [`Self::PinVerified`] carries no `Result` because [`PinVerification`] is already total in all
/// three directions — accepted, refused, undecided — and wrapping it would add a fourth way to
/// fail that means nothing.
#[derive(Debug)]
pub enum AuthAnswer {
    /// Whether a held operator session was restored, and who it belongs to.
    SessionRestored {
        /// The enquiry this answers.
        id: EnquiryId,
        /// `Ok(None)` means nobody was signed in — not a failure.
        outcome: anyhow::Result<Option<OperatorId>>,
    },

    /// The platform issued a pairing code, or did not.
    PairingCodeRequested {
        /// The enquiry this answers.
        id: EnquiryId,
        /// The state, carrying the code, its expiry, and the enrolment signal.
        outcome: anyhow::Result<PairingState>,
    },

    /// The current state of a pairing attempt.
    PairingStatusRead {
        /// The enquiry this answers.
        id: EnquiryId,
        /// The state, whose `enrolment` is `Undetermined` until the platform has answered once.
        outcome: anyhow::Result<PairingState>,
    },

    /// A terminal session, where an approved pairing produced one.
    TerminalSessionOpened {
        /// The enquiry this answers.
        id: EnquiryId,
        /// `Ok(None)` means no saved session existed.
        outcome: anyhow::Result<Option<TerminalSession>>,
    },

    /// The operators this till knows about.
    OperatorsLoaded {
        /// The enquiry this answers.
        id: EnquiryId,
        /// Rows carrying both scripts of each name, which is what an Arabic-locale list needs.
        outcome: anyhow::Result<Vec<OperatorRow>>,
    },

    /// What happened when a PIN was checked.
    PinVerified {
        /// The enquiry this answers.
        id: EnquiryId,
        /// Total in all three directions; no `Result` wrapping it.
        outcome: PinVerification,
    },
}

impl AuthAnswer {
    /// Which enquiry this answers.
    ///
    /// Exhaustive with no catch-all: a new answer variant that forgot its id would otherwise be
    /// unmatched forever by the fold, which is a hang rather than an error.
    pub const fn id(&self) -> EnquiryId {
        match self {
            Self::SessionRestored { id, .. }
            | Self::PairingCodeRequested { id, .. }
            | Self::PairingStatusRead { id, .. }
            | Self::TerminalSessionOpened { id, .. }
            | Self::OperatorsLoaded { id, .. }
            | Self::PinVerified { id, .. } => *id,
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use pos_models::Digit;

    fn operator() -> OperatorId {
        OperatorId::new("op-1").expect("a well-formed operator id")
    }

    fn digits() -> EnteredDigits {
        let mut entered = EnteredDigits::empty();
        for value in [1, 2, 3, 4] {
            entered.push(Digit::new(value).expect("a single decimal digit"));
        }
        entered
    }

    /// Every enquiry, so a new variant fails this list as well as the match in `discardable`.
    fn every_enquiry() -> [AuthEnquiry; 5] {
        [
            AuthEnquiry::RestoreSession,
            AuthEnquiry::RequestPairingCode,
            AuthEnquiry::PairingStatus {
                code: PairingCode::new("ABC123"),
            },
            AuthEnquiry::LoadOperators,
            AuthEnquiry::VerifyPin {
                operator: operator(),
                digits: digits(),
            },
        ]
    }

    // ------------------------------------------------------------------
    // Identity
    // ------------------------------------------------------------------

    #[test]
    fn sign_in_successive_enquiry_ids_are_distinct() {
        let mut ids = EnquiryIds::new();
        let minted: Vec<EnquiryId> = (0..64).map(|_| ids.mint()).collect();

        for (i, a) in minted.iter().enumerate() {
            for b in minted.iter().skip(i + 1) {
                assert_ne!(a, b, "two enquiries were given the same id");
            }
        }
    }

    #[test]
    fn sign_in_a_fresh_minter_starts_at_one_rather_than_zero() {
        // Not cosmetic: `EnquiryId` is `NonZeroU64` precisely so a zeroed field cannot pass for a
        // real id, and a minter whose first value was 0 could not produce one at all.
        let mut ids = EnquiryIds::new();
        assert_eq!(ids.mint().get(), 1);
        assert_eq!(ids.mint().get(), 2);

        // Two independent screens do not share a sequence.
        let mut other = EnquiryIds::new();
        assert_eq!(other.mint().get(), 1);
    }

    // ------------------------------------------------------------------
    // The property step 08 keys on
    // ------------------------------------------------------------------

    #[test]
    fn sign_in_every_enquiry_declares_whether_its_answer_may_be_dropped() {
        // Table-style on purpose: adding an enquiry without answering this question fails here as
        // well as in the match, and the expected value is written out rather than derived from
        // the thing under test.
        let expectations: [(AuthEnquiry, bool); 5] = [
            (AuthEnquiry::LoadOperators, true),
            (AuthEnquiry::RestoreSession, false),
            (AuthEnquiry::RequestPairingCode, false),
            (
                AuthEnquiry::PairingStatus {
                    code: PairingCode::new("ABC123"),
                },
                false,
            ),
            (
                AuthEnquiry::VerifyPin {
                    operator: operator(),
                    digits: digits(),
                },
                false,
            ),
        ];

        for (enquiry, may_drop) in expectations {
            let discardable = matches!(enquiry.discardable(), Discardable::Freely);
            assert_eq!(
                discardable, may_drop,
                "{enquiry:?} declares the wrong thing about dropping its answer"
            );
        }
    }

    #[test]
    fn sign_in_only_the_local_read_is_effect_free() {
        // The security-relevant half stated as its own assertion: if more than one enquiry ever
        // reports `Freely`, something that changes the world has been marked droppable.
        let free: Vec<String> = every_enquiry()
            .iter()
            .filter(|enquiry| matches!(enquiry.discardable(), Discardable::Freely))
            .map(|enquiry| format!("{enquiry:?}"))
            .collect();

        assert_eq!(
            free.len(),
            1,
            "exactly one enquiry on this screen is effect-free; the rest commit before \
             answering. Reported free: {free:?}"
        );
        assert!(
            free[0].starts_with("LoadOperators"),
            "the effect-free enquiry is the local read, not {:?}",
            free[0]
        );
    }

    #[test]
    fn sign_in_every_committing_enquiry_names_the_effect_it_has_already_had() {
        // A `Never` arm with an empty reason would satisfy the type and tell a later reader
        // nothing, which is how a declaration decays into a checkbox.
        for enquiry in every_enquiry() {
            if let Discardable::Never { effect } = enquiry.discardable() {
                assert!(
                    effect.len() > 20,
                    "{enquiry:?} declares an effect but does not describe it: {effect:?}"
                );
            }
        }
    }

    // ------------------------------------------------------------------
    // Dispatch
    // ------------------------------------------------------------------

    #[test]
    fn sign_in_a_dispatched_enquiry_carries_its_own_delay() {
        let mut ids = EnquiryIds::new();
        let poll = DispatchedEnquiry::new(
            &mut ids,
            Duration::from_secs(2),
            AuthEnquiry::PairingStatus {
                code: PairingCode::new("ABC123"),
            },
        );

        assert_eq!(poll.run_after, Duration::from_secs(2));
        assert_eq!(poll.id.get(), 1);

        // The delay is per-enquiry, not per-screen: a pairing poll waits, a PIN check does not.
        let verify = DispatchedEnquiry::new(
            &mut ids,
            Duration::ZERO,
            AuthEnquiry::VerifyPin {
                operator: operator(),
                digits: digits(),
            },
        );
        assert_eq!(verify.run_after, Duration::ZERO);
        assert_eq!(verify.id.get(), 2);
    }

    #[test]
    fn sign_in_dispatch_delegates_the_discardable_question_rather_than_restating_it() {
        let mut ids = EnquiryIds::new();
        for enquiry in every_enquiry() {
            let expected = enquiry.discardable();
            let dispatched = DispatchedEnquiry::new(&mut ids, Duration::ZERO, enquiry);
            assert_eq!(dispatched.discardable(), expected);
        }
    }

    // ------------------------------------------------------------------
    // Answers
    // ------------------------------------------------------------------

    #[test]
    fn sign_in_every_answer_reports_the_enquiry_it_answers() {
        let mut ids = EnquiryIds::new();
        let id = ids.mint();

        let answers = [
            AuthAnswer::SessionRestored {
                id,
                outcome: Ok(None),
            },
            AuthAnswer::TerminalSessionOpened {
                id,
                outcome: Ok(None),
            },
            AuthAnswer::OperatorsLoaded {
                id,
                outcome: Ok(Vec::new()),
            },
            AuthAnswer::PinVerified {
                id,
                outcome: PinVerification::Refused(pos_models::PinRefusal::Locked),
            },
        ];

        for answer in answers {
            assert_eq!(
                answer.id(),
                id,
                "an answer lost the id of the enquiry it belongs to"
            );
        }
    }

    #[test]
    fn sign_in_a_failure_arrives_as_the_services_own_error_not_a_string() {
        // The point of the seam: nothing is flattened on the way through. If this ever needs a
        // `to_string()` to compile, an error type has been invented somewhere it should not be.
        let mut ids = EnquiryIds::new();
        let answer = AuthAnswer::SessionRestored {
            id: ids.mint(),
            outcome: Err(anyhow::anyhow!("the local store was unavailable")),
        };

        match answer {
            AuthAnswer::SessionRestored { outcome, .. } => {
                let error = outcome.expect_err("this fixture is an error");
                assert!(error.to_string().contains("local store"));
            }
            other => panic!("matched the wrong arm: {other:?}"),
        }
    }

    #[test]
    fn sign_in_a_pairing_code_is_not_interchangeable_with_a_bare_string() {
        let code = PairingCode::new("ABC123");
        assert_eq!(code.as_str(), "ABC123");
        // The newtype exists because a pairing code, a hardware id and a terminal code are three
        // `String`s that would otherwise accept each other at every call site.
        assert_ne!(code, PairingCode::new("ABC124"));
    }
}
