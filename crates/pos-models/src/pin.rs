//! The operator's PIN, and the policy that says what a valid one looks like.
//!
//! `AuthService::verify_pin` carries a [`PinPolicy`] as a parameter — the policy arrives with the
//! terminal session at login, strictly before an operator is selected — and takes a [`Pin`] that
//! [`Pin::parse`] made. Nothing in this repository builds a [`RequiredPinLength`] from a terminal
//! configuration yet: the till has no PIN-entry screen here (it belongs to `egui-auth-screen`), so
//! the only [`PinPolicy::new`] calls outside these tests are in the till's own test fixtures.
//!
//! # A credential policy governs minting, not presentation
//!
//! [`Pin::parse`] takes **no policy**, and that absence is the design. Enforcing a tenant's
//! current length at the moment a PIN is *presented* locks out every operator in the company as
//! soon as an administrator presses Save — every standing PIN was minted under the old rule, and
//! a refusal goes through the failed-attempt counter. The platform learned this and moved
//! enforcement to the minting doors; the till has no minting door, so it has nothing to enforce.
//! Rotation is a verdict the server reaches *after* bcrypt accepts the PIN, and it arrives as
//! `POS_PIN_ROTATION_REQUIRED`. It is never derived here.
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
    ///
    /// The bounds are the **platform's** — 4 to 6 — never a tenant's configured length. A PIN of
    /// a legal length that is not the length the tenant now requires is a correct PIN that needs
    /// rotating, and only the server can say so: see the module header.
    #[error(
        "a PIN must be between {} and {} digits, but {actual} were entered",
        PinLength::SHORTEST.digits(),
        PinLength::LONGEST.digits()
    )]
    LengthOutOfRange {
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

    /// `POS_CompanyConfiguration.pinLockoutMinutes` is a signed `Int`. Zero is legal here — a
    /// lockout with no advertised end is still a lockout — but a negative period is not a
    /// duration at all.
    #[error("a lockout period cannot be negative, but {minutes} minutes were configured")]
    NegativeLockoutPeriod {
        /// The rejected value.
        minutes: i32,
    },

    /// A session lifetime of zero or less. Unlike the lockout period, zero is refused: a session
    /// that has expired at the instant it is minted is not a session, and a till holding one
    /// would re-authenticate in a loop.
    #[error("an operator session must live at least an hour, but {hours} were configured")]
    UnusableSessionLifetime {
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
/// The set is the **platform's**, and both sides now agree on it: `OPERATOR_PIN_LENGTHS` is
/// `[4, 5, 6] as const` (`pos/domain/policies/operator-pin.policy.ts`), and
/// `POS_CompanyConfiguration.pinLength` is a nullable `Int` under a CHECK constraint from
/// `20260822020000_pos_pin_policy`. This enum is that tuple.
///
/// What a *tenant* requires is [`RequiredPinLength`], which has an arm this enum deliberately
/// does not: "no particular length" is a rule, not a length.
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
    /// The shortest length the platform accepts.
    pub const SHORTEST: Self = Self::Four;

    /// The longest length the platform accepts, and therefore the strictest requirement a tenant
    /// can express. See [`RequiredPinLength::read`] for why "strictest" is the interesting one.
    pub const LONGEST: Self = Self::Six;

    /// The number of digits this length requires.
    pub const fn digits(self) -> usize {
        match self {
            Self::Four => 4,
            Self::Five => 5,
            Self::Six => 6,
        }
    }
}

// ============================================================================
// RequiredPinLength
// ============================================================================

/// How long a PIN this tenant requires — and *"no answer"* is one of the answers.
///
/// Mirrors the platform's own `RequiredPinLength`, and for the platform's own reason: the wire
/// carries `pinLength: number | null`, where **`null` means any platform-legal length is
/// acceptable**. That is the state every tenant on this deploy is in, because the migration added
/// six nullable columns and ran no `UPDATE`.
///
/// A `Option<PinLength>` would leave every reader to rediscover what the absence means, and the
/// first one to read it as "not configured yet, use six" would refuse every four-digit PIN in the
/// company. The platform's note on its own type says it plainly, and it is worth repeating on this
/// side of the wire: *"Defaulting it to 4 or 6 client-side reintroduces this defect in the other
/// direction — the till would refuse PINs the server mints happily."*
///
/// Nothing in the till enforces this rule at PIN entry. See the module header: the till has no
/// minting door. It is carried so a screen can say what a *new* PIN must look like, and so a
/// rotation prompt can name the length to rotate to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RequiredPinLength {
    /// The tenant has expressed no length rule, so every length the platform accepts satisfies
    /// them. Not "unknown" — an answer.
    AnyPlatformLength,
    /// The tenant requires exactly this many digits of a new PIN.
    Exactly(PinLength),
}

impl RequiredPinLength {
    /// Reads the rule a terminal configuration carries.
    ///
    /// `None` is the tenant saying nothing, and becomes [`Self::AnyPlatformLength`].
    ///
    /// A number the platform does not accept is a misconfiguration, and the answer is
    /// **[`PinLength::LONGEST`], never [`Self::AnyPlatformLength`]**. The two candidates are not
    /// symmetric, and the platform's resolver spells out why: resolving an uninterpretable value
    /// to "no requirement" is fail-open on the exact control the rule exists to add — the tenant
    /// asked for something, it could not be read, and the outcome is that nothing is required of
    /// anybody. Resolving it to six costs a rotation prompt, which is loud and recoverable.
    ///
    /// The fallback travels in the `Err`, carrying the rejected value, because a caller that
    /// silently used it would be logging nothing about a misconfigured tenant.
    /// [`UninterpretablePinLength::resolved`] is what to enforce meanwhile.
    ///
    /// **A read that *failed* is not this function's business.** A configuration the till could
    /// not fetch or could not parse propagates as an `ApiFailure`; degrading it to a policy would
    /// turn an outage into a silent relaxation at the moment the platform is unhealthy — the
    /// distinction the platform's resolver makes between a value it can clamp and a query it
    /// cannot run.
    pub fn read(stored: Option<i64>) -> Result<Self, UninterpretablePinLength> {
        let Some(stored) = stored else {
            return Ok(Self::AnyPlatformLength);
        };

        u8::try_from(stored)
            .ok()
            .and_then(|digits| PinLength::try_from(digits).ok())
            .map(Self::Exactly)
            .ok_or(UninterpretablePinLength { stored })
    }

    /// The length a new PIN must have, or `None` when the tenant requires no particular one.
    ///
    /// The inverse of the platform's `toWireLength`.
    pub const fn as_exact(self) -> Option<PinLength> {
        match self {
            Self::AnyPlatformLength => None,
            Self::Exactly(length) => Some(length),
        }
    }
}

impl fmt::Display for RequiredPinLength {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AnyPlatformLength => f.write_str("any supported length"),
            Self::Exactly(length) => write!(f, "{length}"),
        }
    }
}

/// A configured PIN length the platform does not accept, and what to do about it.
///
/// Not a [`PinPolicyError`]: those are refusals. This one carries a usable answer, because
/// refusing to have a length rule at all is the fail-open outcome
/// [`RequiredPinLength::read`] exists to avoid. It is an error the caller must **log**, not one
/// that stops it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[error(
    "a configured PIN length of {stored} is not one the platform accepts; using the strictest \
     legal length instead, because requiring nothing would remove the control silently"
)]
pub struct UninterpretablePinLength {
    /// The value the configuration carried.
    pub stored: i64,
}

impl UninterpretablePinLength {
    /// The rule to enforce in place of the unreadable one: the strictest legal length.
    pub const fn resolved(self) -> RequiredPinLength {
        RequiredPinLength::Exactly(PinLength::LONGEST)
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
/// There is deliberately **no upper bound here**, and this is the answer to the question this
/// type was left holding. PCI DSS v4.0 §8.3.4 caps invalid attempts at ten, and the *platform*
/// enforces that: `OPERATOR_PIN_POLICY_BOUNDS.pinMaxFailedAttempts` is `{min: 3, max: 10}`, a
/// CHECK constraint from `20260822020000_pos_pin_policy` refuses anything else, and the resolver
/// clamps with a warning for the one row that can predate the constraint. So a value reaching the
/// till has already been through both.
///
/// Re-enforcing it here would be the till disagreeing with the authority that decided it, and the
/// two ways to disagree are the two failure modes named when this question was posed: rejecting
/// locks out a company over a misconfiguration, clamping hides it from the only people who can
/// fix it. Neither is the till's to do. Zero is still refused, because zero is not a
/// misconfigured budget — it is no budget, and no arithmetic on it produces a survivable attempt.
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

/// How long a lockout stands, **as the tenant advertises it**.
///
/// Derived from `POS_CompanyConfiguration.pinLockoutMinutes` (default 30). Zero is legal and means
/// the tenant advertises no end.
///
/// # This is a sentence to render and a figure to report. It is not a timer.
///
/// The till never ends a lockout from a duration it holds. PCI DSS v4.0 §8.3.4 permits a lockout
/// to end after thirty minutes **or** when the user's identity is confirmed, and the till takes
/// the second branch, because the first is measured on the clock of whoever is holding the device.
/// `LockState` has no expiry field for the same reason, and `pos_api::LockoutNotice` carries the
/// same warning about the instant the server sends beside a refusal.
///
/// So the accessor is named for its use, and neither `PartialOrd` nor `Ord` is derived: the
/// comparison that would turn this into an unlock condition does not compile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LockoutPeriod(Duration);

impl LockoutPeriod {
    /// Builds a lockout period from the configured minute count, rejecting a negative one.
    pub fn from_minutes(minutes: i32) -> Result<Self, PinPolicyError> {
        u64::try_from(minutes)
            .map(|minutes| Self(Duration::from_secs(minutes * 60)))
            .map_err(|_| PinPolicyError::NegativeLockoutPeriod { minutes })
    }

    /// The period in whole minutes, to put in front of a person or into a report.
    pub const fn minutes_to_state(self) -> u64 {
        self.0.as_secs() / 60
    }
}

impl fmt::Display for LockoutPeriod {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} minutes", self.minutes_to_state())
    }
}

/// How long a verified operator session lives.
///
/// Derived from `POS_CompanyConfiguration.operatorSessionHours` (default 12).
///
/// Unlike [`LockoutPeriod`] this **is** a duration the till may act on, and the asymmetry is the
/// same one that separates `CredentialExpiry` from a lockout: acting early on a session lifetime
/// fails **closed** — the till re-authenticates sooner than it had to. The server still decides
/// the real answer and says `POS_OPERATOR_SESSION_EXPIRED`; this only lets the till stop sending
/// a credential it already knows is spent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SessionLifetime(Duration);

impl SessionLifetime {
    /// Builds a session lifetime from the configured hour count, rejecting zero and below.
    pub fn from_hours(hours: i32) -> Result<Self, PinPolicyError> {
        match u64::try_from(hours) {
            Ok(hours) if hours > 0 => Ok(Self(Duration::from_secs(hours * 3600))),
            _ => Err(PinPolicyError::UnusableSessionLifetime { hours }),
        }
    }

    /// The lifetime as a duration.
    pub const fn as_duration(self) -> Duration {
        self.0
    }

    /// The lifetime in whole hours, as it was configured.
    pub const fn as_hours(self) -> u64 {
        self.0.as_secs() / 3600
    }
}

impl fmt::Display for SessionLifetime {
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
/// There is deliberately no `Serialize`/`Deserialize`. The wire form is `TerminalPinPolicyDto`
/// (`pos/application/dto/terminal-pin-policy.dto.ts`), whose four fields are
/// `{pinLength, maxFailedAttempts, lockoutMinutes, sessionHours}` — a shape that is the
/// platform's to change. `pos-api` reads that DTO and assembles this at the boundary, where the
/// conversion from `pinLength: number | null` to a [`RequiredPinLength`] is visible.
///
/// [`OfflineWindow`] has no counterpart in that DTO: it comes from
/// `POS_CompanyConfiguration.maxOfflineHours`, which travels with the company configuration
/// rather than with the PIN policy. Five knobs, two sources — worth knowing before assuming one
/// payload carries them all.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PinPolicy {
    length: RequiredPinLength,
    max_attempts: MaxAttempts,
    lockout_period: LockoutPeriod,
    session_lifetime: SessionLifetime,
    offline_window: OfflineWindow,
}

impl PinPolicy {
    /// Assembles a policy from its five decided parts.
    pub const fn new(
        length: RequiredPinLength,
        max_attempts: MaxAttempts,
        lockout_period: LockoutPeriod,
        session_lifetime: SessionLifetime,
        offline_window: OfflineWindow,
    ) -> Self {
        Self {
            length,
            max_attempts,
            lockout_period,
            session_lifetime,
            offline_window,
        }
    }

    /// What this tenant requires of a **new** PIN.
    ///
    /// Never consulted when a PIN is presented — see the module header, and note that
    /// [`Pin::parse`] does not take one.
    pub const fn length(self) -> RequiredPinLength {
        self.length
    }

    /// How many wrong entries an operator gets before the account locks.
    pub const fn max_attempts(self) -> MaxAttempts {
        self.max_attempts
    }

    /// How long a lockout stands, to say and to report. **Not an unlock condition** — see
    /// [`LockoutPeriod`].
    pub const fn lockout_period(self) -> LockoutPeriod {
        self.lockout_period
    }

    /// How long a verified operator session lives.
    pub const fn session_lifetime(self) -> SessionLifetime {
        self.session_lifetime
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
    /// Parses an entered PIN for **shape**: ASCII digits, and a count the platform accepts.
    ///
    /// # It takes no policy, and that is the point
    ///
    /// The earlier signature was `parse(raw, length)`, called as
    /// `Pin::parse(entered, policy.length())`, and it reimplemented inside the till the defect the
    /// platform had already paid for: a tenant's current length checked at the moment a PIN is
    /// *presented* locks out every operator in the company the instant an administrator presses
    /// Save. Every standing PIN was minted under the old rule, and each refusal spends an attempt.
    ///
    /// A credential policy governs **minting**, and the till never mints a PIN. Whether the PIN
    /// that was just proved correct is now the wrong length is a verdict only the server can
    /// reach — after bcrypt accepts it, never before, because deciding it earlier is a free oracle
    /// on the required length. It arrives as `POS_PIN_ROTATION_REQUIRED` carrying the length to
    /// rotate to. See [`RequiredPinLength`] for the rule this function deliberately does not read.
    ///
    /// What remains is the shape the platform itself will not accept from anybody: 4 to 6 ASCII
    /// digits. That is a fact about the API, not about a tenant, so it belongs here.
    ///
    /// **Only ASCII digits are accepted.** The server hashes what its own `/^\d+$/` validator
    /// admitted, and in JavaScript `\d` is ASCII-only, so a PIN entered as Arabic-Indic digits
    /// (`٤٥٦٧`) could never match the stored hash however it was rendered. Rejecting it here
    /// produces a retypeable error instead of a wrong-PIN attempt against the lockout counter.
    /// Whether an Arabic numpad should transliterate before it reaches this function is a
    /// question for the UI; normalising silently inside a domain type would hide a decision about
    /// what the user actually typed.
    pub fn parse(raw: &str) -> Result<Self, PinFormatError> {
        if !raw.chars().all(|c| c.is_ascii_digit()) {
            return Err(PinFormatError::NotNumeric);
        }

        // Counted in `char`s, not bytes. Every ASCII digit is one byte, so the two agree here —
        // but the check above is what makes that true, and reading `raw.len()` would silently
        // stop being right the day someone relaxes it.
        let entered = raw.chars().count();
        if !(PinLength::SHORTEST.digits()..=PinLength::LONGEST.digits()).contains(&entered) {
            return Err(PinFormatError::LengthOutOfRange { actual: entered });
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
// Digit and EnteredDigits
// ============================================================================

/// One decimal digit, 0 to 9.
///
/// A socket that accepts a `u8` accepts 200, and the buffer below would then have to answer what
/// it does with one. It has nothing sensible to answer, so the question is removed instead: the
/// only way to obtain a `Digit` is through [`Self::new`], which refuses everything else.
///
/// It carries a **value, never a character**, and that is the whole point of its existing here
/// rather than the buffer taking a `char`. [`Pin`] is ASCII-digits-only because the platform's
/// own validator is `/^\d+$/` and JavaScript's `\d` is ASCII, so an Arabic-Indic digit could
/// never match the stored hash. A pad that renders `٤` and reports `Digit(4)` is therefore
/// correct in both scripts at once, and the rendering never reaches the credential.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Digit(u8);

impl Digit {
    /// Reads a decimal digit. `None` for anything above nine.
    pub const fn new(value: u8) -> Option<Self> {
        match value {
            0..=9 => Some(Self(value)),
            _ => None,
        }
    }

    /// The value, 0 to 9.
    pub const fn value(self) -> u8 {
        self.0
    }

    /// The ASCII byte for this digit. Always in `b'0'..=b'9'`, which is what lets
    /// [`EnteredDigits`] hold its buffer as bytes and read it back as a `&str` without allocating
    /// a second copy of the secret.
    const fn as_ascii(self) -> u8 {
        b'0' + self.0
    }
}

/// The digits a cashier has typed so far, on the way to becoming a [`Pin`].
///
/// # Why this is a type and not a `String` on the screen
///
/// `tests/guards.rs::a_live_pin_never_reaches_a_derived_debug` refuses a `Debug`-deriving type
/// holding a field **literally named `pin`**, word-bounded. A screen's entry buffer is not called
/// `pin` — it is `entered`, or `digits`, or `buffer` — so the guard could not see it, while the
/// phase enum holding it derives `Debug` like every domain type here and the driver consuming it
/// is a match statement somebody will log. One `tracing::debug!("{phase:?}")` would put a
/// cashier's live PIN in a rotating file on the till's disk.
///
/// So the buffer is a domain type carrying [`Pin`]'s two protections, which defend different
/// things and do not substitute for one another: a hand-written [`fmt::Debug`] that protects
/// **logs**, and a zeroizing [`Drop`] that protects **memory**.
///
/// # The buffer never grows, and that is load-bearing
///
/// `zeroize`'s own documentation is explicit that it "cannot ensure that previous reallocations
/// did not leave values on the heap". A buffer that reallocated while being typed into would
/// leave a copy of the earlier prefix behind, unreachable and unzeroized, and no `Drop` could
/// find it. So the allocation is made once at [`PinLength::LONGEST`] and a press beyond that is
/// ignored rather than grown into.
///
/// Ignoring is the right answer rather than an error, and it matches the pad: the component
/// library's PIN keypad suppresses a press when the caller says it is at capacity and reports
/// nothing happened. A keypress against a full buffer is an ordinary event at a till, not a fault
/// anybody can act on.
pub struct EnteredDigits {
    /// ASCII bytes, always `b'0'..=b'9'` by construction — see [`Digit::as_ascii`].
    digits: Vec<u8>,
}

impl EnteredDigits {
    /// An empty buffer, allocated once at the largest PIN the platform accepts.
    pub fn empty() -> Self {
        Self {
            digits: Vec::with_capacity(PinLength::LONGEST.digits()),
        }
    }

    /// Appends a digit, or does nothing if the buffer is already at the platform's maximum.
    ///
    /// Total on purpose. See the type docs for why a press at capacity is ignored rather than
    /// refused, and why the buffer must not grow.
    pub fn push(&mut self, digit: Digit) {
        if self.digits.len() < PinLength::LONGEST.digits() {
            self.digits.push(digit.as_ascii());
        }
    }

    /// Removes the last digit, zeroing the byte it leaves behind.
    ///
    /// The zeroing is not incidental: `Vec::pop` only moves the length, so without it the digit
    /// stays legible in the allocation for as long as the buffer lives — and a cashier correcting
    /// a mistype is the single most common way a digit gets abandoned mid-entry.
    ///
    /// **What is guaranteed by what.** That the freed slot is reachable and cleared is `zeroize`'s
    /// contract for a `Vec`'s spare capacity, not something this module can observe: reading it
    /// back would mean reading uninitialised memory, which needs `unsafe`. What *is* checked here
    /// is that the mechanism still behaves — see `entered_digits_zeroizing_is_not_a_no_op` — on
    /// the same reasoning as [`Pin`]'s own zeroize test: a dependency change that turned it into a
    /// no-op fails a test rather than silently removing the protection.
    pub fn backspace(&mut self) {
        self.digits.pop();
        self.digits.spare_capacity_mut().zeroize();
    }

    /// Empties the buffer, zeroing every byte including the spare capacity.
    ///
    /// `Vec::clear` alone would only move the length. This is the clear a cashier expects when
    /// they hit the clear key having mistyped, and it must actually destroy what they typed.
    pub fn clear(&mut self) {
        self.digits.zeroize();
    }

    /// How many digits have been entered. Safe to show — it is the fill count, not the secret.
    pub fn len(&self) -> usize {
        self.digits.len()
    }

    /// Whether nothing has been entered yet.
    pub fn is_empty(&self) -> bool {
        self.digits.is_empty()
    }

    /// Builds the [`Pin`], once, at submit.
    ///
    /// The only door from typed digits to a credential. `Pin::parse` re-checks the shape rather
    /// than trusting this type's invariant, which is deliberate: the length rule lives there
    /// because it is a fact about the platform's API, and having one place answer "is this a
    /// PIN-shaped thing" is worth the re-scan of at most six bytes.
    pub fn finish(&self) -> Result<Pin, PinFormatError> {
        // Infallible in practice — every byte came from `Digit::as_ascii` — but written as a
        // total match rather than an `expect`, because a panic here would take down a till in
        // front of a customer to report an invariant this module already guarantees.
        match std::str::from_utf8(&self.digits) {
            Ok(entered) => Pin::parse(entered),
            Err(_) => Err(PinFormatError::NotNumeric),
        }
    }
}

impl fmt::Debug for EnteredDigits {
    /// Renders `EnteredDigits(****)`, with a fixed number of stars.
    ///
    /// Fixed for the reason [`Pin`]'s is: a redaction that tracked the length would leak how much
    /// has been typed, and here that leaks it *continuously*, one frame at a time, to anything
    /// logging the phase.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("EnteredDigits(****)")
    }
}

impl Drop for EnteredDigits {
    fn drop(&mut self) {
        self.digits.zeroize();
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {

    // ========================================================================
    // EnteredDigits
    // ========================================================================

    fn digit(value: u8) -> Digit {
        Digit::new(value).expect("the fixtures use real digits")
    }

    fn entered(values: &[u8]) -> EnteredDigits {
        let mut buffer = EnteredDigits::empty();
        for value in values {
            buffer.push(digit(*value));
        }
        buffer
    }

    #[test]
    fn a_digit_is_zero_to_nine_and_nothing_else() {
        for value in 0..=9u8 {
            assert_eq!(
                Digit::new(value).map(Digit::value),
                Some(value),
                "{value} is a decimal digit"
            );
        }
        for value in [10u8, 11, 99, 200, u8::MAX] {
            assert!(
                Digit::new(value).is_none(),
                "{value} is not a decimal digit and must not be constructible"
            );
        }
    }

    #[test]
    fn entered_digits_never_render_what_was_typed() {
        // The assertion the guard exists to back up. Asserted literally, and on a NON-EMPTY
        // buffer: a redaction that only holds for the empty case is the one that matters least.
        let buffer = entered(&[4, 5, 6, 7]);
        assert_eq!(format!("{buffer:?}"), "EnteredDigits(****)");

        // And the star count does not track the length, or the Debug leaks the search space one
        // frame at a time to anything logging the phase.
        assert_eq!(
            format!("{:?}", entered(&[1, 2, 3, 4, 5, 6])),
            format!("{:?}", entered(&[1, 2, 3, 4])),
            "the redaction must not vary with how much has been typed"
        );
    }

    #[test]
    fn entered_digits_stop_at_the_platform_maximum_rather_than_growing() {
        let mut buffer = EnteredDigits::empty();
        for value in [1u8, 2, 3, 4, 5, 6, 7, 8, 9] {
            buffer.push(digit(value));
        }

        assert_eq!(
            buffer.len(),
            PinLength::LONGEST.digits(),
            "presses past the maximum must be ignored, not appended"
        );

        // The reason the ceiling exists at all: `zeroize` cannot reach a buffer that was
        // reallocated out from under it, so growth would strand a copy of the earlier prefix.
        // A capacity that never moves is what makes the zeroizing Drop honest.
        assert_eq!(
            buffer.finish().map(|pin| pin.length()).ok(),
            Some(PinLength::LONGEST.digits()),
            "the retained digits are the first six, and they still form a PIN"
        );
    }

    #[test]
    fn entered_digits_backspace_and_clear_take_the_digits_with_them() {
        let mut buffer = entered(&[1, 2, 3, 4, 5]);
        buffer.backspace();
        assert_eq!(buffer.len(), 4);
        assert_eq!(
            buffer
                .finish()
                .expect("four digits is a PIN")
                .expose_digits(),
            "1234",
            "backspace removes the last digit and leaves the rest in order"
        );

        buffer.clear();
        assert!(buffer.is_empty(), "clear empties the buffer");
        assert!(
            matches!(
                buffer.finish(),
                Err(PinFormatError::LengthOutOfRange { actual: 0 })
            ),
            "an emptied buffer is not a PIN"
        );

        // Backspace on an empty buffer is a normal keypress at a till, not a fault.
        buffer.backspace();
        assert!(buffer.is_empty());
    }

    #[test]
    fn entered_digits_become_a_pin_only_at_a_platform_legal_length() {
        for values in [
            vec![1u8, 2, 3, 4],
            vec![1, 2, 3, 4, 5],
            vec![1, 2, 3, 4, 5, 6],
        ] {
            let buffer = entered(&values);
            let pin = buffer
                .finish()
                .unwrap_or_else(|error| panic!("{} digits must parse: {error}", values.len()));
            assert_eq!(pin.length(), values.len());
        }

        for values in [vec![], vec![1u8], vec![1, 2], vec![1, 2, 3]] {
            let short = entered(&values);
            assert!(
                matches!(short.finish(), Err(PinFormatError::LengthOutOfRange { .. })),
                "{} digits is below the platform minimum and must not become a PIN",
                values.len()
            );
        }
    }

    #[test]
    fn entered_digits_zeroizing_is_not_a_no_op() {
        // `clear` and `drop` call exactly this, and `backspace` calls its spare-capacity
        // sibling. Observing either through `EnteredDigits` would mean reading a freed or
        // uninitialised allocation, which needs `unsafe` — so the mechanism is asserted instead,
        // the same way `pin_zeroize_clears_the_buffer_it_is_given` does. A dependency bump that
        // turned this into a no-op fails here rather than quietly removing the protection.
        let mut secret = vec![b'1', b'2', b'3', b'4'];
        secret.zeroize();
        assert!(
            secret.is_empty(),
            "zeroize must clear the vector it is given"
        );

        // And the spare-capacity path backspace depends on. Its *existence* is enforced by
        // compilation — `EnteredDigits::backspace` would not build without the impl — so what is
        // worth asserting is that calling it leaves a usable buffer rather than a poisoned one.
        let mut buffer = entered(&[9, 8, 7]);
        buffer.backspace();
        buffer.push(digit(1));
        assert_eq!(
            buffer.finish().map(|pin| pin.expose_digits().to_string()),
            Err(PinFormatError::LengthOutOfRange { actual: 3 }),
            "three digits is still short of a PIN, and the buffer survived the zeroize intact"
        );
    }

    #[test]
    fn entered_digits_carry_values_so_the_rendering_script_never_reaches_the_credential() {
        // The Arabic-Indic trap, pinned rather than trusted. A pad rendering `٤٥٦٧` reports the
        // same four Digit values as one rendering `4567`, so the credential is identical and the
        // locale cannot change what is hashed. If `push` ever took a `char`, this stops holding.
        assert_eq!(
            entered(&[4, 5, 6, 7])
                .finish()
                .expect("four digits")
                .expose_digits(),
            "4567"
        );
    }
    use super::*;
    use crate::operator::OperatorId;

    #[test]
    fn pin_debug_is_redacted_in_every_formatting_mode() {
        let pin = Pin::parse("1234").unwrap();

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
            pin: Pin::parse("1234").unwrap(),
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
    fn pin_parse_accepts_every_length_the_platform_accepts() {
        for (length, entered) in [
            (PinLength::Four, "1234"),
            (PinLength::Five, "12345"),
            (PinLength::Six, "123456"),
        ] {
            let pin = Pin::parse(entered).expect("a PIN of a platform-legal length");
            assert_eq!(pin.expose_digits(), entered);
            assert_eq!(pin.length(), length.digits());
        }
    }

    /// The regression this task exists to prevent, stated as a test.
    ///
    /// A five-digit PIN minted before the tenant required six still parses. Refusing it here is
    /// how a company loses every till the moment an administrator presses Save: the refusal goes
    /// through the failed-attempt counter, and every standing PIN was minted under the old rule.
    #[test]
    fn pin_parse_does_not_enforce_a_tenants_length() {
        let requiring_six = PinPolicy::new(
            RequiredPinLength::Exactly(PinLength::Six),
            MaxAttempts::new(3).unwrap(),
            LockoutPeriod::from_minutes(30).unwrap(),
            SessionLifetime::from_hours(12).unwrap(),
            OfflineWindow::from_hours(24).unwrap(),
        );
        assert_eq!(
            requiring_six.length(),
            RequiredPinLength::Exactly(PinLength::Six)
        );

        // The policy is right there and the parse does not consult it, because it cannot: the
        // signature has no socket for one.
        let older = Pin::parse("12345").expect("a five-digit PIN minted under the old rule");

        assert_eq!(older.length(), 5);
    }

    #[test]
    fn pin_parse_rejects_a_length_outside_the_platform_range() {
        // Asserted through `unwrap_err` rather than against `Err(..)`: `Pin` has no `PartialEq`,
        // so comparing two `Result<Pin, _>` values does not compile. That is the design working,
        // not an inconvenience to route around.
        for (entered, actual) in [("123", 3), ("1234567", 7), ("", 0)] {
            assert_eq!(
                Pin::parse(entered).unwrap_err(),
                PinFormatError::LengthOutOfRange { actual },
                "`{entered}` is not a length the platform accepts"
            );
        }
    }

    #[test]
    fn pin_parse_rejects_arabic_indic_digits() {
        // Arabic is the till's default locale (`config/default.toml`), so this is reachable, not
        // theoretical. The server hashed what its own ASCII-only `/^\d+$/` admitted, so these
        // digits could never match the stored hash — refusing them costs the operator a retype
        // instead of an attempt against the lockout counter.
        assert_eq!(Pin::parse("١٢٣٤").unwrap_err(), PinFormatError::NotNumeric);
    }

    #[test]
    fn pin_parse_rejects_anything_that_is_not_a_digit() {
        for entered in ["12a4", "12 4", "12.4", "abcd", "١٢٣٤"] {
            assert_eq!(
                Pin::parse(entered).unwrap_err(),
                PinFormatError::NotNumeric,
                "`{entered}` must not parse"
            );
        }
    }

    #[test]
    fn pin_format_errors_never_echo_the_entered_digits() {
        let too_short = Pin::parse("13").unwrap_err();
        let not_numeric = Pin::parse("13a4").unwrap_err();

        for message in [too_short.to_string(), not_numeric.to_string()] {
            assert!(!message.contains("13"), "an error leaked digits: {message}");
        }
        // The length *is* safe to report — a count is not the content.
        assert!(too_short.to_string().contains("but 2 were entered"));
        assert!(too_short.to_string().contains("between 4 and 6 digits"));
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
    fn pin_policy_carries_all_five_of_its_parts() {
        // Distinct values per knob: three of the five are small counts, and a policy that wired
        // two of them to the same accessor would read as correct against matching numbers.
        let policy = PinPolicy::new(
            RequiredPinLength::Exactly(PinLength::Five),
            MaxAttempts::new(3).unwrap(),
            LockoutPeriod::from_minutes(45).unwrap(),
            SessionLifetime::from_hours(8).unwrap(),
            OfflineWindow::from_hours(24).unwrap(),
        );

        assert_eq!(policy.length(), RequiredPinLength::Exactly(PinLength::Five));
        assert_eq!(policy.max_attempts().get(), 3);
        assert_eq!(policy.lockout_period().minutes_to_state(), 45);
        assert_eq!(policy.session_lifetime().as_hours(), 8);
        assert_eq!(policy.offline_window().as_hours(), 24);
    }

    /// `null` means *any platform-legal length*, and it is the state every tenant on this deploy
    /// is in. Reading it as "not configured yet, use six" refuses every four-digit PIN in the
    /// company.
    #[test]
    fn an_absent_length_rule_is_an_answer_and_not_a_missing_value() {
        assert_eq!(
            RequiredPinLength::read(None),
            Ok(RequiredPinLength::AnyPlatformLength)
        );
        assert_eq!(RequiredPinLength::AnyPlatformLength.as_exact(), None);
    }

    #[test]
    fn a_stated_length_rule_reads_as_itself() {
        for (stored, expected) in [
            (4, PinLength::Four),
            (5, PinLength::Five),
            (6, PinLength::Six),
        ] {
            assert_eq!(
                RequiredPinLength::read(Some(stored)),
                Ok(RequiredPinLength::Exactly(expected))
            );
            assert_eq!(
                RequiredPinLength::Exactly(expected).as_exact(),
                Some(expected)
            );
        }
    }

    /// An unreadable rule resolves to the **strictest** legal length, never to the unconstrained
    /// arm.
    ///
    /// The two candidates are not symmetric. Resolving to "no requirement" is fail-open on the
    /// exact control the rule exists to add; resolving to six costs a rotation prompt. One failure
    /// mode is silent and permanent, the other is loud and recoverable.
    #[test]
    fn an_unreadable_length_rule_resolves_strict_and_never_open() {
        for stored in [0_i64, 3, 7, 8, -1, i64::from(u8::MAX) + 1, i64::MAX] {
            let breach = RequiredPinLength::read(Some(stored))
                .expect_err("{stored} is not a length the platform accepts");

            assert_eq!(breach.stored, stored);
            assert_eq!(
                breach.resolved(),
                RequiredPinLength::Exactly(PinLength::Six),
                "a rule that cannot be read must never relax to `AnyPlatformLength`"
            );
            assert_ne!(breach.resolved(), RequiredPinLength::AnyPlatformLength);
        }
    }

    #[test]
    fn a_lockout_period_is_minutes_and_refuses_a_negative_one() {
        assert_eq!(
            LockoutPeriod::from_minutes(30).unwrap().minutes_to_state(),
            30
        );
        // Zero is a real configuration: a lockout with no advertised end is still a lockout, and
        // the till does not end one on a duration anyway.
        assert_eq!(
            LockoutPeriod::from_minutes(0).unwrap().minutes_to_state(),
            0
        );
        assert_eq!(
            LockoutPeriod::from_minutes(-1),
            Err(PinPolicyError::NegativeLockoutPeriod { minutes: -1 })
        );
    }

    #[test]
    fn a_session_lifetime_refuses_a_session_that_is_over_before_it_starts() {
        assert_eq!(SessionLifetime::from_hours(12).unwrap().as_hours(), 12);
        assert_eq!(
            SessionLifetime::from_hours(12).unwrap().as_duration(),
            Duration::from_secs(43_200)
        );
        for unusable in [0, -1] {
            assert_eq!(
                SessionLifetime::from_hours(unusable),
                Err(PinPolicyError::UnusableSessionLifetime { hours: unusable })
            );
        }
    }

    #[test]
    fn a_required_length_says_what_it_requires() {
        assert_eq!(
            RequiredPinLength::AnyPlatformLength.to_string(),
            "any supported length"
        );
        assert_eq!(
            RequiredPinLength::Exactly(PinLength::Six).to_string(),
            "6 digits"
        );
    }
}
