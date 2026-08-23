//! The seam every transport test in `auth-outcome-and-offline-lockout` stands on.
//!
//! `ApiClient` is a concrete `reqwest` struct with no trait boundary, and until this file existed
//! no `wiremock`/`mockito`/`httpmock`/`mockall` appeared in any manifest — so the defects that live
//! at the transport boundary (a non-2xx flattened into a string, a body that arrived and would not
//! parse, a refusal read as weather) had nowhere to be reproduced.
//!
//! **There is deliberately no trait here.** `ApiClient::new` takes a base URL, so pointing it at a
//! server bound to `127.0.0.1` needs no abstraction — and a trait with one production
//! implementation would mean the tests exercise a fake while the real `reqwest` path, which is
//! where the bugs are, stays unexercised.
//!
//! This file is the seam's own proof and is meant to stay small: one round trip through the real
//! client, against a real socket, asserting the body arrived. If it fails, every other transport
//! test in this crate is testing something other than what it claims.

use pos_api::ApiClient;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// A real GET, over a real socket, read through the real client.
///
/// `get_catalog_delta` is the subject because it is an ordinary enveloped GET: the platform wraps
/// its payload in `{success, data}` and the client unwraps it. Asserting a field from *inside*
/// `data` is what proves the whole path ran — a client that never made the request, or that read
/// the envelope as the payload, cannot produce `version`.
#[tokio::test]
async fn the_client_reads_a_body_from_a_mock_server() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/api/pos/sync/catalog/delta"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "success": true,
            "data": {
                "updated": [],
                "deleted": ["sku-gone"],
                "version": "v-42",
                "syncedAt": "2026-08-23T10:00:00.000Z",
            }
        })))
        .mount(&server)
        .await;

    let client = ApiClient::new(&server.uri());

    let delta = client
        .get_catalog_delta("2026-08-23T09:00:00.000Z")
        .await
        .expect("the mock answers 200 with a well-formed envelope");

    // From inside `data`, not from the envelope: this is the assertion that distinguishes "the
    // request happened and was read correctly" from "something returned a default".
    assert_eq!(delta.version, "v-42");
    assert_eq!(delta.deleted, vec!["sku-gone".to_string()]);
    assert!(delta.updated.is_empty());

    // The request reached the server. `MockServer` verifies mounted expectations on drop, but
    // saying it here means a future edit that stops calling the endpoint fails loudly rather than
    // silently asserting over a default-constructed response.
    assert_eq!(
        server.received_requests().await.unwrap_or_default().len(),
        1
    );
}
