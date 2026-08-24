//! Pairing Service - Terminal registration and pairing workflow
//!
//! Handles the terminal pairing flow:
//! 1. Check if terminal is registered
//! 2. Get hardware ID
//! 3. Request pairing code from server
//! 4. Poll for pairing completion
//! 5. Save registration credentials

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use pos_api::{
    ApiClient, DeviceInfo, HardwareInfo, OsInfo, PairedTerminalInfo, PairingStatus,
    RegisterDeviceRequest,
};
use pos_db::projection::{optional_scalar, read_one, write};
use pos_db::terminal::{TerminalConfigRow, TERMINAL_CONFIG_ROW, TERMINAL_REGISTRATION_ROW};
use pos_db::Database;
use pos_models::HardwareEnrolment;
use rusqlite::params;
use std::sync::Arc;
use tracing::{debug, info, warn};

/// What the terminal registration row says about this till's enrolment.
///
/// # Why this is a sum and not a struct with a flag
///
/// It was a struct carrying `is_registered: bool` beside `secret: Option<String>`,
/// `terminal_id: Option<String>` and `terminal_code: Option<String>`. That makes
/// `TerminalRegistration { is_registered: true, secret: None }` **representable**, and it is
/// nonsense: the only writer of `is_registered = 1` is [`PairingService::save_registration`],
/// which sets the credential and both identifiers in the same statement. A shape that can express
/// a state its own writers cannot produce is a shape that invites a reader to handle it, and the
/// handling is always a guess.
///
/// The flag was worse than redundant — it was **constant**. `get_registration` returned `Some`
/// only for a row with `is_registered = 1`, so `registration.is_registered` was `true` on every
/// value that ever existed. A field whose value is fixed by the only path that constructs it
/// is not data; it is a comment that the compiler cannot check.
///
/// # What each variant is allowed to carry, and the one that is a deliberate omission
///
/// The unenrolled row is **not** empty during pairing: `save_registration` is reached before the
/// server has confirmed anything, so `terminal_id`, `terminal_code` and `secret` can all sit on a
/// row whose `is_registered` is still `0` — the case
/// `get_registration_is_unenrolled_while_the_row_is_not_registered` exists for. The old return
/// type suppressed that by answering `None` and discarding the row wholesale. [`Self::Unenrolled`]
/// carries the hardware id and nothing else, so an unconfirmed identity is not *filtered out* on
/// the way to the caller — it is **unrepresentable** in what the caller receives.
///
/// `registered_at` stays optional on [`Self::Enrolled`] on purpose. It is neither a credential nor
/// an identifier, and a till that cannot say *when* it enrolled is still enrolled; requiring it
/// would let a missing display timestamp stop the till reading its own registration. This is the
/// same reflex [`PairingService::clear_registration`] warns about with `hardware_id` — a list that
/// looks incomplete is not an invitation to complete it.
///
/// `license_key` is not on either variant, and that is an open question rather than a decision —
/// see the note on [`PairingService::get_platform_license`].
///
/// # The assembly test has no socket to plug into yet, and that is a finding, not a pass
///
/// The rule is that wiring a type into its caller must need no `.unwrap()`, no forced conversion
/// and no adapter shim. **[`PairingService::get_registration`] has no production caller** — every
/// call to it is in a `#[cfg(test)]` module in this file (verified by a repo-wide search that
/// included `crates/pos-updater` and `crates/pos-contract`, which are excluded from the workspace
/// and therefore invisible to every `cargo --workspace` command). The view layer that would have
/// held the other end went with the previous UI; its replacement is the `egui-auth-screen` issue.
///
/// So this type met the compiler and not a consumer. What was actually exercised is the two-arm
/// `match` in the tests, which is the shape a caller will write and is not the same evidence.
/// Whoever builds the auth screen is running the real assembly test, and a socket that turns out
/// to be wrong there is a signal about this type rather than a reason to write a shim.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TerminalRegistration {
    /// This till holds no completed enrolment.
    ///
    /// `hardware_id` is `None` when the till has not generated one yet. Absent row, SQL `NULL`
    /// and the empty string the schema seeds all fold to `None` here, matching the three-way
    /// reading [`PairingService::get_hardware_id`] already makes — the store spells "unset" three
    /// ways and the domain has one.
    Unenrolled {
        /// The device identity, if this till has generated one.
        hardware_id: Option<String>,
    },
    /// The platform has enrolled this till and issued it a credential.
    Enrolled {
        /// The device identity this till enrolled under.
        hardware_id: String,
        /// Terminal ID assigned by the server.
        terminal_id: String,
        /// Terminal code (e.g., "TERM-001").
        terminal_code: String,
        /// Secret for authentication.
        secret: String,
        /// Company name, for display. Genuinely optional: the pairing response declares it
        /// `Option<String>` (`PairedTerminalInfo::company_name`).
        company_name: Option<String>,
        /// When the enrolment was recorded, if it was recorded.
        registered_at: Option<String>,
    },
}

/// The registration row as SQLite hands it over, before anything is asserted about it.
///
/// Separated from [`TerminalRegistration`] because the two answer different questions. This one
/// says what is *stored* — every column nullable, because every column except `hardware_id` is
/// nullable in the schema and `hardware_id` is seeded empty. [`TerminalRegistration`] says what is
/// *true of the enrolment*, which is a claim the row has to earn.
///
/// It exists so the `query_row` closure stays a transport step: that closure can only fail with a
/// [`rusqlite::Error`], so a domain rule expressed inside it would have to be spelled as a type
/// error or a panic. The rule lives in [`Self::into_registration`] instead, where it can name the
/// column that is wrong.
struct RegistrationRow {
    hardware_id: Option<String>,
    terminal_id: Option<String>,
    terminal_code: Option<String>,
    secret: Option<String>,
    company_name: Option<String>,
    is_registered: bool,
    registered_at: Option<String>,
}

/// SQLite spells "this column holds nothing" as `NULL` and this table also spells it as the empty
/// string — the schema seeds `hardware_id` to `''` and a pairing response can carry
/// `secret: ""` (`PairedTerminalInfo::secret` is `String`, and `check_pairing_status` warns about
/// an empty one *after* `save_registration` has already stored it).
///
/// So a check that only rejects `NULL` rejects the case that cannot happen and accepts the case
/// that does. Both fold to `None` here, matching the `!id.is_empty()` reading
/// [`PairingService::get_hardware_id`] already applies to this same table.
fn stored(value: Option<String>) -> Option<String> {
    value.filter(|v| !v.is_empty())
}

impl RegistrationRow {
    /// Decides which enrolment this row describes, or refuses to describe one.
    ///
    /// The refusal is the point. A row marked enrolled but missing a credential is not a till in
    /// a slightly degraded state that a caller can paper over — it is a row whose two halves
    /// contradict each other, and every value a reader could invent for the missing half is a
    /// guess about a credential.
    fn into_registration(self) -> Result<TerminalRegistration> {
        let hardware_id = stored(self.hardware_id);

        if !self.is_registered {
            return Ok(TerminalRegistration::Unenrolled { hardware_id });
        }

        match (
            hardware_id,
            stored(self.terminal_id),
            stored(self.terminal_code),
            stored(self.secret),
        ) {
            (Some(hardware_id), Some(terminal_id), Some(terminal_code), Some(secret)) => {
                Ok(TerminalRegistration::Enrolled {
                    hardware_id,
                    terminal_id,
                    terminal_code,
                    secret,
                    company_name: stored(self.company_name),
                    registered_at: stored(self.registered_at),
                })
            }
            (hardware_id, terminal_id, terminal_code, secret) => {
                let missing = [
                    ("hardware_id", hardware_id.is_none()),
                    ("terminal_id", terminal_id.is_none()),
                    ("terminal_code", terminal_code.is_none()),
                    ("secret", secret.is_none()),
                ]
                .into_iter()
                .filter(|(_, absent)| *absent)
                .map(|(column, _)| column)
                .collect::<Vec<_>>();

                anyhow::bail!(
                    "the terminal_registration row is marked enrolled but {} empty or NULL; an \
                     enrolled terminal holds the credential and identifiers the platform issued \
                     it, so nothing can be reported from this row without inventing one of them. \
                     Re-pair the terminal to replace the row",
                    match missing.as_slice() {
                        [one] => format!("{one} is"),
                        many => format!("{} are", many.join(", ")),
                    }
                )
            }
        }
    }
}

/// Current pairing state
#[derive(Debug, Clone)]
pub struct PairingState {
    /// The current pairing code
    pub pairing_code: String,
    /// When it expires
    pub expires_at: DateTime<Utc>,
    /// Current status
    pub status: PairingStatus,
    /// Hardware ID
    pub hardware_id: String,
    /// Whether the platform considers this hardware already enrolled.
    ///
    /// Read from the platform and never inferred here — see [`HardwareEnrolment`] for why the
    /// local store cannot answer it. `Undetermined` until the first status poll, because the
    /// pairing-request response carries no enrolment signal at all.
    pub enrolment: HardwareEnrolment,
}

/// Service for managing terminal pairing/registration
pub struct PairingService {
    api: Arc<ApiClient>,
    db: Arc<Database>,
}

impl PairingService {
    /// Creates a new pairing service
    pub fn new(api: Arc<ApiClient>, db: Arc<Database>) -> Self {
        Self { api, db }
    }

    // ========================================================================
    // REGISTRATION CHECK
    // ========================================================================

    /// Whether this till holds a completed enrolment.
    ///
    /// # A read that fails is not a till that is unenrolled
    ///
    /// This was `.unwrap_or(0)`, which reported **every** error as "not registered" — including the
    /// ones that mean the store could not answer. A till is then wrong about itself while the row
    /// sits intact, and [`Self::get_hardware_id`] compounds it by overwriting the identity the
    /// platform knows this till by. Three outcomes, and only two of them are answers:
    ///
    /// - the row is absent, which is a fresh install and genuinely means not registered;
    /// - the flag is SQL `NULL`, which the schema's `DEFAULT 0` makes equivalent to unset;
    /// - anything else, which is the store failing and belongs to the caller.
    pub fn is_registered(&self) -> Result<bool> {
        let conn = self.db.connection();
        let conn = conn.lock();

        let flag = optional_scalar::<Option<i32>>(
            &conn,
            "SELECT is_registered FROM terminal_registration WHERE id = 1",
            [],
        );

        match flag {
            // Two `None`s and they mean different things, which is why the type keeps them apart:
            // the outer is *no row* — a fresh install — and the inner is SQL `NULL`, which the
            // schema's `DEFAULT 0` makes equivalent to unset. Both answer "not registered". Only
            // an `Err` is the store failing, and that belongs to the caller.
            Ok(stored) => Ok(stored.flatten() == Some(1)),
            Err(e) => Err(e).context(
                "could not read the terminal registration flag; this is the store failing to \
                 answer, not a till that is unregistered",
            ),
        }
    }

    /// What this till's registration row says about its enrolment.
    ///
    /// # There is no `Option` any more, because the absence had a name
    ///
    /// This returned `Result<Option<TerminalRegistration>>`, and `None` meant three different
    /// things: no row, a row with `is_registered = 0`, and a row the caller was not allowed to
    /// see. Two of those are the same domain fact — this till is not enrolled — and
    /// [`TerminalRegistration::Unenrolled`] states it, carrying the hardware id that the discarded
    /// row held. The `Result` still carries the third outcome, a store that will not answer,
    /// which is not a statement about enrolment (`b10592a`, same argument as
    /// [`Self::is_registered`]).
    ///
    /// # The row is read by position
    ///
    /// A column added, removed or reordered in the `SELECT` list shifts every index after it, and
    /// five of the seven are TEXT, so a swap among them type-checks and simply attributes one
    /// terminal's details to another. `get_registration_reads_every_column_into_its_own_field`
    /// pins the mapping with a distinct value per column. `positional-row-access-in-pos-db` is
    /// migrating this shape workspace-wide and reaches this file at its task 13.
    pub fn get_registration(&self) -> Result<TerminalRegistration> {
        let conn = self.db.connection();
        let conn = conn.lock();

        // One projection of `terminal_registration`, shared with the two credential reads below.
        // It names `license_key` as well, which none of the three hand-written lists did.
        let result = read_one(
            &conn,
            TERMINAL_REGISTRATION_ROW.reader(),
            "FROM terminal_registration WHERE id = 1",
            [],
        )
        .map(|row| {
            row.map(|row| RegistrationRow {
                hardware_id: row.hardware_id,
                terminal_id: row.terminal_id,
                terminal_code: row.terminal_code,
                secret: row.secret,
                company_name: row.company_name,
                is_registered: row.is_registered == Some(1),
                registered_at: row.registered_at,
            })
        });

        match result {
            Ok(Some(row)) => row.into_registration(),
            // The schema seeds row 1, so an absent row means something removed it. That is still
            // not an enrolment, and it is not an identity either. `read_one` already separates
            // this from a failure — it is the only error `.optional()` folds into `None` — so the
            // three outcomes stay three, as they were when the match spelled it out.
            Ok(None) => Ok(TerminalRegistration::Unenrolled { hardware_id: None }),
            Err(e) => Err(e).context(
                "could not read the terminal registration row; this is the store failing to \
                 answer, not a till that is unenrolled",
            ),
        }
    }

    /// Gets the hardware ID for this terminal
    pub fn get_hardware_id(&self) -> Result<String> {
        // Try to get from database first
        let conn = self.db.connection();
        let conn = conn.lock();

        // Absent, empty and NULL all mean "this till has not generated one yet" and fall through.
        // A read that *failed* means something else entirely and must not: generating a fresh id
        // here writes it over the identity the platform knows this till by.
        let stored = optional_scalar::<Option<String>>(
            &conn,
            "SELECT hardware_id FROM terminal_registration WHERE id = 1",
            [],
        );

        match stored.map(Option::flatten) {
            Ok(Some(id)) if !id.is_empty() => return Ok(id),
            // Absent row, SQL `NULL` and empty string all mean "not generated yet" and fall
            // through together — `flatten` collapses the first two because nothing here needs to
            // tell them apart, and the guard above covers the third.
            Ok(_) => {}
            Err(e) => {
                return Err(e).context(
                    "could not read the stored hardware id; refusing to generate a replacement, \
                     because doing so would overwrite the identity this till is enrolled under",
                )
            }
        }

        // Generate a new hardware ID
        let hardware_id = generate_hardware_id();

        // Store it
        drop(conn);
        self.save_hardware_id(&hardware_id)?;

        Ok(hardware_id)
    }

    /// Writes the hardware ID onto the singleton registration row, touching nothing else.
    ///
    /// # `UPDATE`, deliberately, and never `INSERT OR REPLACE`
    ///
    /// This was `INSERT OR REPLACE INTO terminal_registration (id, hardware_id, is_registered)`,
    /// which is not an upsert of the named columns: SQLite deletes the conflicting row and inserts
    /// a new one, so **every column the statement does not name is reset**. Three of the columns it
    /// did not name are the terminal secret, the platform licence key, and the company association.
    ///
    /// That only matters because the caller can reach here holding a live registration.
    /// [`Self::get_hardware_id`] guards this call on the stored id being empty, and reads that id
    /// through a combinator that turns *any* failure into an empty string — so a read that fails
    /// rather than a row that is absent sends a registered till down this path and it emerges with
    /// its credentials gone. The recovery route for a lost secret was removed on purpose (see
    /// [`Self::request_pairing_code`]), which makes that state terminal.
    ///
    /// An `UPDATE` cannot express that mistake. The row is seeded by the schema, so there is no
    /// insert to perform, and `is_registered` is left alone because recording a hardware id is not
    /// a statement about enrolment.
    fn save_hardware_id(&self, hardware_id: &str) -> Result<()> {
        let conn = self.db.connection();
        let conn = conn.lock();

        let updated = conn.execute(
            "UPDATE terminal_registration SET hardware_id = ?1 WHERE id = 1",
            [hardware_id],
        )?;

        // An UPDATE matching nothing succeeds, so the swallowed failure this method exists to stop
        // would come straight back one layer down. The schema seeds row 1; zero rows means the
        // store is not the one this code was written against, and that is worth saying out loud.
        anyhow::ensure!(
            updated == 1,
            "the singleton terminal_registration row (id = 1) is missing, so the hardware id could \
             not be stored; the schema seeds this row and something has removed it"
        );

        Ok(())
    }

    // ========================================================================
    // PAIRING WORKFLOW
    // ========================================================================

    /// Requests a new pairing code from the server.
    ///
    /// **Hardware the platform already knows is not a special case here.** It answers 200 with the
    /// same `{pairingCode, expiresAt, hardwareId}` body a first enrolment gets, flags the request
    /// as a re-pair on its own side, and an administrator authorises it. The till displays the code
    /// and polls, exactly as it does for a first enrolment.
    ///
    /// The till used to detect that case by substring-matching a prose error message and then ask
    /// the platform to hand its own secret back. Both halves are gone. Self-service recovery cannot
    /// be made safe, only removed: **a till that has lost its secret is indistinguishable from an
    /// attacker claiming that hardware id** — whatever the device could prove is the thing it lost.
    pub async fn request_pairing_code(&self) -> Result<PairingState> {
        let hardware_id = self.get_hardware_id()?;

        let device_info = DeviceInfo {
            os_name: Some(std::env::consts::OS.to_string()),
            os_version: Some(std::env::consts::ARCH.to_string()),
            app_version: Some(env!("CARGO_PKG_VERSION").to_string()),
            screen_resolution: None,
        };

        info!("Requesting pairing code for hardware ID: {}", hardware_id);

        let resp = self
            .api
            .request_pairing(&hardware_id, Some(device_info))
            .await?;

        info!("Received pairing code: {}", resp.pairing_code);

        Ok(PairingState {
            pairing_code: resp.pairing_code,
            expires_at: resp.expires_at,
            status: PairingStatus::Pending,
            hardware_id,
            // A constant, and deliberately so. This route answers 200 with the same body for a
            // first enrolment and a re-pair — there is no field to read here. The answer arrives
            // on the first `check_pairing_status`. Do NOT fill this in from the local store: a
            // stored secret proves this device was enrolled here once, not that a *working*
            // terminal would be replaced, and it answers wrong in both directions.
            enrolment: HardwareEnrolment::Undetermined,
        })
    }

    /// Checks the status of a pairing request
    ///
    /// Returns the current status and terminal info if completed.
    pub async fn check_pairing_status(&self, pairing_code: &str) -> Result<PairingState> {
        debug!("Checking pairing status for code: {}", pairing_code);

        let response = self.api.check_pairing_status(pairing_code).await?;

        // Said once, where it first becomes known, and only when there is something to say.
        // `Undetermined` is logged by nothing: a line reporting that we do not know, on every
        // poll, is what trains a reader to skip the line that matters. Neither the pairing code
        // nor the hardware id appears beside it — the code retrieves a credential and the id was
        // the lookup key for one, which is why the platform removed both from its own logs.
        match response.enrolment {
            HardwareEnrolment::AlreadyEnrolled => info!(
                "This hardware is already enrolled: approving this code re-enrols the terminal \
                 and archives the one currently in service"
            ),
            HardwareEnrolment::NotEnrolled | HardwareEnrolment::Undetermined => {}
        }

        // If completed, save the registration and set token on API client
        if response.status == PairingStatus::Completed {
            if let Some(ref terminal) = response.terminal {
                info!(
                    "Pairing completed! Terminal ID: {}, Code: {}",
                    terminal.terminal_id, terminal.terminal_code
                );
                self.save_registration(terminal)?;

                // Login with the terminal credentials to get a session token
                if !terminal.secret.is_empty() {
                    let hardware_id = self.get_hardware_id()?;
                    info!("Logging in terminal to get session token");
                    match self
                        .api
                        .login_terminal(&terminal.terminal_code, &hardware_id, &terminal.secret)
                        .await
                    {
                        Ok(login_response) => {
                            info!("Terminal logged in successfully, session token set");
                            // login_terminal already calls set_token internally
                            // Also save the full terminal config to DB for persistence
                            if let Err(e) = self.save_terminal_config(&hardware_id, &login_response)
                            {
                                warn!("Failed to save terminal config to DB: {}", e);
                            }
                        }
                        Err(e) => {
                            warn!("Failed to login terminal after pairing: {}", e);
                        }
                    }
                } else {
                    warn!("Pairing completed but no secret received");
                }
            }
        }

        let hardware_id = self.get_hardware_id()?;

        Ok(PairingState {
            pairing_code: response.pairing_code,
            expires_at: response.expires_at,
            status: response.status,
            hardware_id,
            enrolment: response.enrolment,
        })
    }

    /// Saves successful registration to local database
    fn save_registration(&self, terminal: &PairedTerminalInfo) -> Result<()> {
        let conn = self.db.connection();
        let conn = conn.lock();

        conn.execute(
            r#"
            UPDATE terminal_registration
            SET terminal_id = ?1,
                terminal_code = ?2,
                secret = ?3,
                company_name = ?4,
                is_registered = 1,
                registered_at = datetime('now')
            WHERE id = 1
            "#,
            params![
                terminal.terminal_id,
                terminal.terminal_code,
                terminal.secret,
                terminal.company_name,
            ],
        )?;

        info!("Terminal registration saved successfully");
        Ok(())
    }

    /// Saves the full terminal config from login response for persistence across restarts
    fn save_terminal_config(
        &self,
        hardware_id: &str,
        response: &pos_api::LoginTerminalResponse,
    ) -> Result<()> {
        let conn = self.db.connection();
        let conn = conn.lock();

        // Through `TERMINAL_CONFIG_ROW`, the same mapping the login path writes. This statement
        // used to name nine columns where that one names eleven, and `INSERT OR REPLACE` is a
        // delete then an insert — so pairing silently reset `tax_rate` and `tax_inclusive` to the
        // column defaults, and neither statement said the other existed.
        //
        // The zeroes below are those same defaults, now written on purpose: pairing has no tax
        // configuration to supply, and this preserves what the omission produced rather than
        // changing behaviour inside a refactor. Whether it *should* preserve an existing
        // configuration is a real question, and it belongs to
        // `project/till/issue/money-and-currency-in-the-till` — see the SAFETY-GAP on the mapping.
        write(
            &conn,
            &TERMINAL_CONFIG_ROW,
            &TerminalConfigRow {
                terminal_id: response.terminal_id.clone(),
                terminal_code: response.terminal_code.clone(),
                hardware_id: hardware_id.to_string(),
                session_token: Some(response.session_token.clone()),
                company_id: Some(response.company_id.clone()),
                branch_id: response.branch_id.clone(),
                locale: response.config.locale.clone(),
                currency: response.config.currency.clone(),
                tax_rate: Some(0.0),
                tax_inclusive: Some(0),
                sector: response.config.business_sector.clone(),
            },
        )?;

        info!("Terminal config saved to database");
        Ok(())
    }

    /// Gets the stored credentials for login
    pub fn get_credentials(&self) -> Result<Option<(String, String)>> {
        let conn = self.db.connection();
        let conn = conn.lock();

        // The same projection as `get_registration`, with the predicate that makes the two
        // columns this caller wants non-null. The pair is destructured from the row rather than
        // read positionally, so a column added ahead of `secret` cannot silently become it.
        let result = read_one(
            &conn,
            TERMINAL_REGISTRATION_ROW.reader(),
            "FROM terminal_registration \
             WHERE id = 1 AND is_registered = 1 AND secret IS NOT NULL",
            [],
        );

        match result {
            // `hardware_id` is `NOT NULL` and the predicate above already excluded a null
            // `secret`, so both are present whenever a row comes back. `zip` says that once
            // instead of two `unwrap_or_default()`s that would each turn a broken row into a
            // blank credential the platform would refuse without explanation.
            Ok(Some(row)) => Ok(row.hardware_id.zip(row.secret)),
            Ok(None) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Clears the registration and all tenant-specific data.
    ///
    /// Wipes products, categories, operators, sync state, offline transactions,
    /// and all other tenant-scoped data so that re-pairing to a different
    /// company starts with a clean slate.
    ///
    /// # Every column that describes an enrolment is named here, deliberately
    ///
    /// This statement used to name five columns and leave `company_name` and `license_key`
    /// standing. Both survived a wipe whose entire purpose is severing this terminal from a
    /// company, and `clear_tenant_data` above empties nineteen tables, so the appearance was of a
    /// thorough clear. `company_name` had survived since schema V3 and `license_key` since V8 —
    /// five schema versions apart, same mechanism, caught by nothing.
    ///
    /// `license_key` was the one that mattered: [`Self::get_platform_license`] reads it with no
    /// `is_registered` scope, so after re-pairing to a second company the till returned the
    /// **first** company's key while enrolled with the second.
    ///
    /// **`hardware_id` is excluded on purpose — do not "complete" this list with it.** It is
    /// `NOT NULL`, it identifies the *device* rather than the *enrolment*, and it must survive a
    /// de-registration so the platform sees a known device re-enrolling rather than a new one.
    /// [`Self::save_hardware_id`] carries the other half of that argument.
    pub fn clear_registration(&self) -> Result<()> {
        // Clear all tenant-specific cached data first
        self.db.clear_tenant_data()?;

        let conn = self.db.connection();
        let conn = conn.lock();

        let updated = conn.execute(
            r#"
            UPDATE terminal_registration
            SET terminal_id = NULL,
                terminal_code = NULL,
                secret = NULL,
                company_name = NULL,
                license_key = NULL,
                is_registered = 0,
                registered_at = NULL
            WHERE id = 1
            "#,
            [],
        )?;

        // An UPDATE that matches nothing succeeds, so without this the till would report a
        // completed wipe having cleared nothing — a swallowed failure inside the fix for a
        // swallowed failure. The schema seeds row 1; zero rows means the store is not the one
        // this code was written against.
        anyhow::ensure!(
            updated == 1,
            "the singleton terminal_registration row (id = 1) is missing, so the registration was \
             not cleared; the schema seeds this row and something has removed it"
        );

        warn!("Terminal registration and all tenant data cleared");
        Ok(())
    }

    /// Stops presenting this till's credentials — the terminal's and the operator's both.
    ///
    /// Call it after [`Self::clear_registration`] so the next pairing request goes out
    /// presenting nothing. Both, not just the terminal's: an operator token held without a
    /// terminal token is a credential that can no longer be used and can still do harm, because
    /// it is the field the platform attributes a write to. [`ApiClient::clear_credentials`]
    /// enforces that at the field; this wrapper must not describe less than it does.
    pub async fn clear_api_credentials(&self) {
        self.api.clear_credentials().await;
    }

    // ========================================================================
    // SETTINGS MANAGEMENT
    // ========================================================================

    /// Default server URL
    pub const DEFAULT_SERVER_URL: &'static str = "https://jooher.app";

    /// Gets a setting value from the settings table
    pub fn get_setting(&self, key: &str) -> Result<Option<String>> {
        let conn = self.db.connection();
        let conn = conn.lock();

        // The free `optional_scalar` over the guard already held above — `Database`'s method
        // would re-take a non-reentrant lock. A missing key is `None`; anything else is an error,
        // which is what the hand-written match below did and what this keeps.
        optional_scalar::<String>(&conn, "SELECT value FROM settings WHERE key = ?1", [key])
            .map_err(Into::into)
    }

    /// Sets a setting value in the settings table
    pub fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        let conn = self.db.connection();
        let conn = conn.lock();

        conn.execute(
            r#"
            INSERT INTO settings (key, value, updated_at)
            VALUES (?1, ?2, datetime('now'))
            ON CONFLICT (key) DO UPDATE SET value = ?2, updated_at = datetime('now')
            "#,
            [key, value],
        )?;

        debug!("Setting '{}' updated to '{}'", key, value);
        Ok(())
    }

    /// Gets the configured server URL, or the default if not set
    pub fn get_server_url(&self) -> Result<String> {
        match self.get_setting("server_url")? {
            Some(url) if !url.is_empty() => {
                info!("Using configured server URL: {}", url);
                Ok(url)
            }
            _ => {
                info!("Using default server URL: {}", Self::DEFAULT_SERVER_URL);
                Ok(Self::DEFAULT_SERVER_URL.to_string())
            }
        }
    }

    /// Sets the server URL
    pub fn set_server_url(&self, url: &str) -> Result<()> {
        // Normalize the URL (remove trailing slash)
        let normalized = url.trim_end_matches('/');
        self.set_setting("server_url", normalized)?;
        info!("Server URL updated to: {}", normalized);
        Ok(())
    }

    // ========================================================================
    // PLATFORM REGISTRATION
    // ========================================================================

    /// Registers the device with the platform registry
    ///
    /// This is called after successful pairing to register the device
    /// for platform-level monitoring and management.
    ///
    /// # Arguments
    ///
    /// * `device_name` - Friendly name for the device
    ///
    /// # Returns
    ///
    /// The license key assigned to the device
    pub async fn register_with_platform(&self, device_name: &str) -> Result<String> {
        let hardware_id = self.get_hardware_id()?;

        info!(
            "Registering device with platform: hardware_id={}, name={}",
            hardware_id, device_name
        );

        let request = RegisterDeviceRequest {
            device_id: hardware_id.clone(),
            device_fingerprint: self.generate_device_fingerprint()?,
            device_name: device_name.to_string(),
            os_info: OsInfo {
                name: std::env::consts::OS.to_string(),
                version: std::env::consts::ARCH.to_string(),
            },
            hardware_info: self.collect_hardware_info(),
        };

        let response = self.api.register_device_platform(&request).await?;

        info!(
            "Device registered with platform: license_key={}, status={:?}",
            response.license_key, response.license_status
        );

        // Save the license key locally
        self.save_platform_license(&response.license_key)?;

        Ok(response.license_key)
    }

    /// Generates a device fingerprint based on hardware characteristics
    fn generate_device_fingerprint(&self) -> Result<String> {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();

        // Hash hostname
        if let Ok(hostname) = hostname::get() {
            hostname.hash(&mut hasher);
        }

        // Hash OS info
        std::env::consts::OS.hash(&mut hasher);
        std::env::consts::ARCH.hash(&mut hasher);

        // Hash machine ID if available (Linux)
        #[cfg(target_os = "linux")]
        {
            if let Ok(machine_id) = std::fs::read_to_string("/etc/machine-id") {
                machine_id.trim().hash(&mut hasher);
            }
        }

        let hash = hasher.finish();
        Ok(format!("FP-{:016X}", hash))
    }

    /// Collects hardware information for registration
    fn collect_hardware_info(&self) -> HardwareInfo {
        let cpu = self.get_cpu_info();
        let memory = self.get_total_memory_mb();

        HardwareInfo { cpu, memory }
    }

    /// Gets CPU information
    fn get_cpu_info(&self) -> String {
        #[cfg(target_os = "linux")]
        {
            if let Ok(content) = std::fs::read_to_string("/proc/cpuinfo") {
                for line in content.lines() {
                    if line.starts_with("model name") {
                        if let Some(value) = line.split(':').nth(1) {
                            return value.trim().to_string();
                        }
                    }
                }
            }
        }
        std::env::consts::ARCH.to_string()
    }

    /// Gets total memory in MB
    fn get_total_memory_mb(&self) -> u64 {
        #[cfg(target_os = "linux")]
        {
            if let Ok(content) = std::fs::read_to_string("/proc/meminfo") {
                for line in content.lines() {
                    if line.starts_with("MemTotal:") {
                        let kb: u64 = line
                            .split_whitespace()
                            .nth(1)
                            .and_then(|s| s.parse().ok())
                            .unwrap_or(0);
                        return kb / 1024; // Convert KB to MB
                    }
                }
            }
        }
        0
    }

    /// Saves the platform license key locally
    fn save_platform_license(&self, license_key: &str) -> Result<()> {
        let conn = self.db.connection();
        let conn = conn.lock();

        conn.execute(
            r#"
            UPDATE terminal_registration
            SET license_key = ?1
            WHERE id = 1
            "#,
            [license_key],
        )?;

        info!("Platform license key saved");
        Ok(())
    }

    /// Gets the stored platform license key, if this till is enrolled.
    ///
    /// # The `is_registered` scope is defence in depth and is NOT what closes the leak
    ///
    /// Read this before deleting anything from [`Self::clear_registration`]'s `SET` list on the
    /// grounds that "the read is guarded". It is not enough on its own, and the order of events is
    /// why:
    ///
    /// 1. the till is enrolled with company A and stores A's key;
    /// 2. [`Self::clear_registration`] runs — `is_registered` goes to 0;
    /// 3. the till enrols with company B — **`is_registered` is back to 1**;
    /// 4. nothing has revisited `license_key` in between, because its only writer is
    ///    [`Self::save_platform_license`], reached only from `register_with_platform`.
    ///
    /// At step 4 this filter passes and would hand back **A's key while the till is enrolled with
    /// B**. Clearing the column in step 2 is what actually prevents that; this scope only covers
    /// the window where the column holds a value and no enrolment is in force.
    ///
    /// [`Self::get_credentials`] has carried the same scope on the same table for far longer. The
    /// right shape was three hundred lines away from the wrong one, which makes this a missing
    /// constraint rather than a knowledge gap.
    ///
    /// # Open: the licence key is not a field of [`TerminalRegistration::Enrolled`], and it is not
    /// obvious that it should be
    ///
    /// Making the sum type a sum type raised the question and did not settle it, so it is recorded
    /// here rather than answered by whichever shape was convenient. The key is written by
    /// `register_with_platform`, a flow that succeeds or fails **independently** of pairing: an
    /// enrolled till can hold no key, and — until [`Self::clear_registration`] was fixed — a key
    /// could outlive the enrolment that fetched it. So it is not a property of the enrolment the
    /// way the secret is, and putting it on the enrolled variant would assert a coupling the two
    /// flows do not have. The candidate shapes are a separate type for the platform-registry
    /// standing, or an `Option` on the variant that admits the decoupling in its own type. Neither
    /// is decided; both are bigger than this read.
    pub fn get_platform_license(&self) -> Result<Option<String>> {
        let conn = self.db.connection();
        let conn = conn.lock();

        let result = optional_scalar::<Option<String>>(
            &conn,
            "SELECT license_key FROM terminal_registration WHERE id = 1 AND is_registered = 1",
            [],
        );

        match result {
            // Unregistered till, no row, and a `NULL` key all mean the same thing to this caller
            // — there is no platform licence to offer — so the two `None`s collapse. An `Err`
            // does not: it means the store could not answer, which is not the same as "no
            // licence" and must not be reported as one.
            Ok(license_key) => Ok(license_key.flatten()),
            Err(e) => Err(e.into()),
        }
    }

    /// Validates the platform license with the server
    pub async fn validate_platform_license(&self) -> Result<bool> {
        let hardware_id = self.get_hardware_id()?;
        let license_key = self.get_platform_license()?;

        match license_key {
            Some(key) => {
                let valid = self.api.validate_license(&hardware_id, &key).await?;
                Ok(valid)
            }
            None => {
                warn!("No platform license key stored");
                Ok(false)
            }
        }
    }
}

/// Generates a unique hardware ID for this terminal
fn generate_hardware_id() -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();

    // Hash various system properties
    if let Ok(hostname) = hostname::get() {
        hostname.hash(&mut hasher);
    }

    // Add some randomness for uniqueness
    let random: u64 = rand::random();
    random.hash(&mut hasher);

    // Add timestamp
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
        .hash(&mut hasher);

    let hash = hasher.finish();
    format!("POS-{:016X}", hash)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pos_db::init_memory_database;

    fn create_test_service() -> PairingService {
        let db = Arc::new(init_memory_database().unwrap());
        let api = Arc::new(ApiClient::new("https://api.example.com"));
        PairingService::new(api, db)
    }

    #[test]
    fn test_not_registered_initially() {
        let service = create_test_service();
        assert!(!service.is_registered().unwrap());
    }

    /// Every column of `terminal_registration` carries a **distinct** value, and every field of
    /// the loaded `TerminalRegistration` is asserted.
    ///
    /// `get_registration` reads its row by position, so a column added, removed or reordered in
    /// the SELECT list shifts every index after it. Five of the seven columns are TEXT, so a swap
    /// among them compiles and passes rusqlite's type check — it just attributes one terminal's
    /// details to another. Asserting a subset leaves the unasserted positions free to swap, and
    /// repeating a value across same-typed columns lets a swap between *those two* pass: a later
    /// edit that tidies these fixtures into shared or omitted values disarms the test without
    /// failing it.
    #[test]
    fn get_registration_reads_every_column_into_its_own_field() {
        let service = create_test_service();

        {
            let conn = service.db.connection();
            let conn = conn.lock();
            conn.execute(
                r#"
                UPDATE terminal_registration
                SET hardware_id = 'hardware-id-1',
                    terminal_id = 'terminal-id-1',
                    terminal_code = 'terminal-code-1',
                    secret = 'secret-1',
                    company_name = 'company-name-1',
                    registered_at = '2026-08-23T12:00:00Z',
                    is_registered = 1
                WHERE id = 1
                "#,
                [],
            )
            .unwrap();
        }

        assert_eq!(
            service.get_registration().unwrap(),
            TerminalRegistration::Enrolled {
                hardware_id: "hardware-id-1".to_string(),
                terminal_id: "terminal-id-1".to_string(),
                terminal_code: "terminal-code-1".to_string(),
                secret: "secret-1".to_string(),
                company_name: Some("company-name-1".to_string()),
                registered_at: Some("2026-08-23T12:00:00Z".to_string()),
            }
        );
    }

    /// A row that exists but is not registered is not an enrolment. The credentials are present
    /// on that row during pairing, so reporting them would hand callers a terminal identity the
    /// server has not confirmed.
    ///
    /// The assertion is on the **whole value**, not on "it is the `Unenrolled` variant". A
    /// variant check would pass just as well if `Unenrolled` grew a `secret` field and this row's
    /// secret arrived in it — which is the exact leak the old `Option` return was filtering by
    /// hand. Equality is what makes the omission structural.
    #[test]
    fn get_registration_is_unenrolled_while_the_row_is_not_registered() {
        let service = create_test_service();

        {
            let conn = service.db.connection();
            let conn = conn.lock();
            conn.execute(
                r#"
                UPDATE terminal_registration
                SET hardware_id = 'hardware-id-1',
                    terminal_id = 'terminal-id-1',
                    terminal_code = 'terminal-code-1',
                    secret = 'secret-1',
                    is_registered = 0
                WHERE id = 1
                "#,
                [],
            )
            .unwrap();
        }

        assert_eq!(
            service.get_registration().unwrap(),
            TerminalRegistration::Unenrolled {
                hardware_id: Some("hardware-id-1".to_string()),
            }
        );
    }

    /// A freshly migrated till has a row, and that row has no identity yet.
    ///
    /// The schema seeds `hardware_id` to the empty string, so this is the state every till passes
    /// through. `Some("")` would be an identity this till does not have.
    #[test]
    fn get_registration_reports_no_identity_before_one_is_generated() {
        let service = create_test_service();

        assert_eq!(
            service.get_registration().unwrap(),
            TerminalRegistration::Unenrolled { hardware_id: None }
        );
    }

    /// A row marked enrolled with no secret is refused, and the refusal names the column.
    ///
    /// # This state is reachable, which is why the check is on the empty string and not on `NULL`
    ///
    /// `PairedTerminalInfo::secret` is a `String`, and `check_pairing_status` calls
    /// `save_registration` — which writes `is_registered = 1` — *before* it looks at whether the
    /// secret is empty, warning "Pairing completed but no secret received" one branch later. So
    /// the server can complete a pairing with a blank secret and leave exactly this row.
    ///
    /// `NULL`, by contrast, is not reachable: `save_registration` is the only writer of
    /// `is_registered = 1` and it always binds all four columns. A check that rejected only
    /// `NULL` would be rejecting the state that cannot occur and accepting the one that does.
    ///
    /// The two arms below are one test on purpose: the second is the control. Without it, this
    /// passes just as happily against a `get_registration` that refuses *every* row, which would
    /// read as a strict type rather than as a broken read.
    #[test]
    fn an_enrolled_row_with_a_blank_secret_is_refused_rather_than_reported_as_enrolled() {
        let service = create_test_service();

        let write_secret = |secret: &str| {
            let conn = service.db.connection();
            let conn = conn.lock();
            conn.execute(
                r#"
                UPDATE terminal_registration
                SET hardware_id = 'hardware-id-1',
                    terminal_id = 'terminal-id-1',
                    terminal_code = 'terminal-code-1',
                    secret = ?1,
                    is_registered = 1
                WHERE id = 1
                "#,
                params![secret],
            )
            .unwrap();
        };

        write_secret("");
        let refused = service
            .get_registration()
            .expect_err("a row marked enrolled with no secret is not an enrolment");
        let message = format!("{refused:#}");
        assert!(
            message.contains("secret"),
            "the refusal has to name the column that is wrong, got: {message}"
        );
        assert!(
            !message.contains("terminal_id"),
            "only the absent columns belong in the message, got: {message}"
        );

        // Control: the same row with a secret is an enrolment, so the refusal above is about the
        // blank secret and not about this read refusing everything.
        //
        // It asserts the whole value rather than the variant, which makes it double as the
        // every-nullable-column-NULL case this table's round-trips are supposed to carry
        // (`doc/considerations-inbox` §A pattern worth copying): `company_name` and
        // `registered_at` are never written above, so a fallback firing over them fails here.
        write_secret("secret-1");
        assert_eq!(
            service.get_registration().unwrap(),
            TerminalRegistration::Enrolled {
                hardware_id: "hardware-id-1".to_string(),
                terminal_id: "terminal-id-1".to_string(),
                terminal_code: "terminal-code-1".to_string(),
                secret: "secret-1".to_string(),
                company_name: None,
                registered_at: None,
            }
        );
    }

    #[test]
    fn test_get_hardware_id() {
        let service = create_test_service();
        let id1 = service.get_hardware_id().unwrap();
        let id2 = service.get_hardware_id().unwrap();

        // Same ID returned each time
        assert_eq!(id1, id2);
        assert!(id1.starts_with("POS-"));
    }

    #[test]
    fn test_save_registration() {
        let service = create_test_service();

        // Set hardware ID first
        service.save_hardware_id("TEST-HW-123").unwrap();

        // Save registration
        let terminal = PairedTerminalInfo {
            terminal_id: "TERM-001".to_string(),
            terminal_code: "TERM-001".to_string(),
            secret: "secret123".to_string(),
            company_name: None,
        };

        service.save_registration(&terminal).unwrap();

        // Verify registered
        assert!(service.is_registered().unwrap());

        // Verify credentials
        let creds = service.get_credentials().unwrap();
        assert!(creds.is_some());
        let (hw_id, secret) = creds.unwrap();
        assert_eq!(hw_id, "TEST-HW-123");
        assert_eq!(secret, "secret123");
    }

    /// Reads the enrolment columns straight from the row, bypassing every accessor.
    ///
    /// **Deliberately not `get_registration` or `get_platform_license`.** The first refuses to
    /// answer once `is_registered` is 0, so it cannot see whether a column was cleared or merely
    /// hidden. The second gains an `is_registered` filter in the next task of this issue — after
    /// which it would answer `None` whether or not `license_key` was cleared, and a test built on
    /// it would keep passing with the fix reverted.
    ///
    /// A test that observes the *column* observes the fix; one that observes an accessor observes
    /// the accessor.
    fn enrolment_columns(service: &PairingService) -> (Option<String>, Option<String>, String) {
        let conn = service.db.connection();
        let conn = conn.lock();
        conn.query_row(
            "SELECT company_name, license_key, hardware_id FROM terminal_registration WHERE id = 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .expect("the seeded terminal_registration row is missing")
    }

    #[test]
    fn test_clear_registration() {
        let service = create_test_service();

        // Register first
        service.save_hardware_id("TEST-HW-123").unwrap();
        let terminal = PairedTerminalInfo {
            terminal_id: "TERM-001".to_string(),
            terminal_code: "TERM-001".to_string(),
            secret: "secret123".to_string(),
            company_name: Some("Acme Trading".to_string()),
        };
        service.save_registration(&terminal).unwrap();
        service.save_platform_license("LICENCE-AAAA-1111").unwrap();
        assert!(service.is_registered().unwrap());

        // Positive controls. Without these, the assertions after the clear pass just as happily
        // against columns that were never populated — which is exactly how this defect survived
        // `company_name` from schema V3 and `license_key` from V8 with a test in the file.
        let (company, licence, hardware) = enrolment_columns(&service);
        assert_eq!(company.as_deref(), Some("Acme Trading"));
        assert_eq!(licence.as_deref(), Some("LICENCE-AAAA-1111"));
        assert_eq!(hardware, "TEST-HW-123");

        // Clear registration
        service.clear_registration().unwrap();
        assert!(!service.is_registered().unwrap());
        assert!(service.get_credentials().unwrap().is_none());

        let (company, licence, hardware) = enrolment_columns(&service);
        assert_eq!(
            company, None,
            "the previous company's name survived de-registration"
        );
        assert_eq!(
            licence, None,
            "the previous company's platform licence key survived de-registration; re-pairing to \
             another company would leave the till holding it while enrolled elsewhere"
        );

        // The exemption, asserted rather than assumed. `hardware_id` must SURVIVE: it identifies
        // the device, not the enrolment, and clearing it would make the platform see a new device
        // rather than a known one re-enrolling. Asserting it here makes the omission from
        // `clear_registration`'s SET list a tested decision instead of a gap someone later
        // "completes".
        assert_eq!(
            hardware, "TEST-HW-123",
            "the hardware id is not part of an enrolment and must survive de-registration"
        );
    }

    /// A licence key sitting on a row that is not enrolled must not be handed back.
    ///
    /// **This is the only state that distinguishes the guarded read from the unguarded one**, and
    /// the scenario the task originally specified — enrol, store, clear, re-enrol, assert `None` —
    /// no longer does. Once `clear_registration` clears `license_key`, that sequence answers
    /// `None` whether or not the `WHERE` clause carries `AND is_registered = 1`: a test that
    /// cannot come out differently.
    ///
    /// The reachable discriminating state is a licence with no enrolment behind it.
    /// `save_platform_license` writes `license_key` and never touches `is_registered`, so it is
    /// one call away.
    #[test]
    fn an_unenrolled_till_does_not_hand_back_a_licence_key() {
        let service = create_test_service();
        service
            .save_platform_license("ORPHAN-LICENCE-9999")
            .unwrap();

        assert!(
            !service.is_registered().unwrap(),
            "the fixture is wrong: this test needs an UNenrolled till"
        );

        // Positive control. Without it this passes against a column that was never written, and
        // the assertion below would be about an empty row rather than about the read's scope.
        let (_, licence, _) = enrolment_columns(&service);
        assert_eq!(licence.as_deref(), Some("ORPHAN-LICENCE-9999"));

        assert_eq!(
            service.get_platform_license().unwrap(),
            None,
            "the till handed back a platform licence key while holding no enrolment"
        );
    }

    #[test]
    fn test_generate_hardware_id() {
        let id1 = generate_hardware_id();
        let id2 = generate_hardware_id();

        // IDs should be different (randomness)
        assert_ne!(id1, id2);

        // IDs should start with POS-
        assert!(id1.starts_with("POS-"));
        assert!(id2.starts_with("POS-"));
    }
}
