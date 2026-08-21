//! Shifts Repository
//!
//! Handles shift data storage and management.

use std::str::FromStr;

use chrono::Utc;
use rusqlite::{params, OptionalExtension, Result as SqliteResult};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use pos_models::OperatorId;

use super::Database;
use crate::column;
use crate::parse::ParseError;
use crate::{decimal_from_sqlite, decimal_to_sqlite};

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

fn read_shift_row(row: &rusqlite::Row) -> rusqlite::Result<ShiftRow> {
    Ok(ShiftRow {
        id: row.get(0)?,
        shift_number: row.get(1)?,
        operator_id: column::operator_id(row, 2)?,
        terminal_id: row.get(3)?,
        opening_cash: decimal_from_sqlite(row.get::<_, f64>(4)?),
        closing_cash: row.get::<_, Option<f64>>(5)?.map(decimal_from_sqlite),
        expected_cash: row.get::<_, Option<f64>>(6)?.map(decimal_from_sqlite),
        variance: row.get::<_, Option<f64>>(7)?.map(decimal_from_sqlite),
        started_at: row.get(8)?,
        ended_at: row.get(9)?,
        status: row.get(10)?,
        sync_status: row.get(11)?,
        server_id: row.get(12)?,
        notes: row.get(13)?,
    })
}

impl Database {
    /// Saves a shift
    pub fn save_shift(&self, shift: &ShiftRow) -> SqliteResult<()> {
        let opening_cash = decimal_to_sqlite(&shift.opening_cash);
        let closing_cash = shift.closing_cash.as_ref().map(decimal_to_sqlite);
        let expected_cash = shift.expected_cash.as_ref().map(decimal_to_sqlite);
        let variance = shift.variance.as_ref().map(decimal_to_sqlite);

        self.execute(
            r#"INSERT OR REPLACE INTO shifts
               (id, shift_number, operator_id, terminal_id, opening_cash, closing_cash,
                expected_cash, variance, started_at, ended_at, status, sync_status, server_id, notes)
               VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)"#,
            &[
                &shift.id,
                &shift.shift_number,
                &shift.operator_id.as_str(),
                &shift.terminal_id,
                &opening_cash,
                &closing_cash,
                &expected_cash,
                &variance,
                &shift.started_at,
                &shift.ended_at,
                &shift.status,
                &shift.sync_status,
                &shift.server_id,
                &shift.notes,
            ],
        )?;
        Ok(())
    }

    /// Gets a shift by ID
    pub fn get_shift_by_id(&self, id: &str) -> SqliteResult<Option<ShiftRow>> {
        let conn = self.connection();
        let conn = conn.lock();

        conn.query_row(
            r#"SELECT id, shift_number, operator_id, terminal_id, opening_cash, closing_cash,
                      expected_cash, variance, started_at, ended_at, status, sync_status, server_id, notes
               FROM shifts WHERE id = ?1"#,
            [id],
            read_shift_row,
        )
        .optional()
    }

    /// Gets the active shift for an operator
    pub fn get_active_shift(&self, operator_id: &OperatorId) -> SqliteResult<Option<ShiftRow>> {
        let conn = self.connection();
        let conn = conn.lock();

        conn.query_row(
            r#"SELECT id, shift_number, operator_id, terminal_id, opening_cash, closing_cash,
                      expected_cash, variance, started_at, ended_at, status, sync_status, server_id, notes
               FROM shifts WHERE operator_id = ?1 AND status = 'ACTIVE'
               ORDER BY started_at DESC LIMIT 1"#,
            [operator_id.as_str()],
            read_shift_row,
        )
        .optional()
    }

    /// Gets any active shift on this terminal
    pub fn get_current_active_shift(&self) -> SqliteResult<Option<ShiftRow>> {
        let conn = self.connection();
        let conn = conn.lock();

        conn.query_row(
            r#"SELECT id, shift_number, operator_id, terminal_id, opening_cash, closing_cash,
                      expected_cash, variance, started_at, ended_at, status, sync_status, server_id, notes
               FROM shifts WHERE status = 'ACTIVE'
               ORDER BY started_at DESC LIMIT 1"#,
            [],
            read_shift_row,
        )
        .optional()
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
        let conn = self.connection();
        let conn = conn.lock();

        let mut stmt = conn.prepare(
            r#"SELECT id, shift_number, operator_id, terminal_id, opening_cash, closing_cash,
                      expected_cash, variance, started_at, ended_at, status, sync_status, server_id, notes
               FROM shifts
               WHERE date(started_at) >= date(?1) AND date(started_at) <= date(?2)
               ORDER BY started_at DESC"#,
        )?;

        let rows = stmt.query_map(params![start_date, end_date], read_shift_row)?;

        rows.collect()
    }

    /// Gets pending shifts (not yet synced)
    pub fn get_pending_shifts(&self) -> SqliteResult<Vec<ShiftRow>> {
        let conn = self.connection();
        let conn = conn.lock();

        let mut stmt = conn.prepare(
            r#"SELECT id, shift_number, operator_id, terminal_id, opening_cash, closing_cash,
                      expected_cash, variance, started_at, ended_at, status, sync_status, server_id, notes
               FROM shifts
               WHERE sync_status IN ('PENDING', 'FAILED')
               ORDER BY started_at ASC"#,
        )?;

        let rows = stmt.query_map([], read_shift_row)?;

        rows.collect()
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
        let conn = self.connection();
        let conn = conn.lock();

        let today = Utc::now().format("%Y%m%d").to_string();
        let prefix = format!("{}-{}-", terminal_code, today);

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM shifts WHERE shift_number LIKE ?1",
                [format!("{}%", prefix)],
                |row| row.get(0),
            )
            .unwrap_or(0);

        Ok(format!("{}{:03}", prefix, count + 1))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::migrations::run_migrations;
    use crate::operators::OperatorRow;

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
            id: id.to_string(),
            code: format!("C{}", id),
            name: "Test Operator".to_string(),
            pin_hash: "hash".to_string(),
            ..Default::default()
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
}
