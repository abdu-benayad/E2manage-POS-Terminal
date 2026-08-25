//! The till's entry point.
//!
//! The workspace's only `[[bin]]`. It installs logging, opens the database, builds the services,
//! installs the component library's token environment, and hands the sign-in screen to `eframe`.
//! Rendering the phases is step 13; this file shows a spinner and drives the machine underneath.
//!
//! # The startup order is a contract, not a convenience
//!
//! Logging is installed **before** anything else runs, because the things that fail earliest — a
//! database that will not open, a data directory the platform will not name — fail before any
//! screen exists to say so, and a failure nobody records is a failure nobody can diagnose from a
//! till in a shop.

mod driver;

use std::process::ExitCode;

use driver::{data_directory, AuthDriver, StartupFailure, TillServices};
use e2manage_pos_terminal::screen::{
    self, install_environment, page_fill, reads_right_to_left, Reading,
};
use e2manage_pos_terminal::ui::sign_in::{
    advance, apply, AuthEnquiry, AuthPhase, EnquiryIds, EnquiryKind, PendingEnquiry,
};
use tracing::{info, warn};
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::EnvFilter;

/// Where the window starts. A till runs full-screen on the shop floor; this is the size it
/// occupies while somebody is developing against it.
const INITIAL_WINDOW_SIZE: [f32; 2] = [1024.0, 768.0];

/// The reading direction the till starts in.
///
/// Arabic and right-to-left, which is the documented default and the majority of this product's
/// shops. It is *not* read from `config/default.toml`: a terminal session carries a locale, and
/// once one is loaded it is the authoritative source — see [`Till::follow_session_locale`]. This
/// constant only has to cover the phases before a session exists, which are the splash and
/// pairing screens.
const DEFAULT_LOCALE: &str = "ar";

fn main() -> ExitCode {
    // Held for the whole run: dropping the guard stops the background writer, and a log file that
    // ends when startup finishes is worse than none, because it looks complete.
    let _logging = install_logging();
    info!("e2manage-pos starting");

    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            // Reported through `tracing` as well as the exit code: a till is started by a
            // launcher, and nobody is watching stderr when it fails at eight in the morning.
            tracing::error!(%error, "the till could not start");
            ExitCode::FAILURE
        }
    }
}

/// Installs the log subscriber, to the terminal and to a daily-rotated file.
///
/// `RUST_LOG` decides the level, defaulting to `info` — the same contract the rest of the
/// workspace documents. A malformed `RUST_LOG` falls back to `info` rather than refusing to start:
/// a mistyped environment variable must not be the reason a shop cannot sell.
///
/// The same fallback governs the file. If the platform names no data directory the till still
/// runs, logging to the terminal only; startup will fail a moment later for the same reason and
/// that failure is the one worth reading.
///
/// Returns the writer's guard, which the caller must keep alive.
fn install_logging() -> Option<WorkerGuard> {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let terminal = tracing_subscriber::fmt::layer();

    let Some(logs) = data_directory().map(|dir| dir.join("logs")) else {
        tracing_subscriber::registry()
            .with(filter)
            .with(terminal)
            .init();
        return None;
    };

    let (writer, guard) =
        tracing_appender::non_blocking(tracing_appender::rolling::daily(logs, "pos-terminal.log"));

    tracing_subscriber::registry()
        .with(filter)
        .with(terminal)
        // No ANSI in the file: colour codes in a log a shopkeeper emails to support are noise.
        .with(
            tracing_subscriber::fmt::layer()
                .with_ansi(false)
                .with_writer(writer),
        )
        .init();

    Some(guard)
}

/// Opens the window and runs until it closes.
fn run() -> Result<(), Box<dyn std::error::Error>> {
    // Before the window: a database that will not open should not produce a blank screen.
    let services = TillServices::open()?;

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size(INITIAL_WINDOW_SIZE),
        ..Default::default()
    };

    eframe::run_native(
        "E2Manage POS",
        options,
        Box::new(move |cc| {
            // Once, here, rather than per widget: every token this library reads lives in the
            // context, and a per-instance theme or locale is how two halves of one screen end up
            // disagreeing about which way it reads.
            install_environment(&cc.egui_ctx, DEFAULT_LOCALE);

            let till = Till::new(cc.egui_ctx.clone(), services)?;
            Ok(Box::new(till) as Box<dyn eframe::App>)
        }),
    )?;

    Ok(())
}

/// The application: one phase, one driver.
///
/// The phase stays the single source of truth for what the screen is showing, which is the
/// property this issue exists to establish — screen state is an enum, never a string.
struct Till {
    phase: AuthPhase,
    driver: AuthDriver,

    /// The locale currently installed in the context.
    ///
    /// Kept so the environment is reinstalled when it *changes* rather than every frame. Doing it
    /// every frame would also be correct — the library documents `install` as safe per frame — but
    /// it would hide a locale that flickers between two sessions' values.
    installed_locale: String,

    /// Ids for enquiries the *intent* fold sends.
    ///
    /// Separate from the driver's own sequence, and deliberately so: two independent minters can
    /// issue the same number, which is why nothing here matches an answer to an enquiry by id
    /// alone. The driver keys single-flight on `EnquiryKind`, and `advance` binds an accepted
    /// verification in every phase whatever id it names — both of which hold under duplicate ids.
    /// The one place an id is compared is `Verifying`'s `awaiting` against the enquiry the same
    /// `apply` call just stamped, which cannot straddle two minters.
    intent_ids: EnquiryIds,
}

impl Till {
    /// Builds the app and asks the first question.
    fn new(context: egui::Context, services: TillServices) -> Result<Self, StartupFailure> {
        let mut driver = AuthDriver::new(context, services)?;

        // The one enquiry nothing else would emit. The fold answers questions; it does not start
        // the conversation, and a splash screen with no outstanding enquiry is a till that never
        // gets past it.
        driver.dispatch(PendingEnquiry::now(AuthEnquiry::RestoreSession));

        Ok(Self {
            phase: AuthPhase::Splash,
            driver,
            installed_locale: DEFAULT_LOCALE.to_owned(),
            intent_ids: EnquiryIds::new(),
        })
    }

    /// Folds every answer that arrived since the last frame into the phase.
    fn settle(&mut self) {
        for answer in self.driver.answers() {
            let was_pairing = matches!(self.phase, AuthPhase::Pairing { .. });

            // `advance` consumes the phase, which is what keeps it a fold rather than a mutator.
            // `Splash` stands in for the moment between taking the old phase and storing the new
            // one; nothing observes it.
            let (next, pending) = advance(
                std::mem::replace(&mut self.phase, AuthPhase::Splash),
                answer,
            );
            self.phase = next;

            // The moment the screen stops being the pairing screen, the poll stops. Left running,
            // a completed poll would register the terminal and perform a full terminal login
            // behind a screen that has moved on.
            if was_pairing && !matches!(self.phase, AuthPhase::Pairing { .. }) {
                self.driver.abandon(EnquiryKind::PairingStatus);
            }

            self.driver.dispatch_all(pending);
        }
    }

    /// Reinstalls the token environment when a loaded session names a different locale.
    ///
    /// The session is authoritative over the startup default: a tenant configured for English is
    /// not served by a till that decided on Arabic before it knew who it belonged to.
    fn follow_session_locale(&mut self, context: &egui::Context) {
        let named = match &self.phase {
            AuthPhase::OperatorSelect { session, .. } | AuthPhase::PinEntry { session, .. } => {
                session.locale.as_str()
            }
            AuthPhase::Splash
            | AuthPhase::Stalled(_)
            | AuthPhase::Pairing { .. }
            | AuthPhase::SignedIn(_) => return,
        };

        if named.is_empty() {
            warn!(
                "the terminal session names no locale; keeping {}",
                self.installed_locale
            );
            return;
        }

        if named != self.installed_locale {
            info!(from = %self.installed_locale, to = %named, "the session names a different locale");
            install_environment(context, named);
            self.installed_locale = named.to_owned();
        }
    }
}

impl eframe::App for Till {
    /// What the window is cleared to, before any widget is drawn.
    ///
    /// Overriding this is load-bearing. eframe's default is `rgba(12, 12, 12, 180)` — near-black —
    /// and [`eframe::App::ui`] hands out a `Ui` with no background of its own, so leaving both
    /// alone draws the light-theme sign-in screen on black. It did: the stalled heading measured
    /// 1.20:1 against its background in a photograph of the running binary, where 4.5:1 is the
    /// floor for body text.
    ///
    /// The value comes from [`page_fill`], which reads the same environment the widgets resolve
    /// against, so the window and its contents cannot disagree about which theme is installed.
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        page_fill().to_normalized_gamma_f32()
    }

    /// eframe 0.34 hands the app a [`egui::Ui`] rather than a [`egui::Context`] — `App::ui` is the
    /// required method and `App::update` is a provided one that does nothing. Implementing
    /// `update` compiles as an inherent-looking override and never runs, so the window would open
    /// blank with no error anywhere. Checked against `eframe-0.34.3/src/epi.rs:176`.
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.settle();
        self.follow_session_locale(ui.ctx());

        let reading = if reads_right_to_left(&self.installed_locale) {
            Reading::RightToLeft
        } else {
            Reading::LeftToRight
        };

        // Drawing reads the phase and returns what was done to it. It never mutates and never
        // calls a service, so the sign-in rules stay in the two folds.
        let intents = screen::render(ui, &self.phase, reading);

        // Folded in order. A frame can produce more than one — a keypad press and a submit click
        // are separate widgets reported in the same pass — and applying only the first would
        // silently drop the other.
        for intent in intents {
            let (next, sent) = apply(
                std::mem::replace(&mut self.phase, AuthPhase::Splash),
                intent,
                &mut self.intent_ids,
            );
            self.phase = next;
            self.driver.send_all(sent);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// WCAG 2.1 relative luminance, then the contrast ratio between two opaque colours.
    ///
    /// Written out rather than taken from a crate: it is four lines, and a dependency added for a
    /// single test is a dependency the offline build has to carry.
    fn contrast_ratio(text: egui::Color32, background: egui::Color32) -> f32 {
        fn luminance(colour: egui::Color32) -> f32 {
            let channel = |value: u8| {
                let value = f32::from(value) / 255.0;
                if value <= 0.039_28 {
                    value / 12.92
                } else {
                    ((value + 0.055) / 1.055).powf(2.4)
                }
            };
            0.2126 * channel(colour.r())
                + 0.7152 * channel(colour.g())
                + 0.0722 * channel(colour.b())
        }

        let (lighter, darker) = {
            let (a, b) = (luminance(text), luminance(background));
            if a >= b {
                (a, b)
            } else {
                (b, a)
            }
        };

        (lighter + 0.05) / (darker + 0.05)
    }

    /// The fill the window is cleared to carries the body text that will be drawn on it.
    ///
    /// This is the assertion the sign-in suite could not make. A window fill reaches no
    /// accessibility node, and the snapshot references are captured over the harness's own
    /// background rather than over this value — so eight green layer-1 tests and three green
    /// reference comparisons all held over a binary drawing light-theme text on near-black.
    #[test]
    fn the_window_fill_carries_the_text_that_is_drawn_on_it() {
        let theme = screen::chrome().theme;
        let ratio = contrast_ratio(theme.foreground, page_fill());

        assert!(
            ratio >= 4.5,
            "body text on the window fill measures {ratio:.2}:1, below the 4.5:1 floor"
        );
    }

    /// The control for the assertion above, and the reason it is not vacuous.
    ///
    /// A contrast check that returns a comfortable number for *any* pair proves nothing. This
    /// feeds it the exact fill the defect shipped — eframe's default clear colour, composited
    /// over black — and requires it to fail. Without this, a broken ratio function would pass the
    /// test above and the guard would be decoration.
    #[test]
    fn the_same_check_refuses_the_fill_the_defect_shipped() {
        // `rgba(12, 12, 12, 180)` over black is what the screenshots measured: sRGB 8.
        let eframes_default_over_black = egui::Color32::from_rgb(8, 8, 8);
        let ratio = contrast_ratio(
            screen::chrome().theme.foreground,
            eframes_default_over_black,
        );

        assert!(
            ratio < 4.5,
            "the check passed the fill the defect shipped ({ratio:.2}:1), so it separates nothing"
        );
    }

    /// The whole screen mirrors on this answer, and the default locale depends on it being right
    /// for a bare `ar`. Both directions are asserted: a predicate that answered `true` for
    /// everything would pass a right-to-left-only check.
    #[test]
    fn reading_direction_follows_the_language_subtag() {
        for right_to_left in ["ar", "ar-SA", "ar_LY", "he", "fa", "ur", "AR", "Ar-Eg"] {
            assert!(
                reads_right_to_left(right_to_left),
                "{right_to_left} is written right to left"
            );
        }

        for left_to_right in ["en", "en-GB", "fr", "tr", "arn", ""] {
            assert!(
                !reads_right_to_left(left_to_right),
                "{left_to_right} is not written right to left"
            );
        }
    }

    /// `arn` is Mapudungun and is left-to-right. It is in the list above because a predicate
    /// written with `starts_with("ar")` instead of a subtag split passes every other case here
    /// and fails this one — the cheapest available control on the matching rule itself.
    #[test]
    fn the_match_is_on_the_whole_subtag_not_a_prefix() {
        assert!(reads_right_to_left("ar"));
        assert!(!reads_right_to_left("arn"));
    }

    /// The till starts right-to-left. If this is ever flipped, the change should be deliberate
    /// enough to edit a test that says so.
    #[test]
    fn the_till_starts_in_arabic_reading_right_to_left() {
        assert_eq!(DEFAULT_LOCALE, "ar");
        assert!(reads_right_to_left(DEFAULT_LOCALE));
    }
}
