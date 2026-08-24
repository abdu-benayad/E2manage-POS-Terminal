//! The pairing screen: a code somebody must approve elsewhere.

use abdu_egui_ui::enums::{Tone, TypeRole};
use abdu_egui_ui::{Button, Label};
use chrono::{DateTime, Utc};
use pos_models::HardwareEnrolment;

use super::{enrolment_sentence, render_code, Reading};
use crate::ui::sign_in::{strings, Intent, PairingCode};

/// How the expiry is written. Wall-clock to the minute: a cashier reading this needs to know
/// whether the code is still good, not to the second, and seconds on screen invite a stopwatch.
const EXPIRY_FORMAT: &str = "%H:%M";

/// Draws the pairing phase.
pub fn render(
    ui: &mut egui::Ui,
    code: &PairingCode,
    expires_at: DateTime<Utc>,
    enrolment: HardwareEnrolment,
    reading: Reading,
) -> Vec<Intent> {
    let mut intents = Vec::new();

    ui.add(Label::new(reading.of(strings::AWAITING_APPROVAL)).role(TypeRole::BodyLg));

    render_code(ui, code.as_str());

    ui.add(
        Label::new(format!(
            "{} {}",
            reading.of(strings::CODE_EXPIRES_AT),
            expires_at.format(EXPIRY_FORMAT)
        ))
        .role(TypeRole::BodySm)
        .tone(Tone::Muted),
    );

    render_enrolment(ui, enrolment, reading);

    // A fresh code is always a legitimate request — the one on screen may have expired while
    // nobody was looking, and the fold turns this into `RequestPairingCode`.
    if ui
        .add(Button::new(reading.of(strings::TRY_AGAIN)))
        .clicked()
    {
        intents.push(Intent::Retry);
    }

    intents
}

/// The enrolment sentence, in the tone its content earns.
///
/// `AlreadyEnrolled` is [`Tone::Destructive`] because approving the code archives a working till;
/// `NotEnrolled` is ordinary body copy because a first enrolment destroys nothing and must not
/// borrow the warning's weight. `Undetermined` draws **nothing at all** — see
/// [`enrolment_sentence`], which is where that decision lives so this function cannot quietly
/// make a different one.
fn render_enrolment(ui: &mut egui::Ui, enrolment: HardwareEnrolment, reading: Reading) {
    let Some(sentence) = enrolment_sentence(enrolment) else {
        return;
    };

    let tone = match enrolment {
        HardwareEnrolment::AlreadyEnrolled => Tone::Destructive,
        HardwareEnrolment::NotEnrolled => Tone::Default,
        // Unreachable: `enrolment_sentence` returned `None` above and this function has already
        // left. Written as its own arm rather than folded in, so a fourth state still fails to
        // compile here as well as there.
        HardwareEnrolment::Undetermined => Tone::Default,
    };

    ui.add(
        Label::new(reading.of(sentence))
            .role(TypeRole::BodyLg)
            .tone(tone)
            .wrap(),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The correctness requirement of this screen, asserted without a renderer.
    ///
    /// `Undetermined` must produce no sentence at all — not the reassuring `NotEnrolled` one,
    /// which is `unwrap_or(false)` spelled in pixels, and not a spinner standing in for an answer
    /// nobody has given.
    #[test]
    fn screen_undetermined_enrolment_says_nothing_about_enrolment() {
        assert!(
            enrolment_sentence(HardwareEnrolment::Undetermined).is_none(),
            "an unknown enrolment must not borrow either of the other two sentences"
        );
    }

    /// The control for the assertion above: the other two states *do* produce sentences, so
    /// `None` above is a decision rather than a function that returns `None` for everything.
    #[test]
    fn screen_the_two_known_enrolment_states_each_have_their_own_words() {
        let enrolled = enrolment_sentence(HardwareEnrolment::AlreadyEnrolled)
            .expect("an already-enrolled device must warn");
        let fresh = enrolment_sentence(HardwareEnrolment::NotEnrolled)
            .expect("a first enrolment must say so");

        assert_ne!(
            enrolled.english(),
            fresh.english(),
            "the destructive case and the ordinary one must not read alike in English"
        );
        assert_ne!(
            enrolled.arabic(),
            fresh.arabic(),
            "nor in Arabic — a translation that collapsed them would lose the warning"
        );
    }

    /// The warning has to actually warn. A destructive act needs a word that says so in both
    /// scripts; a sentence that merely stated the fact would read as informational.
    #[test]
    fn screen_the_destructive_enrolment_sentence_carries_a_warning_word() {
        let warning = enrolment_sentence(HardwareEnrolment::AlreadyEnrolled)
            .expect("an already-enrolled device must warn");

        assert!(
            warning.english().to_lowercase().contains("warning"),
            "the English warning does not name itself as one: {}",
            warning.english()
        );
        assert!(
            warning.arabic().contains("تحذير"),
            "the Arabic warning does not name itself as one: {}",
            warning.arabic()
        );
    }
}
