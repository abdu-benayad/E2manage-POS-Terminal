//! Startup configuration loaded from environment variables.
//!
//! `pos-bootstrap` does not parse files or CLI flags — front-ends are
//! responsible for assembling [`InitConfig`] however they like.
//! [`InitConfig::from_env`] preserves the historical defaults from
//! `src/main.rs` (`data` directory next to the binary's CWD, the dev-IP
//! API URL, 5-minute sync interval) so swapping `main.rs` over to call
//! [`crate::init`] does not change runtime behaviour.

use std::path::PathBuf;

/// Default API URL used when `E2M_API_URL` is unset. This matches the
/// historical default in `src/main.rs`. The remediation plan tracks
/// replacing this with a fail-closed configuration; this struct
/// preserves prior behaviour to keep the bootstrap refactor a pure
/// move.
const DEFAULT_SERVER_URL: &str = "http://178.156.135.235:3000";

/// Default sync interval in minutes. Matches the prior `SYNC_INTERVAL_MINUTES`
/// constant in `src/main.rs` (note: the `docs/ARCHITECTURE.md` quotes 10
/// minutes — the discrepancy is tracked in the remediation plan).
const DEFAULT_SYNC_INTERVAL_MINUTES: u64 = 5;

/// All inputs the startup sequence needs.
#[derive(Debug, Clone)]
pub struct InitConfig {
    /// Directory holding the SQLite database and any on-disk state. Created
    /// by `pos_db::init_database` if it does not already exist.
    pub data_dir: PathBuf,
    /// Base URL of the E2Manage backend.
    pub server_url: String,
    /// How often the background sync loop should poll the backend.
    pub sync_interval_minutes: u64,
}

impl InitConfig {
    /// Build a config from environment variables, falling back to the same
    /// defaults the Slint binary previously hard-coded.
    pub fn from_env() -> Self {
        Self {
            data_dir: PathBuf::from("data"),
            server_url: std::env::var("E2M_API_URL")
                .unwrap_or_else(|_| DEFAULT_SERVER_URL.to_string()),
            sync_interval_minutes: DEFAULT_SYNC_INTERVAL_MINUTES,
        }
    }
}

impl Default for InitConfig {
    fn default() -> Self {
        Self::from_env()
    }
}
