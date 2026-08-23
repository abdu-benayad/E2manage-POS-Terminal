//! End-of-day reporting — the Z-report.
//!
//! **The till and the platform model different operations here, and correcting the URL does not
//! reconcile them.** `generateZReport` (`reports.controller.ts:407-428`) takes
//! `{shiftId, terminalId, notes}`, *requires* `shiftId`, requires that shift to be `CLOSED`, and
//! computes the totals itself from that shift's transactions. What the till sends below is a
//! pre-aggregated day report across `total_shifts` shifts, with no `shiftId` at all.
//!
//! `shift.controller.ts:86` serves `POST /shifts/{shiftId}/z-report` — the same operation on a
//! second route, and the likelier target. Which side moves is a design question filed separately;
//! the route is `authMiddleware` + `POS_CREATE` and unreachable regardless.

use anyhow::Result;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::client::{ApiClient, Enveloped};

/// The till's end-of-day totals, as it currently submits them.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ZReportRequest {
    pub report_number: String,
    pub report_date: String,
    pub terminal_id: String,
    pub currency: String,
    pub total_shifts: u32,
    pub total_transactions: u32,
    pub gross_sales: Decimal,
    pub discounts: Decimal,
    pub returns: Decimal,
    pub net_sales: Decimal,
    pub tax_collected: Decimal,
    pub cash_total: Decimal,
    pub card_total: Decimal,
    pub wallet_total: Decimal,
    pub credit_total: Decimal,
    pub opening_float: Decimal,
    pub expected_cash: Decimal,
    pub actual_cash: Decimal,
    pub variance: Decimal,
    pub generated_at: String,
}

/// The platform's identifier for the stored report.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ZReportResponse {
    pub id: String,
}

impl ApiClient {
    /// Submits the end-of-day report.
    pub async fn submit_z_report(&self, request: &ZReportRequest) -> Result<ZReportResponse> {
        let response: Enveloped<_> = self.post("/api/pos/z-reports", request).await?;
        Ok(response.into_inner())
    }
}
