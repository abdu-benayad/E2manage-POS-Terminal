# E2E API Test Plan - Phase 2: Complete Coverage

## Overview

This plan completes the E2E API test coverage for the POS Terminal system, adding tests for all untested endpoints identified in Phase 1.

**Current Coverage**: ~75% (49 tests passing)
**Target Coverage**: 100% (estimated 85+ tests)

---

## Phase 2 Test Modules

### Module 10: OTA (Over-The-Air) Updates
**Priority**: HIGH - Critical for device management
**Endpoints**: 6
**New Tests**: 8

### Module 11: POS Configuration
**Priority**: HIGH - Operational settings
**Endpoints**: 2
**New Tests**: 4

### Module 12: Screen Management (Admin)
**Priority**: MEDIUM - UI configuration
**Endpoints**: 5
**New Tests**: 6

### Module 13: Fleet Admin Commands
**Priority**: MEDIUM - Device control
**Endpoints**: 3
**New Tests**: 5

### Module 14: Payment Management
**Priority**: MEDIUM - Financial operations
**Endpoints**: 4
**New Tests**: 5

### Module 15: Return Management (Extended)
**Priority**: MEDIUM - Completeness
**Endpoints**: 2
**New Tests**: 3

### Module 16: Terminal Management (Extended)
**Priority**: LOW - Admin operations
**Endpoints**: 3
**New Tests**: 4

---

## Detailed Test Specifications

### Module 10: OTA Updates (`p10_ota_updates`)

```
Location: tests/api_tests.rs (add new module)
Auth: Mix of terminal token (X-Terminal-Token) and user JWT
```

#### Test Cases

| # | Test Name | Endpoint | Method | Auth | Description |
|---|-----------|----------|--------|------|-------------|
| 1 | `test_01_check_for_updates_no_update` | `/api/pos/ota/check` | GET | Terminal | Check updates when none available |
| 2 | `test_02_create_release` | `/api/pos/ota/releases` | POST | User JWT | Create new OTA release |
| 3 | `test_03_list_releases` | `/api/pos/ota/releases` | GET | User JWT | List all releases |
| 4 | `test_04_check_for_updates_available` | `/api/pos/ota/check` | GET | Terminal | Check updates when one is available |
| 5 | `test_05_update_rollout_percentage` | `/api/pos/ota/releases/:id/rollout` | PATCH | User JWT | Update rollout to 50% |
| 6 | `test_06_toggle_release_active` | `/api/pos/ota/releases/:id/active` | PATCH | User JWT | Deactivate a release |
| 7 | `test_07_delete_release` | `/api/pos/ota/releases/:id` | DELETE | User JWT | Delete a release |
| 8 | `test_08_invalid_version_format` | `/api/pos/ota/releases` | POST | User JWT | Reject invalid version format |

#### Request/Response Structures

```rust
// Create Release Request
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateReleaseRequest {
    pub version: String,           // e.g., "1.2.0"
    pub release_notes: String,
    pub download_url: String,
    pub checksum: String,          // SHA256
    pub file_size: i64,
    pub min_app_version: Option<String>,
    pub is_mandatory: bool,
    pub rollout_percentage: i32,   // 0-100
}

// Release Response
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseResponse {
    pub id: String,
    pub version: String,
    pub release_notes: String,
    pub download_url: String,
    pub is_active: bool,
    pub rollout_percentage: i32,
    pub created_at: String,
}

// Check Updates Response
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCheckResponse {
    pub update_available: bool,
    pub current_version: Option<String>,
    pub latest_version: Option<String>,
    pub release: Option<ReleaseResponse>,
    pub is_mandatory: bool,
}
```

---

### Module 11: POS Configuration (`p11_pos_config`)

```
Location: tests/api_tests.rs (add new module)
Auth: User JWT with POS_ADMIN permission
```

#### Test Cases

| # | Test Name | Endpoint | Method | Auth | Description |
|---|-----------|----------|--------|------|-------------|
| 1 | `test_01_get_config` | `/api/pos/config` | GET | User JWT | Get current POS configuration |
| 2 | `test_02_update_config` | `/api/pos/config` | PUT | User JWT | Update configuration settings |
| 3 | `test_03_update_invalid_config` | `/api/pos/config` | PUT | User JWT | Reject invalid configuration |
| 4 | `test_04_config_requires_admin` | `/api/pos/config` | PUT | None | Verify admin permission required |

#### Request/Response Structures

```rust
// POS Configuration
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PosConfigRequest {
    pub default_currency: String,
    pub tax_rate: f64,
    pub tax_inclusive: bool,
    pub require_customer_for_sale: bool,
    pub allow_negative_inventory: bool,
    pub receipt_header: Option<String>,
    pub receipt_footer: Option<String>,
    pub shift_duration_hours: Option<i32>,
    pub auto_close_shift: bool,
    pub enable_offline_mode: bool,
    pub offline_sync_interval_minutes: i32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PosConfigResponse {
    pub tenant_id: String,
    pub config: PosConfigRequest,
    pub updated_at: String,
}
```

---

### Module 12: Screen Management (`p12_screen_management`)

```
Location: tests/api_tests.rs (add new module)
Auth: User JWT with POS_SCREENS_WRITE permission
```

#### Test Cases

| # | Test Name | Endpoint | Method | Auth | Description |
|---|-----------|----------|--------|------|-------------|
| 1 | `test_01_create_screen` | `/api/pos/screens` | PUT | User JWT | Create new POS screen layout |
| 2 | `test_02_list_screens` | `/api/pos/screens` | GET | User JWT | List all screens |
| 3 | `test_03_get_screen_by_id` | `/api/pos/screens/:screenId` | GET | User JWT | Get specific screen |
| 4 | `test_04_toggle_screen_active` | `/api/pos/screens/:screenId/active` | PATCH | User JWT | Deactivate a screen |
| 5 | `test_05_update_screen` | `/api/pos/screens` | PUT | User JWT | Update existing screen |
| 6 | `test_06_delete_screen` | `/api/pos/screens/:screenId` | DELETE | User JWT | Delete a screen |

#### Request/Response Structures

```rust
// Screen Definition
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateScreenRequest {
    pub screen_id: Option<String>,  // None for create, Some for update
    pub name: String,
    pub name_ar: Option<String>,
    pub sector: String,              // RETAIL, RESTAURANT, etc.
    pub screen_type: String,         // MAIN, CATEGORY, FAVORITES
    pub layout: serde_json::Value,   // Screen layout JSON
    pub is_active: bool,
    pub sort_order: i32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScreenResponse {
    pub screen_id: String,
    pub name: String,
    pub sector: String,
    pub is_active: bool,
    pub layout: serde_json::Value,
    pub created_at: String,
}
```

---

### Module 13: Fleet Admin Commands (`p13_fleet_admin`)

```
Location: tests/api_tests.rs (add new module)
Auth: User JWT with pos:fleet:commands permission
```

#### Test Cases

| # | Test Name | Endpoint | Method | Auth | Description |
|---|-----------|----------|--------|------|-------------|
| 1 | `test_01_get_terminal_details` | `/api/pos/fleet/terminals/:id` | GET | User JWT | Get detailed terminal info |
| 2 | `test_02_send_sync_command` | `/api/pos/fleet/commands` | POST | User JWT | Send SYNC command to terminal |
| 3 | `test_03_send_restart_command` | `/api/pos/fleet/commands` | POST | User JWT | Send RESTART command |
| 4 | `test_04_approve_terminal` | `/api/pos/fleet/terminals/:id/action` | POST | User JWT | Approve pending terminal |
| 5 | `test_05_suspend_terminal` | `/api/pos/fleet/terminals/:id/action` | POST | User JWT | Suspend active terminal |

#### Request/Response Structures

```rust
// Send Command Request
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SendCommandRequest {
    pub terminal_ids: Vec<String>,    // Target terminal UUIDs
    pub command_type: String,         // SYNC, RESTART, LOGOUT, UPDATE
    pub payload: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandResponse {
    pub command_id: String,
    pub terminals_targeted: i32,
    pub status: String,
}

// Terminal Action Request
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalActionRequest {
    pub action: String,               // APPROVE, REJECT, SUSPEND, ACTIVATE
    pub reason: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalDetailsResponse {
    pub id: String,
    pub terminal_id: String,
    pub status: String,
    pub hardware_id: String,
    pub app_version: Option<String>,
    pub last_heartbeat: Option<String>,
    pub current_shift: Option<serde_json::Value>,
    pub metrics: Option<serde_json::Value>,
}
```

---

### Module 14: Payment Management (`p14_payment_management`)

```
Location: tests/api_tests.rs (add new module)
Auth: User JWT with POS_CREATE/POS_REFUND permissions
```

#### Test Cases

| # | Test Name | Endpoint | Method | Auth | Description |
|---|-----------|----------|--------|------|-------------|
| 1 | `test_01_record_payment` | `/api/pos/payments` | POST | User JWT | Record payment for transaction |
| 2 | `test_02_list_transaction_payments` | `/api/pos/payments/transaction/:txnId` | GET | User JWT | List payments for a transaction |
| 3 | `test_03_get_payment_by_id` | `/api/pos/payments/:paymentId` | GET | User JWT | Get payment details |
| 4 | `test_04_refund_payment` | `/api/pos/payments/:paymentId/refund` | POST | User JWT | Refund a payment |
| 5 | `test_05_cannot_refund_already_refunded` | `/api/pos/payments/:paymentId/refund` | POST | User JWT | Prevent double refund |

#### Request/Response Structures

```rust
// Record Payment Request
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordPaymentRequest {
    pub transaction_id: String,
    pub payment_method: String,       // CASH, CARD, MOBILE
    pub amount: f64,
    pub currency: String,
    pub reference: Option<String>,    // Card auth code, etc.
    pub tip_amount: Option<f64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PaymentResponse {
    pub id: String,
    pub transaction_id: String,
    pub payment_method: String,
    pub amount: serde_json::Value,    // Decimal
    pub status: String,
    pub reference: Option<String>,
    pub refunded_at: Option<String>,
}

// Refund Request
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RefundPaymentRequest {
    pub reason: String,
    pub refund_method: Option<String>,  // Defaults to original method
}
```

---

### Module 15: Return Management Extended (`p15_return_extended`)

```
Location: tests/api_tests.rs (extend p5_return_management)
Auth: User JWT with POS_READ permission
```

#### Test Cases

| # | Test Name | Endpoint | Method | Auth | Description |
|---|-----------|----------|--------|------|-------------|
| 1 | `test_03_list_returns` | `/api/pos/returns` | GET | User JWT | List all returns with filters |
| 2 | `test_04_get_return_by_id` | `/api/pos/returns/:returnId` | GET | User JWT | Get return details |
| 3 | `test_05_list_returns_by_transaction` | `/api/pos/returns/transaction/:txnId` | GET | User JWT | Get returns for transaction |

---

### Module 16: Terminal Management Extended (`p16_terminal_extended`)

```
Location: tests/api_tests.rs (extend p1_terminal_management)
Auth: User JWT with POS_MANAGE permission
```

#### Test Cases

| # | Test Name | Endpoint | Method | Auth | Description |
|---|-----------|----------|--------|------|-------------|
| 1 | `test_08_list_terminals` | `/api/pos/terminals` | GET | User JWT | List all terminals |
| 2 | `test_09_get_terminal_by_id` | `/api/pos/terminals/:terminalId` | GET | User JWT | Get terminal details |
| 3 | `test_10_update_terminal` | `/api/pos/terminals/:terminalId` | PUT | User JWT | Update terminal info |
| 4 | `test_11_delete_terminal` | `/api/pos/terminals/:terminalId` | DELETE | User JWT | Delete terminal |

---

## Implementation Order

### Phase 2A: High Priority (Critical Features)
1. **Module 10: OTA Updates** - 8 tests
2. **Module 11: POS Configuration** - 4 tests

### Phase 2B: Medium Priority (Complete Coverage)
3. **Module 14: Payment Management** - 5 tests
4. **Module 13: Fleet Admin Commands** - 5 tests
5. **Module 12: Screen Management** - 6 tests
6. **Module 15: Return Extended** - 3 tests

### Phase 2C: Low Priority (Admin Operations)
7. **Module 16: Terminal Extended** - 4 tests

---

## Test File Structure

```
tests/
├── e2e_api_tests.rs           # Entry point (existing)
├── api_tests.rs               # Main tests (existing, extend)
└── api_tests_phase2.rs        # Phase 2 tests (new file, optional)
```

### Option A: Extend Existing File
Add new modules to `api_tests.rs`:
- `mod p10_ota_updates { ... }`
- `mod p11_pos_config { ... }`
- etc.

### Option B: Separate File (Recommended for maintainability)
Create `api_tests_phase2.rs` with:
- All Phase 2 modules
- Shared imports from `api_tests.rs`

---

## Test Data Requirements

### OTA Updates
- Version strings: "1.0.0", "1.1.0", "2.0.0"
- Download URLs (can be mock/placeholder)
- SHA256 checksums

### Screen Management
- Sample screen layout JSON
- Multiple sector types

### Fleet Commands
- At least 2 registered terminals
- Pending terminal for approval tests

---

## Environment Setup

```bash
# Ensure test user has required permissions
./scripts/setup-test-env.sh

# Required permissions for Phase 2:
# - pos:ota:read, pos:ota:write
# - pos:config:read, pos:config:write
# - pos:screens:read, pos:screens:write
# - pos:fleet:read, pos:fleet:commands
# - pos:payments:read, pos:payments:create, pos:payments:refund
```

---

## Success Criteria

| Metric | Phase 1 | Phase 2 Target |
|--------|---------|----------------|
| Total Tests | 49 | 85+ |
| Endpoints Covered | ~48 | 64 (100%) |
| Coverage % | ~75% | 100% |
| Pass Rate | 100% | 100% |

---

## Run Commands

```bash
# Run Phase 2 tests only
BACKEND_URL=http://localhost:3000 cargo test --test e2e_api_tests p10_ p11_ p12_ p13_ p14_ p15_ p16_ -- --ignored --test-threads=1 --nocapture

# Run all tests (Phase 1 + Phase 2)
BACKEND_URL=http://localhost:3000 cargo test --test e2e_api_tests -- --ignored --test-threads=1 --nocapture
```

---

## Timeline Estimate

| Phase | Tests | Complexity |
|-------|-------|------------|
| Phase 2A | 12 tests | Medium |
| Phase 2B | 19 tests | Medium-High |
| Phase 2C | 4 tests | Low |
| **Total** | **35 tests** | |

---

## Notes

1. **Backend Bug**: Fleet status endpoint has RBAC issue - may need backend fix before testing
2. **Missing Endpoints**: Verify `sync/operators` and `terminals/logout` exist in backend
3. **Permission Setup**: May need to add test user permissions for admin operations
4. **Z-Report**: Currently returns 404 - verify implementation status

---

## Files to Modify

1. `tests/api_tests.rs` - Add new test modules
2. `tests/e2e_api_tests.rs` - Register new modules if needed
3. `scripts/setup-test-env.sh` - Add permission grants for new test categories
