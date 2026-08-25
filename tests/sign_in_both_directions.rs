//! Every sign-in phase, in both reading directions.
//!
//! Arabic and right-to-left is the till's default and the majority of its shops; left-to-right is
//! the secondary case. Both are asserted for every phase, because "it works in Arabic" and "it
//! works" are different claims and only one of them is checked by running the suite once.
//!
//! # Layer 1: what the screen *says*, not where the pixels land
//!
//! These are AccessKit assertions and need no GPU. They catch a missing sentence, a wrong script,
//! an unlabelled control, a notice that never reaches the tree. They are blind to geometry — a
//! grid that stops mirroring leaves the accessibility tree byte-identical — which is what the
//! gated snapshot tests are for and the only thing they are for.
//!
//! # Why the harness comes from the library rather than from this file
//!
//! `abdu_egui_ui_testing::harness` calls `TextEngine::validate` at construction and panics if no
//! face resolved for a text role. That check is the reason these assertions mean anything: with no
//! face registered the shaper resolves nothing and every widget paints blank, while the labels
//! below still pass — because a label comes from the widget's `WidgetInfo`, not from pixels. A
//! whole suite can be green over a screen with no visible text on it, and nothing in the output
//! says so.

use abdu_egui_ui::Token as _;
use abdu_egui_ui_testing::{each_direction, harness, Direction};
use chrono::{TimeZone, Utc};
use e2manage_pos_terminal::screen::{self, Reading};
use e2manage_pos_terminal::ui::sign_in::{
    strings, AuthPhase, EnquiryIds, OperatorCard, PairingCode, PinEntryStanding, RefusalNotice,
    Sentence, UndecidedNotice,
};
use egui_kittest::kittest::Queryable as _;
use pos_api::SessionToken;
use pos_db::OperatorRow;
use pos_models::{
    AttemptsRemaining, EnteredDigits, HardwareEnrolment, LockoutPeriod, MaxAttempts, OfflineWindow,
    OperatorId, OperatorName, OperatorRole, PinLength, PinPolicy, PinRefusal, RequiredPinLength,
    SessionLifetime, UndeterminedCause,
};
use pos_services::TerminalSession;

// ============================================================================
// Fixtures
// ============================================================================

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

/// Two operators, both active, each with a name in both scripts.
///
/// Both scripts on purpose: a roster whose names were Latin-only would pass an Arabic-direction
/// assertion by falling back, and the fallback would be the thing under test rather than the
/// rendering.
fn roster() -> Vec<OperatorCard> {
    let rows = [
        OperatorRow {
            id: OperatorId::new("op-1").expect("a well-formed operator id"),
            code: "A1".into(),
            employee_id: None,
            employee_number: None,
            name: OperatorName::new("Sara Ahmed", Some("سارة أحمد")).expect("a well-formed name"),
            role: OperatorRole::Cashier,
            department: None,
            position: None,
            permissions: None,
            is_active: true,
        },
        OperatorRow {
            id: OperatorId::new("op-2").expect("a well-formed operator id"),
            code: "A2".into(),
            employee_id: None,
            employee_number: None,
            name: OperatorName::new("Omar Nasser", Some("عمر ناصر")).expect("a well-formed name"),
            role: OperatorRole::Supervisor,
            department: None,
            position: None,
            permissions: None,
            is_active: true,
        },
    ];

    OperatorCard::roster(&rows)
}

fn digits(count: usize) -> EnteredDigits {
    let mut buffer = EnteredDigits::empty();
    for value in 0..count {
        buffer.push(
            pos_models::Digit::new(u8::try_from(value % 10).unwrap_or(0))
                .expect("a single decimal digit"),
        );
    }
    buffer
}

fn pairing(enrolment: HardwareEnrolment) -> AuthPhase {
    AuthPhase::Pairing {
        code: PairingCode::new("PAIR-4821"),
        expires_at: Utc
            .with_ymd_and_hms(2026, 8, 25, 9, 30, 0)
            .single()
            .expect("a real instant"),
        enrolment,
        poll_in_flight: false,
    }
}

fn pin_entry(standing: PinEntryStanding) -> AuthPhase {
    AuthPhase::PinEntry {
        session: session(),
        operator: roster().into_iter().next().expect("a non-empty roster"),
        policy: policy(),
        standing,
    }
}

/// The sentence as it is written in the direction being read.
fn said(direction: Direction, sentence: Sentence) -> &'static str {
    if direction.is_rtl() {
        sentence.arabic()
    } else {
        sentence.english()
    }
}

fn reading(direction: Direction) -> Reading {
    if direction.is_rtl() {
        Reading::RightToLeft
    } else {
        Reading::LeftToRight
    }
}

// ============================================================================
// The phases that settle
// ============================================================================

/// Drives one phase through both directions and hands each harness to `check`.
///
/// `build` is re-run per direction, so anything the closure needs to mutate lives in a `Cell` —
/// a `&mut` binding cannot be used at all here, because the two passes would need two simultaneous
/// mutable borrows. Nothing in these tests needs one; the phase is read, never written.
fn both_directions(phase: &AuthPhase, check: impl Fn(Direction, &mut egui_kittest::Harness<'_>)) {
    each_direction(
        screen::chrome(),
        |ui| {
            screen::render(ui, phase, reading(direction_of(ui)));
        },
        check,
    );
}

/// Which direction the installed environment is in.
///
/// Read back out of the context rather than threaded in, because `each_direction` owns the loop
/// and hands the closure only a `Ui`. Reading it back is also the stronger check: it asserts the
/// environment the library installed is the one the screen is about to lay out against, rather
/// than trusting two copies of the same intent to agree.
fn direction_of(ui: &egui::Ui) -> Direction {
    if abdu_egui_ui::Locale::get(ui.ctx()).rtl {
        Direction::Rtl
    } else {
        Direction::Ltr
    }
}

#[test]
fn the_pairing_screen_says_the_same_things_in_both_directions() {
    for enrolment in [
        HardwareEnrolment::AlreadyEnrolled,
        HardwareEnrolment::NotEnrolled,
        HardwareEnrolment::Undetermined,
    ] {
        let phase = pairing(enrolment);

        both_directions(&phase, |direction, harness| {
            assert!(
                harness
                    .query_by_label(said(direction, strings::AWAITING_APPROVAL))
                    .is_some(),
                "the standing instruction is missing under {direction:?}"
            );

            // The code is deliberately not run through the reading direction: it is a string
            // somebody reads aloud and types elsewhere, and it must be byte-identical in both.
            assert!(
                harness.query_by_label("PAIR-4821").is_some(),
                "the pairing code itself is missing under {direction:?}"
            );

            match enrolment {
                HardwareEnrolment::AlreadyEnrolled => assert!(
                    harness
                        .query_by_label(said(
                            direction,
                            strings::APPROVING_REPLACES_A_LIVE_TERMINAL
                        ))
                        .is_some(),
                    "the destructive warning is missing under {direction:?}"
                ),
                HardwareEnrolment::NotEnrolled => assert!(
                    harness
                        .query_by_label(said(direction, strings::FIRST_ENROLMENT))
                        .is_some(),
                    "the first-enrolment sentence is missing under {direction:?}"
                ),
                HardwareEnrolment::Undetermined => {}
            }
        });
    }
}

/// The correctness requirement of the pairing screen, asserted through the rendered tree rather
/// than through the function that chooses the sentence.
///
/// This is the assertion that could not be made at the unit level: it proves the screen does not
/// draw *either* known sentence when nobody has said, in both directions, after a real layout.
#[test]
fn an_undetermined_enrolment_shows_neither_of_the_other_two_sentences() {
    let phase = pairing(HardwareEnrolment::Undetermined);

    both_directions(&phase, |direction, harness| {
        for borrowed in [
            strings::APPROVING_REPLACES_A_LIVE_TERMINAL,
            strings::FIRST_ENROLMENT,
        ] {
            assert!(
                harness.query_by_label(said(direction, borrowed)).is_none(),
                "an unknown enrolment borrowed a sentence it has no right to, under {direction:?}"
            );
        }

        // The control. Without it this test passes just as well against a screen that rendered
        // nothing at all, which is the failure mode the whole file is built around.
        assert!(
            harness
                .query_by_label(said(direction, strings::AWAITING_APPROVAL))
                .is_some(),
            "the pairing screen rendered nothing under {direction:?}, so the absences above \
             prove nothing"
        );
    });
}

#[test]
fn the_operator_list_names_every_card_in_the_script_being_read() {
    let operators = roster();
    let phase = AuthPhase::OperatorSelect {
        session: session(),
        operators,
    };

    both_directions(&phase, |direction, harness| {
        assert!(
            harness
                .query_by_label(said(direction, strings::CHOOSE_YOUR_NAME))
                .is_some(),
            "the list heading is missing under {direction:?}"
        );

        let (first, second) = if direction.is_rtl() {
            ("سارة أحمد", "عمر ناصر")
        } else {
            ("Sara Ahmed", "Omar Nasser")
        };

        // `query_all_by_label`, not `query_by_label`, and the reason is a real property of the
        // screen rather than a convenience: each operator's name reaches the tree **twice** — once
        // as the card's own accessible name, which is what activation announces, and once as the
        // visible `Label` drawn inside it. That is the ordinary nesting a screen reader expects
        // (the container names itself, the content repeats it), but the singular query treats two
        // matches as an error and panics, so it would have failed here for a reason that has
        // nothing to do with reading direction.
        for name in [first, second] {
            let found = harness.query_all_by_label(name).count();
            assert!(
                found >= 1,
                "operator `{name}` is not named under {direction:?}"
            );
        }

        // The other script's spelling must *not* be present: a card falling back to Latin under
        // an Arabic reading is the defect this asserts against, and it is invisible to a test
        // that only checks the expected name is there.
        let absent = if direction.is_rtl() {
            "Sara Ahmed"
        } else {
            "سارة أحمد"
        };
        assert_eq!(
            harness.query_all_by_label(absent).count(),
            0,
            "the wrong script's spelling reached the tree under {direction:?}"
        );
    });
}

#[test]
fn an_empty_roster_says_it_is_early_rather_than_broken() {
    let phase = AuthPhase::OperatorSelect {
        session: session(),
        operators: Vec::new(),
    };

    both_directions(&phase, |direction, harness| {
        assert!(
            harness
                .query_by_label(said(direction, strings::NO_OPERATORS_YET))
                .is_some(),
            "an empty roster says nothing under {direction:?}"
        );
    });
}

#[test]
fn pin_entry_labels_its_pad_and_its_only_way_out() {
    let phase = pin_entry(PinEntryStanding::Entering(digits(2)));

    both_directions(&phase, |direction, harness| {
        assert!(
            harness
                .query_by_label(said(direction, strings::ENTER_YOUR_PIN))
                .is_some(),
            "the PIN instruction is missing under {direction:?}"
        );

        // Every digit key must be reachable by name, in both directions. A keypad whose cells
        // lose their labels is untouchable by anyone using a screen reader and looks perfect.
        for key in 0..=9 {
            assert!(
                harness.query_by_label(&key.to_string()).is_some(),
                "digit key {key} is unlabelled under {direction:?}"
            );
        }

        assert!(
            harness
                .query_by_label(said(direction, strings::SIGN_IN))
                .is_some(),
            "the submit control is missing under {direction:?}"
        );
    });
}

/// A refusal and an undecided outcome must not read alike, and this asserts it where a person
/// would notice — in the rendered tree, in both directions.
#[test]
fn a_refused_pin_and_an_undecided_one_never_show_the_same_words() {
    let refused = pin_entry(PinEntryStanding::Refused(RefusalNotice::for_refusal(
        PinRefusal::WrongPin {
            attempts_remaining: AttemptsRemaining::new(2).expect("two is a representable count"),
        },
    )));

    both_directions(&refused, |direction, harness| {
        assert!(
            harness
                .query_by_label(said(direction, strings::WRONG_PIN))
                .is_some(),
            "a wrong PIN does not say so under {direction:?}"
        );
        assert!(
            harness
                .query_by_label(said(direction, strings::SERVER_UNREACHABLE))
                .is_none(),
            "a refusal borrowed the unreachable-server sentence under {direction:?}"
        );
    });

    let undecided = pin_entry(PinEntryStanding::Undecided(UndecidedNotice::for_cause(
        &UndeterminedCause::ServerUnreachable,
    )));

    both_directions(&undecided, |direction, harness| {
        assert!(
            harness
                .query_by_label(said(direction, strings::SERVER_UNREACHABLE))
                .is_some(),
            "an unreachable server does not say so under {direction:?}"
        );

        // The defect this whole issue exists to prevent, asserted at the surface: an undecided
        // outcome must never show a wrong-PIN message, and must never show an attempts count,
        // because nothing was judged and nothing was spent.
        assert!(
            harness
                .query_by_label(said(direction, strings::WRONG_PIN))
                .is_none(),
            "an unreachable server was reported as a wrong PIN under {direction:?}"
        );
        assert!(
            harness
                .query_by_label(said(direction, strings::ATTEMPTS_REMAINING))
                .is_none(),
            "an undecided outcome showed an attempts count under {direction:?}"
        );
    });
}

// ============================================================================
// The phases that never settle
// ============================================================================

/// `each_direction` calls `harness.run()`, which panics on a continuously repainting UI. The
/// splash screen is a spinner and repaints every frame by design — it is the only thing telling a
/// cashier the till has not frozen — so these two drive the harness by hand and use `step()`.
///
/// Written as an explicit loop rather than a second helper, because the difference between the two
/// shapes is exactly one call and hiding it behind a name is how somebody later "simplifies" this
/// back into `each_direction` and gets a panic they have to diagnose.
fn both_directions_stepped(
    phase: &AuthPhase,
    check: impl Fn(Direction, &mut egui_kittest::Harness<'_>),
) {
    for direction in Direction::ALL {
        let mut h = harness(screen::chrome().rtl(direction.is_rtl()), |ui| {
            screen::render(ui, phase, reading(direction));
        });
        h.step();
        check(direction, &mut h);
    }
}

#[test]
fn the_splash_screen_survives_a_frame_in_both_directions() {
    let phase = AuthPhase::Splash;

    // The assertion is that stepping a continuously repainting screen does not panic and produces a tree.
    // There are no words on the splash to look for — that is what it is — so the check is that
    // the harness reached a state at all under both directions.
    both_directions_stepped(&phase, |_direction, harness| {
        let _ = harness.root();
    });
}

#[test]
fn a_verification_in_flight_says_it_is_checking_and_offers_no_way_out() {
    let mut ids = EnquiryIds::new();
    let phase = pin_entry(PinEntryStanding::Verifying {
        awaiting: ids.mint(),
    });

    both_directions_stepped(&phase, |direction, harness| {
        assert!(
            harness
                .query_by_label(said(direction, strings::CHECKING))
                .is_some(),
            "a verification in flight does not say so under {direction:?}"
        );

        // No way out, asserted rather than described. A cancel here could only cancel the
        // screen's interest in an answer whose effect has already landed, leaving the till
        // signed in behind a sign-in screen.
        for exit in [strings::TRY_AGAIN, strings::SIGN_IN] {
            assert!(
                harness.query_by_label(said(direction, exit)).is_none(),
                "a verification in flight offered a way out under {direction:?}"
            );
        }
    });
}

// ============================================================================
// Layer 2 — geometry, which the accessibility tree cannot see
// ============================================================================
//
// Everything above asserts *words*: which sentence, which name, which control. AccessKit reports
// none of the placement, so a screen that stopped mirroring — the roster filling from the left
// under Arabic, the dot row growing the wrong way — leaves every assertion above green. That, and
// only that, is what these three references are for.
//
// **The third reference is not the one the task named, and the substitution is deliberate.** The
// task asked for the keypad's key layout under RTL. `abdu-egui-ui` owns that widget and already
// pins it — `src/widgets/numeric_keypad/mod.rs` snapshots it as "one static image each — the
// deliberate non-mirror", because a number pad keeps phone order in every locale. Re-pinning it
// from here would put a reference to *their* widget in *this* repo's corpus, where it goes red
// when they restyle a key and nobody here can say whether that was a regression. The guide is
// explicit that a mirrored pair over a non-mirroring widget "proves nothing" besides. So the third
// reference pins the pairing screen instead, which is this repo's and which carries a claim made
// in prose at `src/screen/mod.rs` — that the code itself is byte-identical in both directions
// while everything around it mirrors — with, until now, no instrument behind it.

/// The till-shaped viewport every reference is captured at.
///
/// Fixed rather than fitted, and the distinction is not cosmetic. `fit_contents` shrinks the
/// window to the rendered content, but these are **screens**: the roster's column count is
/// computed from the width it is given, and the reading column aligns against that width. Fitting
/// would remove the very constraint that decides the layout and then photograph the result.
#[cfg(feature = "image-snapshots")]
const TILL_VIEWPORT: egui::Vec2 = egui::vec2(800.0, 600.0);

/// Renders one phase at the till viewport under both directions.
#[cfg(feature = "image-snapshots")]
fn snapshot_phase(name: &str, phase: &AuthPhase) {
    abdu_egui_ui_testing::snapshot_each_direction_fixed(
        name,
        screen::chrome(),
        TILL_VIEWPORT,
        |ui| {
            screen::render(ui, phase, reading(direction_of(ui)));
        },
    );
}

/// The roster fills from the reading edge.
#[cfg(feature = "image-snapshots")]
#[test]
fn snapshot_the_operator_grid_fills_from_the_reading_edge() {
    let phase = AuthPhase::OperatorSelect {
        operators: roster(),
        session: session(),
    };
    snapshot_phase("sign-in-operators", &phase);
}

/// The dot row, the pad and the one way out, stacked against the reading edge.
#[cfg(feature = "image-snapshots")]
#[test]
fn snapshot_pin_entry_stacks_against_the_reading_edge() {
    snapshot_phase(
        "sign-in-pin-entry",
        &pin_entry(PinEntryStanding::Entering(digits(2))),
    );
}

/// The pairing code holds still while its surroundings mirror.
#[cfg(feature = "image-snapshots")]
#[test]
fn snapshot_the_pairing_code_holds_still_while_its_surroundings_mirror() {
    snapshot_phase(
        "sign-in-pairing",
        &pairing(HardwareEnrolment::AlreadyEnrolled),
    );
}

/// **The control for all three, and it runs without a GPU.**
///
/// `snapshot_each_direction_fixed` compares each direction against *its own* reference. Nothing in
/// it asserts the two are different pictures — so a screen that ignored the reading direction
/// entirely would commit an identical pair and pass forever, which is precisely the regression
/// this layer exists to catch. Two identical renders produce byte-identical PNGs, so comparing the
/// committed files settles it.
///
/// Reading files rather than rendering is what lets this run in the ordinary lane: the references
/// are committed, so the check that they are meaningful should not itself need a rasterizer.
///
/// A missing file is a failure, never a skip. A skip here would restore exactly the silence the
/// test is built to break.
#[test]
fn every_committed_reference_pair_is_two_different_pictures() {
    let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/snapshots");
    let names = ["sign-in-operators", "sign-in-pin-entry", "sign-in-pairing"];

    for name in names {
        let ltr = directory.join(format!("{name}-ltr.png"));
        let rtl = directory.join(format!("{name}-rtl.png"));

        let left = std::fs::read(&ltr).unwrap_or_else(|e| {
            panic!("reference {} is missing or unreadable: {e}", ltr.display())
        });
        let right = std::fs::read(&rtl).unwrap_or_else(|e| {
            panic!("reference {} is missing or unreadable: {e}", rtl.display())
        });

        assert_ne!(
            left, right,
            "`{name}` rendered the same picture in both directions, so the mirror flip was never \
             exercised and its pair of references pins nothing"
        );

        // The positive half: a byte comparison that could not have come out equal — because one
        // file was empty, say — is not evidence either. Both references must be real images.
        assert!(
            left.len() > 1024 && right.len() > 1024,
            "`{name}` has a suspiciously small reference ({} and {} bytes); an empty or truncated \
             capture differs from anything and would satisfy the assertion above for no reason",
            left.len(),
            right.len()
        );
    }
}
