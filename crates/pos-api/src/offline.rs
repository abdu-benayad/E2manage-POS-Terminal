//! The offline queue upload — `POST /api/pos/offline/upload`.
//!
//! **The till's only write path, and it is closed at two independent points**: the route is
//! `requireTerminalAuth` + `attendedOperatorAuthMiddleware` (`offline.controller.ts:109`), and a
//! cookieless POST is refused 403 by CSRF before it reaches either. Repaired here anyway; the
//! reachability verdict is in `till/doc/till-consumer-surface-audit`.
//!
//! One route, two callers: the queue drain (`OfflineService`) and the conflict resolver
//! (`ConflictService`), which sends the same payload with `force` set. They used to be two
//! separately-declared request types in two files, differing only in that flag and in whether the
//! catalog ETag was carried.

use anyhow::Result;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::client::{ApiClient, Enveloped};
use pos_models::OperatorId;

/// `true` only when the value is `false` — the `skip_serializing_if` predicate that keeps `force`
/// off the wire for an ordinary upload, so adding the flag did not change the queue drain's body.
fn is_false(value: &bool) -> bool {
    !*value
}

/// A queued transaction being uploaded.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UploadOfflineTransactionRequest {
    pub offline_id: String,
    pub transaction_number: Option<String>,
    pub transaction_type: String,
    pub items: serde_json::Value,
    pub payments: serde_json::Value,
    pub subtotal: Decimal,
    pub tax_total: Decimal,
    pub discount_total: Decimal,
    pub grand_total: Decimal,
    pub customer_id: Option<String>,
    pub customer_name: Option<String>,
    pub shift_id: Option<String>,
    pub operator_id: Option<OperatorId>,
    pub terminal_id: Option<String>,
    pub receipt_number: Option<String>,
    pub notes: Option<String>,
    pub created_at: String,
    /// Catalog ETag when the transaction was taken, for price-version tracking.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub catalog_etag: Option<String>,
    /// Overrides the platform's conflict checks. Absent unless the conflict resolver set it.
    #[serde(skip_serializing_if = "is_false")]
    pub force: bool,
}

/// The server-side identity the uploaded transaction was given.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UploadOfflineTransactionResponse {
    pub id: String,
    #[serde(default)]
    pub transaction_number: Option<String>,
}

impl ApiClient {
    /// Uploads one queued transaction. `POST /api/pos/offline/upload`
    pub async fn upload_offline_transaction(
        &self,
        request: &UploadOfflineTransactionRequest,
    ) -> Result<UploadOfflineTransactionResponse> {
        let response: Enveloped<_> = self.post("/api/pos/offline/upload", request).await?;
        Ok(response.into_inner())
    }
}
