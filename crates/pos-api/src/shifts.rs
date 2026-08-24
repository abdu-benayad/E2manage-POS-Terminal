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

use std::fmt;

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::client::{ApiClient, Enveloped};
use crate::failure::ApiResult;
use pos_models::OperatorId;

// ============================================================================
// ServerShiftId
// ============================================================================

/// A shift identifier the platform issued cannot be blank.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[error("the platform's shift id cannot be blank; a shift it has not seen is `None`, not `\"\"`")]
pub struct BlankServerShiftId;

/// The platform's identifier for a shift — **not** the till's local one.
///
/// # Why this is a type and not a `String`
///
/// The till has two shift identifiers and they are not interchangeable. `shifts.id` is a local
/// primary key it mints itself; `shifts.server_id` is what `POST /till/shifts/start` returns. Only
/// the second may go on the wire: `POS_Transaction.shiftId` is a nullable `@db.Uuid` with a real
/// foreign key to `POS_Shift.id` (`prisma/pos.prisma:462`, `:560`), and the validator demands a
/// UUID when the field is present (`transaction.validator.ts:100`).
///
/// The till used to send the local one. Both are UUIDs, so no amount of *validation* separates
/// them — a shape check passes on the wrong value. Only a type does, and this is the socket that
/// refuses it: [`crate::CreateTransactionRequest::shift_id`] accepts nothing else.
///
/// # `Option`, and what the absence means
///
/// A shift opened while the network was down has no platform identifier, and neither does one
/// whose id failed to persist. That is a real state, not an error to paper over — so the field is
/// `Option<ServerShiftId>` and is **omitted from the request** when it is `None`. Omitting is
/// legal: the column is nullable and the validator marks the field `.optional()`. Sending the
/// local id instead would be refused as a malformed UUID, and inventing one would attach the sale
/// to somebody else's shift.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
#[serde(transparent)]
pub struct ServerShiftId(String);

impl ServerShiftId {
    /// Builds one from a value the platform issued, rejecting a blank.
    ///
    /// **There is no constructor that takes a local shift id, and that is the whole point.** The
    /// only two callers are the `start_shift` response and the read of `shifts.server_id`, which
    /// is the column that response is written to.
    pub fn new(raw: impl Into<String>) -> Result<Self, BlankServerShiftId> {
        let raw = raw.into();
        if raw.trim().is_empty() {
            Err(BlankServerShiftId)
        } else {
            Ok(Self(raw))
        }
    }

    /// The identifier itself, for a caller that must log or store it.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ServerShiftId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Deserializes an id and refuses a blank one at the boundary.
///
/// Hand-written for the same reason [`crate::SessionToken`]'s is: a derived impl would accept `""`
/// and build the sentinel this type exists to remove, one layer below every caller that would then
/// have to re-check it. A 2xx body carrying an empty shift id reads as `ApiFailure::Unreadable` —
/// the platform answered and the answer does not satisfy the contract.
impl<'de> Deserialize<'de> for ServerShiftId {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::new(String::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

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
///
/// **Every sale in this shift has to quote this value**, and until 2026-08-24 nothing read it
/// back: `shift_service` stored it in `shifts.server_id` and `transaction_service` put the local
/// id on the wire. Obtain the value, store it, then send something else — the same shape as the
/// operator token that was minted and `debug!`-logged away.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartShiftResponse {
    pub id: ServerShiftId,
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
            .post_or_failure("/api/pos/till/shifts/start", request)
            .await?;
        Ok(response.into_inner())
    }

    /// Closes a shift. `POST /api/pos/shifts/{shiftId}/end`
    pub async fn end_shift(&self, shift_id: &str, request: &EndShiftRequest) -> ApiResult<()> {
        let path = format!("/api/pos/till/shifts/{}/end", urlencoding::encode(shift_id));
        let _: Enveloped<serde::de::IgnoredAny> = self.post_or_failure(&path, request).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::Enveloped;

    /// The opened shift's identifier is read out of the envelope as a typed value.
    #[test]
    fn an_opened_shift_carries_the_platform_identifier() {
        let body = r#"{"success":true,"data":{"id":"9f1c0f6e-0000-4000-8000-000000000001"}}"#;

        let response: Enveloped<StartShiftResponse> =
            serde_json::from_str(body).expect("the platform's shape");

        assert_eq!(
            response.into_inner().id.as_str(),
            "9f1c0f6e-0000-4000-8000-000000000001"
        );
    }

    /// A blank identifier is refused at the boundary rather than carried inward.
    ///
    /// Reaching this means the platform answered 2xx with a body that does not satisfy the
    /// contract, which `handle_response` reads as `ApiFailure::Unreadable`. The alternative — a
    /// derived `Deserialize` accepting `""` — would store an empty `server_id` that every later
    /// sale would either send blank or, worse, fall back from onto the local id.
    #[test]
    fn a_blank_identifier_is_refused_where_it_arrives() {
        let body = r#"{"success":true,"data":{"id":""}}"#;

        serde_json::from_str::<Enveloped<StartShiftResponse>>(body)
            .expect_err("a blank shift id does not satisfy the contract");

        assert!(ServerShiftId::new("").is_err());
        assert!(ServerShiftId::new("  \t ").is_err());
    }
}
