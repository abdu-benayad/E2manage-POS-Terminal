//! Authentication Service - Terminal and operator authentication
//!
//! Handles terminal login, operator PIN verification, and session management.
//! Supports both online verification (via API) and offline verification (via local DB).

use crate::operator_sign_in::OperatorSignIn;
use anyhow::{Context, Result};
use pos_api::{
    ApiClient, ApiFailure, HeartbeatRequest, HeartbeatResponse, LoginTerminalResponse,
    RefusalDetails, ServerErrorCode, SessionToken, TerminalStanding, VerifyPinResponse,
};
use pos_db::column::{operator_name, operator_role};
use pos_db::Database;
use pos_models::{
    Authority, OperatorId, OperatorName, OperatorPermissions, OperatorRole, Pin, PinPolicy,
    PinRefusal, PinVerification, StoreFailure, StoreFailureKind, UndeterminedCause,
    VerifiedOperator,
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
    /// The terminal's bearer credential.
    ///
    /// [`SessionToken`], not `String`, because the empty string was doing sentinel duty here:
    /// `load_saved_session` used to read `Option<String>`, `unwrap_or_default()` it, and then ask
    /// `is_empty()` to decide whether a session existed — so "there is no session" and "there is a
    /// session whose token is empty" were one value wearing two meanings, and every other reader
    /// got the second one without being told. A blank is now refused where it is read.
    pub session_token: SessionToken,
    pub company_id: String,
    pub branch_id: Option<String>,
    pub locale: String,
    pub currency: String,
    pub tax_rate: f64,
    pub tax_inclusive: bool,
    pub sector: String,
    pub features: Vec<String>,
}

/// The five columns the offline path reads, named rather than positional at the call.
///
/// A positional `row.get(n)` beside a tuple of the same arity is structurally blind to a dropped
/// column: remove one from the `SELECT` and every subsequent index silently shifts by one, with
/// the types still lining up. Naming each field where it is read does not fix that on its own —
/// `the_offline_read_takes_every_column_from_its_own_position` does, with a distinct value per
/// column — but it makes the shift visible in the diff instead of invisible in an index.
///
/// **There is no `pin_hash` here any more**, and that is the point of schema v13. See
/// [`AuthService::verify_pin_offline`].
///
/// `name` and `role` arrive as domain types because `pos_db::column` reads them that way, which is
/// what its helpers exist for: a role this till does not recognise means the contract moved, and
/// reading it as `Cashier` would be a privilege decision made by a fallback. Holding them as
/// `String` here would also re-open what
/// `tests/guards.rs::operator_identity_never_survives_as_a_bare_string` closes.
struct StoredOperator {
    name: OperatorName,
    role: OperatorRole,
    permissions_json: Option<String>,
    is_active: bool,
}

/// A column that held something outside the domain type it maps to.
///
/// A named error rather than a `String`, so `StoreFailure`'s source stays an error: the failure
/// this issue exists to remove is the one where an error becomes a message and stops being
/// branchable.
#[derive(Debug, thiserror::Error)]
#[error("{0}")]
struct UnreadableRow(String);

impl StoredOperator {
    /// Reads the row into the domain, fail-closed.
    ///
    /// The result is currently discarded by the one caller: with no credential to check a PIN
    /// against, a well-formed row still cannot produce a [`VerifiedOperator`]. It is called
    /// anyway, because a row this till cannot read is a different answer from one it can — the
    /// first is the till's own fault and reports as `Undetermined(StoreUnavailable)`, the second
    /// reports as `Undetermined(ServerUnreachable)`, and an operator staring at a till deserves to
    /// be told which. `offline-pin-verification-has-no-credential` is what starts using the value.
    fn into_verified(self, operator_id: &OperatorId) -> Result<VerifiedOperator, StoreFailure> {
        // Absent is a real state and means this operator holds nothing beyond ringing a sale. A
        // column that is present and unreadable is not: that is a broken row, and reading it as
        // "no permissions" would be a privilege decision made by a fallback.
        let permissions = match self.permissions_json {
            None => OperatorPermissions::none(),
            Some(json) => serde_json::from_str(&json).map_err(|e| {
                StoreFailure::new(
                    "reading the operator's row",
                    StoreFailureKind::RowUnreadable,
                )
                .caused_by(UnreadableRow(format!("the `permissions_json` column: {e}")))
            })?,
        };

        Ok(VerifiedOperator::from_verified_pin(
            operator_id.clone(),
            self.name,
            self.role,
            permissions,
        ))
    }
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

        // Refused before anything is stored: a login that answered 2xx with a blank token has
        // not logged this terminal in, and writing the row would leave a `terminal_config` that
        // `load_saved_session` then reports as a session.
        let session_token = SessionToken::new(response.session_token.clone())
            .context("the platform answered a terminal login with a blank session token")?;

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
            session_token,
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
                // Every column is read before the token is judged, deliberately. Returning early
                // on a blank token would leave the reads after it unexercised, and this row is
                // read by position — `test_load_saved_session_reads_every_column_into_its_own_field`
                // is what catches a shift, and it can only catch one it reaches.
                let terminal_id = row.get(0)?;
                let terminal_code = row.get(1)?;
                let hardware_id = row.get(2)?;
                let session_token = row.get::<_, Option<String>>(3)?;
                let company_id = row.get::<_, Option<String>>(4)?.unwrap_or_default();
                let branch_id = row.get(5)?;
                let locale = row
                    .get::<_, Option<String>>(6)?
                    .unwrap_or_else(|| "ar".to_string());
                let currency = row
                    .get::<_, Option<String>>(7)?
                    .unwrap_or_else(|| "LYD".to_string());
                let tax_rate = row.get::<_, Option<f64>>(8)?.unwrap_or(0.0);
                let tax_inclusive = row.get::<_, Option<i32>>(9)?.unwrap_or(0) != 0;
                let sector = row
                    .get::<_, Option<String>>(10)?
                    .unwrap_or_else(|| "RETAIL".to_string());

                // NULL and `""` are the same answer — *there is no session* — and they say it
                // here, once, instead of downstream in an `is_empty()` every reader had to
                // remember.
                let Ok(session_token) = SessionToken::new(session_token.unwrap_or_default()) else {
                    return Ok(None);
                };

                Ok(Some(TerminalSession {
                    terminal_id,
                    terminal_code,
                    hardware_id,
                    session_token,
                    company_id,
                    branch_id,
                    locale,
                    currency,
                    tax_rate,
                    tax_inclusive,
                    sector,
                    features: vec![], // Features need to be synced
                }))
            },
        );

        match result {
            Ok(session) => Ok(session),
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
            Ok(verified) => self.accepted_by_platform(operator_id, verified).await,
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
            Err(refusal @ ApiFailure::Refused { .. }) => {
                self.read_the_refusal(refusal, operator_id, pin, policy)
                    .await
            }
        }
    }

    /// A refusal, split by whether it was about the **request** or about the **terminal**.
    ///
    /// `SyncService::is_auth_error` recovers the status by substring-matching `"401"` and gives
    /// everything that matches one response. Four different situations arrive that way and only
    /// one of them is worth retrying — see [`TerminalStanding`], which is where that table lives.
    ///
    /// # `Repudiated` never reaches the local leg
    ///
    /// That is the one branch where the till would override a decision the server actually made:
    /// the platform was reached, it answered, and the answer was that this device is not one of
    /// theirs. Falling back to a local credential there is the bypass this issue is named for,
    /// wearing a different hat. Note which arms can reach [`Self::verify_pin_offline`] — only the
    /// one where a renewal found nobody home.
    async fn read_the_refusal(
        &self,
        refusal: ApiFailure,
        operator_id: &OperatorId,
        pin: &Pin,
        policy: &PinPolicy,
    ) -> PinVerification {
        match TerminalStanding::of(&refusal) {
            // The refusal was about the PIN, not about the device.
            TerminalStanding::Unaffected => Self::refused_by_platform(refusal),
            TerminalStanding::NotProvisioned => {
                warn!("the platform holds no secret for this terminal; it must be paired again");
                PinVerification::Undetermined(UndeterminedCause::TerminalNotProvisioned)
            }
            TerminalStanding::Repudiated(repudiation) => {
                error!("the platform has disowned this terminal: {repudiation}");
                PinVerification::Undetermined(UndeterminedCause::EnrolmentRepudiated(repudiation))
            }
            TerminalStanding::SessionLapsed => {
                self.after_renewing_the_session(operator_id, pin, policy)
                    .await
            }
        }
    }

    /// Renews the terminal session **once**, then asks the platform **once**.
    ///
    /// # Once, and structurally so
    ///
    /// A retry loop against a server that keeps answering 401 is a lockout amplifier: the endpoint
    /// behind it counts failed attempts against the operator, so a till that retries on the
    /// operator's behalf spends their budget without them touching the keypad. The second attempt
    /// is written out in full below rather than reached by recursing into [`Self::verify_pin`],
    /// which is what makes "exactly once" a property of the shape and not of a counter somebody
    /// has to maintain.
    ///
    /// # A refused renewal is very likely a disowned terminal
    ///
    /// `terminal-auth.middleware.ts:76` tests `revokedAt` before `:81` tests `terminal.status`,
    /// with a comment recording the order as known-wrong — so a terminal that has been withdrawn
    /// *and* whose session lapsed reports as merely expired. That is why a refusal here is never
    /// treated as a second chance. When the refusal names a standing the till says so; when it
    /// does not, [`UndeterminedCause::ReauthFailed`] is the honest reading — *the session could
    /// not be renewed* — rather than a guess at which flavour of repudiation it was.
    async fn after_renewing_the_session(
        &self,
        operator_id: &OperatorId,
        pin: &Pin,
        policy: &PinPolicy,
    ) -> PinVerification {
        match self.api.refresh_session().await {
            Ok(_) => {
                info!("the terminal session lapsed and was renewed; asking again, once");
            }
            // Nobody answered the renewal. The platform has made no claim about this terminal, so
            // this is the ordinary weather case and the only path from here to the local leg.
            Err(ApiFailure::Unreachable(error)) => {
                debug!("the session lapsed and the platform is now unreachable: {error}");
                return self.verify_pin_offline(operator_id, pin, policy);
            }
            Err(ApiFailure::Unreadable(error)) => {
                error!("contract breach renewing the terminal session: {error}");
                return PinVerification::Undetermined(UndeterminedCause::contract_breach(error));
            }
            Err(refusal @ ApiFailure::Refused { .. }) => {
                return PinVerification::Undetermined(match TerminalStanding::of(&refusal) {
                    TerminalStanding::Repudiated(repudiation) => {
                        UndeterminedCause::EnrolmentRepudiated(repudiation)
                    }
                    TerminalStanding::NotProvisioned => UndeterminedCause::TerminalNotProvisioned,
                    TerminalStanding::SessionLapsed | TerminalStanding::Unaffected => {
                        UndeterminedCause::ReauthFailed
                    }
                });
            }
        }

        match self
            .api
            .verify_operator_pin(operator_id, pin.expose_digits())
            .await
        {
            Ok(verified) => self.accepted_by_platform(operator_id, verified).await,
            Err(ApiFailure::Unreachable(error)) => {
                debug!("the platform went away between the renewal and the retry: {error}");
                self.verify_pin_offline(operator_id, pin, policy)
            }
            Err(ApiFailure::Unreadable(error)) => {
                error!("contract breach verifying a PIN after a renewal: {error}");
                PinVerification::Undetermined(UndeterminedCause::contract_breach(error))
            }
            Err(refusal @ ApiFailure::Refused { .. }) => match TerminalStanding::of(&refusal) {
                TerminalStanding::Unaffected => Self::refused_by_platform(refusal),
                TerminalStanding::NotProvisioned => {
                    PinVerification::Undetermined(UndeterminedCause::TerminalNotProvisioned)
                }
                TerminalStanding::Repudiated(repudiation) => PinVerification::Undetermined(
                    UndeterminedCause::EnrolmentRepudiated(repudiation),
                ),
                // A session minted seconds ago, rejected. Not weather, and not a third attempt:
                // this is where the loop would be, and it is deliberately not here.
                TerminalStanding::SessionLapsed => {
                    error!("a freshly renewed terminal session was rejected immediately");
                    PinVerification::Undetermined(UndeterminedCause::ReauthFailed)
                }
            },
        }
    }

    /// Builds the accepted outcome **from the response body**.
    ///
    /// The deleted `get_operator_info` read the local `operators` table here — a second fallible
    /// read that re-graded the server's answer against a cache, and silently demoted a
    /// server-confirmed operator whose row had not synced yet. The platform just told the till who
    /// this is; asking the till's own stale copy to confirm it is how a correct PIN produces
    /// "operator not found".
    async fn accepted_by_platform(
        &self,
        operator_id: &OperatorId,
        verified: VerifyPinResponse,
    ) -> PinVerification {
        // A till always presents `X-Terminal-Token`, so the server always mints a session. Absent
        // means this request went out without one, which is a bug on this side of the wire, not a
        // branch to write a fallback for.
        match &verified.session {
            Some(session) => {
                debug!(
                    "operator session minted, expiring {}",
                    session.expires_at().to_rfc3339()
                );
                self.operator_sign_in()
                    .record_and_present(operator_id, session)
                    .await;
            }
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

    /// Who is signed in at this till, over the same client and store this service holds.
    ///
    /// Built per call rather than stored: [`OperatorSignIn`] is two `Arc` clones and no state of
    /// its own, and keeping a fourth field in sync with the two it is made of would be the kind of
    /// duplicated truth this whole task exists to remove.
    pub fn operator_sign_in(&self) -> OperatorSignIn {
        OperatorSignIn::new(Arc::clone(&self.api), Arc::clone(&self.db))
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

    /// What this till can say about a PIN with the platform unreachable, which is no longer *is
    /// it correct*.
    ///
    /// # The till holds no credential to check a PIN against, and has not for some time
    ///
    /// The platform withdrew `pinHash` from `GET /api/pos/sync/operators` and asserts the negative
    /// on the wire. The till kept declaring the field and defaulting it, so every synced operator
    /// carried `""` as their bcrypt hash, and every offline attempt ran `bcrypt::verify(pin, "")`
    /// — which fails, was read as a **wrong PIN**, and was charged to that operator's lockout
    /// budget. A shop with no network could not open, and each cashier who tried was locked out
    /// for trying. Design v8's thesis in one line: *offline is a fallback for unreachable, never
    /// for rejected*, and line 334 rejected.
    ///
    /// So `pin_hash` is **deleted**, not repaired — a column whose every value is `""` is not
    /// data — and this function stops claiming to verify anything.
    ///
    /// # The trade, stated where it is made
    ///
    /// A till with no network **cannot verify any PIN at all** until
    /// `offline-pin-verification-has-no-credential` lands. That is a real regression in
    /// capability. It is not a regression in behaviour: offline verification did not work before
    /// this either — it lied about why, and locked people out permanently while doing so. A
    /// refusal a shop can act on beats a lockout it cannot.
    ///
    /// # What it can still answer, and why that is worth keeping
    ///
    /// `operators` still carries identity and `is_active`, so an operator this till has never
    /// heard of and one whose employment ended are still distinguishable — and telling a cashier
    /// "no such operator here" with the network down is strictly better than telling them nothing.
    /// Neither consumes an attempt. [`UndeterminedCause::ServerUnreachable`] is reserved for a
    /// known, active operator whose PIN simply cannot be checked; its own doc already reads *"the
    /// platform could not be reached, and no local credential could settle the PIN"*, which is
    /// exactly this world, and both halves are true because this leg runs only when the first is.
    ///
    /// # Nothing here can spend the lockout budget, and no guard enforces that
    ///
    /// `consumes_an_attempt` exists only on [`PinRefusal`]. An `Undetermined` is not one, so it is
    /// *structurally incapable* of spending an attempt — there is no method to call. That is the
    /// fix. A check that the outcome does not consume an attempt would be a rule someone could
    /// forget to run; a type that has no such method is a rule nobody can break.
    pub fn verify_pin_offline(
        &self,
        operator_id: &OperatorId,
        _pin: &Pin,
        _policy: &PinPolicy,
    ) -> PinVerification {
        let stored = {
            let conn = self.db.connection();
            let conn = conn.lock();
            conn.query_row(
                "SELECT name, name_ar, role, permissions_json, is_active FROM operators \
                 WHERE id = ?1",
                [operator_id.as_str()],
                |row| {
                    Ok(StoredOperator {
                        // Two indices, because `name` and `name_ar` are one value: read
                        // separately they can drift into a row the domain says cannot exist.
                        name: operator_name(row, 0, 1)?,
                        role: operator_role(row, 2)?,
                        permissions_json: row.get(3)?,
                        is_active: row.get(4)?,
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
                // A column the domain type does not admit arrives as `FromSqlConversionFailure`
                // from `pos_db::column` — a broken row rather than a failed query. Both are the
                // till's fault, and neither is the operator's.
                return Self::store_failed("reading the operator's row", error);
            }
        };

        if !stored.is_active {
            // A distinct refusal, not the same empty row. Nothing the person at the till types
            // could change the answer, so it consumes no attempt either.
            return PinVerification::Refused(PinRefusal::OperatorInactive);
        }

        // The row parses and the operator is real and active. There is simply nothing here to
        // check a PIN against. `_pin` is not compared, and is named with a leading underscore so
        // that stays obvious rather than being something a reader has to notice by its absence.
        if let Err(failure) = stored.into_verified(operator_id) {
            return PinVerification::Undetermined(failure.into());
        }

        warn!(
            "the platform is unreachable and this till holds no credential for {operator_id}: \
             no PIN can be verified until one is issued"
        );
        PinVerification::Undetermined(UndeterminedCause::ServerUnreachable)
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
    pub fn update_session_token(&self, token: &SessionToken) -> Result<()> {
        let conn = self.db.connection();
        let conn = conn.lock();

        conn.execute(
            "UPDATE terminal_config SET session_token = ?1, updated_at = datetime('now') WHERE id = 1",
            params![token.expose()],
        )?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pos_db::init_memory_database;
    use pos_models::{
        LockoutPeriod, MaxAttempts, OfflineWindow, PinLength, RequiredPinLength, SessionLifetime,
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
        assert_eq!(session.session_token.expose(), "session-token-1");
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
        insert_operator(&service, "op-inactive", false);

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

    /// The outcome for a known, active operator: the till holds nothing to check a PIN against.
    ///
    /// Not `WrongPin`, which is what this answered for every operator on every till from the
    /// moment the platform withdrew `pinHash` — `bcrypt::verify(pin, "")` fails, and
    /// `unwrap_or(false)` reported that as a mistyped PIN and charged it to the lockout budget.
    /// A shop with no network could not open, and every cashier who tried was locked out for it.
    #[test]
    fn a_known_active_operator_offline_is_undetermined_because_nothing_can_check_the_pin() {
        let service = create_test_service();
        insert_operator(&service, "op1", true);

        // Any PIN, correct or not: there is no credential, so the digits are never compared.
        for entered in ["1234", "9999"] {
            let outcome = service.verify_pin_offline(&op_id("op1"), &pin(entered), &policy());

            assert!(
                matches!(
                    outcome,
                    PinVerification::Undetermined(UndeterminedCause::ServerUnreachable)
                ),
                "`{entered}` offline: got {outcome:?}"
            );
        }
    }

    /// **The assertion that matters.** No offline outcome can spend the lockout budget.
    ///
    /// `consumes_an_attempt` exists only on [`PinRefusal`], so an `Undetermined` is structurally
    /// incapable of it — this test cannot even ask. What it can assert is the other half: every
    /// refusal the offline leg *can* produce answers false. Together with the type, that is the
    /// whole of "a network outage never locks anybody out".
    #[test]
    fn no_offline_outcome_spends_an_attempt() {
        let service = create_test_service();
        insert_operator(&service, "op-active", true);
        insert_operator(&service, "op-inactive", false);

        for operator in ["op-active", "op-inactive", "op-never-heard-of"] {
            let outcome = service.verify_pin_offline(&op_id(operator), &pin("1234"), &policy());

            match outcome {
                PinVerification::Refused(refusal) => assert!(
                    !refusal.consumes_an_attempt(),
                    "{operator} offline produced {refusal:?}, which spends the budget"
                ),
                // The other two arms cannot spend an attempt: `Undetermined` has no such method,
                // and `Accepted` is not a refusal. Listed rather than wildcarded so a new outcome
                // has to be considered here.
                PinVerification::Undetermined(_) | PinVerification::Accepted { .. } => {}
            }
        }
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
    /// The row is read even though nothing can verify a PIN against it, and the assertions reach
    /// it through the *failure* modes rather than through an `Accepted`: a row this till cannot
    /// read is `StoreUnavailable`, and a row it can read is `ServerUnreachable`. That distinction
    /// is what the read is still for, and it is what these tests hold in place until
    /// `offline-pin-verification-has-no-credential` gives the parsed operator a consumer again.
    #[test]
    fn the_offline_read_takes_every_column_from_its_own_position() {
        let service = create_test_service();
        {
            let conn = service.db.connection();
            let conn = conn.lock();
            conn.execute(
                r#"INSERT INTO operators
                   (id, code, name, name_ar, role, permissions_json, is_active, updated_at)
                   VALUES ('op-distinct', 'CODE-DISTINCT', 'Sara Haddad', 'سارة حداد',
                           'MANAGER', '{"canVoid":true}', 1, '2026-08-23 09:00:00')"#,
                [],
            )
            .unwrap();
        }

        // Read correctly: a well-formed, active row reaches the "nothing to check against" answer.
        let outcome = service.verify_pin_offline(&op_id("op-distinct"), &pin("4321"), &policy());
        assert!(
            matches!(
                outcome,
                PinVerification::Undetermined(UndeterminedCause::ServerUnreachable)
            ),
            "got {outcome:?}"
        );

        // Now each column in turn, corrupted where it sits. A shifted index reads a *different*
        // column, so a corruption planted in one and reported from another is exactly the failure
        // distinct values expose.
        for (column, value, expected) in [
            // `name` blank: the domain refuses a nameless operator.
            ("name", "''", Corrupted::Unreadable),
            // `role` outside the server's enum: a broken row, never a `Cashier` by fallback.
            ("role", "'AUDITOR'", Corrupted::Unreadable),
            // `permissions_json` present and unparseable: a broken row, never "no permissions".
            (
                "permissions_json",
                "'{\"canVoid\": '",
                Corrupted::Unreadable,
            ),
            // `is_active` false: a *refusal*, and a different one from unknown.
            ("is_active", "0", Corrupted::Inactive),
        ] {
            let service = create_test_service();
            {
                let conn = service.db.connection();
                let conn = conn.lock();
                conn.execute(
                    r#"INSERT INTO operators
                       (id, code, name, name_ar, role, permissions_json, is_active, updated_at)
                       VALUES ('op-distinct', 'CODE-DISTINCT', 'Sara Haddad', 'سارة حداد',
                               'MANAGER', '{"canVoid":true}', 1, '2026-08-23 09:00:00')"#,
                    [],
                )
                .unwrap();
                conn.execute(
                    &format!("UPDATE operators SET {column} = {value} WHERE id = 'op-distinct'"),
                    [],
                )
                .unwrap();
            }

            let outcome =
                service.verify_pin_offline(&op_id("op-distinct"), &pin("4321"), &policy());
            match expected {
                Corrupted::Unreadable => assert!(
                    matches!(
                        outcome,
                        PinVerification::Undetermined(UndeterminedCause::StoreUnavailable(_))
                    ),
                    "a corrupt `{column}` must read as a broken row: got {outcome:?}"
                ),
                Corrupted::Inactive => assert!(
                    matches!(
                        outcome,
                        PinVerification::Refused(PinRefusal::OperatorInactive)
                    ),
                    "`is_active = 0` is a refusal, not a broken row: got {outcome:?}"
                ),
            }
        }
    }

    /// What a corrupted column should produce.
    enum Corrupted {
        Unreadable,
        Inactive,
    }

    /// The NULL pass. Both nullable columns absent, and the row still reads correctly.
    ///
    /// A shift that lands a NULL where a NOT NULL column was expected fails differently from one
    /// that lands a value — which is why this is a separate case and not a variation of the one
    /// above.
    #[test]
    fn the_offline_read_survives_its_two_nullable_columns_being_null() {
        let service = create_test_service();
        {
            let conn = service.db.connection();
            let conn = conn.lock();
            conn.execute(
                r#"INSERT INTO operators
                   (id, code, name, name_ar, role, permissions_json, is_active)
                   VALUES ('op-nulls', 'CODE-NULLS', 'Sara Haddad', NULL, 'CASHIER', NULL, 1)"#,
                [],
            )
            .unwrap();
        }

        let outcome = service.verify_pin_offline(&op_id("op-nulls"), &pin("4321"), &policy());

        assert!(
            matches!(
                outcome,
                PinVerification::Undetermined(UndeterminedCause::ServerUnreachable)
            ),
            "two NULLs in nullable columns are not a broken row: got {outcome:?}"
        );
    }

    fn insert_operator(service: &AuthService, id: &str, active: bool) {
        let conn = service.db.connection();
        let conn = conn.lock();
        conn.execute(
            r#"INSERT INTO operators (id, code, name, role, is_active)
               VALUES (?1, ?2, 'Ahmed', 'CASHIER', ?3)"#,
            params![id, format!("C-{id}"), active],
        )
        .unwrap();
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
        service
            .update_session_token(&SessionToken::new("new-token").expect("not blank"))
            .unwrap();

        // Verify token updated
        let session = service.load_saved_session().unwrap().unwrap();
        assert_eq!(session.session_token.expose(), "new-token");
    }
}
