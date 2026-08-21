//! The operator's PIN, and the policy that says what a valid one looks like.
//!
//! Nothing here is referenced yet; `auth-outcome-and-offline-lockout` is the first consumer.
//!
//! # Two protections, neither redundant
//!
//! [`Pin`] has a hand-written [`fmt::Debug`] that prints `Pin(****)`, and a [`Drop`] that
//! zeroizes its buffer. They defend different things and neither substitutes for the other:
//!
//! - The redacted `Debug` protects **logs**. `VerifyPinRequest` in `pos-api` carries the PIN as a
//!   `String` until this type is wired in, so it had to give up its derived `Debug` to stay safe —
//!   one `tracing` call on it would otherwise write a live PIN to a rotated log file on the till's
//!   disk. A `Pin` field needs no such abstinence, which is the point of the redaction.
//! - The zeroizing `Drop` protects **memory**. A freed `String` keeps its digits until something
//!   else reuses the allocation, where a core dump or a memory scrape can still read them.
//!
//! Deleting either one as "already covered" would remove a defence that was never covering it.
//!
//! # What this cannot protect
//!
//! [`Pin::parse`] takes a `&str`, so the caller's own buffer — the UI's text field — is outside
//! this type's reach and stays the caller's responsibility to clear. `Pin` owns exactly one
//! allocation, made at the size of the input so it never grows into a second one that `Drop`
//! would not see.

use std::fmt;
use std::time::Duration;

use thiserror::Error;
use zeroize::Zeroize;

// ============================================================================
// Errors
// ============================================================================

/// Why an entered PIN is not a PIN.
///
/// No variant carries the entered digits. An error value travels into logs and up through
/// `Display`, which is the one place a redacted `Debug` would not save it.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PinFormatError {
    /// The right characters, the wrong number of them. The count is not the content.
    #[error("a PIN must be {} digits, but {actual} were entered", expected.digits())]
    WrongLength {
        /// The length this tenant's policy requires.
        expected: PinLength,
        /// How many characters were entered.
        actual: usize,
    },

    /// Something other than an ASCII digit was entered.
    ///
    /// Arabic-Indic digits (`٠`–`٩`) land here too, and that is deliberate rather than
    /// incidental — see [`Pin::parse`].
    #[error("a PIN must contain only the digits 0 to 9")]
    NotNumeric,
}

/// A PIN policy value the till cannot work with.
///
/// Separate from [`PinFormatError`] because the audience is different: this is a misconfiguration
/// an administrator has to fix, not something a cashier can retype.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PinPolicyError {
    /// A PIN length outside the range the server's own validator accepts.
    #[error("a PIN length of {digits} is outside the supported range of 4 to 6 digits")]
    UnsupportedPinLength {
        /// The rejected length.
        digits: u8,
    },

    /// A retry budget of zero, which no operator could ever satisfy.
    #[error("a PIN retry budget of zero would lock out every operator on the first attempt")]
    NoAttemptsPermitted,

    /// `POS_CompanyConfiguration.maxOfflineHours` is a signed `Int`, so a negative value can
    /// reach the till even though nothing should write one.
    #[error("an offline window cannot be negative, but {hours} hours were configured")]
    NegativeOfflineWindow {
        /// The rejected value.
        hours: i32,
    },
}

// ============================================================================
// PinLength
// ============================================================================

/// How many digits this tenant's PINs have.
///
/// A closed enum rather than a number, which is the entire point: 3, 8 and 0 have no spelling
/// here, so no code path can produce one and no later validation has to catch it.
///
/// The till's range is currently the stricter of the two ends. `operator.validator.ts:30`
/// hardcodes `.min(4).max(6)` and `POS_CompanyConfiguration` carries no PIN policy fields at all
/// (checked 2026-08-22), so tenant-configurable length is still ahead of the platform — filed as
/// `e2manage/issue/pos-pin-policy-is-not-tenant-configuration`. Being stricter than the server is
/// the safe direction to be wrong in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PinLength {
    /// Four digits.
    Four,
    /// Five digits.
    Five,
    /// Six digits.
    Six,
}

impl PinLength {
    /// The number of digits this length requires.
    pub const fn digits(self) -> usize {
        match self {
            Self::Four => 4,
            Self::Five => 5,
            Self::Six => 6,
        }
    }
}

impl TryFrom<u8> for PinLength {
    type Error = PinPolicyError;

    fn try_from(digits: u8) -> Result<Self, Self::Error> {
        match digits {
            4 => Ok(Self::Four),
            5 => Ok(Self::Five),
            6 => Ok(Self::Six),
            other => Err(PinPolicyError::UnsupportedPinLength { digits: other }),
        }
    }
}

impl fmt::Display for PinLength {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} digits", self.digits())
    }
}

// ============================================================================
// MaxAttempts and OfflineWindow
// ============================================================================

/// How many consecutive wrong PINs an operator may enter before the account locks.
///
/// A newtype rather than a `u8` so it cannot be swapped with any other small number in a
/// signature, and so that zero — a budget nobody can satisfy — is unconstructible.
///
/// There is deliberately **no upper bound here.** PCI DSS v4.0 §8.3.4 caps invalid attempts at
/// ten, but enforcing that in this constructor would mean either rejecting a tenant's policy
/// outright (locking out their whole company over a misconfiguration) or clamping it silently
/// (hiding the misconfiguration from the only people who can fix it). Which of those is right is
/// `auth-outcome-and-offline-lockout`'s decision, made where it can be seen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MaxAttempts(u8);

impl MaxAttempts {
    /// Builds a retry budget, rejecting zero.
    pub const fn new(attempts: u8) -> Result<Self, PinPolicyError> {
        if attempts == 0 {
            Err(PinPolicyError::NoAttemptsPermitted)
        } else {
            Ok(Self(attempts))
        }
    }

    /// The budget as a count.
    pub const fn get(self) -> u8 {
        self.0
    }
}

impl fmt::Display for MaxAttempts {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// How long the till may authenticate operators against locally stored credentials before it must
/// reach the platform again.
///
/// Derived from `POS_CompanyConfiguration.maxOfflineHours` (`prisma/pos.prisma:234`, default 24).
/// Held as a [`Duration`], which cannot be negative — the column is a signed `Int`, and a
/// negative offline window would otherwise be a value the till had to keep re-checking.
///
/// Zero is valid and means offline authentication is not permitted at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct OfflineWindow(Duration);

impl OfflineWindow {
    /// Builds an offline window from the configured hour count, rejecting a negative one.
    pub fn from_hours(hours: i32) -> Result<Self, PinPolicyError> {
        u64::try_from(hours)
            .map(|hours| Self(Duration::from_secs(hours * 3600)))
            .map_err(|_| PinPolicyError::NegativeOfflineWindow { hours })
    }

    /// The window as a duration.
    pub const fn as_duration(self) -> Duration {
        self.0
    }

    /// The window in whole hours, as it was configured.
    pub const fn as_hours(self) -> u64 {
        self.0.as_secs() / 3600
    }
}

impl fmt::Display for OfflineWindow {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} hours", self.as_hours())
    }
}

// ============================================================================
// PinPolicy
// ============================================================================

/// The tenant's rules for PIN entry: how long a PIN is, how many tries it gets, and how long the
/// till may go on verifying them without the platform.
///
/// **Carried, never looked up.** The policy arrives with the terminal session at login, strictly
/// before any operator is selected, so a screen that holds a `PinPolicy` field cannot render PIN
/// entry without one. A global or a lazily-fetched policy would make "PIN entry rendered before
/// its rules were known" representable, and it is not.
///
/// There is deliberately no `Serialize`/`Deserialize`. Only [`OfflineWindow`] has a server-side
/// source today; `POS_CompanyConfiguration` carries no PIN length and no attempt budget, so a
/// wire form for this struct would be a shape this crate invented. The consuming issue assembles
/// a policy from the session and picks the till-side values at a call site where that choice is
/// visible.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PinPolicy {
    length: PinLength,
    max_attempts: MaxAttempts,
    offline_window: OfflineWindow,
}

impl PinPolicy {
    /// Assembles a policy from its three decided parts.
    pub const fn new(
        length: PinLength,
        max_attempts: MaxAttempts,
        offline_window: OfflineWindow,
    ) -> Self {
        Self {
            length,
            max_attempts,
            offline_window,
        }
    }

    /// How many digits this tenant's PINs have.
    pub const fn length(self) -> PinLength {
        self.length
    }

    /// How many wrong entries an operator gets before the account locks.
    pub const fn max_attempts(self) -> MaxAttempts {
        self.max_attempts
    }

    /// How long the till may verify locally before it must reach the platform.
    pub const fn offline_window(self) -> OfflineWindow {
        self.offline_window
    }
}

// ============================================================================
// Pin
// ============================================================================

/// An operator's PIN, in memory, for as short a time as possible.
///
/// The type has no `Clone`, no `PartialEq`, no `Serialize` and no `Display`, and each absence is
/// a decision:
///
/// - **No `Clone`** — a secret you can duplicate is a secret with more than one buffer to erase,
///   and only one of them would be this value's to erase.
/// - **No `PartialEq`** — comparing two PINs is not an operation this domain has. Verification is
///   against a bcrypt hash, and a derived `==` would be a byte-at-a-time comparison sitting
///   exactly where someone would reach for it.
/// - **No `Serialize`, no `Display`** — the only ways out are [`Pin::expose_digits`], which is
///   named so its call sites can be found, and the redacted [`fmt::Debug`].
pub struct Pin {
    digits: String,
}

impl Pin {
    /// Parses an entered PIN against the tenant's configured length.
    ///
    /// Takes a [`PinLength`] rather than a whole [`PinPolicy`]: length is all this needs, and a
    /// parameter is a socket that should admit exactly what the function consumes. Call it as
    /// `Pin::parse(entered, policy.length())`.
    ///
    /// **Only ASCII digits are accepted.** The server hashes what its own `/^\d+$/` validator
    /// admitted, and in JavaScript `\d` is ASCII-only, so a PIN entered as Arabic-Indic digits
    /// (`٤٥٦٧`) could never match the stored hash however it was rendered. Rejecting it here
    /// produces a retypeable error instead of a wrong-PIN attempt against the lockout counter.
    /// Whether an Arabic numpad should transliterate before it reaches this function is a
    /// question for the UI and for `auth-outcome-and-offline-lockout`; normalising silently
    /// inside a domain type would hide a decision about what the user actually typed.
    pub fn parse(raw: &str, length: PinLength) -> Result<Self, PinFormatError> {
        if !raw.chars().all(|c| c.is_ascii_digit()) {
            return Err(PinFormatError::NotNumeric);
        }

        let entered = raw.chars().count();
        if entered != length.digits() {
            return Err(PinFormatError::WrongLength {
                expected: length,
                actual: entered,
            });
        }

        // Sized to the input so the buffer never grows into a second allocation that `Drop`
        // would leave behind unzeroized.
        let mut digits = String::with_capacity(raw.len());
        digits.push_str(raw);
        Ok(Self { digits })
    }

    /// The digits, for the one caller that must hash them.
    ///
    /// Named to be conspicuous at a call site and in a grep. Every use is a place the PIN is in
    /// the clear; there should be exactly one, and it should be the bcrypt verification.
    pub fn expose_digits(&self) -> &str {
        &self.digits
    }

    /// How many digits this PIN has. Safe to log — it is the policy's length, not the secret.
    pub fn length(&self) -> usize {
        self.digits.len()
    }
}

impl fmt::Debug for Pin {
    /// Renders `Pin(****)`, with a fixed number of stars.
    ///
    /// The star count does not follow the PIN's length: a redaction that leaked the length would
    /// hand an attacker the search space for free, and a `Debug` that changed shape between
    /// tenants would be a worse thing to assert on.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Pin(****)")
    }
}

impl Drop for Pin {
    fn drop(&mut self) {
        self.digits.zeroize();
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::operator::OperatorId;

    fn policy() -> PinPolicy {
        PinPolicy::new(
            PinLength::Four,
            MaxAttempts::new(3).unwrap(),
            OfflineWindow::from_hours(24).unwrap(),
        )
    }

    #[test]
    fn pin_debug_is_redacted_in_every_formatting_mode() {
        let pin = Pin::parse("1234", policy().length()).unwrap();

        assert_eq!(format!("{pin:?}"), "Pin(****)");
        // The alternate flag is what `{:#?}` on an enclosing struct passes down.
        assert_eq!(format!("{pin:#?}"), "Pin(****)");
    }

    #[test]
    fn pin_stays_redacted_inside_a_derived_debug() {
        // The scenario this type exists for. `VerifyPinRequest` in `pos-api` holds the PIN as a
        // `String`, so it can only stay out of the logs by refusing the `Debug` derive. A `Pin`
        // field keeps the derive and still cannot leak.
        #[derive(Debug)]
        struct VerifyPin {
            operator_id: OperatorId,
            pin: Pin,
        }

        let request = VerifyPin {
            operator_id: OperatorId::new("op-1").unwrap(),
            pin: Pin::parse("1234", PinLength::Four).unwrap(),
        };
        let rendered = format!("{request:?}");

        assert!(rendered.contains("Pin(****)"), "got {rendered}");
        assert!(!rendered.contains("1234"), "got {rendered}");
        // The other fields still render normally — the redaction is the PIN's, not the struct's.
        assert!(
            rendered.contains(request.operator_id.as_str()),
            "got {rendered}"
        );
        assert_eq!(request.pin.length(), 4);
    }

    #[test]
    fn pin_zeroize_clears_the_buffer_it_is_given() {
        // `Pin::drop` calls exactly this. Observing `Pin`'s own drop would mean reading a freed
        // allocation, which needs `unsafe`; asserting the mechanism it depends on means a
        // dependency change that turned `String::zeroize` into a no-op fails here instead of
        // silently removing the protection.
        let mut secret = String::from("1234");
        secret.zeroize();

        assert!(secret.is_empty());
    }

    #[test]
    fn pin_parse_accepts_each_configured_length() {
        for (length, entered) in [
            (PinLength::Four, "1234"),
            (PinLength::Five, "12345"),
            (PinLength::Six, "123456"),
        ] {
            let pin = Pin::parse(entered, length).expect("a PIN of the configured length");
            assert_eq!(pin.expose_digits(), entered);
            assert_eq!(pin.length(), length.digits());
        }
    }

    #[test]
    fn pin_parse_rejects_a_pin_of_the_wrong_length() {
        // Asserted through `unwrap_err` rather than against `Err(..)`: `Pin` has no `PartialEq`,
        // so comparing two `Result<Pin, _>` values does not compile. That is the design working,
        // not an inconvenience to route around.
        for (entered, actual) in [("123", 3), ("12345", 5), ("", 0)] {
            assert_eq!(
                Pin::parse(entered, PinLength::Four).unwrap_err(),
                PinFormatError::WrongLength {
                    expected: PinLength::Four,
                    actual,
                },
                "`{entered}` must not parse as a four-digit PIN"
            );
        }
    }

    #[test]
    fn pin_parse_rejects_arabic_indic_digits() {
        // Arabic is the till's default locale (`config/default.toml`), so this is reachable, not
        // theoretical. The server hashed what its own ASCII-only `/^\d+$/` admitted, so these
        // digits could never match the stored hash — refusing them costs the operator a retype
        // instead of an attempt against the lockout counter.
        assert_eq!(
            Pin::parse("١٢٣٤", PinLength::Four).unwrap_err(),
            PinFormatError::NotNumeric
        );
    }

    #[test]
    fn pin_parse_rejects_anything_that_is_not_a_digit() {
        for entered in ["12a4", "12 4", "12.4", "abcd", "١٢٣٤"] {
            assert_eq!(
                Pin::parse(entered, PinLength::Four).unwrap_err(),
                PinFormatError::NotNumeric,
                "`{entered}` must not parse"
            );
        }
    }

    #[test]
    fn pin_format_errors_never_echo_the_entered_digits() {
        let too_short = Pin::parse("13", PinLength::Four).unwrap_err();
        let not_numeric = Pin::parse("13a4", PinLength::Four).unwrap_err();

        for message in [too_short.to_string(), not_numeric.to_string()] {
            assert!(!message.contains("13"), "an error leaked digits: {message}");
        }
        // The length *is* safe to report — a count is not the content.
        assert!(too_short.to_string().contains("but 2 were entered"));
    }

    #[test]
    fn pin_length_rejects_a_length_the_platform_does_not_support() {
        assert_eq!(PinLength::try_from(4), Ok(PinLength::Four));
        assert_eq!(PinLength::try_from(6), Ok(PinLength::Six));
        for unsupported in [0_u8, 1, 3, 7, 8, 255] {
            assert_eq!(
                PinLength::try_from(unsupported),
                Err(PinPolicyError::UnsupportedPinLength {
                    digits: unsupported
                })
            );
        }
    }

    #[test]
    fn pin_policy_rejects_a_retry_budget_of_zero() {
        assert_eq!(
            MaxAttempts::new(0),
            Err(PinPolicyError::NoAttemptsPermitted)
        );
        assert_eq!(MaxAttempts::new(1).unwrap().get(), 1);
    }

    #[test]
    fn pin_policy_rejects_a_negative_offline_window() {
        assert_eq!(
            OfflineWindow::from_hours(-1),
            Err(PinPolicyError::NegativeOfflineWindow { hours: -1 })
        );
    }

    #[test]
    fn pin_policy_offline_window_round_trips_its_configured_hours() {
        // `POS_CompanyConfiguration.maxOfflineHours` defaults to 24.
        let window = OfflineWindow::from_hours(24).unwrap();
        assert_eq!(window.as_hours(), 24);
        assert_eq!(window.as_duration(), Duration::from_secs(86_400));

        // Zero is a real configuration: offline authentication is not permitted at all.
        assert_eq!(OfflineWindow::from_hours(0).unwrap().as_hours(), 0);
    }

    #[test]
    fn pin_policy_carries_its_three_parts() {
        let policy = policy();

        assert_eq!(policy.length(), PinLength::Four);
        assert_eq!(policy.max_attempts().get(), 3);
        assert_eq!(policy.offline_window().as_hours(), 24);
    }
}
