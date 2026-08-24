//! Whether the platform considers this terminal's hardware already enrolled.
//!
//! One concept, because one sentence on the pairing screen depends on it and that sentence is
//! about destruction: completing a re-enrolment **archives a working terminal**, and completing a
//! first enrolment does not.

use serde::{Deserialize, Deserializer};

/// Whether the platform considers this hardware already enrolled.
///
/// # Only the platform's answer builds this
///
/// The till's own store cannot answer it, and the two ways it gets this wrong point in opposite
/// directions:
///
/// - **A reinstalled till holds no secret and *is* enrolled.** Losing local data is the whole
///   reason a terminal re-enrols, so "do I hold a secret?" answers *no* for exactly the case that
///   matters.
/// - **A till whose company was deleted holds a secret and is *not* enrolled.** The platform
///   archives an orphaned terminal at request time and issues a fresh enrolment; nothing tells the
///   till, whose stored secret is cleared only by a deliberate de-registration.
///
/// A stored secret proves this device was enrolled here *once*. The question the screen asks is
/// whether approving the code replaces a *working* terminal, and liveness is precisely what a
/// server-side archival invalidates without the till observing it. Wrong in both directions is not
/// a signal, so this type is built from the platform's `isRePair` and from nothing else.
///
/// # `Undetermined` is not a placeholder
///
/// It means the platform has not been asked yet — the pairing-request response carries no
/// enrolment signal at all — or is older than the platform's re-pair change. It is the same
/// refusal-to-invent that `PinVerification::Undetermined` carries, and it exists so that no
/// `unwrap_or(false)` can spell "nobody said" as "no". There is deliberately no `bool` accessor:
/// a boolean is where these three states collapse back into two.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HardwareEnrolment {
    /// The platform says this hardware is already enrolled, so approving the pairing code
    /// replaces a live terminal.
    AlreadyEnrolled,
    /// The platform says this hardware is not enrolled.
    NotEnrolled,
    /// Nobody has said. The status poll has not run, or the server predates the re-pair change.
    Undetermined,
}

impl Default for HardwareEnrolment {
    /// An absent `isRePair` means the server never spoke about enrolment, which is
    /// [`Self::Undetermined`] and emphatically not [`Self::NotEnrolled`].
    ///
    /// This is what `#[serde(default)]` lands on at the wire boundary, so getting it wrong here
    /// would silently turn every pre-re-pair server into one asserting a negative it never sent.
    fn default() -> Self {
        Self::Undetermined
    }
}

impl<'de> Deserialize<'de> for HardwareEnrolment {
    /// Reads the platform's `isRePair`, which is a bare JSON bool rather than a tagged enum.
    ///
    /// Hand-written rather than derived for that reason: a derive would expect the variant names
    /// this type spells, and the wire spells `true` and `false`.
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(match bool::deserialize(deserializer)? {
            true => Self::AlreadyEnrolled,
            false => Self::NotEnrolled,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_platform_saying_re_pair_means_already_enrolled() {
        let enrolment: HardwareEnrolment = serde_json::from_str("true").expect("a bare bool");
        assert_eq!(enrolment, HardwareEnrolment::AlreadyEnrolled);
    }

    #[test]
    fn the_platform_saying_not_a_re_pair_means_not_enrolled() {
        let enrolment: HardwareEnrolment = serde_json::from_str("false").expect("a bare bool");
        assert_eq!(enrolment, HardwareEnrolment::NotEnrolled);
    }

    /// The default is the whole reason an older server does not read as a denial.
    #[test]
    fn saying_nothing_is_undetermined_and_never_not_enrolled() {
        assert_eq!(
            HardwareEnrolment::default(),
            HardwareEnrolment::Undetermined
        );
        assert_ne!(HardwareEnrolment::default(), HardwareEnrolment::NotEnrolled);
    }

    /// A bool is not a legal spelling of this concept, and the type must refuse anything else the
    /// wire could carry. Without this, a server sending `"RE_PAIR"` would deserialise as *some*
    /// answer rather than being rejected.
    #[test]
    fn a_non_bool_is_refused_rather_than_guessed() {
        assert!(serde_json::from_str::<HardwareEnrolment>("\"RE_PAIR\"").is_err());
        assert!(serde_json::from_str::<HardwareEnrolment>("null").is_err());
    }
}
