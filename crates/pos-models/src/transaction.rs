//! Transaction Domain Models
//!
//! This module contains the transaction-related domain models for the POS terminal.
//! Transactions represent completed or in-progress sales, returns, and exchanges.

use std::str::FromStr;

use chrono::{DateTime, Utc};
use rust_decimal::prelude::*;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::cart::{Cart, CartItem};
use crate::operator::{OperatorId, RecordedOperatorName};
use crate::parse::ParseError;

/// Transaction status
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum TransactionStatus {
    /// Transaction is being created (not yet completed)
    #[default]
    Draft,
    /// Transaction completed successfully
    Completed,
    /// Transaction was voided/cancelled
    Voided,
    /// Transaction was a return (refund)
    Returned,
}

impl TransactionStatus {
    /// Returns the status as a string for database storage
    pub fn as_str(&self) -> &'static str {
        match self {
            TransactionStatus::Draft => "DRAFT",
            TransactionStatus::Completed => "COMPLETED",
            TransactionStatus::Voided => "VOIDED",
            TransactionStatus::Returned => "RETURNED",
        }
    }
}

impl FromStr for TransactionStatus {
    type Err = ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_uppercase().as_str() {
            "DRAFT" => Ok(TransactionStatus::Draft),
            "COMPLETED" => Ok(TransactionStatus::Completed),
            "VOIDED" => Ok(TransactionStatus::Voided),
            "RETURNED" => Ok(TransactionStatus::Returned),
            _ => Err(ParseError::TransactionStatus(s.to_string())),
        }
    }
}

/// Payment method types
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum PaymentMethod {
    /// Cash payment
    #[default]
    Cash,
    /// Card payment (credit/debit via EMV)
    Card,
    /// Mobile wallet (e.g., Sadad, MobiCash)
    Wallet,
    /// Credit (customer account)
    Credit,
    /// Other payment method
    Other,
}

impl PaymentMethod {
    /// Returns the method as a string for database/API
    pub fn as_str(&self) -> &'static str {
        match self {
            PaymentMethod::Cash => "CASH",
            PaymentMethod::Card => "CARD",
            PaymentMethod::Wallet => "WALLET",
            PaymentMethod::Credit => "CREDIT",
            PaymentMethod::Other => "OTHER",
        }
    }

    /// Returns the display name for this payment method
    pub fn display_name(&self, locale: &str) -> &'static str {
        if locale == "ar" {
            match self {
                PaymentMethod::Cash => "نقدي",
                PaymentMethod::Card => "بطاقة",
                PaymentMethod::Wallet => "محفظة",
                PaymentMethod::Credit => "آجل",
                PaymentMethod::Other => "أخرى",
            }
        } else {
            match self {
                PaymentMethod::Cash => "Cash",
                PaymentMethod::Card => "Card",
                PaymentMethod::Wallet => "Wallet",
                PaymentMethod::Credit => "Credit",
                PaymentMethod::Other => "Other",
            }
        }
    }
}

impl FromStr for PaymentMethod {
    type Err = ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_uppercase().as_str() {
            "CASH" => Ok(PaymentMethod::Cash),
            "CARD" => Ok(PaymentMethod::Card),
            "WALLET" => Ok(PaymentMethod::Wallet),
            "CREDIT" => Ok(PaymentMethod::Credit),
            "OTHER" => Ok(PaymentMethod::Other),
            _ => Err(ParseError::PaymentMethod(s.to_string())),
        }
    }
}

/// A single payment applied to a transaction
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Payment {
    /// Unique identifier for this payment
    pub id: String,
    /// Payment method used
    pub method: PaymentMethod,
    /// Amount paid
    pub amount: Decimal,
    /// Currency code (e.g., "LYD")
    pub currency: String,
    /// External reference (receipt number, auth code, etc.)
    pub reference: Option<String>,
    /// Last four digits of card (for card payments)
    pub card_last_four: Option<String>,
    /// Card type/brand (VISA, MASTERCARD, etc.)
    pub card_type: Option<String>,
    /// Authorization code from card processor
    pub auth_code: Option<String>,
    /// Wallet/provider type (for wallet payments)
    pub wallet_type: Option<String>,
    /// When this payment was created
    pub created_at: DateTime<Utc>,
}

impl Payment {
    /// Creates a new cash payment
    pub fn cash(amount: Decimal, currency: &str) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            method: PaymentMethod::Cash,
            amount,
            currency: currency.to_string(),
            reference: None,
            card_last_four: None,
            card_type: None,
            auth_code: None,
            wallet_type: None,
            created_at: Utc::now(),
        }
    }

    /// Creates a new card payment
    pub fn card(
        amount: Decimal,
        currency: &str,
        card_last_four: &str,
        card_type: &str,
        auth_code: &str,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            method: PaymentMethod::Card,
            amount,
            currency: currency.to_string(),
            reference: Some(auth_code.to_string()),
            card_last_four: Some(card_last_four.to_string()),
            card_type: Some(card_type.to_string()),
            auth_code: Some(auth_code.to_string()),
            wallet_type: None,
            created_at: Utc::now(),
        }
    }

    /// Creates a new wallet payment
    pub fn wallet(
        amount: Decimal,
        currency: &str,
        wallet_type: &str,
        transaction_id: &str,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            method: PaymentMethod::Wallet,
            amount,
            currency: currency.to_string(),
            reference: Some(transaction_id.to_string()),
            card_last_four: None,
            card_type: None,
            auth_code: None,
            wallet_type: Some(wallet_type.to_string()),
            created_at: Utc::now(),
        }
    }

    /// Creates a new credit payment (customer account)
    pub fn credit(amount: Decimal, currency: &str, customer_id: &str) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            method: PaymentMethod::Credit,
            amount,
            currency: currency.to_string(),
            reference: Some(customer_id.to_string()),
            card_last_four: None,
            card_type: None,
            auth_code: None,
            wallet_type: None,
            created_at: Utc::now(),
        }
    }

    /// Creates a new generic payment
    pub fn new(method: PaymentMethod, amount: Decimal, currency: &str) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            method,
            amount,
            currency: currency.to_string(),
            reference: None,
            card_last_four: None,
            card_type: None,
            auth_code: None,
            wallet_type: None,
            created_at: Utc::now(),
        }
    }
}

/// A single item in a transaction (snapshot of cart item at time of sale)
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TransactionItem {
    /// Unique identifier for this line item
    pub id: String,
    /// Reference to product ID
    pub product_id: String,
    /// Product name (English) - snapshot at time of sale
    pub product_name: String,
    /// Product name (Arabic) - snapshot at time of sale
    pub product_name_ar: Option<String>,
    /// SKU - snapshot at time of sale
    pub sku: String,
    /// Barcode - snapshot at time of sale
    pub barcode: Option<String>,
    /// Quantity sold
    pub quantity: Decimal,
    /// Unit of measure
    pub unit: String,
    /// Unit price at time of sale (before tax)
    pub unit_price: Decimal,
    /// Tax rate percentage
    pub tax_rate: Decimal,
    /// Tax amount for this line
    pub tax_amount: Decimal,
    /// Discount amount for this line
    pub discount_amount: Decimal,
    /// Line total after tax and discount
    pub line_total: Decimal,
    /// Product type at time of sale (Phase 3 Track H)
    #[serde(default)]
    pub product_type: String,
    /// Whether inventory was deducted for this line item
    #[serde(default)]
    pub inventory_deducted: bool,
}

impl TransactionItem {
    /// Creates a transaction item from a cart item
    pub fn from_cart_item(item: &CartItem) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            product_id: item.product_id.clone(),
            product_name: item.product_name.clone(),
            product_name_ar: item.product_name_ar.clone(),
            sku: item.sku.clone(),
            barcode: item.barcode.clone(),
            quantity: item.quantity,
            unit: item.unit.label("en").to_string(),
            unit_price: item.unit_price,
            tax_rate: item.tax_rate,
            tax_amount: item.line_tax,
            discount_amount: item.line_discount,
            line_total: item.line_total,
            product_type: item.product_type.as_str().to_string(),
            inventory_deducted: item.track_inventory,
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

    /// Returns the original price before discount
    pub fn original_total(&self) -> Decimal {
        (self.unit_price * self.quantity) + self.tax_amount
    }
}

/// A complete transaction
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Transaction {
    /// Unique identifier (UUID)
    pub id: String,
    /// Human-readable transaction number (e.g., "TXN2412121234")
    pub transaction_number: String,
    /// Transaction type (sale, return, exchange)
    pub transaction_type: String,
    /// Line items in this transaction
    pub items: Vec<TransactionItem>,
    /// Payments applied to this transaction
    pub payments: Vec<Payment>,
    /// Subtotal before tax
    pub subtotal: Decimal,
    /// Total tax amount
    pub tax_total: Decimal,
    /// Total discount amount
    pub discount_total: Decimal,
    /// Grand total after tax and discount
    pub grand_total: Decimal,
    /// Currency code
    pub currency: String,
    /// Associated customer ID (optional)
    pub customer_id: Option<String>,
    /// Associated customer name (optional)
    pub customer_name: Option<String>,
    /// Shift ID this transaction belongs to
    pub shift_id: String,
    /// Terminal ID where transaction occurred
    pub terminal_id: String,
    /// Operator who processed the transaction
    pub operator_id: OperatorId,
    /// The operator's name as it stood when this transaction was rung, for display and receipts.
    ///
    /// A snapshot, not a lookup: a receipt reprinted after the operator is renamed must still say
    /// what it said. See `RecordedOperatorName`.
    ///
    /// Optional because a transaction rebuilt out of the offline queue does not have one —
    /// `offline_transactions` keeps `operator_id` and no name column, so the reconstruction can
    /// only report the absence. `None` says the name was never recorded; it is not a stand-in
    /// for an operator without a name.
    pub operator_name: Option<RecordedOperatorName>,
    /// Current status
    pub status: TransactionStatus,
    /// When the transaction was created
    pub created_at: DateTime<Utc>,
    /// When the transaction was completed (paid)
    pub completed_at: Option<DateTime<Utc>>,
    /// When the transaction was voided (if applicable)
    pub voided_at: Option<DateTime<Utc>>,
    /// Reason for voiding (if applicable)
    pub void_reason: Option<String>,
    /// Optional note/comment
    pub note: Option<String>,
    /// Receipt number (assigned after completion)
    pub receipt_number: Option<String>,
    /// Server ID after sync (for offline transactions)
    pub server_id: Option<String>,
}

impl Transaction {
    /// Creates a new transaction
    pub fn new(
        transaction_type: &str,
        currency: &str,
        shift_id: &str,
        terminal_id: &str,
        operator_id: OperatorId,
        operator_name: RecordedOperatorName,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            transaction_number: generate_transaction_number(),
            transaction_type: transaction_type.to_string(),
            items: Vec::new(),
            payments: Vec::new(),
            subtotal: Decimal::ZERO,
            tax_total: Decimal::ZERO,
            discount_total: Decimal::ZERO,
            grand_total: Decimal::ZERO,
            currency: currency.to_string(),
            customer_id: None,
            customer_name: None,
            shift_id: shift_id.to_string(),
            terminal_id: terminal_id.to_string(),
            operator_id,
            operator_name: Some(operator_name),
            status: TransactionStatus::Draft,
            created_at: Utc::now(),
            completed_at: None,
            voided_at: None,
            void_reason: None,
            note: None,
            receipt_number: None,
            server_id: None,
        }
    }

    /// Creates a sale transaction from a cart
    pub fn from_cart(
        cart: &Cart,
        currency: &str,
        shift_id: &str,
        terminal_id: &str,
        operator_id: OperatorId,
        operator_name: RecordedOperatorName,
    ) -> Self {
        let mut txn = Self::new(
            "sale",
            currency,
            shift_id,
            terminal_id,
            operator_id,
            operator_name,
        );

        // Convert cart items to transaction items
        txn.items = cart
            .items
            .iter()
            .map(TransactionItem::from_cart_item)
            .collect();

        // Cart and Transaction both use Decimal — direct assignment
        txn.subtotal = cart.subtotal;
        txn.tax_total = cart.tax_total;
        txn.discount_total = cart.discount_total;
        txn.grand_total = cart.grand_total;

        // Copy customer if set
        txn.customer_id = cart.customer_id.clone();
        txn.customer_name = cart.customer_name.clone();

        // Copy note if set
        txn.note = cart.note.clone();

        txn
    }

    /// Adds a payment to the transaction
    pub fn add_payment(&mut self, payment: Payment) {
        self.payments.push(payment);
    }

    /// Adds a cash payment
    pub fn add_cash_payment(&mut self, amount: Decimal) {
        self.payments.push(Payment::cash(amount, &self.currency));
    }

    /// Adds a card payment
    pub fn add_card_payment(
        &mut self,
        amount: Decimal,
        card_last_four: &str,
        card_type: &str,
        auth_code: &str,
    ) {
        self.payments.push(Payment::card(
            amount,
            &self.currency,
            card_last_four,
            card_type,
            auth_code,
        ));
    }

    /// Adds a wallet payment
    pub fn add_wallet_payment(&mut self, amount: Decimal, wallet_type: &str, transaction_id: &str) {
        self.payments.push(Payment::wallet(
            amount,
            &self.currency,
            wallet_type,
            transaction_id,
        ));
    }

    /// Adds a credit payment
    pub fn add_credit_payment(&mut self, amount: Decimal) {
        if let Some(customer_id) = &self.customer_id {
            self.payments
                .push(Payment::credit(amount, &self.currency, customer_id));
        }
    }

    /// Returns total amount paid
    pub fn total_paid(&self) -> Decimal {
        self.payments.iter().map(|p| p.amount).sum()
    }

    /// Returns remaining amount to pay
    pub fn remaining(&self) -> Decimal {
        let remaining = self.grand_total - self.total_paid();
        if remaining <= Decimal::ZERO {
            Decimal::ZERO
        } else {
            remaining
        }
    }

    /// Returns true if the transaction is fully paid
    pub fn is_fully_paid(&self) -> bool {
        self.total_paid() >= self.grand_total
    }

    /// Returns the change due (for cash overpayment)
    pub fn change_due(&self) -> Decimal {
        let paid = self.total_paid();
        if paid > self.grand_total {
            paid - self.grand_total
        } else {
            Decimal::ZERO
        }
    }

    /// Returns the number of items in the transaction
    pub fn item_count(&self) -> Decimal {
        self.items.iter().map(|i| i.quantity).sum()
    }

    /// Returns the number of line items
    pub fn line_count(&self) -> usize {
        self.items.len()
    }

    /// Returns the number of payments
    pub fn payment_count(&self) -> usize {
        self.payments.len()
    }

    /// Marks the transaction as completed
    pub fn complete(&mut self) -> Result<(), &'static str> {
        if !self.is_fully_paid() {
            return Err("Transaction not fully paid");
        }
        if self.status != TransactionStatus::Draft {
            return Err("Transaction already completed or voided");
        }
        self.status = TransactionStatus::Completed;
        self.completed_at = Some(Utc::now());
        Ok(())
    }

    /// Voids the transaction
    pub fn void(&mut self, reason: &str) -> Result<(), &'static str> {
        if self.status == TransactionStatus::Voided {
            return Err("Transaction already voided");
        }
        self.status = TransactionStatus::Voided;
        self.voided_at = Some(Utc::now());
        self.void_reason = Some(reason.to_string());
        Ok(())
    }

    /// Returns true if the transaction can be voided
    pub fn can_void(&self) -> bool {
        matches!(
            self.status,
            TransactionStatus::Draft | TransactionStatus::Completed
        )
    }

    /// Returns true if the transaction is a sale
    pub fn is_sale(&self) -> bool {
        self.transaction_type.eq_ignore_ascii_case("sale")
    }

    /// Returns true if the transaction is a return
    pub fn is_return(&self) -> bool {
        self.transaction_type.eq_ignore_ascii_case("return")
    }

    /// Returns true if the transaction has a customer
    pub fn has_customer(&self) -> bool {
        self.customer_id.is_some()
    }

    /// Sets the receipt number
    pub fn set_receipt_number(&mut self, number: &str) {
        self.receipt_number = Some(number.to_string());
    }
}

/// Generates a unique transaction number
fn generate_transaction_number() -> String {
    let now = Utc::now();
    // Use microseconds and UUID for uniqueness (no rand dependency)
    let uuid = Uuid::new_v4();
    let uuid_bytes = uuid.as_bytes();
    let random_suffix: u16 = ((uuid_bytes[0] as u16) << 8 | uuid_bytes[1] as u16) % 10000;
    format!(
        "TXN{}{:02}{:02}{:02}{:02}{:04}",
        now.format("%y"),
        now.format("%m"),
        now.format("%d"),
        now.format("%H"),
        now.format("%M"),
        random_suffix
    )
}

/// Generates a receipt number for a completed transaction
pub fn generate_receipt_number(terminal_id: &str, sequence: u32) -> String {
    let now = Utc::now();
    format!(
        "{}-{}{:02}{:02}-{:06}",
        terminal_id,
        now.format("%y"),
        now.format("%m"),
        now.format("%d"),
        sequence
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn operator() -> OperatorId {
        OperatorId::new("op-1").expect("a non-blank id")
    }

    fn recorded() -> RecordedOperatorName {
        RecordedOperatorName::new("Ahmed").expect("a non-blank name")
    }
    use crate::product::{Product, ProductUnit};
    use rust_decimal::Decimal;

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

    fn create_test_cart() -> Cart {
        let product = create_test_product();
        let mut cart = Cart::new();
        cart.items.push(CartItem::new(&product, Decimal::from(2)));
        cart.recalculate();
        cart
    }

    #[test]
    fn test_transaction_status_conversion() {
        assert_eq!(TransactionStatus::Draft.as_str(), "DRAFT");
        assert_eq!(TransactionStatus::Completed.as_str(), "COMPLETED");
        assert_eq!(TransactionStatus::Voided.as_str(), "VOIDED");
        assert_eq!(TransactionStatus::Returned.as_str(), "RETURNED");

        assert_eq!(
            TransactionStatus::from_str("DRAFT"),
            Ok(TransactionStatus::Draft)
        );
        assert_eq!(
            TransactionStatus::from_str("completed"),
            Ok(TransactionStatus::Completed)
        );
    }

    #[test]
    fn transaction_status_from_str_rejects_unknown_and_names_it() {
        assert_eq!(
            TransactionStatus::from_str("invalid"),
            Err(ParseError::TransactionStatus("invalid".to_string()))
        );
        // The rejected value must survive into the message — an error that does not name the
        // offending row is not diagnosable.
        let message = TransactionStatus::from_str("invalid")
            .unwrap_err()
            .to_string();
        assert!(message.contains("invalid"), "{message}");
    }

    #[test]
    fn test_payment_method_conversion() {
        assert_eq!(PaymentMethod::Cash.as_str(), "CASH");
        assert_eq!(PaymentMethod::Card.as_str(), "CARD");
        assert_eq!(PaymentMethod::Wallet.as_str(), "WALLET");
        assert_eq!(PaymentMethod::Credit.as_str(), "CREDIT");

        assert_eq!(PaymentMethod::from_str("CASH"), Ok(PaymentMethod::Cash));
        assert_eq!(PaymentMethod::from_str("card"), Ok(PaymentMethod::Card));
    }

    #[test]
    fn payment_method_from_str_rejects_unknown_and_names_it() {
        assert_eq!(
            PaymentMethod::from_str("invalid"),
            Err(ParseError::PaymentMethod("invalid".to_string()))
        );
        let message = PaymentMethod::from_str("invalid").unwrap_err().to_string();
        assert!(message.contains("invalid"), "{message}");
    }

    #[test]
    fn transaction_status_and_payment_method_defaults_are_unchanged() {
        // `#[derive(Default)]` replaced hand-written impls; pin the variants it selects.
        assert_eq!(TransactionStatus::default(), TransactionStatus::Draft);
        assert_eq!(PaymentMethod::default(), PaymentMethod::Cash);
    }

    #[test]
    fn test_payment_method_display_name() {
        assert_eq!(PaymentMethod::Cash.display_name("en"), "Cash");
        assert_eq!(PaymentMethod::Cash.display_name("ar"), "نقدي");
        assert_eq!(PaymentMethod::Card.display_name("en"), "Card");
        assert_eq!(PaymentMethod::Card.display_name("ar"), "بطاقة");
    }

    #[test]
    fn test_payment_cash() {
        let payment = Payment::cash(Decimal::from(100), "LYD");
        assert_eq!(payment.method, PaymentMethod::Cash);
        assert_eq!(payment.amount, Decimal::from(100));
        assert_eq!(payment.currency, "LYD");
        assert!(payment.card_last_four.is_none());
    }

    #[test]
    fn test_payment_card() {
        let payment = Payment::card(Decimal::from(100), "LYD", "1234", "VISA", "AUTH123");
        assert_eq!(payment.method, PaymentMethod::Card);
        assert_eq!(payment.amount, Decimal::from(100));
        assert_eq!(payment.card_last_four, Some("1234".to_string()));
        assert_eq!(payment.card_type, Some("VISA".to_string()));
        assert_eq!(payment.auth_code, Some("AUTH123".to_string()));
    }

    #[test]
    fn test_payment_wallet() {
        let payment = Payment::wallet(Decimal::from(50), "LYD", "Sadad", "TXN123");
        assert_eq!(payment.method, PaymentMethod::Wallet);
        assert_eq!(payment.wallet_type, Some("Sadad".to_string()));
        assert_eq!(payment.reference, Some("TXN123".to_string()));
    }

    #[test]
    fn test_transaction_item_from_cart_item() {
        let product = create_test_product();
        let cart_item = CartItem::new(&product, Decimal::from(2));
        let txn_item = TransactionItem::from_cart_item(&cart_item);

        assert_eq!(txn_item.product_id, "prod-001");
        assert_eq!(txn_item.quantity, Decimal::from(2));
        assert_eq!(txn_item.unit_price, Decimal::from(10));
        assert_eq!(txn_item.line_total, Decimal::from(23));
    }

    #[test]
    fn test_transaction_item_display_name() {
        let product = create_test_product();
        let cart_item = CartItem::new(&product, Decimal::ONE);
        let txn_item = TransactionItem::from_cart_item(&cart_item);

        assert_eq!(txn_item.display_name("en"), "Test Product");
        assert_eq!(txn_item.display_name("ar"), "منتج تجريبي");
    }

    #[test]
    fn test_transaction_new() {
        let txn = Transaction::new("sale", "LYD", "shift-1", "TERM-001", operator(), recorded());

        assert_eq!(txn.transaction_type, "sale");
        assert_eq!(txn.currency, "LYD");
        assert_eq!(txn.shift_id, "shift-1");
        assert_eq!(txn.terminal_id, "TERM-001");
        assert_eq!(txn.operator_id, operator());
        assert_eq!(txn.operator_name, Some(recorded()));
        assert_eq!(txn.status, TransactionStatus::Draft);
        assert!(txn.transaction_number.starts_with("TXN"));
    }

    #[test]
    fn test_transaction_from_cart() {
        let cart = create_test_cart();
        let txn =
            Transaction::from_cart(&cart, "LYD", "shift-1", "TERM-001", operator(), recorded());

        assert_eq!(txn.items.len(), 1);
        assert_eq!(txn.subtotal, Decimal::from(20));
        assert_eq!(txn.tax_total, Decimal::from(3));
        assert_eq!(txn.grand_total, Decimal::from(23));
    }

    #[test]
    fn test_transaction_add_cash_payment() {
        let cart = create_test_cart();
        let mut txn =
            Transaction::from_cart(&cart, "LYD", "shift-1", "TERM-001", operator(), recorded());

        txn.add_cash_payment(Decimal::from(30));

        assert_eq!(txn.payments.len(), 1);
        assert_eq!(txn.total_paid(), Decimal::from(30));
        assert!(txn.is_fully_paid());
        assert_eq!(txn.change_due(), Decimal::from(7));
    }

    #[test]
    fn test_transaction_add_card_payment() {
        let cart = create_test_cart();
        let mut txn =
            Transaction::from_cart(&cart, "LYD", "shift-1", "TERM-001", operator(), recorded());

        txn.add_card_payment(Decimal::from(23), "1234", "VISA", "AUTH123");

        assert_eq!(txn.payments.len(), 1);
        assert!(txn.is_fully_paid());
        assert_eq!(txn.payments[0].card_last_four, Some("1234".to_string()));
    }

    #[test]
    fn test_transaction_split_payment() {
        let cart = create_test_cart();
        let mut txn =
            Transaction::from_cart(&cart, "LYD", "shift-1", "TERM-001", operator(), recorded());

        // Pay partially with cash, then card
        txn.add_cash_payment(Decimal::from(10));
        assert!(!txn.is_fully_paid());
        assert_eq!(txn.remaining(), Decimal::from(13));

        txn.add_card_payment(Decimal::from(13), "5678", "MASTERCARD", "AUTH456");
        assert!(txn.is_fully_paid());
        assert_eq!(txn.remaining(), Decimal::ZERO);
    }

    #[test]
    fn test_transaction_complete() {
        let cart = create_test_cart();
        let mut txn =
            Transaction::from_cart(&cart, "LYD", "shift-1", "TERM-001", operator(), recorded());

        // Cannot complete without payment
        assert!(txn.complete().is_err());

        // Add payment and complete
        txn.add_cash_payment(Decimal::from(25));
        assert!(txn.complete().is_ok());
        assert_eq!(txn.status, TransactionStatus::Completed);
        assert!(txn.completed_at.is_some());

        // Cannot complete again
        assert!(txn.complete().is_err());
    }

    #[test]
    fn test_transaction_void() {
        let cart = create_test_cart();
        let mut txn =
            Transaction::from_cart(&cart, "LYD", "shift-1", "TERM-001", operator(), recorded());

        assert!(txn.can_void());
        assert!(txn.void("Customer changed mind").is_ok());
        assert_eq!(txn.status, TransactionStatus::Voided);
        assert_eq!(txn.void_reason, Some("Customer changed mind".to_string()));
        assert!(txn.voided_at.is_some());

        // Cannot void again
        assert!(txn.void("Duplicate void").is_err());
        assert!(!txn.can_void());
    }

    #[test]
    fn test_transaction_item_count() {
        let cart = create_test_cart();
        let txn =
            Transaction::from_cart(&cart, "LYD", "shift-1", "TERM-001", operator(), recorded());

        assert_eq!(txn.item_count(), Decimal::from(2));
        assert_eq!(txn.line_count(), 1);
    }

    #[test]
    fn test_transaction_is_sale_return() {
        let sale = Transaction::new("sale", "LYD", "s1", "t1", operator(), recorded());
        let ret = Transaction::new("return", "LYD", "s1", "t1", operator(), recorded());

        assert!(sale.is_sale());
        assert!(!sale.is_return());
        assert!(!ret.is_sale());
        assert!(ret.is_return());
    }

    #[test]
    fn test_transaction_has_customer() {
        let mut txn = Transaction::new("sale", "LYD", "s1", "t1", operator(), recorded());
        assert!(!txn.has_customer());

        txn.customer_id = Some("cust-001".to_string());
        assert!(txn.has_customer());
    }

    #[test]
    fn test_transaction_number_generation() {
        let num1 = generate_transaction_number();
        let num2 = generate_transaction_number();

        assert!(num1.starts_with("TXN"));
        assert!(num2.starts_with("TXN"));
        // Format: TXN(3) + YY(2) + MM(2) + DD(2) + HH(2) + MM(2) + random(4) = 17
        assert_eq!(num1.len(), 17);
    }

    #[test]
    fn test_receipt_number_generation() {
        let receipt = generate_receipt_number("TERM-001", 42);
        assert!(receipt.starts_with("TERM-001-"));
        assert!(receipt.ends_with("-000042"));
    }

    #[test]
    fn test_set_receipt_number() {
        let mut txn = Transaction::new("sale", "LYD", "s1", "t1", operator(), recorded());
        assert!(txn.receipt_number.is_none());

        txn.set_receipt_number("RCP-001");
        assert_eq!(txn.receipt_number, Some("RCP-001".to_string()));
    }
}
