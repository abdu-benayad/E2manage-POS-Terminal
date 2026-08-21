//! QR Payment Service
//!
//! Handles QR code generation and mobile wallet payment verification.
//!
//! ## Supported Wallets
//!
//! - Sadad - Libyan mobile payment system
//! - MobiCash - Mobile cash payment network
//!
//! ## Usage
//!
//! ```rust,ignore
//! use crate::services::{QrService, QrEvent};
//!
//! let qr_service = QrService::new();
//! let (tx, mut rx) = tokio::sync::broadcast::channel(16);
//!
//! // Start QR payment
//! let result = qr_service.start_payment(25.500, "LYD", "INV-001", tx).await?;
//!
//! // Listen for events
//! while let Ok(event) = rx.recv().await {
//!     match event {
//!         QrEvent::QrGenerated { payment_url, expires_at } => {
//!             // Display QR code
//!         }
//!         QrEvent::PaymentReceived { wallet, transaction_id } => {
//!             // Payment successful
//!         }
//!         QrEvent::Expired => {
//!             // QR code expired
//!         }
//!         _ => {}
//!     }
//! }
//! ```

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use thiserror::Error;

/// QR payment events broadcast to UI
#[derive(Debug, Clone)]
pub enum QrEvent {
    /// QR code generated and ready for display
    QrGenerated {
        /// Payment URL encoded in QR
        payment_url: String,
        /// Expiration timestamp (seconds from now)
        expires_in: u32,
    },
    /// User scanned QR code
    Scanned {
        /// Wallet that scanned
        wallet_type: String,
    },
    /// Verifying payment with wallet provider
    Verifying,
    /// Payment received and confirmed
    PaymentReceived {
        /// Wallet provider name
        wallet_type: String,
        /// Transaction ID from wallet provider
        transaction_id: String,
    },
    /// QR code expired
    Expired,
    /// Payment failed
    Failed {
        /// Error message
        error: String,
    },
    /// Payment cancelled by user
    Cancelled,
    /// Timer tick (seconds remaining)
    TimerTick {
        /// Seconds remaining
        remaining: u32,
    },
}

/// QR payment result
#[derive(Debug, Clone)]
pub struct QrPaymentResult {
    /// Whether payment was successful
    pub success: bool,
    /// Wallet provider name
    pub wallet_type: String,
    /// Transaction ID from wallet provider
    pub transaction_id: String,
    /// Error message if failed
    pub error_message: String,
}

impl Default for QrPaymentResult {
    fn default() -> Self {
        Self {
            success: false,
            wallet_type: String::new(),
            transaction_id: String::new(),
            error_message: String::new(),
        }
    }
}

/// QR service errors
#[derive(Debug, Error)]
pub enum QrError {
    #[error("QR generation failed: {0}")]
    GenerationFailed(String),

    #[error("Payment timeout")]
    Timeout,

    #[error("Payment cancelled")]
    Cancelled,

    #[error("Wallet verification failed: {0}")]
    VerificationFailed(String),

    #[error("Network error: {0}")]
    NetworkError(String),
}

/// QR code payment service
///
/// Generates QR codes for mobile wallet payments and verifies payment completion
/// through wallet provider APIs.
pub struct QrService {
    /// Whether simulation mode is enabled (for development/testing)
    simulation_mode: bool,
    /// Whether a payment is currently in progress
    payment_in_progress: Arc<AtomicBool>,
    /// Merchant ID for payment requests
    merchant_id: String,
    /// Default timeout for QR codes (seconds)
    default_timeout: u32,
}

impl Default for QrService {
    fn default() -> Self {
        Self::new()
    }
}

impl QrService {
    /// Create a new QR service instance
    pub fn new() -> Self {
        Self {
            simulation_mode: true, // Default to simulation for development
            payment_in_progress: Arc::new(AtomicBool::new(false)),
            merchant_id: "E2MANAGE-POS-001".to_string(),
            default_timeout: 300, // 5 minutes
        }
    }

    /// Create QR service with custom merchant ID
    pub fn with_merchant_id(mut self, merchant_id: &str) -> Self {
        self.merchant_id = merchant_id.to_string();
        self
    }

    /// Set simulation mode (for development/testing)
    pub fn set_simulation_mode(&mut self, enabled: bool) {
        self.simulation_mode = enabled;
    }

    /// Check if simulation mode is enabled
    pub fn is_simulation_mode(&self) -> bool {
        self.simulation_mode
    }

    /// Set the default QR code timeout (seconds)
    pub fn set_default_timeout(&mut self, timeout: u32) {
        self.default_timeout = timeout;
    }

    /// Check if a payment is currently in progress
    pub fn is_payment_in_progress(&self) -> bool {
        self.payment_in_progress.load(Ordering::SeqCst)
    }

    /// Generate a payment URL for QR code
    pub fn generate_payment_url(&self, amount: f64, currency: &str, reference: &str) -> String {
        // Create payment data URL
        // In production, this would use actual wallet provider URL schemes
        format!(
            "pay://wallet.e2manage.com?amount={:.3}&currency={}&ref={}&merchant={}",
            amount, currency, reference, self.merchant_id
        )
    }

    /// Generate QR code data (placeholder - returns URL for now)
    ///
    /// In production, this would generate actual QR code image data
    /// using a library like `qrcode-rs`.
    pub fn generate_qr_data(
        &self,
        amount: f64,
        currency: &str,
        reference: &str,
    ) -> Result<Vec<u8>, QrError> {
        let payment_url = self.generate_payment_url(amount, currency, reference);

        // In production, this would generate actual QR code image:
        // use qrcode::{QrCode, render::svg};
        // let code = QrCode::new(payment_url.as_bytes())?;
        // let svg = code.render::<svg::Color>().build();
        // Ok(svg.into_bytes())

        // For now, return URL as bytes (placeholder)
        Ok(payment_url.into_bytes())
    }

    /// Start a QR payment flow
    ///
    /// This initiates a QR payment by:
    /// 1. Generating a QR code with payment URL
    /// 2. Broadcasting QR generated event
    /// 3. Starting countdown timer
    /// 4. Polling for payment confirmation (in production)
    /// 5. Broadcasting result events
    pub async fn start_payment(
        &self,
        amount: f64,
        currency: &str,
        reference: &str,
        event_tx: tokio::sync::broadcast::Sender<QrEvent>,
    ) -> Result<QrPaymentResult, QrError> {
        // Check if already processing
        if self.payment_in_progress.load(Ordering::SeqCst) {
            return Err(QrError::GenerationFailed(
                "Payment already in progress".to_string(),
            ));
        }

        self.payment_in_progress.store(true, Ordering::SeqCst);
        let payment_in_progress = self.payment_in_progress.clone();

        // Generate payment URL
        let payment_url = self.generate_payment_url(amount, currency, reference);

        // Send QR generated event
        let _ = event_tx.send(QrEvent::QrGenerated {
            payment_url: payment_url.clone(),
            expires_in: self.default_timeout,
        });

        if self.simulation_mode {
            // Simulation mode: simulate payment flow
            let result = self
                .simulate_payment_flow(event_tx.clone(), self.default_timeout)
                .await;
            payment_in_progress.store(false, Ordering::SeqCst);
            result
        } else {
            // Production mode: poll wallet APIs for payment confirmation
            // This would integrate with actual wallet provider APIs
            let result = self
                .poll_for_payment(event_tx.clone(), reference, self.default_timeout)
                .await;
            payment_in_progress.store(false, Ordering::SeqCst);
            result
        }
    }

    /// Cancel an ongoing payment
    pub fn cancel_payment(&self, event_tx: &tokio::sync::broadcast::Sender<QrEvent>) {
        if self.payment_in_progress.load(Ordering::SeqCst) {
            self.payment_in_progress.store(false, Ordering::SeqCst);
            let _ = event_tx.send(QrEvent::Cancelled);
        }
    }

    /// Simulate payment flow for development/testing
    async fn simulate_payment_flow(
        &self,
        event_tx: tokio::sync::broadcast::Sender<QrEvent>,
        timeout_seconds: u32,
    ) -> Result<QrPaymentResult, QrError> {
        use tokio::time::{sleep, Duration};

        // Simulate timer countdown
        for remaining in (1..=timeout_seconds).rev() {
            if !self.payment_in_progress.load(Ordering::SeqCst) {
                return Err(QrError::Cancelled);
            }

            // Send timer tick every second
            let _ = event_tx.send(QrEvent::TimerTick { remaining });

            // Simulate scan after 5 seconds
            if remaining == timeout_seconds - 5 {
                let _ = event_tx.send(QrEvent::Scanned {
                    wallet_type: "Sadad".to_string(),
                });
            }

            // Simulate verification after 8 seconds
            if remaining == timeout_seconds - 8 {
                let _ = event_tx.send(QrEvent::Verifying);
            }

            // Simulate successful payment after 12 seconds
            if remaining == timeout_seconds - 12 {
                let transaction_id = format!(
                    "TXN-{}",
                    uuid::Uuid::new_v4().to_string()[..8].to_uppercase()
                );
                let _ = event_tx.send(QrEvent::PaymentReceived {
                    wallet_type: "Sadad".to_string(),
                    transaction_id: transaction_id.clone(),
                });

                return Ok(QrPaymentResult {
                    success: true,
                    wallet_type: "Sadad".to_string(),
                    transaction_id,
                    error_message: String::new(),
                });
            }

            sleep(Duration::from_secs(1)).await;
        }

        // Timeout reached
        let _ = event_tx.send(QrEvent::Expired);
        Err(QrError::Timeout)
    }

    /// Poll wallet APIs for payment confirmation (production mode)
    async fn poll_for_payment(
        &self,
        event_tx: tokio::sync::broadcast::Sender<QrEvent>,
        _reference: &str,
        timeout_seconds: u32,
    ) -> Result<QrPaymentResult, QrError> {
        use tokio::time::{sleep, Duration};

        // In production, this would:
        // 1. Register payment request with wallet provider
        // 2. Poll provider API for payment status
        // 3. Verify payment signature/callback

        let poll_interval = Duration::from_secs(2);
        let max_polls = timeout_seconds / 2;

        for remaining in (1..=max_polls).rev() {
            if !self.payment_in_progress.load(Ordering::SeqCst) {
                return Err(QrError::Cancelled);
            }

            // Send timer tick
            let _ = event_tx.send(QrEvent::TimerTick {
                remaining: remaining * 2,
            });

            // TODO: Poll wallet provider API here
            // let status = self.check_payment_status(reference).await?;
            // if status.paid {
            //     return Ok(QrPaymentResult { ... });
            // }

            sleep(poll_interval).await;
        }

        // Timeout reached
        let _ = event_tx.send(QrEvent::Expired);
        Err(QrError::Timeout)
    }

    /// Refresh QR code (generate new one after expiration)
    pub async fn refresh_qr(
        &self,
        amount: f64,
        currency: &str,
        reference: &str,
        event_tx: tokio::sync::broadcast::Sender<QrEvent>,
    ) -> Result<String, QrError> {
        // Generate new reference with timestamp
        let new_reference = format!("{}-{}", reference, chrono::Utc::now().timestamp());

        // Generate new payment URL
        let payment_url = self.generate_payment_url(amount, currency, &new_reference);

        // Send QR generated event
        let _ = event_tx.send(QrEvent::QrGenerated {
            payment_url: payment_url.clone(),
            expires_in: self.default_timeout,
        });

        Ok(payment_url)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_qr_service_creation() {
        let service = QrService::new();
        assert!(service.is_simulation_mode());
        assert!(!service.is_payment_in_progress());
    }

    #[test]
    fn test_payment_url_generation() {
        let service = QrService::new().with_merchant_id("TEST-MERCHANT");
        let url = service.generate_payment_url(25.500, "LYD", "INV-001");

        assert!(url.contains("amount=25.500"));
        assert!(url.contains("currency=LYD"));
        assert!(url.contains("ref=INV-001"));
        assert!(url.contains("merchant=TEST-MERCHANT"));
    }

    #[test]
    fn test_qr_data_generation() {
        let service = QrService::new();
        let result = service.generate_qr_data(10.0, "LYD", "TEST-001");

        assert!(result.is_ok());
        let data = result.unwrap();
        assert!(!data.is_empty());
    }

    #[test]
    fn test_simulation_mode_toggle() {
        let mut service = QrService::new();
        assert!(service.is_simulation_mode());

        service.set_simulation_mode(false);
        assert!(!service.is_simulation_mode());

        service.set_simulation_mode(true);
        assert!(service.is_simulation_mode());
    }

    #[test]
    fn test_timeout_configuration() {
        let mut service = QrService::new();
        service.set_default_timeout(600);
        // Default timeout is private, but we can verify it doesn't panic
    }

    #[tokio::test]
    async fn test_cancel_payment() {
        let service = QrService::new();
        let (tx, mut rx) = tokio::sync::broadcast::channel(16);

        // Cancel without active payment (should be no-op)
        service.cancel_payment(&tx);

        // Start and immediately cancel
        service.payment_in_progress.store(true, Ordering::SeqCst);
        service.cancel_payment(&tx);

        // Should receive cancelled event
        let event = rx.recv().await.unwrap();
        assert!(matches!(event, QrEvent::Cancelled));
        assert!(!service.is_payment_in_progress());
    }

    #[test]
    fn test_qr_payment_result_default() {
        let result = QrPaymentResult::default();
        assert!(!result.success);
        assert!(result.wallet_type.is_empty());
        assert!(result.transaction_id.is_empty());
        assert!(result.error_message.is_empty());
    }

    #[test]
    fn test_qr_error_display() {
        let error = QrError::Timeout;
        assert_eq!(format!("{}", error), "Payment timeout");

        let error = QrError::Cancelled;
        assert_eq!(format!("{}", error), "Payment cancelled");

        let error = QrError::GenerationFailed("test".to_string());
        assert_eq!(format!("{}", error), "QR generation failed: test");
    }
}
