//! UI Module - Slint Integration
//!
//! This module provides bridges between Rust services and the Slint UI.
//!
//! ## Available Bridges
//!
//! - **cart_bridge**: Converts cart data to Slint-compatible types
//! - **conflict_bridge**: Converts conflict data to Slint-compatible types
//! - **navigation**: Feature-aware navigation with screen validation

pub mod cart_bridge;
pub mod conflict_bridge;
pub mod navigation;

pub use cart_bridge::{CartBridge, CartItemModel, CartTotals};
pub use conflict_bridge::{ConflictBridge, ConflictModel};
pub use navigation::{NavigationResult, Navigator};
