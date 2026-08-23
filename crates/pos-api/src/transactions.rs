//! Transaction endpoints — `/api/pos/transactions/*`.
//!
//! These routes are **`authMiddleware` + a `POS_*` permission** (`transaction.controller.ts:129`,
//! `:145`, `:170`), which is a user JWT the till does not hold, and the unsafe ones are not on the
//! CSRF exemption list. So none of this is reachable today; see
//! `the-till-holds-no-credential-the-pos-write-routes-accept`. The shapes are repaired regardless,
//! because an unaudited call site reads as correct to the next person.

use anyhow::Result;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::client::{ApiClient, Enveloped};
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
    /// Records a completed transaction. `POST /api/pos/transactions`
    pub async fn create_transaction(
        &self,
        request: &CreateTransactionRequest,
    ) -> Result<CreateTransactionResponse> {
        let response: Enveloped<_> = self.post("/api/pos/transactions", request).await?;
        Ok(response.into_inner())
    }

    /// Voids a recorded transaction. `POST /api/pos/transactions/{id}/void`
    pub async fn void_transaction(
        &self,
        transaction_id: &str,
        request: &VoidTransactionRequest,
    ) -> Result<CreateTransactionResponse> {
        let path = format!(
            "/api/pos/transactions/{}/void",
            urlencoding::encode(transaction_id)
        );
        let response: Enveloped<_> = self.post(&path, request).await?;
        Ok(response.into_inner())
    }

    /// Looks a transaction up by its printed receipt number.
    /// `GET /api/pos/transactions/by-receipt/{receiptNumber}`
    pub async fn get_transaction_by_receipt(
        &self,
        receipt_number: &str,
    ) -> Result<TransactionDetailDto> {
        let path = format!(
            "/api/pos/transactions/by-receipt/{}",
            urlencoding::encode(receipt_number)
        );
        let response: Enveloped<_> = self.get(&path).await?;
        Ok(response.into_inner())
    }
}
