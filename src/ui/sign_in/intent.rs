//! What a person does to the sign-in screen, and what it does to the phase.
//!
//! [`super::advance`] folds *answers* — things the platform and the local store say. This folds
//! *intents* — things a cashier does. Both return the same pair, and neither is allowed to be the
//! place where the other's rules live.
//!
//! Splitting them is not ceremony. An answer arrives whether or not anyone is looking and may
//! land in a phase that has moved on; an intent is only ever produced by a control that the
//! current phase drew. The two have different totality obligations, and folding them through one
//! function would hide that: every `(phase, answer)` pair must have an arm, whereas most
//! `(phase, intent)` pairs are simply unreachable — a digit cannot arrive while the splash screen
//! is up, because no keypad is on screen to produce one.
//!
//! # The unreachable pairs are still handled, and still do nothing
//!
//! They fall to one catch-all that returns the phase untouched. That is deliberate rather than
//! lazy: a stale intent is a control that was drawn last frame and clicked this one, and the right
//! response to it is to ignore it. Unlike a stale *answer*, no effect has landed — nothing was
//! sent anywhere — so dropping it costs exactly nothing. This is the one place in the sign-in
//! machine where a catch-all is the correct construct, and the asymmetry with `advance` is the
//! reason [`super::enquiry::Discardable`] exists.

use pos_models::{Digit, OperatorId};

use super::enquiry::{AuthEnquiry, DispatchedEnquiry, EnquiryIds, PendingEnquiry};
use super::notice::Recheck;
use super::phase::{AuthPhase, PinEntryStanding};

/// Something a person did to the screen.
///
/// Deliberately small. Every variant corresponds to a control that some phase actually draws, and
/// nothing here names a *screen* — an intent says what was done, never where the screen should go
/// next, which is [`apply`]'s decision and nobody else's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Intent {
    /// A card on the operator list was activated.
    ///
    /// Carries the id rather than the card: the card is a render-ready projection the roster owns,
    /// and an intent that carried one would let a screen hand back a card the phase never issued.
    ChooseOperator(OperatorId),

    /// A digit was pressed on the PIN keypad.
    ///
    /// Carries a [`Digit`], not a `char` and not a `u8`: the keypad's own event carries the
    /// numeric newtype, so there is no parsing step here and no way to push a `'x'` into a PIN
    /// buffer.
    PressDigit(Digit),

    /// The last digit was taken back.
    Backspace,

    /// The whole entry was cleared.
    ClearEntry,

    /// The deliberate submit.
    ///
    /// Separate from [`Self::PressDigit`] because the keypad has no submit event of its own — the
    /// point-of-sale keypad has one, the PIN keypad does not — so submitting is always an explicit
    /// act on a button, never a side effect of reaching a length.
    SubmitPin,

    /// Try the thing that failed again.
    ///
    /// One variant rather than one per phase: what "again" means is a property of the phase the
    /// screen is on, and [`apply`] is where that is known.
    Retry,
}

/// Advances the screen by one thing a person did.
///
/// # Why this mints ids and [`super::advance`] does not
///
/// The answer fold returns *unstamped* enquiries, because it runs when an answer arrives and has
/// no reason to know whether anything leaves. This one cannot: submitting a PIN moves the standing
/// to [`PinEntryStanding::Verifying`], which names the enquiry it is waiting for, so the id has to
/// exist before the phase can be built. Taking the minter as a parameter is what keeps that honest
/// — the alternative is a `Verifying` that holds no id and a screen that cannot tell its own
/// outstanding verification from a stale one.
///
/// # `SubmitPin` does not check the length
///
/// It submits whatever is in the buffer, and the *button* is what refuses to be pressable below
/// the policy's minimum. That looks like the check is missing; it is placed rather than missing,
/// and the placement is the point. `EnteredDigits::finish` is fallible, `AuthAnswer::PinVerified`
/// carries a bare `PinVerification` with no `Result`, and a too-short PIN therefore has nowhere to
/// be reported once it has been sent. Refusing to *draw* a live submit control is the only
/// treatment that does not end in a fabricated refusal or a lie about the platform — see
/// `AuthEnquiry::VerifyPin`'s `pin` field.
///
/// So a `SubmitPin` whose digits do not form a PIN is dropped here, silently, because the only
/// way to produce one is a control that should not have existed.
pub fn apply(
    phase: AuthPhase,
    intent: Intent,
    ids: &mut EnquiryIds,
) -> (AuthPhase, Vec<DispatchedEnquiry>) {
    match (phase, intent) {
        // ------------------------------------------------------------------
        // Typing
        // ------------------------------------------------------------------
        (
            AuthPhase::PinEntry {
                session,
                operator,
                policy,
                standing: PinEntryStanding::Entering(mut digits),
            },
            keyed,
        ) => {
            match keyed {
                Intent::PressDigit(digit) => digits.push(digit),
                Intent::Backspace => digits.backspace(),
                Intent::ClearEntry => digits.clear(),
                Intent::SubmitPin => {
                    // Dropped silently when the digits do not form a PIN: the only control that
                    // could produce such a submit is one that should not have been drawn. See the
                    // note on this function.
                    if let Ok(pin) = digits.finish() {
                        let sent = PendingEnquiry::now(AuthEnquiry::VerifyPin {
                            operator: operator.id().clone(),
                            pin,
                            policy,
                        })
                        .dispatch(ids);

                        return (
                            AuthPhase::PinEntry {
                                session,
                                operator,
                                policy,
                                // `digits` is dropped here, and dropping it zeroizes it. The
                                // standing carries no buffer, so there is nothing to hand back
                                // and no affordance for a cancel to be wired to.
                                standing: PinEntryStanding::Verifying { awaiting: sent.id },
                            },
                            vec![sent],
                        );
                    }
                }
                // A retry means nothing while entering: nothing has failed.
                Intent::Retry => {}
                // Nor does choosing an operator: the list is not on screen, and the operator is
                // already fixed by the phase this entry belongs to.
                Intent::ChooseOperator(_) => {}
            }

            (
                AuthPhase::PinEntry {
                    session,
                    operator,
                    policy,
                    standing: PinEntryStanding::Entering(digits),
                },
                Vec::new(),
            )
        }

        // ------------------------------------------------------------------
        // Recovering from an outcome
        // ------------------------------------------------------------------
        //
        // A refusal that a different PIN could fix hands the pad back, empty. One that no PIN can
        // fix does not, and the notice stays on screen — `RefusalNotice::pad` already decided
        // that, so this arm reads the decision rather than making a second one.
        (
            AuthPhase::PinEntry {
                session,
                operator,
                policy,
                standing: PinEntryStanding::Refused(notice),
            },
            Intent::Retry,
        ) => {
            let standing = if notice.pad().a_different_pin_could_help() {
                PinEntryStanding::Entering(pos_models::EnteredDigits::empty())
            } else {
                PinEntryStanding::Refused(notice)
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

        // An undecided outcome is not a refusal and never spent an attempt, so the pad always
        // comes back — but only when retrying is worth it. `Recheck::Futile` names the cases where
        // trying the same thing again cannot produce a different answer.
        (
            AuthPhase::PinEntry {
                session,
                operator,
                policy,
                standing: PinEntryStanding::Undecided(notice),
            },
            Intent::Retry,
        ) => {
            let standing = if matches!(notice.recheck(), Recheck::WorthRetrying) {
                PinEntryStanding::Entering(pos_models::EnteredDigits::empty())
            } else {
                PinEntryStanding::Undecided(notice)
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

        // ------------------------------------------------------------------
        // Starting over
        // ------------------------------------------------------------------
        //
        // The start-up probe failed and the person asked for another go. Only a cause worth
        // rechecking gets one; a `Futile` one would spin behind an unchanging screen, which is the
        // silent retry loop `Stalled` exists to make visible rather than to reproduce.
        (AuthPhase::Stalled(notice), Intent::Retry) => {
            if matches!(notice.recheck(), Recheck::WorthRetrying) {
                (
                    AuthPhase::Splash,
                    vec![PendingEnquiry::now(AuthEnquiry::RestoreSession).dispatch(ids)],
                )
            } else {
                (AuthPhase::Stalled(notice), Vec::new())
            }
        }

        // A pairing screen whose code could not be fetched asks for another.
        (
            AuthPhase::Pairing {
                code,
                expires_at,
                enrolment,
                poll_in_flight,
            },
            Intent::Retry,
        ) => (
            AuthPhase::Pairing {
                code,
                expires_at,
                enrolment,
                poll_in_flight,
            },
            vec![PendingEnquiry::now(AuthEnquiry::RequestPairingCode).dispatch(ids)],
        ),

        // ------------------------------------------------------------------
        // Choosing an operator, which cannot yet be honoured
        // ------------------------------------------------------------------
        //
        // This arm returns the phase unchanged, and that is a defect with an owner rather than a
        // decision. `AuthPhase::PinEntry` requires a `PinPolicy`; `OperatorSelect` carries none,
        // `TerminalSession` has no field for one, and `AuthService::login_terminal` never
        // assembles one — measured 2026-08-24, every call to `TerminalConfig::pin_policy` in this
        // repository is inside `pos-api`'s own test module. So there is no policy anywhere in the
        // running system to build the next phase from.
        //
        // Written as its own arm rather than left to the catch-all below so that it is visible,
        // testable, and a one-line change when the policy arrives:
        // `till/issue/pin-policy-does-not-survive-a-restart`.
        //
        // The alternative — inventing a default policy here to make the screen advance — would
        // put a fabricated attempts budget behind a real lockout, which is the one failure the
        // carried-not-fetched design exists to prevent.
        (phase @ AuthPhase::OperatorSelect { .. }, Intent::ChooseOperator(_)) => {
            (phase, Vec::new())
        }

        // ------------------------------------------------------------------
        // Everything a phase did not draw a control for
        // ------------------------------------------------------------------
        //
        // See the module header: no effect has landed, so dropping it costs nothing.
        (phase, _) => (phase, Vec::new()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use chrono::Utc;
    use pos_api::SessionToken;
    use pos_db::OperatorRow;
    use pos_models::{
        EnteredDigits, HardwareEnrolment, LockoutPeriod, MaxAttempts, OfflineWindow, OperatorId,
        OperatorName, OperatorRole, PinLength, PinPolicy, PinRefusal, RequiredPinLength,
        SessionLifetime, UndeterminedCause,
    };
    use pos_services::TerminalSession;

    use super::super::notice::{RefusalNotice, UndecidedNotice};
    use super::super::phase::OperatorCard;
    use super::super::PairingCode;

    fn policy() -> PinPolicy {
        PinPolicy::new(
            RequiredPinLength::Exactly(PinLength::Four),
            MaxAttempts::new(3).expect("a non-zero budget"),
            LockoutPeriod::from_minutes(15).expect("fifteen is not negative"),
            SessionLifetime::from_hours(8).expect("eight is positive"),
            OfflineWindow::from_hours(72).expect("seventy-two is not negative"),
        )
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

    fn card() -> OperatorCard {
        let rows = [OperatorRow {
            id: OperatorId::new("op-1").expect("a well-formed operator id"),
            code: "A1".into(),
            employee_id: None,
            employee_number: None,
            name: OperatorName::new("Sara", Some("سارة")).expect("a well-formed name"),
            role: OperatorRole::Cashier,
            department: None,
            position: None,
            permissions: None,
            is_active: true,
        }];

        OperatorCard::roster(&rows)
            .into_iter()
            .next()
            .expect("one active operator makes one card")
    }

    fn entering(digits: &[u8]) -> AuthPhase {
        let mut buffer = EnteredDigits::empty();
        for value in digits {
            buffer.push(Digit::new(*value).expect("a single decimal digit"));
        }

        AuthPhase::PinEntry {
            session: session(),
            operator: card(),
            policy: policy(),
            standing: PinEntryStanding::Entering(buffer),
        }
    }

    fn standing_of(phase: &AuthPhase) -> &PinEntryStanding {
        match phase {
            AuthPhase::PinEntry { standing, .. } => standing,
            other => panic!("expected PIN entry, got {other:?}"),
        }
    }

    fn entered_len(phase: &AuthPhase) -> usize {
        match standing_of(phase) {
            PinEntryStanding::Entering(digits) => digits.len(),
            other => panic!("expected an entering standing, got {other:?}"),
        }
    }

    #[test]
    fn sign_in_a_digit_lands_in_the_buffer_and_nothing_is_sent() {
        let mut ids = EnquiryIds::new();
        let (phase, sent) = apply(
            entering(&[1, 2]),
            Intent::PressDigit(Digit::new(3).expect("a single decimal digit")),
            &mut ids,
        );

        assert_eq!(entered_len(&phase), 3);
        assert!(sent.is_empty(), "typing a digit asks the platform nothing");
    }

    #[test]
    fn sign_in_backspace_and_clear_shorten_the_buffer() {
        let mut ids = EnquiryIds::new();

        let (phase, _) = apply(entering(&[1, 2, 3]), Intent::Backspace, &mut ids);
        assert_eq!(entered_len(&phase), 2);

        let (phase, _) = apply(entering(&[1, 2, 3]), Intent::ClearEntry, &mut ids);
        assert_eq!(entered_len(&phase), 0);
    }

    /// The submit path, and the invariant that makes `Verifying` mean something: the standing must
    /// name the enquiry that was actually sent, or a stale answer cannot be told from a live one.
    #[test]
    fn sign_in_a_submitted_pin_names_the_enquiry_it_is_waiting_for() {
        let mut ids = EnquiryIds::new();
        let (phase, sent) = apply(entering(&[1, 2, 3, 4]), Intent::SubmitPin, &mut ids);

        assert_eq!(sent.len(), 1, "one submit sends exactly one verification");

        match standing_of(&phase) {
            PinEntryStanding::Verifying { awaiting } => {
                assert_eq!(
                    *awaiting, sent[0].id,
                    "the standing must name the enquiry that left, not a fresh id"
                );
            }
            other => panic!("a submitted PIN must be verifying, got {other:?}"),
        }
    }

    /// The silent drop, asserted rather than described. A control that could produce this should
    /// not have been drawn — but if one is, nothing is sent and no attempt is spent.
    #[test]
    fn sign_in_a_submit_too_short_to_form_a_pin_sends_nothing() {
        let mut ids = EnquiryIds::new();
        let (phase, sent) = apply(entering(&[1, 2]), Intent::SubmitPin, &mut ids);

        assert!(
            sent.is_empty(),
            "two digits do not form a PIN, so nothing may reach the platform"
        );
        assert_eq!(
            entered_len(&phase),
            2,
            "and the buffer is left exactly as it was"
        );
    }

    /// A refusal a different PIN could fix hands the pad back empty; one that no PIN can fix does
    /// not. Both directions, because a predicate that always said `true` would pass the first.
    #[test]
    fn sign_in_only_a_refusal_a_different_pin_could_fix_hands_the_pad_back() {
        let mut ids = EnquiryIds::new();

        let retryable = RefusalNotice::for_refusal(PinRefusal::WrongPin {
            attempts_remaining: pos_models::AttemptsRemaining::new(2)
                .expect("two remaining attempts is a representable count"),
        });
        assert!(retryable.pad().a_different_pin_could_help());

        let (phase, _) = apply(
            AuthPhase::PinEntry {
                session: session(),
                operator: card(),
                policy: policy(),
                standing: PinEntryStanding::Refused(retryable),
            },
            Intent::Retry,
            &mut ids,
        );
        assert_eq!(entered_len(&phase), 0, "the pad comes back empty");

        let final_refusal = RefusalNotice::for_refusal(PinRefusal::OperatorUnknown);
        assert!(!final_refusal.pad().a_different_pin_could_help());

        let (phase, _) = apply(
            AuthPhase::PinEntry {
                session: session(),
                operator: card(),
                policy: policy(),
                standing: PinEntryStanding::Refused(final_refusal),
            },
            Intent::Retry,
            &mut ids,
        );
        assert!(
            matches!(standing_of(&phase), PinEntryStanding::Refused(_)),
            "a refusal no PIN can fix does not offer the pad back"
        );
    }

    /// An undecided outcome never spent an attempt, so it gets the pad back — unless rechecking is
    /// futile, in which case the same request would produce the same answer.
    #[test]
    fn sign_in_an_undecided_outcome_returns_the_pad_only_when_a_recheck_could_differ() {
        let mut ids = EnquiryIds::new();

        let worth = UndecidedNotice::for_cause(&UndeterminedCause::ServerUnreachable);
        assert!(matches!(worth.recheck(), Recheck::WorthRetrying));

        let (phase, _) = apply(
            AuthPhase::PinEntry {
                session: session(),
                operator: card(),
                policy: policy(),
                standing: PinEntryStanding::Undecided(worth),
            },
            Intent::Retry,
            &mut ids,
        );
        assert_eq!(entered_len(&phase), 0);

        let futile = UndecidedNotice::for_cause(&UndeterminedCause::TerminalNotProvisioned);
        assert!(matches!(futile.recheck(), Recheck::Futile));

        let (phase, _) = apply(
            AuthPhase::PinEntry {
                session: session(),
                operator: card(),
                policy: policy(),
                standing: PinEntryStanding::Undecided(futile),
            },
            Intent::Retry,
            &mut ids,
        );
        assert!(matches!(
            standing_of(&phase),
            PinEntryStanding::Undecided(_)
        ));
    }

    /// A stalled start-up probe retries by asking the same first question again.
    #[test]
    fn sign_in_retrying_a_stalled_probe_asks_the_start_up_question_again() {
        let mut ids = EnquiryIds::new();
        let notice = UndecidedNotice::for_startup_probe_failure();

        let (phase, sent) = apply(AuthPhase::Stalled(notice), Intent::Retry, &mut ids);

        if matches!(notice.recheck(), Recheck::WorthRetrying) {
            assert!(matches!(phase, AuthPhase::Splash));
            assert_eq!(sent.len(), 1);
            assert!(matches!(sent[0].asking, AuthEnquiry::RestoreSession));
        } else {
            assert!(matches!(phase, AuthPhase::Stalled(_)));
            assert!(sent.is_empty());
        }
    }

    /// The catch-all, asserted so that "does nothing" is a measured property rather than a claim.
    /// A digit cannot reach the splash screen, because no keypad is drawn there — but if one did,
    /// it must change nothing and send nothing.
    #[test]
    fn sign_in_an_intent_no_phase_drew_a_control_for_changes_nothing() {
        let mut ids = EnquiryIds::new();

        for intent in [
            Intent::PressDigit(Digit::new(1).expect("a single decimal digit")),
            Intent::Backspace,
            Intent::ClearEntry,
            Intent::SubmitPin,
        ] {
            // The label is taken before `apply` consumes the intent: `Intent` carries an
            // `OperatorId` now and is no longer `Copy`.
            let label = format!("{intent:?}");
            let (phase, sent) = apply(AuthPhase::Splash, intent, &mut ids);
            assert!(
                matches!(phase, AuthPhase::Splash),
                "{label} moved the splash screen"
            );
            assert!(
                sent.is_empty(),
                "{label} sent something from the splash screen"
            );
        }

        // The control: this same fold *does* act on a phase that draws the control, so the
        // assertions above are about reachability rather than about `apply` doing nothing at all.
        let (phase, _) = apply(entering(&[1]), Intent::Backspace, &mut ids);
        assert_eq!(entered_len(&phase), 0, "the fold is not inert");
    }

    /// **This test asserts a defect, deliberately.** Choosing an operator cannot advance the
    /// screen, because `AuthPhase::PinEntry` needs a `PinPolicy` and nothing in the running system
    /// has one: every call to `TerminalConfig::pin_policy` is inside `pos-api`'s own test module,
    /// `TerminalSession` has no field for a policy, and `login_terminal` never assembles one.
    ///
    /// When `till/issue/pin-policy-does-not-survive-a-restart` lands, this test must go red. It is
    /// written this way so that the fix cannot quietly leave the dead control behind — a screen
    /// that draws a card nobody can activate is worse than one that draws no cards, because the
    /// cashier concludes the till is broken rather than unfinished.
    #[test]
    fn sign_in_choosing_an_operator_cannot_yet_advance_the_screen() {
        let mut ids = EnquiryIds::new();
        let card = card();
        let chosen = card.id().clone();

        let (phase, sent) = apply(
            AuthPhase::OperatorSelect {
                session: session(),
                operators: vec![card],
            },
            Intent::ChooseOperator(chosen),
            &mut ids,
        );

        assert!(
            matches!(phase, AuthPhase::OperatorSelect { .. }),
            "if this now reaches PIN entry, the policy landed — delete this test and wire the \
             transition, do not weaken the assertion"
        );
        assert!(
            sent.is_empty(),
            "a transition that cannot happen must not send an enquiry either"
        );
    }

    /// A pairing screen's retry asks for a fresh code rather than polling the dead one.
    #[test]
    fn sign_in_retrying_a_pairing_screen_asks_for_a_new_code() {
        let mut ids = EnquiryIds::new();

        let (phase, sent) = apply(
            AuthPhase::Pairing {
                code: PairingCode::new("AAA-111"),
                expires_at: Utc::now(),
                enrolment: HardwareEnrolment::Undetermined,
                poll_in_flight: false,
            },
            Intent::Retry,
            &mut ids,
        );

        assert!(matches!(phase, AuthPhase::Pairing { .. }));
        assert_eq!(sent.len(), 1);
        assert!(matches!(sent[0].asking, AuthEnquiry::RequestPairingCode));
    }
}
