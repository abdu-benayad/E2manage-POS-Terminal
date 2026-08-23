//! `AuthService::verify_pin` against a real socket — the four outcomes that matter most.
//!
//! Acceptance rows 1, 2 and 15 of `auth-outcome-and-offline-lockout`. They are here rather than in
//! a unit test because the defect they guard lives **between** the layers: a refusal the platform
//! actually made, travelling through the transport, arriving as an outcome. Every stage of that
//! path was individually plausible and the whole was wrong —
//!
//! - the server refuses with 401 `POS_OPERATOR_LOCKED`,
//! - the client flattened it into `anyhow!("API Error (401): …")`,
//! - `verify_pin` read *any* error as "the platform is unavailable" and fell through to local
//!   verification, against a table with no attempt counter and no lock column.
//!
//! So a locked operator with the correct PIN was admitted, with the network up, by a till that had
//! just been told not to admit them. Mocking at the type level cannot reproduce that; only a real
//! response through the real client can.

use std::sync::Arc;

use pos_api::ApiClient;
use pos_db::init_memory_database;
use pos_models::{
    Authority, LockoutPeriod, MaxAttempts, OfflineWindow, OperatorId, OperatorRole, Permission,
    Pin, PinLength, PinPolicy, PinRefusal, PinVerification, RequiredPinLength, SessionLifetime,
    UndeterminedCause,
};
use pos_services::AuthService;
use wiremock::matchers::method;
use wiremock::{Mock, MockServer, ResponseTemplate};

fn operator() -> OperatorId {
    OperatorId::new("op-001").expect("a fixture id is never blank")
}

fn pin() -> Pin {
    Pin::parse("1234").expect("four ASCII digits are a platform-legal PIN")
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

/// A service whose store holds this operator as a **known, active** row.
///
/// Load-bearing in every refusal test below: it is what makes a fall-through observable. The
/// offline leg answers `Undetermined(ServerUnreachable)` for exactly this row — a real operator
/// whose PIN this till cannot check — so a refusal that leaked into it would arrive as
/// `Undetermined` instead of the refusal the platform actually made, and every assertion below
/// would fail.
///
/// Until schema v13 this seeded a bcrypt hash and a leak showed up as `Accepted`, which was a
/// louder tell. That form is gone with the hash: the till holds no credential to verify a PIN
/// against, by design, until `offline-pin-verification-has-no-credential` lands. The weaker tell
/// still separates the two branches, which is what these tests need.
fn service_with_a_synced_operator(base_url: &str) -> AuthService {
    let db = init_memory_database().expect("an in-memory database");
    {
        let conn = db.connection();
        let conn = conn.lock();
        conn.execute(
            r#"INSERT INTO operators (id, code, name, role, is_active)
               VALUES ('op-001', 'C001', 'Ahmed', 'CASHIER', 1)"#,
            [],
        )
        .expect("the fixture row inserts");
    }
    AuthService::new(Arc::new(ApiClient::new(base_url)), Arc::new(db))
}

async fn server_answering(status: u16, body: serde_json::Value) -> MockServer {
    let server = MockServer::start().await;
    // Matched on the verb alone, deliberately. `pos-api` owns which route this is — the till's
    // architecture says so, and `tests/guards.rs::only_the_transport_crates_name_a_route` fails
    // the build if a route literal appears outside the transport crates. The exact path is pinned
    // in `crates/pos-api/tests/verify_pin.rs`, where the DTO it produces also lives; restating it
    // here would be a second copy of one fact, in the file least likely to be updated with it.
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(status).set_body_json(body))
        .mount(&server)
        .await;
    server
}

fn refusal(code: &str, message: &str, details: serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "success": false,
        "message": message,
        "error": { "code": code, "message": message, "details": details }
    })
}

async fn outcome_against(status: u16, body: serde_json::Value) -> PinVerification {
    let server = server_answering(status, body).await;
    let service = service_with_a_synced_operator(&server.uri());
    service.verify_pin(&operator(), &pin(), &policy()).await
}

/// **Acceptance row 1.** A locked operator entering the correct PIN, with the network up, is
/// refused — and does not reach the local leg that would have admitted them.
#[tokio::test]
async fn a_locked_operator_with_the_correct_pin_is_refused_and_never_falls_through() {
    let outcome = outcome_against(
        401,
        refusal(
            "POS_OPERATOR_LOCKED",
            "Operator locked",
            serde_json::json!({ "lockedUntil": "2026-08-23T14:32:00.000Z" }),
        ),
    )
    .await;

    assert!(
        matches!(outcome, PinVerification::Refused(PinRefusal::Locked)),
        "a lock the platform declared must not be overridden locally: {outcome:?}"
    );
    assert!(!PinRefusal::Locked.consumes_an_attempt());
}

/// **Acceptance row 2.** A correct PIN online is accepted, and the operator is built from the
/// **response body**.
///
/// The deleted `get_operator_info` read the local `operators` table here. This fixture's local row
/// is `Ahmed`/`CASHIER`, and the server says `Sara Haddad`/`MANAGER` — so asserting the server's
/// values is what proves the till stopped re-grading a decision the platform already made against
/// a cache that may not have synced.
#[tokio::test]
async fn a_correct_pin_online_is_accepted_on_the_platforms_authority_and_its_own_body() {
    let outcome = outcome_against(
        200,
        serde_json::json!({
            "success": true,
            "data": {
                "session": { "token": "op-sess-abc", "expiresAt": "2026-08-23T22:00:00.000Z" },
                "operatorId": "op-001",
                "employeeId": "emp-77",
                "employeeNumber": "EMP001",
                "name": "Sara Haddad",
                "nameAr": "سارة حداد",
                "role": "MANAGER",
                "permissions": { "canVoid": true }
            }
        }),
    )
    .await;

    let PinVerification::Accepted {
        operator,
        decided_by,
    } = outcome
    else {
        panic!("a 200 is the affirmative answer: {outcome:?}");
    };
    assert_eq!(decided_by, Authority::Platform);
    assert_eq!(operator.name().latin(), "Sara Haddad");
    assert_eq!(operator.role(), OperatorRole::Manager);
    assert!(operator.permissions().allows(Permission::VoidTransaction));
}

/// **Acceptance row 15, first half.** `WrongPin` carries the **server's** count.
///
/// The till keeps no ledger, so there is nothing here to second-guess the figure with — and
/// nothing to fabricate one from either, which is why `AttemptsRemaining` is a required field.
#[tokio::test]
async fn a_wrong_pin_carries_the_count_the_platform_reported() {
    let outcome = outcome_against(
        401,
        refusal(
            "POS_PIN_INVALID",
            "Invalid PIN",
            serde_json::json!({ "attemptsRemaining": 2 }),
        ),
    )
    .await;

    let PinVerification::Refused(PinRefusal::WrongPin { attempts_remaining }) = outcome else {
        panic!("a wrong PIN is a refusal that carries its count: {outcome:?}");
    };
    assert_eq!(attempts_remaining.get(), 2);
    assert!(
        PinRefusal::WrongPin { attempts_remaining }.consumes_an_attempt(),
        "this is the one refusal that spends the budget"
    );
}

/// **Acceptance row 15, second half.** The boundary attempt answers `Locked`, not "zero remaining".
///
/// `AttemptsRemaining` wraps a `NonZeroU8`, so "wrong PIN, none left" is unconstructible — and the
/// platform repartitioned its own boundary to match: the attempt that trips the lock answers
/// `POS_OPERATOR_LOCKED`. The case below is the server contradicting that partition, which reads
/// as the lock it means rather than as a counter that cannot exist.
#[tokio::test]
async fn the_attempt_that_empties_the_budget_answers_locked_and_not_a_zero_counter() {
    let outcome = outcome_against(
        401,
        refusal(
            "POS_PIN_INVALID",
            "Invalid PIN",
            serde_json::json!({ "attemptsRemaining": 0 }),
        ),
    )
    .await;

    assert!(
        matches!(outcome, PinVerification::Refused(PinRefusal::Locked)),
        "zero remaining is a lockout, not a wrong PIN with an impossible count: {outcome:?}"
    );
}

/// A rotation requirement is a **third** fact: the PIN was correct, and it consumes no attempt.
///
/// 403 rather than 401 so a till classifying by status does not tell a cashier they mistyped a PIN
/// they typed correctly.
#[tokio::test]
async fn a_correct_pin_of_the_wrong_length_is_a_rotation_and_costs_no_attempt() {
    let outcome = outcome_against(
        403,
        refusal(
            "POS_PIN_ROTATION_REQUIRED",
            "PIN rotation required",
            serde_json::json!({ "requiredLength": 6 }),
        ),
    )
    .await;

    assert!(
        matches!(
            outcome,
            PinVerification::Refused(PinRefusal::CredentialRequiresRotation {
                expected: PinLength::Six
            })
        ),
        "got {outcome:?}"
    );
    assert!(!PinRefusal::CredentialRequiresRotation {
        expected: PinLength::Six
    }
    .consumes_an_attempt());
}

/// Only *nobody answered* reaches the local leg — proved by an outcome only that leg can produce.
///
/// Nothing is listening on the port, so the request fails at connect: `ApiFailure::Unreachable`,
/// the one case that is ordinary weather.
///
/// The second half is the sharp one. `Refused(OperatorUnknown)` is unreachable from the online
/// path against a server that never answered — there is no response to map it from — so its
/// arrival is positive proof the local leg ran, rather than an inference from what did *not*
/// happen.
#[tokio::test]
async fn an_unreachable_platform_is_the_only_thing_that_reaches_the_local_leg() {
    let base_url = closed_port_uri();

    // A known, active operator: the till holds nothing to check the PIN against, and says so.
    let known = service_with_a_synced_operator(&base_url)
        .verify_pin(&operator(), &pin(), &policy())
        .await;
    assert!(
        matches!(
            known,
            PinVerification::Undetermined(UndeterminedCause::ServerUnreachable)
        ),
        "got {known:?}"
    );

    // An operator this till has never synced. Only the local leg can answer this.
    let unknown = service_with_a_synced_operator(&base_url)
        .verify_pin(
            &OperatorId::new("op-never-synced").expect("a non-blank id"),
            &pin(),
            &policy(),
        )
        .await;
    assert!(
        matches!(
            unknown,
            PinVerification::Refused(PinRefusal::OperatorUnknown)
        ),
        "only the local leg can produce this: {unknown:?}"
    );
    assert!(!PinRefusal::OperatorUnknown.consumes_an_attempt());
}

/// A base URL with nothing behind it.
///
/// Binding to port 0 lets the OS pick a free one, and dropping the listener closes it
/// synchronously. Not a dropped `MockServer` — that shuts down on a background task, so the port
/// can still answer for a moment afterwards, which reads as a contract breach rather than as an
/// unreachable server.
fn closed_port_uri() -> String {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a loopback port");
    let addr = listener
        .local_addr()
        .expect("a bound listener has an address");
    drop(listener);
    format!("http://{addr}")
}

/// **The trade this task makes, asserted rather than described.**
///
/// A till with no network can no longer verify a PIN at all. That is a real regression in
/// capability, and the reason it is the right one is here: the outcome is `Undetermined`, which
/// has no `consumes_an_attempt` to call, so an outage cannot lock anybody out. Before this, the
/// same situation answered `WrongPin` — `bcrypt::verify(pin, "")` failing under `unwrap_or(false)`
/// — and charged it to the operator's budget. A shop with no network could not open, and every
/// cashier who tried was locked out for trying.
#[tokio::test]
async fn a_network_outage_can_no_longer_lock_anybody_out() {
    let service = service_with_a_synced_operator(&closed_port_uri());

    // Ten attempts, which under a three-attempt policy was three lockouts' worth.
    for attempt in 0..10 {
        let outcome = service.verify_pin(&operator(), &pin(), &policy()).await;

        match outcome {
            PinVerification::Refused(refusal) => assert!(
                !refusal.consumes_an_attempt(),
                "attempt {attempt} produced {refusal:?}, which spends the operator's budget"
            ),
            // Neither can spend one: `Undetermined` has no such method, and `Accepted` is not a
            // refusal. Listed rather than wildcarded so a new outcome has to be considered here.
            PinVerification::Undetermined(_) | PinVerification::Accepted { .. } => {}
        }
    }
}

/// A response that arrived and could not be read is a **contract breach**, not weather — and it
/// does not fall through to the local leg either.
///
/// Folding this into "the network was down" is how a disagreement between the till and the
/// platform about an endpoint's shape survives to production, retried forever by a caller that
/// thinks it is weather.
#[tokio::test]
async fn a_body_that_does_not_match_the_contract_is_undetermined_and_not_a_fallback() {
    let outcome = outcome_against(200, serde_json::json!({ "totally": "unexpected" })).await;

    assert!(
        matches!(
            outcome,
            PinVerification::Undetermined(UndeterminedCause::ContractBreach { .. })
        ),
        "got {outcome:?}"
    );
}
