//! UI Module - view-model bridges
//!
//! This module converts service types into flat, render-ready shapes. It holds no
//! dependency on any UI toolkit, and must not acquire one: the view layer imports
//! these, never the reverse.
//!
//! ## Available Bridges
//!
//! - **cart_bridge**: flattens cart data into per-row view models
//! - **conflict_bridge**: flattens sync conflicts into per-row view models
//! - **navigation**: Feature-aware navigation with screen validation

pub mod cart_bridge;
pub mod conflict_bridge;
pub mod navigation;

pub use cart_bridge::{CartBridge, CartItemModel, CartTotals};
pub use conflict_bridge::{ConflictBridge, ConflictModel};
pub use navigation::{NavigationResult, Navigator};
