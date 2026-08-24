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
//! - **write_outcome_bridge**: flattens a platform write outcome into what a screen shows —
//!   and keeps the two capability refusals in two different variants, because rendering them
//!   alike sends a cashier to fetch someone who is refused in turn

pub mod cart_bridge;
pub mod conflict_bridge;
pub mod navigation;
pub mod write_outcome_bridge;

pub use cart_bridge::{CartBridge, CartItemModel, CartTotals};
pub use conflict_bridge::{ConflictBridge, ConflictModel};
pub use navigation::{NavigationResult, Navigator};
pub use write_outcome_bridge::WriteOutcomeModel;
