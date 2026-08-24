//! HTTP Client - Backend API communication
//!
//! Provides a reqwest-based HTTP client for communicating with the E2Manage backend.
//! Handles authentication tokens, timeouts, and basic error handling.

use anyhow::Result;
use reqwest::{
    header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE, IF_NONE_MATCH},
    Client, Response, StatusCode,
};
use serde::{de, de::DeserializeOwned, Deserialize, Deserializer, Serialize};

use crate::failure::{ApiFailure, ServerErrorCode};
use crate::refusal_details::RefusalDetails;
use crate::session::SessionToken;
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

/// The header `attendedOperatorAuthMiddleware` reads.
///
/// A **different credential** from [`HEADER_TERMINAL_TOKEN`], not a second spelling of it: the
/// terminal token says *this device is enrolled* and lives 24 hours; this one says *this person
/// proved their PIN at this device* and lives 12. `POST /api/pos/offline/upload` and the six
/// `/api/pos/till/*` routes mount `requireTerminalAuth, attendedOperatorAuthMiddleware` and demand
/// both. The till sent only the first, so every write it ever attempted answered 401
/// `POS_OPERATOR_SESSION_REQUIRED`.
pub const HEADER_OPERATOR_TOKEN: &str = "X-Operator-Token";

/// API error response from server
///
/// Mirrors the envelope `respondWithApiError` writes
/// (`wadi-dms-api/src/shared/types/api-error.type.ts:100-112`):
/// `{ success, message, error: { code, message }, errors? }`.
///
/// Both optional fields were previously declared at the wrong path — `error_code` at the top
/// level where the server nests it under `error.code`, and `details` under a name the server
/// never emits — so with `#[serde(default)]` on each, **both had always deserialized to `None`**.
/// A till-side bug against a correct server; nothing read either field, which is why it survived.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiErrorResponse {
    pub message: String,
    #[serde(default)]
    pub error: Option<ApiErrorDetail>,
    /// Per-field validation failures, sent by the 400 and 422 paths only (`ApiError.errors?:
    /// any[]`).
    #[serde(default)]
    pub errors: Option<Vec<serde_json::Value>>,
}

/// The machine-readable half of the error envelope.
///
/// `details` is nested **here**, beside the code, not at the top level — and it is typed by the
/// code it travels with. See [`RefusalDetails`].
#[derive(Debug)]
pub struct ApiErrorDetail {
    /// See [`ServerErrorCode`] for how much this is currently worth.
    pub code: ServerErrorCode,
    pub message: String,
    /// The figures this refusal carried, if it carried any and they could be read.
    ///
    /// `None` covers three different things — the code carries no payload, the payload was
    /// omitted, the payload did not match — and [`RefusalDetails::read`] logs which. They are one
    /// value here because a caller has the same move in all three: act on the code alone.
    pub details: Option<RefusalDetails>,
}

/// The envelope as it arrives, before `details` is typed against its code.
///
/// Private, and the reason [`ApiErrorDetail`] does not derive `Deserialize`: `details` is
/// discriminated by its **sibling** `code` field, which no serde attribute expresses. So the
/// untyped value is read once, here, and converted immediately — a `serde_json::Value` never
/// reaches a caller.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WireErrorDetail {
    code: ServerErrorCode,
    message: String,
    #[serde(default)]
    details: Option<serde_json::Value>,
}

impl<'de> Deserialize<'de> for ApiErrorDetail {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        let wire = WireErrorDetail::deserialize(deserializer)?;
        // Total, and deliberately not a `?`: a figure the till could not read must not cost it
        // the refusal that figure travelled on.
        let details = RefusalDetails::read(&wire.code, wire.details.as_ref());
        Ok(Self {
            code: wire.code,
            message: wire.message,
            details,
        })
    }
}

/// API response envelope from backend
/// The backend wraps all responses in {success, message?, data}
///
/// **Kept only for `crates/pos-contract`**, which reads it directly at `tests/contract.rs:31, :449`
/// to assert the envelope's own shape. Every call site in this crate reads payloads through
/// [`Enveloped`] instead, which checks `success` and a missing `data` rather than handing the
/// caller two `Option`-shaped decisions it will not make. `pos-contract` is `[workspace] exclude`d,
/// so `cargo clippy --workspace` cannot tell you that deleting this broke it — `cd
/// crates/pos-contract && cargo test` is what answers.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiEnvelope<T> {
    pub success: bool,
    #[serde(default)]
    pub message: Option<String>,
    pub data: Option<T>,
}

/// A payload the platform carries inside its success envelope, `{ success, message, data }`.
///
/// The wire shape is declared **by the type of the thing being read**, at the one place the route
/// is named, rather than by which of two helper families the call site happened to pick. That is
/// the whole point: there is one helper family, so there is no wrong helper to choose.
///
/// # What it refuses, and why each refusal is load-bearing
///
/// - **`success: false`.** Only the deleted `_envelope` helpers ever checked this. A raw helper
///   against an enveloped route answering `{"success": false, ...}` deserialised whatever happened
///   to fit and reported nothing at all.
/// - **`success: true` with `data` absent or null.** The route promised a payload and did not send
///   one. Producing a default here would push a fabricated value into the domain.
/// - **A bare, unenveloped body.** Deserialisation fails on the missing `success` field. This is
///   the property that separates this design from "teach the client to recognise both shapes":
///   accepting both is how a contract stops being a contract, and it would mask the day the
///   platform stops wrapping.
///
/// A route that answers `{success, message}` with **no** `data` — `terminals/logout`
/// (`terminal.controller.ts:281`), `platform/report-version` (`:409`) — is therefore *not*
/// `Enveloped`. Reading its `success` at the top level with a plain DTO is correct, and wrapping it
/// here would turn a working call into `no `data` payload`. Both directions of this mistake are
/// live in the tree today; see `till/doc/till-consumer-surface-audit`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Enveloped<T>(T);

impl<T> Enveloped<T> {
    /// The payload, by value. Unwrapping is deliberate; there is no public field.
    pub fn into_inner(self) -> T {
        self.0
    }
}

impl<'de, T> Deserialize<'de> for Enveloped<T>
where
    T: DeserializeOwned,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        /// The envelope exactly as the platform writes it. `data` carries `#[serde(default)]` so
        /// that an absent payload reaches the check below with a message naming the route's
        /// promise, rather than serde's generic "missing field `data`".
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        #[serde(bound(deserialize = "T: DeserializeOwned"))]
        struct Wire<T> {
            success: bool,
            #[serde(default)]
            message: Option<String>,
            #[serde(default)]
            data: Option<T>,
        }

        let wire = Wire::<T>::deserialize(deserializer)?;

        if !wire.success {
            return Err(de::Error::custom(format!(
                "the platform refused the request: {}",
                wire.message.as_deref().unwrap_or("no message given"),
            )));
        }

        wire.data.map(Enveloped).ok_or_else(|| {
            de::Error::custom("the platform answered success with no `data` payload")
        })
    }
}

/// Result of a GET request with ETag support
#[derive(Debug)]
pub enum GetResult<T> {
    /// Data was returned (200 OK)
    Data { data: T, etag: Option<String> },
    /// Not modified (304), use cached data
    NotModified,
}

impl<T> GetResult<Enveloped<T>> {
    /// Unwraps the envelope inside a conditional-GET result, leaving `NotModified` untouched.
    ///
    /// A 304 carries no body — both ETag helpers return before `handle_response` is reached — so
    /// there is nothing to unwrap on that arm, and expressing that here rather than at each call
    /// site is what stops someone deriving a payload from a response that had none.
    pub fn into_inner(self) -> GetResult<T> {
        match self {
            GetResult::Data { data, etag } => GetResult::Data {
                data: data.into_inner(),
                etag,
            },
            GetResult::NotModified => GetResult::NotModified,
        }
    }
}

/// API Client for E2Manage backend communication
pub struct ApiClient {
    client: Client,
    base_url: String,
    session_token: Arc<RwLock<Option<String>>>,
    terminal_id: Arc<RwLock<Option<String>>>,
    /// The operator's session, when one is held. `SessionToken` and not `String`, so the blank
    /// that `build_headers` must never send is unrepresentable rather than checked.
    operator_token: Arc<RwLock<Option<SessionToken>>>,
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
            operator_token: Arc::new(RwLock::new(None)),
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

    /// Stops presenting this terminal's session — and the operator's with it.
    ///
    /// **Both, because an operator session cannot outlive the terminal session it was issued
    /// against.** Every till mount runs `terminalAuth` before `attendedOperatorAuth`, so an
    /// operator token held without a terminal token is a credential that can no longer be used
    /// and can still do harm: it is what the platform attributes a write to. Leaving it behind at
    /// logout means the next person to open this till writes under the last cashier's name — the
    /// same class of false attribution as putting a terminal id in `processed_by`.
    ///
    /// Clearing here rather than at each call site is deliberate. Both callers — terminal logout
    /// and de-registration — want both gone, and a rule enforced at the field is one a third
    /// caller cannot forget. **Renewal is not this**: [`Self::set_token`] replaces a terminal
    /// token within the same session lineage and must leave the cashier signed in, so it does
    /// not clear the operator's.
    pub async fn clear_token(&self) {
        let mut guard = self.session_token.write().await;
        *guard = None;
        drop(guard);
        self.clear_operator_token().await;
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

    /// Presents this operator's session on every subsequent request.
    ///
    /// Takes the token by value from a [`SessionToken`], so there is no path here for the empty
    /// string: absent and blank are different answers to `attendedOperatorAuthMiddleware`, and
    /// blank is the sloppier bug — it reads as a token that simply is not one the platform knows.
    pub async fn set_operator_token(&self, token: SessionToken) {
        *self.operator_token.write().await = Some(token);
    }

    /// Stops presenting an operator session. Called when the platform refuses one, and at sign-out.
    pub async fn clear_operator_token(&self) {
        *self.operator_token.write().await = None;
    }

    /// The operator session currently being presented, if any.
    pub async fn operator_token(&self) -> Option<SessionToken> {
        self.operator_token.read().await.clone()
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

        // The operator's session, and only when one is held. `attendedOperatorAuthMiddleware`
        // distinguishes an absent header (`POS_OPERATOR_SESSION_REQUIRED` — verify a PIN) from a
        // present one it cannot honour (`POS_OPERATOR_SESSION_INVALID` — discard and verify), so
        // sending an empty header would turn "nobody is signed in" into "the token you hold is
        // not ours". `SessionToken` cannot be blank, so that is closed by construction here.
        if let Some(token) = self.operator_token().await {
            if let Ok(value) = HeaderValue::from_str(token.expose()) {
                headers.insert(HEADER_OPERATOR_TOKEN, value);
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
    pub(crate) async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        Ok(self.get_or_failure(path).await?)
    }

    /// A GET whose failure keeps its type.
    ///
    /// The read half of [`Self::post_or_failure`], and it exists for the same reason: a caller
    /// that **branches** on the refusal needs the enum, not a string. `get_transaction_by_receipt`
    /// is the first — "the platform has no such receipt" and "the platform would not tell you" are
    /// different answers to the cashier holding the paper, and [`Self::get`] makes them the same
    /// `anyhow::Error`.
    ///
    /// There is deliberately no ETag sibling. [`Self::get_with_etag`] serves the catalogue sync,
    /// whose every caller reports the failure rather than branching on it; adding a door nothing
    /// walks through is how the `_envelope` helper family drifted.
    pub(crate) async fn get_or_failure<T: DeserializeOwned>(
        &self,
        path: &str,
    ) -> std::result::Result<T, ApiFailure> {
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
    pub(crate) async fn get_with_etag<T: DeserializeOwned>(
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
    pub(crate) async fn post<T, R>(&self, path: &str, body: &T) -> Result<R>
    where
        T: Serialize,
        R: DeserializeOwned,
    {
        Ok(self.post_or_failure(path, body).await?)
    }

    /// A POST whose failure keeps its type.
    ///
    /// [`Self::post`] is this, widened into `anyhow::Error` — one line, because `ApiFailure` is
    /// `Error + Send + Sync + 'static` and `?` converts for free. That widening exists so the ~30
    /// public signatures on this client did not all have to change at once, and it is the right
    /// default for a caller that only reports the failure.
    ///
    /// It is the wrong default for a caller that **branches** on it. Downcasting back out of
    /// `anyhow` to ask "was this a refusal, and which one" is a decision the type system should
    /// have made, and one a later edit can silently stop satisfying. `verify_operator_pin` is the
    /// first caller that needs the enum — three different 40x answers, three different things to
    /// tell the person at the till — and it takes this door instead.
    pub(crate) async fn post_or_failure<T, R>(
        &self,
        path: &str,
        body: &T,
    ) -> std::result::Result<R, ApiFailure>
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

    /// Makes a DELETE request whose response body carries nothing the caller reads.
    ///
    /// The status is still checked, and a refusal still goes through the platform's error envelope
    /// — only the success body is skipped. This replaces the
    /// `let _: serde_json::Value = self.delete(&url).await?;` idiom, which parsed a body solely to
    /// drop it and left the route's shape unstatable: `serde_json::Value` accepts anything, so it
    /// asserts nothing and cannot be `Enveloped`.
    ///
    /// Both routes it serves answer `{success, message}` with no `data`
    /// (`parking.controller.ts:280`, `cart.controller.ts:246`).
    pub(crate) async fn delete_discard(&self, path: &str) -> Result<()> {
        let url = format!("{}{}", self.base_url, path);
        debug!("DELETE (body discarded) {}", url);

        let response = self
            .client
            .delete(&url)
            .headers(self.build_headers().await)
            .send()
            .await
            .map_err(|e| self.handle_request_error(e))?;

        let status = response.status();
        if status.is_success() {
            return Ok(());
        }

        let request_url = response.url().to_string();
        let body = response.text().await.unwrap_or_default();
        Err(self.refusal_from_body(status, &request_url, &body).into())
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

    /// The request never produced an answer.
    ///
    /// One outcome, three log lines. Timeout, connect-failure and everything else are worth
    /// telling apart in a log and are **not** a distinction a caller acts on: all three mean "ask
    /// again later", which is what [`ApiFailure::Unreachable`] says and what `is_transient()`
    /// answers. Encoding it in the return type would hand every caller a decision none of them
    /// makes.
    ///
    /// Built through [`ApiFailure::unreachable`] by name rather than a `From` impl, because this
    /// is the one place that knows the error came from a request that never got a reply. A blanket
    /// conversion would also swallow reqwest's *decode* failures, which is the conflation the type
    /// exists to end — see that constructor's own doc comment.
    fn handle_request_error(&self, error: reqwest::Error) -> ApiFailure {
        if error.is_timeout() {
            warn!("Request timeout: {}", error);
        } else if error.is_connect() {
            warn!("Connection error: {}", error);
        } else {
            error!("Request error: {}", error);
        }

        ApiFailure::unreachable(error)
    }

    /// Reads a response into `T`, or says which of the three ways it failed.
    ///
    /// This replaces a body that flattened every non-2xx **and** every parse failure into one
    /// `anyhow!("API Error ({}): {}")`. A flattened string cannot be branched on, which is why
    /// `pos-services::sync_service::is_auth_error` substring-matches `"401"` to recover what was
    /// thrown away.
    ///
    /// **The body is taken as text and decoded with `serde_json`, never with `response.json()`.**
    /// `reqwest` folds decode failures into the same `Error` type as connection failures — only
    /// `is_decode()` separates them — so `response.json()` would route "the body arrived and did
    /// not match the contract" into the same value as "the server was not there".
    async fn handle_response<T: DeserializeOwned>(
        &self,
        response: Response,
    ) -> std::result::Result<T, ApiFailure> {
        let status = response.status();
        let url = response.url().to_string();

        // Taking the body as text is what keeps the two failures apart, so a body that cannot be
        // read at all has to be resolved here. It is `Unreachable`: the transfer was cut off part
        // way, which is a network event rather than a disagreement about shape.
        let body = response.text().await.map_err(|e| {
            warn!("response body could not be read from {}: {}", url, e);
            ApiFailure::unreachable(e)
        })?;

        if status.is_success() {
            return serde_json::from_str(&body).map_err(|e| self.unreadable(&url, status, e));
        }

        Err(self.refusal_from_body(status, &url, &body))
    }

    /// Turns a non-2xx body into the failure it actually is.
    ///
    /// Shared by [`ApiClient::handle_response`] and [`ApiClient::delete_discard`], so that
    /// discarding a success body does not also mean discarding a refusal.
    ///
    /// **An unparseable error body is [`ApiFailure::Unreadable`], not a `Refused` with a guessed
    /// code.** The server said something this client cannot read; claiming to know which refusal
    /// it was would be a fabrication the caller then branches on. The known case that lands here
    /// is a CSRF 403 — `{success, error: "..."}` is flat, while [`ApiErrorResponse`] requires a
    /// top-level `message` and reads `error` as an object — and calling that a contract breach is
    /// correct, because it is one.
    fn refusal_from_body(&self, status: StatusCode, url: &str, body: &str) -> ApiFailure {
        match serde_json::from_str::<ApiErrorResponse>(body) {
            Ok(envelope) => {
                let (code, message, details) = match envelope.error {
                    Some(detail) => (detail.code, detail.message, detail.details),
                    // The envelope parsed and carries no `error` object — a shape the platform
                    // really does emit. So it is a refusal, not a breach; the code is *absent*
                    // rather than unknown, and an empty `Unrecognised` says exactly that without
                    // inventing one. `is_recognised()` answers false either way.
                    None => (ServerErrorCode::from(String::new()), envelope.message, None),
                };

                error!("API refused {} at {}: {} ({})", status, url, message, code);
                ApiFailure::Refused {
                    status,
                    code,
                    message,
                    details,
                }
            }
            Err(e) => self.unreadable(url, status, e),
        }
    }

    /// A body arrived and did not match the contract.
    ///
    /// Logged at `error!` deliberately: this is never weather. It means the till and the platform
    /// disagree about the shape of an endpoint, so one of them has a bug — and a level people
    /// filter out is how such a disagreement survives to production.
    fn unreadable(&self, url: &str, status: StatusCode, error: serde_json::Error) -> ApiFailure {
        error!(
            "contract breach: {} answered {} with a body this client cannot read: {}",
            url, status, error
        );
        ApiFailure::Unreadable(error)
    }
}

impl Clone for ApiClient {
    fn clone(&self) -> Self {
        Self {
            client: self.client.clone(),
            base_url: self.base_url.clone(),
            session_token: Arc::clone(&self.session_token),
            terminal_id: Arc::clone(&self.terminal_id),
            // Shared, like the other two: a clone that held its own operator session would keep
            // presenting one after the original signed the cashier out.
            operator_token: Arc::clone(&self.operator_token),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A payload used only to prove `Enveloped<T>` reaches the real type through the envelope.
    #[derive(Debug, Deserialize, PartialEq, Eq)]
    #[serde(rename_all = "camelCase")]
    struct Session {
        session_token: String,
        expires_at: String,
    }

    fn read_enveloped(json: &str) -> Result<Session, serde_json::Error> {
        serde_json::from_str::<Enveloped<Session>>(json).map(Enveloped::into_inner)
    }

    #[test]
    fn enveloped_reads_the_payload_out_of_a_full_envelope() {
        let session = read_enveloped(
            r#"{"success":true,"message":"ok","data":{"sessionToken":"tok-1","expiresAt":"2026-01-01T00:00:00Z"}}"#,
        )
        .expect("a well-formed envelope must deserialize");

        assert_eq!(
            session,
            Session {
                session_token: "tok-1".to_string(),
                expires_at: "2026-01-01T00:00:00Z".to_string(),
            }
        );
    }

    /// `message` is optional on the wire — several routes omit it — so its absence must not be
    /// mistaken for a malformed envelope.
    #[test]
    fn enveloped_accepts_an_envelope_with_no_message() {
        let session = read_enveloped(
            r#"{"success":true,"data":{"sessionToken":"tok-2","expiresAt":"2026-01-02T00:00:00Z"}}"#,
        )
        .expect("message is optional");

        assert_eq!(session.session_token, "tok-2");
    }

    /// The defect this type exists to end: only the removed `_envelope` helpers ever checked
    /// `success`, so a raw helper against an enveloped route read a refusal as data.
    ///
    /// The server's own message must survive into the error. Dropping it rebuilds exactly the
    /// conflation `ApiFailure` was written to end — a refusal that reads like weather.
    #[test]
    fn enveloped_refuses_success_false_and_keeps_the_servers_message() {
        let error = read_enveloped(r#"{"success":false,"message":"Terminal not found"}"#)
            .expect_err("success:false is a refusal, not a payload");

        assert!(
            error.to_string().contains("Terminal not found"),
            "the server's message must survive into the error, got: {error}"
        );
    }

    /// A route that promised a payload and sent none. A default here would push a fabricated value
    /// into the domain, which is worse than the failure.
    #[test]
    fn enveloped_refuses_success_true_with_no_data() {
        for body in [r#"{"success":true}"#, r#"{"success":true,"data":null}"#] {
            let error = read_enveloped(body).expect_err("an absent payload is an error");
            assert!(
                error.to_string().contains("no `data` payload"),
                "the error must name the missing payload, got: {error}"
            );
        }
    }

    /// The property that separates this design from "teach the client to recognise both shapes".
    ///
    /// Accepting a bare body here would make `Enveloped<T>` silently equivalent to reading the
    /// payload raw, so the day the platform stopped wrapping would pass unnoticed — and a type
    /// that accepts both shapes records no contract at all.
    #[test]
    fn enveloped_refuses_a_bare_unenveloped_body() {
        read_enveloped(r#"{"sessionToken":"tok-3","expiresAt":"2026-01-03T00:00:00Z"}"#)
            .expect_err("a bare body is not an envelope");
    }

    /// `terminals/logout` and `platform/report-version` answer `{success, message}` with no `data`.
    /// Reading those with a plain DTO is correct; this test pins the boundary so that a later
    /// "tidy-up" wrapping them in `Enveloped` fails here rather than in production.
    #[test]
    fn a_no_data_route_is_read_with_a_plain_dto_not_with_enveloped() {
        #[derive(Debug, Deserialize)]
        struct Acknowledged {
            success: bool,
        }

        let body = r#"{"success":true,"message":"Version reported successfully"}"#;

        let plain: Acknowledged =
            serde_json::from_str(body).expect("a plain DTO reads the acknowledgement");
        assert!(plain.success);

        serde_json::from_str::<Enveloped<Acknowledged>>(body)
            .expect_err("Enveloped would demand a `data` this route never sends");
    }

    /// The operator session is presented only when one is held, and never as a blank header.
    ///
    /// `attendedOperatorAuthMiddleware` distinguishes the two: an absent header is
    /// `POS_OPERATOR_SESSION_REQUIRED` (nobody is signed in — verify a PIN) and a present one it
    /// cannot honour is `POS_OPERATOR_SESSION_INVALID` (discard what you hold). Sending `""` turns
    /// the first situation into the second, which is the sloppier bug: the till would report a
    /// credential problem to a cashier who has simply not signed in yet.
    #[tokio::test]
    async fn the_operator_token_is_sent_only_when_one_is_held() {
        let client = ApiClient::new("https://api.example.com");

        let headers = client.build_headers().await;
        assert!(
            !headers.contains_key(HEADER_OPERATOR_TOKEN),
            "with nobody signed in the header must be absent, not empty"
        );

        client
            .set_operator_token(SessionToken::new("op-sess-abc").expect("not blank"))
            .await;
        let headers = client.build_headers().await;
        assert_eq!(
            headers
                .get(HEADER_OPERATOR_TOKEN)
                .and_then(|value| value.to_str().ok()),
            Some("op-sess-abc")
        );

        // It is a *second* credential, not a replacement: the terminal token and the operator
        // token travel together, and a route behind both guards needs both.
        client.set_token("terminal-tok".to_string()).await;
        let headers = client.build_headers().await;
        assert_eq!(
            headers
                .get(HEADER_TERMINAL_TOKEN)
                .and_then(|value| value.to_str().ok()),
            Some("terminal-tok")
        );
        assert_eq!(
            headers
                .get(HEADER_OPERATOR_TOKEN)
                .and_then(|value| value.to_str().ok()),
            Some("op-sess-abc")
        );

        client.clear_operator_token().await;
        let headers = client.build_headers().await;
        assert!(!headers.contains_key(HEADER_OPERATOR_TOKEN));
        assert!(
            headers.contains_key(HEADER_TERMINAL_TOKEN),
            "signing the operator out must not un-enrol the device"
        );
    }

    /// Logging the terminal out takes the operator's session with it.
    ///
    /// The half of the two-credential change that was missing: `logout_terminal` and
    /// de-registration both cleared the terminal token and left `X-Operator-Token` presented, so
    /// the next person to open this till would have written under the last cashier's name.
    ///
    /// The control is the assertion above it — signing the *operator* out must not un-enrol the
    /// device. Without that pair, a `clear_token` that wiped everything and a
    /// `clear_operator_token` that wiped everything would both read as correct.
    #[tokio::test]
    async fn logging_the_terminal_out_stops_presenting_the_operator_too() {
        let client = ApiClient::new("https://api.example.com");
        client.set_token("terminal-tok".to_string()).await;
        client
            .set_operator_token(SessionToken::new("op-sess-abc").expect("not blank"))
            .await;

        client.clear_token().await;

        let headers = client.build_headers().await;
        assert!(
            !headers.contains_key(HEADER_OPERATOR_TOKEN),
            "an operator session cannot outlive the terminal session it was issued against"
        );
        assert!(!headers.contains_key(HEADER_TERMINAL_TOKEN));
        assert!(
            client.operator_token().await.is_none(),
            "and it must be gone from the client, not merely unsent"
        );
    }

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
