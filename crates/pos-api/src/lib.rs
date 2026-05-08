//! API Module - HTTP client and backend communication
//!
//! This module handles all communication with the E2Manage backend API.
//!
//! ## Architecture
//!
//! - **client**: HTTP client wrapper with authentication headers
//! - **auth**: Terminal authentication, login, and heartbeat
//! - **sync**: Sync DTOs for catalog, operators, and screens
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
pub mod features;
pub mod platform;
pub mod sync;

// Re-export main types
pub use client::{ApiClient, ApiErrorResponse, GetResult, OnlineStatus};

pub use auth::{
    HeartbeatRequest, HeartbeatResponse, LoginTerminalRequest, LoginTerminalResponse,
    ReceiptConfig, RegisterTerminalRequest, RegisterTerminalResponse, TaxConfig, TerminalCommand,
    TerminalConfig, VerifyPinRequest, VerifyPinResponse,
    // Pairing types
    DeviceInfo, PairedTerminalInfo, PairingStatus, PairingStatusResponse,
    RequestPairingRequest, RequestPairingResponse,
};

pub use sync::{
    CatalogDeltaResponse, CatalogResponse, CategoryDto, CustomerDto, CustomersResponse, OperatorDto,
    OperatorsResponse, PaymentMethodDto, PaymentMethodsResponse, ProductDto, ScreenDefinitionDto,
    ScreensResponse,
};

pub use platform::{
    // Heartbeat types
    PlatformHeartbeatRequest, PlatformHeartbeatResponse, HeartbeatStatus,
    PlatformCommand, PlatformCommandType,
    // Security policy types
    SecurityPoliciesResponse, SecurityPolicy, SecurityCategory, PolicyType, EnforcementMode,
    // Update types
    CheckUpdateResponse, ReportVersionRequest,
    // Device registration types
    RegisterDeviceRequest, RegisterDeviceResponse, OsInfo, HardwareInfo, LicenseStatus,
};

pub use cart::{
    CartItemDto, CartListResponse, CartResponse, ConvertCartRequest, CreateCartRequest,
};
