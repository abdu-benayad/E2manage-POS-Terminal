//! Platform Service Integration
//!
//! Handles initialization and management of platform-level services:
//! - Heartbeat service for health reporting
//! - Policy service for security policy enforcement
//! - Update service for version management
//!
//! These services integrate with the E2Manage platform for device monitoring
//! and management beyond the tenant-level operations.

use crate::api::ApiClient;
use crate::services::{
    HeartbeatEvent, PlatformHeartbeatService, PolicyService, UpdateService,
    DEFAULT_HEARTBEAT_INTERVAL_SECONDS,
};
use std::sync::Arc;
use tokio::runtime::Runtime;
use tokio::sync::broadcast;
use tracing::{debug, error, info, warn};

/// Platform services container
///
/// Holds references to all platform-level services for centralized management.
pub struct PlatformServices {
    /// Heartbeat service for health reporting
    pub heartbeat: Arc<PlatformHeartbeatService>,
    /// Policy service for security policies
    pub policy: Arc<PolicyService>,
    /// Update service for version management
    pub update: Arc<UpdateService>,
    /// Event sender for heartbeat events
    heartbeat_tx: broadcast::Sender<HeartbeatEvent>,
}

impl PlatformServices {
    /// Creates new platform services
    ///
    /// # Arguments
    ///
    /// * `api` - Shared API client
    pub fn new(api: Arc<ApiClient>) -> Self {
        let heartbeat = Arc::new(PlatformHeartbeatService::new(Arc::clone(&api)));
        let policy = Arc::new(PolicyService::new(Arc::clone(&api)));
        let update = Arc::new(UpdateService::new(Arc::clone(&api)));
        let (heartbeat_tx, _) = broadcast::channel(16);

        Self {
            heartbeat,
            policy,
            update,
            heartbeat_tx,
        }
    }

    /// Starts all platform services
    ///
    /// Should be called after successful terminal login.
    ///
    /// # Arguments
    ///
    /// * `runtime` - Tokio runtime for async operations
    pub fn start(&self, runtime: Arc<Runtime>) {
        info!("Starting platform services");

        // Fetch initial policies
        self.fetch_initial_policies(Arc::clone(&runtime));

        // Check for updates
        self.check_for_updates(Arc::clone(&runtime));

        // Start heartbeat service
        self.start_heartbeat(runtime);
    }

    /// Fetches initial security policies from the platform
    fn fetch_initial_policies(&self, runtime: Arc<Runtime>) {
        let policy = Arc::clone(&self.policy);

        std::thread::spawn(move || {
            runtime.block_on(async move {
                match policy.refresh().await {
                    Ok(updated) => {
                        if updated {
                            info!(
                                "Security policies loaded: {} policies",
                                policy.policy_count().await
                            );
                        } else {
                            debug!("Security policies not modified");
                        }
                    }
                    Err(e) => {
                        warn!("Failed to fetch security policies: {}", e);
                        // Continue without policies - they'll be fetched on next heartbeat
                    }
                }
            });
        });
    }

    /// Checks for available updates
    fn check_for_updates(&self, runtime: Arc<Runtime>) {
        let update = Arc::clone(&self.update);

        std::thread::spawn(move || {
            runtime.block_on(async move {
                match update.check_for_updates("stable").await {
                    Ok(info) => {
                        if info.update_available {
                            if info.force_update {
                                warn!(
                                    "Mandatory update available: {} -> {}",
                                    info.current_version, info.latest_version
                                );
                            } else {
                                info!(
                                    "Update available: {} -> {}",
                                    info.current_version, info.latest_version
                                );
                            }
                        } else {
                            debug!("Running latest version: {}", info.current_version);
                        }
                    }
                    Err(e) => {
                        warn!("Failed to check for updates: {}", e);
                    }
                }
            });
        });
    }

    /// Starts the heartbeat service
    fn start_heartbeat(&self, runtime: Arc<Runtime>) {
        let heartbeat = Arc::clone(&self.heartbeat);
        let heartbeat_tx = self.heartbeat_tx.clone();
        let policy = Arc::clone(&self.policy);

        std::thread::spawn(move || {
            runtime.block_on(async move {
                // Get heartbeat interval from policy, or use default
                let interval = policy
                    .get_heartbeat_interval_seconds()
                    .await
                    .unwrap_or(DEFAULT_HEARTBEAT_INTERVAL_SECONDS as u32)
                    as u64;

                info!("Starting heartbeat service with {}s interval", interval);
                heartbeat.start(interval, heartbeat_tx).await;
            });
        });
    }

    /// Subscribes to heartbeat events
    ///
    /// Returns a receiver that will receive all heartbeat events.
    pub fn subscribe_heartbeat(&self) -> broadcast::Receiver<HeartbeatEvent> {
        self.heartbeat_tx.subscribe()
    }

    /// Records an error for the next heartbeat
    pub async fn record_error(&self, error: &str) {
        self.heartbeat.record_error(error).await;
    }

    /// Refreshes security policies
    pub async fn refresh_policies(&self) -> Result<bool, crate::services::PolicyError> {
        self.policy.refresh().await
    }

    /// Stops all platform services
    pub async fn stop(&self) {
        info!("Stopping platform services");
        self.heartbeat.stop().await;
    }

    /// Gets the current app version
    pub fn app_version(&self) -> &str {
        self.update.current_version()
    }
}

/// Initializes platform services and starts them after successful login
///
/// This is the main entry point for platform service integration.
///
/// # Arguments
///
/// * `api` - Shared API client
/// * `runtime` - Tokio runtime
///
/// # Returns
///
/// `PlatformServices` container
pub fn init_platform_services(api: Arc<ApiClient>, runtime: Arc<Runtime>) -> PlatformServices {
    let services = PlatformServices::new(api);
    services.start(runtime);
    services
}

/// Handles heartbeat events
///
/// This function processes events from the heartbeat service and takes
/// appropriate actions (e.g., triggering policy refresh, notifying about updates).
///
/// # Arguments
///
/// * `event` - The heartbeat event to handle
/// * `platform` - Reference to platform services
/// * `runtime` - Tokio runtime
pub fn handle_heartbeat_event(
    event: HeartbeatEvent,
    platform: &PlatformServices,
    runtime: Arc<Runtime>,
) {
    match event {
        HeartbeatEvent::HeartbeatSent { status } => {
            debug!("Heartbeat sent: {:?}", status);
        }
        HeartbeatEvent::PolicyRefreshRequested => {
            info!("Platform requested policy refresh");
            let policy = Arc::clone(&platform.policy);
            std::thread::spawn(move || {
                runtime.block_on(async move {
                    if let Err(e) = policy.refresh().await {
                        error!("Failed to refresh policies: {}", e);
                    }
                });
            });
        }
        HeartbeatEvent::UpdateRequested {
            version,
            download_url,
        } => {
            warn!(
                "Platform requested update to version {} (url: {:?})",
                version, download_url
            );
            // TODO: Trigger update UI or background download
        }
        HeartbeatEvent::RestartRequested => {
            warn!("Platform requested terminal restart");
            // TODO: Trigger graceful restart
        }
        HeartbeatEvent::ForceLogoutRequested => {
            warn!("Platform requested force logout");
            // TODO: Trigger force logout of all operators
        }
        HeartbeatEvent::HeartbeatFailed { error } => {
            error!("Heartbeat failed: {}", error);
        }
        HeartbeatEvent::CommandReceived(cmd) => {
            debug!("Received platform command: {:?}", cmd);
        }
    }
}
