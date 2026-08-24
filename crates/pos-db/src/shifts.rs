//! Shifts Repository
//!
//! Handles shift data storage and management.

use std::str::FromStr;

use chrono::Utc;
use rusqlite::{params, Result as SqliteResult};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use pos_models::OperatorId;

use super::Database;
use crate::column;
use crate::decimal_to_sqlite;
use crate::parse::ParseError;
use crate::projection::OnConflict;
use crate::row_mapping;

/// Shift status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ShiftStatus {
    Active,
    Closed,
    Suspended,
}

impl ShiftStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            ShiftStatus::Active => "ACTIVE",
            ShiftStatus::Closed => "CLOSED",
            ShiftStatus::Suspended => "SUSPENDED",
        }
    }
}

impl FromStr for ShiftStatus {
    type Err = ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_uppercase().as_str() {
            "ACTIVE" => Ok(ShiftStatus::Active),
            "CLOSED" => Ok(ShiftStatus::Closed),
            "SUSPENDED" => Ok(ShiftStatus::Suspended),
            _ => Err(ParseError::ShiftStatus(s.to_string())),
        }
    }
}

/// Shift row from database
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShiftRow {
    pub id: String,
    pub shift_number: String,
    pub operator_id: OperatorId,
    pub terminal_id: Option<String>,
    pub opening_cash: Decimal,
    pub closing_cash: Option<Decimal>,
    pub expected_cash: Option<Decimal>,
    pub variance: Option<Decimal>,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub status: String,
    pub sync_status: String,
    pub server_id: Option<String>,
    pub notes: Option<String>,
}

// There is deliberately no `impl Default for ShiftRow`.
//
// The one it replaces gave a shift an empty `operator_id` and a status of `ACTIVE`, so
// `ShiftRow::default()` was an open shift belonging to nobody. `shifts.operator_id` is
// `NOT NULL`; there is no such row to model. Nothing called it.

row_mapping! {
    /// Every column of `shifts` this till reads or writes, declared once.
    ///
    /// This file already had the shape half-right: `read_shift_row` was extracted and passed at
    /// all five row-shaped queries, which no other file in the tree managed. What extraction could
    /// not reach is the `INSERT` — a fourteen-name column list and a fourteen-slot `VALUES` clause
    /// sitting three hundred lines away from the reader that has to agree with it. A declaration
    /// links them.
    ///
    /// `INSERT OR REPLACE` is preserved from the statement it replaces, and it is load-bearing
    /// here in a way worth naming: a replace is a delete then an insert, and
    /// `offline_transactions.shift_id REFERENCES shifts(id)`. That foreign key has no `ON DELETE`
    /// clause today, so re-saving a shift leaves its transactions attached — measured, not
    /// assumed, and pinned by `replacing_a_shift_leaves_its_transactions_pointing_at_it`. Adding
    /// `ON DELETE CASCADE` there would turn every re-save into a history deletion.
    pub const SHIFT_ROW: RowMapping<ShiftRow> = for "shifts" {
        id,
        shift_number,
        operator_id     via column::OPERATOR_ID,
        terminal_id,
        opening_cash    via column::DECIMAL,
        closing_cash    via column::OPTIONAL_DECIMAL,
        expected_cash   via column::OPTIONAL_DECIMAL,
        variance        via column::OPTIONAL_DECIMAL,
        started_at,
        ended_at,
        status,
        sync_status,
        server_id,
        notes,
    } on_conflict OnConflict::Replace;
}

/// Everything after the projection, for the five queries that read a whole shift row.
const FROM_SHIFTS: &str = "FROM shifts";

impl Database {
    /// Saves a shift
    pub fn save_shift(&self, shift: &ShiftRow) -> SqliteResult<()> {
        self.insert(&SHIFT_ROW, shift)?;
        Ok(())
    }

    /// Gets a shift by ID
    pub fn get_shift_by_id(&self, id: &str) -> SqliteResult<Option<ShiftRow>> {
        self.select_one(
            SHIFT_ROW.reader(),
            &format!("{FROM_SHIFTS} WHERE id = ?1"),
            [id],
        )
    }

    /// Gets the active shift for an operator
    pub fn get_active_shift(&self, operator_id: &OperatorId) -> SqliteResult<Option<ShiftRow>> {
        self.select_one(
            SHIFT_ROW.reader(),
            &format!(
                "{FROM_SHIFTS} WHERE operator_id = ?1 AND status = 'ACTIVE' \
                 ORDER BY started_at DESC LIMIT 1"
            ),
            [operator_id.as_str()],
        )
    }

    /// Gets any active shift on this terminal
    pub fn get_current_active_shift(&self) -> SqliteResult<Option<ShiftRow>> {
        self.select_one(
            SHIFT_ROW.reader(),
            &format!("{FROM_SHIFTS} WHERE status = 'ACTIVE' ORDER BY started_at DESC LIMIT 1"),
            [],
        )
    }

    /// Starts a new shift
    pub fn start_shift(
        &self,
        id: &str,
        shift_number: &str,
        operator_id: &OperatorId,
        terminal_id: Option<&str>,
        opening_cash: Decimal,
    ) -> SqliteResult<ShiftRow> {
        let now = Utc::now().to_rfc3339();

        let shift = ShiftRow {
            id: id.to_string(),
            shift_number: shift_number.to_string(),
            operator_id: operator_id.clone(),
            terminal_id: terminal_id.map(String::from),
            opening_cash,
            closing_cash: None,
            expected_cash: None,
            variance: None,
            started_at: now,
            ended_at: None,
            status: "ACTIVE".to_string(),
            sync_status: "PENDING".to_string(),
            server_id: None,
            notes: None,
        };

        self.save_shift(&shift)?;
        Ok(shift)
    }

    /// Ends a shift
    pub fn end_shift(
        &self,
        id: &str,
        closing_cash: Decimal,
        expected_cash: Decimal,
        notes: Option<&str>,
    ) -> SqliteResult<()> {
        let now = Utc::now().to_rfc3339();
        let variance = closing_cash - expected_cash;

        let closing_cash_f = decimal_to_sqlite(&closing_cash);
        let expected_cash_f = decimal_to_sqlite(&expected_cash);
        let variance_f = decimal_to_sqlite(&variance);

        self.execute(
            r#"UPDATE shifts
               SET closing_cash = ?1, expected_cash = ?2, variance = ?3,
                   ended_at = ?4, status = 'CLOSED', notes = ?5
               WHERE id = ?6"#,
            &[
                &closing_cash_f,
                &expected_cash_f,
                &variance_f,
                &now,
                &notes,
                &id,
            ],
        )?;
        Ok(())
    }

    /// Suspends a shift
    pub fn suspend_shift(&self, id: &str) -> SqliteResult<()> {
        self.execute(
            "UPDATE shifts SET status = 'SUSPENDED' WHERE id = ?1",
            &[&id],
        )?;
        Ok(())
    }

    /// Gets shifts by date range
    pub fn get_shifts_in_range(
        &self,
        start_date: &str,
        end_date: &str,
    ) -> SqliteResult<Vec<ShiftRow>> {
        self.select_all(
            SHIFT_ROW.reader(),
            &format!(
                "{FROM_SHIFTS} \
                 WHERE date(started_at) >= date(?1) AND date(started_at) <= date(?2) \
                 ORDER BY started_at DESC"
            ),
            params![start_date, end_date],
        )
    }

    /// Gets pending shifts (not yet synced)
    pub fn get_pending_shifts(&self) -> SqliteResult<Vec<ShiftRow>> {
        self.select_all(
            SHIFT_ROW.reader(),
            &format!(
                "{FROM_SHIFTS} WHERE sync_status IN ('PENDING', 'FAILED') \
                 ORDER BY started_at ASC"
            ),
            [],
        )
    }

    /// Marks a shift as synced
    pub fn mark_shift_synced(&self, id: &str, server_id: &str) -> SqliteResult<()> {
        self.execute(
            "UPDATE shifts SET sync_status = 'SYNCED', server_id = ?1 WHERE id = ?2",
            &[&server_id, &id],
        )?;
        Ok(())
    }

    /// Generates the next shift number for today
    pub fn generate_shift_number(&self, terminal_code: &str) -> SqliteResult<String> {
        let today = Utc::now().format("%Y%m%d").to_string();
        let prefix = format!("{}-{}-", terminal_code, today);

        // Was `.unwrap_or(0)`. `COUNT(*)` returns exactly one row, so that default could only ever
        // absorb a real failure and reissue `-001` — and unlike `z_reports.report_number`,
        // `shifts.shift_number` is not a key, so the duplicate would be accepted in silence and
        // two shifts would reconcile against one number.
        let count: i64 = self.select_scalar(
            "SELECT COUNT(*) FROM shifts WHERE shift_number LIKE ?1",
            [format!("{}%", prefix)],
        )?;

        Ok(format!("{}{:03}", prefix, count + 1))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::migrations::run_migrations;
    use crate::operators::OperatorRow;
    use crate::projection::scalar;
    use pos_models::{OperatorName, OperatorRole};

    fn op_id(id: &str) -> OperatorId {
        OperatorId::new(id).expect("a non-blank id")
    }

    fn setup_db() -> Database {
        let db = Database::in_memory().unwrap();
        {
            let conn = db.connection();
            let conn = conn.lock();
            run_migrations(&conn).unwrap();
        }
        db
    }

    fn create_test_operator(db: &Database, id: &str) {
        let operator = OperatorRow {
            id: OperatorId::new(id).unwrap(),
            code: format!("C{id}"),
            employee_id: None,
            employee_number: None,
            name: OperatorName::new("Test Operator", None::<&str>).unwrap(),
            role: OperatorRole::Cashier,
            department: None,
            position: None,
            permissions: None,
            is_active: true,
        };
        db.save_operator(&operator).unwrap();
    }

    #[test]
    fn test_start_and_end_shift() {
        let db = setup_db();
        create_test_operator(&db, "op-1");

        // Start shift
        let shift = db
            .start_shift(
                "shift-1",
                "T01-20240101-001",
                &op_id("op-1"),
                Some("term-1"),
                Decimal::from(100),
            )
            .unwrap();

        assert_eq!(shift.status, "ACTIVE");
        assert_eq!(shift.opening_cash, Decimal::from(100));

        // Get active shift
        let active = db.get_active_shift(&op_id("op-1")).unwrap();
        assert!(active.is_some());

        // End shift
        db.end_shift(
            "shift-1",
            Decimal::from(350),
            Decimal::from(300),
            Some("Good shift"),
        )
        .unwrap();

        let ended = db.get_shift_by_id("shift-1").unwrap().unwrap();
        assert_eq!(ended.status, "CLOSED");
        assert_eq!(ended.variance, Some(Decimal::from(50)));

        // No more active shift
        let active = db.get_active_shift(&op_id("op-1")).unwrap();
        assert!(active.is_none());
    }

    #[test]
    fn test_shift_number_generation() {
        let db = setup_db();
        create_test_operator(&db, "op-1");

        let num1 = db.generate_shift_number("T01").unwrap();
        assert!(num1.starts_with("T01-"));
        assert!(num1.ends_with("-001"));

        // Create a shift with this number
        db.start_shift("s1", &num1, &op_id("op-1"), None, Decimal::ZERO)
            .unwrap();

        // Next number should be 002
        let num2 = db.generate_shift_number("T01").unwrap();
        assert!(num2.ends_with("-002"));
    }

    #[test]
    fn test_pending_shifts() {
        let db = setup_db();
        create_test_operator(&db, "op-1");
        create_test_operator(&db, "op-2");

        db.start_shift("s1", "S001", &op_id("op-1"), None, Decimal::from(100))
            .unwrap();
        db.start_shift("s2", "S002", &op_id("op-2"), None, Decimal::from(200))
            .unwrap();

        let pending = db.get_pending_shifts().unwrap();
        assert_eq!(pending.len(), 2);

        db.mark_shift_synced("s1", "server-s1").unwrap();

        let pending = db.get_pending_shifts().unwrap();
        assert_eq!(pending.len(), 1);
    }

    // ------------------------------------------------------------------------------------------
    // `SHIFT_ROW`. The patterns from task 04, plus the one this table needs and no other does.
    // ------------------------------------------------------------------------------------------

    /// A shift whose every column holds a value found nowhere else in the row.
    ///
    /// The four money columns carry four different amounts on purpose: `opening_cash` and
    /// `expected_cash` are adjacent in the projection and the same type, so equal values there
    /// would make a swap between them invisible.
    fn a_shift_with_no_two_columns_alike() -> ShiftRow {
        ShiftRow {
            id: "id-column".to_string(),
            shift_number: "shift-number-column".to_string(),
            operator_id: op_id("operator-id-column"),
            terminal_id: Some("terminal-id-column".to_string()),
            opening_cash: Decimal::from(11),
            closing_cash: Some(Decimal::from(22)),
            expected_cash: Some(Decimal::from(33)),
            variance: Some(Decimal::from(44)),
            started_at: "2026-08-24T10:00:00Z".to_string(),
            ended_at: Some("2026-08-24T18:00:00Z".to_string()),
            status: "SUSPENDED".to_string(),
            sync_status: "FAILED".to_string(),
            server_id: Some("server-id-column".to_string()),
            notes: Some("notes-column".to_string()),
        }
    }

    fn setup_db_with_operator() -> Database {
        // `shifts.operator_id` is a real foreign key and `PRAGMA foreign_keys` is ON, so the
        // fixture names a referent rather than blanking the column.
        let db = setup_db();
        create_test_operator(&db, "operator-id-column");
        db
    }

    #[test]
    fn the_shift_mapping_names_every_column_it_writes_in_the_order_it_reads_them() {
        assert_eq!(
            SHIFT_ROW.reader().select_list(),
            "id, shift_number, operator_id, terminal_id, opening_cash, closing_cash, \
             expected_cash, variance, started_at, ended_at, status, sync_status, server_id, notes"
        );
        assert_eq!(SHIFT_ROW.reader().width(), 14);
        assert_eq!(SHIFT_ROW.insert_column_names().count(), 14);
    }

    #[test]
    fn every_column_of_a_fully_distinct_shift_survives_the_round_trip() {
        let db = setup_db_with_operator();
        let written = a_shift_with_no_two_columns_alike();
        db.save_shift(&written).unwrap();

        let read = db
            .get_shift_by_id("id-column")
            .unwrap()
            .expect("the shift this test just wrote");
        assert_eq!(read.id, written.id);
        assert_eq!(read.shift_number, written.shift_number);
        assert_eq!(read.operator_id, written.operator_id);
        assert_eq!(read.terminal_id, written.terminal_id);
        assert_eq!(read.opening_cash, written.opening_cash);
        assert_eq!(read.closing_cash, written.closing_cash);
        assert_eq!(read.expected_cash, written.expected_cash);
        assert_eq!(read.variance, written.variance);
        assert_eq!(read.started_at, written.started_at);
        assert_eq!(read.ended_at, written.ended_at);
        assert_eq!(read.status, written.status);
        assert_eq!(read.sync_status, written.sync_status);
        assert_eq!(read.server_id, written.server_id);
        assert_eq!(read.notes, written.notes);
    }

    #[test]
    fn save_shift_puts_each_value_in_the_column_that_carries_its_name() {
        let db = setup_db_with_operator();
        db.save_shift(&a_shift_with_no_two_columns_alike()).unwrap();

        let conn = db.connection();
        let conn = conn.lock();
        for (column, expected) in [
            ("id", "id-column"),
            ("shift_number", "shift-number-column"),
            ("operator_id", "operator-id-column"),
            ("terminal_id", "terminal-id-column"),
            ("started_at", "2026-08-24T10:00:00Z"),
            ("ended_at", "2026-08-24T18:00:00Z"),
            ("status", "SUSPENDED"),
            ("sync_status", "FAILED"),
            ("server_id", "server-id-column"),
            ("notes", "notes-column"),
        ] {
            let matched: bool = scalar(
                &conn,
                &format!("SELECT {column} = ?1 FROM shifts"),
                [expected],
            )
            .unwrap();
            assert!(matched, "the `{column}` column does not hold `{expected}`");
        }

        // The four `REAL` columns, checked the same way. A separate loop because a `REAL` compared
        // against a bound `&str` is never equal in SQLite — the text-vs-numeric comparison would
        // make every one of these read `false` had they been folded into the loop above, and the
        // assertion would have looked like a real failure rather than a wrong test.
        for (column, expected) in [
            ("opening_cash", 11.0_f64),
            ("closing_cash", 22.0),
            ("expected_cash", 33.0),
            ("variance", 44.0),
        ] {
            let matched: bool = scalar(
                &conn,
                &format!("SELECT {column} = ?1 FROM shifts"),
                [expected],
            )
            .unwrap();
            assert!(matched, "the `{column}` column does not hold `{expected}`");
        }
    }

    #[test]
    fn a_second_write_of_the_same_shift_id_replaces_the_row() {
        let db = setup_db_with_operator();
        let first = a_shift_with_no_two_columns_alike();
        db.save_shift(&first).unwrap();
        db.save_shift(&ShiftRow {
            notes: Some("second-write".to_string()),
            ..first
        })
        .unwrap();

        let rows: i64 = db.select_scalar("SELECT COUNT(*) FROM shifts", []).unwrap();
        assert_eq!(rows, 1, "the second write inserted rather than replaced");
        assert_eq!(
            db.get_shift_by_id("id-column")
                .unwrap()
                .unwrap()
                .notes
                .as_deref(),
            Some("second-write")
        );
    }

    /// One nullable shift column, blanked, and what must still hold of its neighbours.
    struct AbsentShiftColumn {
        column: &'static str,
        blank: fn(&mut ShiftRow),
        assert_absent: fn(&ShiftRow),
    }

    #[test]
    fn a_null_in_one_shift_column_reaches_that_columns_field_and_no_other() {
        let db = setup_db_with_operator();
        let full = a_shift_with_no_two_columns_alike();

        let cases = [
            AbsentShiftColumn {
                column: "terminal_id",
                blank: |row| row.terminal_id = None,
                assert_absent: |row| {
                    assert_eq!(row.terminal_id, None);
                    assert_eq!(row.server_id.as_deref(), Some("server-id-column"));
                },
            },
            AbsentShiftColumn {
                column: "closing_cash",
                blank: |row| row.closing_cash = None,
                assert_absent: |row| {
                    assert_eq!(row.closing_cash, None);
                    assert_eq!(row.opening_cash, Decimal::from(11));
                    assert_eq!(row.expected_cash, Some(Decimal::from(33)));
                },
            },
            AbsentShiftColumn {
                column: "expected_cash",
                blank: |row| row.expected_cash = None,
                assert_absent: |row| {
                    assert_eq!(row.expected_cash, None);
                    assert_eq!(row.closing_cash, Some(Decimal::from(22)));
                    assert_eq!(row.variance, Some(Decimal::from(44)));
                },
            },
            AbsentShiftColumn {
                column: "variance",
                blank: |row| row.variance = None,
                assert_absent: |row| {
                    assert_eq!(row.variance, None);
                    assert_eq!(row.expected_cash, Some(Decimal::from(33)));
                },
            },
            AbsentShiftColumn {
                column: "ended_at",
                blank: |row| row.ended_at = None,
                assert_absent: |row| {
                    assert_eq!(row.ended_at, None);
                    assert_eq!(row.started_at, "2026-08-24T10:00:00Z");
                },
            },
            AbsentShiftColumn {
                column: "server_id",
                blank: |row| row.server_id = None,
                assert_absent: |row| {
                    assert_eq!(row.server_id, None);
                    assert_eq!(row.terminal_id.as_deref(), Some("terminal-id-column"));
                },
            },
            AbsentShiftColumn {
                column: "notes",
                blank: |row| row.notes = None,
                assert_absent: |row| {
                    assert_eq!(row.notes, None);
                    assert_eq!(row.server_id.as_deref(), Some("server-id-column"));
                },
            },
        ];

        for case in cases {
            let mut blanked = full.clone();
            (case.blank)(&mut blanked);
            db.save_shift(&blanked).unwrap();

            let read = db
                .get_shift_by_id("id-column")
                .unwrap()
                .unwrap_or_else(|| panic!("the shift written with `{}` blank", case.column));
            (case.assert_absent)(&read);
        }
    }

    /// Replacing a shift does **not** take its transactions with it.
    ///
    /// `INSERT OR REPLACE` is a delete followed by an insert, and
    /// `offline_transactions.shift_id REFERENCES shifts(id)` under `PRAGMA foreign_keys = ON`. The
    /// delete half raises the outstanding-violation count and the insert half lowers it again
    /// before the statement ends, so an immediate constraint sees nothing wrong and the children
    /// end up pointing at the re-created parent.
    ///
    /// This test was written the other way round — asserting the replace is *refused* — from
    /// reading the schema, and it failed. The behaviour is benign; the prediction was not. What
    /// makes it worth pinning is that it is one clause away from being destructive: add
    /// `ON DELETE CASCADE` to that foreign key, which reads like tightening the schema, and this
    /// statement silently deletes a shift's whole transaction history on every re-save.
    #[test]
    fn replacing_a_shift_leaves_its_transactions_pointing_at_it() {
        let db = setup_db_with_operator();
        db.save_shift(&a_shift_with_no_two_columns_alike()).unwrap();

        {
            let conn = db.connection();
            let conn = conn.lock();
            conn.execute(
                "INSERT INTO offline_transactions \
                 (offline_id, transaction_type, items_json, payments_json, \
                  subtotal, tax_total, grand_total, shift_id, created_at) \
                 VALUES ('txn-1', 'SALE', '[]', '[]', 0, 0, 0, 'id-column', '2026-08-24T11:00:00Z')",
                [],
            )
            .expect("a transaction against the shift");
        }

        db.save_shift(&ShiftRow {
            status: "CLOSED".to_string(),
            ..a_shift_with_no_two_columns_alike()
        })
        .expect("re-saving a shift that has transactions against it");

        let attached: i64 = db
            .select_scalar(
                "SELECT COUNT(*) FROM offline_transactions WHERE shift_id = 'id-column'",
                [],
            )
            .unwrap();
        assert_eq!(
            attached, 1,
            "re-saving the shift deleted its transaction history"
        );

        // The control: the same count reads 0 when the child genuinely goes away, so the 1 above
        // is the row surviving and not a query that cannot answer differently.
        {
            let conn = db.connection();
            let conn = conn.lock();
            conn.execute("DELETE FROM offline_transactions", [])
                .unwrap();
        }
        let gone: i64 = db
            .select_scalar(
                "SELECT COUNT(*) FROM offline_transactions WHERE shift_id = 'id-column'",
                [],
            )
            .unwrap();
        assert_eq!(gone, 0);
    }
}
