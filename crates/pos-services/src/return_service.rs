//! Return Service - Returns and Refund Processing
//!
//! This module handles return transactions including receipt lookup,
//! eligibility checking, and refund processing.
//!
//! ## Features
//!
//! - Receipt-based returns (lookup original transaction)
//! - Manual returns (without receipt, requires approval)
//! - Return eligibility checking (30-day window, status validation)
//! - Supervisor approval workflow for policy violations
//! - Refund processing (cash, card reversal, store credit)
//!
//! ## Usage
//!
//! ```rust,ignore
//! use crate::services::{ReturnService, ReturnItem, ReturnEligibility};
//! use std::sync::Arc;
//!
//! let return_service = ReturnService::new(Arc::clone(&api), Arc::clone(&db));
//!
//! // Find transaction by receipt
//! if let Some(txn) = return_service.find_transaction("RCP-001").await? {
//!     // Check eligibility
//!     match return_service.can_return(&txn) {
//!         ReturnEligibility::Eligible => {
//!             // Process return
//!             let items = vec![ReturnItem {
//!                 original_item_id: "item-1".to_string(),
//!                 quantity: 1,
//!                 reason: "Defective".to_string(),
//!             }];
//!             let refund = return_service.process_return(&txn, &items, "cash").await?;
//!         }
//!         ReturnEligibility::RequiresApproval(reason) => {
//!             // Request supervisor approval
//!         }
//!         ReturnEligibility::NotEligible(reason) => {
//!             // Show error to user
//!         }
//!     }
//! }
//! ```

use chrono::{DateTime, Utc};
use std::str::FromStr;

use crate::parse::ParseError;
use crate::platform_sync::PlatformSync;
use crate::shift_service::server_shift_id;
use pos_api::{ApiClient, ApiFailure, CreateReturnRequest, ReturnItemRequest, ServerErrorCode};
use pos_db::decimal_from_sqlite;
use pos_db::transactions::OfflineTransactionRow;
use pos_db::Database;
use pos_models::transaction::{
    Payment, PaymentMethod, Transaction, TransactionItem, TransactionStatus,
};
use pos_models::{OperatorId, RecordedOperatorName};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

/// Default return window in days
pub const DEFAULT_RETURN_WINDOW_DAYS: i64 = 30;

/// Return eligibility status
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReturnEligibility {
    /// Transaction can be returned without restrictions
    Eligible,
    /// Transaction can be returned but requires supervisor approval
    RequiresApproval(String),
    /// Transaction cannot be returned
    NotEligible(String),
}

impl ReturnEligibility {
    /// Returns true if the return is allowed (either eligible or requires approval)
    pub fn is_allowed(&self) -> bool {
        !matches!(self, ReturnEligibility::NotEligible(_))
    }

    /// Returns the reason message if not fully eligible
    pub fn reason(&self) -> Option<&str> {
        match self {
            ReturnEligibility::Eligible => None,
            ReturnEligibility::RequiresApproval(reason)
            | ReturnEligibility::NotEligible(reason) => Some(reason),
        }
    }
}

/// An item to be returned
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReturnItem {
    /// ID of the original transaction item
    pub original_item_id: String,
    /// Product ID
    pub product_id: String,
    /// Product name
    pub product_name: String,
    /// Product name (Arabic)
    pub product_name_ar: Option<String>,
    /// SKU
    pub sku: String,
    /// Quantity to return
    pub quantity: Decimal,
    /// Unit price at time of sale
    pub unit_price: Decimal,
    /// Tax rate
    pub tax_rate: Decimal,
    /// Calculated refund amount for this item (price * qty + tax - discount)
    pub refund_amount: Decimal,
    /// Reason for return
    pub reason: ReturnReason,
    /// Additional notes
    pub notes: Option<String>,
}

impl ReturnItem {
    /// Creates a return item from an original transaction item
    pub fn from_transaction_item(
        item: &TransactionItem,
        quantity: Decimal,
        reason: ReturnReason,
    ) -> Self {
        // Calculate refund for partial quantity
        let line_ratio = quantity / item.quantity;
        let refund_amount = item.line_total * line_ratio;

        Self {
            original_item_id: item.id.clone(),
            product_id: item.product_id.clone(),
            product_name: item.product_name.clone(),
            product_name_ar: item.product_name_ar.clone(),
            sku: item.sku.clone(),
            quantity,
            unit_price: item.unit_price,
            tax_rate: item.tax_rate,
            refund_amount,
            reason,
            notes: None,
        }
    }

    /// Returns the display name based on locale
    pub fn display_name(&self, locale: &str) -> &str {
        if locale == "ar" {
            self.product_name_ar
                .as_deref()
                .unwrap_or(&self.product_name)
        } else {
            &self.product_name
        }
    }
}

/// Reason for return
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReturnReason {
    /// Product is defective or damaged
    Defective,
    /// Wrong product purchased
    WrongProduct,
    /// Customer changed their mind
    ChangedMind,
    /// Product expired or near expiry
    Expired,
    /// Price adjustment (returned and repurchased)
    PriceAdjustment,
    /// Exchange for different item
    Exchange,
    /// Other reason (specify in notes)
    Other,
}

impl ReturnReason {
    /// Returns the reason as a string for database storage
    pub fn as_str(&self) -> &'static str {
        match self {
            ReturnReason::Defective => "DEFECTIVE",
            ReturnReason::WrongProduct => "WRONG_PRODUCT",
            ReturnReason::ChangedMind => "CHANGED_MIND",
            ReturnReason::Expired => "EXPIRED",
            ReturnReason::PriceAdjustment => "PRICE_ADJUSTMENT",
            ReturnReason::Exchange => "EXCHANGE",
            ReturnReason::Other => "OTHER",
        }
    }

    /// Creates a reason from a string
    /// Returns the display name for this reason
    pub fn display_name(&self, locale: &str) -> &'static str {
        if locale == "ar" {
            match self {
                ReturnReason::Defective => "معيب أو تالف",
                ReturnReason::WrongProduct => "منتج خاطئ",
                ReturnReason::ChangedMind => "تغيير الرأي",
                ReturnReason::Expired => "منتهي الصلاحية",
                ReturnReason::PriceAdjustment => "تعديل السعر",
                ReturnReason::Exchange => "استبدال",
                ReturnReason::Other => "سبب آخر",
            }
        } else {
            match self {
                ReturnReason::Defective => "Defective/Damaged",
                ReturnReason::WrongProduct => "Wrong Product",
                ReturnReason::ChangedMind => "Changed Mind",
                ReturnReason::Expired => "Expired",
                ReturnReason::PriceAdjustment => "Price Adjustment",
                ReturnReason::Exchange => "Exchange",
                ReturnReason::Other => "Other",
            }
        }
    }

    /// Returns whether this reason typically requires approval
    pub fn requires_approval(&self) -> bool {
        matches!(
            self,
            ReturnReason::ChangedMind | ReturnReason::PriceAdjustment | ReturnReason::Other
        )
    }
}

impl FromStr for ReturnReason {
    type Err = ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_uppercase().as_str() {
            "DEFECTIVE" => Ok(ReturnReason::Defective),
            "WRONG_PRODUCT" => Ok(ReturnReason::WrongProduct),
            "CHANGED_MIND" => Ok(ReturnReason::ChangedMind),
            "EXPIRED" => Ok(ReturnReason::Expired),
            "PRICE_ADJUSTMENT" => Ok(ReturnReason::PriceAdjustment),
            "EXCHANGE" => Ok(ReturnReason::Exchange),
            "OTHER" => Ok(ReturnReason::Other),
            _ => Err(ParseError::ReturnReason(s.to_string())),
        }
    }
}

/// Refund method
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RefundMethod {
    /// Cash refund
    Cash,
    /// Card refund (reversal to original card)
    Card,
    /// Store credit
    StoreCredit,
    /// Exchange (no monetary refund)
    Exchange,
}

impl RefundMethod {
    pub fn as_str(&self) -> &'static str {
        match self {
            RefundMethod::Cash => "CASH",
            RefundMethod::Card => "CARD",
            RefundMethod::StoreCredit => "STORE_CREDIT",
            RefundMethod::Exchange => "EXCHANGE",
        }
    }

    pub fn display_name(&self, locale: &str) -> &'static str {
        if locale == "ar" {
            match self {
                RefundMethod::Cash => "نقدي",
                RefundMethod::Card => "بطاقة",
                RefundMethod::StoreCredit => "رصيد المتجر",
                RefundMethod::Exchange => "استبدال",
            }
        } else {
            match self {
                RefundMethod::Cash => "Cash",
                RefundMethod::Card => "Card",
                RefundMethod::StoreCredit => "Store Credit",
                RefundMethod::Exchange => "Exchange",
            }
        }
    }
}

impl FromStr for RefundMethod {
    type Err = ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_uppercase().as_str() {
            "CASH" => Ok(RefundMethod::Cash),
            "CARD" => Ok(RefundMethod::Card),
            "STORE_CREDIT" => Ok(RefundMethod::StoreCredit),
            "EXCHANGE" => Ok(RefundMethod::Exchange),
            _ => Err(ParseError::RefundMethod(s.to_string())),
        }
    }
}

/// Result of a return transaction lookup
#[derive(Clone, Debug)]
pub struct TransactionLookupResult {
    /// The found transaction
    pub transaction: Transaction,
    /// Return eligibility status
    pub eligibility: ReturnEligibility,
    /// Days since original purchase
    pub days_since_purchase: i64,
    /// Whether the transaction was found locally or from server
    pub source: LookupSource,
}

/// Source of transaction lookup
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LookupSource {
    /// Found in local database
    Local,
    /// Retrieved from server
    Server,
}

/// Result of a completed return
#[derive(Clone, Debug)]
pub struct ReturnResult {
    /// ID of the return transaction
    pub return_transaction_id: String,
    /// Return transaction number
    pub return_number: String,
    /// Total refund amount
    pub refund_amount: Decimal,
    /// Currency
    pub currency: String,
    /// Refund method used
    pub refund_method: RefundMethod,
    /// Number of items returned
    pub item_count: usize,
    /// What the platform did with the refund.
    ///
    /// A return is the worked example of why this cannot be a boolean. `POS_REFUND` sits at
    /// SUPERVISOR, so a cashier attempting one is refused — and the till has to say *fetch one of
    /// these people* rather than *saved, will sync later*.
    pub platform: PlatformSync,
}

/// Return service error types
#[derive(Clone, Debug)]
pub enum ReturnError {
    /// Transaction not found
    TransactionNotFound,
    /// Transaction not eligible for return
    NotEligible(String),
    /// Requires supervisor approval
    RequiresApproval(String),
    /// No items selected for return
    NoItemsSelected,
    /// Invalid return quantity
    InvalidQuantity(String),
    /// Database error
    DatabaseError(String),
    /// Network error
    NetworkError(String),
    /// API error
    ApiError(String),
}

impl std::fmt::Display for ReturnError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReturnError::TransactionNotFound => write!(f, "Transaction not found"),
            ReturnError::NotEligible(reason) => write!(f, "Not eligible: {}", reason),
            ReturnError::RequiresApproval(reason) => write!(f, "Requires approval: {}", reason),
            ReturnError::NoItemsSelected => write!(f, "No items selected for return"),
            ReturnError::InvalidQuantity(msg) => write!(f, "Invalid quantity: {}", msg),
            ReturnError::DatabaseError(msg) => write!(f, "Database error: {}", msg),
            ReturnError::NetworkError(msg) => write!(f, "Network error: {}", msg),
            ReturnError::ApiError(msg) => write!(f, "API error: {}", msg),
        }
    }
}

impl std::error::Error for ReturnError {}

/// Return service result type
pub type ReturnServiceResult<T> = std::result::Result<T, ReturnError>;

/// Return Service for handling returns and refunds
pub struct ReturnService {
    api: Arc<ApiClient>,
    db: Arc<Database>,
    return_window_days: i64,
}

impl ReturnService {
    /// Creates a new ReturnService
    pub fn new(api: Arc<ApiClient>, db: Arc<Database>) -> Self {
        Self {
            api,
            db,
            return_window_days: DEFAULT_RETURN_WINDOW_DAYS,
        }
    }

    /// Creates a ReturnService with custom return window
    pub fn with_return_window(api: Arc<ApiClient>, db: Arc<Database>, days: i64) -> Self {
        Self {
            api,
            db,
            return_window_days: days,
        }
    }

    /// Finds a transaction by receipt number
    ///
    /// First checks local database, then queries server if online
    pub async fn find_transaction(
        &self,
        receipt_number: &str,
    ) -> ReturnServiceResult<Option<TransactionLookupResult>> {
        debug!("Looking up transaction by receipt: {}", receipt_number);

        // First check local database
        if let Some(txn) = self.find_local_transaction(receipt_number)? {
            let eligibility = self.check_eligibility(&txn);
            let days_since = self.days_since_purchase(&txn);

            info!(
                "Found transaction locally: {} (days since: {}, eligible: {:?})",
                receipt_number, days_since, eligibility
            );

            return Ok(Some(TransactionLookupResult {
                transaction: txn,
                eligibility,
                days_since_purchase: days_since,
                source: LookupSource::Local,
            }));
        }

        // Try server if online
        if let Some(txn) = self.find_server_transaction(receipt_number).await? {
            let eligibility = self.check_eligibility(&txn);
            let days_since = self.days_since_purchase(&txn);

            info!(
                "Found transaction on server: {} (days since: {}, eligible: {:?})",
                receipt_number, days_since, eligibility
            );

            return Ok(Some(TransactionLookupResult {
                transaction: txn,
                eligibility,
                days_since_purchase: days_since,
                source: LookupSource::Server,
            }));
        }

        warn!("Transaction not found: {}", receipt_number);
        Ok(None)
    }

    /// Finds a transaction in the local database
    fn find_local_transaction(
        &self,
        receipt_number: &str,
    ) -> ReturnServiceResult<Option<Transaction>> {
        // Search offline transactions table by receipt number
        let conn = self.db.connection();
        let conn = conn.lock();

        // `.ok()` here discarded the error into the same `None` that means "no such receipt", so a
        // store that could not answer became "this transaction does not exist" — and the caller
        // turns that into a refusal the cashier reads as a fact about the customer's receipt.
        let result: Option<OfflineTransactionRow> = match conn
            .query_row(
                r#"SELECT offline_id, transaction_number, transaction_type, items_json, payments_json,
                          subtotal, tax_total, discount_total, grand_total, customer_id, customer_name,
                          shift_id, operator_id, terminal_id, receipt_number, notes, created_at,
                          sync_status, server_id, retry_count, last_error, last_retry_at
                   FROM offline_transactions
                   WHERE receipt_number = ?1 OR transaction_number = ?1
                   LIMIT 1"#,
                [receipt_number],
                |row| {
                    Ok(OfflineTransactionRow {
                        offline_id: row.get(0)?,
                        transaction_number: row.get(1)?,
                        transaction_type: row.get(2)?,
                        items_json: row.get(3)?,
                        payments_json: row.get(4)?,
                        subtotal: decimal_from_sqlite(row.get(5)?),
                        tax_total: decimal_from_sqlite(row.get(6)?),
                        discount_total: decimal_from_sqlite(row.get(7)?),
                        grand_total: decimal_from_sqlite(row.get(8)?),
                        customer_id: row.get(9)?,
                        customer_name: row.get(10)?,
                        shift_id: row.get(11)?,
                        operator_id: pos_db::column::optional_operator_id(row, 12)?,
                        terminal_id: row.get(13)?,
                        receipt_number: row.get(14)?,
                        notes: row.get(15)?,
                        created_at: row.get(16)?,
                        sync_status: row.get(17)?,
                        server_id: row.get(18)?,
                        retry_count: row.get(19)?,
                        last_error: row.get(20)?,
                        last_retry_at: row.get(21)?,
                        catalog_etag: None, // Legacy transactions don't have catalog_etag
                    })
                },
            ) {
            Ok(row) => Some(row),
            Err(rusqlite::Error::QueryReturnedNoRows) => None,
            Err(e) => {
                return Err(ReturnError::DatabaseError(format!(
                    "Failed to look up transaction {}: {}",
                    receipt_number, e
                )))
            }
        };

        if let Some(row) = result {
            // Convert to Transaction model
            let txn = self.row_to_transaction(&row)?;
            Ok(Some(txn))
        } else {
            Ok(None)
        }
    }

    /// Finds a transaction from the server API
    async fn find_server_transaction(
        &self,
        receipt_number: &str,
    ) -> ReturnServiceResult<Option<Transaction>> {
        if !self.api.is_online().await.is_online() {
            debug!("Offline - skipping server lookup for {}", receipt_number);
            return Ok(None);
        }

        // The platform sends the transaction AS the envelope's `data`
        // (`transaction.controller.ts:322`). This used to read a bare `{ transaction: … }` with
        // the raw `get`, so it was wrong twice over: the envelope was never unwrapped, and the
        // payload was looked for under a key the platform does not send.
        // "The platform has no such receipt" and "the platform would not answer" are different
        // things to tell the person holding the paper, and until `get_or_failure` existed they
        // were the same `anyhow::Error`: every non-2xx arrived as `Err`, so the `Ok(None)` arm
        // was unreachable on every path. It is reachable now, and it is reached by reading
        // `ServerErrorCode`, never by matching digits in a message.
        match self.api.get_transaction_by_receipt(receipt_number).await {
            Ok(dto) => {
                {
                    // Convert DTO to Transaction
                    let created_at = DateTime::parse_from_rfc3339(&dto.created_at)
                        .map(|dt| dt.with_timezone(&Utc))
                        .unwrap_or_else(|_| Utc::now());

                    let completed_at = dto.completed_at.as_ref().and_then(|s| {
                        DateTime::parse_from_rfc3339(s)
                            .map(|dt| dt.with_timezone(&Utc))
                            .ok()
                    });

                    let items: Vec<TransactionItem> = dto
                        .items
                        .iter()
                        .map(|item| TransactionItem {
                            id: item.id.clone(),
                            product_id: item.product_id.clone(),
                            product_name: item.product_name.clone(),
                            product_name_ar: item.product_name_ar.clone(),
                            sku: item.sku.clone(),
                            barcode: item.barcode.clone(),
                            quantity: item.quantity,
                            unit: item.unit.clone(),
                            unit_price: item.unit_price,
                            tax_rate: item.tax_rate,
                            tax_amount: item.tax_amount,
                            discount_amount: item.discount_amount,
                            line_total: item.line_total,
                            product_type: String::new(),
                            inventory_deducted: true,
                        })
                        .collect();

                    let payments: Vec<Payment> = dto
                        .payments
                        .iter()
                        .map(|p| {
                            let payment_created_at = DateTime::parse_from_rfc3339(&p.created_at)
                                .map(|dt| dt.with_timezone(&Utc))
                                .unwrap_or_else(|_| Utc::now());

                            Payment {
                                id: p.id.clone(),
                                method: p.method.parse().unwrap_or(PaymentMethod::Cash),
                                amount: p.amount,
                                currency: p.currency.clone(),
                                reference: p.reference.clone(),
                                card_last_four: p.card_last_four.clone(),
                                card_type: p.card_type.clone(),
                                auth_code: p.auth_code.clone(),
                                wallet_type: p.wallet_type.clone(),
                                created_at: payment_created_at,
                            }
                        })
                        .collect();

                    let txn = Transaction {
                        id: dto.id,
                        transaction_number: dto.transaction_number,
                        transaction_type: dto.transaction_type,
                        items,
                        payments,
                        subtotal: dto.subtotal,
                        tax_total: dto.tax_total,
                        discount_total: dto.discount_total,
                        grand_total: dto.grand_total,
                        currency: "LYD".to_string(),
                        customer_id: dto.customer_id,
                        customer_name: dto.customer_name,
                        shift_id: dto.shift_id,
                        terminal_id: dto.terminal_id,
                        operator_id: dto.operator_id,
                        operator_name: Some(dto.operator_name),
                        status: dto.status.parse().unwrap_or(TransactionStatus::Completed),
                        created_at,
                        completed_at,
                        voided_at: None,
                        void_reason: None,
                        note: None,
                        receipt_number: dto.receipt_number,
                        server_id: None,
                    };

                    Ok(Some(txn))
                }
            }
            // The platform answered, and the answer is that it holds no such receipt. That is a
            // fact about the receipt, not a failure of the lookup, so it is `Ok(None)` and the
            // caller falls through to the local queue exactly as it does when offline.
            Err(ApiFailure::Refused {
                code: ServerErrorCode::NotFound,
                ..
            }) => {
                debug!(
                    "Platform holds no transaction for receipt {}",
                    receipt_number
                );
                Ok(None)
            }
            Err(e) => {
                error!("Server lookup failed for {}: {}", receipt_number, e);
                Err(ReturnError::ApiError(e.to_string()))
            }
        }
    }

    /// Converts an offline transaction row to a Transaction model
    fn row_to_transaction(&self, row: &OfflineTransactionRow) -> ReturnServiceResult<Transaction> {
        let items: Vec<TransactionItem> = serde_json::from_str(&row.items_json)
            .map_err(|e| ReturnError::DatabaseError(format!("Failed to parse items: {}", e)))?;

        let payments: Vec<Payment> = serde_json::from_str(&row.payments_json)
            .map_err(|e| ReturnError::DatabaseError(format!("Failed to parse payments: {}", e)))?;

        let created_at = DateTime::parse_from_rfc3339(&row.created_at)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now());

        let status = row
            .sync_status
            .parse()
            .unwrap_or(TransactionStatus::Completed);

        Ok(Transaction {
            id: row.offline_id.clone(),
            transaction_number: row.transaction_number.clone().unwrap_or_default(),
            transaction_type: row.transaction_type.clone(),
            items,
            payments,
            subtotal: row.subtotal,
            tax_total: row.tax_total,
            discount_total: row.discount_total,
            grand_total: row.grand_total,
            currency: "LYD".to_string(),
            customer_id: row.customer_id.clone(),
            customer_name: row.customer_name.clone(),
            shift_id: row.shift_id.clone().unwrap_or_default(),
            terminal_id: row.terminal_id.clone().unwrap_or_default(),
            // The row's `operator_id` is nullable and a transaction's is not. A queued
            // transaction with no operator cannot be rebuilt into one, and saying so is the
            // point: `unwrap_or_default()` used to turn it into a transaction rung by nobody.
            // `operator_name` has no column at all, so the reconstruction reports the absence
            // rather than inventing an empty name.
            operator_id: row.operator_id.clone().ok_or_else(|| {
                ReturnError::DatabaseError(format!(
                    "offline transaction {} has no operator_id and cannot be rebuilt",
                    row.offline_id
                ))
            })?,
            operator_name: None,
            status,
            created_at,
            completed_at: Some(created_at),
            voided_at: None,
            void_reason: None,
            note: row.notes.clone(),
            receipt_number: row.receipt_number.clone(),
            server_id: row.server_id.clone(),
        })
    }

    /// Checks if a transaction is eligible for return
    pub fn check_eligibility(&self, txn: &Transaction) -> ReturnEligibility {
        // Check if already voided
        if txn.status == TransactionStatus::Voided {
            return ReturnEligibility::NotEligible("Transaction has been voided".to_string());
        }

        // Check if already returned
        if txn.status == TransactionStatus::Returned {
            return ReturnEligibility::NotEligible(
                "Transaction has already been returned".to_string(),
            );
        }

        // Check if it's a return transaction (can't return a return)
        if txn.is_return() {
            return ReturnEligibility::NotEligible(
                "Cannot return a return transaction".to_string(),
            );
        }

        // Check return window
        let days_since = self.days_since_purchase(txn);
        if days_since > self.return_window_days {
            return ReturnEligibility::RequiresApproval(format!(
                "Transaction is over {} days old ({} days)",
                self.return_window_days, days_since
            ));
        }

        // Check if transaction has items
        if txn.items.is_empty() {
            return ReturnEligibility::NotEligible("Transaction has no items".to_string());
        }

        ReturnEligibility::Eligible
    }

    /// Alias for check_eligibility for compatibility with Phase 30 spec
    pub fn can_return(&self, txn: &Transaction) -> ReturnEligibility {
        self.check_eligibility(txn)
    }

    /// Calculates days since purchase
    fn days_since_purchase(&self, txn: &Transaction) -> i64 {
        let now = Utc::now();
        let purchase_date = txn.completed_at.unwrap_or(txn.created_at);
        (now - purchase_date).num_days()
    }

    /// Validates return items
    pub fn validate_return_items(
        &self,
        original_txn: &Transaction,
        items: &[ReturnItem],
    ) -> ReturnServiceResult<()> {
        if items.is_empty() {
            return Err(ReturnError::NoItemsSelected);
        }

        for item in items {
            // Find original item
            let original = original_txn
                .items
                .iter()
                .find(|i| i.id == item.original_item_id)
                .ok_or_else(|| {
                    ReturnError::InvalidQuantity(format!(
                        "Item {} not found in original transaction",
                        item.original_item_id
                    ))
                })?;

            // Check quantity doesn't exceed original
            if item.quantity > original.quantity {
                return Err(ReturnError::InvalidQuantity(format!(
                    "Return quantity ({}) exceeds original quantity ({}) for {}",
                    item.quantity, original.quantity, item.product_name
                )));
            }

            // Check quantity is positive
            if item.quantity.is_sign_negative() || item.quantity.is_zero() {
                return Err(ReturnError::InvalidQuantity(format!(
                    "Return quantity must be positive for {}",
                    item.product_name
                )));
            }
        }

        Ok(())
    }

    /// Calculates total refund amount for return items
    pub fn calculate_refund_total(&self, items: &[ReturnItem]) -> Decimal {
        items.iter().map(|i| i.refund_amount).sum()
    }

    /// Processes a return transaction
    ///
    /// Creates a return transaction, calculates refund, and syncs to server
    // Task 09 typed `operator_id` and `operator_name`, so neither is swappable any more.
    // `shift_id` and `terminal_id` remain adjacent `&str` and remain swappable; they belong to a
    // later tier of `type-driven-domain-core`. The arity is unchanged, so the `expect` holds.
    #[expect(
        clippy::too_many_arguments,
        reason = "eight parameters; the two identity ones are typed, the rest belong to later \
                  tiers of type-driven-domain-core"
    )]
    pub async fn process_return(
        &self,
        original_txn: &Transaction,
        items: &[ReturnItem],
        refund_method: RefundMethod,
        operator_id: &OperatorId,
        operator_name: &RecordedOperatorName,
        shift_id: &str,
        terminal_id: &str,
    ) -> ReturnServiceResult<ReturnResult> {
        // Validate items
        self.validate_return_items(original_txn, items)?;

        // Calculate totals
        let refund_total = self.calculate_refund_total(items);

        info!(
            "Processing return for txn {}: {} items, {} LYD refund via {:?}",
            original_txn
                .receipt_number
                .as_deref()
                .unwrap_or(&original_txn.transaction_number),
            items.len(),
            refund_total,
            refund_method
        );

        // Create return transaction
        let return_id = Uuid::new_v4().to_string();
        let return_number = self.generate_return_number(terminal_id);

        // Convert return items to transaction items (negative amounts)
        let return_items: Vec<TransactionItem> = items
            .iter()
            .map(|item| TransactionItem {
                id: Uuid::new_v4().to_string(),
                product_id: item.product_id.clone(),
                product_name: item.product_name.clone(),
                product_name_ar: item.product_name_ar.clone(),
                sku: item.sku.clone(),
                barcode: None,
                quantity: -item.quantity, // Negative for returns
                unit: "pc".to_string(),
                unit_price: item.unit_price,
                tax_rate: item.tax_rate,
                tax_amount: -(item.refund_amount * item.tax_rate
                    / (Decimal::from(100) + item.tax_rate)),
                discount_amount: Decimal::ZERO,
                line_total: -item.refund_amount, // Negative for returns
                product_type: String::new(),
                inventory_deducted: true,
            })
            .collect();

        // Create refund payment
        let refund_payment = match refund_method {
            RefundMethod::Cash => Payment::cash(-refund_total, &original_txn.currency),
            RefundMethod::Card => {
                // For card returns, we'd typically process a reversal
                // For now, create a negative payment
                Payment::new(PaymentMethod::Card, -refund_total, &original_txn.currency)
            }
            RefundMethod::StoreCredit => {
                Payment::new(PaymentMethod::Credit, -refund_total, &original_txn.currency)
            }
            RefundMethod::Exchange => {
                // No monetary payment for exchanges
                Payment::new(PaymentMethod::Other, Decimal::ZERO, &original_txn.currency)
            }
        };

        // Build return transaction
        let return_txn = Transaction {
            id: return_id.clone(),
            transaction_number: return_number.clone(),
            transaction_type: "return".to_string(),
            items: return_items,
            payments: vec![refund_payment],
            subtotal: -refund_total,
            tax_total: Decimal::ZERO, // Tax included in refund total
            discount_total: Decimal::ZERO,
            grand_total: -refund_total,
            currency: original_txn.currency.clone(),
            customer_id: original_txn.customer_id.clone(),
            customer_name: original_txn.customer_name.clone(),
            shift_id: shift_id.to_string(),
            terminal_id: terminal_id.to_string(),
            operator_id: operator_id.clone(),
            operator_name: Some(operator_name.clone()),
            status: TransactionStatus::Completed,
            created_at: Utc::now(),
            completed_at: Some(Utc::now()),
            voided_at: None,
            void_reason: None,
            note: Some(format!(
                "Return for transaction {}",
                original_txn
                    .receipt_number
                    .as_deref()
                    .unwrap_or(&original_txn.transaction_number)
            )),
            receipt_number: Some(return_number.clone()),
            server_id: None,
        };

        // Save to local database
        self.save_return_transaction(&return_txn, original_txn, items)?;

        // Try to sync to server
        let platform = self.sync_return_to_server(&return_txn, original_txn).await;

        Ok(ReturnResult {
            return_transaction_id: return_id,
            return_number,
            refund_amount: refund_total,
            currency: original_txn.currency.clone(),
            refund_method,
            item_count: items.len(),
            platform,
        })
    }

    /// Generates a unique return number
    fn generate_return_number(&self, terminal_id: &str) -> String {
        let now = Utc::now();
        let uuid = Uuid::new_v4();
        let uuid_bytes = uuid.as_bytes();
        let random_suffix: u16 = ((uuid_bytes[0] as u16) << 8 | uuid_bytes[1] as u16) % 10000;
        format!(
            "RTN-{}-{}{:02}{:02}-{:04}",
            terminal_id,
            now.format("%y"),
            now.format("%m"),
            now.format("%d"),
            random_suffix
        )
    }

    /// Saves return transaction to local database
    fn save_return_transaction(
        &self,
        return_txn: &Transaction,
        _original_txn: &Transaction,
        items: &[ReturnItem],
    ) -> ReturnServiceResult<()> {
        let items_json = serde_json::to_string(&return_txn.items)
            .map_err(|e| ReturnError::DatabaseError(format!("Failed to serialize items: {}", e)))?;

        let payments_json = serde_json::to_string(&return_txn.payments).map_err(|e| {
            ReturnError::DatabaseError(format!("Failed to serialize payments: {}", e))
        })?;

        // Build notes with return reasons
        let reasons: Vec<String> = items
            .iter()
            .map(|i| format!("{}: {}", i.product_name, i.reason.as_str()))
            .collect();
        let notes = format!("Return reasons: {}", reasons.join(", "));

        let row = OfflineTransactionRow {
            offline_id: return_txn.id.clone(),
            transaction_number: Some(return_txn.transaction_number.clone()),
            transaction_type: "RETURN".to_string(),
            items_json,
            payments_json,
            subtotal: return_txn.subtotal,
            tax_total: return_txn.tax_total,
            discount_total: return_txn.discount_total,
            grand_total: return_txn.grand_total,
            customer_id: return_txn.customer_id.clone(),
            customer_name: return_txn.customer_name.clone(),
            shift_id: Some(return_txn.shift_id.clone()),
            operator_id: Some(return_txn.operator_id.clone()),
            terminal_id: Some(return_txn.terminal_id.clone()),
            receipt_number: return_txn.receipt_number.clone(),
            notes: Some(notes),
            created_at: Utc::now().to_rfc3339(),
            sync_status: "PENDING".to_string(),
            server_id: None,
            retry_count: 0,
            last_error: None,
            last_retry_at: None,
            catalog_etag: None, // Returns don't need catalog tracking
        };

        self.db
            .save_offline_transaction(&row)
            .map_err(|e| ReturnError::DatabaseError(e.to_string()))?;

        info!(
            "Saved return transaction {} to local database",
            return_txn.id
        );
        Ok(())
    }

    /// Syncs return transaction to server
    async fn sync_return_to_server(
        &self,
        return_txn: &Transaction,
        original_txn: &Transaction,
    ) -> PlatformSync {
        if !self.api.is_online().await.is_online() {
            debug!("Offline - return will be synced later");
            return PlatformSync::Queued;
        }

        let request = CreateReturnRequest {
            original_transaction_id: original_txn.id.clone(),
            items: return_txn
                .items
                .iter()
                .map(|i| ReturnItemRequest {
                    original_item_id: i.id.clone(),
                    quantity: i.quantity.abs(),
                    reason: "RETURN".to_string(),
                })
                .collect(),
            refund_method: return_txn
                .payments
                .first()
                .map(|p| p.method.as_str().to_string())
                .unwrap_or_else(|| "CASH".to_string()),
            refund_amount: return_txn.grand_total.abs(),
            operator_id: return_txn.operator_id.clone(),
            terminal_id: return_txn.terminal_id.clone(),
            shift_id: server_shift_id(&self.db, &return_txn.shift_id),
        };

        let platform = PlatformSync::of(
            self.api
                .create_return(&request)
                .await
                .map(|response| response.id),
        );

        match &platform {
            PlatformSync::Recorded(server_id) => {
                info!("Return recorded by the platform as {}", server_id);
                if let Err(e) = self.db.mark_transaction_synced(&return_txn.id, server_id) {
                    error!("Return {} synced but not marked: {}", return_txn.id, e);
                }
            }
            PlatformSync::Queued => debug!("Nobody answered; the return stays queued for replay"),
            // Recorded against the queued row by **code**, not by rendered message. The messages
            // arrive translated — Arabic is the fallback locale — and a queue that stores one and
            // later matches on it is the substring anti-pattern rebuilt in the database.
            PlatformSync::Refused(refused) => {
                error!(
                    "The platform refused the return ({}): {}",
                    refused.code, refused.message
                );
                if let Err(e) = self
                    .db
                    .mark_transaction_failed(&return_txn.id, &refused.code.to_string())
                {
                    error!(
                        "Could not record the refusal against {}: {}",
                        return_txn.id, e
                    );
                }
            }
            PlatformSync::Undetermined => error!(
                "The platform answered the return with a body this till cannot read; \
                 leaving it queued for a human rather than guessing"
            ),
        }

        platform
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pos_models::cart::CartItem;
    use pos_models::product::{Product, ProductUnit};
    use rust_decimal::Decimal;

    fn op_id(id: &str) -> OperatorId {
        OperatorId::new(id).expect("a fixture id is never blank")
    }

    fn op_name(name: &str) -> RecordedOperatorName {
        RecordedOperatorName::new(name).expect("a fixture name is never blank")
    }

    fn create_test_product() -> Product {
        Product {
            id: "prod-001".to_string(),
            sku: "SKU001".to_string(),
            barcode: Some("1234567890123".to_string()),
            name: "Test Product".to_string(),
            name_ar: Some("منتج تجريبي".to_string()),
            price: Decimal::from(10),
            tax_rate: Decimal::from(15),
            tax_inclusive: false,
            unit: ProductUnit::Unit,
            ..Default::default()
        }
    }

    fn create_test_transaction() -> Transaction {
        let product = create_test_product();
        let mut cart = pos_models::cart::Cart::new();
        cart.items.push(CartItem::new(&product, Decimal::from(2)));
        cart.recalculate();

        Transaction::from_cart(
            &cart,
            "LYD",
            "shift-1",
            "TERM-001",
            op_id("op-1"),
            op_name("Ahmed"),
        )
    }

    #[test]
    fn test_return_eligibility_eligible() {
        let mut txn = create_test_transaction();
        txn.status = TransactionStatus::Completed;
        txn.completed_at = Some(Utc::now());

        let api = Arc::new(ApiClient::new("http://localhost"));
        let db = Arc::new(Database::in_memory().unwrap());
        let service = ReturnService::new(api, db);

        let eligibility = service.check_eligibility(&txn);
        assert_eq!(eligibility, ReturnEligibility::Eligible);
    }

    #[test]
    fn test_return_eligibility_voided() {
        let mut txn = create_test_transaction();
        txn.status = TransactionStatus::Voided;

        let api = Arc::new(ApiClient::new("http://localhost"));
        let db = Arc::new(Database::in_memory().unwrap());
        let service = ReturnService::new(api, db);

        let eligibility = service.check_eligibility(&txn);
        assert!(matches!(eligibility, ReturnEligibility::NotEligible(_)));
    }

    #[test]
    fn test_return_eligibility_already_returned() {
        let mut txn = create_test_transaction();
        txn.status = TransactionStatus::Returned;

        let api = Arc::new(ApiClient::new("http://localhost"));
        let db = Arc::new(Database::in_memory().unwrap());
        let service = ReturnService::new(api, db);

        let eligibility = service.check_eligibility(&txn);
        assert!(matches!(eligibility, ReturnEligibility::NotEligible(_)));
    }

    #[test]
    fn test_return_eligibility_old_transaction() {
        let mut txn = create_test_transaction();
        txn.status = TransactionStatus::Completed;
        // Set completed_at to 60 days ago
        txn.completed_at = Some(Utc::now() - chrono::Duration::days(60));

        let api = Arc::new(ApiClient::new("http://localhost"));
        let db = Arc::new(Database::in_memory().unwrap());
        let service = ReturnService::new(api, db);

        let eligibility = service.check_eligibility(&txn);
        assert!(matches!(
            eligibility,
            ReturnEligibility::RequiresApproval(_)
        ));
    }

    #[test]
    fn test_return_item_from_transaction_item() {
        let product = create_test_product();
        let cart_item = CartItem::new(&product, Decimal::from(2));
        let txn_item = TransactionItem::from_cart_item(&cart_item);

        let return_item =
            ReturnItem::from_transaction_item(&txn_item, Decimal::ONE, ReturnReason::Defective);

        assert_eq!(return_item.quantity, Decimal::ONE);
        assert_eq!(return_item.reason, ReturnReason::Defective);
        // Refund should be half since we're returning 1 of 2
        assert_eq!(
            return_item.refund_amount,
            txn_item.line_total / Decimal::from(2)
        );
    }

    #[test]
    fn test_return_reason_conversion() {
        assert_eq!(ReturnReason::Defective.as_str(), "DEFECTIVE");
        assert_eq!("DEFECTIVE".parse(), Ok(ReturnReason::Defective));
        assert_eq!(
            "invalid".parse::<ReturnReason>(),
            Err(ParseError::ReturnReason("invalid".to_string()))
        );
    }

    #[test]
    fn test_return_reason_display_name() {
        assert_eq!(
            ReturnReason::Defective.display_name("en"),
            "Defective/Damaged"
        );
        assert_eq!(ReturnReason::Defective.display_name("ar"), "معيب أو تالف");
    }

    #[test]
    fn test_return_reason_requires_approval() {
        assert!(!ReturnReason::Defective.requires_approval());
        assert!(ReturnReason::ChangedMind.requires_approval());
        assert!(ReturnReason::Other.requires_approval());
    }

    #[test]
    fn test_refund_method_conversion() {
        assert_eq!(RefundMethod::Cash.as_str(), "CASH");
        assert_eq!("CASH".parse(), Ok(RefundMethod::Cash));
        assert_eq!(
            "invalid".parse::<RefundMethod>(),
            Err(ParseError::RefundMethod("invalid".to_string()))
        );
    }

    #[test]
    fn test_return_eligibility_is_allowed() {
        assert!(ReturnEligibility::Eligible.is_allowed());
        assert!(ReturnEligibility::RequiresApproval("test".to_string()).is_allowed());
        assert!(!ReturnEligibility::NotEligible("test".to_string()).is_allowed());
    }

    #[test]
    fn test_calculate_refund_total() {
        let api = Arc::new(ApiClient::new("http://localhost"));
        let db = Arc::new(Database::in_memory().unwrap());
        let service = ReturnService::new(api, db);

        let items = vec![
            ReturnItem {
                original_item_id: "1".to_string(),
                product_id: "p1".to_string(),
                product_name: "Product 1".to_string(),
                product_name_ar: None,
                sku: "SKU1".to_string(),
                quantity: Decimal::ONE,
                unit_price: Decimal::from(10),
                tax_rate: Decimal::from(15),
                refund_amount: Decimal::new(115, 1), // 11.5
                reason: ReturnReason::Defective,
                notes: None,
            },
            ReturnItem {
                original_item_id: "2".to_string(),
                product_id: "p2".to_string(),
                product_name: "Product 2".to_string(),
                product_name_ar: None,
                sku: "SKU2".to_string(),
                quantity: Decimal::from(2),
                unit_price: Decimal::from(5),
                tax_rate: Decimal::from(15),
                refund_amount: Decimal::new(115, 1), // 11.5
                reason: ReturnReason::WrongProduct,
                notes: None,
            },
        ];

        let total = service.calculate_refund_total(&items);
        assert_eq!(total, Decimal::from(23));
    }

    #[test]
    fn test_validate_return_items_empty() {
        let api = Arc::new(ApiClient::new("http://localhost"));
        let db = Arc::new(Database::in_memory().unwrap());
        let service = ReturnService::new(api, db);

        let txn = create_test_transaction();
        let result = service.validate_return_items(&txn, &[]);

        assert!(matches!(result, Err(ReturnError::NoItemsSelected)));
    }

    #[test]
    fn test_validate_return_items_invalid_quantity() {
        let api = Arc::new(ApiClient::new("http://localhost"));
        let db = Arc::new(Database::in_memory().unwrap());
        let service = ReturnService::new(api, db);

        let txn = create_test_transaction();
        let item_id = txn.items[0].id.clone();

        let items = vec![ReturnItem {
            original_item_id: item_id,
            product_id: "p1".to_string(),
            product_name: "Product".to_string(),
            product_name_ar: None,
            sku: "SKU".to_string(),
            quantity: Decimal::from(100), // More than original
            unit_price: Decimal::from(10),
            tax_rate: Decimal::from(15),
            refund_amount: Decimal::from(1150),
            reason: ReturnReason::Defective,
            notes: None,
        }];

        let result = service.validate_return_items(&txn, &items);
        assert!(matches!(result, Err(ReturnError::InvalidQuantity(_))));
    }

    #[test]
    fn test_generate_return_number() {
        let api = Arc::new(ApiClient::new("http://localhost"));
        let db = Arc::new(Database::in_memory().unwrap());
        let service = ReturnService::new(api, db);

        let number = service.generate_return_number("TERM-001");
        assert!(number.starts_with("RTN-TERM-001-"));
    }
}
