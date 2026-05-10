//! Transaction Service
//!
//! Handles transaction creation, completion, and synchronization.
//! Supports offline operation by queueing transactions when disconnected.

use anyhow::{anyhow, Result};
use parking_lot::RwLock;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{info, warn};

use pos_api::ApiClient;
use pos_db::transactions::{OfflineTransactionRow, SyncStatus};
use pos_db::{Database, SyncResource};
use pos_models::cart::Cart;
use pos_models::transaction::{generate_receipt_number, Payment, Transaction, TransactionStatus};

/// Error types for transaction operations
#[derive(Debug, Clone)]
pub enum TransactionError {
    /// Transaction is empty (no items)
    EmptyTransaction,
    /// Transaction not fully paid
    NotFullyPaid,
    /// Transaction already completed
    AlreadyCompleted,
    /// Transaction not found
    NotFound,
    /// Invalid transaction state
    InvalidState(String),
    /// Database error
    DatabaseError(String),
    /// Network error (offline)
    NetworkError(String),
    /// API error from server
    ApiError(String),
}

impl std::fmt::Display for TransactionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TransactionError::EmptyTransaction => write!(f, "Transaction has no items"),
            TransactionError::NotFullyPaid => write!(f, "Transaction not fully paid"),
            TransactionError::AlreadyCompleted => write!(f, "Transaction already completed"),
            TransactionError::NotFound => write!(f, "Transaction not found"),
            TransactionError::InvalidState(s) => write!(f, "Invalid transaction state: {}", s),
            TransactionError::DatabaseError(s) => write!(f, "Database error: {}", s),
            TransactionError::NetworkError(s) => write!(f, "Network error: {}", s),
            TransactionError::ApiError(s) => write!(f, "API error: {}", s),
        }
    }
}

impl std::error::Error for TransactionError {}

/// Result type for transaction operations
pub type TransactionResult<T> = std::result::Result<T, TransactionError>;

/// Result of completing a transaction
#[derive(Debug, Clone)]
pub struct CompleteResult {
    /// The completed transaction ID
    pub transaction_id: String,
    /// The transaction number
    pub transaction_number: String,
    /// The receipt number
    pub receipt_number: String,
    /// Whether the transaction was synced online
    pub synced: bool,
    /// Server ID if synced
    pub server_id: Option<String>,
    /// Change due to customer
    pub change_due: Decimal,
}

/// API request/response DTOs for transaction sync

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CreateTransactionRequest {
    transaction_number: String,
    transaction_type: String,
    items: Vec<TransactionItemDto>,
    payments: Vec<PaymentDto>,
    subtotal: Decimal,
    tax_total: Decimal,
    discount_total: Decimal,
    grand_total: Decimal,
    currency: String,
    customer_id: Option<String>,
    customer_name: Option<String>,
    shift_id: String,
    terminal_id: String,
    operator_id: String,
    note: Option<String>,
    created_at: String,
    completed_at: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct TransactionItemDto {
    product_id: String,
    product_name: String,
    sku: String,
    quantity: Decimal,
    unit_price: Decimal,
    tax_rate: Decimal,
    tax_amount: Decimal,
    discount_amount: Decimal,
    line_total: Decimal,
    #[serde(skip_serializing_if = "Option::is_none")]
    product_type: Option<String>,
    inventory_deducted: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PaymentDto {
    method: String,
    amount: Decimal,
    currency: String,
    reference: Option<String>,
    card_last_four: Option<String>,
    card_type: Option<String>,
    auth_code: Option<String>,
    wallet_type: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
struct CreateTransactionResponse {
    id: String,
    receipt_number: String,
}

/// Transaction service for managing POS transactions
pub struct TransactionService {
    api: Arc<ApiClient>,
    db: Arc<Database>,
    current_transaction: RwLock<Option<Transaction>>,
    receipt_sequence: RwLock<u32>,
}

impl TransactionService {
    /// Creates a new transaction service
    pub fn new(api: Arc<ApiClient>, db: Arc<Database>) -> Self {
        Self {
            api,
            db,
            current_transaction: RwLock::new(None),
            receipt_sequence: RwLock::new(1),
        }
    }

    /// Creates a new transaction from a cart
    pub fn create_from_cart(
        &self,
        cart: &Cart,
        currency: &str,
        shift_id: &str,
        terminal_id: &str,
        operator_id: &str,
        operator_name: &str,
    ) -> TransactionResult<Transaction> {
        if cart.is_empty() {
            return Err(TransactionError::EmptyTransaction);
        }

        let txn = Transaction::from_cart(
            cart,
            currency,
            shift_id,
            terminal_id,
            operator_id,
            operator_name,
        );

        // Store as current transaction
        let mut current = self.current_transaction.write();
        *current = Some(txn.clone());

        Ok(txn)
    }

    /// Gets the current transaction (if any)
    pub fn get_current(&self) -> Option<Transaction> {
        self.current_transaction.read().clone()
    }

    /// Clears the current transaction
    pub fn clear_current(&self) {
        let mut current = self.current_transaction.write();
        *current = None;
    }

    /// Adds a payment to the current transaction
    pub fn add_payment(&self, payment: Payment) -> TransactionResult<()> {
        let mut current = self.current_transaction.write();
        match current.as_mut() {
            Some(txn) => {
                txn.add_payment(payment);
                Ok(())
            }
            None => Err(TransactionError::NotFound),
        }
    }

    /// Adds a cash payment to the current transaction
    pub fn add_cash_payment(&self, amount: Decimal) -> TransactionResult<()> {
        let mut current = self.current_transaction.write();
        match current.as_mut() {
            Some(txn) => {
                txn.add_cash_payment(amount);
                Ok(())
            }
            None => Err(TransactionError::NotFound),
        }
    }

    /// Adds a card payment to the current transaction
    pub fn add_card_payment(
        &self,
        amount: Decimal,
        card_last_four: &str,
        card_type: &str,
        auth_code: &str,
    ) -> TransactionResult<()> {
        let mut current = self.current_transaction.write();
        match current.as_mut() {
            Some(txn) => {
                txn.add_card_payment(amount, card_last_four, card_type, auth_code);
                Ok(())
            }
            None => Err(TransactionError::NotFound),
        }
    }

    /// Adds a wallet payment to the current transaction
    pub fn add_wallet_payment(
        &self,
        amount: Decimal,
        wallet_type: &str,
        transaction_id: &str,
    ) -> TransactionResult<()> {
        let mut current = self.current_transaction.write();
        match current.as_mut() {
            Some(txn) => {
                txn.add_wallet_payment(amount, wallet_type, transaction_id);
                Ok(())
            }
            None => Err(TransactionError::NotFound),
        }
    }

    /// Gets the current transaction's remaining amount
    pub fn remaining(&self) -> Decimal {
        self.current_transaction
            .read()
            .as_ref()
            .map(|t| t.remaining())
            .unwrap_or(Decimal::ZERO)
    }

    /// Checks if the current transaction is fully paid
    pub fn is_fully_paid(&self) -> bool {
        self.current_transaction
            .read()
            .as_ref()
            .map(|t| t.is_fully_paid())
            .unwrap_or(false)
    }

    /// Gets the change due for the current transaction
    pub fn change_due(&self) -> Decimal {
        self.current_transaction
            .read()
            .as_ref()
            .map(|t| t.change_due())
            .unwrap_or(Decimal::ZERO)
    }

    /// Completes the current transaction
    ///
    /// This will:
    /// 1. Validate the transaction is fully paid
    /// 2. Generate receipt number
    /// 3. CRITICAL: Persist to SQLite BEFORE marking complete (data loss prevention)
    /// 4. Mark the transaction as completed in memory
    /// 5. Try to sync to the backend (safe to fail - already persisted)
    /// 6. Clear current transaction
    pub async fn complete(&self) -> TransactionResult<CompleteResult> {
        // 1. Get and validate current transaction
        let mut txn = {
            let current = self.current_transaction.read();
            match current.as_ref() {
                Some(t) => t.clone(),
                None => return Err(TransactionError::NotFound),
            }
        };

        // Validate transaction state
        if txn.items.is_empty() {
            return Err(TransactionError::EmptyTransaction);
        }
        if !txn.is_fully_paid() {
            return Err(TransactionError::NotFullyPaid);
        }
        if txn.status != TransactionStatus::Draft {
            return Err(TransactionError::AlreadyCompleted);
        }

        // 2. Generate receipt number FIRST
        let receipt_number = {
            let mut seq = self.receipt_sequence.write();
            let num = generate_receipt_number(&txn.terminal_id, *seq);
            *seq += 1;
            num
        };
        txn.set_receipt_number(&receipt_number);

        // 3. CRITICAL: Persist to SQLite BEFORE marking complete
        // This ensures we never lose a paid transaction even if power fails
        self.queue_offline(&txn)
            .map_err(|e| TransactionError::DatabaseError(e.to_string()))?;

        // 4. Now safe to mark complete in memory
        txn.complete()
            .map_err(|e| TransactionError::InvalidState(e.to_string()))?;

        // 5. Try online sync (safe to fail - already in queue)
        let (synced, server_id) = match self.sync_to_backend(&txn).await {
            Ok(response) => {
                info!(
                    "Transaction synced: {} -> {}",
                    txn.transaction_number, response.id
                );
                // Update the queued record with server_id
                if let Err(e) = self.db.mark_transaction_synced(&txn.id, &response.id) {
                    warn!("Failed to mark transaction as synced: {}", e);
                }
                (true, Some(response.id))
            }
            Err(e) => {
                info!("Online sync deferred, queued for later: {}", e);
                (false, None)
            }
        };

        // 6. Store change due before clearing
        let change_due = txn.change_due();

        // 7. Clear current transaction
        self.clear_current();

        Ok(CompleteResult {
            transaction_id: txn.id,
            transaction_number: txn.transaction_number,
            receipt_number,
            synced,
            server_id,
            change_due,
        })
    }

    /// Syncs a transaction to the backend API
    async fn sync_to_backend(
        &self,
        txn: &Transaction,
    ) -> Result<CreateTransactionResponse> {
        let request = CreateTransactionRequest {
            transaction_number: txn.transaction_number.clone(),
            transaction_type: txn.transaction_type.clone(),
            items: txn
                .items
                .iter()
                .map(|i| TransactionItemDto {
                    product_id: i.product_id.clone(),
                    product_name: i.product_name.clone(),
                    sku: i.sku.clone(),
                    quantity: i.quantity,
                    unit_price: i.unit_price,
                    tax_rate: i.tax_rate,
                    tax_amount: i.tax_amount,
                    discount_amount: i.discount_amount,
                    line_total: i.line_total,
                    product_type: if i.product_type.is_empty() { None } else { Some(i.product_type.clone()) },
                    inventory_deducted: i.inventory_deducted,
                })
                .collect(),
            payments: txn
                .payments
                .iter()
                .map(|p| PaymentDto {
                    method: p.method.as_str().to_lowercase(),
                    amount: p.amount,
                    currency: p.currency.clone(),
                    reference: p.reference.clone(),
                    card_last_four: p.card_last_four.clone(),
                    card_type: p.card_type.clone(),
                    auth_code: p.auth_code.clone(),
                    wallet_type: p.wallet_type.clone(),
                })
                .collect(),
            subtotal: txn.subtotal,
            tax_total: txn.tax_total,
            discount_total: txn.discount_total,
            grand_total: txn.grand_total,
            currency: txn.currency.clone(),
            customer_id: txn.customer_id.clone(),
            customer_name: txn.customer_name.clone(),
            shift_id: txn.shift_id.clone(),
            terminal_id: txn.terminal_id.clone(),
            operator_id: txn.operator_id.clone(),
            note: txn.note.clone(),
            created_at: txn.created_at.to_rfc3339(),
            completed_at: txn.completed_at.map(|dt| dt.to_rfc3339()),
        };

        self.api
            .post::<CreateTransactionRequest, CreateTransactionResponse>(
                "/api/pos/transactions",
                &request,
            )
            .await
    }

    /// Queues a transaction for offline sync
    fn queue_offline(&self, txn: &Transaction) -> Result<()> {
        let items_json = serde_json::to_string(&txn.items)?;
        let payments_json = serde_json::to_string(&txn.payments)?;

        // Get current catalog ETag for price version tracking
        let catalog_etag = self.db.get_etag(SyncResource::Products).ok().flatten();

        let row = OfflineTransactionRow {
            offline_id: txn.id.clone(),
            transaction_number: Some(txn.transaction_number.clone()),
            transaction_type: txn.transaction_type.to_uppercase(),
            items_json,
            payments_json,
            subtotal: txn.subtotal,
            tax_total: txn.tax_total,
            discount_total: txn.discount_total,
            grand_total: txn.grand_total,
            customer_id: txn.customer_id.clone(),
            customer_name: txn.customer_name.clone(),
            shift_id: Some(txn.shift_id.clone()),
            operator_id: Some(txn.operator_id.clone()),
            terminal_id: Some(txn.terminal_id.clone()),
            receipt_number: txn.receipt_number.clone(),
            notes: txn.note.clone(),
            created_at: txn.created_at.to_rfc3339(),
            sync_status: SyncStatus::Pending.as_str().to_string(),
            server_id: None,
            retry_count: 0,
            last_error: None,
            last_retry_at: None,
            catalog_etag,
        };

        self.db
            .save_offline_transaction(&row)
            .map_err(|e| anyhow!("Failed to save offline transaction: {}", e))
    }

    /// Voids the current transaction
    pub fn void(&self, reason: &str) -> TransactionResult<()> {
        let mut current = self.current_transaction.write();
        match current.as_mut() {
            Some(txn) => {
                txn.void(reason)
                    .map_err(|e| TransactionError::InvalidState(e.to_string()))?;
                // Clear the transaction
                *current = None;
                Ok(())
            }
            None => Err(TransactionError::NotFound),
        }
    }

    /// Voids a completed transaction by ID
    pub async fn void_transaction(&self, txn_id: &str, reason: &str) -> TransactionResult<()> {
        // Update local database
        self.db
            .update_transaction_sync_status(
                txn_id,
                SyncStatus::Pending,
                None,
                Some(&format!("VOIDED: {}", reason)),
            )
            .map_err(|e| TransactionError::DatabaseError(e.to_string()))?;

        // Try to sync void to backend (fire-and-forget)
        let api = Arc::clone(&self.api);
        let txn_id_owned = txn_id.to_string();
        let reason_owned = reason.to_string();
        tokio::spawn(async move {
            #[derive(Serialize)]
            struct VoidRequest {
                reason: String,
            }
            #[derive(Deserialize)]
            #[allow(dead_code)]
            struct VoidResponse {
                success: bool,
            }

            let _result: Result<VoidResponse> = api
                .post(
                    &format!("/api/pos/transactions/{}/void", txn_id_owned),
                    &VoidRequest {
                        reason: reason_owned,
                    },
                )
                .await;
        });

        Ok(())
    }

    /// Gets a transaction by ID from the offline queue
    pub fn get(&self, id: &str) -> TransactionResult<Option<OfflineTransactionRow>> {
        self.db
            .get_offline_transaction(id)
            .map_err(|e| TransactionError::DatabaseError(e.to_string()))
    }

    /// Gets transactions for a shift
    pub fn get_shift_transactions(
        &self,
        shift_id: &str,
    ) -> TransactionResult<Vec<OfflineTransactionRow>> {
        self.db
            .get_transactions_by_shift(shift_id)
            .map_err(|e| TransactionError::DatabaseError(e.to_string()))
    }

    /// Gets the count of pending (unsynced) transactions
    pub fn pending_count(&self) -> TransactionResult<i64> {
        self.db
            .get_pending_transaction_count()
            .map_err(|e| TransactionError::DatabaseError(e.to_string()))
    }

    /// Calculates shift totals from completed transactions
    pub fn calculate_shift_totals(&self, shift_id: &str) -> TransactionResult<ShiftTotals> {
        let transactions = self.get_shift_transactions(shift_id)?;

        let mut totals = ShiftTotals::default();

        for txn in transactions {
            // Only count completed sales (not voided/returned)
            if txn.sync_status != "VOIDED" {
                match txn.transaction_type.to_uppercase().as_str() {
                    "SALE" => {
                        totals.sales_count += 1;
                        totals.sales_total += txn.grand_total;
                        totals.tax_total += txn.tax_total;
                        totals.discount_total += txn.discount_total;

                        // Parse payments to get breakdown
                        if let Ok(payments) = serde_json::from_str::<Vec<serde_json::Value>>(&txn.payments_json) {
                            for payment in payments {
                                let method = payment
                                    .get("method")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("")
                                    .to_uppercase();
                                let amount = payment
                                    .get("amount")
                                    .and_then(|v| v.as_f64())
                                    .map(|f| Decimal::from_f64_retain(f).unwrap_or(Decimal::ZERO))
                                    .unwrap_or(Decimal::ZERO);

                                match method.as_str() {
                                    "CASH" => totals.cash_total += amount,
                                    "CARD" => totals.card_total += amount,
                                    "WALLET" => totals.wallet_total += amount,
                                    "CREDIT" => totals.credit_total += amount,
                                    _ => totals.other_total += amount,
                                }
                            }
                        }
                    }
                    "RETURN" => {
                        totals.return_count += 1;
                        totals.return_total += txn.grand_total;
                    }
                    "VOID" => {
                        totals.void_count += 1;
                    }
                    _ => {}
                }
            }
        }

        Ok(totals)
    }
}

/// Shift totals summary
#[derive(Debug, Clone)]
pub struct ShiftTotals {
    /// Number of sales transactions
    pub sales_count: u32,
    /// Total sales amount
    pub sales_total: Decimal,
    /// Total tax collected
    pub tax_total: Decimal,
    /// Total discounts given
    pub discount_total: Decimal,
    /// Total cash received
    pub cash_total: Decimal,
    /// Total card payments
    pub card_total: Decimal,
    /// Total wallet payments
    pub wallet_total: Decimal,
    /// Total credit (on account) sales
    pub credit_total: Decimal,
    /// Total other payments
    pub other_total: Decimal,
    /// Number of returns processed
    pub return_count: u32,
    /// Total returns amount
    pub return_total: Decimal,
    /// Number of voided transactions
    pub void_count: u32,
}

impl Default for ShiftTotals {
    fn default() -> Self {
        Self {
            sales_count: 0,
            sales_total: Decimal::ZERO,
            tax_total: Decimal::ZERO,
            discount_total: Decimal::ZERO,
            cash_total: Decimal::ZERO,
            card_total: Decimal::ZERO,
            wallet_total: Decimal::ZERO,
            credit_total: Decimal::ZERO,
            other_total: Decimal::ZERO,
            return_count: 0,
            return_total: Decimal::ZERO,
            void_count: 0,
        }
    }
}

impl ShiftTotals {
    /// Returns net cash (cash - returns)
    pub fn net_cash(&self) -> Decimal {
        self.cash_total - self.return_total
    }

    /// Returns net sales (sales - returns)
    pub fn net_sales(&self) -> Decimal {
        self.sales_total - self.return_total
    }

    /// Returns total payments
    pub fn total_payments(&self) -> Decimal {
        self.cash_total + self.card_total + self.wallet_total + self.credit_total + self.other_total
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pos_db::migrations::run_migrations;
    use pos_models::cart::CartItem;
    use pos_models::product::{Product, ProductUnit};
    use rust_decimal::Decimal;

    fn create_test_product() -> Product {
        Product {
            id: "prod-001".to_string(),
            sku: "SKU001".to_string(),
            name: "Test Product".to_string(),
            name_ar: Some("منتج تجريبي".to_string()),
            price: Decimal::from(10),
            tax_rate: Decimal::from(15),
            tax_inclusive: false,
            unit: ProductUnit::Unit,
            ..Default::default()
        }
    }

    fn create_test_cart() -> Cart {
        let product = create_test_product();
        let mut cart = Cart::new();
        cart.items.push(CartItem::new(&product, Decimal::from(2)));
        cart.recalculate();
        cart
    }

    fn setup_test_service() -> TransactionService {
        let db = Database::in_memory().expect("Failed to create in-memory database");
        {
            let conn = db.connection();
            let conn = conn.lock();
            run_migrations(&conn).expect("Failed to run migrations");
        }
        let api = ApiClient::new("https://test.example.com");
        TransactionService::new(Arc::new(api), Arc::new(db))
    }

    #[test]
    fn test_create_from_cart() {
        let service = setup_test_service();
        let cart = create_test_cart();

        let result = service.create_from_cart(&cart, "LYD", "shift-1", "TERM-001", "op-1", "Ahmed");

        assert!(result.is_ok());
        let txn = result.unwrap();
        assert_eq!(txn.items.len(), 1);
        assert_eq!(txn.grand_total, Decimal::from(23));
        assert_eq!(txn.status, TransactionStatus::Draft);
    }

    #[test]
    fn test_create_from_empty_cart() {
        let service = setup_test_service();
        let cart = Cart::new();

        let result = service.create_from_cart(&cart, "LYD", "shift-1", "TERM-001", "op-1", "Ahmed");

        assert!(matches!(result, Err(TransactionError::EmptyTransaction)));
    }

    #[test]
    fn test_get_current() {
        let service = setup_test_service();
        let cart = create_test_cart();

        // Initially no current transaction
        assert!(service.get_current().is_none());

        // Create transaction
        service
            .create_from_cart(&cart, "LYD", "shift-1", "TERM-001", "op-1", "Ahmed")
            .unwrap();

        // Now should have current transaction
        assert!(service.get_current().is_some());

        // Clear
        service.clear_current();
        assert!(service.get_current().is_none());
    }

    #[test]
    fn test_add_cash_payment() {
        let service = setup_test_service();
        let cart = create_test_cart();

        service
            .create_from_cart(&cart, "LYD", "shift-1", "TERM-001", "op-1", "Ahmed")
            .unwrap();

        let result = service.add_cash_payment(Decimal::from(25));
        assert!(result.is_ok());

        assert!(service.is_fully_paid());
        assert_eq!(service.change_due(), Decimal::from(2));
    }

    #[test]
    fn test_add_card_payment() {
        let service = setup_test_service();
        let cart = create_test_cart();

        service
            .create_from_cart(&cart, "LYD", "shift-1", "TERM-001", "op-1", "Ahmed")
            .unwrap();

        let result = service.add_card_payment(Decimal::from(23), "1234", "VISA", "AUTH123");
        assert!(result.is_ok());

        assert!(service.is_fully_paid());
    }

    #[test]
    fn test_add_payment_no_transaction() {
        let service = setup_test_service();

        let result = service.add_cash_payment(Decimal::from(100));
        assert!(matches!(result, Err(TransactionError::NotFound)));
    }

    #[test]
    fn test_void_current() {
        let service = setup_test_service();
        let cart = create_test_cart();

        service
            .create_from_cart(&cart, "LYD", "shift-1", "TERM-001", "op-1", "Ahmed")
            .unwrap();

        let result = service.void("Customer changed mind");
        assert!(result.is_ok());

        // Current transaction should be cleared
        assert!(service.get_current().is_none());
    }

    #[test]
    fn test_void_no_transaction() {
        let service = setup_test_service();

        let result = service.void("No transaction");
        assert!(matches!(result, Err(TransactionError::NotFound)));
    }

    #[test]
    fn test_remaining_and_change() {
        let service = setup_test_service();
        let cart = create_test_cart();

        service
            .create_from_cart(&cart, "LYD", "shift-1", "TERM-001", "op-1", "Ahmed")
            .unwrap();

        // Grand total is 23
        assert_eq!(service.remaining(), Decimal::from(23));
        assert_eq!(service.change_due(), Decimal::ZERO);

        // Partial payment
        service.add_cash_payment(Decimal::from(10)).unwrap();
        assert_eq!(service.remaining(), Decimal::from(13));
        assert_eq!(service.change_due(), Decimal::ZERO);

        // Overpay
        service.add_cash_payment(Decimal::from(20)).unwrap();
        assert_eq!(service.remaining(), Decimal::ZERO);
        assert_eq!(service.change_due(), Decimal::from(7));
    }

    #[test]
    fn test_split_payment() {
        let service = setup_test_service();
        let cart = create_test_cart();

        service
            .create_from_cart(&cart, "LYD", "shift-1", "TERM-001", "op-1", "Ahmed")
            .unwrap();

        // Split between cash and card
        service.add_cash_payment(Decimal::from(10)).unwrap();
        assert!(!service.is_fully_paid());

        service.add_card_payment(Decimal::from(13), "5678", "MASTERCARD", "AUTH456").unwrap();
        assert!(service.is_fully_paid());
        assert_eq!(service.remaining(), Decimal::ZERO);
    }

    #[test]
    fn test_pending_count() {
        let service = setup_test_service();

        let count = service.pending_count();
        assert!(count.is_ok());
        assert_eq!(count.unwrap(), 0);
    }

    #[test]
    fn test_shift_totals_default() {
        let totals = ShiftTotals::default();

        assert_eq!(totals.sales_count, 0);
        assert_eq!(totals.sales_total, Decimal::ZERO);
        assert_eq!(totals.net_cash(), Decimal::ZERO);
        assert_eq!(totals.net_sales(), Decimal::ZERO);
        assert_eq!(totals.total_payments(), Decimal::ZERO);
    }

    #[test]
    fn test_shift_totals_calculations() {
        let totals = ShiftTotals {
            sales_total: Decimal::from(1000),
            cash_total: Decimal::from(600),
            card_total: Decimal::from(400),
            return_total: Decimal::from(100),
            ..ShiftTotals::default()
        };

        assert_eq!(totals.net_cash(), Decimal::from(500));
        assert_eq!(totals.net_sales(), Decimal::from(900));
        assert_eq!(totals.total_payments(), Decimal::from(1000));
    }

    #[test]
    fn test_transaction_error_display() {
        assert_eq!(
            TransactionError::EmptyTransaction.to_string(),
            "Transaction has no items"
        );
        assert_eq!(
            TransactionError::NotFullyPaid.to_string(),
            "Transaction not fully paid"
        );
        assert_eq!(
            TransactionError::AlreadyCompleted.to_string(),
            "Transaction already completed"
        );
        assert_eq!(
            TransactionError::NotFound.to_string(),
            "Transaction not found"
        );
    }
}
