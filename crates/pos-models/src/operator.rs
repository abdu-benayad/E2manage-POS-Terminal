//! Operator identity — who is standing at the till, and what they are allowed to do.
//!
//! The types here mirror one server contract each, and each names the file it mirrors so the
//! next person can check it rather than trust it:
//!
//! - [`OperatorRole`] mirrors `enum POS_OperatorRole` (`wadi-dms-api/prisma/pos.prisma:942-946`).
//! - [`OperatorPermissions`] mirrors `operatorPermissionsSchema`
//!   (`wadi-dms-api/src/modules/pos/presentation/validators/operator.validator.ts:14-23`), which
//!   is the authoritative shape: `POS_OperatorProfile.permissions` is a `Json?` column, so the
//!   zod schema that validates writes to it is the only contract there is.
//! - [`OperatorId`] and [`OperatorName`] mirror the operator projection emitted by
//!   `GET /api/pos/sync/operators` (`.../presentation/controllers/sync.controller.ts:839-853`).
//!
//! # Why the permission mapping lives in exactly one place
//!
//! It used to live in two, and they drifted. `pos_api::sync::OperatorPermissions` deserialises
//! the wire in `camelCase`; `pos_db::OperatorPermissions` deserialises the same JSON in
//! `snake_case`. Measured on 2026-08-22: `pos-api` writes `{"canVoid":true,…}` into the
//! `permissions_json` column, `pos-db` reads it back, serde reports `missing field 'can_void'`,
//! and `OperatorRow::permissions()` swallows that with `.ok().unwrap_or_default()` — so a manager
//! synced with every privilege reads back with none of them. It fails closed today, which is why
//! nobody noticed; the mechanism would fail open just as quietly if a default ever flipped.
//!
//! [`OperatorPermissions`] therefore owns a single private wire struct and converts through it in
//! both directions. Two crates cannot drift from a mapping they do not each define.

use std::collections::BTreeSet;
use std::fmt;
use std::str::FromStr;

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::parse::ParseError;

// ============================================================================
// Errors
// ============================================================================

/// A value that cannot describe an operator.
///
/// Distinct from [`ParseError`], which is this crate's shared answer to "a stored string names no
/// variant of the enum it was read as". These are constraint violations on values that parsed.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum OperatorError {
    /// An operator id was blank. The empty string is how "there is no operator" is spelled
    /// across this codebase today, and it is exactly the confusion `OperatorId` exists to end.
    #[error(
        "an operator id cannot be blank; an absent operator is `Option<OperatorId>`, not `\"\"`"
    )]
    BlankId,

    /// An operator name was blank. The server builds it as `firstName + \" \" + lastName`, so a
    /// blank name means the record was written by something other than the server.
    #[error("an operator name cannot be blank")]
    BlankName,

    /// A discount ceiling outside the range the server's own validator enforces.
    #[error("a discount ceiling of {value}% is outside the contract's range of 0 to 100")]
    DiscountCeilingOutOfRange {
        /// The rejected value, verbatim, so the row that needs fixing can be found.
        value: Decimal,
    },
}

// ============================================================================
// OperatorId
// ============================================================================

/// The server's identifier for a POS operator profile (`POS_OperatorProfile.id`, a UUID).
///
/// The inner string is private and construction is fallible, so the empty string cannot enter
/// through this type. The format is deliberately *not* validated as a UUID: the till's own
/// fixtures and several existing rows use ids like `op-1`, and rejecting them would be this type
/// asserting an invariant the store does not actually hold.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct OperatorId(String);

impl OperatorId {
    /// Builds an operator id, rejecting a blank one.
    pub fn new(raw: impl Into<String>) -> Result<Self, OperatorError> {
        let raw = raw.into();
        if raw.trim().is_empty() {
            Err(OperatorError::BlankId)
        } else {
            Ok(Self(raw))
        }
    }

    /// The identifier as the server and the store spell it.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for OperatorId {
    type Error = OperatorError;

    fn try_from(raw: String) -> Result<Self, Self::Error> {
        Self::new(raw)
    }
}

impl From<OperatorId> for String {
    fn from(id: OperatorId) -> Self {
        id.0
    }
}

impl fmt::Display for OperatorId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

// ============================================================================
// OperatorName
// ============================================================================

/// Which script a name is being rendered in.
///
/// A named alternative to the `prefer_ar: bool` parameter this replaces: a bare boolean at a call
/// site says nothing about which way round it goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NameScript {
    /// The Latin-script name, as the server assembles it from the HR record.
    Latin,
    /// The Arabic-script name. Arabic is the till's default locale (`config/default.toml`).
    Arabic,
}

/// An operator's name in both scripts the till renders.
///
/// The store keeps `name` and `name_ar` in separate columns and the server sends `name` and
/// `nameAr` as separate fields, so a single-string name would discard information both ends
/// already have. There is deliberately no `Serialize`/`Deserialize`: neither the wire nor the
/// store nests these, and giving the type a nested shape would invite one that nothing wants.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct OperatorName {
    latin: String,
    arabic: Option<String>,
}

impl OperatorName {
    /// Builds a name from the two scripts the server sends.
    ///
    /// A blank Arabic name normalises to absent: `Some("")` and `None` mean the same thing to
    /// every reader, and only one of them should be representable.
    pub fn new(
        latin: impl Into<String>,
        arabic: Option<impl Into<String>>,
    ) -> Result<Self, OperatorError> {
        let latin = latin.into();
        if latin.trim().is_empty() {
            return Err(OperatorError::BlankName);
        }
        let arabic = arabic
            .map(Into::into)
            .filter(|name| !name.trim().is_empty());
        Ok(Self { latin, arabic })
    }

    /// The name in the requested script, falling back to Latin when no Arabic name was synced.
    ///
    /// The fallback is a rendering decision, not a data one: the till must draw *something* above
    /// the cart, and [`Self::arabic`] still reports the absence to anyone who needs to know.
    pub fn in_script(&self, script: NameScript) -> &str {
        match script {
            NameScript::Latin => &self.latin,
            NameScript::Arabic => self.arabic.as_deref().unwrap_or(&self.latin),
        }
    }

    /// The Latin-script name, which is always present.
    pub fn latin(&self) -> &str {
        &self.latin
    }

    /// The Arabic-script name, absent when the HR record carries no Arabic spelling.
    pub fn arabic(&self) -> Option<&str> {
        self.arabic.as_deref()
    }

    /// The first letters of the first two words, for an avatar.
    ///
    /// Takes a script rather than assuming Latin: an Arabic-locale till showing Latin initials
    /// beside an Arabic name is the same mismatch [`Self::in_script`] exists to prevent. Casing is
    /// applied unconditionally because it is a no-op in scripts that have no case.
    pub fn initials(&self, script: NameScript) -> String {
        self.in_script(script)
            .split_whitespace()
            .filter_map(|word| word.chars().next())
            .take(2)
            .collect::<String>()
            .to_uppercase()
    }
}

impl fmt::Display for OperatorName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.latin)
    }
}

/// An operator's name as it was written onto a document — a receipt, a shift report, a parked
/// cart, a shared draft.
///
/// **Not the same concept as [`OperatorName`], and deliberately a different type.** Two facts
/// separate them:
///
/// - **One script, because every place that keeps one has room for one.** `shared_drafts
///   .operator_name` is a single `TEXT` column and the platform's cart API sends a single JSON
///   string; the `shifts`, `drafts` and `offline_transactions` tables keep no name at all. If a
///   document held an [`OperatorName`], its Arabic half would be present in memory and absent
///   after a save, and nothing could tell that apart from an operator who has no Arabic name.
///   Widening those columns is tier 3a's problem; this type states the constraint rather than
///   hiding it.
/// - **It is a snapshot, not a reference.** The operator may be renamed afterwards, and a receipt
///   reprinted next year has to say what it said. An id points at whoever the operator is now; a
///   recorded name says who they were then. Both belong on a financial record, for that reason.
///
/// Serialized as a bare string, which is what the single column and the single JSON field hold.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct RecordedOperatorName(String);

impl RecordedOperatorName {
    /// Records a name onto a document, rejecting a blank one.
    pub fn new(name: impl Into<String>) -> Result<Self, OperatorError> {
        let name = name.into();
        if name.trim().is_empty() {
            Err(OperatorError::BlankName)
        } else {
            Ok(Self(name))
        }
    }

    /// The recorded name.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl OperatorName {
    /// Takes the snapshot a document keeps, in the script the till was rendering.
    ///
    /// The script is a parameter because the choice is real: a receipt printed in Arabic should
    /// record the Arabic spelling, and nothing else in the system can make that call afterwards
    /// from a single column.
    pub fn recorded_in(&self, script: NameScript) -> RecordedOperatorName {
        RecordedOperatorName(self.in_script(script).to_string())
    }
}

impl TryFrom<String> for RecordedOperatorName {
    type Error = OperatorError;

    fn try_from(name: String) -> Result<Self, Self::Error> {
        Self::new(name)
    }
}

impl From<RecordedOperatorName> for String {
    fn from(name: RecordedOperatorName) -> Self {
        name.0
    }
}

impl fmt::Display for RecordedOperatorName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

// ============================================================================
// OperatorRole
// ============================================================================

/// The operator's role, mirroring `enum POS_OperatorRole`.
///
/// Closed, and matched exactly: the server's enum admits three values and nothing else, so a
/// fourth means the contract moved. There is deliberately no `Default` and no catch-all variant —
/// the server defaults an unset role to `CASHIER` *at write time*, and a till-side fallback would
/// be a privilege decision made by whichever code path happened to read a value it did not know.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum OperatorRole {
    /// Rings sales. The server grants this role no privileges by default.
    Cashier,
    /// Rings sales and authorises a cashier's voids, refunds and discounts.
    Supervisor,
    /// A supervisor who may also reach the till's settings.
    Manager,
}

impl OperatorRole {
    /// The spelling the server and the store both use.
    pub const fn as_wire_str(self) -> &'static str {
        match self {
            Self::Cashier => "CASHIER",
            Self::Supervisor => "SUPERVISOR",
            Self::Manager => "MANAGER",
        }
    }
}

impl FromStr for OperatorRole {
    type Err = ParseError;

    /// Parses the wire spelling, case-sensitively. A lowercase `cashier` is not accepted: nothing
    /// in the contract produces one, so its presence means something other than the server wrote
    /// the row, and that is worth surfacing rather than absorbing.
    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        match raw {
            "CASHIER" => Ok(Self::Cashier),
            "SUPERVISOR" => Ok(Self::Supervisor),
            "MANAGER" => Ok(Self::Manager),
            other => Err(ParseError::OperatorRole(other.to_string())),
        }
    }
}

impl fmt::Display for OperatorRole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_wire_str())
    }
}

// ============================================================================
// Permissions
// ============================================================================

/// One thing an operator is allowed to do.
///
/// Discounting is absent on purpose — it is not a yes/no capability but a bounded one, and it is
/// carried by [`DiscountAuthority`] so that "may discount" and "up to what" cannot disagree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Permission {
    /// Void a transaction that has not been settled.
    VoidTransaction,
    /// Refund a settled transaction.
    RefundTransaction,
    /// Open the cash drawer outside a sale.
    OpenDrawer,
    /// Read the till's shift and sales reports.
    ViewReports,
    /// Open and close shifts.
    ManageShifts,
    /// Reach the till's settings.
    AccessSettings,
}

impl Permission {
    /// The key this permission occupies in the server's permissions object.
    ///
    /// Exhaustive on purpose: a new variant fails to compile here, which is the only thing that
    /// forces whoever adds it to decide what the server calls it.
    pub const fn wire_key(self) -> &'static str {
        match self {
            Self::VoidTransaction => "canVoid",
            Self::RefundTransaction => "canRefund",
            Self::OpenDrawer => "canOpenDrawer",
            Self::ViewReports => "canViewReports",
            Self::ManageShifts => "canManageShifts",
            Self::AccessSettings => "canAccessSettings",
        }
    }
}

/// A discount ceiling, as a percentage between 0 and 100 inclusive.
///
/// `Decimal`, never `f64`: the value bounds a subtraction from a price, and the workspace's rule
/// against binary floating point in money paths does not stop at the amount itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DiscountPercent(Decimal);

impl DiscountPercent {
    /// Builds a ceiling, enforcing the same `0..=100` bound the server's validator enforces.
    pub fn new(value: Decimal) -> Result<Self, OperatorError> {
        if value < Decimal::ZERO || value > Decimal::ONE_HUNDRED {
            Err(OperatorError::DiscountCeilingOutOfRange { value })
        } else {
            Ok(Self(value))
        }
    }

    /// The ceiling as a percentage — `20` means twenty percent, not one fifth.
    pub const fn as_percent(self) -> Decimal {
        self.0
    }
}

impl fmt::Display for DiscountPercent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}%", self.0)
    }
}

/// How far an operator may discount.
///
/// The wire carries this as two fields that can contradict each other — `canDiscount: false` next
/// to `maxDiscountPercent: 20` is representable there and meaningless. Here it is one value, so
/// the contradiction has nowhere to live.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DiscountAuthority {
    /// The operator may not apply discounts.
    Denied,
    /// The operator may apply a discount up to and including this ceiling.
    UpTo(DiscountPercent),
}

impl DiscountAuthority {
    /// The ceiling, when there is one. `None` and [`Self::Denied`] are the same statement.
    pub const fn ceiling(self) -> Option<DiscountPercent> {
        match self {
            Self::Denied => None,
            Self::UpTo(ceiling) => Some(ceiling),
        }
    }
}

/// What an operator is allowed to do at this till.
///
/// There is deliberately no `Default`. `OperatorPermissions::default()` is how the current defect
/// spells "I could not read the permissions, so here are none of them" — silently, inside an
/// `unwrap_or_default()`. [`Self::none`] says the same thing at a call site that can be found.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "PermissionsWire", into = "PermissionsWire")]
pub struct OperatorPermissions {
    granted: BTreeSet<Permission>,
    discount: DiscountAuthority,
}

impl OperatorPermissions {
    /// An operator who may do nothing beyond ringing a sale.
    pub fn none() -> Self {
        Self {
            granted: BTreeSet::new(),
            discount: DiscountAuthority::Denied,
        }
    }

    /// Builds a permission set from a decided list of capabilities and a decided discount
    /// authority.
    pub fn new(granted: impl IntoIterator<Item = Permission>, discount: DiscountAuthority) -> Self {
        Self {
            granted: granted.into_iter().collect(),
            discount,
        }
    }

    /// Whether this operator holds a capability.
    pub fn allows(&self, permission: Permission) -> bool {
        self.granted.contains(&permission)
    }

    /// Every capability held, in a stable order.
    pub fn granted(&self) -> impl Iterator<Item = Permission> + '_ {
        self.granted.iter().copied()
    }

    /// How far this operator may discount.
    pub const fn discount_authority(&self) -> DiscountAuthority {
        self.discount
    }
}

/// The exact shape of the server's permissions object — the one place this crate spells those
/// keys, and the only thing that converts to or from [`OperatorPermissions`].
///
/// Every field defaults, and every default is the denying value. That is what makes `#[serde(
/// default)]` acceptable on a privilege type: an absent key grants nothing, so an older row or a
/// `null` column produces an operator who may do nothing, not one who may do everything.
///
/// Unknown keys are ignored rather than rejected. A privilege the server adds and this till has
/// not learned about is simply not granted, which is the correct direction to fail; rejecting the
/// whole object would strip every *known* privilege from every operator the moment the platform
/// shipped a new one.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PermissionsWire {
    #[serde(default)]
    can_void: bool,
    #[serde(default)]
    can_refund: bool,
    #[serde(default)]
    can_discount: bool,
    #[serde(default, with = "rust_decimal::serde::float")]
    max_discount_percent: Decimal,
    #[serde(default)]
    can_open_drawer: bool,
    #[serde(default)]
    can_view_reports: bool,
    #[serde(default)]
    can_manage_shifts: bool,
    #[serde(default)]
    can_access_settings: bool,
}

impl TryFrom<PermissionsWire> for OperatorPermissions {
    type Error = OperatorError;

    /// Resolves the wire's two discount fields into one authority.
    ///
    /// `canDiscount: false` wins over any ceiling beside it, and a ceiling of zero is
    /// [`DiscountAuthority::Denied`] rather than `UpTo(0)` — the two authorise exactly the same
    /// set of discounts, so only one of them should be representable. The ceiling's range is
    /// checked only when `canDiscount` is set, because a value nothing reads cannot harm anyone.
    fn try_from(wire: PermissionsWire) -> Result<Self, Self::Error> {
        let discount = if wire.can_discount && wire.max_discount_percent > Decimal::ZERO {
            DiscountAuthority::UpTo(DiscountPercent::new(wire.max_discount_percent)?)
        } else {
            DiscountAuthority::Denied
        };

        let granted = [
            (Permission::VoidTransaction, wire.can_void),
            (Permission::RefundTransaction, wire.can_refund),
            (Permission::OpenDrawer, wire.can_open_drawer),
            (Permission::ViewReports, wire.can_view_reports),
            (Permission::ManageShifts, wire.can_manage_shifts),
            (Permission::AccessSettings, wire.can_access_settings),
        ]
        .into_iter()
        .filter_map(|(permission, held)| held.then_some(permission));

        Ok(Self::new(granted, discount))
    }
}

impl From<OperatorPermissions> for PermissionsWire {
    fn from(permissions: OperatorPermissions) -> Self {
        let ceiling = permissions.discount.ceiling();
        Self {
            can_void: permissions.allows(Permission::VoidTransaction),
            can_refund: permissions.allows(Permission::RefundTransaction),
            can_discount: ceiling.is_some(),
            max_discount_percent: ceiling.map_or(Decimal::ZERO, DiscountPercent::as_percent),
            can_open_drawer: permissions.allows(Permission::OpenDrawer),
            can_view_reports: permissions.allows(Permission::ViewReports),
            can_manage_shifts: permissions.allows(Permission::ManageShifts),
            can_access_settings: permissions.allows(Permission::AccessSettings),
        }
    }
}

// ============================================================================
// VerifiedOperator
// ============================================================================

/// The operator, as everything past authentication is allowed to see them.
///
/// This is the projection that crosses the auth boundary so that `pos_db::OperatorRow` — which
/// carries a bcrypt PIN hash — does not. A type the domain *can* hold is a type the domain
/// eventually *will* hold, and `pin_hash` has no business in a cart, a shift or a receipt.
///
/// There is no `Serialize`/`Deserialize`: this value is passed in process, and giving it a wire
/// form would be an invitation to persist an authentication result. What outlives the session is
/// the [`OperatorId`] recorded on a transaction, not the operator.
///
/// Constructibility is by convention rather than by proof, and this comment is the honest version
/// of that: `from_verified_pin` names the act, but nothing yet stops another caller from using
/// it. `06-pin-verification-outcome-types` makes `PinVerification::Accepted` its intended
/// producer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedOperator {
    id: OperatorId,
    name: OperatorName,
    role: OperatorRole,
    permissions: OperatorPermissions,
}

impl VerifiedOperator {
    /// Records that this operator's PIN was checked against their stored hash and matched.
    ///
    /// Call this only from the code that performed that check.
    pub fn from_verified_pin(
        id: OperatorId,
        name: OperatorName,
        role: OperatorRole,
        permissions: OperatorPermissions,
    ) -> Self {
        Self {
            id,
            name,
            role,
            permissions,
        }
    }

    /// The operator's server identifier — what a transaction or shift records.
    pub fn id(&self) -> &OperatorId {
        &self.id
    }

    /// The operator's name, in both scripts.
    pub fn name(&self) -> &OperatorName {
        &self.name
    }

    /// The operator's role.
    pub const fn role(&self) -> OperatorRole {
        self.role
    }

    /// What this operator is allowed to do.
    pub const fn permissions(&self) -> &OperatorPermissions {
        &self.permissions
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    /// One operator exactly as `GET /api/pos/sync/operators` emits it, with the permissions the
    /// server's own `getDefaultPermissions('SUPERVISOR')` writes
    /// (`.../presentation/controllers/operator.controller.ts:661-672`).
    const SUPERVISOR_PAYLOAD: &str = r#"{
        "id": "9f1c7a3e-2b4d-4f8a-9c6e-1d2f3a4b5c6d",
        "employeeId": "3c8f1a2b-5d6e-4a7b-8c9d-0e1f2a3b4c5d",
        "employeeNumber": "EMP001",
        "name": "Ahmed Hassan",
        "nameAr": "أحمد حسن",
        "email": "ahmed@example.com",
        "pinHash": "$2b$12$abcdefghijklmnopqrstuv",
        "role": "SUPERVISOR",
        "permissions": {
            "canVoid": true,
            "canRefund": true,
            "canDiscount": true,
            "maxDiscountPercent": 20,
            "canOpenDrawer": true,
            "canViewReports": true,
            "canManageShifts": true,
            "canAccessSettings": false
        },
        "isActive": true,
        "department": "Sales",
        "position": "Supervisor",
        "updatedAt": "2026-08-21T10:00:00.000Z"
    }"#;

    /// Mirrors the server's operator projection field for field, including the two separate name
    /// fields — which is why `name` and `name_ar` are still strings here. That pair is the wire's
    /// shape, and [`OperatorName`] has no serde precisely so nothing gives the wire a nested shape
    /// it does not have; the conversion happens on the way in, and the test below is where this
    /// fixture proves it.
    #[derive(Debug, Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct SyncedOperator {
        id: OperatorId,
        name: String,
        name_ar: Option<String>,
        role: OperatorRole,
        permissions: Option<OperatorPermissions>,
    }

    fn supervisor() -> SyncedOperator {
        serde_json::from_str(SUPERVISOR_PAYLOAD).expect("the captured payload must deserialize")
    }

    #[test]
    fn operator_sync_payload_deserializes_into_the_domain_types() {
        let synced = supervisor();

        assert_eq!(synced.id.as_str(), "9f1c7a3e-2b4d-4f8a-9c6e-1d2f3a4b5c6d");
        assert_eq!(synced.role, OperatorRole::Supervisor);

        // The wire's two name fields are one domain value, and this is the conversion `pos-api`
        // performs in `OperatorDto::to_operator_row`. Asserting it here keeps the captured
        // payload honest about the boundary rather than only about the field names.
        let name = OperatorName::new(synced.name, synced.name_ar)
            .expect("the captured payload carries a name");
        assert_eq!(name.latin(), "Ahmed Hassan");
        assert_eq!(name.arabic(), Some("أحمد حسن"));
        assert_eq!(name.in_script(NameScript::Arabic), "أحمد حسن");

        let permissions = synced.permissions.expect("the payload carries permissions");
        assert!(permissions.allows(Permission::VoidTransaction));
        assert!(permissions.allows(Permission::ManageShifts));
        assert!(!permissions.allows(Permission::AccessSettings));
        assert_eq!(
            permissions.discount_authority(),
            DiscountAuthority::UpTo(DiscountPercent::new(Decimal::from(20)).unwrap())
        );
    }

    #[test]
    fn operator_permissions_survive_a_json_round_trip() {
        // The regression test for the defect this module exists to end: `pos-api` serialised the
        // wire in camelCase and `pos-db` deserialised it in snake_case, so `permissions_json`
        // came back as `Err(missing field 'can_void')` and `unwrap_or_default()` turned a
        // fully-privileged manager into an operator who could do nothing.
        let original = supervisor()
            .permissions
            .expect("the payload carries permissions");

        let json = serde_json::to_string(&original).expect("permissions serialize");
        let back: OperatorPermissions = serde_json::from_str(&json).expect("and deserialize");

        assert_eq!(back, original);
        assert!(
            json.contains("\"canManageShifts\""),
            "the stored shape must be the server's shape, got {json}"
        );
    }

    #[test]
    fn operator_permissions_absent_keys_grant_nothing() {
        let empty: OperatorPermissions = serde_json::from_str("{}").expect("an empty object");
        assert_eq!(empty, OperatorPermissions::none());
    }

    #[test]
    fn operator_permissions_ignore_a_privilege_this_till_does_not_know() {
        // A key the platform adds later must not strip the privileges this till does understand.
        let json = r#"{"canVoid":true,"canApproveReturns":true}"#;
        let permissions: OperatorPermissions = serde_json::from_str(json).expect("unknown key");

        assert!(permissions.allows(Permission::VoidTransaction));
        assert_eq!(permissions.granted().count(), 1);
    }

    #[test]
    fn operator_discount_ceiling_without_the_flag_is_denied() {
        let json = r#"{"canDiscount":false,"maxDiscountPercent":20}"#;
        let permissions: OperatorPermissions = serde_json::from_str(json).expect("contradiction");

        assert_eq!(permissions.discount_authority(), DiscountAuthority::Denied);
    }

    #[test]
    fn operator_discount_flag_without_a_ceiling_is_denied() {
        let json = r#"{"canDiscount":true,"maxDiscountPercent":0}"#;
        let permissions: OperatorPermissions = serde_json::from_str(json).expect("a zero ceiling");

        assert_eq!(permissions.discount_authority(), DiscountAuthority::Denied);
    }

    #[test]
    fn operator_discount_ceiling_beyond_the_contract_is_rejected() {
        let json = r#"{"canDiscount":true,"maxDiscountPercent":500}"#;
        let refused = serde_json::from_str::<OperatorPermissions>(json).unwrap_err();

        assert!(
            refused.to_string().contains("500"),
            "the rejected value must reach the message, got: {refused}"
        );
        assert_eq!(
            DiscountPercent::new(Decimal::from(500)),
            Err(OperatorError::DiscountCeilingOutOfRange {
                value: Decimal::from(500)
            })
        );
    }

    #[test]
    fn operator_role_rejects_a_value_outside_the_servers_enum() {
        assert_eq!("MANAGER".parse(), Ok(OperatorRole::Manager));
        assert_eq!(
            "ADMIN".parse::<OperatorRole>(),
            Err(ParseError::OperatorRole("ADMIN".to_string()))
        );
        // Lowercase is not a spelling the contract produces.
        assert!("cashier".parse::<OperatorRole>().is_err());
        // And serde must refuse it too, rather than falling back to a variant.
        assert!(serde_json::from_str::<OperatorRole>("\"ADMIN\"").is_err());
    }

    #[test]
    fn operator_role_round_trips_through_its_wire_spelling() {
        for role in [
            OperatorRole::Cashier,
            OperatorRole::Supervisor,
            OperatorRole::Manager,
        ] {
            assert_eq!(role.to_string().parse(), Ok(role));
            assert_eq!(
                serde_json::to_string(&role).unwrap(),
                format!("\"{}\"", role.as_wire_str())
            );
        }
    }

    #[test]
    fn operator_name_picks_the_arabic_script_under_the_default_locale() {
        // `config/default.toml` sets `locale = "ar"`.
        let name = OperatorName::new("Ahmed Hassan", Some("أحمد حسن")).unwrap();

        assert_eq!(name.in_script(NameScript::Arabic), "أحمد حسن");
        assert_eq!(name.in_script(NameScript::Latin), "Ahmed Hassan");
    }

    #[test]
    fn operator_name_falls_back_to_latin_when_no_arabic_name_was_synced() {
        let name = OperatorName::new("Ahmed Hassan", None::<String>).unwrap();

        assert_eq!(name.in_script(NameScript::Arabic), "Ahmed Hassan");
        assert_eq!(name.arabic(), None);
    }

    #[test]
    fn operator_name_treats_a_blank_arabic_name_as_absent() {
        let name = OperatorName::new("Ahmed Hassan", Some("   ")).unwrap();
        assert_eq!(name.arabic(), None);
    }

    #[test]
    fn operator_recorded_name_snapshots_the_script_the_till_rendered() {
        let name = OperatorName::new("Ahmed Hassan", Some("أحمد حسن")).unwrap();

        assert_eq!(name.recorded_in(NameScript::Arabic).as_str(), "أحمد حسن");
        assert_eq!(name.recorded_in(NameScript::Latin).as_str(), "Ahmed Hassan");
    }

    #[test]
    fn operator_recorded_name_round_trips_as_a_bare_string() {
        // A single TEXT column and a single JSON field are what actually hold one of these.
        let recorded = RecordedOperatorName::new("Ahmed Hassan").unwrap();
        let json = serde_json::to_string(&recorded).unwrap();

        assert_eq!(json, "\"Ahmed Hassan\"");
        assert_eq!(
            serde_json::from_str::<RecordedOperatorName>(&json).unwrap(),
            recorded
        );
    }

    #[test]
    fn operator_recorded_name_rejects_a_blank_snapshot() {
        assert_eq!(
            RecordedOperatorName::new("  "),
            Err(OperatorError::BlankName)
        );
        assert!(serde_json::from_str::<RecordedOperatorName>("\"\"").is_err());
    }

    #[test]
    fn operator_name_rejects_a_blank_latin_name() {
        assert_eq!(
            OperatorName::new("  ", Some("أحمد")),
            Err(OperatorError::BlankName)
        );
    }

    #[test]
    fn operator_id_rejects_the_empty_string() {
        assert_eq!(OperatorId::new(""), Err(OperatorError::BlankId));
        assert_eq!(OperatorId::new("   "), Err(OperatorError::BlankId));
        assert!(serde_json::from_str::<OperatorId>("\"\"").is_err());
    }

    #[test]
    fn operator_id_round_trips_through_json_as_a_bare_string() {
        let id = OperatorId::new("op-1").unwrap();
        let json = serde_json::to_string(&id).unwrap();

        assert_eq!(json, "\"op-1\"");
        assert_eq!(serde_json::from_str::<OperatorId>(&json).unwrap(), id);
    }

    #[test]
    fn verified_operator_carries_no_pin_material() {
        let synced = supervisor();
        let operator = VerifiedOperator::from_verified_pin(
            synced.id,
            OperatorName::new(synced.name, synced.name_ar).unwrap(),
            synced.role,
            synced.permissions.unwrap_or_else(OperatorPermissions::none),
        );

        assert_eq!(operator.role(), OperatorRole::Supervisor);
        assert_eq!(operator.name().in_script(NameScript::Arabic), "أحمد حسن");
        assert!(operator.permissions().allows(Permission::RefundTransaction));

        // The `Debug` rendering is what reaches a log line; the hash must not be reachable
        // through this type at all, so it cannot appear there.
        assert!(!format!("{operator:?}").contains("$2b$"));
    }
}
