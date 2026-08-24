//! Terminal Identity Tables
//!
//! The three single-row tables that say who this till is: its configuration, its enrolment with
//! the platform, and the operator currently signed in at it.
//!
//! Each is `id INTEGER PRIMARY KEY CHECK (id = 1)`, so `id` is `managed` in every mapping here
//! rather than a field — there is one row and no caller gets a say in which.
//!
//! # Why these row shapes live in `pos-db` and the types that use them do not
//!
//! `pos-services` holds `TerminalSession`, `TerminalRegistration` and `HeldOperatorSession`, and
//! all three are *claims about* a row rather than the row: they apply defaults, refuse blanks, and
//! carry fields no column backs. Those stay where they are. What moves here is the transport
//! shape — every column exactly as the schema declares it, nullable where the column is nullable —
//! because `DECLARED_SHAPES` can only name paths inside this crate, and a shape no registry lists
//! is a shape no schema check verifies.

use pos_models::OperatorId;

use crate::column;
use crate::projection::OnConflict;
use crate::row_mapping;

// ============================================================================
// terminal_config
// ============================================================================

/// The eleven columns of `terminal_config` this till reads and writes.
///
/// **Every field is as nullable as its column**, and that is deliberate. The read this replaces
/// applied six different defaults inside the row closure — `unwrap_or_default()` for the company,
/// `"ar"` for the locale, `"LYD"` for the currency, `0.0` for the tax rate, `0` for its
/// inclusivity, `"RETAIL"` for the sector — so a column that was absent and a column that was
/// present and equal to the default were the same value by the time anything could tell them
/// apart. Those defaults are a domain decision and now live in one place on the far side of this
/// type, where they can be read as decisions.
#[derive(Debug, Clone, PartialEq)]
pub struct TerminalConfigRow {
    pub terminal_id: String,
    pub terminal_code: String,
    pub hardware_id: String,
    pub session_token: Option<String>,
    pub company_id: Option<String>,
    pub branch_id: Option<String>,
    pub locale: Option<String>,
    pub currency: Option<String>,
    pub tax_rate: Option<f64>,
    pub tax_inclusive: Option<i64>,
    pub sector: Option<String>,
}

row_mapping! {
    /// Every column of `terminal_config`, declared once.
    ///
    /// **This closes a drift between two writers that column affinity was hiding.** `auth_service`
    /// wrote eleven columns on login; `pairing_service` wrote nine on pairing, omitting `tax_rate`
    /// and `tax_inclusive`. `INSERT OR REPLACE` is a delete then an insert, so the shorter writer
    /// silently reset the tax configuration to the column defaults — and nothing in either
    /// statement said the other existed. Both go through this mapping now, so the two lists cannot
    /// disagree; `the_pairing_write_and_the_login_write_name_the_same_columns` pins it.
    ///
    /// Preserving behaviour means pairing now writes zero tax **explicitly** rather than by
    /// omission. That is the same value the column default produced, stated instead of implied.
    /// Whether pairing should preserve an existing tax configuration instead is a real question
    /// and not this issue's — see the SAFETY-GAP below.
    pub const TERMINAL_CONFIG_ROW: RowMapping<TerminalConfigRow> = for "terminal_config" {
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
        managed "id" = "1",
        managed "updated_at" = "datetime('now')",
    } on_conflict OnConflict::Replace;
}

// SAFETY-GAP: `tax_rate` is an `f64` and `terminal_config.tax_rate` is `REAL`. It is the rate every
// line of every sale is taxed at, and binary floating point cannot represent a decimal rate
// exactly. Worse than the type: it has four declaration sites and three independent zero-defaults
// across this workspace, so a till that has never synced its configuration and a till whose tax is
// genuinely zero are the same reading. Left as-is on an explicit ruling — fixed inside a
// positional-access refactor it would get no design, no money-correctness review, and a commit
// message about column indices. It belongs to
// `project/till/issue/money-and-currency-in-the-till`. Recorded 2026-08-24 by
// `project/till/issue/positional-row-access-in-pos-db` task 13.
//
// SAFETY-GAP: relatedly, `INSERT OR REPLACE` here means any writer that does not supply
// `tax_rate`/`tax_inclusive` resets them. The mapping makes that impossible to do by accident —
// there is one column list — but it does not decide whether pairing *should* preserve a tax
// configuration it has no value for. Same owning issue.

// ============================================================================
// terminal_registration
// ============================================================================

/// The eight columns of `terminal_registration`, exactly as SQLite hands them over.
///
/// Every column nullable, because every column but `hardware_id` is nullable in the schema and
/// `hardware_id` is seeded as the empty string by `SCHEMA_V3` — so "this till has no hardware id
/// yet" arrives as `Some("")` rather than as `None`, and a reader that treats blank and absent
/// differently is reading a distinction the store does not make.
///
/// `is_registered` is `Option<i64>` and not `bool`: the column is `INTEGER DEFAULT 0`, and the
/// question "is this 1?" is a claim about the row rather than a fact of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalRegistrationRow {
    pub hardware_id: Option<String>,
    pub terminal_id: Option<String>,
    pub terminal_code: Option<String>,
    pub secret: Option<String>,
    pub company_name: Option<String>,
    pub registered_at: Option<String>,
    pub is_registered: Option<i64>,
    pub license_key: Option<String>,
}

row_mapping! {
    /// Every column of `terminal_registration`, declared once.
    ///
    /// Three separate hand-written projections read this table — seven columns for the
    /// registration itself, two for the login credentials, three for re-authentication — and each
    /// was a fresh chance to name a column that had moved. They are one projection now. Reading
    /// eight columns of a single-row table where two were wanted costs nothing worth measuring.
    ///
    /// **`license_key` is in this list and was in none of the three.** It is added by migration
    /// v8, and a shape that omits it is how `clear_registration` came to leave a previous tenant's
    /// key behind. That particular hole is already closed — the wipe names `license_key`
    /// explicitly, verified 2026-08-24 — and this makes the omission unwritable rather than merely
    /// currently-absent.
    pub const TERMINAL_REGISTRATION_ROW: RowMapping<TerminalRegistrationRow> =
        for "terminal_registration" {
            hardware_id,
            terminal_id,
            terminal_code,
            secret,
            company_name,
            registered_at,
            is_registered,
            license_key,
            managed "id" = "1",
        } on_conflict OnConflict::Replace;
}

// ============================================================================
// operator_sessions
// ============================================================================

/// The three columns of `operator_sessions` the sign-in path reads.
///
/// `token` and `expires_at` are nullable text for the same reason as the registration row: the
/// caller decides what a blank means, and here it decides that a blank token and an unparseable
/// instant are both *no usable session*.
///
/// **`operator_id` is `Option<OperatorId>` and not `Option<String>`**, which changes one case.
/// `tests/guards.rs::operator_identity_never_survives_as_a_bare_string` refused the bare string —
/// correctly, and I had written it — and the codec it forces reads `NULL` as `None` while
/// *refusing* the empty string. So a blank operator id is now a read error rather than a fourth
/// spelling of "no session". That is reachable by nothing: `record` writes an `OperatorId`, which
/// cannot be blank, and the migration's own inserts name a real id. A blank in this column is a
/// row that could not have been written by this till, and saying so is what the newtype is for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperatorSessionRow {
    pub operator_id: Option<OperatorId>,
    pub token: Option<String>,
    pub expires_at: Option<String>,
}

row_mapping! {
    /// The operator session on disk, declared once.
    ///
    /// `established_at` is `managed`: the store stamps when the session was recorded and no caller
    /// supplies it, which is the same arrangement `updated_at` has everywhere else here.
    pub const OPERATOR_SESSION_ROW: RowMapping<OperatorSessionRow> = for "operator_sessions" {
        operator_id via column::OPTIONAL_OPERATOR_ID,
        token,
        expires_at,
        managed "id" = "1",
        managed "established_at" = "datetime('now')",
    } on_conflict OnConflict::Replace;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::migrations::run_migrations;
    use crate::projection::DECLARED_SHAPES;
    use crate::Database;

    fn setup_db() -> Database {
        let db = Database::in_memory().unwrap();
        {
            let conn = db.connection();
            let conn = conn.lock();
            run_migrations(&conn).unwrap();
        }
        db
    }

    fn a_config_with_no_two_columns_alike() -> TerminalConfigRow {
        TerminalConfigRow {
            terminal_id: "terminal-id-column".to_string(),
            terminal_code: "terminal-code-column".to_string(),
            hardware_id: "hardware-id-column".to_string(),
            session_token: Some("session-token-column".to_string()),
            company_id: Some("company-id-column".to_string()),
            branch_id: Some("branch-id-column".to_string()),
            locale: Some("locale-column".to_string()),
            currency: Some("currency-column".to_string()),
            tax_rate: Some(17.5),
            tax_inclusive: Some(1),
            sector: Some("sector-column".to_string()),
        }
    }

    #[test]
    fn the_config_mapping_names_every_column_it_writes_in_the_order_it_reads_them() {
        assert_eq!(
            TERMINAL_CONFIG_ROW.reader().select_list(),
            "terminal_id, terminal_code, hardware_id, session_token, company_id, branch_id, \
             locale, currency, tax_rate, tax_inclusive, sector"
        );
        assert_eq!(TERMINAL_CONFIG_ROW.reader().width(), 11);
        assert_eq!(
            TERMINAL_CONFIG_ROW
                .insert_column_names()
                .collect::<Vec<_>>()
                .join(", "),
            "terminal_id, terminal_code, hardware_id, session_token, company_id, branch_id, \
             locale, currency, tax_rate, tax_inclusive, sector, id, updated_at"
        );
    }

    #[test]
    fn every_column_of_a_fully_distinct_config_survives_the_round_trip() {
        let db = setup_db();
        let written = a_config_with_no_two_columns_alike();
        db.insert(&TERMINAL_CONFIG_ROW, &written).unwrap();

        let read = db
            .select_one(
                TERMINAL_CONFIG_ROW.reader(),
                "FROM terminal_config WHERE id = 1",
                [],
            )
            .unwrap()
            .expect("the config this test just wrote");
        assert_eq!(read, written);
    }

    #[test]
    fn saving_the_config_puts_each_value_in_the_column_that_carries_its_name() {
        let db = setup_db();
        db.insert(&TERMINAL_CONFIG_ROW, &a_config_with_no_two_columns_alike())
            .unwrap();

        let conn = db.connection();
        let conn = conn.lock();
        for (column, expected) in [
            ("terminal_id", "terminal-id-column"),
            ("terminal_code", "terminal-code-column"),
            ("hardware_id", "hardware-id-column"),
            ("session_token", "session-token-column"),
            ("company_id", "company-id-column"),
            ("branch_id", "branch-id-column"),
            ("locale", "locale-column"),
            ("currency", "currency-column"),
            ("sector", "sector-column"),
        ] {
            let matched: bool = conn
                .query_row(
                    &format!("SELECT {column} = ?1 FROM terminal_config"),
                    [expected],
                    |row| row.get(0),
                )
                .unwrap();
            assert!(matched, "the `{column}` column does not hold `{expected}`");
        }

        // The numeric columns, compared as numbers — a `REAL` against a bound `&str` is never
        // equal in SQLite, so these would all read `false` folded into the loop above.
        for (column, expected) in [("tax_rate", 17.5_f64), ("tax_inclusive", 1.0), ("id", 1.0)] {
            let matched: bool = conn
                .query_row(
                    &format!("SELECT {column} = ?1 FROM terminal_config"),
                    [expected],
                    |row| row.get(0),
                )
                .unwrap();
            assert!(matched, "the `{column}` column does not hold `{expected}`");
        }
    }

    /// The two writers of `terminal_config` cannot disagree about its columns any more.
    ///
    /// They did: the login path wrote eleven and the pairing path wrote nine, and under
    /// `INSERT OR REPLACE` the shorter one reset the tax configuration. This asserts the property
    /// that makes that unrepresentable — there is one column list — rather than asserting the two
    /// call sites look alike, which is a thing that goes stale.
    #[test]
    fn the_pairing_write_and_the_login_write_name_the_same_columns() {
        let db = setup_db();

        // The login path: a full row.
        db.insert(&TERMINAL_CONFIG_ROW, &a_config_with_no_two_columns_alike())
            .unwrap();

        // The pairing path: the same mapping, with the tax values it has no source for.
        db.insert(
            &TERMINAL_CONFIG_ROW,
            &TerminalConfigRow {
                tax_rate: Some(0.0),
                tax_inclusive: Some(0),
                session_token: Some("re-paired".to_string()),
                ..a_config_with_no_two_columns_alike()
            },
        )
        .unwrap();

        let rows: i64 = db
            .select_scalar("SELECT COUNT(*) FROM terminal_config", [])
            .unwrap();
        assert_eq!(rows, 1, "the second write inserted rather than replaced");

        let read = db
            .select_one(
                TERMINAL_CONFIG_ROW.reader(),
                "FROM terminal_config WHERE id = 1",
                [],
            )
            .unwrap()
            .unwrap();
        assert_eq!(read.session_token.as_deref(), Some("re-paired"));
        // Zero because this writer said zero, not because it forgot to say anything.
        assert_eq!(read.tax_rate, Some(0.0));
    }

    fn a_registration_with_no_two_columns_alike() -> TerminalRegistrationRow {
        TerminalRegistrationRow {
            hardware_id: Some("hardware-id-column".to_string()),
            terminal_id: Some("terminal-id-column".to_string()),
            terminal_code: Some("terminal-code-column".to_string()),
            secret: Some("secret-column".to_string()),
            company_name: Some("company-name-column".to_string()),
            registered_at: Some("2026-08-24T10:00:00Z".to_string()),
            is_registered: Some(1),
            license_key: Some("license-key-column".to_string()),
        }
    }

    #[test]
    fn the_registration_mapping_names_every_column_including_the_one_the_projections_missed() {
        assert_eq!(
            TERMINAL_REGISTRATION_ROW.reader().select_list(),
            "hardware_id, terminal_id, terminal_code, secret, company_name, registered_at, \
             is_registered, license_key"
        );
        assert_eq!(TERMINAL_REGISTRATION_ROW.reader().width(), 8);
    }

    #[test]
    fn every_column_of_a_fully_distinct_registration_survives_the_round_trip() {
        let db = setup_db();
        let written = a_registration_with_no_two_columns_alike();
        db.insert(&TERMINAL_REGISTRATION_ROW, &written).unwrap();

        let read = db
            .select_one(
                TERMINAL_REGISTRATION_ROW.reader(),
                "FROM terminal_registration WHERE id = 1",
                [],
            )
            .unwrap()
            .expect("the registration this test just wrote");
        assert_eq!(read, written);
    }

    #[test]
    fn saving_a_registration_puts_each_value_in_the_column_that_carries_its_name() {
        let db = setup_db();
        db.insert(
            &TERMINAL_REGISTRATION_ROW,
            &a_registration_with_no_two_columns_alike(),
        )
        .unwrap();

        let conn = db.connection();
        let conn = conn.lock();
        for (column, expected) in [
            ("hardware_id", "hardware-id-column"),
            ("terminal_id", "terminal-id-column"),
            ("terminal_code", "terminal-code-column"),
            ("secret", "secret-column"),
            ("company_name", "company-name-column"),
            ("registered_at", "2026-08-24T10:00:00Z"),
            ("license_key", "license-key-column"),
        ] {
            let matched: bool = conn
                .query_row(
                    &format!("SELECT {column} = ?1 FROM terminal_registration"),
                    [expected],
                    |row| row.get(0),
                )
                .unwrap();
            assert!(matched, "the `{column}` column does not hold `{expected}`");
        }
    }

    /// A `NULL` in one registration column reaches that column's field and no other.
    #[test]
    fn a_null_secret_reaches_that_field_and_leaves_the_hardware_id_alone() {
        let db = setup_db();
        db.insert(
            &TERMINAL_REGISTRATION_ROW,
            &TerminalRegistrationRow {
                secret: None,
                ..a_registration_with_no_two_columns_alike()
            },
        )
        .unwrap();

        let read = db
            .select_one(
                TERMINAL_REGISTRATION_ROW.reader(),
                "FROM terminal_registration WHERE id = 1",
                [],
            )
            .unwrap()
            .unwrap();
        assert_eq!(read.secret, None);
        assert_eq!(read.hardware_id.as_deref(), Some("hardware-id-column"));
        assert_eq!(read.license_key.as_deref(), Some("license-key-column"));
    }

    #[test]
    fn an_operator_session_survives_the_round_trip_and_replaces_on_the_second_write() {
        let db = setup_db();
        let first = OperatorSessionRow {
            operator_id: Some(OperatorId::new("operator-id-column").unwrap()),
            token: Some("token-column".to_string()),
            expires_at: Some("2026-08-24T18:00:00Z".to_string()),
        };
        db.insert(&OPERATOR_SESSION_ROW, &first).unwrap();

        let read = db
            .select_one(
                OPERATOR_SESSION_ROW.reader(),
                "FROM operator_sessions WHERE id = 1",
                [],
            )
            .unwrap()
            .expect("the session this test just wrote");
        assert_eq!(read, first);

        db.insert(
            &OPERATOR_SESSION_ROW,
            &OperatorSessionRow {
                token: Some("second-token".to_string()),
                ..first
            },
        )
        .unwrap();

        let rows: i64 = db
            .select_scalar("SELECT COUNT(*) FROM operator_sessions", [])
            .unwrap();
        assert_eq!(rows, 1, "signing in twice left two sessions on the till");

        // …and the store stamped `established_at` itself, which is why it is `managed`.
        let stamped: bool = db
            .select_scalar(
                "SELECT established_at IS NOT NULL AND established_at <> '' FROM operator_sessions",
                [],
            )
            .unwrap();
        assert!(stamped, "the store did not stamp `established_at`");
    }

    /// The three shapes declared here are in the registry the schema check walks.
    ///
    /// Not a formality: a shape absent from `DECLARED_SHAPES` is a shape
    /// `every_mapping_names_columns_the_schema_has` never sees, and its absence looks exactly like
    /// a passing guard. `tests/mappings.rs` reconstructs that list from the source tree and
    /// refuses a mismatch, so this is belt to that brace — it fails here, next to the
    /// declarations, rather than in a file about registries.
    #[test]
    fn every_shape_declared_here_is_registered() {
        for name in [
            "TERMINAL_CONFIG_ROW",
            "TERMINAL_REGISTRATION_ROW",
            "OPERATOR_SESSION_ROW",
        ] {
            assert!(
                DECLARED_SHAPES.iter().any(|shape| shape.name == name),
                "`{name}` is declared in this module and absent from `DECLARED_SHAPES`"
            );
        }
    }
}
