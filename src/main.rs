//! The till's entry point.
//!
//! Reintroduces the workspace's only `[[bin]]`, deleted with the previous view layer. What it
//! does today is start logging and open the window on [`AuthPhase::Splash`]; the driver that
//! answers the screen's enquiries is task 12 and the remaining phases render in task 13.
//!
//! # The startup order is a contract, not a convenience
//!
//! Logging is installed **before** anything else runs, because the things that fail earliest —
//! a database that will not open, a config file that will not parse — fail before any screen
//! exists to say so, and a failure nobody records is a failure nobody can diagnose from a till
//! in a shop.

use std::process::ExitCode;

use e2manage_pos_terminal::ui::sign_in::AuthPhase;
use tracing::info;
use tracing_subscriber::EnvFilter;

/// Where the window starts. A till runs full-screen on the shop floor; this is the size it
/// occupies while somebody is developing against it.
const INITIAL_WINDOW_SIZE: [f32; 2] = [1024.0, 768.0];

fn main() -> ExitCode {
    install_logging();
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

/// Installs the log subscriber.
///
/// `RUST_LOG` decides the level, defaulting to `info` — the same contract the rest of the
/// workspace documents. A malformed `RUST_LOG` falls back to `info` rather than refusing to
/// start: a mistyped environment variable must not be the reason a shop cannot sell.
fn install_logging() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();
}

/// Opens the window and runs until it closes.
fn run() -> Result<(), eframe::Error> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size(INITIAL_WINDOW_SIZE),
        ..Default::default()
    };

    eframe::run_native(
        "E2Manage POS",
        options,
        Box::new(|_cc| Ok(Box::new(Till::new()))),
    )
}

/// The application, holding the sign-in screen's phase.
///
/// One field today. Task 12 adds the driver and the answer channel beside it; the phase stays
/// the single source of truth for what the screen is showing, which is the property
/// `egui-auth-screen` exists to establish — screen state is an enum, never a string.
struct Till {
    phase: AuthPhase,
}

impl Till {
    fn new() -> Self {
        Self {
            phase: AuthPhase::Splash,
        }
    }
}

impl eframe::App for Till {
    /// eframe 0.34 hands the app a [`egui::Ui`] rather than a [`egui::Context`] — `App::ui` is
    /// the required method and `App::update` is a provided one that does nothing. Implementing
    /// `update` compiles as an inherent-looking override and never runs, so the window would
    /// open blank with no error anywhere. Checked against `eframe-0.34.3/src/epi.rs:176`.
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // Exhaustive with no catch-all, for the reason every other match in this issue is: a
        // new phase must fail to compile rather than fall through to a blank screen.
        match &self.phase {
            AuthPhase::Splash => {
                ui.centered_and_justified(|ui| {
                    ui.spinner();
                });
            }
            AuthPhase::Stalled(_)
            | AuthPhase::Pairing { .. }
            | AuthPhase::OperatorSelect { .. }
            | AuthPhase::PinEntry { .. }
            | AuthPhase::SignedIn(_) => {
                // Unreachable until task 12 wires the driver: nothing advances the phase yet.
                // A distinct arm rather than folded into `Splash`, so the compiler still forces
                // every phase to be considered when task 13 renders them.
                ui.centered_and_justified(|ui| {
                    ui.spinner();
                });
            }
        }
    }
}
