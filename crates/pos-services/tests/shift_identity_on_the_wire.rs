//! A sale names the shift the **platform** issued, or it names none.
//!
//! `POS_Transaction.shiftId` is a nullable `@db.Uuid` with a foreign key to `POS_Shift.id`
//! (`prisma/pos.prisma:462`, `:560`), and `transaction.validator.ts:100` demands a UUID when the
//! field is present. The till has two shift identifiers: `shifts.id`, which it mints itself, and
//! `shifts.server_id`, which `POST /till/shifts/start` returns. Only the second may go on the
//! wire.
//!
//! The till sent the first. `mark_shift_synced` wrote `server_id` and **nothing in the codebase
//! read it back** — the same shape as the operator token that was minted and then `debug!`-logged
//! away. Both identifiers are UUIDs, so no shape check separates them; only a type does.
//!
//! These tests pin the reader, both its answers, and the failure that must not promote the local
//! id onto the wire.

use pos_api::ServerShiftId;
use pos_db::init_memory_database;
use pos_models::OperatorId;
use pos_services::shift_service::server_shift_id;
use rust_decimal::Decimal;
use std::sync::Arc;

fn operator() -> OperatorId {
    OperatorId::new("op-1").expect("a fixture id is never blank")
}

/// An open shift on a till that has not reached the platform.
///
/// `shifts.operator_id` carries a foreign key, so the operator has to exist first — which is the
/// point of the column and not an inconvenience of the fixture.
fn a_local_shift(db: &pos_db::Database, id: &str) {
    db.execute(
        r#"INSERT OR IGNORE INTO operators (id, code, name, role, is_active)
           VALUES ('op-1', 'OP1', 'Sara Haddad', 'CASHIER', 1)"#,
        &[],
    )
    .expect("the operator this shift belongs to");

    db.start_shift(id, "SH-1", &operator(), Some("TERM-001"), Decimal::ZERO)
        .expect("a shift opens locally whether or not anybody answers");
}

/// A shift the platform has seen answers with the platform's id, not the till's.
#[test]
fn a_synced_shift_reports_the_platform_identifier() {
    let db = Arc::new(init_memory_database().expect("an in-memory database"));
    a_local_shift(&db, "local-shift-1");
    db.mark_shift_synced("local-shift-1", "9f1c0f6e-0000-4000-8000-000000000001")
        .expect("the platform's id persists");

    let found = server_shift_id(&db, "local-shift-1").expect("the platform issued one");

    assert_eq!(found.as_str(), "9f1c0f6e-0000-4000-8000-000000000001");
    assert_ne!(
        found.as_str(),
        "local-shift-1",
        "the local primary key must never be what goes on the wire"
    );
}

/// The control, and the case the offline-first till spends most of its life in.
///
/// A shift opened with the network down has no platform identifier. `None` is the honest answer
/// and the request omits the field — legal, because the column is nullable and the validator marks
/// it `.optional()`. The failure this prevents is falling back to the local id, which passes the
/// transaction validator's UUID check and then breaks the foreign key.
#[test]
fn an_unsynced_shift_reports_no_identifier_rather_than_the_local_one() {
    let db = Arc::new(init_memory_database().expect("an in-memory database"));
    a_local_shift(&db, "local-shift-2");

    assert_eq!(server_shift_id(&db, "local-shift-2"), None);
}

/// A shift the till has no row for is `None` too — a lookup miss must not promote anything.
#[test]
fn a_shift_the_till_does_not_hold_reports_no_identifier() {
    let db = Arc::new(init_memory_database().expect("an in-memory database"));

    assert_eq!(server_shift_id(&db, "no-such-shift"), None);
}

/// A blank stored identifier is an absent one, not an empty one on the wire.
///
/// `ServerShiftId` refuses a blank at construction, so a `server_id` column holding `""` — which
/// the schema permits — cannot become `"shiftId": ""` in a request body. That string would fail
/// the platform's UUID check and read as a client bug rather than as the missing value it is.
#[test]
fn a_blank_stored_identifier_is_read_as_absent() {
    let db = Arc::new(init_memory_database().expect("an in-memory database"));
    a_local_shift(&db, "local-shift-3");
    db.mark_shift_synced("local-shift-3", "   ")
        .expect("the column accepts it; the type is what refuses it");

    assert_eq!(server_shift_id(&db, "local-shift-3"), None);
    assert!(ServerShiftId::new("   ").is_err());
}
