#!/bin/bash
#
# POS Terminal E2E API Test Script
# Tests all POS APIs against the running backend
#
# Usage: ./scripts/test-api-e2e.sh [backend_url]
#

set -e

# Configuration
BACKEND_URL="${1:-http://localhost:3000}"
API_BASE="$BACKEND_URL/api/pos"

# Test data
HARDWARE_ID="HW-E2E-TEST-$(date +%s)"
TERMINAL_NAME="E2E Test Terminal"
SECTOR="RETAIL"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Counters
PASSED=0
FAILED=0
SKIPPED=0

# State variables (populated during tests)
TERMINAL_ID=""
TERMINAL_SECRET=""
AUTH_TOKEN=""
SHIFT_ID=""
TRANSACTION_ID=""
RECEIPT_NUMBER=""

# Helper functions
log_info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

log_pass() {
    echo -e "${GREEN}[PASS]${NC} $1"
    PASSED=$((PASSED + 1))
}

log_fail() {
    echo -e "${RED}[FAIL]${NC} $1"
    FAILED=$((FAILED + 1))
}

log_skip() {
    echo -e "${YELLOW}[SKIP]${NC} $1"
    SKIPPED=$((SKIPPED + 1))
}

log_section() {
    echo ""
    echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
    echo -e "${BLUE}  $1${NC}"
    echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
}

# Check if backend is running
check_backend() {
    log_section "Checking Backend"

    RESPONSE=$(curl -s "$BACKEND_URL/api/health" || echo "CONNECTION_FAILED")

    if echo "$RESPONSE" | grep -q "healthy"; then
        log_pass "Backend is running at $BACKEND_URL"
        return 0
    else
        log_fail "Backend not reachable at $BACKEND_URL"
        echo "Response: $RESPONSE"
        exit 1
    fi
}

# ============================================================================
# Phase 1: Terminal Management
# ============================================================================

test_terminal_registration() {
    log_section "Phase 1: Terminal Registration"

    RESPONSE=$(curl -s -X POST "$API_BASE/terminals/register" \
        -H "Content-Type: application/json" \
        -d "{
            \"hardwareId\": \"$HARDWARE_ID\",
            \"name\": \"$TERMINAL_NAME\",
            \"sector\": \"$SECTOR\"
        }")

    if echo "$RESPONSE" | grep -q '"success":true'; then
        TERMINAL_ID=$(echo "$RESPONSE" | grep -o '"terminalId":"[^"]*"' | cut -d'"' -f4)
        TERMINAL_SECRET=$(echo "$RESPONSE" | grep -o '"secret":"[^"]*"' | cut -d'"' -f4)

        if [ -n "$TERMINAL_ID" ] && [ -n "$TERMINAL_SECRET" ]; then
            log_pass "Terminal registered: $TERMINAL_ID"
            echo "  Hardware ID: $HARDWARE_ID"
            echo "  Secret: ${TERMINAL_SECRET:0:20}..."
        else
            log_fail "Missing terminal ID or secret in response"
            echo "Response: $RESPONSE"
        fi
    else
        log_fail "Terminal registration failed"
        echo "Response: $RESPONSE"
    fi
}

test_terminal_auth() {
    log_section "Phase 1: Terminal Authentication"

    if [ -z "$TERMINAL_SECRET" ]; then
        log_skip "No terminal secret (registration failed)"
        return
    fi

    RESPONSE=$(curl -s -X POST "$API_BASE/terminals/authenticate" \
        -H "Content-Type: application/json" \
        -d "{
            \"hardwareId\": \"$HARDWARE_ID\",
            \"secret\": \"$TERMINAL_SECRET\"
        }")

    if echo "$RESPONSE" | grep -q '"success":true'; then
        AUTH_TOKEN=$(echo "$RESPONSE" | grep -o '"sessionToken":"[^"]*"' | cut -d'"' -f4)

        if [ -n "$AUTH_TOKEN" ]; then
            log_pass "Terminal authenticated"
            echo "  Token: ${AUTH_TOKEN:0:30}..."
        else
            log_fail "No session token in response"
            echo "Response: $RESPONSE"
        fi
    else
        log_fail "Terminal authentication failed"
        echo "Response: $RESPONSE"
    fi
}

test_terminal_auth_invalid() {
    log_info "Testing invalid credentials..."

    RESPONSE=$(curl -s -w "\n%{http_code}" -X POST "$API_BASE/terminals/authenticate" \
        -H "Content-Type: application/json" \
        -d "{
            \"hardwareId\": \"$HARDWARE_ID\",
            \"secret\": \"wrong-secret\"
        }")

    HTTP_CODE=$(echo "$RESPONSE" | tail -n1)

    if [ "$HTTP_CODE" = "401" ]; then
        log_pass "Invalid credentials returns 401"
    else
        log_fail "Expected 401, got $HTTP_CODE"
    fi
}

# ============================================================================
# Phase 2: Sync APIs
# ============================================================================

test_sync_catalog() {
    log_section "Phase 2: Sync - Catalog"

    if [ -z "$AUTH_TOKEN" ]; then
        log_skip "No auth token"
        return
    fi

    RESPONSE=$(curl -s -X GET "$API_BASE/sync/catalog?includeCategories=true" \
        -H "Authorization: Bearer $AUTH_TOKEN")

    if echo "$RESPONSE" | grep -q '"success":true'; then
        VERSION=$(echo "$RESPONSE" | grep -o '"version":"[^"]*"' | head -1 | cut -d'"' -f4)
        log_pass "Catalog retrieved"
        echo "  Version: $VERSION"

        # Test ETag caching
        RESPONSE2=$(curl -s -w "\n%{http_code}" -X GET "$API_BASE/sync/catalog" \
            -H "Authorization: Bearer $AUTH_TOKEN" \
            -H "If-None-Match: $VERSION")

        HTTP_CODE=$(echo "$RESPONSE2" | tail -n1)
        if [ "$HTTP_CODE" = "304" ]; then
            log_pass "ETag caching works (304 Not Modified)"
        else
            log_info "ETag test: got $HTTP_CODE (may be OK if data changed)"
        fi
    else
        log_fail "Catalog sync failed"
        echo "Response: ${RESPONSE:0:200}..."
    fi
}

test_sync_products() {
    log_info "Testing products sync..."

    if [ -z "$AUTH_TOKEN" ]; then
        log_skip "No auth token"
        return
    fi

    # Full sync
    RESPONSE=$(curl -s -X GET "$API_BASE/sync/products" \
        -H "Authorization: Bearer $AUTH_TOKEN")

    if echo "$RESPONSE" | grep -q '"syncType":"FULL"'; then
        log_pass "Full sync works"
    else
        log_fail "Full sync failed"
    fi

    # Incremental sync
    LAST_SYNC=$(date -u -d '1 hour ago' +%Y-%m-%dT%H:%M:%SZ 2>/dev/null || date -u +%Y-%m-%dT%H:%M:%SZ)
    RESPONSE=$(curl -s -X GET "$API_BASE/sync/products?lastSync=$LAST_SYNC" \
        -H "Authorization: Bearer $AUTH_TOKEN")

    if echo "$RESPONSE" | grep -q '"syncType":"INCREMENTAL"'; then
        log_pass "Incremental sync works"
    else
        log_info "Incremental sync returned: ${RESPONSE:0:100}"
    fi
}

test_sync_tenant_config() {
    log_info "Testing tenant config sync..."

    if [ -z "$AUTH_TOKEN" ]; then
        log_skip "No auth token"
        return
    fi

    RESPONSE=$(curl -s -X GET "$API_BASE/sync/tenant-config" \
        -H "Authorization: Bearer $AUTH_TOKEN")

    if echo "$RESPONSE" | grep -q '"success":true'; then
        log_pass "Tenant config retrieved"
    else
        log_fail "Tenant config failed"
        echo "Response: ${RESPONSE:0:200}"
    fi
}

test_sync_screens() {
    log_info "Testing screens sync..."

    if [ -z "$AUTH_TOKEN" ]; then
        log_skip "No auth token"
        return
    fi

    RESPONSE=$(curl -s -X GET "$API_BASE/sync/screens?sector=RETAIL" \
        -H "Authorization: Bearer $AUTH_TOKEN")

    if echo "$RESPONSE" | grep -q '"success":true'; then
        log_pass "Screens retrieved"
    else
        log_fail "Screens sync failed"
        echo "Response: ${RESPONSE:0:200}"
    fi
}

test_sync_status() {
    log_info "Testing sync status..."

    if [ -z "$AUTH_TOKEN" ]; then
        log_skip "No auth token"
        return
    fi

    RESPONSE=$(curl -s -X GET "$API_BASE/sync/status" \
        -H "Authorization: Bearer $AUTH_TOKEN")

    if echo "$RESPONSE" | grep -q '"status":"online"'; then
        log_pass "Sync status: online"
    else
        log_fail "Sync status failed"
    fi
}

test_sync_requires_auth() {
    log_info "Testing sync requires auth..."

    RESPONSE=$(curl -s -w "\n%{http_code}" -X GET "$API_BASE/sync/catalog")
    HTTP_CODE=$(echo "$RESPONSE" | tail -n1)

    if [ "$HTTP_CODE" = "401" ]; then
        log_pass "Sync endpoints require auth (401)"
    else
        log_fail "Expected 401, got $HTTP_CODE"
    fi
}

# ============================================================================
# Phase 3: Shift Management
# ============================================================================

test_shift_start() {
    log_section "Phase 3: Shift Management"

    if [ -z "$AUTH_TOKEN" ] || [ -z "$TERMINAL_ID" ]; then
        log_skip "No auth token or terminal ID"
        return
    fi

    # First get an operator ID (we need to query or use a known one)
    # For testing, we'll try without operatorId first

    RESPONSE=$(curl -s -X POST "$API_BASE/shifts" \
        -H "Authorization: Bearer $AUTH_TOKEN" \
        -H "Content-Type: application/json" \
        -d "{
            \"terminalId\": \"$TERMINAL_ID\",
            \"openingCash\": 100.00
        }")

    if echo "$RESPONSE" | grep -q '"success":true'; then
        SHIFT_ID=$(echo "$RESPONSE" | grep -o '"id":"[^"]*"' | head -1 | cut -d'"' -f4)
        log_pass "Shift started: $SHIFT_ID"
    else
        log_fail "Shift start failed"
        echo "Response: ${RESPONSE:0:300}"
    fi
}

test_shift_current() {
    log_info "Testing get current shift..."

    if [ -z "$AUTH_TOKEN" ]; then
        log_skip "No auth token"
        return
    fi

    RESPONSE=$(curl -s -X GET "$API_BASE/shifts/current?terminalId=$TERMINAL_ID" \
        -H "Authorization: Bearer $AUTH_TOKEN")

    if echo "$RESPONSE" | grep -q '"success":true'; then
        log_pass "Current shift retrieved"
    else
        log_info "No current shift (may be expected)"
    fi
}

# ============================================================================
# Phase 4: Transactions
# ============================================================================

test_create_transaction() {
    log_section "Phase 4: Transactions"

    if [ -z "$AUTH_TOKEN" ] || [ -z "$TERMINAL_ID" ]; then
        log_skip "No auth token or terminal ID"
        return
    fi

    # Create a simple cash sale
    RESPONSE=$(curl -s -X POST "$API_BASE/transactions" \
        -H "Authorization: Bearer $AUTH_TOKEN" \
        -H "Content-Type: application/json" \
        -d "{
            \"terminalId\": \"$TERMINAL_ID\",
            \"shiftId\": \"$SHIFT_ID\",
            \"transactionType\": \"SALE\",
            \"items\": [{
                \"productId\": \"test-product-001\",
                \"productName\": \"Test Product\",
                \"quantity\": 2,
                \"unitPrice\": 10.00,
                \"taxRate\": 15,
                \"lineTotal\": 20.00
            }],
            \"payments\": [{
                \"paymentType\": \"CASH\",
                \"amount\": 23.00
            }],
            \"subtotal\": 20.00,
            \"taxTotal\": 3.00,
            \"discountTotal\": 0,
            \"grandTotal\": 23.00
        }")

    if echo "$RESPONSE" | grep -q '"success":true'; then
        TRANSACTION_ID=$(echo "$RESPONSE" | grep -o '"id":"[^"]*"' | head -1 | cut -d'"' -f4)
        RECEIPT_NUMBER=$(echo "$RESPONSE" | grep -o '"receiptNumber":"[^"]*"' | cut -d'"' -f4)
        log_pass "Transaction created: $TRANSACTION_ID"
        echo "  Receipt: $RECEIPT_NUMBER"
    else
        log_fail "Transaction creation failed"
        echo "Response: ${RESPONSE:0:400}"
    fi
}

test_get_transaction() {
    log_info "Testing get transaction..."

    if [ -z "$AUTH_TOKEN" ] || [ -z "$TRANSACTION_ID" ]; then
        log_skip "No auth token or transaction ID"
        return
    fi

    RESPONSE=$(curl -s -X GET "$API_BASE/transactions/$TRANSACTION_ID" \
        -H "Authorization: Bearer $AUTH_TOKEN")

    if echo "$RESPONSE" | grep -q '"success":true'; then
        log_pass "Transaction retrieved"
    else
        log_fail "Get transaction failed"
    fi
}

test_get_transaction_by_receipt() {
    log_info "Testing get by receipt number..."

    if [ -z "$AUTH_TOKEN" ] || [ -z "$RECEIPT_NUMBER" ]; then
        log_skip "No auth token or receipt number"
        return
    fi

    RESPONSE=$(curl -s -X GET "$API_BASE/transactions/by-receipt/$RECEIPT_NUMBER" \
        -H "Authorization: Bearer $AUTH_TOKEN")

    if echo "$RESPONSE" | grep -q '"success":true'; then
        log_pass "Transaction found by receipt"
    else
        log_fail "Get by receipt failed"
    fi
}

# ============================================================================
# Phase 5: Offline Queue
# ============================================================================

test_offline_upload() {
    log_section "Phase 5: Offline Queue"

    if [ -z "$AUTH_TOKEN" ] || [ -z "$TERMINAL_ID" ]; then
        log_skip "No auth token or terminal ID"
        return
    fi

    LOCAL_ID="local-$(date +%s)"

    RESPONSE=$(curl -s -X POST "$API_BASE/offline/upload" \
        -H "Authorization: Bearer $AUTH_TOKEN" \
        -H "Content-Type: application/json" \
        -d "{
            \"transactions\": [{
                \"localId\": \"$LOCAL_ID\",
                \"terminalId\": \"$TERMINAL_ID\",
                \"transactionType\": \"SALE\",
                \"items\": [{
                    \"productId\": \"offline-product\",
                    \"productName\": \"Offline Test\",
                    \"quantity\": 1,
                    \"unitPrice\": 5.00,
                    \"lineTotal\": 5.00
                }],
                \"payments\": [{
                    \"paymentType\": \"CASH\",
                    \"amount\": 5.75
                }],
                \"subtotal\": 5.00,
                \"taxTotal\": 0.75,
                \"grandTotal\": 5.75,
                \"createdAt\": \"$(date -u +%Y-%m-%dT%H:%M:%SZ)\"
            }]
        }")

    if echo "$RESPONSE" | grep -q '"queued":1'; then
        log_pass "Offline transaction uploaded"
    else
        log_fail "Offline upload failed"
        echo "Response: ${RESPONSE:0:300}"
    fi
}

test_offline_queue_stats() {
    log_info "Testing queue stats..."

    if [ -z "$AUTH_TOKEN" ] || [ -z "$TERMINAL_ID" ]; then
        log_skip "No auth token or terminal ID"
        return
    fi

    RESPONSE=$(curl -s -X GET "$API_BASE/sync/queue/$TERMINAL_ID/stats" \
        -H "Authorization: Bearer $AUTH_TOKEN")

    if echo "$RESPONSE" | grep -q '"success":true'; then
        log_pass "Queue stats retrieved"
    else
        log_fail "Queue stats failed"
    fi
}

# ============================================================================
# Phase 6: Fleet Management
# ============================================================================

test_fleet_heartbeat() {
    log_section "Phase 6: Fleet Management"

    if [ -z "$AUTH_TOKEN" ] || [ -z "$TERMINAL_ID" ]; then
        log_skip "No auth token or terminal ID"
        return
    fi

    RESPONSE=$(curl -s -X POST "$API_BASE/fleet/$TERMINAL_ID/heartbeat" \
        -H "Authorization: Bearer $AUTH_TOKEN" \
        -H "Content-Type: application/json" \
        -d "{
            \"uptimeSeconds\": 3600,
            \"cpuPercent\": 25.5,
            \"memoryMb\": 512,
            \"diskFreeMb\": 1024,
            \"offlineTxnCount\": 0,
            \"appVersion\": \"1.0.0\"
        }")

    if echo "$RESPONSE" | grep -q '"success":true'; then
        log_pass "Heartbeat sent"
    else
        log_fail "Heartbeat failed"
        echo "Response: ${RESPONSE:0:200}"
    fi
}

test_fleet_status() {
    log_info "Testing fleet status..."

    if [ -z "$AUTH_TOKEN" ]; then
        log_skip "No auth token"
        return
    fi

    RESPONSE=$(curl -s -X GET "$API_BASE/fleet/status" \
        -H "Authorization: Bearer $AUTH_TOKEN")

    if echo "$RESPONSE" | grep -q '"success":true'; then
        log_pass "Fleet status retrieved"
    else
        log_fail "Fleet status failed"
    fi
}

# ============================================================================
# Phase 7: Reports
# ============================================================================

test_reports_daily() {
    log_section "Phase 7: Reports"

    if [ -z "$AUTH_TOKEN" ]; then
        log_skip "No auth token"
        return
    fi

    TODAY=$(date +%Y-%m-%d)

    RESPONSE=$(curl -s -X GET "$API_BASE/reports/daily-sales?date=$TODAY" \
        -H "Authorization: Bearer $AUTH_TOKEN")

    if echo "$RESPONSE" | grep -q '"success":true'; then
        log_pass "Daily sales report retrieved"
    else
        log_fail "Daily report failed"
        echo "Response: ${RESPONSE:0:200}"
    fi
}

# ============================================================================
# Cleanup and Summary
# ============================================================================

test_shift_end() {
    log_section "Cleanup: End Shift"

    if [ -z "$AUTH_TOKEN" ] || [ -z "$SHIFT_ID" ]; then
        log_skip "No auth token or shift ID"
        return
    fi

    RESPONSE=$(curl -s -X POST "$API_BASE/shifts/$SHIFT_ID/end" \
        -H "Authorization: Bearer $AUTH_TOKEN" \
        -H "Content-Type: application/json" \
        -d "{
            \"closingCash\": 123.00,
            \"notes\": \"E2E Test completed\"
        }")

    if echo "$RESPONSE" | grep -q '"success":true'; then
        log_pass "Shift ended"
    else
        log_fail "Shift end failed"
        echo "Response: ${RESPONSE:0:200}"
    fi
}

print_summary() {
    log_section "Test Summary"

    TOTAL=$((PASSED + FAILED + SKIPPED))

    echo ""
    echo -e "  ${GREEN}Passed:${NC}  $PASSED"
    echo -e "  ${RED}Failed:${NC}  $FAILED"
    echo -e "  ${YELLOW}Skipped:${NC} $SKIPPED"
    echo -e "  Total:   $TOTAL"
    echo ""

    if [ $FAILED -eq 0 ]; then
        echo -e "${GREEN}All tests passed!${NC}"
        exit 0
    else
        echo -e "${RED}Some tests failed!${NC}"
        exit 1
    fi
}

# ============================================================================
# Main Execution
# ============================================================================

main() {
    echo ""
    echo -e "${BLUE}╔═══════════════════════════════════════════════════════════════╗${NC}"
    echo -e "${BLUE}║       POS Terminal E2E API Test Suite                         ║${NC}"
    echo -e "${BLUE}║       Backend: $BACKEND_URL                       ║${NC}"
    echo -e "${BLUE}╚═══════════════════════════════════════════════════════════════╝${NC}"

    # Check backend
    check_backend

    # Phase 1: Terminal Management
    test_terminal_registration
    test_terminal_auth
    test_terminal_auth_invalid

    # Phase 2: Sync APIs
    test_sync_requires_auth
    test_sync_catalog
    test_sync_products
    test_sync_tenant_config
    test_sync_screens
    test_sync_status

    # Phase 3: Shift Management
    test_shift_start
    test_shift_current

    # Phase 4: Transactions
    test_create_transaction
    test_get_transaction
    test_get_transaction_by_receipt

    # Phase 5: Offline Queue
    test_offline_upload
    test_offline_queue_stats

    # Phase 6: Fleet Management
    test_fleet_heartbeat
    test_fleet_status

    # Phase 7: Reports
    test_reports_daily

    # Cleanup
    test_shift_end

    # Summary
    print_summary
}

# Run main
main "$@"
