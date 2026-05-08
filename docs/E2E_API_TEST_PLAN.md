# POS Terminal E2E API Test Plan

## Overview

This document outlines the comprehensive End-to-End API testing plan for the POS Terminal communicating with the E2Manage backend.

**Backend Base URL:** `http://localhost:3000/api/pos`

---

## Test Workflow Sequence

The tests must run in a specific order to satisfy dependencies:

```
1. Terminal Registration → Terminal Auth →
2. Sync (Catalog, Operators, Screens) →
3. Shift Start →
4. Transactions (Sale, Void) →
5. Returns →
6. Cash Drawer Events →
7. Offline Queue (Upload, Process) →
8. Shift End →
9. Reports (Daily, Shift, Z-Report)
```

---

## Phase 1: Terminal Management

### 1.1 Terminal Registration
| Endpoint | `POST /api/pos/terminals/register` |
|----------|-----------------------------------|
| Auth | None (public) |
| Purpose | Register a new POS terminal |

**Request:**
```json
{
  "hardwareId": "HW-TEST-001",
  "name": "Test Terminal 1",
  "sector": "RETAIL",
  "branchId": null
}
```

**Expected Response:**
```json
{
  "success": true,
  "data": {
    "terminalId": "uuid",
    "terminalCode": "TERM-001",
    "secret": "terminal-secret-key",
    "pairingQr": "optional-qr-code"
  }
}
```

**Test Cases:**
- [ ] Register new terminal successfully
- [ ] Duplicate hardwareId returns existing terminal
- [ ] Invalid sector returns 400
- [ ] Missing required fields returns 400

---

### 1.2 Terminal Authentication
| Endpoint | `POST /api/pos/terminals/authenticate` |
|----------|----------------------------------------|
| Auth | None |
| Purpose | Login terminal and get session token |

**Request:**
```json
{
  "hardwareId": "HW-TEST-001",
  "secret": "terminal-secret-key"
}
```

**Expected Response:**
```json
{
  "success": true,
  "data": {
    "sessionToken": "jwt-token",
    "terminalId": "uuid",
    "terminalCode": "TERM-001",
    "tenantId": "tenant-uuid",
    "companyId": "company-uuid",
    "config": {
      "locale": "ar",
      "currency": "LYD",
      "businessSector": "RETAIL"
    }
  }
}
```

**Test Cases:**
- [ ] Valid credentials return session token
- [ ] Invalid secret returns 401
- [ ] Non-existent terminal returns 404
- [ ] Disabled terminal returns 403

---

### 1.3 Token Refresh
| Endpoint | `POST /api/pos/terminals/refresh` |
|----------|----------------------------------|
| Auth | Bearer Token |
| Purpose | Refresh expiring session token |

**Test Cases:**
- [ ] Valid token returns new token
- [ ] Expired token returns 401
- [ ] Invalid token returns 401

---

### 1.4 Terminal Logout
| Endpoint | `POST /api/pos/terminals/logout` |
|----------|----------------------------------|
| Auth | Bearer Token |
| Purpose | Invalidate session token |

**Test Cases:**
- [ ] Logout invalidates token
- [ ] Subsequent requests with old token fail

---

## Phase 2: Sync APIs

### 2.1 Catalog Sync
| Endpoint | `GET /api/pos/sync/catalog` |
|----------|----------------------------|
| Auth | Bearer Token |
| Purpose | Get products and categories |

**Query Params:**
- `includeCategories` (boolean) - Include categories
- `businessSector` (string) - Filter by sector

**Headers:**
- `If-None-Match` - ETag for caching

**Test Cases:**
- [ ] Full catalog returns products
- [ ] ETag returns 304 Not Modified
- [ ] Without auth returns 401
- [ ] Include categories flag works

---

### 2.2 Products Sync (Legacy)
| Endpoint | `GET /api/pos/sync/products` |
|----------|------------------------------|
| Auth | Bearer Token |
| Purpose | Get products for sync |

**Query Params:**
- `lastSync` (ISO date) - For incremental sync
- `limit` (number) - Pagination
- `offset` (number) - Pagination

**Test Cases:**
- [ ] Full sync without lastSync
- [ ] Incremental sync with lastSync
- [ ] Pagination works correctly

---

### 2.3 Operators Sync
| Endpoint | `GET /api/pos/sync/operators` |
|----------|-------------------------------|
| Auth | Bearer Token |
| Purpose | Get operators with PIN hashes |

**Test Cases:**
- [ ] Returns operators list
- [ ] PIN hashes are bcrypt format
- [ ] ETag caching works

---

### 2.4 Screens Sync
| Endpoint | `GET /api/pos/sync/screens` |
|----------|----------------------------|
| Auth | Bearer Token |
| Purpose | Get screen definitions |

**Query Params:**
- `sector` (string) - Business sector

**Test Cases:**
- [ ] Returns screen definitions
- [ ] Sector filter works
- [ ] ETag caching works

---

### 2.5 Tenant Config
| Endpoint | `GET /api/pos/sync/tenant-config` |
|----------|-----------------------------------|
| Auth | Bearer Token |
| Purpose | Get tenant configuration |

**Test Cases:**
- [ ] Returns tax config
- [ ] Returns receipt config
- [ ] Returns feature flags

---

## Phase 3: Shift Management

### 3.1 Start Shift
| Endpoint | `POST /api/pos/shifts` |
|----------|------------------------|
| Auth | Bearer Token |
| Purpose | Open a new shift |

**Request:**
```json
{
  "terminalId": "terminal-uuid",
  "operatorId": "operator-uuid",
  "openingCash": 100.00
}
```

**Test Cases:**
- [ ] Start shift successfully
- [ ] Cannot start shift while another is open
- [ ] Invalid terminal returns 400
- [ ] Returns shift ID and number

---

### 3.2 Get Current Shift
| Endpoint | `GET /api/pos/shifts/current` |
|----------|-------------------------------|
| Auth | Bearer Token |
| Purpose | Get active shift for terminal |

**Test Cases:**
- [ ] Returns current shift details
- [ ] Returns null/404 if no active shift

---

### 3.3 End Shift
| Endpoint | `POST /api/pos/shifts/:id/end` |
|----------|--------------------------------|
| Auth | Bearer Token |
| Purpose | Close a shift |

**Request:**
```json
{
  "closingCash": 250.00,
  "notes": "End of day"
}
```

**Test Cases:**
- [ ] End shift successfully
- [ ] Calculate expected vs actual cash
- [ ] Cannot end already closed shift
- [ ] Returns shift summary

---

## Phase 4: Transaction Management

### 4.1 Create Transaction
| Endpoint | `POST /api/pos/transactions` |
|----------|------------------------------|
| Auth | Bearer Token |
| Purpose | Create a new sale/return |

**Request:**
```json
{
  "terminalId": "terminal-uuid",
  "shiftId": "shift-uuid",
  "operatorId": "operator-uuid",
  "transactionType": "SALE",
  "items": [
    {
      "productId": "product-uuid",
      "quantity": 2,
      "unitPrice": 10.00,
      "discountAmount": 0,
      "taxRate": 15
    }
  ],
  "payments": [
    {
      "paymentType": "CASH",
      "amount": 23.00
    }
  ],
  "subtotal": 20.00,
  "taxTotal": 3.00,
  "discountTotal": 0,
  "grandTotal": 23.00,
  "customerId": null
}
```

**Test Cases:**
- [ ] Create cash sale
- [ ] Create card sale
- [ ] Create split payment sale
- [ ] Create sale with discount
- [ ] Insufficient payment returns error
- [ ] Invalid product returns error
- [ ] Returns transaction ID and receipt number

---

### 4.2 Get Transaction
| Endpoint | `GET /api/pos/transactions/:id` |
|----------|--------------------------------|
| Auth | Bearer Token |
| Purpose | Get transaction details |

**Test Cases:**
- [ ] Returns full transaction with items
- [ ] Non-existent ID returns 404
- [ ] Cross-tenant access denied

---

### 4.3 Get Transaction by Receipt
| Endpoint | `GET /api/pos/transactions/by-receipt/:number` |
|----------|------------------------------------------------|
| Auth | Bearer Token |
| Purpose | Lookup transaction for returns |

**Test Cases:**
- [ ] Returns transaction by receipt number
- [ ] Invalid receipt returns 404

---

### 4.4 Void Transaction
| Endpoint | `POST /api/pos/transactions/:id/void` |
|----------|---------------------------------------|
| Auth | Bearer Token |
| Purpose | Void a transaction |

**Request:**
```json
{
  "reason": "Customer cancelled",
  "operatorId": "operator-uuid"
}
```

**Test Cases:**
- [ ] Void within allowed time
- [ ] Cannot void already voided
- [ ] Cannot void after time limit
- [ ] Requires void permission

---

## Phase 5: Return Management

### 5.1 Create Return
| Endpoint | `POST /api/pos/returns` |
|----------|-------------------------|
| Auth | Bearer Token |
| Purpose | Process a return |

**Request:**
```json
{
  "originalTransactionId": "transaction-uuid",
  "terminalId": "terminal-uuid",
  "shiftId": "shift-uuid",
  "operatorId": "operator-uuid",
  "items": [
    {
      "originalItemId": "item-uuid",
      "quantity": 1,
      "reason": "Defective"
    }
  ],
  "refundMethod": "CASH",
  "refundAmount": 11.50
}
```

**Test Cases:**
- [ ] Full return
- [ ] Partial return
- [ ] Cannot return more than purchased
- [ ] Cannot double-return same items

---

## Phase 6: Offline Queue

### 6.1 Upload Offline Transactions
| Endpoint | `POST /api/pos/offline/upload` |
|----------|--------------------------------|
| Auth | X-Terminal-Token |
| Purpose | Upload offline transactions |

**Request:**
```json
{
  "transactions": [
    {
      "localId": "local-uuid",
      "terminalId": "terminal-uuid",
      "transactionType": "SALE",
      "items": [...],
      "payments": [...],
      "createdAt": "ISO-timestamp"
    }
  ]
}
```

**Test Cases:**
- [ ] Upload single transaction
- [ ] Upload batch transactions
- [ ] Duplicate localId detected
- [ ] Returns queue IDs

---

### 6.2 Process Queue
| Endpoint | `POST /api/pos/sync/queue/:terminalId/process` |
|----------|------------------------------------------------|
| Auth | Bearer Token |
| Purpose | Process pending queue items |

**Test Cases:**
- [ ] Process pending items
- [ ] Failed items marked with error
- [ ] Returns processed/failed counts

---

### 6.3 Queue Status
| Endpoint | `GET /api/pos/sync/queue/:terminalId/stats` |
|----------|---------------------------------------------|
| Auth | Bearer Token |
| Purpose | Get queue statistics |

**Test Cases:**
- [ ] Returns pending/synced/failed counts

---

## Phase 7: Cash Drawer

### 7.1 Log Cash Drawer Event
| Endpoint | `POST /api/pos/cash-drawer/events` |
|----------|-----------------------------------|
| Auth | Bearer Token |
| Purpose | Log cash drawer open/close |

**Request:**
```json
{
  "terminalId": "terminal-uuid",
  "shiftId": "shift-uuid",
  "operatorId": "operator-uuid",
  "eventType": "OPEN",
  "reason": "Sale",
  "transactionId": "optional-transaction-uuid"
}
```

**Test Cases:**
- [ ] Log drawer open for sale
- [ ] Log drawer open for cash in/out
- [ ] Audit trail maintained

---

## Phase 8: Reports

### 8.1 Daily Sales Report
| Endpoint | `GET /api/pos/reports/daily-sales` |
|----------|-----------------------------------|
| Auth | Bearer Token |
| Purpose | Get daily sales summary |

**Query Params:**
- `date` (ISO date)
- `terminalId` (optional)

**Test Cases:**
- [ ] Returns sales by payment method
- [ ] Returns transaction counts
- [ ] Date filter works

---

### 8.2 Shift Report
| Endpoint | `GET /api/pos/reports/shift/:id` |
|----------|----------------------------------|
| Auth | Bearer Token |
| Purpose | Get shift summary |

**Test Cases:**
- [ ] Returns shift totals
- [ ] Returns cash variance

---

### 8.3 Z-Report
| Endpoint | `POST /api/pos/z-reports` |
|----------|---------------------------|
| Auth | Bearer Token |
| Purpose | Generate end-of-day Z-report |

**Test Cases:**
- [ ] Generate Z-report
- [ ] Cannot generate twice same day

---

## Phase 9: Fleet Management

### 9.1 Terminal Heartbeat
| Endpoint | `POST /api/pos/fleet/:terminalId/heartbeat` |
|----------|---------------------------------------------|
| Auth | Bearer Token |
| Purpose | Report terminal health |

**Request:**
```json
{
  "uptimeSeconds": 3600,
  "cpuPercent": 25.5,
  "memoryMb": 512,
  "diskFreeMb": 1024,
  "offlineTxnCount": 0,
  "appVersion": "1.0.0",
  "currentShiftId": "optional-shift-uuid"
}
```

**Test Cases:**
- [ ] Accept heartbeat
- [ ] Return commands (SYNC, RESTART, etc.)
- [ ] Update last_seen timestamp

---

### 9.2 Fleet Status
| Endpoint | `GET /api/pos/fleet/status` |
|----------|----------------------------|
| Auth | Bearer Token |
| Purpose | Get all terminals status |

**Test Cases:**
- [ ] Returns all terminals for tenant
- [ ] Shows online/offline status

---

## Test Execution Order

```bash
# Run all E2E tests in sequence
npm run test:e2e -- --runInBand

# Or manually:
1. test_terminal_registration
2. test_terminal_auth
3. test_sync_catalog
4. test_sync_operators
5. test_start_shift
6. test_create_sale
7. test_void_transaction
8. test_create_return
9. test_offline_upload
10. test_offline_process
11. test_end_shift
12. test_shift_report
13. test_z_report
```

---

## Environment Variables

```bash
# Backend URL
BACKEND_URL=http://localhost:3000

# Test credentials (from terminal registration)
TEST_HARDWARE_ID=HW-TEST-001
TEST_TERMINAL_SECRET=<from-registration>
TEST_AUTH_TOKEN=<from-login>
TEST_TERMINAL_ID=<from-login>

# Test data IDs (populated during test run)
TEST_SHIFT_ID=<from-start-shift>
TEST_TRANSACTION_ID=<from-create-sale>
```

---

## Coverage Summary

| Module | Endpoints | Tested |
|--------|-----------|--------|
| Terminals | 5 | ✅ |
| Sync | 6 | ✅ |
| Shifts | 4 | ✅ |
| Transactions | 4 | ✅ |
| Returns | 3 | ✅ |
| Offline | 4 | ✅ |
| Cash Drawer | 3 | ✅ |
| Reports | 6 | ✅ |
| Fleet | 5 | ✅ |
| Config | 2 | ✅ |
| Screens | 4 | ✅ |
| OTA | 4 | ☐ |
| **Total** | **50** | **46** |

---

## Implementation Status

**Implemented:** 2024-12-13

Test files:
- `tests/e2e_api_tests.rs` - Entry point
- `tests/api_tests.rs` - Main test implementation

### Running Tests

```bash
# Run all E2E tests (requires running backend)
./scripts/run-e2e-tests.sh

# Or manually:
BACKEND_URL=http://localhost:3000 cargo test --test e2e_api_tests -- --ignored --test-threads=1

# Run specific test phase
./scripts/run-e2e-tests.sh terminal   # Terminal management
./scripts/run-e2e-tests.sh sync       # Sync APIs
./scripts/run-e2e-tests.sh shift      # Shift management
./scripts/run-e2e-tests.sh transaction # Transactions
./scripts/run-e2e-tests.sh return     # Returns
./scripts/run-e2e-tests.sh offline    # Offline queue
./scripts/run-e2e-tests.sh cash       # Cash drawer
./scripts/run-e2e-tests.sh reports    # Reports
./scripts/run-e2e-tests.sh fleet      # Fleet management
```

---

## Next Steps

1. ~~Create test data fixtures (tenant, products, operators)~~ ✅ Handled by test state
2. ~~Implement test runner in Rust~~ ✅ Complete
3. Execute tests against running backend
4. Implement OTA update tests (4 endpoints remaining)
