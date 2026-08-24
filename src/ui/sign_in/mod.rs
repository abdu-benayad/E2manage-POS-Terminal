//! View models for the sign-in screen.
//!
//! Holds no toolkit dependency and must not acquire one: the view layer imports these, never the
//! reverse. See [`super`] for the rule this tree lives under.
//!
//! The shapes here exist to make one class of screen bug unrepresentable rather than merely
//! avoided — a refused PIN and an undecided one have no type in common, so no element can render
//! them alike. [`notice`] carries the reasoning.

pub mod enquiry;
pub mod notice;
pub mod phase;
pub mod strings;

pub use enquiry::{
    AuthAnswer, AuthEnquiry, Discardable, DispatchedEnquiry, EnquiryId, EnquiryIds, PairingCode,
    PendingEnquiry,
};
pub use notice::{PadOffer, Recheck, RefusalNotice, SignedInAtTheTill, UndecidedNotice};
pub use phase::{advance, AuthPhase, OperatorCard, PinEntryStanding, PAIRING_POLL_INTERVAL};
pub use strings::Sentence;
