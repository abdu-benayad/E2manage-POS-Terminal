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

use abdu_egui_ui::{Environment, Locale};
use driver::{data_directory, AuthDriver, StartupFailure, TillServices};
use e2manage_pos_terminal::ui::sign_in::{
    advance, AuthEnquiry, AuthPhase, EnquiryKind, PendingEnquiry,
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

/// Installs the component library's tokens for one locale.
///
/// Idempotent by the library's own contract — each token overwrites its slot in the context — so
/// this is also the path a locale *change* takes. There is no separate replace call.
fn install_environment(context: &egui::Context, locale: &str) {
    Environment::light()
        .locale(Locale {
            current: locale.to_owned(),
            rtl: reads_right_to_left(locale),
            ..Locale::default()
        })
        .install(context);
}

/// Whether a language tag is written right to left.
///
/// A short list rather than a library: these are the scripts this product ships into, and a wrong
/// answer here mirrors an entire screen. Matching is on the language subtag, so `ar-SA` and `ar`
/// agree.
fn reads_right_to_left(locale: &str) -> bool {
    let language = locale
        .split(['-', '_'])
        .next()
        .unwrap_or(locale)
        .to_ascii_lowercase();

    matches!(language.as_str(), "ar" | "he" | "fa" | "ur")
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
    /// eframe 0.34 hands the app a [`egui::Ui`] rather than a [`egui::Context`] — `App::ui` is the
    /// required method and `App::update` is a provided one that does nothing. Implementing
    /// `update` compiles as an inherent-looking override and never runs, so the window would open
    /// blank with no error anywhere. Checked against `eframe-0.34.3/src/epi.rs:176`.
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.settle();
        self.follow_session_locale(ui.ctx());

        // Exhaustive with no catch-all, for the reason every other match in this issue is: a new
        // phase must fail to compile rather than fall through to a blank screen.
        match &self.phase {
            AuthPhase::Splash
            | AuthPhase::Stalled(_)
            | AuthPhase::Pairing { .. }
            | AuthPhase::OperatorSelect { .. }
            | AuthPhase::PinEntry { .. }
            | AuthPhase::SignedIn(_) => {
                // Step 13 renders these. One arm rather than six placeholders, so the compiler
                // still forces every phase to be considered when it does.
                ui.centered_and_justified(|ui| {
                    ui.spinner();
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
