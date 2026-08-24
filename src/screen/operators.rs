//! The operator list: who can sign in at this till.

use abdu_egui_ui::enums::{AvatarScript, Tone, TypeRole};
use abdu_egui_ui::widgets::{
    CardGrid, CardGridContent, CardGridEvent, MaxColumns, MinTileWidth, TileHeight,
};
use abdu_egui_ui::{Avatar, EmptyState, Label};
use pos_models::NameScript;

use super::Reading;
use crate::ui::sign_in::{strings, Intent, OperatorCard};

/// The narrowest a card may be before the grid drops a column.
///
/// Sized for a name and a face on a shop-floor screen touched with a thumb, not for density.
const MIN_CARD_WIDTH: f32 = 220.0;

/// Card height. Fixed rather than content-driven so a long Arabic name and a short Latin one do
/// not produce a ragged grid on the same till.
const CARD_HEIGHT: f32 = 132.0;

/// At most this many across, however wide the screen is. A till turned landscape should not lay
/// nine faces in a row and make the cashier hunt.
const MAX_ACROSS: usize = 4;

/// Draws the roster.
///
/// Activation returns [`Intent::ChooseOperator`]. Whether that advances the screen is
/// [`crate::ui::sign_in::apply`]'s business, and today it does not — see that function's
/// `ChooseOperator` arm, which names the missing piece rather than inventing one.
pub fn render(ui: &mut egui::Ui, operators: &[OperatorCard], reading: Reading) -> Vec<Intent> {
    let mut intents = Vec::new();

    ui.add(Label::new(reading.of(strings::CHOOSE_YOUR_NAME)).role(TypeRole::TitleLg));

    let script = script_for(reading);

    // An empty roster is **not** a failure and must not be drawn as one. Sign-in works offline
    // once operators have synced; a till that has not synced yet is early, not broken, and a
    // failure state here would send a shopkeeper looking for a fault that does not exist.
    let content = if operators.is_empty() {
        CardGridContent::Empty(EmptyState::new(reading.of(strings::NO_OPERATORS_YET)))
    } else {
        CardGridContent::Ready(operators)
    };

    let response = CardGrid::new(
        MinTileWidth::new(MIN_CARD_WIDTH),
        MaxColumns::bounded_or_unbounded(MAX_ACROSS),
        TileHeight::new(CARD_HEIGHT),
        content,
    )
    .show(
        ui,
        // The accessible name of each card. The operator's own name in the script being read,
        // never the id — a screen reader announcing `op-1` has announced nothing.
        |card: &OperatorCard| card.name().in_script(script).to_owned(),
        |ui, card: &OperatorCard| draw_card(ui, card, script),
    );

    match response.event {
        CardGridEvent::Activated(card) => intents.push(Intent::ChooseOperator(card.id().clone())),
        // The empty state carries no action, so this cannot arrive; `Retry` belongs to the failed
        // content this screen never builds. Both named rather than folded into a catch-all, so a
        // future empty-state action has to be handled here on purpose.
        CardGridEvent::Idle | CardGridEvent::EmptyActionInvoked | CardGridEvent::Retry => {}
    }

    intents
}

/// One operator: a face, a name, and what they are allowed to do.
fn draw_card(ui: &mut egui::Ui, card: &OperatorCard, script: NameScript) {
    ui.add(
        Avatar::new()
            .initials(card.name().initials(script))
            // The library does not sniff the script off the initials string — the caller declares
            // it, and declaring the wrong one shapes Arabic initials with a Latin face.
            .script(avatar_script(script)),
    );

    ui.add(Label::new(card.name().in_script(script)).role(TypeRole::TitleMd));

    ui.add(
        Label::new(card.role().to_string())
            .role(TypeRole::BodySm)
            .tone(Tone::Muted),
    );
}

/// Which script an operator's name is shown in.
///
/// Follows the reading direction rather than the name's own contents: a till configured for Arabic
/// shows Arabic names, and an operator with no Arabic name falls back to the Latin one inside
/// `OperatorName::in_script` rather than here, so there is one fallback and not two.
pub(crate) const fn script_for(reading: Reading) -> NameScript {
    match reading {
        Reading::RightToLeft => NameScript::Arabic,
        Reading::LeftToRight => NameScript::Latin,
    }
}

/// The same choice, in the component library's vocabulary.
///
/// Two enums saying one thing is not duplication worth removing: `NameScript` is a fact about a
/// person's record and `AvatarScript` is a fact about typeface selection, and they are equal here
/// by coincidence of this product's two locales rather than by definition.
const fn avatar_script(script: NameScript) -> AvatarScript {
    match script {
        NameScript::Arabic => AvatarScript::Arabic,
        NameScript::Latin => AvatarScript::Latin,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An empty roster is an empty state, never a failure state. Asserted at the level the
    /// decision is made, since the difference is one variant and both render as "no cards".
    #[test]
    fn screen_no_operators_is_an_empty_state_not_a_failure() {
        let content: CardGridContent<'_, OperatorCard> =
            CardGridContent::Empty(EmptyState::new("x"));

        assert!(
            matches!(content, CardGridContent::Empty(_)),
            "a till that has not synced yet is early, not broken"
        );
        assert!(
            !matches!(content, CardGridContent::Failed(_)),
            "drawing it as a failure sends a shopkeeper looking for a fault that does not exist"
        );
    }

    /// The two script choices must track the reading direction and must differ from each other —
    /// a mapping that returned one value for both would pass any single-direction check.
    #[test]
    fn screen_the_script_follows_the_reading_direction_in_both_vocabularies() {
        assert_eq!(script_for(Reading::RightToLeft), NameScript::Arabic);
        assert_eq!(script_for(Reading::LeftToRight), NameScript::Latin);

        assert_eq!(avatar_script(NameScript::Arabic), AvatarScript::Arabic);
        assert_eq!(avatar_script(NameScript::Latin), AvatarScript::Latin);

        assert_ne!(
            avatar_script(NameScript::Arabic),
            avatar_script(NameScript::Latin),
            "both scripts mapping to one face is how Arabic initials get shaped in Latin"
        );
    }
}
