//! Utils Module - Utility functions
//!
//! This module contains utility functions and helpers.
//!
//! ## Available Modules
//!
//! - `time` - Time formatting utilities (time_ago, countdown, etc.)
//! - `formatting` - Locale-aware number, currency, and percentage formatting
//!
//! ## Planned Modules (to be implemented)
//!
//! - `config` - Configuration loading
//! - `error` - Error types
//!
//! ## Usage
//!
//! ```rust,ignore
//! use e2manage_pos_terminal::utils::{time_ago, format_countdown, format_receipt_datetime};
//! use e2manage_pos_terminal::utils::{format_number, format_currency, format_percentage};
//! use chrono::Utc;
//!
//! let dt = Utc::now();
//! println!("{}", time_ago(dt, "ar")); // الآن
//! println!("{}", time_ago(dt, "en")); // just now
//!
//! use rust_decimal::Decimal;
//! println!("{}", format_currency(Decimal::new(1234567, 3), "LYD", "ar")); // 1,234.567 د.ل
//! println!("{}", format_currency(Decimal::new(1234567, 3), "LYD", "en")); // 1,234.567 LYD
//! ```

pub mod formatting;
pub mod time;

// Re-export commonly used functions from time module
pub use time::{format_countdown, format_receipt_datetime, time_ago};

// Re-export commonly used functions from formatting module
pub use formatting::{
    currency_decimals, format_currency, format_number, format_percentage, format_quantity,
    to_eastern_arabic,
};

// pub mod config;
// pub mod error;
