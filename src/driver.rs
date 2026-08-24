//! The one place in the till that touches the async runtime.
//!
//! The sign-in screen is a fold: [`advance`](e2manage_pos_terminal::ui::sign_in::advance) takes a
//! phase and an answer and returns the next phase plus the enquiries to make. It holds no service,
//! no runtime and no channel, and it must not acquire any — that is what makes the whole machine
//! testable by calling a function. This module is the other half: it turns an
//! [`AuthEnquiry`] into a service call, puts the result back on a channel, and wakes the window.
//!
//! Nothing here decides anything about the screen. If a rule about *what happens next* appears in
//! this file, it belongs in the fold instead.
//!
//! # Two obligations that are easy to miss, and silent when missed
//!
//! **The repaint.** egui only redraws when something asks it to. An answer arriving on a channel
//! is not an input event, so without [`Context::request_repaint`] a till that has been approved on
//! the platform sits on "waiting for approval" until a cashier happens to touch the screen. The
//! design deliberately cannot fall back on repainting continuously: a permanently-repainting UI
//! makes `egui_kittest`'s run-to-settle call panic, so the test harness for step 14 depends on
//! this being event-driven.
//!
//! **Single-flight.** Every enquiry here except `LoadOperators` has already committed an effect by
//! the time its answer exists, so two in flight is not a wasted request — it is two effects. The
//! fold gates the pairing poll with `poll_in_flight`, and this module gates every kind
//! independently, because the fold cannot see an enquiry the *view* originated.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;

use directories::ProjectDirs;
use e2manage_pos_terminal::ui::sign_in::{
    AuthAnswer, AuthEnquiry, DispatchedEnquiry, EnquiryId, EnquiryIds, EnquiryKind, PendingEnquiry,
};
use pos_api::ApiClient;
use pos_db::{init_database, Database};
use pos_services::{AuthService, PairingService};
use thiserror::Error;
use tokio::runtime::Runtime;
use tracing::{debug, warn};

/// The API this till talks to when `E2M_API_URL` says nothing.
///
/// Matches the value `CLAUDE.md` documents. It is a constant rather than a read of
/// `config/default.toml`: nothing in this workspace parses TOML, no manifest declares a parser,
/// and adding one to answer a single string would be a configuration subsystem introduced by
/// accident.
const DEFAULT_API_URL: &str = "http://178.156.135.235:3000";

/// The environment variable that overrides it.
const API_URL_VARIABLE: &str = "E2M_API_URL";

// ============================================================================
// Startup
// ============================================================================

/// What can go wrong before there is a screen to say so.
///
/// Each variant names the thing that failed and what it was trying to reach, because the reader is
/// a person in a shop with a till that will not start and a log file.
#[derive(Debug, Error)]
pub enum StartupFailure {
    /// No directory this platform agrees is the user's.
    #[error(
        "this platform names no data directory for the till, so there is nowhere to keep its \
         database; the till cannot run without one"
    )]
    NoDataDirectory,

    /// The database would not open, or its migrations would not run.
    #[error("the till's database under {path} could not be opened or migrated")]
    Database {
        /// Where it looked, so the reader can check permissions and free space.
        path: PathBuf,
        /// The underlying failure, kept as an error rather than flattened into a message.
        #[source]
        source: rusqlite::Error,
    },

    /// The runtime that reaches the platform would not start.
    #[error("the async runtime the till uses to reach the platform could not be started")]
    Runtime(#[source] std::io::Error),
}

/// Everything the sign-in enquiries need, assembled once.
///
/// Deliberately **not** the whole platform-services set. This screen needs three things, and a
/// driver holding more would tie the window's startup to subsystems that have nothing to do with
/// signing in.
pub struct TillServices {
    auth: Arc<AuthService>,
    pairing: Arc<PairingService>,
    db: Arc<Database>,
}

impl TillServices {
    /// Opens the database and builds the services over it.
    ///
    /// Fallible in one direction only: everything that can fail does so here, before the window
    /// opens, so the driver below never has a half-built service to explain.
    pub fn open() -> Result<Self, StartupFailure> {
        let data_dir = data_directory().ok_or(StartupFailure::NoDataDirectory)?;

        let db = init_database(&data_dir).map_err(|source| StartupFailure::Database {
            path: data_dir.clone(),
            source,
        })?;
        let db = Arc::new(db);

        let api = Arc::new(ApiClient::new(&api_base_url()));

        Ok(Self {
            auth: Arc::new(AuthService::new(Arc::clone(&api), Arc::clone(&db))),
            pairing: Arc::new(PairingService::new(api, Arc::clone(&db))),
            db,
        })
    }
}

/// Where this till keeps its database and logs.
///
/// The same three-part identity `log_service` already uses, so the logs and the database live
/// beside each other rather than in two conventions that drift.
pub fn data_directory() -> Option<PathBuf> {
    ProjectDirs::from("com", "e2manage", "pos-terminal")
        .map(|dirs| dirs.data_local_dir().to_path_buf())
}

/// The platform's base URL.
fn api_base_url() -> String {
    std::env::var(API_URL_VARIABLE).unwrap_or_else(|_| DEFAULT_API_URL.to_owned())
}

// ============================================================================
// The driver
// ============================================================================

/// One enquiry that has left and not yet come back.
struct Outstanding {
    /// Which enquiry, for the log line when a second of its kind is refused.
    id: EnquiryId,

    /// Raised to stop a *delayed* enquiry before it commits anything.
    ///
    /// Checked once, after the delay and strictly before the service call. It is never checked
    /// again: abandoning a call that has already started would leave the effect (a registration,
    /// a terminal login, a spent PIN attempt) with nothing on the screen that knows about it,
    /// which is the exact state [`Discardable::Never`] exists to describe.
    ///
    /// [`Discardable::Never`]: e2manage_pos_terminal::ui::sign_in::Discardable::Never
    abandon: Arc<AtomicBool>,
}

/// What one finished enquiry hands back.
///
/// Carries the *kind* rather than deriving it from the answers, because one enquiry may answer
/// more than once — see [`perform`] — and a driver that cleared its single-flight slot on the
/// first of two would let a second enquiry of that kind start while the first was still running.
struct Delivery {
    kind: EnquiryKind,
    answers: Vec<AuthAnswer>,
}

/// Turns enquiries into service calls, and answers into a repaint.
pub struct AuthDriver {
    runtime: Runtime,
    services: Arc<TillServices>,
    context: egui::Context,
    post: Sender<Delivery>,
    deliveries: Receiver<Delivery>,
    ids: EnquiryIds,
    outstanding: HashMap<EnquiryKind, Outstanding>,
}

impl AuthDriver {
    /// Starts the runtime and takes a handle on the window.
    ///
    /// The [`egui::Context`] is cloned rather than borrowed: it is an `Arc` internally, and every
    /// spawned task needs one to wake the window with.
    pub fn new(context: egui::Context, services: TillServices) -> Result<Self, StartupFailure> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .thread_name("till-platform")
            .build()
            .map_err(StartupFailure::Runtime)?;

        let (post, deliveries) = mpsc::channel();

        Ok(Self {
            runtime,
            services: Arc::new(services),
            context,
            post,
            deliveries,
            ids: EnquiryIds::new(),
            outstanding: HashMap::new(),
        })
    }

    /// Sends one enquiry, unless one of its kind is already outstanding.
    ///
    /// Refusing is the whole point rather than a fallback. `check_pairing_status` is not the
    /// status read its name suggests: on completion it saves the registration, performs a full
    /// terminal login — which *replaces* the client's terminal token — and writes the terminal
    /// configuration. Two interleaved polls can leave the client presenting one session while the
    /// stored configuration holds another, and nothing downstream would report the mismatch.
    pub fn dispatch(&mut self, pending: PendingEnquiry) {
        let kind = pending.asking.kind();

        if let Some(already) = self.outstanding.get(&kind) {
            debug!(
                ?kind,
                outstanding = already.id.get(),
                "an enquiry of this kind is already in flight; not sending a second"
            );
            return;
        }

        let DispatchedEnquiry {
            id,
            run_after,
            asking,
        } = pending.dispatch(&mut self.ids);

        let abandon = Arc::new(AtomicBool::new(false));
        self.outstanding.insert(
            kind,
            Outstanding {
                id,
                abandon: Arc::clone(&abandon),
            },
        );

        let services = Arc::clone(&self.services);
        let post = self.post.clone();
        let context = self.context.clone();

        self.runtime.spawn(async move {
            if !run_after.is_zero() {
                tokio::time::sleep(run_after).await;
            }

            // The only cancellation point, and it is before the effect. See `Outstanding::abandon`.
            if abandon.load(Ordering::SeqCst) {
                debug!(?kind, "a delayed enquiry was abandoned before it was sent");
                return;
            }

            let answers = perform(&services, id, asking).await;

            if let Err(undelivered) = post.send(Delivery { kind, answers }) {
                // The window has closed. The effect landed regardless; nothing can act on it now,
                // so this is recorded rather than handled.
                warn!(
                    ?kind,
                    "an answer arrived after the screen was gone: {undelivered}"
                );
                return;
            }

            // Without this the window sleeps until somebody touches it.
            context.request_repaint();
        });
    }

    /// Sends each of the enquiries a fold asked for.
    pub fn dispatch_all(&mut self, pending: Vec<PendingEnquiry>) {
        for enquiry in pending {
            self.dispatch(enquiry);
        }
    }

    /// Every answer that has arrived since the last frame, in the order it was produced.
    ///
    /// Never blocks: this runs inside a frame, and a frame that waits on the platform is a frozen
    /// till.
    pub fn answers(&mut self) -> Vec<AuthAnswer> {
        let mut arrived = Vec::new();

        // `Empty` (nothing more this frame) and `Disconnected` (every sender is gone and there
        // never will be) end this loop alike, which is why it matches on `Ok` rather than naming
        // the two: a frame renders what has arrived either way, and a match that distinguished
        // them would imply a difference this function does not act on.
        while let Ok(delivery) = self.deliveries.try_recv() {
            self.outstanding.remove(&delivery.kind);
            arrived.extend(delivery.answers);
        }

        arrived
    }

    /// Stops a delayed enquiry of this kind, if it has not yet been sent.
    ///
    /// Used for the pairing poll the moment the phase stops being `Pairing`. An enquiry already
    /// past its delay is left to finish — see [`Outstanding::abandon`] for why that is the safe
    /// direction — but its slot is released here, because the screen is no longer waiting on it.
    pub fn abandon(&mut self, kind: EnquiryKind) {
        if let Some(outstanding) = self.outstanding.remove(&kind) {
            outstanding.abandon.store(true, Ordering::SeqCst);
            debug!(?kind, id = outstanding.id.get(), "abandoned");
        }
    }
}

/// Makes the call one enquiry names.
///
/// # Why this returns a list
///
/// Every enquiry answers once, except [`AuthEnquiry::RestoreSession`], which answers twice — and
/// the *order* is a contract the fold enforces silently.
///
/// The plan describes the splash phase as "reads the saved terminal session, and if one is held,
/// restores the operator". Those are two questions with two answers, and the fold has arms at
/// `Splash` for both. It has no arm that turns one into the other: `(Splash, SessionRestored ->
/// Ok(None))` emits **no** follow-up enquiry, so if this function did not send the second answer
/// unprompted, a till with nobody signed in would sit on the splash screen forever.
///
/// The order is forced, and getting it backwards loses a signed-in operator without any error.
/// `TerminalSessionOpened -> Ok(Some(..))` at `Splash` moves the screen to `OperatorSelect`. A
/// `SessionRestored -> Ok(Some(operator))` arriving *after* that lands at `OperatorSelect`, where
/// the fold's catch-all returns the phase unchanged — the operator was restored, the till holds
/// their token, and the screen shows a list asking who they are. So `SessionRestored` is sent
/// first, and the terminal session is only looked up when it came back `Ok(None)`: if somebody is
/// already signed in, the screen is going to `SignedIn` and the question does not arise.
async fn perform(services: &TillServices, id: EnquiryId, asking: AuthEnquiry) -> Vec<AuthAnswer> {
    match asking {
        AuthEnquiry::RestoreSession => {
            let restored = services.auth.operator_sign_in().restore().await;

            match restored {
                Ok(None) => vec![
                    AuthAnswer::SessionRestored {
                        id,
                        outcome: Ok(None),
                    },
                    AuthAnswer::TerminalSessionOpened {
                        id,
                        outcome: services.auth.load_saved_session(),
                    },
                ],
                settled => vec![AuthAnswer::SessionRestored {
                    id,
                    outcome: settled,
                }],
            }
        }

        AuthEnquiry::RequestPairingCode => vec![AuthAnswer::PairingCodeRequested {
            id,
            outcome: services.pairing.request_pairing_code().await,
        }],

        AuthEnquiry::PairingStatus { code } => vec![AuthAnswer::PairingStatusRead {
            id,
            outcome: services.pairing.check_pairing_status(code.as_str()).await,
        }],

        AuthEnquiry::LoadOperators => vec![AuthAnswer::OperatorsLoaded {
            id,
            outcome: services.db.get_operators().map_err(anyhow::Error::from),
        }],

        AuthEnquiry::VerifyPin {
            operator,
            pin,
            policy,
        } => vec![AuthAnswer::PinVerified {
            id,
            // No `Result`: `verify_pin` is total in all three directions, and the timeout it
            // honours is the API client's own. A second, shorter one here would report a slow
            // platform as an unreachable one, which is a different sentence on the screen.
            outcome: services.auth.verify_pin(&operator, &pin, &policy).await,
        }],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use e2manage_pos_terminal::ui::sign_in::{advance, AuthPhase};
    use pos_api::SessionToken;
    use pos_models::OperatorId;
    use pos_services::TerminalSession;

    fn id() -> EnquiryId {
        EnquiryIds::new().mint()
    }

    fn operator() -> OperatorId {
        OperatorId::new("op-1").expect("a well-formed operator id")
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

    /// The order `perform` sends `RestoreSession`'s two answers in is load-bearing, and the fold
    /// enforces it by *silence* — the wrong order hits the catch-all and returns the phase
    /// unchanged. This is the arrangement the driver actually uses.
    #[test]
    fn restore_then_terminal_session_reaches_the_operator_list() {
        let (phase, _) = advance(
            AuthPhase::Splash,
            AuthAnswer::SessionRestored {
                id: id(),
                outcome: Ok(None),
            },
        );
        assert!(
            matches!(phase, AuthPhase::Splash),
            "nobody signed in should leave the screen on the splash, waiting for the second answer"
        );

        let (phase, pending) = advance(
            phase,
            AuthAnswer::TerminalSessionOpened {
                id: id(),
                outcome: Ok(Some(session())),
            },
        );

        assert!(
            matches!(phase, AuthPhase::OperatorSelect { .. }),
            "a held terminal session should reach the operator list, not {phase:?}"
        );
        assert_eq!(
            pending.len(),
            1,
            "reaching the operator list should ask for the operators"
        );
    }

    /// The failure the order exists to prevent, asserted rather than described.
    ///
    /// Sending the terminal session first moves the screen to `OperatorSelect`; the restored
    /// operator then arrives at a phase with no arm for it and is swallowed by the catch-all. The
    /// till holds that operator's token and shows a list asking who they are. Nothing goes red —
    /// which is exactly why this is a test and not a comment.
    #[test]
    fn the_reverse_order_silently_loses_a_restored_operator() {
        let (phase, _) = advance(
            AuthPhase::Splash,
            AuthAnswer::TerminalSessionOpened {
                id: id(),
                outcome: Ok(Some(session())),
            },
        );
        assert!(matches!(phase, AuthPhase::OperatorSelect { .. }));

        let (phase, pending) = advance(
            phase,
            AuthAnswer::SessionRestored {
                id: id(),
                outcome: Ok(Some(operator())),
            },
        );

        assert!(
            matches!(phase, AuthPhase::OperatorSelect { .. }),
            "this test documents the defect: the operator is dropped and the screen stays on the \
             list. If this now reaches `SignedIn`, the fold grew an arm for it and `perform`'s \
             ordering comment is out of date — fix the comment, keep the arm"
        );
        assert!(
            pending.is_empty(),
            "the catch-all emits nothing, so nothing recovers the lost operator"
        );
    }

    /// `Ok(Some(..))` must not go looking for a terminal session: the screen is bound for
    /// `SignedIn` and the question does not arise. Guards the branch in `perform` directly.
    #[test]
    fn a_restored_operator_needs_no_terminal_session_answer() {
        let (phase, pending) = advance(
            AuthPhase::Splash,
            AuthAnswer::SessionRestored {
                id: id(),
                outcome: Ok(Some(operator())),
            },
        );

        assert!(
            matches!(phase, AuthPhase::SignedIn(_)),
            "a restored operator signs straight in, so the second answer would arrive at a phase \
             that has moved on"
        );
        assert!(pending.is_empty());
    }
}
