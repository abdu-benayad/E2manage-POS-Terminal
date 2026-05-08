//! E2Manage POS Terminal - Library
//!
//! This library provides the core functionality for the POS Terminal application.
//! The codebase is organized into workspace crates for better modularity:
//!
//! - **pos_models**: Domain models (Product, Cart, Transaction, etc.)
//! - **pos_db**: Local SQLite database for offline operation
//! - **pos_api**: HTTP client for server communication
//! - **pos_escpos**: ESC/POS thermal printer commands
//! - **pos_printing**: Receipt and report printing
//! - **pos_services**: Business logic services
//!
//! Additional modules in this crate:
//! - **hardware**: Hardware abstraction (printers, scanners)
//! - **ui**: Slint UI bridges and helpers
//! - **utils**: Utility functions
//!
//! ## Example
//!
//! ```rust,ignore
//! use e2manage_pos_terminal::{Database, init_database, ApiClient};
//! use e2manage_pos_terminal::services::{AuthService, SyncService, ProductService, CartService};
//! use e2manage_pos_terminal::models::{Product, Category, Cart, CartItem};
//! use std::sync::Arc;
//!
//! // Initialize database
//! let db = Arc::new(init_database(&PathBuf::from("./data"))?);
//!
//! // Create API client
//! let api = Arc::new(ApiClient::new("https://api.e2manage.com"));
//!
//! // Create auth service
//! let auth = AuthService::new(Arc::clone(&api), Arc::clone(&db));
//!
//! // Verify PIN offline
//! let result = auth.verify_pin_sync("op1", "1234")?;
//! ```

// Local modules (not split into workspace crates yet)
pub mod hardware;
pub mod platform;
pub mod ui;
pub mod utils;

// Re-export workspace crates for backwards compatibility
pub use pos_models as models;
pub use pos_db as db;
pub use pos_api as api;
pub use pos_escpos as escpos;
pub use pos_printing as printing;
pub use pos_services as services;

// Re-export commonly used types from db
pub use pos_db::{init_database, init_memory_database, Database};

// Re-export commonly used types from api
pub use pos_api::{ApiClient, GetResult, LoginTerminalResponse, TerminalConfig};

// Re-export sync DTOs
pub use pos_api::{CatalogResponse, CategoryDto, OperatorDto, OperatorsResponse, ProductDto};

// Re-export platform API types
pub use pos_api::{
    HeartbeatStatus, PlatformCommand, PlatformCommandType,
    SecurityPolicy, SecurityCategory, EnforcementMode, PolicyType,
    CheckUpdateResponse, LicenseStatus,
};

// Re-export commonly used types from services
pub use pos_services::{AuthService, PinVerificationResult, TerminalSession};
pub use pos_services::{CartService, CartError, CartResult};
pub use pos_services::{ProductService, ProductError};
pub use pos_services::{SyncService, SyncEvent, SyncStatus};
pub use pos_services::{TransactionService, TransactionError, TransactionResult, CompleteResult};
pub use pos_services::{PrintService, PrinterConfig, ShiftReport};
pub use pos_services::{ShiftService, ShiftError, ShiftResult, ShiftSummary, VarianceStatus};
pub use pos_services::{ZReportService, ZReportError, ZReportResult};
pub use pos_services::{ReturnService, ReturnError, ReturnItem, ReturnReason, RefundMethod};
pub use pos_services::{DraftService, DraftError, Draft};
pub use pos_services::{OfflineService, QueueStats};
pub use pos_services::{PairingService, PairingState, TerminalRegistration};
pub use pos_services::{EmvService, EmvEvent, CardPaymentResult};
pub use pos_services::{QrService, QrEvent, QrPaymentResult};
pub use pos_services::{FeatureService, FeatureError, FeatureResult};
pub use pos_services::{LogService, LogEntry, LogLevel, LogError, LogResult};
pub use pos_services::{SupportService, SupportContact};
pub use pos_services::{UpdateService, UpdateError, UpdateResult, VersionInfo, ReleaseNotes};
pub use pos_services::SystemService;
pub use pos_services::{PlatformHeartbeatService, HeartbeatEvent, HeartbeatError, DEFAULT_HEARTBEAT_INTERVAL_SECONDS};
pub use pos_services::{PolicyService, PolicyResult, PolicyError};

// Re-export commonly used types from models
pub use pos_models::{Product, Category, ProductUnit, ProductSearchResult};
pub use pos_models::{Cart, CartItem};
pub use pos_models::{Transaction, TransactionItem, TransactionStatus, Payment, PaymentMethod};
pub use pos_models::ZReport;

// Re-export ESC/POS types
pub use pos_escpos::{EscPos, Alignment, CodePage, BarcodeFormat};
