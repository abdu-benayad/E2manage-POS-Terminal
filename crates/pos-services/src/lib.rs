//! Services Module - Business logic
//!
//! This module contains the business logic services for the POS terminal.
//!
//! ## Available Services
//!
//! - **auth_service**: Terminal and operator authentication, PIN verification
//! - **sync_service**: Background data synchronization with polling
//! - **product_service**: Product search and lookup with FTS5
//! - **cart_service**: Shopping cart management with discounts and customer support
//! - **emv_service**: EMV card payment terminal integration
//! - **qr_service**: QR code generation and mobile wallet payment verification
//! - **transaction_service**: Transaction processing, completion, and sync
//! - **escpos**: ESC/POS printer command builder
//! - **print_service**: Receipt and report printing
//! - **draft_service**: Draft/held order management (save, recall, list)
//! - **return_service**: Returns and refund processing
//! - **shift_service**: Shift management (open, close, cash count)
//! - **offline_service**: Offline transaction queue management and sync
//! - **z_report_service**: End of day Z-Report generation
//!
//! ## Usage
//!
//! ```rust,ignore
//! use crate::services::{AuthService, SyncService, ProductService, CartService, EmvService, QrService, TransactionService};
//! use crate::services::{PrintService, PrinterConfig, ShiftReport, EscPos, Alignment};
//! use std::sync::Arc;
//!
//! // Authentication
//! let auth = AuthService::new(Arc::new(api.clone()), Arc::new(db.clone()));
//! // Total: accepted, refused for a stated reason, or undecided for a stated reason. There is
//! // no `valid` boolean and no `?` — "the till could not find out" is a case, not an error.
//! match auth.verify_pin(&operator_id, &pin, &policy).await {
//!     PinVerification::Accepted { operator, decided_by } => {
//!         println!("Welcome, {} (decided by {decided_by:?})", operator.name().latin());
//!     }
//!     PinVerification::Refused(refusal) => println!("refused: {refusal:?}"),
//!     PinVerification::Undetermined(cause) => println!("could not decide: {cause}"),
//! }
//!
//! // Sync service
//! let sync = SyncService::new(Arc::new(api), Arc::new(db.clone()), 10);
//! let (tx, _rx) = tokio::sync::broadcast::channel(16);
//! sync.start_polling(tx).await;
//!
//! // Product service
//! let products = ProductService::new(Arc::new(db));
//! let results = products.search("milk", 20)?;
//! for product in results.products {
//!     println!("{}: ${:.2}", product.name, product.price);
//! }
//!
//! // Cart service
//! let cart = Arc::new(CartService::new());
//! cart.add_item(&product, 2.0)?;
//! cart.apply_cart_discount(10.0)?;
//! println!("Total: {:.2}", cart.grand_total());
//!
//! // EMV service for card payments
//! let emv = EmvService::new();
//! let (tx, rx) = tokio::sync::broadcast::channel(16);
//! let result = emv.start_payment(25.500, "LYD", tx).await?;
//!
//! // QR/Wallet payment service
//! let qr = QrService::new();
//! let (tx, rx) = tokio::sync::broadcast::channel(16);
//! let result = qr.start_payment(25.500, "LYD", "INV-001", tx).await?;
//!
//! // Transaction service
//! let txn_service = TransactionService::new(Arc::clone(&api), Arc::clone(&db));
//! let txn = txn_service.create_from_cart(&cart.get_cart(), "LYD", "shift-1", "TERM-001", "op-1", "Ahmed")?;
//! txn_service.add_cash_payment(50.0)?;
//! let result = txn_service.complete().await?;
//! println!("Receipt: {}", result.receipt_number);
//!
//! // Print service
//! let config = PrinterConfig {
//!     printer_path: Some("/dev/usb/lp0".to_string()),
//!     store_name: "My Store".to_string(),
//!     ..Default::default()
//! };
//! let printer = PrintService::new(config);
//! printer.print_receipt(&txn_service.current_transaction().unwrap(), "en")?;
//! printer.open_drawer()?;
//! ```

pub mod auth_service;
pub mod cart_service;
pub mod conflict_service;
pub mod draft_service;
pub mod emv_service;
pub mod feature_service;
pub mod log_service;
pub mod offline_service;
pub mod operator_sign_in;
pub mod pairing_service;
pub mod parse;
pub mod platform_heartbeat_service;
pub mod policy_service;
pub mod product_service;
pub mod qr_service;
pub mod return_service;
pub mod shared_draft_service;
pub mod shift_service;
pub mod support_service;
pub mod sync_service;
pub mod system_service;
pub mod transaction_service;
pub mod update_service;
pub mod z_report_service;

#[cfg(test)]
mod feature_service_tests;

// Re-export from other crates for convenience
pub use pos_escpos::{Alignment, BarcodeFormat, BarcodeTextPosition, CodePage, EscPos};
pub use pos_printing::{PrintService, PrinterConfig, ShiftReport};

// Re-export main types
pub use auth_service::{AuthService, TerminalSession};
pub use cart_service::{CartError, CartResult, CartService};
pub use conflict_service::{
    ConflictError, ConflictResult, ConflictService, ConflictSummary, ResolutionResult,
};
pub use draft_service::{
    Draft, DraftError, DraftResult, DraftService, DEFAULT_DRAFT_EXPIRY_HOURS,
    MAX_DRAFTS_PER_OPERATOR,
};
pub use emv_service::{CardPaymentResult, EmvEvent, EmvService, TerminalStatus};
pub use feature_service::{FeatureError, FeatureResult, FeatureService};
pub use log_service::{LogEntry, LogError, LogLevel, LogResult, LogService};
pub use offline_service::{
    OfflineService, QueueStats, SyncFailureType, SyncResult, BATCH_SIZE, MAX_RETRY_COUNT,
};
pub use operator_sign_in::{HeldOperatorSession, OperatorSignIn};
pub use pairing_service::{PairingService, PairingState, TerminalRegistration};
pub use parse::ParseError;
pub use platform_heartbeat_service::{
    HeartbeatError, HeartbeatEvent, PlatformHeartbeatService, SystemMetrics,
    DEFAULT_HEARTBEAT_INTERVAL_SECONDS,
};
pub use policy_service::{
    PolicyError, PolicyResult, PolicyService, PolicyServiceResult, RangeValue,
};
pub use product_service::{ProductError, ProductService};
pub use qr_service::{QrError, QrEvent, QrPaymentResult, QrService};
pub use return_service::{
    RefundMethod, ReturnEligibility, ReturnError, ReturnItem, ReturnReason, ReturnResult,
    ReturnService, ReturnServiceResult, DEFAULT_RETURN_WINDOW_DAYS,
};
pub use shared_draft_service::{
    SharedDraft, SharedDraftError, SharedDraftResult, SharedDraftService, SyncQueueResult,
};
pub use shift_service::{
    default_denominations_lyd, CashCount, CloseShiftResult, Denomination, ShiftError, ShiftResult,
    ShiftService, StartShiftResult,
};
pub use support_service::{SupportContact, SupportService};
pub use sync_service::{SyncEvent, SyncService, SyncStatus, DEFAULT_SYNC_INTERVAL_MINUTES};
pub use system_service::SystemService;
pub use transaction_service::{
    CompleteResult, ShiftTotals, TransactionError, TransactionResult, TransactionService,
};
pub use update_service::{ReleaseNotes, UpdateError, UpdateResult, UpdateService, VersionInfo};
pub use z_report_service::{CloseDayResult, ZReportError, ZReportResult, ZReportService};

// Re-export model types that were previously defined in services
pub use pos_models::{ShiftStatus, ShiftSummary, VarianceStatus, ZReport};
