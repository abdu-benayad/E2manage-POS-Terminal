#!/bin/bash
# E2E API Test Runner for POS Terminal
#
# This script runs the comprehensive E2E API tests against the E2Manage backend.
#
# Usage:
#   ./scripts/run-e2e-tests.sh                    # Run all tests
#   ./scripts/run-e2e-tests.sh terminal           # Run terminal management tests
#   ./scripts/run-e2e-tests.sh sync               # Run sync API tests
#   ./scripts/run-e2e-tests.sh shift              # Run shift management tests
#   ./scripts/run-e2e-tests.sh transaction        # Run transaction tests
#   ./scripts/run-e2e-tests.sh return             # Run return management tests
#   ./scripts/run-e2e-tests.sh offline            # Run offline queue tests
#   ./scripts/run-e2e-tests.sh cash               # Run cash drawer tests
#   ./scripts/run-e2e-tests.sh reports            # Run reports tests
#   ./scripts/run-e2e-tests.sh fleet              # Run fleet management tests
#
# Environment Variables:
#   BACKEND_URL  - E2Manage API URL (default: http://localhost:3000)

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Configuration
BACKEND_URL="${BACKEND_URL:-http://localhost:3000}"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"

echo -e "${YELLOW}=== POS Terminal E2E API Tests ===${NC}"
echo "Backend URL: $BACKEND_URL"
echo "Project Dir: $PROJECT_DIR"
echo ""

# Check if backend is reachable
echo -n "Checking backend connectivity... "
if curl -s -o /dev/null -w "%{http_code}" "$BACKEND_URL/api/health" | grep -q "200"; then
    echo -e "${GREEN}OK${NC}"
else
    echo -e "${RED}FAILED${NC}"
    echo ""
    echo "Backend is not reachable at $BACKEND_URL"
    echo "Make sure the E2Manage API server is running."
    echo ""
    echo "You can start it with:"
    echo "  cd /home/admin/projects/WadiDMS/wadi-dms-api"
    echo "  npm run dev"
    exit 1
fi

cd "$PROJECT_DIR"

# Determine which tests to run
TEST_FILTER="${1:-}"

case "$TEST_FILTER" in
    "terminal")
        TEST_PATTERN="terminal_management"
        ;;
    "sync")
        TEST_PATTERN="sync_apis"
        ;;
    "shift")
        TEST_PATTERN="shift_management"
        ;;
    "transaction")
        TEST_PATTERN="transaction_management"
        ;;
    "return")
        TEST_PATTERN="return_management"
        ;;
    "offline")
        TEST_PATTERN="offline_queue"
        ;;
    "cash")
        TEST_PATTERN="cash_drawer"
        ;;
    "reports")
        TEST_PATTERN="reports"
        ;;
    "fleet")
        TEST_PATTERN="fleet_management"
        ;;
    "")
        TEST_PATTERN=""
        ;;
    *)
        TEST_PATTERN="$TEST_FILTER"
        ;;
esac

echo ""
echo "Running E2E tests..."
echo ""

# Build test command
TEST_CMD="cargo test --test e2e_api_tests"
if [ -n "$TEST_PATTERN" ]; then
    TEST_CMD="$TEST_CMD $TEST_PATTERN"
fi
TEST_CMD="$TEST_CMD -- --ignored --test-threads=1 --nocapture"

# Run tests
export BACKEND_URL
echo "Executing: $TEST_CMD"
echo ""
echo "================================================"
echo ""

if $TEST_CMD; then
    echo ""
    echo "================================================"
    echo -e "${GREEN}All tests passed!${NC}"
else
    echo ""
    echo "================================================"
    echo -e "${RED}Some tests failed.${NC}"
    exit 1
fi
