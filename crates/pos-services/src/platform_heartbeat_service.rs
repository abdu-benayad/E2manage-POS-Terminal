//! Platform Heartbeat Service - Periodic health reporting to platform
//!
//! Provides functionality to:
//! - Collect system metrics (CPU, memory, disk usage)
//! - Send periodic heartbeats to the platform
//! - Process commands received from the platform
//! - Track error counts for health status
//!
//! ## Usage
//!
//! ```rust,ignore
//! use pos_services::{PlatformHeartbeatService, HeartbeatEvent};
//! use pos_api::ApiClient;
//! use std::sync::Arc;
//!
//! let api = Arc::new(ApiClient::new("https://api.example.com"));
//! let service = PlatformHeartbeatService::new(api);
//!
//! // Start the heartbeat loop
//! let (tx, rx) = tokio::sync::broadcast::channel(16);
//! service.start(60, tx).await;
//! ```

use pos_api::{
    ApiClient, HeartbeatStatus, PlatformCommand, PlatformCommandType, PlatformHeartbeatRequest,
};
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

/// Default heartbeat interval in seconds
pub const DEFAULT_HEARTBEAT_INTERVAL_SECONDS: u64 = 60;

/// Maximum number of recent errors to track
const MAX_RECENT_ERRORS: usize = 10;

/// Events emitted by the heartbeat service
#[derive(Debug, Clone)]
pub enum HeartbeatEvent {
    /// Heartbeat was sent successfully
    HeartbeatSent {
        status: HeartbeatStatus,
    },
    /// Platform sent a command
    CommandReceived(PlatformCommandType),
    /// Update command received
    UpdateRequested {
        version: String,
        download_url: Option<String>,
    },
    /// Restart command received
    RestartRequested,
    /// Policy refresh requested
    PolicyRefreshRequested,
    /// Force logout requested
    ForceLogoutRequested,
    /// Heartbeat failed
    HeartbeatFailed {
        error: String,
    },
}

/// System metrics collected for heartbeat
#[derive(Debug, Clone, Default)]
pub struct SystemMetrics {
    /// CPU usage percentage (0-100)
    pub cpu_usage: f64,
    /// Memory usage percentage (0-100)
    pub memory_usage: f64,
    /// Disk usage percentage (0-100)
    pub disk_usage: f64,
}

/// Platform Heartbeat Service
///
/// Sends periodic heartbeats to the platform with device health metrics.
pub struct PlatformHeartbeatService {
    api: Arc<ApiClient>,
    /// Current application version
    app_version: String,
    /// Error count since last successful heartbeat
    error_count: AtomicU32,
    /// Total uptime in seconds
    uptime_seconds: AtomicU64,
    /// Recent error messages
    recent_errors: RwLock<Vec<String>>,
    /// Whether the service is running
    is_running: RwLock<bool>,
}

impl PlatformHeartbeatService {
    /// Creates a new platform heartbeat service
    ///
    /// # Arguments
    ///
    /// * `api` - API client for server communication
    pub fn new(api: Arc<ApiClient>) -> Self {
        Self {
            api,
            app_version: env!("CARGO_PKG_VERSION").to_string(),
            error_count: AtomicU32::new(0),
            uptime_seconds: AtomicU64::new(0),
            recent_errors: RwLock::new(Vec::new()),
            is_running: RwLock::new(false),
        }
    }

    /// Gets the current application version
    pub fn app_version(&self) -> &str {
        &self.app_version
    }

    /// Records an error for the next heartbeat
    pub async fn record_error(&self, error: &str) {
        self.error_count.fetch_add(1, Ordering::SeqCst);

        let mut errors = self.recent_errors.write().await;
        if errors.len() >= MAX_RECENT_ERRORS {
            errors.remove(0);
        }
        errors.push(error.to_string());
    }

    /// Clears the error count and recent errors
    pub async fn clear_errors(&self) {
        self.error_count.store(0, Ordering::SeqCst);
        self.recent_errors.write().await.clear();
    }

    /// Collects current system metrics
    fn collect_metrics(&self) -> SystemMetrics {
        let mut metrics = SystemMetrics::default();

        // CPU usage - use /proc/stat on Linux
        #[cfg(target_os = "linux")]
        {
            metrics.cpu_usage = self.get_cpu_usage_linux();
        }

        // Memory usage - use /proc/meminfo on Linux
        #[cfg(target_os = "linux")]
        {
            metrics.memory_usage = self.get_memory_usage_linux();
        }

        // Disk usage - use statvfs
        #[cfg(target_os = "linux")]
        {
            metrics.disk_usage = self.get_disk_usage_linux();
        }

        // On other platforms, return zeros (could be extended)
        #[cfg(not(target_os = "linux"))]
        {
            metrics.cpu_usage = 0.0;
            metrics.memory_usage = 0.0;
            metrics.disk_usage = 0.0;
        }

        metrics
    }

    #[cfg(target_os = "linux")]
    fn get_cpu_usage_linux(&self) -> f64 {
        // Read /proc/stat for CPU info
        if let Ok(content) = std::fs::read_to_string("/proc/stat") {
            if let Some(cpu_line) = content.lines().next() {
                let parts: Vec<u64> = cpu_line
                    .split_whitespace()
                    .skip(1) // Skip "cpu"
                    .filter_map(|s| s.parse().ok())
                    .collect();

                if parts.len() >= 4 {
                    let user = parts[0];
                    let nice = parts[1];
                    let system = parts[2];
                    let idle = parts[3];

                    let total = user + nice + system + idle;
                    let used = user + nice + system;

                    if total > 0 {
                        return (used as f64 / total as f64) * 100.0;
                    }
                }
            }
        }
        0.0
    }

    #[cfg(target_os = "linux")]
    fn get_memory_usage_linux(&self) -> f64 {
        if let Ok(content) = std::fs::read_to_string("/proc/meminfo") {
            let mut mem_total: u64 = 0;
            let mut mem_available: u64 = 0;

            for line in content.lines() {
                if line.starts_with("MemTotal:") {
                    mem_total = parse_meminfo_value(line);
                } else if line.starts_with("MemAvailable:") {
                    mem_available = parse_meminfo_value(line);
                }
            }

            if mem_total > 0 {
                let used = mem_total.saturating_sub(mem_available);
                return (used as f64 / mem_total as f64) * 100.0;
            }
        }
        0.0
    }

    #[cfg(target_os = "linux")]
    fn get_disk_usage_linux(&self) -> f64 {
        // Use nix crate for statvfs if available, or shell out
        // For simplicity, we'll use a basic approach
        if let Ok(output) = std::process::Command::new("df")
            .args(["--output=pcent", "/"])
            .output()
        {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                // Output format: "Use%\n XX%"
                if let Some(line) = stdout.lines().nth(1) {
                    if let Some(percent_str) = line.trim().strip_suffix('%') {
                        if let Ok(percent) = percent_str.parse::<f64>() {
                            return percent;
                        }
                    }
                }
            }
        }
        0.0
    }

    /// Determines the heartbeat status based on metrics and errors
    fn determine_status(&self, metrics: &SystemMetrics) -> HeartbeatStatus {
        let error_count = self.error_count.load(Ordering::SeqCst);

        // Error status if there are recent errors
        if error_count > 5 {
            return HeartbeatStatus::Error;
        }

        // Degraded if high resource usage
        if metrics.cpu_usage > 90.0 || metrics.memory_usage > 90.0 || metrics.disk_usage > 95.0 {
            return HeartbeatStatus::Degraded;
        }

        // Degraded if some errors
        if error_count > 0 {
            return HeartbeatStatus::Degraded;
        }

        HeartbeatStatus::Healthy
    }

    /// Sends a single heartbeat
    pub async fn send_heartbeat(&self) -> Result<Vec<PlatformCommand>, HeartbeatError> {
        let metrics = self.collect_metrics();
        let status = self.determine_status(&metrics);
        let error_count = self.error_count.load(Ordering::SeqCst);
        let errors = self.recent_errors.read().await;

        let request = PlatformHeartbeatRequest {
            status,
            current_version: self.app_version.clone(),
            cpu_usage: Some(metrics.cpu_usage),
            memory_usage: Some(metrics.memory_usage),
            disk_usage: Some(metrics.disk_usage),
            error_count: if error_count > 0 {
                Some(error_count)
            } else {
                None
            },
            errors: if errors.is_empty() {
                None
            } else {
                Some(errors.clone())
            },
        };

        drop(errors); // Release lock before async call

        debug!(
            "Sending heartbeat: status={:?}, cpu={:.1}%, mem={:.1}%, disk={:.1}%",
            status, metrics.cpu_usage, metrics.memory_usage, metrics.disk_usage
        );

        let response = self
            .api
            .platform_heartbeat(&request)
            .await
            .map_err(|e| HeartbeatError::NetworkError(e.to_string()))?;

        if response.acknowledged {
            // Clear errors after successful heartbeat
            self.clear_errors().await;
            debug!("Heartbeat acknowledged, commands: {}", response.commands.len());
        } else {
            warn!("Heartbeat not acknowledged by server");
        }

        Ok(response.commands)
    }

    /// Starts the heartbeat loop
    ///
    /// Sends heartbeats at the specified interval and broadcasts events.
    ///
    /// # Arguments
    ///
    /// * `interval_seconds` - Interval between heartbeats
    /// * `event_tx` - Channel to broadcast heartbeat events
    pub async fn start(
        self: Arc<Self>,
        interval_seconds: u64,
        event_tx: broadcast::Sender<HeartbeatEvent>,
    ) {
        // Check if already running
        {
            let mut is_running = self.is_running.write().await;
            if *is_running {
                warn!("Heartbeat service already running");
                return;
            }
            *is_running = true;
        }

        info!(
            "Starting platform heartbeat service (interval: {}s)",
            interval_seconds
        );

        let interval = Duration::from_secs(interval_seconds);
        let mut ticker = tokio::time::interval(interval);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            ticker.tick().await;

            // Check if we should stop
            if !*self.is_running.read().await {
                info!("Heartbeat service stopped");
                break;
            }

            // Update uptime
            self.uptime_seconds
                .fetch_add(interval_seconds, Ordering::SeqCst);

            // Send heartbeat
            match self.send_heartbeat().await {
                Ok(commands) => {
                    let metrics = self.collect_metrics();
                    let status = self.determine_status(&metrics);

                    // Broadcast success event
                    let _ = event_tx.send(HeartbeatEvent::HeartbeatSent { status });

                    // Process commands
                    for command in commands {
                        self.process_command(&command, &event_tx).await;
                    }
                }
                Err(e) => {
                    error!("Heartbeat failed: {}", e);
                    let _ = event_tx.send(HeartbeatEvent::HeartbeatFailed {
                        error: e.to_string(),
                    });
                }
            }
        }
    }

    /// Stops the heartbeat loop
    pub async fn stop(&self) {
        info!("Stopping platform heartbeat service");
        *self.is_running.write().await = false;
    }

    /// Processes a command received from the platform
    async fn process_command(
        &self,
        command: &PlatformCommand,
        event_tx: &broadcast::Sender<HeartbeatEvent>,
    ) {
        info!("Processing platform command: {:?}", command.command_type);

        let _ = event_tx.send(HeartbeatEvent::CommandReceived(command.command_type.clone()));

        match command.command_type {
            PlatformCommandType::Update => {
                let version = command
                    .params
                    .get("version")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string();
                let download_url = command
                    .params
                    .get("downloadUrl")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());

                info!("Update command received: version={}", version);
                let _ = event_tx.send(HeartbeatEvent::UpdateRequested {
                    version,
                    download_url,
                });
            }
            PlatformCommandType::Restart => {
                warn!("Restart command received from platform");
                let _ = event_tx.send(HeartbeatEvent::RestartRequested);
            }
            PlatformCommandType::RefreshPolicies => {
                info!("Policy refresh command received");
                let _ = event_tx.send(HeartbeatEvent::PolicyRefreshRequested);
            }
            PlatformCommandType::ForceLogout => {
                warn!("Force logout command received from platform");
                let _ = event_tx.send(HeartbeatEvent::ForceLogoutRequested);
            }
            PlatformCommandType::ClearCache => {
                info!("Clear cache command received");
                // This would be handled by the main application
            }
            PlatformCommandType::Unknown => {
                warn!("Unknown command type received: {:?}", command.params);
            }
        }
    }

    /// Checks if the service is currently running
    pub async fn is_running(&self) -> bool {
        *self.is_running.read().await
    }

    /// Gets the current uptime in seconds
    pub fn uptime_seconds(&self) -> u64 {
        self.uptime_seconds.load(Ordering::SeqCst)
    }
}

/// Heartbeat service errors
#[derive(Debug, Clone)]
pub enum HeartbeatError {
    /// Network communication error
    NetworkError(String),
    /// Server rejected heartbeat
    Rejected(String),
}

impl std::fmt::Display for HeartbeatError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HeartbeatError::NetworkError(e) => write!(f, "Network error: {}", e),
            HeartbeatError::Rejected(e) => write!(f, "Heartbeat rejected: {}", e),
        }
    }
}

impl std::error::Error for HeartbeatError {}

/// Parse a value from /proc/meminfo line (e.g., "MemTotal:        8000000 kB")
#[cfg(target_os = "linux")]
fn parse_meminfo_value(line: &str) -> u64 {
    line.split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pos_api::ApiClient;

    fn create_test_service() -> Arc<PlatformHeartbeatService> {
        let api = Arc::new(ApiClient::new("https://api.example.com"));
        Arc::new(PlatformHeartbeatService::new(api))
    }

    #[test]
    fn test_new_service() {
        let service = create_test_service();
        assert!(!service.app_version().is_empty());
    }

    #[tokio::test]
    async fn test_record_error() {
        let service = create_test_service();

        service.record_error("Test error 1").await;
        service.record_error("Test error 2").await;

        assert_eq!(service.error_count.load(Ordering::SeqCst), 2);

        let errors = service.recent_errors.read().await;
        assert_eq!(errors.len(), 2);
    }

    #[tokio::test]
    async fn test_clear_errors() {
        let service = create_test_service();

        service.record_error("Test error").await;
        assert_eq!(service.error_count.load(Ordering::SeqCst), 1);

        service.clear_errors().await;
        assert_eq!(service.error_count.load(Ordering::SeqCst), 0);

        let errors = service.recent_errors.read().await;
        assert!(errors.is_empty());
    }

    #[test]
    fn test_collect_metrics() {
        let service = create_test_service();
        let metrics = service.collect_metrics();

        // Metrics should be valid percentages
        assert!(metrics.cpu_usage >= 0.0 && metrics.cpu_usage <= 100.0);
        assert!(metrics.memory_usage >= 0.0 && metrics.memory_usage <= 100.0);
        assert!(metrics.disk_usage >= 0.0 && metrics.disk_usage <= 100.0);
    }

    #[test]
    fn test_determine_status_healthy() {
        let service = create_test_service();
        let metrics = SystemMetrics {
            cpu_usage: 30.0,
            memory_usage: 50.0,
            disk_usage: 60.0,
        };

        let status = service.determine_status(&metrics);
        assert_eq!(status, HeartbeatStatus::Healthy);
    }

    #[test]
    fn test_determine_status_degraded_high_cpu() {
        let service = create_test_service();
        let metrics = SystemMetrics {
            cpu_usage: 95.0,
            memory_usage: 50.0,
            disk_usage: 60.0,
        };

        let status = service.determine_status(&metrics);
        assert_eq!(status, HeartbeatStatus::Degraded);
    }

    #[tokio::test]
    async fn test_determine_status_with_errors() {
        let service = create_test_service();

        // Add some errors
        service.record_error("Error 1").await;
        service.record_error("Error 2").await;

        let metrics = SystemMetrics::default();
        let status = service.determine_status(&metrics);
        assert_eq!(status, HeartbeatStatus::Degraded);
    }

    #[tokio::test]
    async fn test_determine_status_error() {
        let service = create_test_service();

        // Add many errors
        for i in 0..10 {
            service.record_error(&format!("Error {}", i)).await;
        }

        let metrics = SystemMetrics::default();
        let status = service.determine_status(&metrics);
        assert_eq!(status, HeartbeatStatus::Error);
    }

    #[tokio::test]
    async fn test_max_recent_errors() {
        let service = create_test_service();

        // Add more than MAX_RECENT_ERRORS
        for i in 0..15 {
            service.record_error(&format!("Error {}", i)).await;
        }

        let errors = service.recent_errors.read().await;
        assert_eq!(errors.len(), MAX_RECENT_ERRORS);

        // First error should have been removed
        assert!(!errors[0].contains("Error 0"));
    }
}
