//! Transaction endpoints — `/api/pos/till/transactions/*`.
//!
//! **The till's own door, not the back office's.** `/api/pos/transactions/*` is
//! `authMiddleware` + a `POS_*` permission — a user JWT this till does not hold — and it is
//! deliberately untouched, still answering a cookieless caller. The three routes here are the
//! till-audienced half of the same route table, mounted at `pos.routes.ts:163` behind
//! `[terminalAuth, attendedOperatorAuth]`: an enrolled device, then the cashier who proved a PIN
//! on it.
//!
//! Each was confirmed against its `audiences` declaration rather than by substituting `till/` into
//! the old path — `transaction.controller.ts:167` (`POST /`, `POS_CREATE`), `:186`
//! (`GET /by-receipt/:receiptNumber`, `POS_READ`), `:212` (`POST /:transactionId/void`,
//! `POS_VOID`). The sibling declarations at `:175`, `:198` and `:205` are `['back-office']` only,
//! so `GET /`, `/undeducted-lines` and `/:transactionId` have no till mount and are not reachable
//! from here whatever URL is written.

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::client::{ApiClient, Enveloped};
use crate::failure::ApiResult;
use crate::shifts::ServerShiftId;
use pos_models::{OperatorId, RecordedOperatorName};

/// A transaction as the till submits it to `POST /api/pos/transactions`.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateTransactionRequest {
    pub transaction_number: String,
    pub transaction_type: String,
    pub items: Vec<TransactionItemDto>,
    pub payments: Vec<PaymentDto>,
    pub subtotal: Decimal,
    pub tax_total: Decimal,
    pub discount_total: Decimal,
    pub grand_total: Decimal,
    pub currency: String,
    pub customer_id: Option<String>,
    pub customer_name: Option<String>,
    /// The platform's identifier for the shift this sale belongs to, when it has one.
    ///
    /// Omitted rather than blanked when absent — see [`ServerShiftId`]. `POS_Transaction.shiftId`
    /// is a nullable `@db.Uuid` with a foreign key, and the validator marks it `.optional()`, so
    /// an omitted field is a sale the platform records against no shift. A *local* id here would
    /// be refused as a malformed UUID.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shift_id: Option<ServerShiftId>,
    pub terminal_id: String,
    pub operator_id: OperatorId,
    pub note: Option<String>,
    pub created_at: String,
    pub completed_at: Option<String>,
}

/// One line of a submitted transaction.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransactionItemDto {
    pub product_id: String,
    pub product_name: String,
    pub sku: String,
    pub quantity: Decimal,
    pub unit_price: Decimal,
    pub tax_rate: Decimal,
    pub tax_amount: Decimal,
    pub discount_amount: Decimal,
    pub line_total: Decimal,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub product_type: Option<String>,
    pub inventory_deducted: bool,
}

/// One payment against a submitted transaction.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PaymentDto {
    pub method: String,
    pub amount: Decimal,
    pub currency: String,
    pub reference: Option<String>,
    pub card_last_four: Option<String>,
    pub card_type: Option<String>,
    pub auth_code: Option<String>,
    pub wallet_type: Option<String>,
}

/// What the platform returns once a transaction is recorded.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateTransactionResponse {
    pub id: String,
    pub receipt_number: String,
}

/// The reason a transaction is being voided.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VoidTransactionRequest {
    pub reason: String,
}

/// A transaction as `GET /transactions/by-receipt/{n}` returns it.
///
/// The platform sends this **directly as the envelope's `data`** (`transaction.controller.ts:322`).
/// The till used to read a bare `{ transaction: … }` with the raw `get`, so it was wrong twice:
/// the envelope was never unwrapped, and the payload was looked for under a key the platform does
/// not send.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransactionDetailDto {
    pub id: String,
    pub transaction_number: String,
    pub transaction_type: String,
    pub items: Vec<TransactionDetailItemDto>,
    pub payments: Vec<TransactionDetailPaymentDto>,
    pub subtotal: Decimal,
    pub tax_total: Decimal,
    pub discount_total: Decimal,
    pub grand_total: Decimal,
    pub customer_id: Option<String>,
    pub customer_name: Option<String>,
    pub shift_id: String,
    pub terminal_id: String,
    pub operator_id: OperatorId,
    pub operator_name: RecordedOperatorName,
    pub status: String,
    pub created_at: String,
    pub completed_at: Option<String>,
    pub receipt_number: Option<String>,
}

/// One line of a returned transaction.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransactionDetailItemDto {
    pub id: String,
    pub product_id: String,
    pub product_name: String,
    pub product_name_ar: Option<String>,
    pub sku: String,
    pub barcode: Option<String>,
    pub quantity: Decimal,
    pub unit: String,
    pub unit_price: Decimal,
    pub tax_rate: Decimal,
    pub tax_amount: Decimal,
    pub discount_amount: Decimal,
    pub line_total: Decimal,
}

/// One payment of a returned transaction.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransactionDetailPaymentDto {
    pub id: String,
    pub method: String,
    pub amount: Decimal,
    pub currency: String,
    pub reference: Option<String>,
    pub card_last_four: Option<String>,
    pub card_type: Option<String>,
    pub auth_code: Option<String>,
    pub wallet_type: Option<String>,
    pub created_at: String,
}

impl ApiClient {
    /// Records a completed transaction. `POST /api/pos/till/transactions`
    ///
    /// Returns [`ApiResult`] rather than `anyhow::Result`: this route can refuse for reasons the
    /// caller must tell apart — a capability the operator does not hold, a product the catalogue
    /// does not know, a currency the company has not configured — and a flattened error makes all
    /// of them "sync failed".
    pub async fn create_transaction(
        &self,
        request: &CreateTransactionRequest,
    ) -> ApiResult<CreateTransactionResponse> {
        let response: Enveloped<_> = self
            .post_or_failure("/api/pos/till/transactions", request)
            .await?;
        Ok(response.into_inner())
    }

    /// Voids a recorded transaction. `POST /api/pos/transactions/{id}/void`
    pub async fn void_transaction(
        &self,
        transaction_id: &str,
        request: &VoidTransactionRequest,
    ) -> ApiResult<CreateTransactionResponse> {
        let path = format!(
            "/api/pos/till/transactions/{}/void",
            urlencoding::encode(transaction_id)
        );
        let response: Enveloped<_> = self.post_or_failure(&path, request).await?;
        Ok(response.into_inner())
    }

    /// Looks a transaction up by its printed receipt number.
    /// `GET /api/pos/transactions/by-receipt/{receiptNumber}`
    ///
    /// The one read of the six, and the reason [`ApiClient::get_or_failure`] had to exist: a
    /// receipt the platform does not have and a lookup it would not answer are different things to
    /// tell the person holding the paper, and `get` made them one `anyhow::Error`.
    pub async fn get_transaction_by_receipt(
        &self,
        receipt_number: &str,
    ) -> ApiResult<TransactionDetailDto> {
        let path = format!(
            "/api/pos/till/transactions/by-receipt/{}",
            urlencoding::encode(receipt_number)
        );
        let response: Enveloped<_> = self.get_or_failure(&path).await?;
        Ok(response.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shifts::ServerShiftId;

    fn a_sale(shift_id: Option<ServerShiftId>) -> CreateTransactionRequest {
        CreateTransactionRequest {
            transaction_number: "TXN-1".to_string(),
            transaction_type: "SALE".to_string(),
            items: vec![],
            payments: vec![],
            subtotal: Decimal::ONE,
            tax_total: Decimal::ZERO,
            discount_total: Decimal::ZERO,
            grand_total: Decimal::ONE,
            currency: "LYD".to_string(),
            customer_id: None,
            customer_name: None,
            shift_id,
            terminal_id: "TERM-001".to_string(),
            operator_id: OperatorId::new("op-1").expect("a fixture id is never blank"),
            note: None,
            created_at: "2026-08-24T09:00:00Z".to_string(),
            completed_at: None,
        }
    }

    /// A sale in a shift the platform knows quotes the platform's identifier.
    #[test]
    fn a_sale_names_the_shift_the_platform_issued() {
        let id = ServerShiftId::new("9f1c0f6e-0000-4000-8000-000000000001")
            .expect("a fixture id is never blank");
        let body = serde_json::to_value(a_sale(Some(id))).expect("the request serialises");

        assert_eq!(body["shiftId"], "9f1c0f6e-0000-4000-8000-000000000001");
    }

    /// A sale in a shift the platform has never seen omits the field entirely.
    ///
    /// The control for the test above, and the case an offline-first till spends most of its life
    /// in. **Omitted, not blank and not the local id.** `POS_Transaction.shiftId` is a nullable
    /// `@db.Uuid` and `transaction.validator.ts:100` marks the field `.optional()`, so the absence
    /// is legal — while `""` and a local primary key both fail the UUID check, and a local id that
    /// somehow passed it would break the foreign key.
    #[test]
    fn a_sale_in_an_unsynced_shift_names_no_shift_at_all() {
        let body = serde_json::to_value(a_sale(None)).expect("the request serialises");

        assert!(
            body.get("shiftId").is_none(),
            "an absent shift must be an absent field, not a blank or a local id: {body}"
        );
        // The control on the control: the rest of the body is still there, so a serialiser that
        // dropped everything would not read as a pass.
        assert_eq!(body["transactionNumber"], "TXN-1");
        assert_eq!(body["terminalId"], "TERM-001");
    }
}
