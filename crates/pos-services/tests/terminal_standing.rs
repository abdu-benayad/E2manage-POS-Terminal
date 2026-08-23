//! **Acceptance row 5.** One 401-shaped failure, four different situations, four answers.
//!
//! `SyncService::is_auth_error` recovered the status by substring-matching `"401"` in a rendered
//! message and gave everything that matched one response: re-authenticate, and if that fails,
//! declare the terminal unregistered. The situations it folded together are —
//!
//! | server answer | what it means | the till's answer |
//! | --- | --- | --- |
//! | 403 `POS_TERMINAL_GONE` | the device was withdrawn | stop, permanently |
//! | 403 `POS_TERMINAL_NOT_ACTIVE` | enrolled and not active | stop; an administrator can fix it |
//! | 401 `POS_TERMINAL_SESSION_EXPIRED` | the session lapsed | renew once, ask once more |
//! | 409 `POS_TERMINAL_NOT_PROVISIONED` | no secret to seal with | pair again; do not retry |
//!
//! Two things are asserted about each: the outcome, **and the number of requests it took**. A
//! repudiated terminal that quietly re-tries is the lockout amplifier this issue is named for
//! wearing a different hat — the endpoint behind these routes counts failed attempts against the
//! operator, so a till retrying on their behalf spends a budget they never touched.
//!
//! # How the two routes are told apart without naming either
//!
//! `tests/guards.rs::only_the_transport_crates_name_a_route` fails the build if a route literal
//! appears outside the transport crates, and this file is under `crates/`. It is not an obstacle
//! worth routing around: `pos-api` owns which path is which, and a second copy of that fact here
//! would be the copy nobody updates. The mocks discriminate on the **request body** instead, which
//! is a thing `pos-services` genuinely knows because it supplies it — the PIN request carries an
//! `operatorId`, and the session renewal has no body at all.
//!
//! # What is not here
//!
//! *A renewal that reaches nobody*, which falls to the local leg. `ApiClient` carries a 30-second
//! request timeout, so producing an `Unreachable` mid-sequence would mean a 30-second test, and a
//! closed port fails the **first** request rather than the renewal. The property it would assert —
//! only `Unreachable` reaches local verification — is covered from the top by
//! `verify_pin_outcomes.rs::a_network_outage_can_no_longer_lock_anybody_out`. Said rather than
//! left as a hole nobody notices.

use std::sync::Arc;

use pos_api::ApiClient;
use pos_db::init_memory_database;
use pos_models::{
    Authority, LockoutPeriod, MaxAttempts, OfflineWindow, OperatorId, Pin, PinLength, PinPolicy,
    PinVerification, Repudiation, RequiredPinLength, SessionLifetime, UndeterminedCause,
};
use pos_services::AuthService;
use wiremock::matchers::{body_json, body_partial_json, method};
use wiremock::{Mock, MockBuilder, MockServer, ResponseTemplate};

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

/// A service whose store holds this operator as a known, active row.
///
/// The same fixture as `verify_pin_outcomes.rs`, and load-bearing for the same reason: it is what
/// makes a fall-through to the local leg observable. That leg answers
/// `Undetermined(ServerUnreachable)` for this row, so a repudiation that leaked into it would
/// arrive wearing the wrong cause and every assertion below would fail.
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

fn refusal(code: &str, message: &str) -> serde_json::Value {
    serde_json::json!({
        "success": false,
        "message": message,
        "error": { "code": code, "message": message }
    })
}

/// The PIN request: the one this till sends with an `operatorId` in the body.
fn asking_about_the_pin() -> MockBuilder {
    Mock::given(method("POST")).and(body_partial_json(serde_json::json!({
        "operatorId": "op-001"
    })))
}

/// The session renewal: `ApiClient::refresh_session` posts `&()`, which is a JSON `null`.
///
/// `body_json` and not `body_partial_json` — a partial match against `null` matches every body,
/// including the PIN request's, and the mock ordering would then hide the bug instead of showing
/// it.
fn asking_for_a_new_session() -> MockBuilder {
    Mock::given(method("POST")).and(body_json(serde_json::Value::Null))
}

/// A verified operator, in the shape `verify-pin` answers with.
fn accepted() -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(serde_json::json!({
        "success": true,
        "data": {
            "session": { "token": "op-sess-abc", "expiresAt": "2026-08-23T22:00:00.000Z" },
            "operatorId": "op-001",
            "employeeId": "emp-77",
            "employeeNumber": "EMP001",
            "name": "Sara Haddad",
            "nameAr": "سارة حداد",
            "role": "MANAGER"
        }
    }))
}

/// How many requests the till made in total. The second assertion of every test below.
async fn requests_made(server: &MockServer) -> usize {
    server.received_requests().await.unwrap_or_default().len()
}

// ============================================================================
// The two repudiations
// ============================================================================

/// A withdrawn terminal stops, and does not fall through to the local leg.
///
/// This is the branch where the till would override a decision the server actually made: the
/// platform was reached, it answered, and the answer was that this device is not one of theirs.
/// `EnrolmentState::Repudiated` confers no offline authority for exactly this case.
#[tokio::test]
async fn a_withdrawn_terminal_stops_and_never_reaches_the_local_leg() {
    let server = MockServer::start().await;
    asking_about_the_pin()
        .respond_with(ResponseTemplate::new(403).set_body_json(refusal(
            "POS_TERMINAL_GONE",
            "Terminal has been de-enrolled",
        )))
        .mount(&server)
        .await;

    let service = service_with_a_synced_operator(&server.uri());
    let outcome = service.verify_pin(&operator(), &pin(), &policy()).await;

    let PinVerification::Undetermined(UndeterminedCause::EnrolmentRepudiated(repudiation)) =
        outcome
    else {
        panic!("a disowned terminal cannot decide a PIN, locally or otherwise: {outcome:?}");
    };
    assert_eq!(repudiation, Repudiation::Withdrawn);
    assert!(
        !repudiation.has_a_remedy_at_the_till(),
        "nothing at the drawer restores a withdrawn terminal, and the till must not imply otherwise"
    );
    assert_eq!(
        requests_made(&server).await,
        1,
        "a withdrawn terminal must not try to renew a session it will never be given"
    );
}

/// A suspended terminal stops too — and says something different, because the remedy exists.
///
/// `EnrolmentState` folds both into `Repudiated` because the *decision* is identical. They stay
/// two values because they are two sentences to the person standing at the drawer, and only one of
/// them has somebody to call.
#[tokio::test]
async fn a_suspended_terminal_stops_but_has_a_remedy() {
    let server = MockServer::start().await;
    asking_about_the_pin()
        .respond_with(
            ResponseTemplate::new(403)
                .set_body_json(refusal("POS_TERMINAL_NOT_ACTIVE", "Terminal is not active")),
        )
        .mount(&server)
        .await;

    let service = service_with_a_synced_operator(&server.uri());
    let outcome = service.verify_pin(&operator(), &pin(), &policy()).await;

    let PinVerification::Undetermined(UndeterminedCause::EnrolmentRepudiated(repudiation)) =
        outcome
    else {
        panic!("a suspended terminal cannot decide a PIN: {outcome:?}");
    };
    assert_eq!(repudiation, Repudiation::Suspended);
    assert!(
        repudiation.has_a_remedy_at_the_till(),
        "an administrator can reactivate this one, and telling the operator so is the difference"
    );
    assert_eq!(requests_made(&server).await, 1);
}

// ============================================================================
// Not provisioned
// ============================================================================

/// A half-provisioned terminal is not disowned, and is not worth retrying either.
///
/// The platform holds no `secretHash` for it, so no credential can be sealed. The answer will be
/// the same every time until somebody pairs the device — which is what separates it from
/// `ServerUnreachable`, the cause a retry loop is built on.
#[tokio::test]
async fn a_terminal_with_no_secret_is_told_to_pair_rather_than_retried() {
    let server = MockServer::start().await;
    asking_about_the_pin()
        .respond_with(ResponseTemplate::new(409).set_body_json(refusal(
            "POS_TERMINAL_NOT_PROVISIONED",
            "Terminal is not provisioned",
        )))
        .mount(&server)
        .await;

    let service = service_with_a_synced_operator(&server.uri());
    let outcome = service.verify_pin(&operator(), &pin(), &policy()).await;

    assert!(
        matches!(
            outcome,
            PinVerification::Undetermined(UndeterminedCause::TerminalNotProvisioned)
        ),
        "a terminal the platform cannot seal a credential for must say so: {outcome:?}"
    );
    assert_eq!(requests_made(&server).await, 1);
}

// ============================================================================
// The lapsed session — the one case that is worth a retry
// ============================================================================

/// A lapsed session is renewed **once**, and the PIN is asked **once more**.
///
/// Three requests, in order: the PIN (401), the renewal (200), the PIN again (200). Asserting the
/// count is what makes "once" a property of this test rather than a claim in a comment.
#[tokio::test]
async fn a_lapsed_session_is_renewed_once_and_the_pin_asked_once_more() {
    let server = MockServer::start().await;
    asking_about_the_pin()
        .respond_with(ResponseTemplate::new(401).set_body_json(refusal(
            "POS_TERMINAL_SESSION_EXPIRED",
            "Terminal session expired",
        )))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    asking_for_a_new_session()
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "success": true,
            "data": { "sessionToken": "fresh-terminal-token" }
        })))
        .mount(&server)
        .await;
    asking_about_the_pin()
        .respond_with(accepted())
        .mount(&server)
        .await;

    let service = service_with_a_synced_operator(&server.uri());
    let outcome = service.verify_pin(&operator(), &pin(), &policy()).await;

    let PinVerification::Accepted { decided_by, .. } = outcome else {
        panic!("the retry against a renewed session succeeds: {outcome:?}");
    };
    assert_eq!(decided_by, Authority::Platform);
    assert_eq!(
        requests_made(&server).await,
        3,
        "exactly: the PIN, the renewal, the PIN again"
    );
}

/// A refused renewal is not a second chance, and is not retried.
///
/// `terminal-auth.middleware.ts:76` tests `revokedAt` before `:81` tests `terminal.status`, with a
/// comment recording the order as known-wrong — so a terminal that has been withdrawn *and* whose
/// session lapsed reports as merely expired. The till therefore never reads a refused renewal as
/// weather. When the refusal names no standing, `ReauthFailed` is the honest reading: the session
/// could not be renewed. Guessing which flavour of repudiation it was would be inventing an answer
/// the platform did not give.
#[tokio::test]
async fn a_refused_renewal_is_not_a_second_chance() {
    let server = MockServer::start().await;
    asking_about_the_pin()
        .respond_with(ResponseTemplate::new(401).set_body_json(refusal(
            "POS_TERMINAL_SESSION_EXPIRED",
            "Terminal session expired",
        )))
        .mount(&server)
        .await;
    asking_for_a_new_session()
        .respond_with(
            ResponseTemplate::new(401)
                .set_body_json(refusal("POS_TERMINAL_AUTH_FAILED", "Authentication failed")),
        )
        .mount(&server)
        .await;

    let service = service_with_a_synced_operator(&server.uri());
    let outcome = service.verify_pin(&operator(), &pin(), &policy()).await;

    assert!(
        matches!(
            outcome,
            PinVerification::Undetermined(UndeterminedCause::ReauthFailed)
        ),
        "a renewal the platform refused ends the attempt: {outcome:?}"
    );
    assert_eq!(
        requests_made(&server).await,
        2,
        "the PIN and one renewal. A loop here would spend the operator's attempt budget for them"
    );
}

/// A renewal that names a repudiation is read as one.
///
/// The middleware's ordering hides this most of the time; when it does not, the till must not
/// downgrade a stated repudiation into a generic "could not renew".
#[tokio::test]
async fn a_renewal_refused_with_a_named_standing_keeps_that_standing() {
    let server = MockServer::start().await;
    asking_about_the_pin()
        .respond_with(ResponseTemplate::new(401).set_body_json(refusal(
            "POS_TERMINAL_TOKEN_INVALID",
            "Terminal token invalid",
        )))
        .mount(&server)
        .await;
    asking_for_a_new_session()
        .respond_with(ResponseTemplate::new(403).set_body_json(refusal(
            "POS_TERMINAL_GONE",
            "Terminal has been de-enrolled",
        )))
        .mount(&server)
        .await;

    let service = service_with_a_synced_operator(&server.uri());
    let outcome = service.verify_pin(&operator(), &pin(), &policy()).await;

    let PinVerification::Undetermined(UndeterminedCause::EnrolmentRepudiated(repudiation)) =
        outcome
    else {
        panic!("the renewal named a standing and it must survive: {outcome:?}");
    };
    assert_eq!(repudiation, Repudiation::Withdrawn);
    assert_eq!(requests_made(&server).await, 2);
}
