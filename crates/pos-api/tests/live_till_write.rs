//! The only test in this repository that can see whether the till's writes actually work.
//!
//! # Why it had to be built rather than reused
//!
//! `scripts/run-e2e-tests.sh` looks like the answer and is a false green. Measured:
//!
//! - `run-e2e-tests.sh:101` sets `TEST_CMD="cargo test --test e2e_api_tests"` unconditionally; the
//!   category arguments (`transaction`, `shift`, `return`) are name *filters*, not binaries.
//! - `tests/e2e_api_tests.rs` is one line, `mod api_tests;`.
//! - `tests/api_tests.rs` contains **zero** references to `pos_api`. It builds its own
//!   `reqwest::Client` with a cookie store, fetches `/api/csrf-token`, and authenticates with a
//!   back-office `Authorization: Bearer` plus `x-csrf-token` — never `X-Terminal-Token`, never
//!   `X-Operator-Token` — and posts to the *back-office* literals this issue moved away from.
//!
//! So that script exits 0 whether or not any of this work is correct, and would not go red if the
//! repoint were broken. Every other `pos-api` and `pos-services` test is wiremock- or pact-backed:
//! they pin what the till *sends*, and a mock agrees with whatever the till believes. **Nothing
//! here could disagree with the platform**, which is the whole failure this issue exists inside.
//!
//! This harness drives `pos_api::ApiClient` — the real one, with the real header logic and the
//! real envelope reading — against a real server.
//!
//! # It has never been run, and that is recorded rather than glossed
//!
//! As of 2026-08-24 no current platform instance is reachable from this machine.
//! `localhost:3000` answers, and answers about 2026-08-16: it is 286 commits behind, predates
//! `csrf-exemption.ts` and predates the `/api/pos/till/*` mounts this issue targets. A 401 under
//! `/api` is also a constant there — `POST /api/pos/xyzzy` answers exactly as a real route
//! refusing a real credential does — so it cannot even tell "route exists and refused" from "route
//! was never mounted". Running this against it would produce a reading that could not come out
//! differently, which is worse than not running it.
//!
//! So these are `#[ignore]`d, and the issue records task 07 as unsatisfied. **Do not report a
//! green from this file without saying which server answered.**
//!
//! # Running it
//!
//! ```bash
//! E2M_API_URL=https://… \
//! E2M_TERMINAL_CODE=TERM-001 E2M_HARDWARE_ID=… E2M_TERMINAL_SECRET=… \
//! E2M_OPERATOR_ID=… E2M_OPERATOR_PIN=… \
//! E2M_SUPERVISOR_ID=… E2M_SUPERVISOR_PIN=… \
//! E2M_PRODUCT_ID=… E2M_PRODUCT_SKU=… E2M_CURRENCY=LYD \
//! cargo test -p pos-api --test live_till_write -- --ignored --nocapture --test-threads=1
//! ```
//!
//! `--test-threads=1` is not decoration: these share one terminal enrolment and one shift.
//!
//! # Two pieces of tenant setup nothing prompts for
//!
//! Both refusals arrive in **Arabic**, the fallback locale, which is what makes them slow to
//! recognise in a log:
//!
//! - lines are verified online against the catalogue, so an unknown `E2M_PRODUCT_ID` refuses the
//!   **whole sale** with `reason: 'unknown-product'`;
//! - `currencyForCompany` refuses any code absent from `company_currency_settings`, which the POS
//!   module's own `baseCurrency` setting does **not** seed.

use std::env;

use pos_api::{
    ApiClient, ApiFailure, CapabilityStanding, CreateReturnRequest, CreateTransactionRequest,
    EndShiftRequest, PaymentDto, ReturnItemRequest, ServerErrorCode, ServerShiftId, SessionToken,
    StartShiftRequest, TransactionItemDto, VoidTransactionRequest,
};
use pos_models::{OperatorId, Pin};
use rust_decimal::Decimal;

// ============================================================================
// Configuration
// ============================================================================

/// Everything this harness needs from the environment.
///
/// Read as a whole and reported as a whole. A harness that dies on the first missing variable
/// makes its operator rediscover the list one run at a time.
struct LiveConfig {
    base_url: String,
    terminal_code: String,
    hardware_id: String,
    terminal_secret: String,
    operator_id: OperatorId,
    operator_pin: Pin,
    supervisor_id: OperatorId,
    supervisor_pin: Pin,
    product_id: String,
    product_sku: String,
    currency: String,
}

impl LiveConfig {
    /// Reads the environment, or explains everything that is missing at once.
    ///
    /// **`E2M_API_URL` has no default here, deliberately.** The repo convention is that a fixture
    /// must never point at `localhost:3000` — that is the dev backend, so a defaulted harness hits
    /// a real database wherever one happens to be up and nothing where it is down, and the same
    /// run means two different things on two machines.
    fn from_env() -> Result<Self, String> {
        let mut missing = Vec::new();
        let mut read = |key: &str| -> String {
            match env::var(key) {
                Ok(value) if !value.trim().is_empty() => value,
                _ => {
                    missing.push(key.to_string());
                    String::new()
                }
            }
        };

        let base_url = read("E2M_API_URL");
        let terminal_code = read("E2M_TERMINAL_CODE");
        let hardware_id = read("E2M_HARDWARE_ID");
        let terminal_secret = read("E2M_TERMINAL_SECRET");
        let operator_id = read("E2M_OPERATOR_ID");
        let operator_pin = read("E2M_OPERATOR_PIN");
        let supervisor_id = read("E2M_SUPERVISOR_ID");
        let supervisor_pin = read("E2M_SUPERVISOR_PIN");
        let product_id = read("E2M_PRODUCT_ID");
        let product_sku = read("E2M_PRODUCT_SKU");
        let currency = read("E2M_CURRENCY");

        if !missing.is_empty() {
            return Err(format!(
                "this harness needs a real enrolled terminal and cannot invent one. Missing: {}",
                missing.join(", ")
            ));
        }

        Ok(Self {
            operator_id: OperatorId::new(&operator_id).map_err(|e| e.to_string())?,
            operator_pin: Pin::parse(&operator_pin).map_err(|e| e.to_string())?,
            supervisor_id: OperatorId::new(&supervisor_id).map_err(|e| e.to_string())?,
            supervisor_pin: Pin::parse(&supervisor_pin).map_err(|e| e.to_string())?,
            base_url,
            terminal_code,
            hardware_id,
            terminal_secret,
            product_id,
            product_sku,
            currency,
        })
    }
}

/// Reads the configuration or fails the test with the whole list.
fn config() -> LiveConfig {
    LiveConfig::from_env().unwrap_or_else(|reason| panic!("{reason}"))
}

// ============================================================================
// The two credentials
// ============================================================================

/// Enrols the client as the terminal, and asserts the platform said so.
///
/// Asserting on the response rather than on the absence of an error is the point: a 401 with an
/// empty body and a 200 both "do not panic", which is exactly how the previous audit's wrong paths
/// passed a compile and proved nothing.
async fn enrol(client: &ApiClient, config: &LiveConfig) {
    let response = client
        .login_terminal(
            &config.terminal_code,
            &config.hardware_id,
            &config.terminal_secret,
        )
        .await
        .expect("the terminal must be able to log in before anything else is meaningful");

    assert!(
        !response.session_token.trim().is_empty(),
        "a terminal login that returns no session token has not enrolled anything"
    );
    assert_eq!(
        response.terminal_code, config.terminal_code,
        "the platform answered about a different terminal than the one that asked"
    );

    client.set_token(response.session_token).await;
    client.set_terminal_id(response.terminal_id).await;
}

/// Signs an operator in and presents their session on every later request.
///
/// Returns the operator session so a caller can assert on it. `VerifyPinResponse.session` is
/// `Option` because the route falls back to `authMiddleware` when no terminal authenticated the
/// request — for a till that is a **till-side bug**, a request that went out without
/// `X-Terminal-Token`, so it is asserted rather than degraded around.
async fn sign_in(client: &ApiClient, operator: &OperatorId, pin: &Pin) -> SessionToken {
    let response = client
        .verify_operator_pin(operator, pin)
        .await
        .expect("the operator's PIN must verify before any till write is reachable");

    let session = response
        .session
        .expect("no operator session: this request went out without X-Terminal-Token");
    assert_eq!(
        &response.operator_id, operator,
        "the platform verified a different operator than the one that presented a PIN"
    );

    let token = session.token().clone();
    client.set_operator_token(token.clone()).await;
    token
}

// ============================================================================
// The sequence
// ============================================================================

/// Enrol, sign in, open a shift, ring a sale, find it, void it — through the real client.
///
/// This is the assertion the whole issue rests on. A repointed path that compiles proves nothing;
/// the previous audit's wrong paths compiled too.
#[tokio::test]
#[ignore = "needs a current platform and a real enrolled terminal; see this file's header"]
async fn a_till_can_open_a_shift_ring_a_sale_find_it_and_void_it() {
    let config = config();
    let client = ApiClient::new(&config.base_url);

    enrol(&client, &config).await;
    sign_in(&client, &config.operator_id, &config.operator_pin).await;

    // ---- open a shift -------------------------------------------------------------------------
    let shift = client
        .start_shift(&StartShiftRequest {
            shift_number: format!("HARNESS-{}", config.terminal_code),
            operator_id: config.operator_id.clone(),
            terminal_id: config.terminal_code.clone(),
            opening_cash: Decimal::ZERO,
            currency: config.currency.clone(),
            started_at: "2026-08-24T09:00:00.000Z".to_string(),
        })
        .await
        .expect("an enrolled, attended till may open a shift");

    let server_shift = shift.id;
    println!("shift opened: {server_shift}");

    // ---- ring a sale --------------------------------------------------------------------------
    let sale = client
        .create_transaction(&a_sale(&config, Some(server_shift.clone())))
        .await
        .expect("an enrolled, attended till may ring a sale");

    assert!(
        !sale.receipt_number.trim().is_empty(),
        "a recorded sale must come back with the receipt number the till prints"
    );
    println!("sale recorded: {} / {}", sale.id, sale.receipt_number);

    // ---- find it by receipt -------------------------------------------------------------------
    let found = client
        .get_transaction_by_receipt(&sale.receipt_number)
        .await
        .expect("a sale the platform just recorded must be findable by its receipt number");

    assert_eq!(
        found.id, sale.id,
        "the by-receipt lookup returned a different transaction than the one just written"
    );
    assert_eq!(
        found.operator_id, config.operator_id,
        "the sale must be attributed to the operator who signed in, not to the terminal"
    );
    assert!(
        !found.items.is_empty(),
        "a sale that came back with no lines did not record what was sold"
    );

    // ---- and the control for the lookup -------------------------------------------------------
    // Without this, `get_transaction_by_receipt` could be answering about anything at all. A
    // receipt the platform does not hold must be a NOT_FOUND refusal, not an empty success and
    // not an unreadable body.
    let missing = client
        .get_transaction_by_receipt("HARNESS-NO-SUCH-RECEIPT")
        .await
        .expect_err("a receipt the platform does not hold is not a success");
    assert!(
        matches!(&missing, ApiFailure::Refused { code, .. } if *code == ServerErrorCode::NotFound),
        "a missing receipt must refuse with NOT_FOUND, got: {missing:?}"
    );

    // ---- void it ------------------------------------------------------------------------------
    // `POS_VOID` sits above CASHIER, so this needs the supervisor credential. Signing in again
    // replaces the operator token on the same client, which is the real flow: a supervisor steps
    // up to the till.
    sign_in(&client, &config.supervisor_id, &config.supervisor_pin).await;

    let voided = client
        .void_transaction(
            &sale.id,
            &VoidTransactionRequest {
                reason: "harness".to_string(),
            },
        )
        .await
        .expect("a supervisor may void a sale from an enrolled till");
    assert_eq!(
        voided.id, sale.id,
        "the void answered about a different transaction"
    );

    // ---- close the shift ----------------------------------------------------------------------
    client
        .end_shift(
            server_shift.as_str(),
            &EndShiftRequest {
                closing_cash: Decimal::ZERO,
                expected_cash: Decimal::ZERO,
                variance: Decimal::ZERO,
                note: Some("harness".to_string()),
                ended_at: "2026-08-24T17:00:00.000Z".to_string(),
                denomination_breakdown: None,
            },
        )
        .await
        .expect("the till that opened this shift may close it");
}

// ============================================================================
// The two refusals, verified by revoking, separately
// ============================================================================

/// A cashier attempting a refund is told **who can authorise it**.
///
/// `CASHIER_CAPABILITIES` is `['POS_READ', 'POS_CREATE']` (`operator-capabilities.ts:66`) and
/// `POS_REFUND` sits at SUPERVISOR (`:76-80`). So this refusal needs no server-side setup beyond
/// an operator who is a cashier — it is the ordinary state of the shop.
///
/// The assertion that matters is `heldBy` arriving populated. A till that renders every 403 as
/// "fetch a supervisor" passes a test that only checks the status.
#[tokio::test]
#[ignore = "needs a current platform and a real enrolled terminal; see this file's header"]
async fn a_cashier_refunding_is_told_which_roles_can_authorise_it() {
    let config = config();
    let client = ApiClient::new(&config.base_url);

    enrol(&client, &config).await;
    sign_in(&client, &config.operator_id, &config.operator_pin).await;

    let failure = client
        .create_return(&CreateReturnRequest {
            original_transaction_id: "00000000-0000-4000-8000-000000000000".to_string(),
            items: vec![ReturnItemRequest {
                original_item_id: "00000000-0000-4000-8000-000000000001".to_string(),
                quantity: Decimal::ONE,
                reason: "harness".to_string(),
            }],
            refund_method: "CASH".to_string(),
            refund_amount: Decimal::ONE,
            operator_id: config.operator_id.clone(),
            terminal_id: config.terminal_code.clone(),
            shift_id: None,
        })
        .await
        .expect_err("a cashier does not hold POS_REFUND");

    // The capability gate runs ahead of the handler (`pos-route-table.ts:124-130`), so this is
    // refused before the transaction id above is ever looked at — which is why a bogus one is fine
    // here and would not be if this test expected to reach the handler.
    let standing = CapabilityStanding::of(&failure);
    let CapabilityStanding::SupervisorHolds(approval) = &standing else {
        panic!("a cashier refunding must be told a supervisor can, got: {standing:?}");
    };
    let approval = approval
        .as_ref()
        .expect("the refusal must name who holds POS_REFUND; that is the whole point of heldBy");

    assert_eq!(approval.capability.as_str(), "POS_REFUND");
    assert!(
        !approval.held_by.is_empty(),
        "heldBy must not arrive empty — an empty list is a supervisor prompt naming nobody"
    );
    println!(
        "refund refused, held by: {:?}",
        approval.held_by.iter().collect::<Vec<_>>()
    );
}

/// A capability **no** operator role holds must refuse differently.
///
/// The control for the test above, and the one the design insists on running separately: *a test
/// that exercises only the first passes against a till that renders both as "fetch a supervisor",
/// which is the defect.*
///
/// # This one needs setup, and the setup is the test
///
/// There is no till-reachable route that declares a back-office-only capability — if there were,
/// this refusal would be reachable in the ordinary run of the shop. So producing
/// `POS_OPERATOR_CAPABILITY_DENIED` means arranging one of the two states that produce it:
/// an operator whose stored role the capability enum no longer names, or a route whose declared
/// capability sits outside every operator role. Both are server-side, and both belong to whoever
/// runs this against a real platform.
///
/// It is written and left failing-by-absence rather than omitted: an unwritten control is
/// indistinguishable from a control that passed.
#[tokio::test]
#[ignore = "needs a current platform AND an operator whose role holds no till capability; see the doc comment"]
async fn a_capability_no_role_holds_is_not_rendered_as_fetch_a_supervisor() {
    let config = config();
    let denied_operator = env::var("E2M_DENIED_OPERATOR_ID").unwrap_or_else(|_| {
        panic!(
            "this control needs E2M_DENIED_OPERATOR_ID: an operator whose stored role holds none \
             of the till capabilities. Without it the supervisor test above stands alone, and it \
             passes against a till that renders every 403 as \"fetch a supervisor\""
        )
    });
    let denied_pin = env::var("E2M_DENIED_OPERATOR_PIN").expect("the denied operator's PIN");

    let client = ApiClient::new(&config.base_url);
    enrol(&client, &config).await;
    sign_in(
        &client,
        &OperatorId::new(&denied_operator).expect("a configured id is never blank"),
        &Pin::parse(&denied_pin).expect("a configured PIN is digits"),
    )
    .await;

    let failure = client
        .create_transaction(&a_sale(&config, None))
        .await
        .expect_err("an operator holding no till capability may not ring a sale");

    let standing = CapabilityStanding::of(&failure);
    assert!(
        matches!(standing, CapabilityStanding::NoOperatorRoleHolds(_)),
        "a capability no operator role holds must NOT read as an escalatable one — rendering it \
         as \"fetch a supervisor\" sends a cashier after someone who is refused in turn. Got: \
         {standing:?}"
    );
    assert!(!standing.escalating_at_the_till_can_help());
}

// ============================================================================
// Fixtures
// ============================================================================

fn a_sale(config: &LiveConfig, shift_id: Option<ServerShiftId>) -> CreateTransactionRequest {
    let unit_price = Decimal::ONE;

    CreateTransactionRequest {
        transaction_number: format!("HARNESS-{}-1", config.terminal_code),
        transaction_type: "SALE".to_string(),
        items: vec![TransactionItemDto {
            product_id: config.product_id.clone(),
            product_name: "harness line".to_string(),
            sku: config.product_sku.clone(),
            quantity: Decimal::ONE,
            unit_price,
            tax_rate: Decimal::ZERO,
            tax_amount: Decimal::ZERO,
            discount_amount: Decimal::ZERO,
            line_total: unit_price,
            product_type: None,
            inventory_deducted: false,
        }],
        payments: vec![PaymentDto {
            method: "cash".to_string(),
            amount: unit_price,
            currency: config.currency.clone(),
            reference: None,
            card_last_four: None,
            card_type: None,
            auth_code: None,
            wallet_type: None,
        }],
        subtotal: unit_price,
        tax_total: Decimal::ZERO,
        discount_total: Decimal::ZERO,
        grand_total: unit_price,
        currency: config.currency.clone(),
        customer_id: None,
        customer_name: None,
        shift_id,
        terminal_id: config.terminal_code.clone(),
        operator_id: config.operator_id.clone(),
        note: Some("harness".to_string()),
        created_at: "2026-08-24T09:05:00.000Z".to_string(),
        completed_at: Some("2026-08-24T09:05:00.000Z".to_string()),
    }
}
