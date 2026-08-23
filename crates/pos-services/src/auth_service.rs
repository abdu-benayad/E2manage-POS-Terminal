//! Authentication Service - Terminal and operator authentication
//!
//! Handles terminal login, operator PIN verification, and session management.
//! Supports both online verification (via API) and offline verification (via local DB).

use anyhow::{anyhow, Result};
use bcrypt::{hash, verify, DEFAULT_COST};
use chrono::{DateTime, NaiveDateTime, Utc};
use pos_api::{
    ApiClient, ApiFailure, HeartbeatRequest, HeartbeatResponse, LoginTerminalResponse,
    RefusalDetails, ServerErrorCode, VerifyPinResponse,
};
use pos_db::column::{operator_name, operator_role};
use pos_db::Database;
use pos_models::{
    Authority, CredentialExpiry, EnrolmentState, OperatorId, OperatorName, OperatorPermissions,
    OperatorRole, Pin, PinPolicy, PinRefusal, PinVerification, StoreFailure, StoreFailureKind,
    UndeterminedCause, VerifiedOperator,
};
use rusqlite::params;
use std::sync::Arc;
use tracing::{debug, error, info, warn};

/// Authentication service for terminal and operator management
pub struct AuthService {
    api: Arc<ApiClient>,
    db: Arc<Database>,
}

/// Represents the current terminal configuration
#[derive(Debug, Clone)]
pub struct TerminalSession {
    pub terminal_id: String,
    pub terminal_code: String,
    pub hardware_id: String,
    pub session_token: String,
    pub company_id: String,
    pub branch_id: Option<String>,
    pub locale: String,
    pub currency: String,
    pub tax_rate: f64,
    pub tax_inclusive: bool,
    pub sector: String,
    pub features: Vec<String>,
}

/// The seven columns the offline PIN check reads, named rather than positional at the call.
///
/// A positional `row.get(n)` beside a tuple of the same arity is structurally blind to a dropped
/// column: remove one from the `SELECT` and every subsequent index silently shifts by one, with
/// the types still lining up. Naming each field where it is read does not fix that on its own —
/// `the_offline_read_takes_every_column_from_its_own_position` does, with a distinct value per
/// column — but it makes the shift visible in the diff instead of invisible in an index.
///
/// `name` and `role` arrive as domain types because `pos_db::column` reads them that way, which is
/// what its helpers exist for: a role this till does not recognise means the contract moved, and
/// reading it as `Cashier` would be a privilege decision made by a fallback. Holding them as
/// `String` here would also re-open what
/// `tests/guards.rs::operator_identity_never_survives_as_a_bare_string` closes.
struct StoredCredential {
    pin_hash: String,
    name: OperatorName,
    role: OperatorRole,
    permissions_json: Option<String>,
    is_active: bool,
    updated_at: String,
}

/// A stored credential that parsed into the domain, with the two raw fields still needed.
struct ReadableCredential {
    verified: VerifiedOperator,
    pin_hash: String,
    updated_at: String,
}

/// A column that held something outside the domain type it maps to.
///
/// A named error rather than a `String`, so `StoreFailure`'s source stays an error: the failure
/// this issue exists to remove is the one where an error becomes a message and stops being
/// branchable.
#[derive(Debug, thiserror::Error)]
#[error("{0}")]
struct UnreadableRow(String);

impl StoredCredential {
    /// Reads the row into the domain, fail-closed.
    ///
    /// A role the server's enum does not admit, or a blank name, is a broken row — and refusing to
    /// authenticate against a broken row is the safe direction. Parsing **before** the PIN
    /// comparison keeps that true for a wrong PIN too.
    fn into_verified(self, operator_id: &OperatorId) -> Result<ReadableCredential, StoreFailure> {
        // Absent is a real state and means this operator holds nothing beyond ringing a sale. A
        // column that is present and unreadable is not: that is a broken row, and reading it as
        // "no permissions" would be a privilege decision made by a fallback.
        let permissions = match self.permissions_json {
            None => OperatorPermissions::none(),
            Some(json) => serde_json::from_str(&json).map_err(|e| {
                StoreFailure::new(
                    "reading the operator's stored credential",
                    StoreFailureKind::RowUnreadable,
                )
                .caused_by(UnreadableRow(format!("the `permissions_json` column: {e}")))
            })?,
        };

        Ok(ReadableCredential {
            verified: VerifiedOperator::from_verified_pin(
                operator_id.clone(),
                self.name,
                self.role,
                permissions,
            ),
            pin_hash: self.pin_hash,
            updated_at: self.updated_at,
        })
    }
}

/// Reads the instant SQLite's `datetime('now')` writes: `YYYY-MM-DD HH:MM:SS`, in UTC.
///
/// RFC 3339 is accepted too, because a row written by anything other than that default carries a
/// `T` and an offset. Anything else is a row the till cannot date, and a credential it will not
/// authenticate against.
fn parse_store_instant(raw: &str) -> Option<DateTime<Utc>> {
    NaiveDateTime::parse_from_str(raw, "%Y-%m-%d %H:%M:%S")
        .map(|naive| naive.and_utc())
        .ok()
        .or_else(|| {
            DateTime::parse_from_rfc3339(raw)
                .map(|dt| dt.with_timezone(&Utc))
                .ok()
        })
}

impl AuthService {
    /// Creates a new authentication service
    ///
    /// # Arguments
    ///
    /// * `api` - API client for backend communication
    /// * `db` - Local database for offline support
    pub fn new(api: Arc<ApiClient>, db: Arc<Database>) -> Self {
        Self { api, db }
    }

    // ========================================================================
    // TERMINAL AUTHENTICATION
    // ========================================================================

    /// Logs in the terminal with the backend
    ///
    /// Authenticates using hardware ID and secret, stores the session locally.
    ///
    /// # Arguments
    ///
    /// * `hardware_id` - Terminal hardware identifier
    /// * `secret` - Terminal secret from registration
    ///
    /// # Returns
    ///
    /// Terminal session with configuration
    pub async fn login_terminal(
        &self,
        terminal_code: &str,
        hardware_id: &str,
        secret: &str,
    ) -> Result<TerminalSession> {
        info!(
            "Logging in terminal {} with hardware ID: {}",
            terminal_code, hardware_id
        );

        let response = self
            .api
            .login_terminal(terminal_code, hardware_id, secret)
            .await?;

        // Save terminal configuration to local database
        self.save_terminal_config(hardware_id, &response)?;

        info!(
            "Terminal logged in successfully: {} ({})",
            response.terminal_code, response.terminal_id
        );

        Ok(TerminalSession {
            terminal_id: response.terminal_id,
            terminal_code: response.terminal_code,
            hardware_id: hardware_id.to_string(),
            session_token: response.session_token,
            company_id: response.company_id,
            branch_id: response.branch_id,
            locale: response.config.locale.unwrap_or_else(|| "ar".to_string()),
            currency: response
                .config
                .currency
                .unwrap_or_else(|| "LYD".to_string()),
            tax_rate: response
                .config
                .tax_config
                .as_ref()
                .map(|t| t.default_rate)
                .unwrap_or(0.0),
            tax_inclusive: response
                .config
                .tax_config
                .as_ref()
                .map(|t| t.tax_inclusive)
                .unwrap_or(false),
            sector: response
                .config
                .business_sector
                .unwrap_or_else(|| "RETAIL".to_string()),
            features: response.features,
        })
    }

    /// Saves terminal configuration to local database
    fn save_terminal_config(
        &self,
        hardware_id: &str,
        response: &LoginTerminalResponse,
    ) -> Result<()> {
        let conn = self.db.connection();
        let conn = conn.lock();

        conn.execute(
            r#"
            INSERT OR REPLACE INTO terminal_config
            (id, terminal_id, terminal_code, hardware_id, session_token,
             company_id, branch_id, locale, currency,
             tax_rate, tax_inclusive, sector, updated_at)
            VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, datetime('now'))
            "#,
            params![
                response.terminal_id,
                response.terminal_code,
                hardware_id,
                response.session_token,
                response.company_id,
                response.branch_id,
                response.config.locale,
                response.config.currency,
                response
                    .config
                    .tax_config
                    .as_ref()
                    .map(|t| t.default_rate)
                    .unwrap_or(0.0),
                response
                    .config
                    .tax_config
                    .as_ref()
                    .map(|t| t.tax_inclusive as i32)
                    .unwrap_or(0),
                response.config.business_sector,
            ],
        )?;

        debug!("Terminal config saved to local database");
        Ok(())
    }

    /// Loads saved terminal session from local database
    ///
    /// Useful for restoring session after app restart
    pub fn load_saved_session(&self) -> Result<Option<TerminalSession>> {
        let conn = self.db.connection();
        let conn = conn.lock();

        let result = conn.query_row(
            r#"
            SELECT terminal_id, terminal_code, hardware_id, session_token,
                   company_id, branch_id, locale, currency,
                   tax_rate, tax_inclusive, sector
            FROM terminal_config
            WHERE id = 1
            "#,
            [],
            |row| {
                Ok(TerminalSession {
                    terminal_id: row.get(0)?,
                    terminal_code: row.get(1)?,
                    hardware_id: row.get(2)?,
                    session_token: row.get::<_, Option<String>>(3)?.unwrap_or_default(),
                    company_id: row.get::<_, Option<String>>(4)?.unwrap_or_default(),
                    branch_id: row.get(5)?,
                    locale: row
                        .get::<_, Option<String>>(6)?
                        .unwrap_or_else(|| "ar".to_string()),
                    currency: row
                        .get::<_, Option<String>>(7)?
                        .unwrap_or_else(|| "LYD".to_string()),
                    tax_rate: row.get::<_, Option<f64>>(8)?.unwrap_or(0.0),
                    tax_inclusive: row.get::<_, Option<i32>>(9)?.unwrap_or(0) != 0,
                    sector: row
                        .get::<_, Option<String>>(10)?
                        .unwrap_or_else(|| "RETAIL".to_string()),
                    features: vec![], // Features need to be synced
                })
            },
        );

        match result {
            Ok(session) => {
                if !session.session_token.is_empty() {
                    Ok(Some(session))
                } else {
                    Ok(None)
                }
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Sends a heartbeat to the backend
    pub async fn send_heartbeat(&self, metrics: &HeartbeatRequest) -> Result<HeartbeatResponse> {
        self.api.send_heartbeat(metrics).await
    }

    /// Checks if the backend is reachable and authenticated
    pub async fn is_online(&self) -> bool {
        self.api.is_online().await.is_online()
    }

    // ========================================================================
    // OPERATOR PIN VERIFICATION
    // ========================================================================

    /// What happened when this operator entered this PIN.
    ///
    /// **Total.** There is no `Result`: "the till could not find out" is
    /// [`PinVerification::Undetermined`], a case a caller must handle, not an error it can `?`
    /// past. That `?` was the bug — `verify_pin` used to treat *every* error as grounds to fall
    /// back to local verification, so a 401 saying "this operator is locked" and a dead network
    /// took the same branch, and the local branch has no attempt counter.
    ///
    /// # The online leg decides, and only `Unreachable` reaches the offline one
    ///
    /// A refusal is an **answer**. The platform reached its verdict against the operator's real
    /// lockout state and a real attempt ledger, and the till has neither, so overriding it locally
    /// is the bypass this issue is named for. Only [`ApiFailure::Unreachable`] — nobody answered —
    /// falls through.
    ///
    /// # The `is_online()` precheck is gone
    ///
    /// It was a second round trip to `/api/pos/sync/status`, computing reachability independently
    /// of what the PIN request itself discovers. Two representations of one fact, sampled at two
    /// different moments, free to disagree — and its `OnlineStatus::AuthRejected` arm returned
    /// `false` exactly like a dead network, so a lapsed *terminal* token silently skipped the
    /// online leg entirely. The PIN request is its own reachability probe.
    ///
    /// `policy` is a parameter because the policy is **carried, not looked up**: it arrives with
    /// the terminal session at login, strictly before an operator is selected, so a screen that
    /// can render PIN entry necessarily holds one.
    pub async fn verify_pin(
        &self,
        operator_id: &OperatorId,
        pin: &Pin,
        policy: &PinPolicy,
    ) -> PinVerification {
        debug!("Verifying PIN for operator: {}", operator_id);

        match self
            .api
            .verify_operator_pin(operator_id, pin.expose_digits())
            .await
        {
            Ok(verified) => self.accepted_by_platform(operator_id, verified),
            Err(ApiFailure::Unreachable(error)) => {
                debug!("the platform could not be reached, verifying locally: {error}");
                self.verify_pin_offline(operator_id, pin, policy)
            }
            Err(ApiFailure::Unreadable(error)) => {
                error!(
                    "contract breach verifying a PIN: the platform answered and this till could \
                     not read the answer: {error}"
                );
                PinVerification::Undetermined(UndeterminedCause::contract_breach(error))
            }
            Err(refusal @ ApiFailure::Refused { .. }) => Self::refused_by_platform(refusal),
        }
    }

    /// Builds the accepted outcome **from the response body**.
    ///
    /// The deleted `get_operator_info` read the local `operators` table here — a second fallible
    /// read that re-graded the server's answer against a cache, and silently demoted a
    /// server-confirmed operator whose row had not synced yet. The platform just told the till who
    /// this is; asking the till's own stale copy to confirm it is how a correct PIN produces
    /// "operator not found".
    fn accepted_by_platform(
        &self,
        operator_id: &OperatorId,
        verified: VerifyPinResponse,
    ) -> PinVerification {
        // A till always presents `X-Terminal-Token`, so the server always mints a session. Absent
        // means this request went out without one, which is a bug on this side of the wire, not a
        // branch to write a fallback for. Persisting it needs the column task 10 adds.
        match &verified.session {
            Some(session) => debug!(
                "operator session minted, expiring {}",
                session.expires_at().to_rfc3339()
            ),
            None => error!(
                "the platform minted no operator session for {operator_id}: this till sent a \
                 verify-pin request without a terminal token"
            ),
        }

        let name = match OperatorName::new(verified.name.clone(), verified.name_ar.clone()) {
            Ok(name) => name,
            Err(error) => {
                error!("contract breach: the platform sent an unusable operator name: {error}");
                return PinVerification::Undetermined(UndeterminedCause::contract_breach(error));
            }
        };

        PinVerification::Accepted {
            operator: VerifiedOperator::from_verified_pin(
                verified.operator_id,
                name,
                verified.role,
                // `POS_OperatorProfile.permissions` is `Json?`, so an absent object is a real
                // state and means this operator holds nothing beyond ringing a sale. Constructed
                // here rather than derived from a `Default` — see
                // `tests/guards.rs::operator_permissions_has_exactly_one_definition_and_no_default`.
                verified
                    .permissions
                    .unwrap_or_else(OperatorPermissions::none),
            ),
            decided_by: Authority::Platform,
        }
    }

    /// Maps a refusal the platform actually made onto the outcome it means.
    ///
    /// Every arm here is a decision the server reached with information the till does not have.
    /// None of them falls through to the local leg.
    fn refused_by_platform(refusal: ApiFailure) -> PinVerification {
        let ApiFailure::Refused {
            status,
            code,
            message,
            details,
        } = refusal
        else {
            unreachable!("only a `Refused` reaches this function");
        };

        match (&code, details) {
            // The only refusal that spends the budget, and the count is the **platform's**. The
            // till keeps no ledger to second-guess it with.
            (ServerErrorCode::PosPinInvalid, Some(RefusalDetails::PinInvalid(pin_invalid))) => {
                PinVerification::Refused(PinRefusal::WrongPin {
                    attempts_remaining: pin_invalid.attempts_remaining,
                })
            }
            // The server contradicting its own partition: the attempt that empties the budget is
            // supposed to answer `POS_OPERATOR_LOCKED`. Read as the lock it means.
            (ServerErrorCode::PosPinInvalid, Some(RefusalDetails::PinBudgetExhausted)) => {
                PinVerification::Refused(PinRefusal::Locked)
            }
            (ServerErrorCode::PosOperatorLocked, _) => PinVerification::Refused(PinRefusal::Locked),
            (ServerErrorCode::PosOperatorInactive, _) => {
                PinVerification::Refused(PinRefusal::OperatorInactive)
            }
            (ServerErrorCode::PosOperatorNotFound, _) => {
                PinVerification::Refused(PinRefusal::OperatorUnknown)
            }
            // 403, and the PIN was **correct** — the server reaches this verdict only after bcrypt
            // accepts, deliberately, because deciding it from the stored length beforehand is a
            // free unlimited oracle on the required length. Consumes no attempt.
            //
            // The till must not reproduce that oracle: nothing about `expected` may be rendered
            // until a PIN has been accepted, and this outcome is the first moment that is true.
            (
                ServerErrorCode::PosPinRotationRequired,
                Some(RefusalDetails::PinRotationRequired(rotation)),
            ) => PinVerification::Refused(PinRefusal::CredentialRequiresRotation {
                expected: rotation.required_length,
            }),

            // The till has no standing to ask anything. Task 09 splits these into
            // `EnrolmentState` — a 403 `POS_TERMINAL_NOT_ACTIVE` is a repudiated enrolment and
            // terminal, while a 401 refreshes once and retries. Until then they share the outcome
            // that matters most: **not** a fall-through to the local leg.
            (
                ServerErrorCode::PosTerminalTokenMissing
                | ServerErrorCode::PosTerminalTokenInvalid
                | ServerErrorCode::PosTerminalSessionExpired
                | ServerErrorCode::PosTerminalSessionRevoked
                | ServerErrorCode::PosTerminalNotActive
                | ServerErrorCode::PosTerminalGone
                | ServerErrorCode::PosTerminalAuthFailed
                | ServerErrorCode::PosTerminalAuthRequired
                | ServerErrorCode::PosCompanyInactive,
                _,
            ) => {
                warn!("the platform refused this terminal's standing: {status} ({code}) {message}");
                PinVerification::Undetermined(UndeterminedCause::ReauthFailed)
            }

            // Everything else: a refusal this till cannot turn into an outcome. That includes a
            // code whose `details` did not arrive — `WrongPin` cannot be built without a count,
            // and inventing one would be a figure a cashier reads.
            (code, details) => {
                error!(
                    "unmapped refusal verifying a PIN: {status} ({code}) {message} \
                     [details: {details:?}]"
                );
                PinVerification::Undetermined(UndeterminedCause::contract_breach(
                    ApiFailure::Refused {
                        status,
                        code: code.clone(),
                        message,
                        details,
                    },
                ))
            }
        }
    }

    /// Verifies a PIN against what this till has stored, with the platform unreachable.
    ///
    /// # Three things this used to say that were not true
    ///
    /// - **An unknown operator and an inactive one were the same answer.** The query read
    ///   `WHERE id = ?1 AND is_active = 1`, so both produced no rows and both reported "operator
    ///   not found". The predicate is gone and the column is branched on. Neither consumes an
    ///   attempt.
    /// - **A bcrypt failure read as a wrong PIN.** `verify(pin, &hash).unwrap_or(false)` is the
    ///   anti-pattern the till conventions name outright: it *"reports a corrupt credential store
    ///   as a wrong PIN, silently and forever"*. An unreadable hash is the till's fault, not the
    ///   operator's, and charging their budget for it locks people out of a terminal over a
    ///   corrupt row.
    /// - **A wrong PIN was reported as a wrong PIN, with nothing counting it.** See below.
    ///
    /// # Why a local mismatch is `Undetermined` and not `WrongPin`
    ///
    /// [`PinRefusal::WrongPin`] is defined as *the only refusal that spends the retry budget*, and
    /// it carries the count that remains. Offline, this till has **no ledger** — `pos-db` has no
    /// lockout table, column or query — so there is no budget to spend and no count to report.
    /// Answering `WrongPin` here would mean either fabricating a figure a cashier then reads, or
    /// asserting a budget nothing enforces, which is precisely the bypass this issue is named for.
    ///
    /// So the honest answer is that the till could not settle it, which is what
    /// [`UndeterminedCause::ServerUnreachable`] already says: *"the platform could not be reached,
    /// and no local credential could settle the PIN."* It consumes no attempt by construction.
    /// Task 08 replaces this leg with a named outcome.
    pub fn verify_pin_offline(
        &self,
        operator_id: &OperatorId,
        pin: &Pin,
        policy: &PinPolicy,
    ) -> PinVerification {
        let stored = {
            let conn = self.db.connection();
            let conn = conn.lock();
            conn.query_row(
                "SELECT pin_hash, name, name_ar, role, permissions_json, is_active, updated_at \
                 FROM operators WHERE id = ?1",
                [operator_id.as_str()],
                |row| {
                    Ok(StoredCredential {
                        pin_hash: row.get(0)?,
                        // Two indices, because `name` and `name_ar` are one value: read
                        // separately they can drift into a row the domain says cannot exist.
                        name: operator_name(row, 1, 2)?,
                        role: operator_role(row, 3)?,
                        permissions_json: row.get(4)?,
                        is_active: row.get(5)?,
                        updated_at: row.get(6)?,
                    })
                },
            )
        };

        let stored = match stored {
            Ok(stored) => stored,
            Err(rusqlite::Error::QueryReturnedNoRows) => {
                // Consumes no attempt: there is no operator to charge it to, and charging an
                // unknown identifier would let anyone exhaust a real operator's budget by
                // guessing ids.
                warn!("no operator {operator_id} is known to this till");
                return PinVerification::Refused(PinRefusal::OperatorUnknown);
            }
            Err(error) => {
                // A column the domain type does not admit arrives as
                // `FromSqlConversionFailure` from `pos_db::column`, which is a broken **row**
                // rather than a failed query. Distinguishing them is what lets an operator with a
                // corrupt role be told apart from a database that is down.
                return Self::store_failed("reading the operator's stored credential", error);
            }
        };

        if !stored.is_active {
            // A distinct refusal, not the same empty row. Nothing the person at the till types
            // could change the answer, so it consumes no attempt either.
            return PinVerification::Refused(PinRefusal::OperatorInactive);
        }

        let operator = match stored.into_verified(operator_id) {
            Ok(operator) => operator,
            Err(failure) => return PinVerification::Undetermined(failure.into()),
        };

        // `updated_at` is when the platform last confirmed this row, and the tenant's offline
        // window is how long the till may act on a confirmation. That is the design's
        // `not_after = issued_at + maxOfflineHours`, with the only issuance instant this till
        // actually records. A row the till cannot date is a row it will not authenticate against.
        let confirmed_at = match parse_store_instant(&operator.updated_at) {
            Some(instant) => instant,
            None => {
                return Self::row_unreadable(
                    "dating the operator's stored credential",
                    UnreadableRow(format!("`updated_at` held `{}`", operator.updated_at)),
                );
            }
        };
        let not_after = CredentialExpiry::at(confirmed_at + policy.offline_window().as_duration());
        if not_after.has_passed(Utc::now()) {
            // Consumes no attempt: the operator must reach the platform, and no amount of
            // retyping achieves that.
            return PinVerification::Refused(PinRefusal::CredentialExpired);
        }

        match verify(pin.expose_digits(), &operator.pin_hash) {
            Ok(true) => {
                // `offline_authority` is the only route to an offline `Authority`, so "fell back
                // to a repudiated credential because the network was down" has to be written on
                // purpose rather than reached by accident. Enrolment state is task 09's; until it
                // is read from the store, a row this till holds is a row the platform last
                // confirmed at `updated_at`.
                match EnrolmentState::Active.offline_authority(not_after) {
                    Some(decided_by) => PinVerification::Accepted {
                        operator: operator.verified,
                        decided_by,
                    },
                    None => PinVerification::Refused(PinRefusal::CredentialUnreadable),
                }
            }
            Ok(false) => {
                // Not `WrongPin` — see the doc comment. Nothing counts offline.
                warn!("a locally stored credential did not match, and this till counts nothing");
                PinVerification::Undetermined(UndeterminedCause::ServerUnreachable)
            }
            Err(error) => {
                // The anti-pattern this replaces answered `false` here, reporting a corrupt
                // credential store as a wrong PIN — silently, forever, and against the operator's
                // budget.
                error!("the stored credential for {operator_id} could not be read: {error}");
                Self::store_failed("comparing the stored credential", error)
            }
        }
    }

    /// Verifies a PIN against local storage without an async runtime.
    ///
    /// Kept as a thin delegate; task 11 removes it along with its three test call sites.
    pub fn verify_pin_sync(
        &self,
        operator_id: &OperatorId,
        pin: &Pin,
        policy: &PinPolicy,
    ) -> PinVerification {
        self.verify_pin_offline(operator_id, pin, policy)
    }

    /// The store failed while doing something. Never a refusal — the operator is not at fault.
    fn store_failed(
        operation: &'static str,
        error: impl std::error::Error + Send + Sync + 'static,
    ) -> PinVerification {
        PinVerification::Undetermined(UndeterminedCause::StoreUnavailable(
            StoreFailure::new(operation, StoreFailureKind::QueryFailed).caused_by(error),
        ))
    }

    /// The same, for a query that ran and returned a row nobody could read.
    fn row_unreadable(
        operation: &'static str,
        error: impl std::error::Error + Send + Sync + 'static,
    ) -> PinVerification {
        PinVerification::Undetermined(UndeterminedCause::StoreUnavailable(
            StoreFailure::new(operation, StoreFailureKind::RowUnreadable).caused_by(error),
        ))
    }

    // ========================================================================
    // PIN HASHING UTILITIES
    // ========================================================================

    /// Hashes a PIN for storage
    ///
    /// # Arguments
    ///
    /// * `pin` - Plain text PIN
    ///
    /// # Returns
    ///
    /// Bcrypt hash of the PIN
    pub fn hash_pin(pin: &str) -> Result<String> {
        hash(pin, DEFAULT_COST).map_err(|e| anyhow!("Failed to hash PIN: {}", e))
    }

    /// Compares a PIN against a stored bcrypt hash.
    ///
    /// # `Result<bool>`, not `bool`
    ///
    /// This read `verify(pin, hash).unwrap_or(false)`, which the till's conventions name as an
    /// anti-pattern in those words: *"No `unwrap_or(false)` on a fallible verification.
    /// `bcrypt::verify(...).unwrap_or(false)` reports a corrupt credential store as a wrong PIN,
    /// silently and forever."* The two answers it folded together — *this is not the PIN* and
    /// *this is not a hash* — are the operator's fault and the till's respectively, and only one
    /// of them should ever cost somebody an attempt.
    ///
    /// A `bool` return has nowhere to put the second answer, so the signature is what had to
    /// change. It has no production callers; it exists for the tests and for
    /// [`Self::hash_pin`]'s round trip.
    pub fn verify_pin_hash(pin: &str, hash: &str) -> Result<bool> {
        verify(pin, hash).map_err(|e| anyhow!("the stored credential could not be read: {e}"))
    }

    // ========================================================================
    // SESSION MANAGEMENT
    // ========================================================================

    /// Clears the saved session
    pub fn clear_session(&self) -> Result<()> {
        let conn = self.db.connection();
        let conn = conn.lock();

        conn.execute(
            "UPDATE terminal_config SET session_token = NULL WHERE id = 1",
            [],
        )?;

        Ok(())
    }

    /// Updates the session token in local storage
    pub fn update_session_token(&self, token: &str) -> Result<()> {
        let conn = self.db.connection();
        let conn = conn.lock();

        conn.execute(
            "UPDATE terminal_config SET session_token = ?1, updated_at = datetime('now') WHERE id = 1",
            [token],
        )?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pos_db::init_memory_database;
    use pos_models::{
        LockoutPeriod, MaxAttempts, OfflineWindow, Permission, PinLength, RequiredPinLength,
        SessionLifetime,
    };

    fn op_id(id: &str) -> OperatorId {
        OperatorId::new(id).expect("a fixture id is never blank")
    }

    fn create_test_service() -> AuthService {
        let db = init_memory_database().unwrap();
        let api = ApiClient::new("https://api.example.com");
        AuthService::new(Arc::new(api), Arc::new(db))
    }

    #[test]
    fn test_hash_pin() {
        let pin = "1234";
        let hashed = AuthService::hash_pin(pin).unwrap();

        // Hash should not equal plain PIN
        assert_ne!(hashed, pin);

        // Should be verifiable
        assert!(AuthService::verify_pin_hash(pin, &hashed).expect("a hash this call just made"));

        // Wrong PIN should not verify
        assert!(!AuthService::verify_pin_hash("5678", &hashed).expect("still a readable hash"));
    }

    #[test]
    fn test_verify_pin_hash() {
        // Test with known bcrypt hash of "1234"
        let pin = "1234";
        let hash = AuthService::hash_pin(pin).unwrap();

        assert!(AuthService::verify_pin_hash("1234", &hash).expect("a readable hash"));
        assert!(!AuthService::verify_pin_hash("0000", &hash).expect("a readable hash"));
        assert!(!AuthService::verify_pin_hash("", &hash).expect("a readable hash"));
    }

    /// A hash that is not a hash is the till's problem, not the operator's.
    ///
    /// This is the whole reason the signature is a `Result`. `unwrap_or(false)` answered "wrong
    /// PIN" for every string below, which sends a corrupt credential store through the operator's
    /// lockout budget — silently, and forever.
    #[test]
    fn an_unreadable_hash_is_not_a_wrong_pin() {
        for not_a_hash in ["", "   ", "1234", "$2b$notactuallyahash", "\u{0}"] {
            let outcome = AuthService::verify_pin_hash("1234", not_a_hash);

            assert!(
                outcome.is_err(),
                "`{not_a_hash}` is not a bcrypt hash and must not read as a wrong PIN"
            );
        }
    }

    #[test]
    fn test_load_saved_session_empty() {
        let service = create_test_service();
        let session = service.load_saved_session().unwrap();
        assert!(session.is_none());
    }

    /// Every column of `terminal_config` carries a **distinct** value, and every field of the
    /// loaded `TerminalSession` is asserted.
    ///
    /// Both halves are load-bearing. `load_saved_session` reads its row by position, so a column
    /// added, removed or reordered in the SELECT list silently shifts every index after it —
    /// and because the columns either side are all TEXT, the swapped read compiles and satisfies
    /// rusqlite's type check. Asserting a subset leaves the unasserted positions free to swap;
    /// repeating a value across same-typed columns lets a swap between *those two* pass. So a
    /// later edit that tidies these fixtures into shared or omitted values disarms the test
    /// without failing it.
    ///
    /// The three columns with fallbacks — locale, currency, sector — are given values that
    /// differ from their defaults ("ar", "LYD", "RETAIL"), so a fallback firing over a stored
    /// value fails here too.
    #[test]
    fn test_load_saved_session_reads_every_column_into_its_own_field() {
        let service = create_test_service();

        {
            let conn = service.db.connection();
            let conn = conn.lock();
            conn.execute(
                r#"
                INSERT INTO terminal_config
                (id, terminal_id, terminal_code, hardware_id, session_token,
                 company_id, branch_id, locale, currency, tax_rate, tax_inclusive, sector)
                VALUES (1, 'terminal-id-1', 'terminal-code-1', 'hardware-id-1', 'session-token-1',
                        'company-id-1', 'branch-id-1', 'en', 'USD', 15.5, 1, 'PHARMACY')
                "#,
                [],
            )
            .unwrap();
        }

        let session = service.load_saved_session().unwrap().unwrap();

        assert_eq!(session.terminal_id, "terminal-id-1");
        assert_eq!(session.terminal_code, "terminal-code-1");
        assert_eq!(session.hardware_id, "hardware-id-1");
        assert_eq!(session.session_token, "session-token-1");
        assert_eq!(session.company_id, "company-id-1");
        assert_eq!(session.branch_id, Some("branch-id-1".to_string()));
        assert_eq!(session.locale, "en");
        assert_eq!(session.currency, "USD");
        assert_eq!(session.tax_rate, 15.5);
        assert!(session.tax_inclusive);
        assert_eq!(session.sector, "PHARMACY");
        assert!(
            session.features.is_empty(),
            "features are synced separately"
        );
    }

    /// The fallbacks are the other half of the mapping: a NULL in any nullable column must reach
    /// its own field's default, not another field's.
    #[test]
    fn test_load_saved_session_applies_each_fallback_to_its_own_field() {
        let service = create_test_service();

        {
            let conn = service.db.connection();
            let conn = conn.lock();
            conn.execute(
                r#"
                INSERT INTO terminal_config
                (id, terminal_id, terminal_code, hardware_id, session_token,
                 company_id, branch_id, locale, currency, tax_rate, tax_inclusive, sector)
                VALUES (1, 'terminal-id-1', 'terminal-code-1', 'hardware-id-1', 'session-token-1',
                        NULL, NULL, NULL, NULL, NULL, NULL, NULL)
                "#,
                [],
            )
            .unwrap();
        }

        let session = service.load_saved_session().unwrap().unwrap();

        assert_eq!(session.company_id, "");
        assert_eq!(session.branch_id, None);
        assert_eq!(session.locale, "ar");
        assert_eq!(session.currency, "LYD");
        assert_eq!(session.tax_rate, 0.0);
        assert!(!session.tax_inclusive);
        assert_eq!(session.sector, "RETAIL");
    }

    fn pin(digits: &str) -> Pin {
        Pin::parse(digits).expect("the fixtures use platform-legal PINs")
    }

    fn policy() -> PinPolicy {
        PinPolicy::new(
            RequiredPinLength::Exactly(PinLength::Four),
            MaxAttempts::new(3).expect("three is not zero"),
            LockoutPeriod::from_minutes(30).expect("thirty is not negative"),
            SessionLifetime::from_hours(12).expect("twelve is positive"),
            OfflineWindow::from_hours(24).expect("twenty-four is not negative"),
        )
    }

    #[test]
    fn an_operator_this_till_has_never_heard_of_is_unknown() {
        let service = create_test_service();

        let outcome = service.verify_pin_offline(&op_id("nonexistent"), &pin("1234"), &policy());

        assert!(
            matches!(
                outcome,
                PinVerification::Refused(PinRefusal::OperatorUnknown)
            ),
            "got {outcome:?}"
        );
    }

    /// Unknown and inactive are **distinct** refusals, and neither spends an attempt.
    ///
    /// The query read `WHERE id = ?1 AND is_active = 1`, so both were one empty row and both
    /// reported "operator not found". Acceptance row 7.
    #[test]
    fn an_inactive_operator_is_not_an_unknown_one() {
        let service = create_test_service();
        insert_operator(&service, "op-inactive", "1234", false);

        let outcome = service.verify_pin_offline(&op_id("op-inactive"), &pin("1234"), &policy());

        assert!(
            matches!(
                outcome,
                PinVerification::Refused(PinRefusal::OperatorInactive)
            ),
            "got {outcome:?}"
        );
        for refusal in [PinRefusal::OperatorInactive, PinRefusal::OperatorUnknown] {
            assert!(
                !refusal.consumes_an_attempt(),
                "{refusal:?} must not spend the operator's budget"
            );
        }
    }

    /// A credential the till cannot date is a credential it will not authenticate against.
    #[test]
    fn a_credential_past_the_offline_window_is_refused_without_spending_an_attempt() {
        let service = create_test_service();
        insert_operator(&service, "op-stale", "1234", true);
        {
            let conn = service.db.connection();
            let conn = conn.lock();
            conn.execute(
                "UPDATE operators SET updated_at = '2020-01-01 00:00:00' WHERE id = 'op-stale'",
                [],
            )
            .unwrap();
        }

        let outcome = service.verify_pin_offline(&op_id("op-stale"), &pin("1234"), &policy());

        assert!(
            matches!(
                outcome,
                PinVerification::Refused(PinRefusal::CredentialExpired)
            ),
            "got {outcome:?}"
        );
        assert!(!PinRefusal::CredentialExpired.consumes_an_attempt());
    }

    /// A stored hash that is not a hash answers `Undetermined`, never "wrong PIN".
    ///
    /// The end-to-end form of `an_unreadable_hash_is_not_a_wrong_pin`, through the query. This is
    /// the exact row every operator on this till has today: `getOperators` stopped sending
    /// `pinHash`, and the DTO defaults the missing field to `""`.
    #[test]
    fn a_credential_store_that_cannot_be_read_is_undetermined_and_costs_nothing() {
        let service = create_test_service();
        insert_operator(&service, "op-corrupt", "1234", true);
        {
            let conn = service.db.connection();
            let conn = conn.lock();
            conn.execute(
                "UPDATE operators SET pin_hash = '' WHERE id = 'op-corrupt'",
                [],
            )
            .unwrap();
        }

        let outcome = service.verify_pin_offline(&op_id("op-corrupt"), &pin("1234"), &policy());

        let PinVerification::Undetermined(UndeterminedCause::StoreUnavailable(failure)) = outcome
        else {
            panic!("an unreadable credential store is not a decision about the PIN: {outcome:?}");
        };
        assert_eq!(failure.operation(), "comparing the stored credential");
    }

    /// Every column the offline read selects carries a **distinct** value, and every field that
    /// reaches the outcome is asserted.
    ///
    /// `verify_pin_offline` reads its row by position, so a column added, removed or reordered in
    /// the `SELECT` list silently shifts every index after it — and with `name`, `name_ar`,
    /// `role` and `permissions_json` all TEXT, a shifted read still compiles and still returns a
    /// `String`. Reading the SQL beside the indices is structurally blind to that; only distinct
    /// values catch it.
    ///
    /// The NULL pass below is the other half: `name_ar` and `permissions_json` are nullable, and a
    /// shift that lands a NULL where a NOT NULL column was expected fails differently.
    #[test]
    fn the_offline_read_takes_every_column_from_its_own_position() {
        let service = create_test_service();
        let hash = AuthService::hash_pin("4321").unwrap();
        {
            let conn = service.db.connection();
            let conn = conn.lock();
            conn.execute(
                r#"INSERT INTO operators
                   (id, code, name, name_ar, pin_hash, role, permissions_json, is_active,
                    updated_at)
                   VALUES ('op-distinct', 'CODE-DISTINCT', 'Sara Haddad', 'سارة حداد', ?1,
                           'MANAGER', '{"canVoid":true}', 1, '2026-08-23 09:00:00')"#,
                [&hash],
            )
            .unwrap();
        }

        let outcome = service.verify_pin_offline(&op_id("op-distinct"), &pin("4321"), &policy());

        let PinVerification::Accepted {
            operator,
            decided_by,
        } = outcome
        else {
            panic!("the fixture's PIN is the fixture's hash: {outcome:?}");
        };
        assert_eq!(operator.id().as_str(), "op-distinct");
        assert_eq!(operator.name().latin(), "Sara Haddad");
        assert_eq!(operator.name().arabic(), Some("سارة حداد"));
        assert_eq!(operator.role(), OperatorRole::Manager);
        assert!(operator.permissions().allows(Permission::VoidTransaction));
        // `updated_at` reached the authority, not just the row: 09:00 plus the policy's 24 hours.
        let Authority::OfflineCredential { not_after } = decided_by else {
            panic!("a locally decided PIN is not a platform decision: {decided_by:?}");
        };
        assert!(not_after.has_passed(
            "2026-08-24T09:00:01Z"
                .parse::<chrono::DateTime<Utc>>()
                .unwrap()
        ));
        assert!(!not_after.has_passed(
            "2026-08-24T08:59:59Z"
                .parse::<chrono::DateTime<Utc>>()
                .unwrap()
        ));
    }

    /// The NULL pass. Both nullable columns absent, and the row still reads correctly.
    #[test]
    fn the_offline_read_survives_its_two_nullable_columns_being_null() {
        let service = create_test_service();
        let hash = AuthService::hash_pin("4321").unwrap();
        {
            let conn = service.db.connection();
            let conn = conn.lock();
            conn.execute(
                r#"INSERT INTO operators
                   (id, code, name, name_ar, pin_hash, role, permissions_json, is_active)
                   VALUES ('op-nulls', 'CODE-NULLS', 'Sara Haddad', NULL, ?1, 'CASHIER', NULL, 1)"#,
                [&hash],
            )
            .unwrap();
        }

        let outcome = service.verify_pin_offline(&op_id("op-nulls"), &pin("4321"), &policy());

        let PinVerification::Accepted { operator, .. } = outcome else {
            panic!("two NULLs in nullable columns are not a broken row: {outcome:?}");
        };
        assert_eq!(operator.name().arabic(), None);
        // Absent permissions mean this operator holds nothing beyond ringing a sale — constructed
        // at a site that means it, never a `Default`.
        assert!(!operator.permissions().allows(Permission::VoidTransaction));
    }

    /// A `role` column outside the server's enum is a broken row, and a broken row is refused —
    /// before the PIN is compared, so a wrong PIN does not get a different answer.
    #[test]
    fn a_role_the_server_does_not_admit_is_a_broken_row_and_not_a_cashier() {
        let service = create_test_service();
        insert_operator(&service, "op-badrole", "1234", true);
        {
            let conn = service.db.connection();
            let conn = conn.lock();
            conn.execute(
                "UPDATE operators SET role = 'AUDITOR' WHERE id = 'op-badrole'",
                [],
            )
            .unwrap();
        }

        for entered in ["1234", "9999"] {
            let outcome =
                service.verify_pin_offline(&op_id("op-badrole"), &pin(entered), &policy());
            assert!(
                matches!(
                    outcome,
                    PinVerification::Undetermined(UndeterminedCause::StoreUnavailable(_))
                ),
                "`{entered}` against a broken row: got {outcome:?}"
            );
        }
    }

    fn insert_operator(service: &AuthService, id: &str, pin: &str, active: bool) {
        let conn = service.db.connection();
        let conn = conn.lock();
        let hash = AuthService::hash_pin(pin).unwrap();
        conn.execute(
            r#"INSERT INTO operators (id, code, name, pin_hash, role, is_active)
               VALUES (?1, ?2, 'Ahmed', ?3, 'CASHIER', ?4)"#,
            params![id, format!("C-{id}"), hash, active],
        )
        .unwrap();
    }

    #[test]
    fn a_correct_pin_offline_is_accepted_on_the_local_credential_authority() {
        let service = create_test_service();
        insert_operator(&service, "op1", "1234", true);

        let outcome = service.verify_pin_offline(&op_id("op1"), &pin("1234"), &policy());

        let PinVerification::Accepted {
            operator,
            decided_by,
        } = outcome
        else {
            panic!("got {outcome:?}");
        };
        assert_eq!(operator.name().latin(), "Ahmed");
        assert_eq!(operator.role(), OperatorRole::Cashier);
        // Never `Authority::Platform`: a shift opened against a locally verified PIN is a
        // different audit record, and the till uploads shifts.
        assert!(matches!(decided_by, Authority::OfflineCredential { .. }));
    }

    /// A local mismatch is `Undetermined`, **not** `WrongPin`.
    ///
    /// `WrongPin` is defined as the only refusal that spends the retry budget, and it carries the
    /// count that remains. This till keeps no ledger — `pos-db` has no lockout table, column or
    /// query — so answering `WrongPin` would mean fabricating a figure a cashier reads, or
    /// asserting a budget nothing enforces. The second is the bypass this issue is named for.
    #[test]
    fn a_local_mismatch_is_undetermined_because_nothing_counts_it() {
        let service = create_test_service();
        insert_operator(&service, "op1", "1234", true);

        let outcome = service.verify_pin_offline(&op_id("op1"), &pin("9999"), &policy());

        assert!(
            matches!(
                outcome,
                PinVerification::Undetermined(UndeterminedCause::ServerUnreachable)
            ),
            "got {outcome:?}"
        );
    }

    #[test]
    fn test_clear_session() {
        let service = create_test_service();

        // Insert a session
        {
            let conn = service.db.connection();
            let conn = conn.lock();
            conn.execute(
                r#"
                INSERT INTO terminal_config
                (id, terminal_id, terminal_code, hardware_id, session_token)
                VALUES (1, 'term1', 'TERM-001', 'HW123', 'token123')
                "#,
                [],
            )
            .unwrap();
        }

        // Clear session
        service.clear_session().unwrap();

        // Verify session is cleared
        let session = service.load_saved_session().unwrap();
        assert!(session.is_none());
    }

    #[test]
    fn test_update_session_token() {
        let service = create_test_service();

        // Insert initial config
        {
            let conn = service.db.connection();
            let conn = conn.lock();
            conn.execute(
                r#"
                INSERT INTO terminal_config
                (id, terminal_id, terminal_code, hardware_id, session_token)
                VALUES (1, 'term1', 'TERM-001', 'HW123', 'old-token')
                "#,
                [],
            )
            .unwrap();
        }

        // Update token
        service.update_session_token("new-token").unwrap();

        // Verify token updated
        let session = service.load_saved_session().unwrap().unwrap();
        assert_eq!(session.session_token, "new-token");
    }
}
