//! What the till does when the platform already knows this hardware.
//!
//! `PairingService` had **no tests at all** before this file. That is worth stating rather than
//! quietly fixing: the two defects this issue removed — a branch keyed on the words in an error
//! message, and a call asking the platform to hand this till its own secret back — both lived on
//! the one path in this crate that nothing exercised.
//!
//! # The negative is the point
//!
//! A test that drives the happy path re-passes against a till that still reads prose. So the first
//! test here runs the *same* scenario twice with the platform's message reworded, and requires the
//! two runs to agree. Only a till that has stopped reading the message can satisfy both.
//!
//! # Why no mock here names a route
//!
//! `tests/guards.rs::only_the_transport_crates_name_a_route` scans `crates/*/tests/` and fails the
//! build on an `"/api/…"` literal outside the transport crates. That is not an obstacle to route
//! around: `pos-api` owns which path is which, and a second copy of that fact here would be the
//! copy nobody updates. The mocks discriminate on **method** instead — the pairing request is a
//! `POST` carrying `{"hardwareId": …}`, the status poll is a `GET` — which `pos-services`
//! genuinely knows, because it supplies them.
//!
//! # What is deliberately absent
//!
//! No test asserts an operator-facing sentence, because `pos-services` renders none. The
//! distinction this file pins is the one the eventual screen will read.

use std::sync::Arc;

use pos_api::ApiClient;
use pos_db::{init_memory_database, Database};
use pos_models::HardwareEnrolment;
use pos_services::{PairingService, PairingState};
use wiremock::matchers::{body_partial_json, method};
use wiremock::{Mock, MockBuilder, MockServer, ResponseTemplate};

const HARDWARE_ID: &str = "pact-free-hardware-id";

/// A pairing service over an in-memory store, pointed at a mock bound to **port 0**.
///
/// Port 0 and a dropped listener are what make "nobody answered" a fact about this process. A
/// fixture pointing at `localhost:3000` would hit the dev backend where it happens to be up and
/// nothing where it is down — the same test with two different meanings.
fn service(base_url: &str) -> (PairingService, Arc<Database>) {
    let db = Arc::new(init_memory_database().expect("an in-memory database"));
    {
        let conn = db.connection();
        let conn = conn.lock();
        conn.execute(
            "UPDATE terminal_registration SET hardware_id = ?1 WHERE id = 1",
            [HARDWARE_ID],
        )
        .expect("the singleton registration row exists from the schema");
    }
    (
        PairingService::new(Arc::new(ApiClient::new(base_url)), Arc::clone(&db)),
        db,
    )
}

/// Marks this till as enrolled, holding a terminal secret.
///
/// The input a future edit is most likely to reach for when asked "is this a re-enrolment?", and
/// the whole point of tests 5 and 6 is that reaching for it must change nothing.
fn holding_a_secret(db: &Database) {
    let conn = db.connection();
    let conn = conn.lock();
    conn.execute(
        r#"UPDATE terminal_registration
           SET terminal_id = 'term-1', terminal_code = 'TERM-001',
               secret = 'a-real-looking-secret', is_registered = 1
           WHERE id = 1"#,
        [],
    )
    .expect("the fixture row updates");
}

/// The pairing request: the only `POST` this service sends, and it carries the hardware id.
fn asking_for_a_pairing_code() -> MockBuilder {
    Mock::given(method("POST")).and(body_partial_json(serde_json::json!({
        "hardwareId": HARDWARE_ID
    })))
}

fn a_pairing_code() -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(serde_json::json!({
        "success": true,
        "data": {
            "pairingCode": "ABC123",
            "expiresAt": "2026-08-24T12:00:00.000Z",
            "hardwareId": HARDWARE_ID,
        }
    }))
}

/// A pending poll answer, with `isRePair` present or absent exactly as given.
fn a_pending_poll(is_re_pair: Option<bool>) -> ResponseTemplate {
    let mut data = serde_json::json!({
        "status": "PENDING",
        "pairingCode": "ABC123",
        "expiresAt": "2026-08-24T12:00:00.000Z",
    });
    if let Some(flag) = is_re_pair {
        data["isRePair"] = serde_json::Value::Bool(flag);
    }
    ResponseTemplate::new(200).set_body_json(serde_json::json!({
        "success": true,
        "data": data
    }))
}

/// Nothing this till sends may reach the deleted secret-recovery route.
///
/// Asserted **without naming the path**, deliberately, and the next reader should not "fix" this
/// into a path matcher: `tests/guards.rs` bans the route literal tree-wide with no test exemption,
/// and a mock registered against it would fail the build. It needs no matcher anyway — an
/// unmatched request still reaches wiremock and is still recorded, so the recorded set answers the
/// question directly. `recover` is the bare word, not a route: it is neither `"/api/…"` nor the
/// full path, and it is the only route the till ever called that carried it.
async fn nothing_asked_for_a_secret_back(server: &MockServer, expected_requests: usize) {
    let seen = server
        .received_requests()
        .await
        .expect("the mock server records what it was sent");

    assert_eq!(
        seen.len(),
        expected_requests,
        "the till sent {} request(s), not the {expected_requests} this test accounts for; an \
         unaccounted request is how the deleted recovery call would come back",
        seen.len()
    );

    let asked: Vec<String> = seen
        .iter()
        .map(|request| request.url.path().to_string())
        .filter(|path| path.contains("recover"))
        .collect();
    assert!(
        asked.is_empty(),
        "the till asked the platform to hand back its own secret, at {asked:?}. That route \
         returned a terminal's raw secret for a client-chosen hardware id and the platform \
         deleted it; there is no replacement and none is possible"
    );
}

/// **Rewording the platform's message must not change what the till does.**
///
/// The till used to decide by matching `"409"` and `"already registered"` in rendered prose. This
/// runs the same refusal twice with only the wording changed and requires the two runs to agree —
/// on the outcome *and* on the request count. A till still reading the message cannot satisfy
/// both; a happy-path test would not notice either way.
///
/// The request count matters as much as the outcome: the old code answered a matching message by
/// making a *second* request, to the recovery route. Equal counts is what proves it did not.
#[tokio::test]
async fn rewording_the_refusal_does_not_change_what_the_till_does() {
    async fn refused_with(message: &str) -> (bool, usize) {
        let server = MockServer::start().await;
        asking_for_a_pairing_code()
            .respond_with(ResponseTemplate::new(409).set_body_json(serde_json::json!({
                "success": false,
                "message": message,
                "error": { "code": "CONFLICT", "message": message }
            })))
            .mount(&server)
            .await;

        let (pairing, _db) = service(&server.uri());
        let outcome = pairing.request_pairing_code().await;
        nothing_asked_for_a_secret_back(&server, 1).await;
        (
            outcome.is_err(),
            server
                .received_requests()
                .await
                .expect("recorded requests")
                .len(),
        )
    }

    // The exact prose the deleted branch keyed on.
    let matching = refused_with("409: terminal already registered").await;
    // The same refusal, reworded the way a translation or a copy edit would reword it.
    let reworded = refused_with("This device is enrolled already").await;

    assert!(matching.0, "a 409 is an error the caller must see");
    assert_eq!(
        matching, reworded,
        "the till behaved differently for two spellings of the same refusal, so something is \
         still reading the message. Branch on `ServerErrorCode`, never on rendered prose: \
         messages are translated, product names contain digits, and none of it is a contract"
    );
}

/// Already-enrolled hardware reaches the ordinary pending-pairing path, carrying no guess.
///
/// The `Undetermined` assertion is not decoration. An earlier version of this work inferred
/// enrolment from the local store at exactly this call site, and a test asserting only `Pending`
/// and the code would have shipped that green.
#[tokio::test]
async fn a_pairing_request_answers_pending_and_claims_to_know_nothing_about_enrolment() {
    let server = MockServer::start().await;
    asking_for_a_pairing_code()
        .respond_with(a_pairing_code())
        .mount(&server)
        .await;

    let (pairing, _db) = service(&server.uri());
    let state: PairingState = pairing
        .request_pairing_code()
        .await
        .expect("a 200 with a pairing code");

    assert_eq!(state.pairing_code, "ABC123");
    assert_eq!(
        state.enrolment,
        HardwareEnrolment::Undetermined,
        "the pairing-request response carries no enrolment signal — its body is identical for a \
         first enrolment and a re-pair — so anything but `Undetermined` here was invented locally"
    );
    nothing_asked_for_a_secret_back(&server, 1).await;
}

/// `isRePair` on the poll reaches `PairingState.enrolment`, in all three of its states.
///
/// The omitted case is the one that matters and the `true` case is its control: a test that omits
/// a field and gets the default passes *identically* against a field name misspelt everywhere, so
/// on its own it cannot come out differently.
#[tokio::test]
async fn the_polls_re_pair_flag_reaches_the_caller_and_its_absence_is_not_a_denial() {
    for (flag, expected) in [
        (Some(true), HardwareEnrolment::AlreadyEnrolled),
        (Some(false), HardwareEnrolment::NotEnrolled),
        (None, HardwareEnrolment::Undetermined),
    ] {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(a_pending_poll(flag))
            .mount(&server)
            .await;

        let (pairing, _db) = service(&server.uri());
        let state = pairing
            .check_pairing_status("ABC123")
            .await
            .expect("a 200 pending poll");

        assert_eq!(
            state.enrolment, expected,
            "isRePair {flag:?} should reach the caller as {expected:?}"
        );
        nothing_asked_for_a_secret_back(&server, 1).await;
    }
}

/// **A stored secret changes nothing.** The regression test for the defect this plan removed.
///
/// A till holding a secret looks enrolled to itself and may not be: the platform archives a
/// terminal whose company was deleted and issues a fresh enrolment without telling anyone here.
/// So the local store must not reach the answer, on either path — and the platform's `false` must
/// survive a local secret that disagrees with it.
#[tokio::test]
async fn a_stored_secret_never_becomes_an_enrolment_answer() {
    let server = MockServer::start().await;
    asking_for_a_pairing_code()
        .respond_with(a_pairing_code())
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .respond_with(a_pending_poll(Some(false)))
        .mount(&server)
        .await;

    let (pairing, db) = service(&server.uri());
    holding_a_secret(&db);

    let requested = pairing
        .request_pairing_code()
        .await
        .expect("a 200 with a pairing code");
    assert_eq!(
        requested.enrolment,
        HardwareEnrolment::Undetermined,
        "a stored secret was read as an enrolment answer. It proves this device was enrolled here \
         once, not that a working terminal would be replaced — and a reinstalled till, the case \
         this issue exists for, holds no secret while being enrolled"
    );

    let polled = pairing
        .check_pairing_status("ABC123")
        .await
        .expect("a 200 pending poll");
    assert_eq!(
        polled.enrolment,
        HardwareEnrolment::NotEnrolled,
        "a stored secret overruled the platform saying this hardware is not enrolled. The \
         platform's answer is about its own records and is always the more recent of the two"
    );

    nothing_asked_for_a_secret_back(&server, 2).await;
}

// ============================================================================
// A READ THAT FAILS IS NOT A ROW THAT IS ABSENT
// ============================================================================
//
// `is_registered` and `get_hardware_id` each read one column of row 1 and flatten the result into
// a default — `.unwrap_or(0)` and `.unwrap_or_default()`. Neither can distinguish
// `QueryReturnedNoRows`, the legitimate fresh-install case those defaults exist to serve, from any
// other error. The tests below drive the *other* error and record what the till then does.
//
// # What these establish, and what they do not
//
// They establish that the consequence is real **given the triggering state**: the till answers
// "not registered" while holding a valid secret, and then deletes that secret. They do **not**
// establish that a production write path produces the triggering state — see the negative recorded
// below `a_blob_in_a_text_column`. That distinction is the whole severity question, so it is
// written here rather than left for a reader to assume in either direction.
//
// # Why a BLOB
//
// SQLite's TEXT affinity converts numbers to text but leaves a BLOB a BLOB, so `hardware_id TEXT
// NOT NULL` accepts one and `row.get::<_, String>` then fails with `InvalidColumnType`. It is a
// *mechanism* for producing a non-`QueryReturnedNoRows` error on this exact call, chosen because
// it is precise and local — not a claim that a BLOB is how this happens in the field.

/// Makes the next read of `column` fail with something that is not `QueryReturnedNoRows`.
fn a_blob_in_a_text_column(db: &Database, column: &str) {
    let conn = db.connection();
    let conn = conn.lock();
    conn.execute(
        &format!("UPDATE terminal_registration SET {column} = X'00' WHERE id = 1"),
        [],
    )
    .expect("the singleton registration row updates");
}

fn stored_hardware_id(db: &Database) -> Option<String> {
    let conn = db.connection();
    let conn = conn.lock();
    conn.query_row(
        "SELECT hardware_id FROM terminal_registration WHERE id = 1",
        [],
        |row| row.get::<_, Option<String>>(0),
    )
    .expect("the singleton registration row is readable")
}

fn stored_secret(db: &Database) -> Option<String> {
    let conn = db.connection();
    let conn = conn.lock();
    conn.query_row(
        "SELECT secret FROM terminal_registration WHERE id = 1",
        [],
        |row| row.get::<_, Option<String>>(0),
    )
    .expect("the singleton registration row is readable")
}

/// A read that fails surfaces as an error rather than as "this till is not registered".
///
/// The control is the assertion *before* the corruption: without it, a test that only checks the
/// error cannot tell a surfaced failure from a fixture that was never registered in the first place.
#[test]
fn an_unreadable_registration_flag_is_an_error_not_a_negative_answer() {
    let (service, db) = service("http://127.0.0.1:1");
    holding_a_secret(&db);

    assert!(
        service.is_registered().expect("the flag is readable"),
        "control: the fixture registered this till, so the pre-corruption answer must be true"
    );

    a_blob_in_a_text_column(&db, "is_registered");

    let answer = service.is_registered();

    assert!(
        answer.is_err(),
        "a store that cannot answer must not be reported as a till that is unregistered"
    );
    assert!(
        stored_secret(&db).is_some(),
        "and the row is untouched — the failure is in the reading, not in the data"
    );
}

/// An unreadable hardware id refuses rather than inventing a replacement.
///
/// Both halves of the original defect are pinned here. `save_hardware_id` used `INSERT OR REPLACE`,
/// a delete-and-insert that reset every column it did not name — the terminal secret and the
/// platform licence key among them. And `get_hardware_id` reached it at all because it could not
/// tell `QueryReturnedNoRows` from a read that failed, so it concluded "no hardware id yet" and
/// generated one. Either fix alone leaves a till that silently presents an identity the platform
/// has never seen; the assertions below require both.
#[test]
fn an_unreadable_hardware_id_refuses_rather_than_replacing_the_till_identity() {
    let (service, db) = service("http://127.0.0.1:1");
    holding_a_secret(&db);

    assert_eq!(
        service
            .get_hardware_id()
            .expect("the id is readable")
            .as_str(),
        HARDWARE_ID,
        "control: the stored id is returned as-is before the corruption"
    );

    a_blob_in_a_text_column(&db, "hardware_id");

    assert!(
        service.get_hardware_id().is_err(),
        "the failed read reaches the caller instead of becoming a fresh identity"
    );
    assert_eq!(
        stored_secret(&db).as_deref(),
        Some("a-real-looking-secret"),
        "the terminal secret survives"
    );
    assert!(
        service
            .is_registered()
            .expect("the flag column is untouched, so it still reads"),
        "and the enrolment is not silently revoked"
    );
}

/// The absent case still works, because that is what the swallowed default was there to serve.
///
/// A fix that stops distinguishing failures by refusing everything would pass the two tests above
/// and brick first boot. The schema seeds `hardware_id` as the empty string, so this is the real
/// fresh-install path rather than a constructed one.
#[test]
fn a_till_with_no_hardware_id_yet_still_generates_one() {
    let (service, db) = service("http://127.0.0.1:1");
    {
        let conn = db.connection();
        let conn = conn.lock();
        conn.execute(
            "UPDATE terminal_registration SET hardware_id = '' WHERE id = 1",
            [],
        )
        .expect("the singleton registration row updates");
    }

    let generated = service
        .get_hardware_id()
        .expect("an empty stored id is the fresh-install case, not a failure");

    assert!(!generated.is_empty(), "a hardware id was generated");
    assert_eq!(
        stored_hardware_id(&db).as_deref(),
        Some(generated.as_str()),
        "and persisted, so the next call returns the same one"
    );
    assert_eq!(
        service.get_hardware_id().expect("the stored id reads back"),
        generated,
        "the id is stable across calls — it is an identity, not a nonce"
    );
}
