//! Who is signed in at this till, and the credential that proves it.
//!
//! # The gap this closes
//!
//! `POST /api/pos/offline/upload` mounts `requireTerminalAuth, attendedOperatorAuthMiddleware`
//! (`offline.controller.ts:110-116`), and since `bf0d84bf`/`b927baab` six `/api/pos/till/*` routes
//! — ring a sale, void one, look up a receipt, open a shift, close it, process a return — sit
//! behind the same pair. The till held a terminal token and presented no operator one, so every
//! write it has ever attempted answered 401 `POS_OPERATOR_SESSION_REQUIRED`. The platform mints
//! the session on a verified PIN; the till was logging it and throwing it away.
//!
//! Only `SELF_SERVICE` and `KIOSK` terminals are exempt, read from the terminal's own row and
//! never from what the caller claims — and an **unknown** terminal is not exempt, because an
//! absent row is not evidence of a kiosk.
//!
//! # Why this is its own type rather than three methods on `AuthService`
//!
//! Two callers need it and they are not in a dependency relationship. `AuthService` mints the
//! session; `OfflineService` presents it and is the first to be told it is no longer good. Both
//! hold the same `(ApiClient, Database)` pair, and the discard is a **two-place write** — the
//! stored row and the client's header slot — where doing one and not the other is the bug: the
//! till either keeps presenting a credential the platform has already refused, turning one
//! refusal into a steady stream, or signs the cashier back in at the next restart. One owner, so
//! there is one place that can get it wrong.

use std::sync::Arc;

use anyhow::Result;
use chrono::{DateTime, Utc};
use pos_api::{ApiClient, ApiFailure, OperatorSession, OperatorSessionRefusal, SessionToken};
use pos_db::projection::{read_one, write};
use pos_db::terminal::{OperatorSessionRow, OPERATOR_SESSION_ROW};
use pos_db::Database;
use pos_models::OperatorId;
use tracing::{error, info, warn};

/// An operator session the till is holding, and whose it is.
///
/// The pair, because neither half is usable alone: the token is what
/// `attendedOperatorAuthMiddleware` reads, and the operator id is what the till needs to know who
/// is standing at the drawer after a restart. [`OperatorSession`] carries only the credential,
/// deliberately — it is what the *platform* mints, and the platform already knows whose it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeldOperatorSession {
    /// Who is signed in.
    pub operator_id: OperatorId,
    /// The credential, and when the platform says it lapses.
    pub session: OperatorSession,
}

/// The pair of things a till needs to prove a person is standing at the drawer.
pub struct OperatorSignIn {
    api: Arc<ApiClient>,
    db: Arc<Database>,
}

impl OperatorSignIn {
    /// Builds a view over the same client and store its callers already hold.
    pub const fn new(api: Arc<ApiClient>, db: Arc<Database>) -> Self {
        Self { api, db }
    }

    /// Records the session and starts presenting it, in that order.
    ///
    /// Persist first: a till that presents a credential it has not stored signs its cashier out on
    /// the next restart, and the queue it wakes up wanting to drain is exactly the work that needs
    /// one. A failed write is logged and not returned — the PIN *was* verified, and refusing to
    /// report that because SQLite would not answer would turn a storage fault into a failed
    /// sign-in.
    pub async fn record_and_present(&self, operator_id: &OperatorId, session: &OperatorSession) {
        if let Err(error) = self.record(operator_id, session) {
            error!(
                "the operator session could not be stored, so it will not survive a restart: \
                 {error:#}"
            );
        }
        self.api.set_operator_token(session.token().clone()).await;
    }

    /// Writes the operator session over whatever was there.
    ///
    /// `INSERT OR REPLACE` on the single row: one operator is signed in at a till at a time, and
    /// the previous session is gone the moment a new PIN is verified. Its own table rather than a
    /// column on `terminal_config`, which `save_terminal_config` rewrites wholesale on every
    /// terminal login — see [`pos_db::schema::SCHEMA_V14`].
    pub fn record(&self, operator_id: &OperatorId, session: &OperatorSession) -> Result<()> {
        let conn = self.db.connection();
        let conn = conn.lock();

        write(
            &conn,
            &OPERATOR_SESSION_ROW,
            &OperatorSessionRow {
                operator_id: Some(operator_id.clone()),
                token: Some(session.token().expose().to_string()),
                expires_at: Some(session.expires_at().to_rfc3339()),
            },
        )?;

        Ok(())
    }

    /// The operator session on disk, if the till is holding one.
    ///
    /// A stored row that cannot be read back into the domain answers `Ok(None)` and says why:
    /// a blank token, a blank operator id or an unparseable instant are all *no usable session*,
    /// and the remedy for every one of them is the same — verify a PIN. Returning an error would
    /// make a corrupt row look like a database that is down.
    pub fn held(&self) -> Result<Option<HeldOperatorSession>> {
        let conn = self.db.connection();
        let conn = conn.lock();

        // One projection, shared with `record` above, so the write list and the read list are the
        // same three names in the same order and cannot drift apart.
        let result = read_one(
            &conn,
            OPERATOR_SESSION_ROW.reader(),
            "FROM operator_sessions WHERE id = 1",
            [],
        );

        let row = match result {
            Ok(Some(row)) => row,
            Ok(None) => return Ok(None),
            Err(error) => return Err(error.into()),
        };

        // Blank and absent are the same answer here — *no usable session* — and they are made the
        // same answer once, in this function, rather than inside a `query_row` closure whose only
        // failure type is `rusqlite::Error` and which therefore could not have said why.
        // `OperatorId` refuses a blank, so an absent id is the only "no owner" this can be — the
        // `unwrap_or_default()` that used to stand here was over a `String` and folded a blank
        // into the same answer. Nothing can write a blank; see `OperatorSessionRow`.
        let Some(operator_id) = row.operator_id else {
            warn!("the stored operator session names no operator; nobody is signed in");
            return Ok(None);
        };
        let token = row.token.unwrap_or_default();
        let expires_at = row.expires_at.unwrap_or_default();

        let Ok(token) = SessionToken::new(token) else {
            warn!("the stored operator session has no token; nobody is signed in");
            return Ok(None);
        };
        let expires_at = match DateTime::parse_from_rfc3339(&expires_at) {
            Ok(instant) => instant.with_timezone(&Utc),
            Err(error) => {
                warn!("the stored operator session has an unreadable expiry: {error}");
                return Ok(None);
            }
        };

        Ok(Some(HeldOperatorSession {
            operator_id,
            session: OperatorSession::new(token, expires_at),
        }))
    }

    /// Stops holding an operator session, on disk and on the wire.
    ///
    /// Both, always. A session cleared from one and not the other is the state where the till
    /// keeps presenting a credential the platform has already refused — one refusal becoming a
    /// steady stream of them — or, the other way round, signs the cashier back in at the next
    /// restart.
    pub async fn sign_out(&self) -> Result<()> {
        self.api.clear_operator_token().await;

        let conn = self.db.connection();
        let conn = conn.lock();
        conn.execute("DELETE FROM operator_sessions WHERE id = 1", [])?;

        Ok(())
    }

    /// Presents the stored session again after a restart, and says whose it is.
    ///
    /// Deliberately does **not** check `expires_at` against the local clock. The server decides
    /// expiry and answers `POS_OPERATOR_SESSION_EXPIRED`; a till whose clock has drifted forward
    /// would otherwise sign a cashier out mid-shift over nothing, and one whose clock has drifted
    /// back would present a lapsed session either way. Reading the instant locally is a courtesy
    /// for the interface, never a gate — see [`Self::has_lapsed`].
    pub async fn restore(&self) -> Result<Option<OperatorId>> {
        let Some(held) = self.held()? else {
            return Ok(None);
        };

        self.api
            .set_operator_token(held.session.token().clone())
            .await;
        info!("operator session restored for {}", held.operator_id);
        Ok(Some(held.operator_id))
    }

    /// Whether the held session's stated expiry is in the past, as of `now`.
    ///
    /// A courtesy for the interface — *your session has probably run out, sign in again* — and
    /// never a substitute for the server's answer. `now` is a parameter so this is a pure function
    /// of the two instants rather than a call that reads a clock nobody passed it.
    pub fn has_lapsed(held: &HeldOperatorSession, now: DateTime<Utc>) -> bool {
        now >= held.session.expires_at()
    }

    /// Acts on a refusal that was about the operator's session.
    ///
    /// `None` when the failure said nothing about one — every non-`Refused` failure included,
    /// because an unreachable server has made no claim about who is signed in. When it did, the
    /// held session is discarded for all but [`OperatorSessionRefusal::NotPresented`], where there
    /// was nothing to hold.
    pub async fn read_refusal_of(&self, failure: &ApiFailure) -> Option<OperatorSessionRefusal> {
        let refusal = OperatorSessionRefusal::of(failure)?;
        self.sign_out_if_refused(refusal).await;
        Some(refusal)
    }

    /// Stops holding the session when the platform has declined it.
    ///
    /// A no-op for exactly [`OperatorSessionRefusal::NotPresented`], where there was nothing to
    /// hold. For every other refusal the credential is thrown away in both places — the stored row
    /// and the client's header slot — because keeping it means presenting it again on the next
    /// request, which is the shape that turns one refusal into a steady stream of them.
    ///
    /// A failed delete is logged and not returned. The session is gone from the wire either way,
    /// and a caller that already has a refusal to report should not also be handed a storage
    /// error it cannot act on.
    pub async fn sign_out_if_refused(&self, refusal: OperatorSessionRefusal) {
        if !refusal.discards_the_held_session() {
            return;
        }

        warn!("discarding the held operator session: {refusal}");
        if let Err(error) = self.sign_out().await {
            error!("the refused operator session could not be deleted: {error:#}");
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    use pos_db::init_memory_database;

    fn sign_in() -> OperatorSignIn {
        let db = init_memory_database().expect("an in-memory database");
        OperatorSignIn::new(Arc::new(ApiClient::new("http://127.0.0.1:1")), Arc::new(db))
    }

    fn instant(raw: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(raw)
            .expect("a literal instant")
            .with_timezone(&Utc)
    }

    fn session(token: &str, expires_at: &str) -> OperatorSession {
        OperatorSession::new(
            SessionToken::new(token).expect("a fixture token is never blank"),
            instant(expires_at),
        )
    }

    fn operator(id: &str) -> OperatorId {
        OperatorId::new(id).expect("a fixture id is never blank")
    }

    /// Nothing stored is nobody signed in — and that is not an error.
    #[test]
    fn an_empty_table_holds_nobody() {
        assert_eq!(
            sign_in().held().expect("an empty table is not a fault"),
            None
        );
    }

    /// Every column carries a distinct value and every field is asserted against its own.
    ///
    /// **The original reason is retired, and the test is not.** Per `df4e089` this guarded a
    /// positional read: `held()` took its row by `row.get(0..2)`, a column added, removed or
    /// reordered in the `SELECT` shifted every index after it, and `operator_id`, `token` and
    /// `expires_at` are all `TEXT` — so a swap compiled, ran, and signed the wrong person in with
    /// the wrong credential. `held()` now reads through `OPERATOR_SESSION_ROW`, which names its
    /// columns, so that failure is unreachable.
    ///
    /// What a declaration does not pin is *which* named column reaches which field. That swap is
    /// still expressible, it still type-checks for the same reason, and distinct values are still
    /// the only thing that catches it. Mutation-verified in both regimes: swapping `operator_id`
    /// and `token` fails this test on both fields.
    #[test]
    fn the_operator_session_read_takes_every_column_into_its_own_field() {
        let sign_in = sign_in();
        {
            let conn = sign_in.db.connection();
            let conn = conn.lock();
            conn.execute(
                "INSERT INTO operator_sessions (id, operator_id, token, expires_at) \
                 VALUES (1, 'operator-in-column-one', 'token-in-column-two', \
                 '2026-08-24T06:30:00+00:00')",
                [],
            )
            .expect("the fixture row inserts");
        }

        let held = sign_in
            .held()
            .expect("the row reads")
            .expect("a stored session is held");

        assert_eq!(held.operator_id.as_str(), "operator-in-column-one");
        assert_eq!(held.session.token().expose(), "token-in-column-two");
        assert_eq!(
            held.session.expires_at(),
            instant("2026-08-24T06:30:00+00:00")
        );
    }

    /// The NULL pass. Every column is nullable to SQLite's reader, and a NULL in any of the three
    /// means the same thing: no usable session, verify a PIN. Never an error, and never a panic.
    #[test]
    fn a_session_row_with_nulls_holds_nobody_rather_than_failing() {
        for (label, sql) in [
            (
                "no operator",
                "VALUES (1, NULL, 'tok', '2026-08-24T06:00:00+00:00')",
            ),
            (
                "no token",
                "VALUES (1, 'op-1', NULL, '2026-08-24T06:00:00+00:00')",
            ),
            ("no expiry", "VALUES (1, 'op-1', 'tok', NULL)"),
            (
                "an expiry that is not an instant",
                "VALUES (1, 'op-1', 'tok', 'sometime tuesday')",
            ),
        ] {
            let sign_in = sign_in();
            {
                let conn = sign_in.db.connection();
                let conn = conn.lock();
                // The columns are `NOT NULL` in the shipped schema, so the fixture has to build a
                // permissive table to produce the row at all. That is the point: a device whose
                // file was written by an older build, or edited, must not crash the till.
                conn.execute_batch(
                    "DROP TABLE operator_sessions;
                     CREATE TABLE operator_sessions (
                         id INTEGER PRIMARY KEY,
                         operator_id TEXT,
                         token TEXT,
                         expires_at TEXT
                     );",
                )
                .expect("the permissive fixture table builds");
                conn.execute(
                    &format!(
                        "INSERT INTO operator_sessions (id, operator_id, token, expires_at) {sql}"
                    ),
                    [],
                )
                .expect("the fixture row inserts");
            }

            assert_eq!(
                sign_in.held().expect("a damaged row is not a fault"),
                None,
                "with {label}, nobody is signed in"
            );
        }
    }

    /// Signing in twice replaces the session rather than adding one.
    #[tokio::test]
    async fn a_second_sign_in_replaces_the_first() {
        let sign_in = sign_in();

        sign_in
            .record_and_present(&operator("op-1"), &session("tok-1", "2026-08-24T06:00:00Z"))
            .await;
        sign_in
            .record_and_present(&operator("op-2"), &session("tok-2", "2026-08-24T18:00:00Z"))
            .await;

        let held = sign_in.held().expect("the row reads").expect("one is held");
        assert_eq!(held.operator_id.as_str(), "op-2");
        assert_eq!(held.session.token().expose(), "tok-2");

        // And the client is presenting the second one, not the first. Two places, one truth.
        assert_eq!(
            sign_in
                .api
                .operator_token()
                .await
                .expect("a session is presented")
                .expose(),
            "tok-2"
        );
    }

    /// A session survives a restart: `restore` puts a stored credential back on the wire.
    #[tokio::test]
    async fn a_stored_session_is_presented_again_after_a_restart() {
        let db = Arc::new(init_memory_database().expect("an in-memory database"));

        // The shift before the restart.
        let before =
            OperatorSignIn::new(Arc::new(ApiClient::new("http://127.0.0.1:1")), db.clone());
        before
            .record_and_present(&operator("op-1"), &session("tok-1", "2026-08-24T06:00:00Z"))
            .await;

        // A new process: same store, a client that has never seen a token.
        let after = OperatorSignIn::new(Arc::new(ApiClient::new("http://127.0.0.1:1")), db);
        assert_eq!(after.api.operator_token().await, None);

        let restored = after.restore().await.expect("the row reads");
        assert_eq!(
            restored.map(|id| id.as_str().to_string()),
            Some("op-1".to_string())
        );
        assert_eq!(
            after
                .api
                .operator_token()
                .await
                .expect("the restored session is presented")
                .expose(),
            "tok-1"
        );
    }

    /// A refusal that discards takes the session out of **both** places.
    ///
    /// One and not the other is the bug: leaving the row signs the cashier back in at the next
    /// restart, and leaving the header presents a credential the platform has already refused, on
    /// every subsequent request.
    #[tokio::test]
    async fn a_refusal_that_discards_clears_the_row_and_the_header() {
        for refusal in [
            OperatorSessionRefusal::NotHonoured,
            OperatorSessionRefusal::Lapsed,
            OperatorSessionRefusal::Revoked,
            OperatorSessionRefusal::OperatorInactive,
            OperatorSessionRefusal::OperatorLocked,
            OperatorSessionRefusal::OperatorUnknown,
        ] {
            let sign_in = sign_in();
            sign_in
                .record_and_present(&operator("op-1"), &session("tok-1", "2026-08-24T06:00:00Z"))
                .await;

            sign_in.sign_out_if_refused(refusal).await;

            assert_eq!(
                sign_in.held().expect("the row reads"),
                None,
                "for {refusal:?}"
            );
            assert_eq!(sign_in.api.operator_token().await, None, "for {refusal:?}");
        }
    }

    /// `NotPresented` discards nothing, because there was nothing to discard.
    ///
    /// The one refusal where the till holding a session is not the problem — and where throwing
    /// one away would sign out an operator over a request that simply forgot to present it.
    #[tokio::test]
    async fn no_session_presented_does_not_sign_anybody_out() {
        let sign_in = sign_in();
        sign_in
            .record_and_present(&operator("op-1"), &session("tok-1", "2026-08-24T06:00:00Z"))
            .await;

        sign_in
            .sign_out_if_refused(OperatorSessionRefusal::NotPresented)
            .await;

        assert!(sign_in.held().expect("the row reads").is_some());
        assert!(sign_in.api.operator_token().await.is_some());
    }

    /// The local expiry read is a courtesy over two instants, and reads no clock of its own.
    #[test]
    fn the_lapse_check_is_a_pure_function_of_the_two_instants() {
        let held = HeldOperatorSession {
            operator_id: operator("op-1"),
            session: session("tok-1", "2026-08-24T06:00:00Z"),
        };

        assert!(!OperatorSignIn::has_lapsed(
            &held,
            instant("2026-08-24T05:59:59Z")
        ));
        // At the instant itself, not after it: a session that expires at 06:00 is not usable at
        // 06:00.
        assert!(OperatorSignIn::has_lapsed(
            &held,
            instant("2026-08-24T06:00:00Z")
        ));
        assert!(OperatorSignIn::has_lapsed(
            &held,
            instant("2026-08-24T06:00:01Z")
        ));
    }
}
