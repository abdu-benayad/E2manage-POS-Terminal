//! Feature Service
//!
//! Service for managing feature access checks and navigation.
//! Provides high-level operations for checking if screens are enabled,
//! navigating between screens, and retrieving feature information.

use pos_db::Database;
use pos_models::{Feature, FeatureScreen};
use std::sync::Arc;
use thiserror::Error;
use tracing::{debug, warn};

/// Errors that can occur in feature service operations
#[derive(Debug, Error)]
pub enum FeatureError {
    #[error("Feature not found: {0}")]
    NotFound(String),

    #[error("Screen disabled: {0}")]
    ScreenDisabled(String),

    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),
}

/// Result type for feature operations
pub type FeatureResult<T> = Result<T, FeatureError>;

/// Service for managing feature access and navigation
///
/// This service provides methods to check if features and screens are enabled,
/// navigate between screens in a feature flow, and retrieve feature information.
///
/// # Example
///
/// ```rust,ignore
/// use pos_services::FeatureService;
/// use pos_db::Database;
/// use std::sync::Arc;
///
/// let db = Arc::new(Database::new("pos.db").unwrap());
/// let service = FeatureService::new(db);
///
/// // Check if a screen is enabled
/// if service.is_screen_enabled("checkout").unwrap() {
///     println!("Checkout is enabled");
/// }
///
/// // Get next screen in flow
/// if let Some(next) = service.get_next_screen("return-entry").unwrap() {
///     println!("Next screen: {}", next);
/// }
/// ```
pub struct FeatureService {
    db: Arc<Database>,
}

impl FeatureService {
    /// Creates a new FeatureService instance
    ///
    /// # Arguments
    ///
    /// * `db` - Arc-wrapped Database instance
    pub fn new(db: Arc<Database>) -> Self {
        Self { db }
    }

    /// Check if a screen is accessible (its feature is enabled)
    ///
    /// Returns true if the screen exists and its parent feature is enabled.
    /// Returns false if the screen doesn't exist or its feature is disabled.
    ///
    /// # Arguments
    ///
    /// * `screen_id` - The screen identifier to check
    pub fn is_screen_enabled(&self, screen_id: &str) -> FeatureResult<bool> {
        let result = self.db.is_screen_enabled(screen_id)?;
        if !result {
            debug!("Screen {} is not enabled", screen_id);
        }
        Ok(result)
    }

    /// Check if a feature is enabled
    ///
    /// Returns true if the feature exists and is enabled.
    /// Returns false if the feature doesn't exist or is disabled.
    ///
    /// # Arguments
    ///
    /// * `feature_id` - The feature identifier to check
    pub fn is_feature_enabled(&self, feature_id: &str) -> FeatureResult<bool> {
        match self.db.get_feature(feature_id)? {
            Some(feature) => Ok(feature.is_enabled),
            None => {
                debug!("Feature {} not found, treating as disabled", feature_id);
                Ok(false)
            }
        }
    }

    /// Get all enabled screen IDs
    ///
    /// Returns a list of screen IDs whose parent features are enabled.
    pub fn get_enabled_screen_ids(&self) -> FeatureResult<Vec<String>> {
        Ok(self.db.get_enabled_screen_ids()?)
    }

    /// Get next screen in navigation flow
    ///
    /// Returns the next screen in the navigation chain, if any.
    ///
    /// # Arguments
    ///
    /// * `current_screen` - The current screen identifier
    pub fn get_next_screen(&self, current_screen: &str) -> FeatureResult<Option<String>> {
        match self.db.get_screen(current_screen)? {
            Some(screen) => Ok(screen.next_screen),
            None => {
                debug!("Screen {} not found", current_screen);
                Ok(None)
            }
        }
    }

    /// Get entry point screen for a feature
    ///
    /// Returns the screen marked as the entry point for the feature.
    ///
    /// # Arguments
    ///
    /// * `feature_id` - The feature identifier
    pub fn get_feature_entry_screen(&self, feature_id: &str) -> FeatureResult<Option<String>> {
        match self.db.get_feature_entry_screen(feature_id)? {
            Some(screen) => Ok(Some(screen.screen_id)),
            None => {
                debug!("No entry screen found for feature {}", feature_id);
                Ok(None)
            }
        }
    }

    /// Get feature ID for a screen
    ///
    /// Returns the feature that owns this screen.
    ///
    /// # Arguments
    ///
    /// * `screen_id` - The screen identifier
    pub fn get_screen_feature(&self, screen_id: &str) -> FeatureResult<Option<String>> {
        Ok(self.db.get_screen_feature(screen_id)?)
    }

    /// Validate navigation target
    ///
    /// Checks if navigation to the target screen is allowed.
    /// Returns false if the screen is disabled or doesn't exist.
    ///
    /// # Arguments
    ///
    /// * `target_screen` - The target screen identifier
    pub fn can_navigate_to(&self, target_screen: &str) -> FeatureResult<bool> {
        if !self.is_screen_enabled(target_screen)? {
            warn!("Navigation blocked: screen {} is disabled", target_screen);
            return Ok(false);
        }
        Ok(true)
    }

    /// Get all features (enabled and disabled)
    ///
    /// Returns all features ordered by display_order.
    pub fn get_all_features(&self) -> FeatureResult<Vec<Feature>> {
        Ok(self.db.get_all_features()?)
    }

    /// Get enabled features
    ///
    /// Returns only enabled features ordered by display_order.
    pub fn get_enabled_features(&self) -> FeatureResult<Vec<Feature>> {
        Ok(self.db.get_enabled_features()?)
    }

    /// Get all screens for a feature
    ///
    /// Returns all screens belonging to a feature ordered by display_order.
    ///
    /// # Arguments
    ///
    /// * `feature_id` - The feature identifier
    pub fn get_feature_screens(&self, feature_id: &str) -> FeatureResult<Vec<FeatureScreen>> {
        Ok(self.db.get_feature_screens(feature_id)?)
    }

    /// Get a screen by ID
    ///
    /// Returns the screen details if found.
    ///
    /// # Arguments
    ///
    /// * `screen_id` - The screen identifier
    pub fn get_screen(&self, screen_id: &str) -> FeatureResult<Option<FeatureScreen>> {
        Ok(self.db.get_screen(screen_id)?)
    }

    /// Get a feature by ID
    ///
    /// Returns the feature details if found.
    ///
    /// # Arguments
    ///
    /// * `feature_id` - The feature identifier
    pub fn get_feature(&self, feature_id: &str) -> FeatureResult<Option<Feature>> {
        Ok(self.db.get_feature(feature_id)?)
    }
}
