//! Features API - Fetch terminal features from backend
//!
//! This module handles fetching the feature configuration for this terminal
//! from the E2Manage backend. Features determine which screens are available.

use anyhow::Result;
use pos_models::FeaturesResponse;
use tracing::debug;

use super::client::ApiClient;
use super::GetResult;

impl ApiClient {
    /// Fetches features configured for this terminal
    ///
    /// Uses ETag for conditional GET - returns `Ok(None)` if features
    /// haven't changed (304 Not Modified).
    ///
    /// # Arguments
    ///
    /// * `etag` - Optional ETag from previous request for caching
    ///
    /// # Returns
    ///
    /// * `Ok(Some(FeaturesResponse))` - New features data
    /// * `Ok(None)` - Not modified (304), use cached data
    /// * `Err(_)` - Request failed
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let api = ApiClient::new("https://api.e2manage.com");
    /// api.set_token("terminal-token".to_string()).await;
    ///
    /// // First request (no ETag)
    /// let features = api.get_features(None).await?;
    ///
    /// // Subsequent request with ETag
    /// let cached_etag = Some("\"abc123\"");
    /// match api.get_features(cached_etag.as_deref()).await? {
    ///     Some(features) => println!("Got {} features", features.features.len()),
    ///     None => println!("Features unchanged"),
    /// }
    /// ```
    pub async fn get_features(&self, etag: Option<&str>) -> Result<Option<FeaturesResponse>> {
        debug!("Fetching features (ETag: {:?})", etag);

        // Deliberately a raw read and NOT `Enveloped`: this is one of the four POS success
        // responses that carry no envelope at all. `feature.controller.ts:104` answers a bare
        // `{version, features, lastUpdated}`, and all four exceptions live in that one file.
        // Wrapping this would break a call site that is correct today.
        //
        // Its *error* shape is bare too — `{error: "Terminal not authenticated"}` at `:75`,
        // rather than the nested `error.{code, message}` `ApiErrorResponse` parses — so a refusal
        // here lands in `handle_response`'s raw-text branch. Out of scope for this issue; the
        // platform tracks it as `api-error-envelope-has-six-shapes`.
        let result: GetResult<FeaturesResponse> = self
            .get_with_etag("/api/pos/features/terminal", etag)
            .await?;

        match result {
            GetResult::NotModified => {
                debug!("Features not modified (304)");
                Ok(None)
            }
            GetResult::Data { data, etag: _ } => {
                debug!("Got {} features", data.features.len());
                Ok(Some(data))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_features_response_deserialization() {
        let json = r#"{
            "features": [
                {
                    "featureId": "checkout",
                    "name": "Checkout",
                    "nameAr": "نقطة البيع",
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
                },
                {
                    "featureId": "returns",
                    "name": "Returns",
                    "nameAr": "المرتجعات",
                    "isCore": false,
                    "enabled": true,
                    "displayOrder": 60,
                    "screens": [
                        {
                            "screenId": "return-entry",
                            "name": "Return Entry",
                            "isEntryPoint": true,
                            "nextScreen": "return-items",
                            "displayOrder": 10
                        },
                        {
                            "screenId": "return-items",
                            "name": "Return Items",
                            "isEntryPoint": false,
                            "displayOrder": 20
                        }
                    ]
                }
            ],
            "version": "abc123",
            "syncedAt": "2025-01-01T00:00:00Z"
        }"#;

        let response: FeaturesResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.features.len(), 2);
        assert_eq!(response.version, "abc123");

        let checkout = &response.features[0];
        assert_eq!(checkout.feature_id, "checkout");
        assert!(checkout.is_core);
        assert!(checkout.enabled);
        assert_eq!(checkout.screens.len(), 1);

        let returns = &response.features[1];
        assert_eq!(returns.feature_id, "returns");
        assert!(!returns.is_core);
        assert_eq!(returns.screens.len(), 2);

        let entry_screen = &returns.screens[0];
        assert!(entry_screen.is_entry_point);
        assert_eq!(entry_screen.next_screen, Some("return-items".to_string()));
    }
}
