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
    pub shift_id: String,
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
