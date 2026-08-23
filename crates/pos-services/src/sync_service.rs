//! Sync Service - Background data synchronization
//!
//! This service handles periodic synchronization of data from the E2Manage backend:
//! - Products and categories catalog
//! - Operators list
//! - Screens definitions (optional)
//!
//! ## Features
//!
//! - 10-minute polling interval (configurable)
//! - ETag support for efficient bandwidth usage
//! - Broadcast channel for sync events
//! - Graceful offline handling
//!
//! ## Usage
//!
//! ```rust,ignore
//! use crate::services::SyncService;
//! use tokio::sync::broadcast;
//!
//! let sync_service = SyncService::new(api, db, 10); // 10 minute interval
//! let (tx, mut rx) = broadcast::channel(16);
//!
//! // Start polling in background
//! tokio::spawn(async move {
//!     sync_service.start_polling(tx).await;
//! });
//!
//! // Listen for events
//! while let Ok(event) = rx.recv().await {
//!     println!("Sync event: {:?}", event);
//! }
//! ```

use crate::offline_service::OfflineService;
use crate::shared_draft_service::SharedDraftService;
use anyhow::Result;
use chrono::Utc;
use pos_api::sync::{CatalogDeltaResponse, CatalogResponse, OperatorsResponse};
use pos_api::{ApiClient, GetResult};
use pos_db::{Database, SyncResource};
use pos_models::{Feature, FeatureScreen};
use rusqlite::params;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast;
use tracing::{debug, error, info, warn};

/// Default sync interval in minutes
pub const DEFAULT_SYNC_INTERVAL_MINUTES: u64 = 10;

/// Sync events broadcast to listeners
#[derive(Clone, Debug)]
pub enum SyncEvent {
    /// Sync started
    Started,
    /// Catalog (products + categories) updated
    CatalogUpdated {
        products_count: usize,
        categories_count: usize,
    },
    /// Catalog delta synced (incremental update)
    CatalogDeltaSynced {
        products_updated: usize,
        products_deleted: usize,
        categories_updated: usize,
        categories_deleted: usize,
    },
    /// Catalog not modified (304)
    CatalogNotModified,
    /// Operators updated
    OperatorsUpdated { count: usize },
    /// Operators not modified (304)
    OperatorsNotModified,
    /// Screens updated
    ScreensUpdated { count: usize },
    /// Features updated
    FeaturesUpdated { count: usize },
    /// Features not modified (304)
    FeaturesNotModified,
    /// Offline queue processed
    OfflineQueueProcessed {
        synced: u32,
        failed: u32,
        conflicts: u32,
    },
    /// No offline transactions to process
    OfflineQueueEmpty,
    /// Shared drafts queue processed
    DraftsSynced { synced: u32, failed: u32 },
    /// No pending draft sync operations
    DraftsQueueEmpty,
    /// Conflict detected during sync (needs manager resolution)
    ConflictDetected {
        transaction_id: String,
        error: String,
    },
    /// Multiple conflicts detected during sync
    ConflictsDetected { count: u32 },
    /// Sync completed successfully
    Completed,
    /// Sync failed
    Failed(String),
    /// Terminal is offline
    Offline,
    /// Terminal registration is invalid - needs re-pairing
    TerminalNotRegistered,
}

/// Sync service for background data synchronization
pub struct SyncService {
    api: Arc<ApiClient>,
    db: Arc<Database>,
    interval: Duration,
    shared_draft_service: Option<Arc<SharedDraftService>>,
}

impl SyncService {
    /// Creates a new sync service
    ///
    /// # Arguments
    ///
    /// * `api` - Shared API client
    /// * `db` - Shared database
    /// * `interval_minutes` - Polling interval in minutes
    pub fn new(api: Arc<ApiClient>, db: Arc<Database>, interval_minutes: u64) -> Self {
        Self {
            api,
            db,
            interval: Duration::from_secs(interval_minutes * 60),
            shared_draft_service: None,
        }
    }

    /// Creates a sync service with default 10-minute interval
    pub fn with_defaults(api: Arc<ApiClient>, db: Arc<Database>) -> Self {
        Self::new(api, db, DEFAULT_SYNC_INTERVAL_MINUTES)
    }

    /// Sets the shared draft service for draft sync operations
    pub fn with_shared_draft_service(mut self, service: Arc<SharedDraftService>) -> Self {
        self.shared_draft_service = Some(service);
        self
    }

    /// Sets the shared draft service (mutable reference version)
    pub fn set_shared_draft_service(&mut self, service: Arc<SharedDraftService>) {
        self.shared_draft_service = Some(service);
    }

    /// Returns the sync interval
    pub fn interval(&self) -> Duration {
        self.interval
    }

    /// Starts the polling loop (runs forever)
    ///
    /// This method runs an infinite loop that syncs data at the configured interval.
    /// Each sync cycle broadcasts events through the provided sender.
    ///
    /// # Arguments
    ///
    /// * `tx` - Broadcast sender for sync events
    pub async fn start_polling(&self, tx: broadcast::Sender<SyncEvent>) {
        info!(
            "Starting sync service with {} minute interval",
            self.interval.as_secs() / 60
        );

        // Initial sync immediately
        self.run_sync_cycle(&tx).await;

        // Then poll at interval
        loop {
            tokio::time::sleep(self.interval).await;
            self.run_sync_cycle(&tx).await;
        }
    }

    /// Runs a single sync cycle
    async fn run_sync_cycle(&self, tx: &broadcast::Sender<SyncEvent>) {
        let _ = tx.send(SyncEvent::Started);

        // Check if online
        use pos_api::OnlineStatus;
        match self.api.is_online().await {
            OnlineStatus::Online => { /* proceed with sync */ }
            OnlineStatus::AuthRejected => {
                error!("Online check returned auth rejected — terminal/tenant invalid");
                // Try re-auth before giving up
                match self.try_reauthenticate().await {
                    Ok(true) => {
                        info!("Re-authentication successful after auth rejection");
                    }
                    _ => {
                        let _ = tx.send(SyncEvent::TerminalNotRegistered);
                        error!("Re-authentication failed, terminal needs re-pairing");
                        return;
                    }
                }
            }
            OnlineStatus::Offline => {
                warn!("Sync skipped: terminal is offline");
                let _ = tx.send(SyncEvent::Offline);
                return;
            }
        }

        match self.sync_all(tx).await {
            Ok(_) => {
                let _ = tx.send(SyncEvent::Completed);
                info!("Sync completed successfully");
            }
            Err(e) => {
                // Check if this is an authentication error (401)
                if Self::is_auth_error(&e) {
                    warn!("Sync failed with auth error, attempting re-authentication...");

                    // Try to re-authenticate
                    match self.try_reauthenticate().await {
                        Ok(true) => {
                            // Re-auth succeeded, retry sync
                            info!("Re-authentication successful, retrying sync...");
                            match self.sync_all(tx).await {
                                Ok(_) => {
                                    let _ = tx.send(SyncEvent::Completed);
                                    info!("Sync completed successfully after re-authentication");
                                }
                                Err(retry_err) => {
                                    let _ = tx.send(SyncEvent::Failed(retry_err.to_string()));
                                    error!("Sync failed after re-authentication: {}", retry_err);
                                }
                            }
                        }
                        Ok(false) => {
                            let _ = tx.send(SyncEvent::TerminalNotRegistered);
                            error!("Re-authentication failed, terminal needs re-pairing");
                        }
                        Err(reauth_err) => {
                            let _ = tx.send(SyncEvent::Failed(reauth_err.to_string()));
                            error!("Re-authentication error: {}", reauth_err);
                        }
                    }
                } else {
                    let _ = tx.send(SyncEvent::Failed(e.to_string()));
                    error!("Sync failed: {}", e);
                }
            }
        }
    }

    /// Synchronizes all data types
    ///
    /// This is the main sync method that can be called directly for manual sync.
    pub async fn sync_all(&self, tx: &broadcast::Sender<SyncEvent>) -> Result<()> {
        // Sync catalog (products + categories)
        self.sync_catalog(tx).await?;

        // Sync operators
        self.sync_operators(tx).await?;

        // Sync features (Feature-Based Screen Library) - non-fatal, log and continue
        if let Err(e) = self.sync_features(tx).await {
            warn!("Features sync failed (non-fatal): {}", e);
            // Don't fail the entire sync for features - they're not critical
        }

        // Sync screens (optional, uncomment if needed)
        // self.sync_screens(tx).await?;

        // Process shared drafts sync queue
        self.process_draft_sync_queue(tx).await?;

        // Process offline transaction queue
        self.process_offline_queue(tx).await?;

        Ok(())
    }

    /// Processes the shared drafts sync queue
    ///
    /// Syncs any pending draft operations (create, convert, delete) to the backend.
    async fn process_draft_sync_queue(&self, tx: &broadcast::Sender<SyncEvent>) -> Result<()> {
        // Only process if we have a shared draft service configured
        let service = match &self.shared_draft_service {
            Some(s) => s,
            None => {
                debug!("No shared draft service configured, skipping draft sync");
                return Ok(());
            }
        };

        // Check if there are pending operations
        let pending = match service.pending_sync_count() {
            Ok(count) => count,
            Err(e) => {
                warn!("Failed to check pending draft syncs: {}", e);
                return Ok(());
            }
        };

        if pending == 0 {
            debug!("No pending draft sync operations");
            let _ = tx.send(SyncEvent::DraftsQueueEmpty);
            return Ok(());
        }

        info!("Processing {} pending draft sync operations", pending);

        match service.process_sync_queue().await {
            Ok(result) => {
                if result.has_activity() {
                    info!(
                        "Draft sync queue: {} synced, {} failed",
                        result.synced, result.failed
                    );
                    let _ = tx.send(SyncEvent::DraftsSynced {
                        synced: result.synced,
                        failed: result.failed,
                    });
                }
            }
            Err(e) => {
                warn!("Draft sync queue processing failed: {}", e);
                // Non-fatal - continue with other sync operations
            }
        }

        Ok(())
    }

    /// Checks if an error is a 401 authentication error
    fn is_auth_error(error: &anyhow::Error) -> bool {
        let msg = error.to_string();
        msg.contains("401") || msg.contains("authentication failed") || msg.contains("Unauthorized")
    }

    /// Gets stored terminal credentials for re-authentication
    /// Returns (terminal_code, hardware_id, secret)
    fn get_credentials(&self) -> Result<Option<(String, String, String)>> {
        let conn = self.db.connection();
        let conn = conn.lock();

        let result = conn.query_row(
            r#"
            SELECT terminal_code, hardware_id, secret
            FROM terminal_registration
            WHERE id = 1 AND is_registered = 1 AND secret IS NOT NULL
            "#,
            [],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            },
        );

        match result {
            Ok((terminal_code, hardware_id, secret)) => {
                Ok(Some((terminal_code, hardware_id, secret)))
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Saves the new session token to database
    fn save_session_token(&self, token: &str) -> Result<()> {
        let conn = self.db.connection();
        let conn = conn.lock();

        conn.execute(
            "UPDATE terminal_config SET session_token = ?1, updated_at = datetime('now') WHERE id = 1",
            params![token],
        )?;

        info!("Session token updated in database");
        Ok(())
    }

    /// Attempts to re-authenticate when token is expired
    async fn try_reauthenticate(&self) -> Result<bool> {
        info!("Attempting to re-authenticate due to expired token...");

        // Get stored credentials
        let credentials = self.get_credentials()?;
        let (terminal_code, hardware_id, secret) = match credentials {
            Some(creds) => creds,
            None => {
                warn!("No stored credentials found for re-authentication");
                return Ok(false);
            }
        };

        // Try to login
        match self
            .api
            .login_terminal(&terminal_code, &hardware_id, &secret)
            .await
        {
            Ok(response) => {
                info!("Re-authentication successful, new token acquired");
                // Save new token to database
                if let Err(e) = self.save_session_token(&response.session_token) {
                    warn!("Failed to save new session token: {}", e);
                }
                Ok(true)
            }
            Err(e) => {
                error!("Re-authentication failed: {}", e);
                Ok(false)
            }
        }
    }

    /// Processes the offline transaction queue
    ///
    /// Syncs any pending offline transactions to the backend.
    async fn process_offline_queue(&self, tx: &broadcast::Sender<SyncEvent>) -> Result<()> {
        debug!("Processing offline transaction queue...");

        let offline_service = OfflineService::new(Arc::clone(&self.api), Arc::clone(&self.db));

        // Check if there are pending transactions
        let pending = offline_service.pending_count()?;
        if pending == 0 {
            debug!("No offline transactions to process");
            let _ = tx.send(SyncEvent::OfflineQueueEmpty);
            return Ok(());
        }

        info!("Processing {} pending offline transactions", pending);

        let result = offline_service.process_queue().await?;

        if result.has_activity() {
            info!(
                "Offline queue: {} synced, {} failed, {} conflicts",
                result.synced, result.failed, result.conflicts
            );
            let _ = tx.send(SyncEvent::OfflineQueueProcessed {
                synced: result.synced,
                failed: result.failed,
                conflicts: result.conflicts,
            });

            // Notify UI about conflicts needing resolution
            if result.has_conflicts() {
                let _ = tx.send(SyncEvent::ConflictsDetected {
                    count: result.conflicts,
                });
            }
        }

        Ok(())
    }

    /// Syncs product catalog from server
    ///
    /// Uses delta sync when ETag exists for efficient incremental updates.
    /// Falls back to full sync if delta is unavailable or on first sync.
    async fn sync_catalog(&self, tx: &broadcast::Sender<SyncEvent>) -> Result<()> {
        debug!("Syncing catalog...");

        // Debug: Check if token is set
        let has_token = self.api.is_authenticated().await;
        let token = self.api.get_token().await;
        info!(
            "Sync catalog - has token: {}, token preview: {:?}",
            has_token,
            token
                .as_ref()
                .map(|t| format!("{}...", &t[..t.len().min(16)]))
        );

        // Get current ETag
        let etag = self.db.get_etag(SyncResource::Products)?;

        // Get last sync time for delta requests
        let last_sync = self
            .db
            .get_sync_state(SyncResource::Products)?
            .and_then(|s| s.last_sync);

        // If we have an ETag and last_sync, try delta sync first
        if let (Some(_), Some(last_sync)) = (&etag, &last_sync) {
            if let Ok(true) = self.try_delta_sync(tx, last_sync).await {
                return Ok(());
            }
            // Delta sync failed or returned nothing, fall through to full sync
            info!("Delta sync unavailable, falling back to full catalog sync");
        }

        // Full sync (first sync or delta not available)
        self.sync_catalog_full(tx, etag.as_deref()).await
    }

    /// Attempts delta sync - returns Ok(true) if successful, Ok(false) if should fall back
    async fn try_delta_sync(&self, tx: &broadcast::Sender<SyncEvent>, since: &str) -> Result<bool> {
        debug!("Attempting delta sync since {}", since);

        // Try to fetch delta
        let result: Result<CatalogDeltaResponse> = self.api.get_catalog_delta(since).await;

        match result {
            Ok(delta) => {
                let products_updated = delta.updated.len();
                let products_deleted = delta.deleted.len();
                let categories_updated = delta.categories_updated.len();
                let categories_deleted = delta.categories_deleted.len();

                // No changes
                if products_updated == 0
                    && products_deleted == 0
                    && categories_updated == 0
                    && categories_deleted == 0
                {
                    debug!("Delta sync: no changes");
                    let _ = tx.send(SyncEvent::CatalogNotModified);
                    return Ok(true);
                }

                // Apply category updates first (before products due to FK)
                for cat_dto in &delta.categories_updated {
                    self.db.save_category(&cat_dto.to_category_row())?;
                }

                // Apply category deletions
                if !delta.categories_deleted.is_empty() {
                    self.db.delete_categories(&delta.categories_deleted)?;
                }

                // Apply product updates
                let products: Vec<_> = delta.updated.iter().map(|p| p.to_product_row()).collect();
                if !products.is_empty() {
                    self.db.save_products(&products)?;
                }

                // Apply product deletions
                if !delta.deleted.is_empty() {
                    self.db.delete_products(&delta.deleted)?;
                }

                // Update sync state with new version
                self.db.update_sync_state(
                    SyncResource::Products,
                    Some(&delta.version),
                    Some(&delta.version),
                    delta.total_product_count as i64,
                )?;

                info!(
                    "Delta sync: {} products updated, {} deleted, {} categories updated, {} deleted",
                    products_updated, products_deleted, categories_updated, categories_deleted
                );

                let _ = tx.send(SyncEvent::CatalogDeltaSynced {
                    products_updated,
                    products_deleted,
                    categories_updated,
                    categories_deleted,
                });

                Ok(true)
            }
            Err(e) => {
                // Check if it's a 404 (endpoint not available) or other error
                let err_str = e.to_string();
                if err_str.contains("404") || err_str.contains("Not Found") {
                    debug!("Delta endpoint not available, will use full sync");
                    Ok(false)
                } else {
                    warn!("Delta sync failed: {}, falling back to full sync", e);
                    Ok(false)
                }
            }
        }
    }

    /// Performs full catalog sync (downloads everything)
    async fn sync_catalog_full(
        &self,
        tx: &broadcast::Sender<SyncEvent>,
        etag: Option<&str>,
    ) -> Result<()> {
        // Fetch with ETag (includeCategories=true to get Arabic category names)
        let result: GetResult<CatalogResponse> = self.api.get_catalog(etag).await?;

        match result {
            GetResult::NotModified => {
                debug!("Catalog not modified (304)");
                let _ = tx.send(SyncEvent::CatalogNotModified);
            }
            GetResult::Data { data, etag } => {
                // Extract unique categories from products (API doesn't return separate categories)
                let mut category_map = std::collections::HashMap::new();
                for product in &data.products {
                    if let Some(ref cat_id) = product.category_id {
                        if !category_map.contains_key(cat_id) {
                            category_map.insert(
                                cat_id.clone(),
                                pos_db::CategoryRow {
                                    id: cat_id.clone(),
                                    parent_id: None,
                                    name: product
                                        .category_name
                                        .clone()
                                        .unwrap_or_else(|| "Unknown".to_string()),
                                    name_ar: None,
                                    color: None,
                                    icon: None,
                                    image_url: None,
                                    display_order: 0,
                                    is_active: true,
                                },
                            );
                        }
                    }
                }

                // Also add any categories from the response (if provided)
                for cat in &data.categories {
                    category_map.insert(cat.id.clone(), cat.to_category_row());
                }

                // Full replace: clear old products first, then categories
                // (products reference categories via FK, so delete products first)
                self.db.execute("DELETE FROM products", &[])?;
                self.db.execute("DELETE FROM categories", &[])?;

                // Save categories FIRST (before products due to foreign key)
                for cat in category_map.values() {
                    self.db.save_category(cat)?;
                }

                // Save products
                let products: Vec<_> = data.products.iter().map(|p| p.to_product_row()).collect();
                self.db.save_products(&products)?;

                // Update sync state
                self.db.update_sync_state(
                    SyncResource::Products,
                    etag.as_deref(),
                    data.version.as_deref(),
                    products.len() as i64,
                )?;

                self.db.update_sync_state(
                    SyncResource::Categories,
                    None,
                    None,
                    category_map.len() as i64,
                )?;

                info!(
                    "Catalog synced: {} products, {} categories",
                    products.len(),
                    category_map.len()
                );

                let _ = tx.send(SyncEvent::CatalogUpdated {
                    products_count: products.len(),
                    categories_count: category_map.len(),
                });
            }
        }

        Ok(())
    }

    /// Syncs operators from server
    async fn sync_operators(&self, tx: &broadcast::Sender<SyncEvent>) -> Result<()> {
        debug!("Syncing operators...");

        // Get current ETag
        let etag = self.db.get_etag(SyncResource::Operators)?;

        // Fetch with ETag
        let result: GetResult<OperatorsResponse> = self.api.get_operators(etag.as_deref()).await?;

        match result {
            GetResult::NotModified => {
                debug!("Operators not modified (304)");
                let _ = tx.send(SyncEvent::OperatorsNotModified);
            }
            GetResult::Data { data, etag } => {
                // Convert before touching the store. `to_operator_row` is fallible now — the
                // wire's name pair becomes one domain value that refuses to be blank — and
                // deactivating first would mean one malformed operator in the batch leaves the
                // till with every operator switched off and none saved back. Nobody could log in.
                let operators: Vec<_> = data
                    .operators
                    .iter()
                    .map(pos_api::OperatorDto::to_operator_row)
                    .collect::<Result<_, _>>()?;

                // Full replace: deactivate all existing operators, then save fresh list.
                // This ensures operators removed on the server don't linger locally.
                self.db.deactivate_all_operators()?;

                // Save operators (INSERT OR REPLACE re-activates them)
                self.db.save_operators(&operators)?;

                // Update sync state
                self.db.update_sync_state(
                    SyncResource::Operators,
                    etag.as_deref(),
                    None,
                    operators.len() as i64,
                )?;

                info!("Operators synced: {} operators", operators.len());

                let _ = tx.send(SyncEvent::OperatorsUpdated {
                    count: operators.len(),
                });
            }
        }

        Ok(())
    }

    /// Syncs features from server (Feature-Based Screen Library)
    ///
    /// Fetches enabled features and their screens, storing them in SQLite.
    /// Uses ETag for conditional GET - skips if unchanged.
    async fn sync_features(&self, tx: &broadcast::Sender<SyncEvent>) -> Result<()> {
        debug!("Syncing features...");

        // Get current ETag
        let etag = self.db.get_etag(SyncResource::Features)?;

        // Fetch from API
        let response = self.api.get_features(etag.as_deref()).await?;

        match response {
            None => {
                // 304 Not Modified
                debug!("Features not modified (304)");
                let _ = tx.send(SyncEvent::FeaturesNotModified);
            }
            Some(features_response) => {
                // Convert DTOs to models
                let mut features: Vec<Feature> = Vec::new();
                let mut screens: Vec<FeatureScreen> = Vec::new();

                for feature_dto in &features_response.features {
                    // Convert FeatureDto to Feature
                    let feature = Feature {
                        feature_id: feature_dto.feature_id.clone(),
                        name: feature_dto.name.clone(),
                        name_ar: feature_dto.name_ar.clone(),
                        config_key: feature_dto.config_key.clone(),
                        is_core: feature_dto.is_core,
                        is_enabled: feature_dto.enabled,
                        icon: feature_dto.icon.clone(),
                        display_order: feature_dto.display_order,
                        updated_at: Utc::now().to_rfc3339(),
                    };
                    features.push(feature);

                    // Convert FeatureScreenDto to FeatureScreen
                    for screen_dto in &feature_dto.screens {
                        let screen = FeatureScreen {
                            feature_id: feature_dto.feature_id.clone(),
                            screen_id: screen_dto.screen_id.clone(),
                            name: screen_dto.name.clone(),
                            name_ar: screen_dto.name_ar.clone(),
                            is_entry_point: screen_dto.is_entry_point,
                            next_screen: screen_dto.next_screen.clone(),
                            display_order: screen_dto.display_order,
                        };
                        screens.push(screen);
                    }
                }

                // Store in database (this clears old data first)
                self.db.sync_features(&features, &screens)?;

                // Update sync state with ETag
                self.db.update_sync_state(
                    SyncResource::Features,
                    Some(&features_response.version),
                    Some(&features_response.version),
                    features.len() as i64,
                )?;

                let count = features.len();
                info!(
                    "Features synced: {} features, {} screens",
                    count,
                    screens.len()
                );

                let _ = tx.send(SyncEvent::FeaturesUpdated { count });
            }
        }

        Ok(())
    }

    /// Forces a full sync (ignoring ETags)
    ///
    /// Useful when data may be corrupted or for initial setup.
    pub async fn force_full_sync(&self, tx: &broadcast::Sender<SyncEvent>) -> Result<()> {
        info!("Forcing full sync (clearing ETags)...");

        // Clear sync state to force full download
        self.db.clear_all_sync_state()?;

        // Run sync
        self.sync_all(tx).await
    }

    /// Checks if any resource needs sync
    pub fn needs_sync(&self) -> Result<bool> {
        let interval_minutes = (self.interval.as_secs() / 60) as i64;

        let products_need = self
            .db
            .needs_sync(SyncResource::Products, interval_minutes)?;
        let operators_need = self
            .db
            .needs_sync(SyncResource::Operators, interval_minutes)?;

        Ok(products_need || operators_need)
    }

    /// Gets sync status for all resources
    pub fn get_sync_status(&self) -> Result<Vec<SyncStatus>> {
        let mut statuses = Vec::new();

        for resource in &[
            SyncResource::Products,
            SyncResource::Categories,
            SyncResource::Operators,
        ] {
            if let Some(state) = self.db.get_sync_state(*resource)? {
                // Get time_ago before moving last_sync
                let time_ago = state.time_ago();
                statuses.push(SyncStatus {
                    resource: resource.to_string(),
                    last_sync: state.last_sync,
                    record_count: state.record_count,
                    time_ago,
                });
            } else {
                statuses.push(SyncStatus {
                    resource: resource.to_string(),
                    last_sync: None,
                    record_count: 0,
                    time_ago: "never".to_string(),
                });
            }
        }

        Ok(statuses)
    }
}

/// Status information for a synced resource
#[derive(Debug, Clone)]
pub struct SyncStatus {
    pub resource: String,
    pub last_sync: Option<String>,
    pub record_count: i64,
    pub time_ago: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use pos_db::init_memory_database;

    /// Port 1 is reserved and never listening, so `is_online()` resolves to `Offline`
    /// immediately. Pointing at a conventional dev port instead makes these tests pass or
    /// fail depending on whether a backend happens to be running on the machine.
    const UNREACHABLE_BACKEND: &str = "http://127.0.0.1:1";

    fn setup() -> (Arc<ApiClient>, Arc<Database>) {
        let api = Arc::new(ApiClient::new(UNREACHABLE_BACKEND));
        let db = Arc::new(init_memory_database().unwrap());
        (api, db)
    }

    #[test]
    fn test_sync_service_creation() {
        let (api, db) = setup();
        let service = SyncService::new(api, db, 10);
        assert_eq!(service.interval().as_secs(), 600);
    }

    #[test]
    fn test_sync_service_with_defaults() {
        let (api, db) = setup();
        let service = SyncService::with_defaults(api, db);
        assert_eq!(
            service.interval().as_secs(),
            DEFAULT_SYNC_INTERVAL_MINUTES * 60
        );
    }

    #[test]
    fn test_needs_sync_initially() {
        let (api, db) = setup();
        let service = SyncService::new(api, db, 10);

        // Should need sync initially (no data)
        assert!(service.needs_sync().unwrap());
    }

    #[test]
    fn test_get_sync_status_empty() {
        let (api, db) = setup();
        let service = SyncService::new(api, db, 10);

        let statuses = service.get_sync_status().unwrap();
        assert_eq!(statuses.len(), 3); // products, categories, operators

        for status in &statuses {
            assert_eq!(status.time_ago, "never");
            assert_eq!(status.record_count, 0);
        }
    }

    #[test]
    fn test_sync_event_clone() {
        let event = SyncEvent::CatalogUpdated {
            products_count: 100,
            categories_count: 10,
        };

        let cloned = event.clone();
        match cloned {
            SyncEvent::CatalogUpdated {
                products_count,
                categories_count,
            } => {
                assert_eq!(products_count, 100);
                assert_eq!(categories_count, 10);
            }
            _ => panic!("Wrong event type"),
        }
    }

    #[test]
    fn test_sync_event_debug() {
        let event = SyncEvent::Started;
        let debug_str = format!("{:?}", event);
        assert_eq!(debug_str, "Started");

        let event = SyncEvent::Failed("Network error".to_string());
        let debug_str = format!("{:?}", event);
        assert!(debug_str.contains("Network error"));
    }

    #[tokio::test]
    async fn test_sync_service_single_run() {
        let (api, db) = setup();
        let service = SyncService::new(api, db, 10);

        let (tx, mut rx) = broadcast::channel(16);

        // This will fail because there's no server, but should not panic
        // and should send appropriate events
        tokio::spawn(async move {
            service.run_sync_cycle(&tx).await;
        });

        // Should receive Started event
        let event = rx.recv().await;
        assert!(event.is_ok());
        match event.unwrap() {
            SyncEvent::Started => {}
            _ => panic!("Expected Started event"),
        }

        // The backend is unreachable, so the cycle stops at the online check.
        let event = rx.recv().await;
        assert!(event.is_ok());
        match event.unwrap() {
            SyncEvent::Offline => {}
            e => panic!("Expected Offline event, got {e:?}"),
        }
    }

    #[test]
    fn test_sync_status_fields() {
        let status = SyncStatus {
            resource: "products".to_string(),
            last_sync: Some("2025-01-01T00:00:00Z".to_string()),
            record_count: 100,
            time_ago: "1 day ago".to_string(),
        };

        assert_eq!(status.resource, "products");
        assert_eq!(status.record_count, 100);
        assert_eq!(status.time_ago, "1 day ago");
    }
}
