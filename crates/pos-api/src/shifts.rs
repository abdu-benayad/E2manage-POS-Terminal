//! Shift endpoints — `/api/pos/shifts/*`.
//!
//! `authMiddleware` + `POS_CREATE` (`shift.controller.ts:68`, `:76`), not CSRF-exempt, so neither
//! is reachable by the till today.
//!
//! **Note the route name.** The till posted to `/api/pos/shifts`, which the shift router does not
//! serve — it registers `POST /start`, `POST /:shiftId/end` and `POST /:shiftId/z-report` and no
//! `POST /`. That was a third undiscovered 404, found by resolving every path the till names
//! against the router rather than checking the ones already reported. The URL correction is task
//! `05b`; this module holds the shape.

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::client::{ApiClient, Enveloped};
use crate::failure::ApiResult;
use pos_models::OperatorId;

/// Opening a shift.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartShiftRequest {
    pub shift_number: String,
    pub operator_id: OperatorId,
    pub terminal_id: String,
    pub opening_cash: Decimal,
    pub currency: String,
    pub started_at: String,
}

/// The platform's identifier for the opened shift.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartShiftResponse {
    pub id: String,
}

/// Closing a shift, with the drawer count.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EndShiftRequest {
    pub closing_cash: Decimal,
    pub expected_cash: Decimal,
    pub variance: Decimal,
    pub note: Option<String>,
    pub ended_at: String,
    pub denomination_breakdown: Option<Vec<DenominationDto>>,
}

/// One denomination of the closing count.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DenominationDto {
    pub label: String,
    pub value: Decimal,
    pub count: i32,
    pub subtotal: Decimal,
}

impl ApiClient {
    /// Opens a shift. `POST /api/pos/shifts/start`
    ///
    /// The till posted to `/api/pos/shifts`, which the shift router does not serve — it registers
    /// `POST /start`, `POST /:shiftId/end` and `POST /:shiftId/z-report`, and no `POST /`. A 404
    /// nobody had reported, found by resolving every path the till names against the router
    /// instead of checking the two that were already known.
    ///
    /// Corrected, and still unreachable: `authMiddleware` + `POS_CREATE` (`shift.controller.ts:68`),
    /// not on the CSRF exemption list.
    pub async fn start_shift(&self, request: &StartShiftRequest) -> ApiResult<StartShiftResponse> {
        let response: Enveloped<_> = self
            .post_or_failure("/api/pos/shifts/start", request)
            .await?;
        Ok(response.into_inner())
    }

    /// Closes a shift. `POST /api/pos/shifts/{shiftId}/end`
    pub async fn end_shift(&self, shift_id: &str, request: &EndShiftRequest) -> ApiResult<()> {
        let path = format!("/api/pos/shifts/{}/end", urlencoding::encode(shift_id));
        let _: Enveloped<serde::de::IgnoredAny> = self.post_or_failure(&path, request).await?;
        Ok(())
    }
}
