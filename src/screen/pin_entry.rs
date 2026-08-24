//! PIN entry, and the four states it can be in.
//!
//! # The requirement this file exists to satisfy
//!
//! A refused PIN and an undecided one must not read alike. A till that cannot reach the platform
//! has not judged anybody's PIN — saying "wrong PIN" there is false, and it reads as an attempt
//! spent against a budget the till does not keep. The two notices share no type, so no function
//! here can render them with the same element even by accident; this file's job is to not
//! reintroduce by hand what the types already forbid.

use abdu_egui_ui::accessibility::LastResortLandmark;
use abdu_egui_ui::enums::{ButtonVariant, PinStatus, Tone, TypeRole};
use abdu_egui_ui::{
    AccessibleName, Button, Label, Locale, NumericKeypad, PinDisplay, PinKeypadEvent, Spinner,
    Token,
};
use pos_models::{AttemptsRemaining, EnteredDigits, PinLength, PinPolicy, RequiredPinLength};

use super::Reading;
use crate::ui::sign_in::{
    strings, Intent, OperatorCard, PadOffer, Recheck, RefusalNotice, UndecidedNotice,
};

/// The id the keypad keeps its own state under.
const KEYPAD_ID: &str = "sign-in-pin-keypad";

/// Draws PIN entry for whichever standing it is in.
pub fn render(
    ui: &mut egui::Ui,
    operator: &OperatorCard,
    policy: PinPolicy,
    standing: &crate::ui::sign_in::PinEntryStanding,
    reading: Reading,
) -> Vec<Intent> {
    use crate::ui::sign_in::PinEntryStanding as Standing;

    ui.add(Label::new(reading.of(strings::ENTER_YOUR_PIN)).role(TypeRole::TitleLg));
    ui.add(
        Label::new(
            operator
                .name()
                .in_script(super::operators::script_for(reading)),
        )
        .role(TypeRole::BodyLg)
        .tone(Tone::Muted),
    );

    match standing {
        Standing::Entering(digits) => render_pad(ui, digits, policy, PinStatus::Normal, reading),
        Standing::Verifying { .. } => {
            render_verifying(ui, reading);
            Vec::new()
        }
        Standing::Refused(notice) => render_refusal(ui, *notice, reading),
        Standing::Undecided(notice) => render_undecided(ui, *notice, reading),
    }
}

/// How many dots the display shows, and how many digits the submit needs.
///
/// Both arms are written even though nothing produces `Exactly` until the platform read lands.
/// Writing only the reachable one is how a screen becomes wrong on the day a tenant first
/// configures a length — the arm would be added under time pressure, by someone reading the
/// display code rather than the policy.
const fn capacity_and_minimum(policy: PinPolicy) -> (PinLength, PinLength) {
    match policy.length() {
        RequiredPinLength::Exactly(length) => (length, length),
        // The platform accepts any length in range, so the display offers the longest and the
        // submit opens at the shortest.
        RequiredPinLength::AnyPlatformLength => (PinLength::LONGEST, PinLength::SHORTEST),
    }
}

/// The dots, the keypad, and the one deliberate way out.
fn render_pad(
    ui: &mut egui::Ui,
    digits: &EnteredDigits,
    policy: PinPolicy,
    status: PinStatus,
    reading: Reading,
) -> Vec<Intent> {
    let mut intents = Vec::new();
    let (capacity, minimum) = capacity_and_minimum(policy);

    // `length` is capacity and `filled` is how much of it is used — independent, so the row of
    // dots does not grow as somebody types and leak the length to anyone watching the screen.
    let display = PinDisplay::new(pin_display_name(ui), capacity.digits()).filled(digits.len());
    ui.add(display.status(status));

    let at_capacity = digits.len() >= capacity.digits();
    let response = NumericKeypad::pin(KEYPAD_ID)
        // Stops the pad accepting a digit that would overflow the buffer, rather than letting the
        // press land and be silently dropped.
        .at_capacity(at_capacity)
        .show(ui);

    match response.event {
        // Two different `Digit` newtypes meet here: the keypad's, which is a fact about a key that
        // was pressed, and the domain's, which is a fact about a decimal digit. Converting through
        // `value()` is deliberate rather than an inconvenience — the alternative is one crate
        // depending on the other's numeric type, and the whole point of `src/ui` holding no
        // toolkit is that the domain does not know a keypad exists. Neither can fail: both are
        // 0-9 by construction, and the `None` arm is unreachable rather than defensive.
        PinKeypadEvent::Digit(pressed) => {
            if let Some(digit) = pos_models::Digit::new(pressed.value()) {
                intents.push(Intent::PressDigit(digit));
            }
        }
        PinKeypadEvent::Backspace => intents.push(Intent::Backspace),
        PinKeypadEvent::Clear => intents.push(Intent::ClearEntry),
        PinKeypadEvent::Idle => {}
    }

    // The PIN keypad has no enter key — unlike its point-of-sale sibling — so submitting is always
    // an explicit act on this button and never a side effect of reaching a length.
    //
    // This `enabled` is where the length rule is enforced. `apply` deliberately does not check:
    // `EnteredDigits::finish` is fallible and `AuthAnswer::PinVerified` has no `Result` to report
    // a malformed PIN in, so the only treatment that does not end in a fabricated refusal is to
    // refuse to draw a live control.
    let long_enough = digits.len() >= minimum.digits();
    if ui
        .add(
            Button::new(reading.of(strings::SIGN_IN))
                .variant(ButtonVariant::Default)
                .enabled(long_enough),
        )
        .clicked()
    {
        intents.push(Intent::SubmitPin);
    }

    intents
}

/// A verification is in flight.
///
/// Progress and no way back. There is deliberately no cancel: by the time an answer exists the
/// operator session has already been written and presented, so a cancel could only cancel the
/// screen's interest in it and would leave the till signed in behind a sign-in screen. The wait is
/// bounded by the API client's own timeout, which arrives here as an undecided outcome.
fn render_verifying(ui: &mut egui::Ui, reading: Reading) {
    ui.add(Spinner::new());
    ui.add(Label::new(reading.of(strings::CHECKING)).role(TypeRole::BodyLg));
}

/// The PIN was refused.
///
/// Whether the pad comes back is [`RefusalNotice`]'s decision, already made, and this reads it
/// rather than making a second one. The attempts count is rendered from an [`AttemptsRemaining`],
/// which only a wrong PIN can produce — so no other refusal can grow a count, and an undecided
/// outcome cannot reach this function at all.
fn render_refusal(ui: &mut egui::Ui, notice: RefusalNotice, reading: Reading) -> Vec<Intent> {
    ui.add(
        Label::new(reading.of(notice.sentence()))
            .role(TypeRole::BodyLg)
            .tone(Tone::Destructive)
            .wrap(),
    );

    match notice.pad() {
        PadOffer::AtCost { attempts_remaining } => {
            render_attempts(ui, attempts_remaining, reading);
        }
        // A rotation is a dead end at this till: there is no rotate-PIN call in the client, so the
        // honest deliverable is naming where it can be done, not a flow that cannot complete.
        PadOffer::FreeOfCharge { required_length } => {
            ui.add(
                Label::new(format!(
                    "{} — {}",
                    reading.of(strings::CREDENTIAL_REQUIRES_ROTATION),
                    required_length.digits()
                ))
                .role(TypeRole::BodyMd)
                .wrap(),
            );
        }
        PadOffer::Withheld => {}
    }

    let mut intents = Vec::new();
    if notice.pad().a_different_pin_could_help()
        && ui
            .add(Button::new(reading.of(strings::TRY_AGAIN)))
            .clicked()
    {
        intents.push(Intent::Retry);
    }

    intents
}

/// The till could not find out.
///
/// **Its own words, and never a wrong-PIN message or an attempts count.** Nothing was judged, so
/// nothing was spent. The tone is muted rather than destructive: a network that is down is not the
/// cashier's mistake, and colouring it like one teaches them to distrust the screen.
fn render_undecided(ui: &mut egui::Ui, notice: UndecidedNotice, reading: Reading) -> Vec<Intent> {
    ui.add(
        Label::new(reading.of(notice.sentence()))
            .role(TypeRole::BodyLg)
            .tone(Tone::Muted)
            .wrap(),
    );

    let mut intents = Vec::new();
    if matches!(notice.recheck(), Recheck::WorthRetrying)
        && ui
            .add(Button::new(reading.of(strings::TRY_AGAIN)))
            .clicked()
    {
        intents.push(Intent::Retry);
    }

    intents
}

/// How many tries are left.
///
/// Takes an [`AttemptsRemaining`] rather than a number, which is what makes it unreachable from
/// any outcome but a wrong PIN.
fn render_attempts(ui: &mut egui::Ui, remaining: AttemptsRemaining, reading: Reading) {
    ui.add(
        Label::new(format!(
            "{} {}",
            remaining.get(),
            reading.of(strings::ATTEMPTS_REMAINING)
        ))
        .role(TypeRole::BodyMd)
        .tone(Tone::Warning),
    );
}

/// The dot row's accessible name.
///
/// The widget is display-only and has no text to derive one from, so it must be supplied.
///
/// `new` is fallible because a name conveying nothing is refused. The literal below cannot be one,
/// but `.expect` would still be a panic path in a till for a case that cannot happen — and a
/// panic that cannot happen is exactly the kind that does. `last_resort` is the library's own
/// total constructor for this position, and it names the landmark in the reader's locale, which is
/// a better failure than anything this file could invent.
fn pin_display_name(ui: &egui::Ui) -> AccessibleName {
    AccessibleName::new("PIN").unwrap_or_else(|_| {
        AccessibleName::last_resort(LastResortLandmark::KeypadPin, &Locale::get(ui.ctx()))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy_of(length: RequiredPinLength) -> PinPolicy {
        PinPolicy::new(
            length,
            pos_models::MaxAttempts::new(3).expect("a non-zero budget"),
            pos_models::LockoutPeriod::from_minutes(15).expect("fifteen is not negative"),
            pos_models::SessionLifetime::from_hours(8).expect("eight is positive"),
            pos_models::OfflineWindow::from_hours(72).expect("seventy-two is not negative"),
        )
    }

    /// Both policy arms, including the one nothing produces yet. `Exactly` pins the display to the
    /// configured length and opens submit at exactly that; `AnyPlatformLength` offers the longest
    /// and opens at the shortest.
    #[test]
    fn screen_the_policy_decides_both_the_dot_count_and_when_submit_opens() {
        let (capacity, minimum) =
            capacity_and_minimum(policy_of(RequiredPinLength::AnyPlatformLength));
        assert_eq!(capacity, PinLength::LONGEST);
        assert_eq!(minimum, PinLength::SHORTEST);
        assert!(
            capacity.digits() > minimum.digits(),
            "the open-length case must leave room between the two, or this test proves nothing"
        );

        for exact in [PinLength::Four, PinLength::Five, PinLength::Six] {
            let (capacity, minimum) =
                capacity_and_minimum(policy_of(RequiredPinLength::Exactly(exact)));
            assert_eq!(capacity, exact, "the dots must match the configured length");
            assert_eq!(
                minimum, exact,
                "a fixed length must not accept a shorter PIN than it configures"
            );
        }
    }

    /// The accessible name must survive whatever the fallback chain does, because the widget
    /// cannot derive one and an empty name announces nothing.
    #[test]
    fn screen_the_pin_display_always_has_something_to_announce() {
        let name = AccessibleName::new("PIN").expect("a non-blank literal is a usable name");
        assert!(
            !name.as_str().trim().is_empty(),
            "a display-only widget with a blank accessible name is silent to a screen reader"
        );

        // The control: the constructor really does refuse a name that conveys nothing, so the
        // assertion above is about this literal rather than about a constructor that accepts
        // anything.
        assert!(
            AccessibleName::new("   ").is_err(),
            "if blanks were accepted, the fallback in `pin_display_name` would be unreachable \
             and the assertion above would prove nothing"
        );
    }
}
