//! Returns — `POST /api/pos/returns`.
//!
//! `authMiddleware` + `POS_REFUND` (`return.controller.ts:68`), not CSRF-exempt: unreachable by
//! the till today. The DTOs were declared inside the function body that used them, so nothing
//! outside `return_service` could see the shape the till depends on.

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::client::{ApiClient, Enveloped};
use crate::failure::ApiResult;
use crate::shifts::ServerShiftId;
use pos_models::OperatorId;

/// A refund against an earlier transaction.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateReturnRequest {
    pub original_transaction_id: String,
    pub items: Vec<ReturnItemRequest>,
    pub refund_method: String,
    pub refund_amount: Decimal,
    pub operator_id: OperatorId,
    pub terminal_id: String,
    /// The platform's identifier for the shift, when it has one.
    ///
    /// `POS_Return.shiftId` is a nullable `@db.Uuid` (`prisma/pos.prisma:894`) exactly like the
    /// transaction column. Its validator is *weaker* — `return.validator.ts:46` is
    /// `z.string().optional()` with no UUID check — which makes this the more dangerous of the
    /// two: a local id passes validation and reaches Postgres, where the column type refuses it
    /// as a 500 rather than a 400. Task 09 named only the transaction; the defect is the same
    /// field and repairing one would have left the other.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shift_id: Option<ServerShiftId>,
}

/// One line being returned.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReturnItemRequest {
    pub original_item_id: String,
    pub quantity: Decimal,
    pub reason: String,
}

/// The platform's identifier for the recorded return.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateReturnResponse {
    pub id: String,
}

impl ApiClient {
    /// Records a return. `POST /api/pos/returns`
    pub async fn create_return(
        &self,
        request: &CreateReturnRequest,
    ) -> ApiResult<CreateReturnResponse> {
        let response: Enveloped<_> = self
            .post_or_failure("/api/pos/till/returns", request)
            .await?;
        Ok(response.into_inner())
    }
}
