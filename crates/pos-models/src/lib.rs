//! POS Models - Domain models for E2Manage POS Terminal
//!
//! This crate contains the core domain models:
//! - `Product` and `Category` - product catalog models
//! - `Cart` and `CartItem` - shopping cart models
//! - `Transaction`, `Payment`, `PaymentMethod` - transaction models
//! - `ShiftSummary`, `VarianceStatus`, `ShiftStatus` - shift models
//! - `ZReport` - end of day report model
//! - `Feature`, `FeatureScreen` - feature library models
//! - `OperatorId`, `OperatorName`, `OperatorRole`, `OperatorPermissions`, `VerifiedOperator` -
//!   operator identity models
//! - `Pin`, `PinPolicy`, `PinLength` - PIN entry and the policy that governs it

pub mod cart;
pub mod feature;
pub mod operator;
pub mod parse;
pub mod pin;
pub mod product;
pub mod shift;
pub mod transaction;
pub mod z_report;

// Re-export main types for convenience
pub use cart::{Cart, CartItem};
pub use feature::{Feature, FeatureDto, FeatureScreen, FeatureScreenDto, FeaturesResponse};
pub use operator::{
    DiscountAuthority, DiscountPercent, NameScript, OperatorError, OperatorId, OperatorName,
    OperatorPermissions, OperatorRole, Permission, VerifiedOperator,
};
pub use parse::ParseError;
pub use pin::{
    MaxAttempts, OfflineWindow, Pin, PinFormatError, PinLength, PinPolicy, PinPolicyError,
};
pub use product::{
    Category, Product, ProductNature, ProductSearchResult, ProductType, ProductUnit,
};
pub use shift::{ShiftStatus, ShiftSummary, VarianceStatus};
pub use transaction::{
    generate_receipt_number, Payment, PaymentMethod, Transaction, TransactionItem,
    TransactionStatus,
};
pub use z_report::ZReport;
