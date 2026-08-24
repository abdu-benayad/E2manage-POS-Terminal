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
//! - **sign_in**: what the sign-in screen shows once a PIN attempt has an outcome — a refusal
//!   and an undecided outcome share no type, so no element can render them alike

pub mod cart_bridge;
pub mod conflict_bridge;
pub mod navigation;
pub mod sign_in;
pub mod write_outcome_bridge;

pub use cart_bridge::{CartBridge, CartItemModel, CartTotals};
pub use conflict_bridge::{ConflictBridge, ConflictModel};
pub use navigation::{NavigationResult, Navigator};
pub use sign_in::{PadOffer, Recheck, RefusalNotice, Sentence, SignedInAtTheTill, UndecidedNotice};
pub use write_outcome_bridge::WriteOutcomeModel;
