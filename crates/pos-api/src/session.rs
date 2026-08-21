//! The terminal's session with the platform, and what happens when it has to be renewed.
//!
//! Nothing here is wired in yet; `auth-outcome-and-offline-lockout` is the first consumer.

use std::fmt;

use thiserror::Error;

// ============================================================================
// SessionToken
// ============================================================================

/// A session token cannot be blank.
///
/// The empty string is currently doing sentinel duty: `AuthService::get_session`
/// (`crates/pos-services/src/auth_service.rs:220-226`) reads a stored session and answers
/// `Ok(None)` when `session_token.is_empty()`, so "there is no session" and "there is a session
/// whose token is empty" are the same value wearing two meanings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[error("a session token cannot be blank; an absent session is `Option<SessionToken>`, not `\"\"`")]
pub struct BlankSessionToken;

/// The bearer token the till presents to the platform.
///
/// A newtype rather than a `String`, for two reasons that are separate:
///
/// - **It cannot be blank**, so it cannot be the sentinel described on [`BlankSessionToken`].
/// - **Its `Debug` is redacted.** The token is a bearer credential: anything holding it can act
///   as this terminal until it expires. A derived `Debug` would put it into any `tracing` call
///   that renders a struct containing one, which is how bearer tokens end up in log files.
///
/// There is no `Display` and no `Serialize`. The only way to the characters is
/// [`Self::expose`], named so that every place the token is in the clear can be found by
/// searching for it.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct SessionToken(String);

impl SessionToken {
    /// Builds a session token, rejecting a blank one.
    pub fn new(raw: impl Into<String>) -> Result<Self, BlankSessionToken> {
        let raw = raw.into();
        if raw.trim().is_empty() {
            Err(BlankSessionToken)
        } else {
            Ok(Self(raw))
        }
    }

    /// The token itself, for the caller that must put it in a header.
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for SessionToken {
    /// Renders `SessionToken(****)`, with a fixed star count that does not track the token's
    /// length.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SessionToken(****)")
    }
}

// ============================================================================
// ReauthOutcome
// ============================================================================

/// What happened when the till tried to renew its session.
///
/// This replaces a `Result<bool>`. `SyncService::try_reauthenticate`
/// (`crates/pos-services/src/sync_service.rs:384-409`) returns `Ok(false)` both when there are no
/// stored credentials to log in with (`:388-391`) and when the login itself was rejected
/// (`:401-405`) — one boolean carrying two situations that call for opposite responses, separated
/// only by the log level of the line above the `return`.
///
/// The difference matters: **no stored credentials** means this terminal was never paired, or its
/// pairing was wiped, and no amount of retrying fixes it — someone has to re-pair the device.
/// **Rejected** means the platform knows this terminal and has revoked it. **Unreachable** is the
/// only one of the three that is ordinary weather.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReauthOutcome {
    /// The platform issued a new session token.
    Renewed(SessionToken),
    /// This terminal has no stored credentials to authenticate with. Re-pairing is required;
    /// retrying is not.
    NoStoredCredentials,
    /// The platform knows this terminal and refused to renew it.
    Rejected,
    /// The platform could not be reached. The existing session may still be fine.
    Unreachable,
}

impl ReauthOutcome {
    /// Whether trying again later could plausibly succeed without anyone intervening.
    ///
    /// Exhaustive with no catch-all arm: a new outcome has to answer this deliberately, and
    /// "should the sync loop keep retrying this" is exactly what the boolean it replaces got
    /// wrong for [`Self::NoStoredCredentials`].
    pub const fn is_worth_retrying(&self) -> bool {
        match self {
            Self::Unreachable => true,
            Self::Renewed(_) | Self::NoStoredCredentials | Self::Rejected => false,
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_token_debug_never_shows_the_token() {
        let token = SessionToken::new("eyJhbGciOiJIUzI1NiJ9.secret").unwrap();

        assert_eq!(format!("{token:?}"), "SessionToken(****)");
        assert_eq!(format!("{token:#?}"), "SessionToken(****)");

        // Including inside an enclosing struct that derives `Debug` — the path a bearer token
        // actually takes into a log file.
        #[derive(Debug)]
        struct Session {
            terminal_code: &'static str,
            token: SessionToken,
        }
        let session = Session {
            terminal_code: "POS-001",
            token,
        };
        let rendered = format!("{session:?}");

        assert!(rendered.contains("SessionToken(****)"), "got {rendered}");
        assert!(!rendered.contains("secret"), "got {rendered}");
        // The other fields render normally — the redaction is the token's, not the struct's.
        assert!(rendered.contains(session.terminal_code), "got {rendered}");
        assert!(session.token.expose().ends_with("secret"));
    }

    #[test]
    fn session_token_rejects_the_blank_sentinel() {
        // `AuthService::get_session` reads an empty stored token as "no session"; with this type
        // that value cannot be built in the first place.
        assert_eq!(SessionToken::new(""), Err(BlankSessionToken));
        assert_eq!(SessionToken::new("   "), Err(BlankSessionToken));
        assert_eq!(SessionToken::new("t").unwrap().expose(), "t");
    }

    #[test]
    fn reauth_outcome_retries_only_when_the_platform_was_unreachable() {
        assert!(ReauthOutcome::Unreachable.is_worth_retrying());

        // The two cases the `Ok(false)` this replaces could not tell apart. Neither is fixed by
        // trying again: one needs the device re-paired, the other has been revoked.
        assert!(!ReauthOutcome::NoStoredCredentials.is_worth_retrying());
        assert!(!ReauthOutcome::Rejected.is_worth_retrying());
        assert!(!ReauthOutcome::Renewed(SessionToken::new("fresh").unwrap()).is_worth_retrying());
    }
}
