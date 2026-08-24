//! API Module - HTTP client and backend communication
//!
//! This module handles all communication with the E2Manage backend API.
//!
//! ## Architecture
//!
//! - **client**: HTTP client wrapper with authentication headers
//! - **auth**: Terminal authentication, login, and heartbeat
//! - **sync**: Sync DTOs for catalog, operators, and screens
//! - **failure**: How a call to the platform failed — refused, unreachable, or unreadable
//! - **refusal_details**: The typed figures a refusal carries beside its code
//! - **session**: The terminal's session token and what happened when it was renewed
//!
//! ## Usage
//!
//! ```rust,ignore
//! use crate::api::{ApiClient, LoginTerminalResponse};
//!
//! let client = ApiClient::new("https://api.e2manage.com");
//!
//! // Login terminal
//! let response = client.login_terminal("HW-123", "secret-key").await?;
//! println!("Logged in as {}", response.terminal_code);
//!
//! // Make authenticated request
//! let data: SomeData = client.get("/api/pos/some-endpoint").await?;
//! ```

pub mod auth;
pub mod cart;
pub mod client;
pub mod failure;
pub mod features;
pub mod offline;
pub mod platform;
pub mod refusal_details;
pub mod reports;
pub mod returns;
pub mod session;
pub mod shifts;
pub mod sync;
pub mod transactions;

// Re-export main types
pub use client::{ApiClient, ApiErrorDetail, ApiErrorResponse, Enveloped, GetResult, OnlineStatus};
pub use failure::{
    ApiFailure, ApiResult, OperatorSessionRefusal, ServerErrorCode, TerminalStanding,
};

pub use refusal_details::{
    CapabilityCode, HeldBy, LockoutNotice, OfflineReportExpiredDetails,
    OfflineReportOverBudgetDetails, OperatorCapabilityDeniedDetails, OperatorLockedDetails,
    PinInvalidDetails, PinPolicyViolationDetails, PinRotationRequiredDetails, RefusalDetails,
    SupervisorApprovalRequiredDetails,
};
/// The status type [`ApiFailure::Refused`] carries.
///
/// Re-exported because that field is public: a caller outside this crate can already *read* the
/// status and could not *name* it, so anything wanting to construct a refusal — every test that
/// exercises a downstream branch on one — had to depend on `reqwest` directly to do it. That is
/// the dependency `pos-api` exists to keep to itself.
pub use reqwest::StatusCode;
pub use session::{BlankSessionToken, OperatorSession, ReauthOutcome, SessionToken};

pub use auth::{
    // Pairing types
    DeviceInfo,
    HeartbeatRequest,
    HeartbeatResponse,
    LoginTerminalRequest,
    LoginTerminalResponse,
    PairedTerminalInfo,
    PairingStatus,
    PairingStatusResponse,
    ReceiptConfig,
    RefreshResponse,
    RequestPairingRequest,
    RequestPairingResponse,
    TaxConfig,
    TerminalCommand,
    TerminalConfig,
    VerifyPinRequest,
    VerifyPinResponse,
};

pub use sync::{
    CatalogDeltaResponse, CatalogResponse, CategoryDto, CustomerDto, CustomersResponse,
    OperatorDto, OperatorsResponse, PaymentMethodDto, PaymentMethodsResponse, ProductDto,
    ScreenDefinitionDto, ScreensResponse,
};

pub use platform::{
    // Update types
    CheckUpdateResponse,
    EnforcementMode,
    HardwareInfo,
    HeartbeatStatus,
    LicenseStatus,
    OsInfo,
    PlatformCommand,
    PlatformCommandType,
    // Heartbeat types
    PlatformHeartbeatRequest,
    PlatformHeartbeatResponse,
    PolicyType,
    // Device registration types
    RegisterDeviceRequest,
    RegisterDeviceResponse,
    ReportVersionRequest,
    SecurityCategory,
    // Security policy types
    SecurityPoliciesResponse,
    SecurityPolicy,
};

pub use transactions::{
    CreateTransactionRequest, CreateTransactionResponse, PaymentDto, TransactionDetailDto,
    TransactionDetailItemDto, TransactionDetailPaymentDto, TransactionItemDto,
    VoidTransactionRequest,
};

pub use shifts::{DenominationDto, EndShiftRequest, StartShiftRequest, StartShiftResponse};

pub use offline::{UploadOfflineTransactionRequest, UploadOfflineTransactionResponse};

pub use returns::{CreateReturnRequest, CreateReturnResponse, ReturnItemRequest};

pub use reports::{ZReportRequest, ZReportResponse};

pub use cart::{
    CartItemDto, CartListResponse, CartResponse, ConvertCartRequest, CreateCartRequest,
};
