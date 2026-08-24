//! Z-Reports Repository
//!
//! Handles Z-Report (end-of-day) data storage and aggregation.

use rusqlite::{params, Result as SqliteResult};
use rust_decimal::Decimal;

use super::Database;
use crate::column;
use crate::projection::{read_one, OnConflict};
use crate::{row_mapping, row_reader};
use pos_models::VarianceStatus;

// ============================================================================
// What the store keeps, and what the domain adds
// ============================================================================

/// The 23 columns of `z_reports`, which is not the whole Z-report.
///
/// `pos_models::ZReport` has 39 fields. Sixteen of them — `tax_rate`, the per-method payment
/// counts, `total_cash_in`/`total_cash_out`, `return_total`, and the whole `shifts` breakdown —
/// are computed at report time and never written here. The read this replaces closed with
/// `..Default::default()`, which meant a column added to `z_reports` would be read by nobody and
/// the report would carry a zero for it with nothing failing.
///
/// So the two shapes are separated rather than reconciled. `ZReportRow` is exactly the store's
/// 23, and widening it into a `ZReport` happens in one named place in `pos-services`. A column
/// added to the table and not to this struct is E0063 at the macro.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZReportRow {
    pub report_number: String,
    pub report_date: String,
    pub terminal_id: String,
    pub currency: String,
    pub total_shifts: i64,
    pub total_transactions: i64,
    pub gross_sales: Decimal,
    pub discounts: Decimal,
    pub returns: Decimal,
    pub net_sales: Decimal,
    pub tax_collected: Decimal,
    pub cash_total: Decimal,
    pub card_total: Decimal,
    pub wallet_total: Decimal,
    pub credit_total: Decimal,
    pub opening_float: Decimal,
    pub expected_cash: Decimal,
    pub actual_cash: Decimal,
    pub variance: Decimal,
    pub variance_status: VarianceStatus,
    pub generated_at: String,
    pub synced: bool,
    pub server_id: Option<String>,
}

row_mapping! {
    /// Every column of `z_reports`, declared once.
    ///
    /// **`OnConflict::Fail`, and that is a decision rather than a default.** `report_number` is
    /// `TEXT PRIMARY KEY` and the shipped writer was a plain `INSERT`, so re-running day close
    /// errors today. Under `Replace` it would silently overwrite a finalised fiscal record with a
    /// second reading of the same day — pinned by
    /// `a_second_save_of_the_same_report_number_is_refused`.
    pub const Z_REPORT_ROW: RowMapping<ZReportRow> = for "z_reports" {
        report_number,
        report_date,
        terminal_id,
        currency,
        total_shifts,
        total_transactions,
        gross_sales     via column::DECIMAL,
        discounts       via column::DECIMAL,
        returns         via column::DECIMAL,
        net_sales       via column::DECIMAL,
        tax_collected   via column::DECIMAL,
        cash_total      via column::DECIMAL,
        card_total      via column::DECIMAL,
        wallet_total    via column::DECIMAL,
        credit_total    via column::DECIMAL,
        opening_float   via column::DECIMAL,
        expected_cash   via column::DECIMAL,
        actual_cash     via column::DECIMAL,
        variance        via column::DECIMAL,
        variance_status via column::VARIANCE_STATUS,
        generated_at,
        synced,
        server_id,
    } on_conflict OnConflict::Fail;
}

// SAFETY-GAP: every `via column::DECIMAL` above crosses an `f64`. `decimal_from_sqlite` answers
// `Decimal::ZERO` for a value it cannot convert and `decimal_to_sqlite` answers `0.0`, both
// silently — so a Z-report's gross sales, its expected cash and its variance can each read zero
// because a conversion failed rather than because the day was empty, and nothing distinguishes
// the two. This is the record a cashier is held to at end of day and the one this till hands the
// tax authority. It is out of scope here deliberately: the fix is a two-function change in
// `crates/pos-db/src/column.rs` and it belongs to `project/till/issue/money-and-currency-in-the-till`.
// Left as a marked gap rather than a clean file, because a rewritten file that looks finished is
// how a deferred defect stops being visible. Recorded 2026-08-24 by
// `project/till/issue/positional-row-access-in-pos-db` task 10.

/// The day's transaction aggregates, read straight out of `offline_transactions`.
///
/// Read-only by construction: a [`RowReader`](crate::projection::RowReader) and never a
/// `RowMapping`, because there is no table these eight values are rows of. Writing one is a
/// compile error rather than a check nobody runs.
///
/// Deliberately **not** `Default`. See [`DayTotals`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DayTotalsRow {
    pub transaction_count: i64,
    pub gross_sales: Decimal,
    pub discounts: Decimal,
    pub returns: Decimal,
    pub tax_collected: Decimal,
    pub return_count: i64,
    pub void_count: i64,
    pub void_total: Decimal,
}

row_reader! {
    /// The eight day aggregates, expression and result name declared together.
    ///
    /// This is the shape the `field from ("expr" as "name")` entry exists for: every column here
    /// is an expression, so the name the reader binds and the SQL that produces it are two
    /// different strings that must stay in step. Declaring them as one entry is the only place in
    /// this crate where that is true.
    pub const DAY_TOTALS_ROW: RowReader<DayTotalsRow> = {
        transaction_count from ("COALESCE(COUNT(*), 0)" as "transaction_count"),
        gross_sales from
            ("COALESCE(SUM(CASE WHEN type = 'SALE' THEN total ELSE 0 END), 0)" as "gross_sales")
            via column::DECIMAL,
        discounts from
            ("COALESCE(SUM(CASE WHEN type = 'SALE' THEN discount ELSE 0 END), 0)" as "discounts")
            via column::DECIMAL,
        returns from
            ("COALESCE(SUM(CASE WHEN type = 'RETURN' THEN ABS(total) ELSE 0 END), 0)" as "returns")
            via column::DECIMAL,
        tax_collected from
            ("COALESCE(SUM(CASE WHEN type = 'SALE' THEN tax ELSE 0 END), 0)" as "tax_collected")
            via column::DECIMAL,
        return_count from
            ("COALESCE(SUM(CASE WHEN type = 'RETURN' THEN 1 ELSE 0 END), 0)" as "return_count"),
        void_count from
            ("COALESCE(SUM(CASE WHEN status = 'VOIDED' THEN 1 ELSE 0 END), 0)" as "void_count"),
        void_total from
            ("COALESCE(SUM(CASE WHEN status = 'VOIDED' THEN ABS(total) ELSE 0 END), 0)"
                as "void_total")
            via column::DECIMAL,
    };
}

/// The day's takings split by how the customer paid.
///
/// A separate query from [`DayTotalsRow`] because it excludes voided transactions and that one
/// does not — the two `WHERE` clauses genuinely differ, so this is two reads and not one.
///
/// **Deliberately not `Default`, and not a tuple.** It was a nine-element tuple, read positionally
/// and unpacked positionally forty lines away as `payment_totals.0` through `.8`, with the column
/// meanings carried in trailing comments. Four of those nine are the amounts a cashier reconciles
/// the drawer against.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaymentBreakdownRow {
    pub cash_total: Decimal,
    pub card_total: Decimal,
    pub wallet_total: Decimal,
    pub credit_total: Decimal,
    pub cash_count: i64,
    pub card_count: i64,
    pub wallet_count: i64,
    pub credit_count: i64,
    pub cash_refunds: Decimal,
}

row_reader! {
    /// The nine payment-method aggregates.
    pub const PAYMENT_BREAKDOWN_ROW: RowReader<PaymentBreakdownRow> = {
        cash_total from
            ("COALESCE(SUM(CASE WHEN payment_method = 'CASH' AND type = 'SALE' THEN total ELSE 0 END), 0)"
                as "cash_total")
            via column::DECIMAL,
        card_total from
            ("COALESCE(SUM(CASE WHEN payment_method = 'CARD' AND type = 'SALE' THEN total ELSE 0 END), 0)"
                as "card_total")
            via column::DECIMAL,
        wallet_total from
            ("COALESCE(SUM(CASE WHEN payment_method = 'WALLET' AND type = 'SALE' THEN total ELSE 0 END), 0)"
                as "wallet_total")
            via column::DECIMAL,
        credit_total from
            ("COALESCE(SUM(CASE WHEN payment_method = 'CREDIT' AND type = 'SALE' THEN total ELSE 0 END), 0)"
                as "credit_total")
            via column::DECIMAL,
        cash_count from
            ("COALESCE(SUM(CASE WHEN payment_method = 'CASH' AND type = 'SALE' THEN 1 ELSE 0 END), 0)"
                as "cash_count"),
        card_count from
            ("COALESCE(SUM(CASE WHEN payment_method = 'CARD' AND type = 'SALE' THEN 1 ELSE 0 END), 0)"
                as "card_count"),
        wallet_count from
            ("COALESCE(SUM(CASE WHEN payment_method = 'WALLET' AND type = 'SALE' THEN 1 ELSE 0 END), 0)"
                as "wallet_count"),
        credit_count from
            ("COALESCE(SUM(CASE WHEN payment_method = 'CREDIT' AND type = 'SALE' THEN 1 ELSE 0 END), 0)"
                as "credit_count"),
        cash_refunds from
            ("COALESCE(SUM(CASE WHEN payment_method = 'CASH' AND type = 'RETURN' THEN ABS(total) ELSE 0 END), 0)"
                as "cash_refunds")
            via column::DECIMAL,
    };
}

/// Day totals for Z-Report aggregation
///
/// **No `Default`, and its absence is the mechanism rather than a style preference.** The
/// assembly below used to close with `.unwrap_or_default()` on the aggregate read: a failed query
/// became a day with no sales, no tax and no takings, indistinguishable from a genuinely quiet
/// day, on the record the day is closed against. Deleting the derive makes that call *stop
/// compiling*, so the swallow cannot come back by hand — which asserting on the current behaviour
/// would not have prevented.
#[derive(Debug, Clone)]
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

impl DayTotals {
    /// Assembles the day from its two reads. The only place `net_sales` is derived.
    fn from_reads(aggregates: DayTotalsRow, payments: PaymentBreakdownRow) -> Self {
        Self {
            transaction_count: aggregates.transaction_count,
            gross_sales: aggregates.gross_sales,
            discounts: aggregates.discounts,
            returns: aggregates.returns,
            net_sales: aggregates.gross_sales - aggregates.discounts - aggregates.returns,
            tax_collected: aggregates.tax_collected,
            cash_total: payments.cash_total,
            card_total: payments.card_total,
            wallet_total: payments.wallet_total,
            credit_total: payments.credit_total,
            cash_count: payments.cash_count,
            card_count: payments.card_count,
            wallet_count: payments.wallet_count,
            credit_count: payments.credit_count,
            cash_refunds: payments.cash_refunds,
            return_count: aggregates.return_count,
            void_count: aggregates.void_count,
            void_total: aggregates.void_total,
        }
    }
}

/// An aggregate query over zero rows still returns one row of zeros, so `None` here means the
/// statement did not run as an aggregate at all — not that the day was quiet.
fn no_aggregate_row() -> rusqlite::Error {
    rusqlite::Error::QueryReturnedNoRows
}

impl Database {
    /// Counts Z-Reports for a specific date and terminal
    pub fn count_z_reports_for_date(&self, terminal_id: &str, date: &str) -> SqliteResult<i64> {
        self.select_scalar(
            "SELECT COUNT(*) FROM z_reports WHERE terminal_id = ?1 AND report_date = ?2",
            params![terminal_id, date],
        )
    }

    /// Counts open (active) shifts for a terminal
    pub fn count_open_shifts(&self, terminal_id: &str) -> SqliteResult<i64> {
        self.select_scalar(
            "SELECT COUNT(*) FROM shifts WHERE terminal_id = ?1 AND status = 'ACTIVE'",
            [terminal_id],
        )
    }

    /// Counts pending sync transactions
    pub fn count_pending_sync(&self) -> SqliteResult<i64> {
        self.select_scalar(
            "SELECT COUNT(*) FROM offline_transactions WHERE sync_status IN ('PENDING', 'FAILED')",
            [],
        )
    }

    /// Counts shifts for a specific date
    pub fn count_shifts_for_date(&self, terminal_id: &str, date: &str) -> SqliteResult<i64> {
        self.select_scalar(
            "SELECT COUNT(*) FROM shifts \
             WHERE terminal_id = ?1 AND date(started_at) = date(?2)",
            params![terminal_id, date],
        )
    }

    /// Gets aggregated day totals from transactions
    ///
    /// Two reads, **under one lock held across both**. Dropping it between them would let a
    /// concurrent write land in the gap, and `gross_sales` and `cash_total` would then describe
    /// different sets of transactions on one printed report.
    ///
    /// Both use the free [`read_one`] over `&Connection` rather than `Database::select_one`. The
    /// connection guard is not reentrant, so a `&self` call here would hang day close with no
    /// error and no panic.
    ///
    /// Neither read swallows its failure any more. Both used to: the aggregate query ended
    /// `.unwrap_or_default()` and the payment query ended `.unwrap_or((Decimal::ZERO, …))` — a
    /// tuple literal, which the `Default` derive did not reach and which the first draft of this
    /// change did not see. A `SqliteResult` now leaves this function either way.
    pub fn get_day_totals(&self, terminal_id: &str, date: &str) -> SqliteResult<DayTotals> {
        let conn = self.connection();
        let conn = conn.lock();

        let aggregates = read_one(
            &conn,
            &DAY_TOTALS_ROW,
            "FROM offline_transactions WHERE terminal_id = ?1 AND date(created_at) = date(?2)",
            params![terminal_id, date],
        )?
        .ok_or_else(no_aggregate_row)?;

        let payments = read_one(
            &conn,
            &PAYMENT_BREAKDOWN_ROW,
            "FROM offline_transactions \
             WHERE terminal_id = ?1 AND date(created_at) = date(?2) AND status != 'VOIDED'",
            params![terminal_id, date],
        )?
        .ok_or_else(no_aggregate_row)?;

        Ok(DayTotals::from_reads(aggregates, payments))
    }

    /// Saves a Z-Report to the database
    ///
    /// Takes the store's own 23 columns, not a `pos_models::ZReport`. The widening between them
    /// lives in one function in `pos-services`; see [`ZReportRow`].
    pub fn save_z_report(&self, report: &ZReportRow) -> SqliteResult<()> {
        self.insert(&Z_REPORT_ROW, report)?;
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
    ) -> SqliteResult<Vec<ZReportRow>> {
        self.select_all(
            Z_REPORT_ROW.reader(),
            "FROM z_reports WHERE report_date >= ?1 AND report_date <= ?2 ORDER BY report_date DESC",
            params![start_date, end_date],
        )
    }

    /// Gets the next Z-Report sequence number for a date
    pub fn get_next_z_report_sequence(
        &self,
        terminal_id: &str,
        date_part: &str,
    ) -> SqliteResult<i64> {
        let prefix = format!("Z-{}-{}-", terminal_id, date_part);

        // Was `.unwrap_or(0)`. `COUNT(*)` always returns exactly one row, so the default could
        // never mean "no rows matched" — it could only absorb a real failure and hand back
        // sequence 1, which under this table's `TEXT PRIMARY KEY` collides with the day's first
        // report. Task 10 made that collision loud; propagating makes it not happen.
        let count: i64 = self.select_scalar(
            "SELECT COUNT(*) FROM z_reports WHERE report_number LIKE ?1",
            [format!("{}%", prefix)],
        )?;

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
        let count = db
            .count_z_reports_for_date("POS-001", "2024-01-15")
            .unwrap();
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
    fn test_get_next_z_report_sequence() {
        let db = setup_db();
        let seq = db
            .get_next_z_report_sequence("POS-001", "20240115")
            .unwrap();
        assert_eq!(seq, 1);
    }

    // ------------------------------------------------------------------------------------------
    // Day totals. The test these replace asserted zeros against an empty database — which passes
    // whether or not the query works, and is how a swallowed failure survived here.
    // ------------------------------------------------------------------------------------------

    /// One group of rows in the fixture day: how many, and what each one carries.
    struct SomeTransactions {
        how_many: usize,
        kind: &'static str,
        status: &'static str,
        method: &'static str,
        total: f64,
        discount: f64,
        tax: f64,
    }

    /// Writes a day whose every aggregate comes out different from every other.
    ///
    /// **All seven counts differ and all eleven amounts differ**, which took a rebuild: the first
    /// version gave one return and one void, so `return_count == void_count == 1` — and a mutation
    /// swapping those two fields in `DayTotals::from_reads` passed. The fixture violated the exact
    /// property it is named for, and only the mutation said so.
    ///
    /// Group sizes are 1/2/3/4/5/6 so that no two counts can collide, and the amounts are chosen
    /// so no sum equals another. Eleven of these numbers are sums over one table under different
    /// predicates, which is the case where a swapped column is least visible and most expensive.
    fn a_day_with_no_two_totals_alike(db: &Database) {
        let groups = [
            // 1 cash sale, 2 card, 3 wallet, 4 credit — so each payment count is its own number.
            SomeTransactions {
                how_many: 1,
                kind: "SALE",
                status: "COMPLETED",
                method: "CASH",
                total: 100.0,
                discount: 1.0,
                tax: 7.0,
            },
            SomeTransactions {
                how_many: 2,
                kind: "SALE",
                status: "COMPLETED",
                method: "CARD",
                total: 150.0,
                discount: 2.0,
                tax: 5.0,
            },
            SomeTransactions {
                how_many: 3,
                kind: "SALE",
                status: "COMPLETED",
                method: "WALLET",
                total: 200.0,
                discount: 3.0,
                tax: 6.0,
            },
            SomeTransactions {
                how_many: 4,
                kind: "SALE",
                status: "COMPLETED",
                method: "CREDIT",
                total: 250.0,
                discount: 4.0,
                tax: 8.0,
            },
            // 5 returns, split so `returns` and `cash_refunds` cannot be the same number.
            SomeTransactions {
                how_many: 2,
                kind: "RETURN",
                status: "COMPLETED",
                method: "CASH",
                total: -25.0,
                discount: 0.0,
                tax: 0.0,
            },
            SomeTransactions {
                how_many: 3,
                kind: "RETURN",
                status: "COMPLETED",
                method: "CARD",
                total: -40.0,
                discount: 0.0,
                tax: 0.0,
            },
            // 6 voided sales.
            SomeTransactions {
                how_many: 6,
                kind: "SALE",
                status: "VOIDED",
                method: "CASH",
                total: 500.0,
                discount: 0.0,
                tax: 0.0,
            },
        ];

        let conn = db.connection();
        let conn = conn.lock();
        let mut next_id = 0;
        for group in groups {
            for _ in 0..group.how_many {
                next_id += 1;
                conn.execute(
                    // `transaction_type` is the V1 column and `NOT NULL`; `type` is a separate one
                    // added by migration v2, and it is the one every aggregate here reads. Both
                    // are written so the row is legal and the query sees what it looks for.
                    "INSERT INTO offline_transactions \
                     (offline_id, transaction_type, type, status, payment_method, \
                      total, discount, tax, \
                      items_json, payments_json, subtotal, tax_total, grand_total, \
                      terminal_id, created_at) \
                     VALUES (?1, ?2, ?2, ?3, ?4, ?5, ?6, ?7, '[]', '[]', 0, 0, ?5, \
                             'POS-001', '2026-08-24T10:00:00Z')",
                    params![
                        format!("t-{next_id}"),
                        group.kind,
                        group.status,
                        group.method,
                        group.total,
                        group.discount,
                        group.tax
                    ],
                )
                .expect("a transaction");
            }
        }
    }

    #[test]
    fn every_day_total_is_the_aggregate_it_is_named_for() {
        let db = setup_db();
        a_day_with_no_two_totals_alike(&db);

        let totals = db.get_day_totals("POS-001", "2026-08-24").unwrap();

        // 1 + 2 + 3 + 4 + 2 + 3 + 6 rows land in the day, voided and returned included.
        assert_eq!(totals.transaction_count, 21);
        // 100 + 300 + 600 + 1000 sold, plus 6 x 500 voided. **The voided rows are counted**: they
        // are `type = 'SALE'`, and the gross-sales aggregate keys on `type` while only
        // `void_total` keys on `status`. So a voided sale is reported in gross *and* in voids, and
        // `net_sales` does not subtract it. That is the shipped behaviour, pinned here rather than
        // corrected: whether a till's gross includes voids is an accounting convention, not a bug
        // on its face, and nobody in this issue is the right person to decide it. It was invisible
        // before this test — the only day-totals test asserted zeros against an empty table.
        assert_eq!(totals.gross_sales, Decimal::from(5000));
        assert_eq!(totals.discounts, Decimal::from(30));
        assert_eq!(totals.returns, Decimal::from(170));
        assert_eq!(totals.net_sales, Decimal::from(5000 - 30 - 170));
        assert_eq!(totals.tax_collected, Decimal::from(67));
        assert_eq!(totals.return_count, 5);
        assert_eq!(totals.void_count, 6);
        assert_eq!(totals.void_total, Decimal::from(3000));

        // The payment breakdown excludes the voided rows, which is the whole reason it is a
        // second query with a different `WHERE` clause rather than more columns on the first.
        assert_eq!(totals.cash_total, Decimal::from(100));
        assert_eq!(totals.card_total, Decimal::from(300));
        assert_eq!(totals.wallet_total, Decimal::from(600));
        assert_eq!(totals.credit_total, Decimal::from(1000));
        assert_eq!(totals.cash_count, 1);
        assert_eq!(totals.card_count, 2);
        assert_eq!(totals.wallet_count, 3);
        assert_eq!(totals.credit_count, 4);
        assert_eq!(totals.cash_refunds, Decimal::from(50));
    }

    /// No two numbers in the fixture day are the same number.
    ///
    /// The control for every swap assertion above: if two of them collide, a mutation exchanging
    /// those two fields passes and the test that looks like it covers them does not. That is not
    /// hypothetical — it happened here, `return_count` against `void_count`, and the fixture's own
    /// name claimed otherwise.
    #[test]
    fn no_two_totals_in_the_fixture_day_are_equal() {
        let db = setup_db();
        a_day_with_no_two_totals_alike(&db);
        let totals = db.get_day_totals("POS-001", "2026-08-24").unwrap();

        let counts = [
            ("transaction_count", totals.transaction_count),
            ("return_count", totals.return_count),
            ("void_count", totals.void_count),
            ("cash_count", totals.cash_count),
            ("card_count", totals.card_count),
            ("wallet_count", totals.wallet_count),
            ("credit_count", totals.credit_count),
        ];
        for (i, (left_name, left)) in counts.iter().enumerate() {
            for (right_name, right) in counts.iter().skip(i + 1) {
                assert_ne!(
                    left, right,
                    "`{left_name}` and `{right_name}` are both {left}"
                );
            }
        }

        let amounts = [
            ("gross_sales", totals.gross_sales),
            ("discounts", totals.discounts),
            ("returns", totals.returns),
            ("net_sales", totals.net_sales),
            ("tax_collected", totals.tax_collected),
            ("cash_total", totals.cash_total),
            ("card_total", totals.card_total),
            ("wallet_total", totals.wallet_total),
            ("credit_total", totals.credit_total),
            ("cash_refunds", totals.cash_refunds),
            ("void_total", totals.void_total),
        ];
        for (i, (left_name, left)) in amounts.iter().enumerate() {
            for (right_name, right) in amounts.iter().skip(i + 1) {
                assert_ne!(
                    left, right,
                    "`{left_name}` and `{right_name}` are both {left}"
                );
            }
        }
    }

    /// A quiet day still reads zero — the control for the test above.
    ///
    /// On its own this asserts nothing: it is what a query that never runs also produces. It
    /// earns its place only beside the populated case, which is why the two are adjacent and why
    /// the version of this test that stood alone is deleted rather than kept.
    #[test]
    fn a_day_with_no_transactions_reads_zero_rather_than_failing() {
        let db = setup_db();
        let totals = db.get_day_totals("POS-001", "2026-08-24").unwrap();
        assert_eq!(totals.transaction_count, 0);
        assert_eq!(totals.gross_sales, Decimal::ZERO);
        assert_eq!(totals.cash_total, Decimal::ZERO);
    }

    /// A day's totals must not include another terminal's takings.
    #[test]
    fn day_totals_are_scoped_to_the_terminal_and_the_date() {
        let db = setup_db();
        a_day_with_no_two_totals_alike(&db);

        assert_eq!(
            db.get_day_totals("POS-002", "2026-08-24")
                .unwrap()
                .transaction_count,
            0,
            "another terminal's transactions reached this terminal's day"
        );
        assert_eq!(
            db.get_day_totals("POS-001", "2026-08-25")
                .unwrap()
                .transaction_count,
            0,
            "another day's transactions reached this day"
        );
    }

    // ------------------------------------------------------------------------------------------
    // `Z_REPORT_ROW`.
    // ------------------------------------------------------------------------------------------

    /// A Z-report whose every column holds a value found nowhere else in the row.
    ///
    /// Nineteen of the twenty-three columns are money or counts, most of them adjacent and all of
    /// them `REAL`. Distinct powers of two make a swap between any two of them arithmetically
    /// visible.
    fn a_report_with_no_two_columns_alike() -> ZReportRow {
        ZReportRow {
            report_number: "report-number-column".to_string(),
            report_date: "2026-08-24".to_string(),
            terminal_id: "terminal-id-column".to_string(),
            currency: "LYD".to_string(),
            total_shifts: 3,
            total_transactions: 5,
            gross_sales: Decimal::from(1),
            discounts: Decimal::from(2),
            returns: Decimal::from(4),
            net_sales: Decimal::from(8),
            tax_collected: Decimal::from(16),
            cash_total: Decimal::from(32),
            card_total: Decimal::from(64),
            wallet_total: Decimal::from(128),
            credit_total: Decimal::from(256),
            opening_float: Decimal::from(512),
            expected_cash: Decimal::from(1024),
            actual_cash: Decimal::from(2048),
            variance: Decimal::from(4096),
            variance_status: VarianceStatus::Over,
            generated_at: "2026-08-24T23:00:00Z".to_string(),
            synced: true,
            server_id: Some("server-id-column".to_string()),
        }
    }

    #[test]
    fn the_z_report_mapping_names_every_column_it_writes_in_the_order_it_reads_them() {
        assert_eq!(
            Z_REPORT_ROW.reader().select_list(),
            "report_number, report_date, terminal_id, currency, total_shifts, \
             total_transactions, gross_sales, discounts, returns, net_sales, tax_collected, \
             cash_total, card_total, wallet_total, credit_total, opening_float, expected_cash, \
             actual_cash, variance, variance_status, generated_at, synced, server_id"
        );
        assert_eq!(Z_REPORT_ROW.reader().width(), 23);
        assert_eq!(Z_REPORT_ROW.insert_column_names().count(), 23);
    }

    #[test]
    fn every_column_of_a_fully_distinct_z_report_survives_the_round_trip() {
        let db = setup_db();
        let written = a_report_with_no_two_columns_alike();
        db.save_z_report(&written).unwrap();

        let read = db
            .get_z_reports_in_range("2026-08-01", "2026-08-31")
            .unwrap();
        assert_eq!(read.len(), 1);
        assert_eq!(read[0], written, "a column did not survive the round trip");
    }

    #[test]
    fn save_z_report_puts_each_value_in_the_column_that_carries_its_name() {
        let db = setup_db();
        db.save_z_report(&a_report_with_no_two_columns_alike())
            .unwrap();

        let conn = db.connection();
        let conn = conn.lock();
        for (column, expected) in [
            ("report_number", "report-number-column"),
            ("report_date", "2026-08-24"),
            ("terminal_id", "terminal-id-column"),
            ("currency", "LYD"),
            ("variance_status", "over"),
            ("generated_at", "2026-08-24T23:00:00Z"),
            ("server_id", "server-id-column"),
        ] {
            let matched: bool = conn
                .query_row(
                    &format!("SELECT {column} = ?1 FROM z_reports"),
                    [expected],
                    |row| row.get(0),
                )
                .unwrap();
            assert!(matched, "the `{column}` column does not hold `{expected}`");
        }

        // The numeric columns, compared as numbers. A `REAL` against a bound `&str` is never
        // equal in SQLite, so folding these into the loop above would read `false` for all of
        // them and look like a real failure rather than a wrong test.
        for (column, expected) in [
            ("total_shifts", 3.0_f64),
            ("total_transactions", 5.0),
            ("gross_sales", 1.0),
            ("discounts", 2.0),
            ("returns", 4.0),
            ("net_sales", 8.0),
            ("tax_collected", 16.0),
            ("cash_total", 32.0),
            ("card_total", 64.0),
            ("wallet_total", 128.0),
            ("credit_total", 256.0),
            ("opening_float", 512.0),
            ("expected_cash", 1024.0),
            ("actual_cash", 2048.0),
            ("variance", 4096.0),
            ("synced", 1.0),
        ] {
            let matched: bool = conn
                .query_row(
                    &format!("SELECT {column} = ?1 FROM z_reports"),
                    [expected],
                    |row| row.get(0),
                )
                .unwrap();
            assert!(matched, "the `{column}` column does not hold `{expected}`");
        }
    }

    /// A second Z-report under the same number is **refused**, not silently overwritten.
    ///
    /// `report_number` is `TEXT PRIMARY KEY` and the mapping declares `OnConflict::Fail`. Under
    /// `Replace` — the disposition every other mapping in this crate uses, so the one a tidy-up
    /// would reach for — re-running day close would overwrite a finalised fiscal record with a
    /// second reading of the same day, and the first would be gone with nothing to say so.
    #[test]
    fn a_second_save_of_the_same_report_number_is_refused() {
        let db = setup_db();
        let first = a_report_with_no_two_columns_alike();
        db.save_z_report(&first).unwrap();

        let refused = db.save_z_report(&ZReportRow {
            gross_sales: Decimal::from(9999),
            ..first.clone()
        });
        assert!(
            refused.is_err(),
            "the second save overwrote a finalised Z-report"
        );

        let stored = db
            .get_z_reports_in_range("2026-08-01", "2026-08-31")
            .unwrap();
        assert_eq!(stored.len(), 1);
        assert_eq!(
            stored[0].gross_sales,
            Decimal::from(1),
            "the refused save changed the stored report anyway"
        );

        // The control: a different report number inserts cleanly, so the refusal above is the
        // primary key and not a writer that cannot write twice at all.
        db.save_z_report(&ZReportRow {
            report_number: "a-different-report".to_string(),
            ..first
        })
        .expect("a second report under its own number");
    }

    /// A `variance_status` the domain does not admit is an error, not a balanced till.
    ///
    /// The read this replaces was `match raw { "short" => …, "over" => …, _ => Balanced }`. On
    /// the record a cashier is held to at end of day, that fallback reports the drawer as
    /// reconciled for a value the store could not interpret.
    #[test]
    fn an_unreadable_variance_status_is_an_error_not_a_balanced_till() {
        let db = setup_db();
        db.save_z_report(&a_report_with_no_two_columns_alike())
            .unwrap();

        {
            let conn = db.connection();
            let conn = conn.lock();
            conn.execute("UPDATE z_reports SET variance_status = 'sideways'", [])
                .unwrap();
        }

        let read = db.get_z_reports_in_range("2026-08-01", "2026-08-31");
        assert!(
            read.is_err(),
            "an unrecognised variance status was read as a value"
        );

        // The control: the three the domain does admit all read back as themselves, so the
        // refusal above discriminates rather than the column being unreadable in general.
        for (stored, expected) in [
            ("balanced", VarianceStatus::Balanced),
            ("short", VarianceStatus::Short),
            ("over", VarianceStatus::Over),
        ] {
            {
                let conn = db.connection();
                let conn = conn.lock();
                conn.execute("UPDATE z_reports SET variance_status = ?1", [stored])
                    .unwrap();
            }
            assert_eq!(
                db.get_z_reports_in_range("2026-08-01", "2026-08-31")
                    .unwrap()[0]
                    .variance_status,
                expected
            );
        }
    }

    /// One nullable Z-report column, blanked. `server_id` is the only one.
    #[test]
    fn a_null_server_id_reaches_that_field_and_no_other() {
        let db = setup_db();
        db.save_z_report(&ZReportRow {
            server_id: None,
            ..a_report_with_no_two_columns_alike()
        })
        .unwrap();

        let read = db
            .get_z_reports_in_range("2026-08-01", "2026-08-31")
            .unwrap();
        assert_eq!(read[0].server_id, None);
        assert_eq!(read[0].generated_at, "2026-08-24T23:00:00Z");
        assert!(read[0].synced);
    }
}
