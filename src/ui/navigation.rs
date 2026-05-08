//! Navigation Module
//!
//! Provides feature-aware navigation for the POS terminal.
//! Integrates with FeatureService to check screen availability
//! before allowing navigation.

use pos_services::FeatureService;
use tracing::{debug, info, warn};

/// Navigation result indicating the outcome of a navigation attempt
#[derive(Debug, Clone)]
pub enum NavigationResult {
    /// Navigation succeeded
    Success { screen_id: String },
    /// Navigation blocked (feature disabled)
    Blocked { reason: String },
    /// Screen not found in feature library
    NotFound { screen_id: String },
}

/// Handle screen navigation with feature checks
///
/// The Navigator wraps FeatureService and provides methods to navigate
/// between screens while respecting feature enablement status.
///
/// # Example
///
/// ```rust,ignore
/// use pos_services::FeatureService;
/// use e2manage_pos_terminal::ui::navigation::{Navigator, NavigationResult};
/// use std::sync::Arc;
///
/// let db = Arc::new(Database::new("pos.db").unwrap());
/// let service = FeatureService::new(db);
/// let mut navigator = Navigator::new(service);
///
/// match navigator.navigate_to("checkout") {
///     NavigationResult::Success { screen_id } => {
///         window.set_current_screen(screen_id.into());
///     }
///     NavigationResult::Blocked { reason } => {
///         show_error(&reason);
///     }
///     NavigationResult::NotFound { screen_id } => {
///         error!("Screen not found: {}", screen_id);
///     }
/// }
/// ```
pub struct Navigator {
    feature_service: FeatureService,
    current_screen: String,
}

impl Navigator {
    /// Creates a new Navigator instance
    ///
    /// # Arguments
    ///
    /// * `feature_service` - FeatureService instance for checking screen availability
    pub fn new(feature_service: FeatureService) -> Self {
        Self {
            feature_service,
            current_screen: "splash".to_string(),
        }
    }

    /// Navigate to a screen with feature validation
    ///
    /// Checks if the target screen exists and its feature is enabled.
    /// Returns appropriate result based on the check.
    ///
    /// # Arguments
    ///
    /// * `screen_id` - The target screen identifier
    pub fn navigate_to(&mut self, screen_id: &str) -> NavigationResult {
        debug!(
            "Navigation requested: {} -> {}",
            self.current_screen, screen_id
        );

        // Check if screen exists and is enabled
        match self.feature_service.is_screen_enabled(screen_id) {
            Ok(true) => {
                info!("Navigating to: {}", screen_id);
                self.current_screen = screen_id.to_string();
                NavigationResult::Success {
                    screen_id: screen_id.to_string(),
                }
            }
            Ok(false) => {
                // Check if screen exists but feature is disabled
                match self.feature_service.get_screen_feature(screen_id) {
                    Ok(Some(feature_id)) => {
                        warn!(
                            "Navigation blocked: screen {} disabled (feature {} is off)",
                            screen_id, feature_id
                        );
                        NavigationResult::Blocked {
                            reason: format!("Feature '{}' is disabled", feature_id),
                        }
                    }
                    Ok(None) => {
                        warn!("Navigation blocked: screen {} not found", screen_id);
                        NavigationResult::NotFound {
                            screen_id: screen_id.to_string(),
                        }
                    }
                    Err(e) => {
                        warn!("Navigation error: {}", e);
                        NavigationResult::Blocked {
                            reason: format!("Error: {}", e),
                        }
                    }
                }
            }
            Err(e) => {
                warn!("Failed to check screen status: {}", e);
                NavigationResult::Blocked {
                    reason: format!("Error checking screen: {}", e),
                }
            }
        }
    }

    /// Navigate to next screen in flow
    ///
    /// Uses the `next_screen` field from the current screen's definition
    /// to determine where to navigate next.
    pub fn navigate_next(&mut self) -> NavigationResult {
        match self.feature_service.get_next_screen(&self.current_screen) {
            Ok(Some(next)) => self.navigate_to(&next),
            Ok(None) => {
                debug!("No next screen defined for: {}", self.current_screen);
                NavigationResult::NotFound {
                    screen_id: "".to_string(),
                }
            }
            Err(e) => NavigationResult::Blocked {
                reason: format!("Error: {}", e),
            },
        }
    }

    /// Navigate to feature entry point
    ///
    /// Looks up the entry point screen for the given feature and navigates to it.
    ///
    /// # Arguments
    ///
    /// * `feature_id` - The feature identifier
    pub fn navigate_to_feature(&mut self, feature_id: &str) -> NavigationResult {
        match self.feature_service.get_feature_entry_screen(feature_id) {
            Ok(Some(entry)) => self.navigate_to(&entry),
            Ok(None) => {
                warn!("Feature {} has no entry point", feature_id);
                NavigationResult::NotFound {
                    screen_id: feature_id.to_string(),
                }
            }
            Err(e) => NavigationResult::Blocked {
                reason: format!("Error: {}", e),
            },
        }
    }

    /// Get current screen ID
    pub fn current_screen(&self) -> &str {
        &self.current_screen
    }

    /// Set current screen without validation
    ///
    /// Used for system screens (splash, setup, login) that are not
    /// part of the feature library.
    ///
    /// # Arguments
    ///
    /// * `screen_id` - The screen identifier
    pub fn set_current_screen(&mut self, screen_id: &str) {
        self.current_screen = screen_id.to_string();
    }

    /// Get all navigable screens (for menu building)
    ///
    /// Returns a list of all screen IDs whose parent features are enabled.
    pub fn get_available_screens(&self) -> Vec<String> {
        self.feature_service
            .get_enabled_screen_ids()
            .unwrap_or_default()
    }

    /// Check if a screen button should be visible
    ///
    /// Returns true if the screen's parent feature is enabled.
    ///
    /// # Arguments
    ///
    /// * `screen_id` - The screen identifier
    pub fn is_screen_available(&self, screen_id: &str) -> bool {
        self.feature_service
            .is_screen_enabled(screen_id)
            .unwrap_or(false)
    }

    /// Check if a feature is enabled
    ///
    /// # Arguments
    ///
    /// * `feature_id` - The feature identifier
    pub fn is_feature_enabled(&self, feature_id: &str) -> bool {
        self.feature_service
            .is_feature_enabled(feature_id)
            .unwrap_or(false)
    }

    /// Get reference to the underlying FeatureService
    pub fn feature_service(&self) -> &FeatureService {
        &self.feature_service
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Note: Integration tests are in tests/navigation_tests.rs
    // These are unit tests for NavigationResult enum

    #[test]
    fn test_navigation_result_debug() {
        let success = NavigationResult::Success {
            screen_id: "test".to_string(),
        };
        assert!(format!("{:?}", success).contains("Success"));

        let blocked = NavigationResult::Blocked {
            reason: "test".to_string(),
        };
        assert!(format!("{:?}", blocked).contains("Blocked"));

        let not_found = NavigationResult::NotFound {
            screen_id: "test".to_string(),
        };
        assert!(format!("{:?}", not_found).contains("NotFound"));
    }
}
