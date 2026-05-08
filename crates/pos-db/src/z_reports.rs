//! Z-Reports Repository
//!
//! Handles Z-Report (end-of-day) data storage and aggregation.

use rust_decimal::Decimal;
use rusqlite::{params, Result as SqliteResult};

use super::Database;
use crate::{decimal_from_sqlite, decimal_to_sqlite};
use pos_models::{VarianceStatus, ZReport};

/// Day totals for Z-Report aggregation
#[derive(Debug, Clone, Default)]
pub struct DayTotals {
    /// Total number of transactions
    pub transaction_count: i64,
    /// Gross sales before discounts/returns
    pub gross_sales: Decimal,
    /// Total discounts applied
    pub discounts: Decimal,
    /// Total returns
    pub returns: Decimal,
    /// Net sales (gross - discounts - returns)
    pub net_sales: Decimal,
    /// Tax collected
    pub tax_collected: Decimal,
    /// Cash payments total
    pub cash_total: Decimal,
    /// Card payments total
    pub card_total: Decimal,
    /// Wallet payments total
    pub wallet_total: Decimal,
    /// Credit sales total
    pub credit_total: Decimal,
    /// Cash payment count
    pub cash_count: i64,
    /// Card payment count
    pub card_count: i64,
    /// Wallet payment count
    pub wallet_count: i64,
    /// Credit payment count
    pub credit_count: i64,
    /// Cash refunds (for returns)
    pub cash_refunds: Decimal,
    /// Return count
    pub return_count: i64,
    /// Void count
    pub void_count: i64,
    /// Void total
    pub void_total: Decimal,
}

impl Database {
    /// Counts Z-Reports for a specific date and terminal
    pub fn count_z_reports_for_date(&self, terminal_id: &str, date: &str) -> SqliteResult<i64> {
        let conn = self.connection();
        let conn = conn.lock();

        conn.query_row(
            "SELECT COUNT(*) FROM z_reports WHERE terminal_id = ?1 AND report_date = ?2",
            params![terminal_id, date],
            |row| row.get(0),
        )
    }

    /// Counts open (active) shifts for a terminal
    pub fn count_open_shifts(&self, terminal_id: &str) -> SqliteResult<i64> {
        let conn = self.connection();
        let conn = conn.lock();

        conn.query_row(
            "SELECT COUNT(*) FROM shifts WHERE terminal_id = ?1 AND status = 'ACTIVE'",
            [terminal_id],
            |row| row.get(0),
        )
    }

    /// Counts pending sync transactions
    pub fn count_pending_sync(&self) -> SqliteResult<i64> {
        let conn = self.connection();
        let conn = conn.lock();

        conn.query_row(
            "SELECT COUNT(*) FROM offline_transactions WHERE sync_status IN ('PENDING', 'FAILED')",
            [],
            |row| row.get(0),
        )
    }

    /// Counts shifts for a specific date
    pub fn count_shifts_for_date(&self, terminal_id: &str, date: &str) -> SqliteResult<i64> {
        let conn = self.connection();
        let conn = conn.lock();

        conn.query_row(
            r#"SELECT COUNT(*) FROM shifts
               WHERE terminal_id = ?1
               AND date(started_at) = date(?2)"#,
            params![terminal_id, date],
            |row| row.get(0),
        )
    }

    /// Gets aggregated day totals from transactions
    pub fn get_day_totals(&self, terminal_id: &str, date: &str) -> SqliteResult<DayTotals> {
        let conn = self.connection();
        let conn = conn.lock();

        // Query transaction summary for the day
        let result = conn.query_row(
            r#"SELECT
                COALESCE(COUNT(*), 0) as transaction_count,
                COALESCE(SUM(CASE WHEN type = 'SALE' THEN total ELSE 0 END), 0) as gross_sales,
                COALESCE(SUM(CASE WHEN type = 'SALE' THEN discount ELSE 0 END), 0) as discounts,
                COALESCE(SUM(CASE WHEN type = 'RETURN' THEN ABS(total) ELSE 0 END), 0) as returns,
                COALESCE(SUM(CASE WHEN type = 'SALE' THEN tax ELSE 0 END), 0) as tax_collected,
                COALESCE(SUM(CASE WHEN type = 'RETURN' THEN 1 ELSE 0 END), 0) as return_count,
                COALESCE(SUM(CASE WHEN status = 'VOIDED' THEN 1 ELSE 0 END), 0) as void_count,
                COALESCE(SUM(CASE WHEN status = 'VOIDED' THEN ABS(total) ELSE 0 END), 0) as void_total
               FROM offline_transactions
               WHERE terminal_id = ?1 AND date(created_at) = date(?2)"#,
            params![terminal_id, date],
            |row| {
                Ok(DayTotals {
                    transaction_count: row.get(0)?,
                    gross_sales: decimal_from_sqlite(row.get::<_, f64>(1)?),
                    discounts: decimal_from_sqlite(row.get::<_, f64>(2)?),
                    returns: decimal_from_sqlite(row.get::<_, f64>(3)?),
                    tax_collected: decimal_from_sqlite(row.get::<_, f64>(4)?),
                    return_count: row.get(5)?,
                    void_count: row.get(6)?,
                    void_total: decimal_from_sqlite(row.get::<_, f64>(7)?),
                    ..Default::default()
                })
            },
        ).unwrap_or_default();

        // Query payment method breakdown
        let payment_totals = conn.query_row(
            r#"SELECT
                COALESCE(SUM(CASE WHEN payment_method = 'CASH' AND type = 'SALE' THEN total ELSE 0 END), 0) as cash_total,
                COALESCE(SUM(CASE WHEN payment_method = 'CARD' AND type = 'SALE' THEN total ELSE 0 END), 0) as card_total,
                COALESCE(SUM(CASE WHEN payment_method = 'WALLET' AND type = 'SALE' THEN total ELSE 0 END), 0) as wallet_total,
                COALESCE(SUM(CASE WHEN payment_method = 'CREDIT' AND type = 'SALE' THEN total ELSE 0 END), 0) as credit_total,
                COALESCE(SUM(CASE WHEN payment_method = 'CASH' AND type = 'SALE' THEN 1 ELSE 0 END), 0) as cash_count,
                COALESCE(SUM(CASE WHEN payment_method = 'CARD' AND type = 'SALE' THEN 1 ELSE 0 END), 0) as card_count,
                COALESCE(SUM(CASE WHEN payment_method = 'WALLET' AND type = 'SALE' THEN 1 ELSE 0 END), 0) as wallet_count,
                COALESCE(SUM(CASE WHEN payment_method = 'CREDIT' AND type = 'SALE' THEN 1 ELSE 0 END), 0) as credit_count,
                COALESCE(SUM(CASE WHEN payment_method = 'CASH' AND type = 'RETURN' THEN ABS(total) ELSE 0 END), 0) as cash_refunds
               FROM offline_transactions
               WHERE terminal_id = ?1 AND date(created_at) = date(?2) AND status != 'VOIDED'"#,
            params![terminal_id, date],
            |row| {
                Ok((
                    decimal_from_sqlite(row.get::<_, f64>(0)?),   // cash_total
                    decimal_from_sqlite(row.get::<_, f64>(1)?),   // card_total
                    decimal_from_sqlite(row.get::<_, f64>(2)?),   // wallet_total
                    decimal_from_sqlite(row.get::<_, f64>(3)?),   // credit_total
                    row.get::<_, i64>(4)?,                        // cash_count
                    row.get::<_, i64>(5)?,                        // card_count
                    row.get::<_, i64>(6)?,                        // wallet_count
                    row.get::<_, i64>(7)?,                        // credit_count
                    decimal_from_sqlite(row.get::<_, f64>(8)?),   // cash_refunds
                ))
            },
        ).unwrap_or((Decimal::ZERO, Decimal::ZERO, Decimal::ZERO, Decimal::ZERO, 0, 0, 0, 0, Decimal::ZERO));

        Ok(DayTotals {
            transaction_count: result.transaction_count,
            gross_sales: result.gross_sales,
            discounts: result.discounts,
            returns: result.returns,
            net_sales: result.gross_sales - result.discounts - result.returns,
            tax_collected: result.tax_collected,
            cash_total: payment_totals.0,
            card_total: payment_totals.1,
            wallet_total: payment_totals.2,
            credit_total: payment_totals.3,
            cash_count: payment_totals.4,
            card_count: payment_totals.5,
            wallet_count: payment_totals.6,
            credit_count: payment_totals.7,
            cash_refunds: payment_totals.8,
            return_count: result.return_count,
            void_count: result.void_count,
            void_total: result.void_total,
        })
    }

    /// Saves a Z-Report to the database
    pub fn save_z_report(&self, report: &ZReport) -> SqliteResult<()> {
        self.execute(
            r#"INSERT INTO z_reports (
                report_number, report_date, terminal_id, currency,
                total_shifts, total_transactions, gross_sales, discounts, returns,
                net_sales, tax_collected, cash_total, card_total, wallet_total, credit_total,
                opening_float, expected_cash, actual_cash, variance, variance_status,
                generated_at, synced, server_id
            ) VALUES (
                ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15,
                ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23
            )"#,
            &[
                &report.report_number,
                &report.report_date,
                &report.terminal_id,
                &report.currency,
                &(report.total_shifts as i64),
                &(report.total_transactions as i64),
                &decimal_to_sqlite(&report.gross_sales),
                &decimal_to_sqlite(&report.discounts),
                &decimal_to_sqlite(&report.returns),
                &decimal_to_sqlite(&report.net_sales),
                &decimal_to_sqlite(&report.tax_collected),
                &decimal_to_sqlite(&report.cash_total),
                &decimal_to_sqlite(&report.card_total),
                &decimal_to_sqlite(&report.wallet_total),
                &decimal_to_sqlite(&report.credit_total),
                &decimal_to_sqlite(&report.opening_float),
                &decimal_to_sqlite(&report.expected_cash),
                &decimal_to_sqlite(&report.actual_cash),
                &decimal_to_sqlite(&report.variance),
                &report.variance_status.as_str(),
                &report.generated_at,
                &report.synced,
                &report.server_id,
            ],
        )?;
        Ok(())
    }

    /// Marks a day as closed in a separate tracking table
    pub fn mark_day_closed(&self, terminal_id: &str, date: &str) -> SqliteResult<()> {
        let now = chrono::Utc::now().to_rfc3339();
        self.execute(
            r#"INSERT OR REPLACE INTO day_closures (terminal_id, date, closed_at)
               VALUES (?1, ?2, ?3)"#,
            &[&terminal_id, &date, &now],
        )?;
        Ok(())
    }

    /// Marks a Z-Report as synced
    pub fn mark_z_report_synced(&self, report_number: &str, server_id: &str) -> SqliteResult<()> {
        self.execute(
            "UPDATE z_reports SET synced = 1, server_id = ?1 WHERE report_number = ?2",
            &[&server_id, &report_number],
        )?;
        Ok(())
    }

    /// Gets Z-Reports in a date range
    pub fn get_z_reports_in_range(
        &self,
        start_date: &str,
        end_date: &str,
    ) -> SqliteResult<Vec<ZReport>> {
        let conn = self.connection();
        let conn = conn.lock();

        let mut stmt = conn.prepare(
            r#"SELECT
                report_number, report_date, terminal_id, currency,
                total_shifts, total_transactions, gross_sales, discounts, returns,
                net_sales, tax_collected, cash_total, card_total, wallet_total, credit_total,
                opening_float, expected_cash, actual_cash, variance, variance_status,
                generated_at, synced, server_id
               FROM z_reports
               WHERE report_date >= ?1 AND report_date <= ?2
               ORDER BY report_date DESC"#,
        )?;

        let rows = stmt.query_map(params![start_date, end_date], |row: &rusqlite::Row| {
            let variance_str: String = row.get(19)?;
            let variance_status = match variance_str.as_str() {
                "short" => VarianceStatus::Short,
                "over" => VarianceStatus::Over,
                _ => VarianceStatus::Balanced,
            };

            Ok(ZReport {
                report_number: row.get(0)?,
                report_date: row.get(1)?,
                terminal_id: row.get(2)?,
                currency: row.get(3)?,
                total_shifts: row.get::<_, i64>(4)? as u32,
                total_transactions: row.get::<_, i64>(5)? as u32,
                gross_sales: decimal_from_sqlite(row.get::<_, f64>(6)?),
                discounts: decimal_from_sqlite(row.get::<_, f64>(7)?),
                returns: decimal_from_sqlite(row.get::<_, f64>(8)?),
                net_sales: decimal_from_sqlite(row.get::<_, f64>(9)?),
                tax_collected: decimal_from_sqlite(row.get::<_, f64>(10)?),
                cash_total: decimal_from_sqlite(row.get::<_, f64>(11)?),
                card_total: decimal_from_sqlite(row.get::<_, f64>(12)?),
                wallet_total: decimal_from_sqlite(row.get::<_, f64>(13)?),
                credit_total: decimal_from_sqlite(row.get::<_, f64>(14)?),
                opening_float: decimal_from_sqlite(row.get::<_, f64>(15)?),
                expected_cash: decimal_from_sqlite(row.get::<_, f64>(16)?),
                actual_cash: decimal_from_sqlite(row.get::<_, f64>(17)?),
                variance: decimal_from_sqlite(row.get::<_, f64>(18)?),
                variance_status,
                generated_at: row.get(20)?,
                synced: row.get(21)?,
                server_id: row.get(22)?,
                ..Default::default()
            })
        })?;

        rows.collect()
    }

    /// Gets the next Z-Report sequence number for a date
    pub fn get_next_z_report_sequence(&self, terminal_id: &str, date_part: &str) -> SqliteResult<i64> {
        let conn = self.connection();
        let conn = conn.lock();

        let prefix = format!("Z-{}-{}-", terminal_id, date_part);

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM z_reports WHERE report_number LIKE ?1",
                [format!("{}%", prefix)],
                |row| row.get(0),
            )
            .unwrap_or(0);

        Ok(count + 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::migrations::run_migrations;

    fn setup_db() -> Database {
        let db = Database::in_memory().unwrap();
        {
            let conn = db.connection();
            let conn = conn.lock();
            run_migrations(&conn).unwrap();
        }
        db
    }

    #[test]
    fn test_count_z_reports_for_date() {
        let db = setup_db();
        let count = db.count_z_reports_for_date("POS-001", "2024-01-15").unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn test_count_open_shifts() {
        let db = setup_db();
        let count = db.count_open_shifts("POS-001").unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn test_count_pending_sync() {
        let db = setup_db();
        let count = db.count_pending_sync().unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn test_get_day_totals_empty() {
        let db = setup_db();
        let totals = db.get_day_totals("POS-001", "2024-01-15").unwrap();
        assert_eq!(totals.transaction_count, 0);
        assert_eq!(totals.gross_sales, Decimal::ZERO);
    }

    #[test]
    fn test_get_next_z_report_sequence() {
        let db = setup_db();
        let seq = db.get_next_z_report_sequence("POS-001", "20240115").unwrap();
        assert_eq!(seq, 1);
    }
}
