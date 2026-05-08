//! Process-startup wiring shared by the `pos-terminal` Slint binary and the
//! `pos-headless` developer CLI.
//!
//! The single entry point is [`init`], which mirrors the startup sequence
//! that lived in `src/main.rs` — open the SQLite database, build an
//! `ApiClient`, restore the saved session token on it, and instantiate every
//! service the application uses at the process boundary. The result is an
//! [`AppContext`] that callers wire into their own front-end (Slint event
//! loop, CLI subcommand, future daemon, etc.).
//!
//! `pos-bootstrap` deliberately does *not* depend on Slint or any UI crate —
//! that is the entire point. A change to `pos-services` should rebuild
//! `pos-bootstrap` and `pos-headless` in seconds, without touching Slint.

use std::sync::Arc;

use pos_api::ApiClient;
use pos_db::{init_database, Database};
use pos_services::{
    AuthService, CartService, DraftService, PairingService, ProductService, SharedDraftService,
    SyncEvent, SyncService, SystemService, TerminalRegistration, TerminalSession,
};
use tokio::runtime::Runtime;
use tokio::sync::broadcast;
use tracing::info;

pub mod config;
pub mod error;

pub use config::InitConfig;
pub use error::BootstrapError;

/// Capacity of the shared `SyncEvent` broadcast channel. Matches the value
/// previously used in `src/main.rs`.
const SYNC_EVENT_CHANNEL_CAPACITY: usize = 16;

/// All process-wide state produced by [`init`]. Every field is `Arc`-wrapped
/// where the application shares it across threads, exactly as the previous
/// `src/main.rs` did. Callers move what they need into closures or pass it
/// to subcommands; nothing here is owned by `pos-bootstrap`.
///
/// `FeatureService` is intentionally omitted: it is consumed by the binary's
/// `Navigator`, which is UI-specific. Each front-end constructs its own
/// `FeatureService` from the shared `db` if it needs one.
pub struct AppContext {
    pub config: InitConfig,
    pub runtime: Arc<Runtime>,
    pub db: Arc<Database>,
    pub api: Arc<ApiClient>,
    pub auth: Arc<AuthService>,
    pub pairing: Arc<PairingService>,
    pub cart: Arc<CartService>,
    pub draft: Arc<DraftService>,
    pub shared_draft: Arc<SharedDraftService>,
    pub sync: Arc<SyncService>,
    pub product: Arc<ProductService>,
    pub system: Arc<SystemService>,
    pub sync_tx: Arc<broadcast::Sender<SyncEvent>>,
    pub saved_session: Option<TerminalSession>,
    pub registration: Option<TerminalRegistration>,
}

/// Run the application's startup sequence and return the wired
/// [`AppContext`].
///
/// This is the function the Slint binary's `main` previously expressed
/// inline as ~80 lines of `Arc::new(Service::new(...))` calls. Callers must
/// initialise their own logging subscriber (`tracing_subscriber`) before
/// invoking — `pos-bootstrap` does not touch global tracing state.
pub fn init(config: InitConfig) -> Result<AppContext, BootstrapError> {
    let runtime = Arc::new(Runtime::new().map_err(BootstrapError::Runtime)?);

    let db = Arc::new(
        init_database(&config.data_dir)
            .map_err(|e| BootstrapError::Database(anyhow::Error::from(e)))?,
    );
    info!(data_dir = ?config.data_dir, "database initialised");

    let api = Arc::new(ApiClient::new(&config.server_url));
    info!(server_url = %config.server_url, "api client created");

    let auth = Arc::new(AuthService::new(Arc::clone(&api), Arc::clone(&db)));
    let saved_session = auth.load_saved_session().map_err(BootstrapError::SessionLoad)?;
    if let Some(ref session) = saved_session {
        if !session.session_token.is_empty() {
            info!(
                terminal_code = %session.terminal_code,
                "restoring saved session token onto api client"
            );
            runtime.block_on(api.set_token(session.session_token.clone()));
        }
    }

    let pairing = Arc::new(PairingService::new(Arc::clone(&api), Arc::clone(&db)));
    let registration = pairing.get_registration().map_err(BootstrapError::Registration)?;

    // Terminal/warehouse identity for shared-draft sync. Falls back to the
    // historical "TERM-001" / "default-warehouse" values if no registration
    // exists, preserving the binary's prior behaviour. Replacing this with a
    // hard error is tracked separately in the remediation plan.
    let terminal_id_for_drafts = registration
        .as_ref()
        .and_then(|r| r.terminal_id.clone())
        .or_else(|| registration.as_ref().and_then(|r| r.terminal_code.clone()))
        .unwrap_or_else(|| "TERM-001".to_string());

    let warehouse_id_for_drafts = registration
        .as_ref()
        .and_then(|r| r.tenant_id.clone())
        .unwrap_or_else(|| "default-warehouse".to_string());

    let shared_draft = Arc::new(SharedDraftService::new(
        Arc::clone(&api),
        Arc::clone(&db),
        warehouse_id_for_drafts,
        terminal_id_for_drafts,
    ));

    let mut sync_service = SyncService::new(
        Arc::clone(&api),
        Arc::clone(&db),
        config.sync_interval_minutes,
    );
    sync_service.set_shared_draft_service(Arc::clone(&shared_draft));
    let sync = Arc::new(sync_service);

    let cart = Arc::new(CartService::new());
    let draft = Arc::new(DraftService::new(Arc::clone(&db)));
    let product = Arc::new(ProductService::new(Arc::clone(&db)));
    let system = Arc::new(SystemService::new());

    let (sync_tx, _sync_rx) = broadcast::channel::<SyncEvent>(SYNC_EVENT_CHANNEL_CAPACITY);
    let sync_tx = Arc::new(sync_tx);

    Ok(AppContext {
        config,
        runtime,
        db,
        api,
        auth,
        pairing,
        cart,
        draft,
        shared_draft,
        sync,
        product,
        system,
        sync_tx,
        saved_session,
        registration,
    })
}
