//! HTTP Client - Backend API communication
//!
//! Provides a reqwest-based HTTP client for communicating with the E2Manage backend.
//! Handles authentication tokens, timeouts, and basic error handling.

use anyhow::{anyhow, Result};
use reqwest::{
    header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE, IF_NONE_MATCH},
    Client, Response, StatusCode,
};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::{debug, error, warn};

/// Result of an online connectivity check
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OnlineStatus {
    /// Server reachable and (if authenticated) session/tenant valid
    Online,
    /// Server unreachable (network down, DNS failure, timeout)
    Offline,
    /// Server reachable but returned 401/403 (token expired, tenant deleted)
    AuthRejected,
}

impl OnlineStatus {
    /// Returns true only when fully online and authenticated
    pub fn is_online(&self) -> bool {
        matches!(self, OnlineStatus::Online)
    }
}

/// Custom header for terminal authentication
pub const HEADER_TERMINAL_TOKEN: &str = "X-Terminal-Token";

/// Custom header for terminal identification
pub const HEADER_TERMINAL_ID: &str = "X-Terminal-ID";

/// API error response from server
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiErrorResponse {
    pub message: String,
    #[serde(default)]
    pub error_code: Option<String>,
    #[serde(default)]
    pub details: Option<serde_json::Value>,
}

/// API response envelope from backend
/// The backend wraps all responses in {success, message?, data}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiEnvelope<T> {
    pub success: bool,
    #[serde(default)]
    pub message: Option<String>,
    pub data: Option<T>,
}

/// Result of a GET request with ETag support
#[derive(Debug)]
pub enum GetResult<T> {
    /// Data was returned (200 OK)
    Data { data: T, etag: Option<String> },
    /// Not modified (304), use cached data
    NotModified,
}

/// API Client for E2Manage backend communication
pub struct ApiClient {
    client: Client,
    base_url: String,
    session_token: Arc<RwLock<Option<String>>>,
    terminal_id: Arc<RwLock<Option<String>>>,
}

impl ApiClient {
    /// Creates a new API client
    ///
    /// # Arguments
    ///
    /// * `base_url` - Base URL of the E2Manage API (e.g., "https://api.e2manage.com")
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let client = ApiClient::new("https://api.e2manage.com");
    /// ```
    pub fn new(base_url: &str) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .connect_timeout(Duration::from_secs(10))
            .pool_max_idle_per_host(5)
            .build()
            .expect("Failed to create HTTP client");

        Self {
            client,
            base_url: base_url.trim_end_matches('/').to_string(),
            session_token: Arc::new(RwLock::new(None)),
            terminal_id: Arc::new(RwLock::new(None)),
        }
    }

    /// Sets the session token for authenticated requests
    pub async fn set_token(&self, token: String) {
        let mut guard = self.session_token.write().await;
        *guard = Some(token);
    }

    /// Gets the current session token
    pub async fn get_token(&self) -> Option<String> {
        self.session_token.read().await.clone()
    }

    /// Clears the session token (logout)
    pub async fn clear_token(&self) {
        let mut guard = self.session_token.write().await;
        *guard = None;
    }

    /// Sets the terminal ID for identification headers
    pub async fn set_terminal_id(&self, terminal_id: String) {
        let mut guard = self.terminal_id.write().await;
        *guard = Some(terminal_id);
    }

    /// Gets the terminal ID
    pub async fn get_terminal_id(&self) -> Option<String> {
        self.terminal_id.read().await.clone()
    }

    /// Checks if the client is authenticated
    pub async fn is_authenticated(&self) -> bool {
        self.get_token().await.is_some()
    }

    /// Builds headers for requests
    async fn build_headers(&self) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));

        if let Some(token) = self.get_token().await {
            // Add standard Authorization header (required by backend)
            let bearer = format!("Bearer {}", token);
            if let Ok(value) = HeaderValue::from_str(&bearer) {
                headers.insert(AUTHORIZATION, value);
            }
            // Also add custom X-Terminal-Token for backwards compatibility
            if let Ok(value) = HeaderValue::from_str(&token) {
                headers.insert(HEADER_TERMINAL_TOKEN, value);
            }
        }

        if let Some(terminal_id) = self.get_terminal_id().await {
            if let Ok(value) = HeaderValue::from_str(&terminal_id) {
                headers.insert(HEADER_TERMINAL_ID, value);
            }
        }

        headers
    }

    /// Makes a GET request
    ///
    /// # Arguments
    ///
    /// * `path` - API path (e.g., "/api/pos/sync/catalog")
    ///
    /// # Returns
    ///
    /// Deserialized response body
    pub async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        let url = format!("{}{}", self.base_url, path);
        debug!("GET {}", url);

        let response = self
            .client
            .get(&url)
            .headers(self.build_headers().await)
            .send()
            .await
            .map_err(|e| self.handle_request_error(e))?;

        self.handle_response(response).await
    }

    /// Makes a GET request with ETag support for caching
    ///
    /// # Arguments
    ///
    /// * `path` - API path
    /// * `etag` - Optional ETag from previous request
    ///
    /// # Returns
    ///
    /// `GetResult::Data` with new data and ETag, or `GetResult::NotModified` if unchanged
    pub async fn get_with_etag<T: DeserializeOwned>(
        &self,
        path: &str,
        etag: Option<&str>,
    ) -> Result<GetResult<T>> {
        let url = format!("{}{}", self.base_url, path);
        debug!("GET {} (ETag: {:?})", url, etag);

        let mut request = self.client.get(&url).headers(self.build_headers().await);

        // Add If-None-Match header if we have an ETag
        if let Some(etag_value) = etag {
            if let Ok(value) = HeaderValue::from_str(etag_value) {
                request = request.header(IF_NONE_MATCH, value);
            }
        }

        let response = request
            .send()
            .await
            .map_err(|e| self.handle_request_error(e))?;

        // Handle 304 Not Modified
        if response.status() == StatusCode::NOT_MODIFIED {
            debug!("304 Not Modified for {}", path);
            return Ok(GetResult::NotModified);
        }

        // Extract ETag from response headers
        let new_etag = response
            .headers()
            .get("etag")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());

        let data: T = self.handle_response(response).await?;

        Ok(GetResult::Data {
            data,
            etag: new_etag,
        })
    }

    /// Makes a conditional GET request with ETag support, unwrapping the API envelope
    ///
    /// Use this for endpoints that return {success, data} wrapper and support ETag caching.
    pub async fn get_with_etag_envelope<T: DeserializeOwned>(
        &self,
        path: &str,
        etag: Option<&str>,
    ) -> Result<GetResult<T>> {
        let url = format!("{}{}", self.base_url, path);
        debug!("GET (envelope) {} (ETag: {:?})", url, etag);

        let mut request = self.client.get(&url).headers(self.build_headers().await);

        // Add If-None-Match header if we have an ETag
        if let Some(etag_value) = etag {
            if let Ok(value) = HeaderValue::from_str(etag_value) {
                request = request.header(IF_NONE_MATCH, value);
            }
        }

        let response = request
            .send()
            .await
            .map_err(|e| self.handle_request_error(e))?;

        // Handle 304 Not Modified
        if response.status() == StatusCode::NOT_MODIFIED {
            debug!("304 Not Modified for {}", path);
            return Ok(GetResult::NotModified);
        }

        // Extract ETag from response headers
        let new_etag = response
            .headers()
            .get("etag")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());

        // Unwrap the envelope
        let envelope: ApiEnvelope<T> = self.handle_response(response).await?;

        if !envelope.success {
            return Err(anyhow!(
                "API error: {}",
                envelope
                    .message
                    .unwrap_or_else(|| "Unknown error".to_string())
            ));
        }

        let data = envelope
            .data
            .ok_or_else(|| anyhow!("API returned success but no data"))?;

        Ok(GetResult::Data {
            data,
            etag: new_etag,
        })
    }

    /// Makes a GET request and unwraps the API envelope response
    /// Use this for endpoints that return {success, data} wrapper
    pub async fn get_envelope<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        let url = format!("{}{}", self.base_url, path);
        debug!("GET (envelope) {}", url);

        let response = self
            .client
            .get(&url)
            .headers(self.build_headers().await)
            .send()
            .await
            .map_err(|e| self.handle_request_error(e))?;

        let envelope: ApiEnvelope<T> = self.handle_response(response).await?;

        if !envelope.success {
            return Err(anyhow!(
                "API error: {}",
                envelope
                    .message
                    .unwrap_or_else(|| "Unknown error".to_string())
            ));
        }

        envelope
            .data
            .ok_or_else(|| anyhow!("API returned success but no data"))
    }

    /// Makes a POST request
    ///
    /// # Arguments
    ///
    /// * `path` - API path
    /// * `body` - Request body to serialize as JSON
    ///
    /// # Returns
    ///
    /// Deserialized response body
    pub async fn post<T, R>(&self, path: &str, body: &T) -> Result<R>
    where
        T: Serialize,
        R: DeserializeOwned,
    {
        let url = format!("{}{}", self.base_url, path);
        debug!("POST {}", url);

        let response = self
            .client
            .post(&url)
            .headers(self.build_headers().await)
            .json(body)
            .send()
            .await
            .map_err(|e| self.handle_request_error(e))?;

        self.handle_response(response).await
    }

    /// Makes a POST request and unwraps the API envelope response
    /// Use this for endpoints that return {success, data} wrapper
    pub async fn post_envelope<T, R>(&self, path: &str, body: &T) -> Result<R>
    where
        T: Serialize,
        R: DeserializeOwned,
    {
        let url = format!("{}{}", self.base_url, path);
        debug!("POST (envelope) {}", url);

        let response = self
            .client
            .post(&url)
            .headers(self.build_headers().await)
            .json(body)
            .send()
            .await
            .map_err(|e| self.handle_request_error(e))?;

        let envelope: ApiEnvelope<R> = self.handle_response(response).await?;

        if !envelope.success {
            return Err(anyhow!(
                "API error: {}",
                envelope
                    .message
                    .unwrap_or_else(|| "Unknown error".to_string())
            ));
        }

        envelope
            .data
            .ok_or_else(|| anyhow!("API returned success but no data"))
    }

    /// Makes a PUT request
    pub async fn put<T, R>(&self, path: &str, body: &T) -> Result<R>
    where
        T: Serialize,
        R: DeserializeOwned,
    {
        let url = format!("{}{}", self.base_url, path);
        debug!("PUT {}", url);

        let response = self
            .client
            .put(&url)
            .headers(self.build_headers().await)
            .json(body)
            .send()
            .await
            .map_err(|e| self.handle_request_error(e))?;

        self.handle_response(response).await
    }

    /// Makes a DELETE request
    pub async fn delete<R: DeserializeOwned>(&self, path: &str) -> Result<R> {
        let url = format!("{}{}", self.base_url, path);
        debug!("DELETE {}", url);

        let response = self
            .client
            .delete(&url)
            .headers(self.build_headers().await)
            .send()
            .await
            .map_err(|e| self.handle_request_error(e))?;

        self.handle_response(response).await
    }

    /// Checks if the backend is reachable and the terminal session is valid.
    ///
    /// Returns [`OnlineStatus`] to distinguish three states:
    /// - `Online`: server reachable and (if authenticated) tenant valid
    /// - `Offline`: server unreachable (network/DNS/timeout)
    /// - `AuthRejected`: server reachable but returned 401/403 (expired token, deleted tenant)
    ///
    /// When a session token exists, uses the authenticated `/api/pos/sync/status`
    /// endpoint which validates: token + session + tenant still active.
    /// Before pairing (no token), falls back to `/api/health` so the pairing
    /// screen can tell whether the server is reachable.
    pub async fn is_online(&self) -> OnlineStatus {
        if self.get_token().await.is_some() {
            // Authenticated check: validates token, session, and tenant
            let url = format!("{}/api/pos/sync/status", self.base_url);
            let headers = self.build_headers().await;
            match self
                .client
                .get(&url)
                .headers(headers)
                .timeout(Duration::from_secs(5))
                .send()
                .await
            {
                Ok(response) => {
                    let status = response.status();
                    if status.is_success() {
                        OnlineStatus::Online
                    } else if status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN
                    {
                        warn!("Online check: auth rejected ({})", status);
                        OnlineStatus::AuthRejected
                    } else {
                        // Other server error (500, 503, etc.) — server is reachable but unhealthy
                        debug!("Online check: server error ({})", status);
                        OnlineStatus::Offline
                    }
                }
                Err(e) => {
                    debug!("Authenticated online check failed: {}", e);
                    OnlineStatus::Offline
                }
            }
        } else {
            // Pre-pairing: just check server reachability
            let url = format!("{}/api/health", self.base_url);
            match self
                .client
                .get(&url)
                .timeout(Duration::from_secs(5))
                .send()
                .await
            {
                Ok(response) if response.status().is_success() => OnlineStatus::Online,
                Ok(_) => OnlineStatus::Offline,
                Err(e) => {
                    debug!("Health check failed: {}", e);
                    OnlineStatus::Offline
                }
            }
        }
    }

    /// Handles request errors (connection issues, timeouts, etc.)
    fn handle_request_error(&self, error: reqwest::Error) -> anyhow::Error {
        if error.is_timeout() {
            warn!("Request timeout: {}", error);
            anyhow!("Request timeout - server not responding")
        } else if error.is_connect() {
            warn!("Connection error: {}", error);
            anyhow!("Connection error - unable to reach server")
        } else {
            error!("Request error: {}", error);
            anyhow!("Request failed: {}", error)
        }
    }

    /// Handles response status and deserialization
    async fn handle_response<T: DeserializeOwned>(&self, response: Response) -> Result<T> {
        let status = response.status();
        let url = response.url().to_string();

        if status.is_success() {
            let data = response
                .json()
                .await
                .map_err(|e| anyhow!("Failed to parse response: {}", e))?;
            Ok(data)
        } else {
            // Try to parse error response
            let error_text = response.text().await.unwrap_or_default();

            match serde_json::from_str::<ApiErrorResponse>(&error_text) {
                Ok(api_error) => {
                    error!(
                        "API error {} at {}: {}",
                        status.as_u16(),
                        url,
                        api_error.message
                    );
                    Err(anyhow!(
                        "API Error ({}): {}",
                        status.as_u16(),
                        api_error.message
                    ))
                }
                Err(_) => {
                    error!("HTTP error {} at {}: {}", status.as_u16(), url, error_text);
                    Err(anyhow!(
                        "HTTP Error {}: {}",
                        status.as_u16(),
                        if error_text.is_empty() {
                            status.canonical_reason().unwrap_or("Unknown error")
                        } else {
                            &error_text
                        }
                    ))
                }
            }
        }
    }
}

impl Clone for ApiClient {
    fn clone(&self) -> Self {
        Self {
            client: self.client.clone(),
            base_url: self.base_url.clone(),
            session_token: Arc::clone(&self.session_token),
            terminal_id: Arc::clone(&self.terminal_id),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_api_client_new() {
        let client = ApiClient::new("https://api.example.com/");
        // Trailing slash should be trimmed
        assert_eq!(client.base_url, "https://api.example.com");
    }

    #[tokio::test]
    async fn test_token_management() {
        let client = ApiClient::new("https://api.example.com");

        // Initially no token
        assert!(client.get_token().await.is_none());
        assert!(!client.is_authenticated().await);

        // Set token
        client.set_token("test-token-123".to_string()).await;
        assert_eq!(client.get_token().await, Some("test-token-123".to_string()));
        assert!(client.is_authenticated().await);

        // Clear token
        client.clear_token().await;
        assert!(client.get_token().await.is_none());
        assert!(!client.is_authenticated().await);
    }

    #[tokio::test]
    async fn test_terminal_id_management() {
        let client = ApiClient::new("https://api.example.com");

        // Initially no terminal ID
        assert!(client.get_terminal_id().await.is_none());

        // Set terminal ID
        client.set_terminal_id("TERM-001".to_string()).await;
        assert_eq!(client.get_terminal_id().await, Some("TERM-001".to_string()));
    }

    #[tokio::test]
    async fn test_headers_with_auth() {
        let client = ApiClient::new("https://api.example.com");
        client.set_token("secret-token".to_string()).await;
        client.set_terminal_id("TERM-001".to_string()).await;

        let headers = client.build_headers().await;

        assert!(headers.contains_key(CONTENT_TYPE));
        assert!(headers.contains_key(HEADER_TERMINAL_TOKEN));
        assert!(headers.contains_key(HEADER_TERMINAL_ID));
    }

    #[test]
    fn test_client_clone() {
        let client = ApiClient::new("https://api.example.com");
        let cloned = client.clone();
        assert_eq!(client.base_url, cloned.base_url);
    }
}
