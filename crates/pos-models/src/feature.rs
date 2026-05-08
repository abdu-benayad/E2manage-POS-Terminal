//! Feature and Feature Screen models for Feature-Based Screen Library
//!
//! These models support dynamic feature enablement/disablement at the terminal level.

use serde::{Deserialize, Serialize};

/// Represents a feature that can be enabled/disabled on a terminal
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Feature {
    pub feature_id: String,
    pub name: String,
    #[serde(default)]
    pub name_ar: Option<String>,
    #[serde(default)]
    pub config_key: Option<String>,
    #[serde(default)]
    pub is_core: bool,
    #[serde(default)]
    pub is_enabled: bool,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub display_order: i32,
    #[serde(default)]
    pub updated_at: String,
}

/// Represents a screen that belongs to a feature
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct FeatureScreen {
    pub feature_id: String,
    pub screen_id: String,
    pub name: String,
    #[serde(default)]
    pub name_ar: Option<String>,
    #[serde(default)]
    pub is_entry_point: bool,
    #[serde(default)]
    pub next_screen: Option<String>,
    #[serde(default)]
    pub display_order: i32,
}

/// DTO for feature data from the API
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeatureDto {
    pub feature_id: String,
    pub name: String,
    #[serde(default)]
    pub name_ar: Option<String>,
    #[serde(default)]
    pub config_key: Option<String>,
    #[serde(default)]
    pub is_core: bool,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub display_order: i32,
    #[serde(default)]
    pub screens: Vec<FeatureScreenDto>,
}

/// DTO for feature screen data from the API
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeatureScreenDto {
    pub screen_id: String,
    pub name: String,
    #[serde(default)]
    pub name_ar: Option<String>,
    #[serde(default)]
    pub is_entry_point: bool,
    #[serde(default)]
    pub next_screen: Option<String>,
    #[serde(default)]
    pub display_order: i32,
}

/// Response from the features API endpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeaturesResponse {
    #[serde(default)]
    pub features: Vec<FeatureDto>,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub synced_at: String,
}

impl Feature {
    /// Create a new Feature with required fields
    pub fn new(feature_id: &str, name: &str) -> Self {
        Self {
            feature_id: feature_id.to_string(),
            name: name.to_string(),
            is_enabled: true,
            ..Default::default()
        }
    }
}

impl FeatureScreen {
    /// Create a new FeatureScreen with required fields
    pub fn new(feature_id: &str, screen_id: &str, name: &str) -> Self {
        Self {
            feature_id: feature_id.to_string(),
            screen_id: screen_id.to_string(),
            name: name.to_string(),
            ..Default::default()
        }
    }

    /// Create an entry point screen
    pub fn entry_point(feature_id: &str, screen_id: &str, name: &str) -> Self {
        Self {
            feature_id: feature_id.to_string(),
            screen_id: screen_id.to_string(),
            name: name.to_string(),
            is_entry_point: true,
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_feature_default() {
        let feature = Feature::default();
        assert!(feature.feature_id.is_empty());
        assert!(!feature.is_enabled);
        assert!(!feature.is_core);
    }

    #[test]
    fn test_feature_new() {
        let feature = Feature::new("returns", "Returns");
        assert_eq!(feature.feature_id, "returns");
        assert_eq!(feature.name, "Returns");
        assert!(feature.is_enabled);
    }

    #[test]
    fn test_feature_screen_entry_point() {
        let screen = FeatureScreen::entry_point("returns", "return-entry", "Return Entry");
        assert!(screen.is_entry_point);
        assert_eq!(screen.feature_id, "returns");
        assert_eq!(screen.screen_id, "return-entry");
    }

    #[test]
    fn test_feature_serde() {
        let json = r#"{
            "featureId": "returns",
            "name": "Returns",
            "nameAr": "المرتجعات",
            "configKey": "allowReturns",
            "isCore": false,
            "isEnabled": true,
            "icon": "rotate-ccw",
            "displayOrder": 60,
            "updatedAt": "2024-01-01T00:00:00Z"
        }"#;

        let feature: Feature = serde_json::from_str(json).unwrap();
        assert_eq!(feature.feature_id, "returns");
        assert_eq!(feature.name_ar, Some("المرتجعات".to_string()));
        assert!(feature.is_enabled);
    }

    #[test]
    fn test_features_response_serde() {
        let json = r#"{
            "features": [
                {
                    "featureId": "checkout",
                    "name": "Checkout",
                    "isCore": true,
                    "enabled": true,
                    "displayOrder": 10,
                    "screens": [
                        {
                            "screenId": "checkout-main",
                            "name": "Checkout",
                            "isEntryPoint": true,
                            "displayOrder": 10
                        }
                    ]
                }
            ],
            "version": "1.0.0",
            "syncedAt": "2024-01-01T00:00:00Z"
        }"#;

        let response: FeaturesResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.features.len(), 1);
        assert_eq!(response.features[0].feature_id, "checkout");
        assert!(response.features[0].enabled);
        assert_eq!(response.features[0].screens.len(), 1);
    }
}
