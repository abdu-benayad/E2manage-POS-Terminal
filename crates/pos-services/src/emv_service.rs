//! EMV Service - Card Payment Terminal Integration (Phase 22)
//!
//! This module provides integration with EMV card payment terminals.
//! Currently implements a stub/simulation for development purposes.
//!
//! In production, this would integrate with:
//! - Physical EMV terminals (PAX, Ingenico, Verifone)
//! - Payment processors (via SDK or serial communication)
//! - NFC/contactless payment support
//!
//! ## Usage
//!
//! ```rust,ignore
//! use crate::services::emv_service::{EmvService, EmvEvent};
//! use tokio::sync::broadcast;
//!
//! let emv = EmvService::new();
//! let (tx, mut rx) = broadcast::channel::<EmvEvent>(16);
//!
//! // Start payment
//! tokio::spawn(async move {
//!     let result = emv.start_payment(25.500, "LYD", tx).await;
//!     match result {
//!         Ok(payment) if payment.success => {
//!             println!("Payment approved: {}", payment.auth_code.unwrap_or_default());
//!         }
//!         Ok(payment) => {
//!             println!("Payment declined: {}", payment.error_message.unwrap_or_default());
//!         }
//!         Err(e) => {
//!             println!("Payment error: {}", e);
//!         }
//!     }
//! });
//!
//! // Listen for events
//! while let Ok(event) = rx.recv().await {
//!     match event {
//!         EmvEvent::WaitingForCard => println!("Insert card..."),
//!         EmvEvent::Processing => println!("Processing..."),
//!         EmvEvent::Approved(result) => println!("Approved!"),
//!         _ => {}
//!     }
//! }
//! ```

use anyhow::Result;
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;

/// Result of a card payment transaction
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CardPaymentResult {
    /// Whether the payment was successful
    pub success: bool,
    /// Card type (visa, mastercard, amex, etc.)
    pub card_type: Option<String>,
    /// Last 4 digits of card number
    pub last_four: Option<String>,
    /// Authorization code from payment processor
    pub auth_code: Option<String>,
    /// Transaction reference/ID
    pub transaction_id: Option<String>,
    /// Error message if payment failed
    pub error_message: Option<String>,
    /// Response code from EMV terminal
    pub response_code: Option<String>,
    /// Masked PAN (for receipt printing)
    pub masked_pan: Option<String>,
    /// Cardholder name (if available)
    pub cardholder_name: Option<String>,
    /// Application label (EMV app name)
    pub application_label: Option<String>,
    /// EMV cryptogram
    pub cryptogram: Option<String>,
    /// Transaction timestamp
    pub timestamp: Option<String>,
}

impl Default for CardPaymentResult {
    fn default() -> Self {
        Self {
            success: false,
            card_type: None,
            last_four: None,
            auth_code: None,
            transaction_id: None,
            error_message: None,
            response_code: None,
            masked_pan: None,
            cardholder_name: None,
            application_label: None,
            cryptogram: None,
            timestamp: None,
        }
    }
}

impl CardPaymentResult {
    /// Create a new successful payment result
    pub fn approved(
        card_type: impl Into<String>,
        last_four: impl Into<String>,
        auth_code: impl Into<String>,
        transaction_id: impl Into<String>,
    ) -> Self {
        Self {
            success: true,
            card_type: Some(card_type.into()),
            last_four: Some(last_four.into()),
            auth_code: Some(auth_code.into()),
            transaction_id: Some(transaction_id.into()),
            response_code: Some("00".to_string()), // Standard approval code
            timestamp: Some(chrono::Utc::now().to_rfc3339()),
            ..Default::default()
        }
    }

    /// Create a declined payment result
    pub fn declined(error_message: impl Into<String>, response_code: impl Into<String>) -> Self {
        Self {
            success: false,
            error_message: Some(error_message.into()),
            response_code: Some(response_code.into()),
            timestamp: Some(chrono::Utc::now().to_rfc3339()),
            ..Default::default()
        }
    }

    /// Create an error payment result
    pub fn error(error_message: impl Into<String>) -> Self {
        Self {
            success: false,
            error_message: Some(error_message.into()),
            timestamp: Some(chrono::Utc::now().to_rfc3339()),
            ..Default::default()
        }
    }
}

/// Events emitted during EMV transaction processing
#[derive(Clone, Debug)]
pub enum EmvEvent {
    /// Waiting for card insertion or tap
    WaitingForCard,
    /// Card has been detected/inserted
    CardInserted,
    /// Card data is being read
    ReadingCard,
    /// Transaction is being processed with payment processor
    Processing,
    /// PIN entry is required on the terminal
    PinRequired,
    /// PIN has been entered (don't include actual PIN!)
    PinEntered,
    /// Transaction approved
    Approved(CardPaymentResult),
    /// Transaction declined by issuer
    Declined(String),
    /// Error during transaction
    Error(String),
    /// Transaction cancelled by user or timeout
    Cancelled,
    /// Card removed too early
    CardRemovedEarly,
    /// Contactless transaction detected
    ContactlessDetected,
    /// Signature required (for non-PIN transactions)
    SignatureRequired,
    /// Remove card prompt
    RemoveCard,
}

/// EMV terminal connection status
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TerminalStatus {
    /// Terminal is disconnected
    Disconnected,
    /// Terminal is connecting
    Connecting,
    /// Terminal is connected and ready
    Ready,
    /// Terminal is busy with a transaction
    Busy,
    /// Terminal has an error
    Error(String),
}

/// EMV Service for card payment processing
///
/// This is a stub implementation for development.
/// In production, replace with actual EMV SDK integration.
pub struct EmvService {
    /// Terminal connection status
    status: TerminalStatus,
    /// Simulation mode flag (for development)
    simulation_mode: bool,
    /// Simulated delay in milliseconds
    simulation_delay_ms: u64,
}

impl EmvService {
    /// Create a new EMV service instance
    pub fn new() -> Self {
        Self {
            status: TerminalStatus::Ready,
            simulation_mode: true,
            simulation_delay_ms: 1500,
        }
    }

    /// Create EMV service with custom simulation settings
    pub fn with_simulation(delay_ms: u64) -> Self {
        Self {
            status: TerminalStatus::Ready,
            simulation_mode: true,
            simulation_delay_ms: delay_ms,
        }
    }

    /// Check if terminal is ready for transactions
    pub fn is_ready(&self) -> bool {
        self.status == TerminalStatus::Ready
    }

    /// Get current terminal status
    pub fn get_status(&self) -> &TerminalStatus {
        &self.status
    }

    /// Start a card payment transaction
    ///
    /// This method initiates a card payment and sends progress events
    /// through the broadcast channel.
    ///
    /// # Arguments
    ///
    /// * `amount` - The payment amount
    /// * `currency` - The currency code (e.g., "LYD")
    /// * `tx` - Broadcast sender for EMV events
    ///
    /// # Returns
    ///
    /// Returns `CardPaymentResult` with transaction details
    pub async fn start_payment(
        &self,
        amount: f64,
        currency: &str,
        tx: broadcast::Sender<EmvEvent>,
    ) -> Result<CardPaymentResult> {
        // Validate amount
        if amount <= 0.0 {
            let _ = tx.send(EmvEvent::Error("Invalid amount".to_string()));
            return Ok(CardPaymentResult::error("Invalid payment amount"));
        }

        // In simulation mode, run the simulated payment flow
        if self.simulation_mode {
            return self.simulate_payment(amount, currency, tx).await;
        }

        // Production implementation would go here:
        // 1. Connect to EMV terminal
        // 2. Send transaction request
        // 3. Handle card insertion/tap
        // 4. Process PIN if required
        // 5. Communicate with payment processor
        // 6. Return result

        // For now, return error indicating no real terminal configured
        let _ = tx.send(EmvEvent::Error("EMV terminal not configured".to_string()));
        Ok(CardPaymentResult::error("EMV terminal not configured"))
    }

    /// Simulate a card payment for development/testing
    async fn simulate_payment(
        &self,
        _amount: f64,
        _currency: &str,
        tx: broadcast::Sender<EmvEvent>,
    ) -> Result<CardPaymentResult> {
        let delay = std::time::Duration::from_millis(self.simulation_delay_ms);

        // Step 1: Waiting for card
        let _ = tx.send(EmvEvent::WaitingForCard);
        tokio::time::sleep(delay).await;

        // Step 2: Card inserted
        let _ = tx.send(EmvEvent::CardInserted);
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;

        // Step 3: Reading card
        let _ = tx.send(EmvEvent::ReadingCard);
        tokio::time::sleep(std::time::Duration::from_millis(800)).await;

        // Step 4: Processing
        let _ = tx.send(EmvEvent::Processing);
        tokio::time::sleep(delay).await;

        // Step 5: PIN required (for chip transactions)
        let _ = tx.send(EmvEvent::PinRequired);
        tokio::time::sleep(delay).await;

        // Step 6: PIN entered
        let _ = tx.send(EmvEvent::PinEntered);
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;

        // Step 7: Processing with bank
        let _ = tx.send(EmvEvent::Processing);
        tokio::time::sleep(delay).await;

        // Step 8: Approved (simulated success)
        // In a real implementation, this would come from the payment processor
        let result = CardPaymentResult {
            success: true,
            card_type: Some("visa".to_string()),
            last_four: Some("4242".to_string()),
            auth_code: Some(format!("{:06}", rand_auth_code())),
            transaction_id: Some(format!("TXN{}", generate_txn_id())),
            response_code: Some("00".to_string()),
            masked_pan: Some("************4242".to_string()),
            cardholder_name: None, // Often not available
            application_label: Some("VISA CREDIT".to_string()),
            cryptogram: None,
            timestamp: Some(chrono::Utc::now().to_rfc3339()),
            error_message: None,
        };

        let _ = tx.send(EmvEvent::Approved(result.clone()));

        // Step 9: Remove card prompt
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        let _ = tx.send(EmvEvent::RemoveCard);

        Ok(result)
    }

    /// Simulate a declined payment
    pub async fn simulate_declined(
        &self,
        tx: broadcast::Sender<EmvEvent>,
        reason: &str,
    ) -> Result<CardPaymentResult> {
        let delay = std::time::Duration::from_millis(self.simulation_delay_ms);

        let _ = tx.send(EmvEvent::WaitingForCard);
        tokio::time::sleep(delay).await;

        let _ = tx.send(EmvEvent::CardInserted);
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;

        let _ = tx.send(EmvEvent::Processing);
        tokio::time::sleep(delay).await;

        let result = CardPaymentResult::declined(reason, "05"); // 05 = Do not honor

        let _ = tx.send(EmvEvent::Declined(reason.to_string()));

        Ok(result)
    }

    /// Simulate a payment error
    pub async fn simulate_error(
        &self,
        tx: broadcast::Sender<EmvEvent>,
        error: &str,
    ) -> Result<CardPaymentResult> {
        let delay = std::time::Duration::from_millis(self.simulation_delay_ms);

        let _ = tx.send(EmvEvent::WaitingForCard);
        tokio::time::sleep(delay).await;

        let _ = tx.send(EmvEvent::Error(error.to_string()));

        Ok(CardPaymentResult::error(error))
    }

    /// Cancel the current transaction
    pub async fn cancel(&self, tx: broadcast::Sender<EmvEvent>) -> Result<()> {
        let _ = tx.send(EmvEvent::Cancelled);
        Ok(())
    }

    /// Check terminal connection
    pub async fn check_connection(&mut self) -> Result<bool> {
        // In simulation mode, always connected
        if self.simulation_mode {
            self.status = TerminalStatus::Ready;
            return Ok(true);
        }

        // Production: Check actual terminal connection
        // self.status = ... based on terminal response
        Ok(self.status == TerminalStatus::Ready)
    }
}

impl Default for EmvService {
    fn default() -> Self {
        Self::new()
    }
}

/// Generate a random authorization code (6 digits)
fn rand_auth_code() -> u32 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u32;
    (seed % 900000) + 100000 // 100000-999999
}

/// Generate a transaction ID
fn generate_txn_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis();
    format!("{}", timestamp % 10_000_000_000)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_card_payment_result_approved() {
        let result = CardPaymentResult::approved("visa", "4242", "ABC123", "TXN001");
        assert!(result.success);
        assert_eq!(result.card_type, Some("visa".to_string()));
        assert_eq!(result.last_four, Some("4242".to_string()));
        assert_eq!(result.auth_code, Some("ABC123".to_string()));
        assert_eq!(result.response_code, Some("00".to_string()));
    }

    #[test]
    fn test_card_payment_result_declined() {
        let result = CardPaymentResult::declined("Insufficient funds", "51");
        assert!(!result.success);
        assert_eq!(result.error_message, Some("Insufficient funds".to_string()));
        assert_eq!(result.response_code, Some("51".to_string()));
    }

    #[test]
    fn test_card_payment_result_error() {
        let result = CardPaymentResult::error("Connection timeout");
        assert!(!result.success);
        assert_eq!(result.error_message, Some("Connection timeout".to_string()));
    }

    #[test]
    fn test_emv_service_creation() {
        let service = EmvService::new();
        assert!(service.is_ready());
        assert!(service.simulation_mode);
    }

    #[test]
    fn test_emv_service_with_simulation() {
        let service = EmvService::with_simulation(500);
        assert!(service.is_ready());
        assert_eq!(service.simulation_delay_ms, 500);
    }

    #[test]
    fn test_terminal_status_ready() {
        let service = EmvService::new();
        assert_eq!(*service.get_status(), TerminalStatus::Ready);
    }

    #[tokio::test]
    async fn test_start_payment_invalid_amount() {
        let service = EmvService::new();
        let (tx, _rx) = broadcast::channel::<EmvEvent>(16);

        let result = service.start_payment(-10.0, "LYD", tx).await.unwrap();
        assert!(!result.success);
        assert!(result.error_message.unwrap().contains("Invalid"));
    }

    #[tokio::test]
    async fn test_simulate_payment_success() {
        let service = EmvService::with_simulation(100); // Fast simulation
        let (tx, mut rx) = broadcast::channel::<EmvEvent>(16);

        let handle = tokio::spawn(async move {
            service.start_payment(25.500, "LYD", tx).await
        });

        // Collect some events
        let mut events = Vec::new();
        while let Ok(event) = rx.recv().await {
            events.push(event.clone());
            if matches!(event, EmvEvent::RemoveCard) {
                break;
            }
        }

        let result = handle.await.unwrap().unwrap();
        assert!(result.success);
        assert_eq!(result.card_type, Some("visa".to_string()));
        assert!(!events.is_empty());
    }

    #[tokio::test]
    async fn test_cancel_payment() {
        let service = EmvService::new();
        let (tx, mut rx) = broadcast::channel::<EmvEvent>(16);

        service.cancel(tx).await.unwrap();

        if let Ok(event) = rx.recv().await {
            assert!(matches!(event, EmvEvent::Cancelled));
        }
    }

    #[tokio::test]
    async fn test_check_connection() {
        let mut service = EmvService::new();
        let connected = service.check_connection().await.unwrap();
        assert!(connected);
        assert_eq!(*service.get_status(), TerminalStatus::Ready);
    }

    #[test]
    fn test_rand_auth_code() {
        let code = rand_auth_code();
        assert!(code >= 100000);
        assert!(code <= 999999);
    }

    #[test]
    fn test_generate_txn_id() {
        let id = generate_txn_id();
        assert!(!id.is_empty());
        assert!(id.chars().all(|c| c.is_ascii_digit()));
    }
}
