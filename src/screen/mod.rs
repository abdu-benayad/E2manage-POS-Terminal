//! Drawing the sign-in screen.
//!
//! This is the only part of the till that knows a toolkit exists. [`crate::ui`] holds the
//! view-*models* and must never acquire a rendering dependency; this module imports those and
//! draws them, never the reverse.
//!
//! # Rendering reads, it does not decide
//!
//! Every function here takes `&AuthPhase` and returns [`Intent`]s. Nothing mutates the phase, and
//! nothing calls a service. What a click *means* is [`crate::ui::sign_in::apply`]'s decision, and
//! what an answer means is `advance`'s. A screen that decided anything for itself would be a third
//! place where the sign-in rules live, and the two folds exist precisely so there is not one.
//!
//! # Why it returns a list rather than one intent
//!
//! A frame can produce more than one — a keypad press and a submit click are separate widgets and
//! egui reports both in the same pass. Returning the list and folding each in order is the only
//! treatment that does not silently drop one.

use abdu_egui_ui::enums::{ScaleStep, TextOverflow, Tone, TypeRole};
use abdu_egui_ui::tokens::EguiCoherence;
use abdu_egui_ui::{Button, Code, Environment, Label, Locale, Spinner};
use pos_models::HardwareEnrolment;

use crate::ui::sign_in::{AuthPhase, Intent, Sentence};

mod operators;
mod pairing;
mod pin_entry;

/// Which script the screen is reading in.
///
/// A newtype over the direction rather than a bare `bool` at every call site: `render(ui, phase,
/// true)` says nothing about what `true` meant, and the two readings are mirror images of each
/// other, so a flipped argument is invisible until somebody looks at the screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reading {
    /// Left to right — English and the rest of the Latin-script locales.
    LeftToRight,
    /// Right to left — Arabic, the till's default.
    RightToLeft,
}

impl Reading {
    /// Whether this is right-to-left, for the library calls that take a `bool`.
    pub const fn is_rtl(self) -> bool {
        matches!(self, Self::RightToLeft)
    }

    /// Which of a sentence's two scripts to show.
    ///
    /// Every [`Sentence`] carries both, so this cannot fail to find one and there is no fallback
    /// to a language nobody in the shop reads.
    pub const fn of(self, sentence: Sentence) -> &'static str {
        match self {
            Self::RightToLeft => sentence.arabic(),
            Self::LeftToRight => sentence.english(),
        }
    }
}

/// Draws whichever phase the screen is on.
///
/// Exhaustive with no catch-all, for the reason every other match in this issue is: a new phase
/// must fail to compile here rather than fall through to a blank screen, which is the one failure
/// a cashier cannot report usefully.
pub fn render(ui: &mut egui::Ui, phase: &AuthPhase, reading: Reading) -> Vec<Intent> {
    ui.with_layout(abdu_egui_ui::reading_column(reading.is_rtl()), |ui| {
        match phase {
            AuthPhase::Splash => {
                render_splash(ui);
                Vec::new()
            }

            AuthPhase::Stalled(notice) => render_stalled(ui, *notice, reading),

            AuthPhase::Pairing {
                code,
                expires_at,
                enrolment,
                ..
            } => pairing::render(ui, code, *expires_at, *enrolment, reading),

            AuthPhase::OperatorSelect { operators, .. } => {
                operators::render(ui, operators, reading)
            }

            AuthPhase::PinEntry {
                operator,
                policy,
                standing,
                ..
            } => pin_entry::render(ui, operator, *policy, standing, reading),

            // The screen is done and the till is signed in. Nothing to draw: whatever comes after
            // sign-in is a different screen and not this issue's. A spinner rather than a blank
            // panel, because the frame between this and the next screen should not look like a
            // crash.
            AuthPhase::SignedIn(_) => {
                render_splash(ui);
                Vec::new()
            }
        }
    })
    .inner
}

/// The start-up probe, still running.
///
/// # The spinner is the harness trap, and it is deliberate
///
/// [`Spinner`] calls `request_repaint` every frame while it is running, so a test that drives this
/// screen with `egui_kittest`'s run-to-settle call never settles and panics. Step 14's tests use
/// the step-per-frame call instead. Swapping in a still image to make the harness easier would
/// trade a real affordance — the only thing telling a cashier the till has not frozen — for a
/// convenience in a test, so the harness accommodates the screen rather than the other way round.
fn render_splash(ui: &mut egui::Ui) {
    ui.centered_and_justified(|ui| {
        ui.add(Spinner::new());
    });
}

/// The start-up probe could not answer.
///
/// Offers a retry only when one could produce a different answer. A `Futile` cause with a retry
/// button is an invitation to a loop that cannot terminate, which is the failure `Stalled` exists
/// to surface rather than to reproduce.
fn render_stalled(
    ui: &mut egui::Ui,
    notice: crate::ui::sign_in::UndecidedNotice,
    reading: Reading,
) -> Vec<Intent> {
    let mut intents = Vec::new();

    ui.add(Label::new(reading.of(notice.sentence())).role(TypeRole::BodyLg));

    if matches!(notice.recheck(), crate::ui::sign_in::Recheck::WorthRetrying)
        && ui
            .add(Button::new(
                reading.of(crate::ui::sign_in::strings::TRY_AGAIN),
            ))
            .clicked()
    {
        intents.push(Intent::Retry);
    }

    intents
}

/// Whether approving a pairing code destroys a working terminal.
///
/// Three sentences for three states, and the third says nothing about enrolment at all. Rendering
/// `Undetermined` as the reassuring `NotEnrolled` sentence would be `unwrap_or(false)` spelled in
/// pixels — the exact defect [`HardwareEnrolment`] was built to make unrepresentable — and hiding
/// the phase behind a spinner until the answer arrives is the same lie told by omission.
///
/// No catch-all: a fourth state must fail to compile rather than default into a wrong sentence.
pub(crate) fn enrolment_sentence(enrolment: HardwareEnrolment) -> Option<Sentence> {
    match enrolment {
        HardwareEnrolment::AlreadyEnrolled => {
            Some(crate::ui::sign_in::strings::APPROVING_REPLACES_A_LIVE_TERMINAL)
        }
        HardwareEnrolment::NotEnrolled => Some(crate::ui::sign_in::strings::FIRST_ENROLMENT),
        // Deliberately nothing. Nobody has said yet, so the screen says nothing about it.
        HardwareEnrolment::Undetermined => None,
    }
}

/// The pairing code itself, at display size and never abbreviated.
///
/// [`Code`] defaults to [`TextOverflow::Ellipsis`], which is actively wrong here: this is a string
/// a cashier reads aloud down a phone line and types into another device, and an ellipsis silently
/// removes the part they need. It reads no `Locale`, so it is byte-identical in both directions —
/// which is correct for a code, and is why it is not run through [`Reading`].
pub(crate) fn render_code(ui: &mut egui::Ui, code: &str) {
    ui.add(
        Code::new(code)
            .overflow(TextOverflow::Clip)
            .size(ScaleStep::Display)
            .tone(Tone::Default),
    );
}

// ============================================================================
// The chrome the till runs in
// ============================================================================

/// The till's presentation, with no locale bound to it yet.
///
/// # Why `AlignSelector` is here and not left at the default
///
/// The library keeps a deliberate quarantine: `Environment::install` writes its own tokens and
/// **never** egui's [`egui::Visuals`] or theme preference, which are the host's to own. Left at the
/// default [`EguiCoherence::Manual`], egui's preference follows the operating system and *falls
/// back to dark when there is no signal* — which is what a till on an embedded box with no desktop
/// session is. The result is light library surfaces, correct in themselves, floating on egui's dark
/// panel fill, with no diagnostic anywhere.
///
/// That was shipping. It was invisible to every assertion in `tests/sign_in_both_directions.rs`,
/// because a panel's fill colour reaches no accessibility node; the first snapshot rendered showed
/// a black band behind white cards and that is how it was found.
///
/// # Why this is a function the tests call rather than a value they restate
///
/// A snapshot photographs whatever environment it is handed. Had the test built its own
/// `Environment::light()`, it would have pinned a configuration the binary does not use and gone on
/// passing while the real till was wrong — the defect above, with a green suite over it.
pub fn chrome() -> Environment {
    Environment::light().egui_coherence(EguiCoherence::AlignSelector)
}

/// The fill the window is cleared to, before a single widget is drawn.
///
/// # This is not optional, and neither test layer can see it
///
/// eframe hands [`eframe::App::ui`] a `Ui` with *no background of its own* and clears the window
/// to its own default — `rgba(12, 12, 12, 180)`, a near-black. A screen that never states a fill
/// is therefore drawn on that, and light-theme widgets over it are unreadable: measured at a
/// 1.20:1 contrast ratio for the stalled heading, against a 4.5:1 floor.
///
/// Nothing in the suite could catch it. A window fill reaches no accessibility node, so layer 1
/// is blind by construction; the snapshot references are captured over the harness's own
/// transparent background rather than over this value, so layer 2 photographs a window fill the
/// binary never had. The instrument is the test beside `clear_color` in `main.rs`, which asserts
/// the fill carries the foreground it will be drawn under.
pub fn page_fill() -> egui::Color32 {
    chrome().theme.background
}

/// The till's presentation for one locale.
pub fn environment(locale: &str) -> Environment {
    chrome().locale(Locale {
        current: locale.to_owned(),
        rtl: reads_right_to_left(locale),
        ..Locale::default()
    })
}

/// Installs [`environment`] into a context.
///
/// Idempotent by the library's own contract — each token overwrites its slot in the context — so
/// this is also the path a locale *change* takes. There is no separate replace call.
pub fn install_environment(context: &egui::Context, locale: &str) {
    environment(locale).install(context);
}

/// Whether a language tag is written right to left.
///
/// A short list rather than a library: these are the scripts this product ships into, and a wrong
/// answer here mirrors an entire screen. Matching is on the language subtag, so `ar-SA` and `ar`
/// agree.
pub fn reads_right_to_left(locale: &str) -> bool {
    let language = locale
        .split(['-', '_'])
        .next()
        .unwrap_or(locale)
        .to_ascii_lowercase();

    matches!(language.as_str(), "ar" | "he" | "fa" | "ur")
}
