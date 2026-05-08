#!/bin/bash
# Feature Library E2E Test Script
#
# This script runs the end-to-end tests for the feature library feature
# across both the backend (wadi-dms-api) and POS terminal (e2manage-pos-terminal).
#
# Prerequisites:
# - Backend server running at $BACKEND_URL
# - Test terminal registered and seeded with features
# - Database seeded with test features
#
# Usage:
#   ./scripts/run-feature-e2e.sh
#   BACKEND_URL=http://custom:3000 ./scripts/run-feature-e2e.sh
#
# Environment Variables:
#   BACKEND_URL          - Backend API URL (default: http://localhost:3000)
#   TEST_HARDWARE_ID     - Test terminal hardware ID (default: E2E-TEST-HW-001)
#   TEST_TERMINAL_SECRET - Test terminal secret (default: e2e-test-secret-12345678)
#   SKIP_BACKEND         - Set to "1" to skip backend tests
#   SKIP_TERMINAL        - Set to "1" to skip terminal tests

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Configuration
BACKEND_URL=${BACKEND_URL:-http://localhost:3000}
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
BACKEND_ROOT="$(cd "$PROJECT_ROOT/../wadi-dms-api" && pwd)"

echo -e "${BLUE}=== Feature Library E2E Tests ===${NC}"
echo "Backend URL: $BACKEND_URL"
echo "Project Root: $PROJECT_ROOT"
echo ""

# Check backend availability
echo -e "${YELLOW}Checking backend availability...${NC}"
HEALTH_STATUS=$(curl -s -o /dev/null -w "%{http_code}" "$BACKEND_URL/api/health" 2>/dev/null || echo "000")

if [ "$HEALTH_STATUS" != "200" ]; then
    echo -e "${RED}ERROR: Backend not available at $BACKEND_URL (status: $HEALTH_STATUS)${NC}"
    echo "Please ensure the backend is running before running E2E tests."
    exit 1
fi

echo -e "${GREEN}✓ Backend is running${NC}"
echo ""

# Run backend E2E tests
if [ "$SKIP_BACKEND" != "1" ]; then
    echo -e "${BLUE}=== Running Backend E2E Tests ===${NC}"
    cd "$BACKEND_ROOT"

    if [ -f "package.json" ]; then
        echo "Running: npm run test:e2e -- 14-feature-library.e2e.test.ts"
        npm run test:e2e -- 14-feature-library.e2e.test.ts || {
            echo -e "${RED}✗ Backend E2E tests failed${NC}"
            exit 1
        }
        echo -e "${GREEN}✓ Backend E2E tests passed${NC}"
    else
        echo -e "${YELLOW}⚠ package.json not found, skipping backend tests${NC}"
    fi
    echo ""
else
    echo -e "${YELLOW}Skipping backend tests (SKIP_BACKEND=1)${NC}"
    echo ""
fi

# Run POS terminal E2E tests
if [ "$SKIP_TERMINAL" != "1" ]; then
    echo -e "${BLUE}=== Running POS Terminal E2E Tests ===${NC}"
    cd "$PROJECT_ROOT"

    # Set environment variables for tests
    export BACKEND_URL
    export TEST_HARDWARE_ID=${TEST_HARDWARE_ID:-E2E-TEST-HW-001}
    export TEST_TERMINAL_SECRET=${TEST_TERMINAL_SECRET:-e2e-test-secret-12345678}

    echo "Using:"
    echo "  BACKEND_URL: $BACKEND_URL"
    echo "  TEST_HARDWARE_ID: $TEST_HARDWARE_ID"
    echo ""

    echo "Running: cargo test --test e2e_feature_sync -- --ignored --nocapture"
    cargo test --test e2e_feature_sync -- --ignored --nocapture || {
        echo -e "${RED}✗ POS Terminal E2E tests failed${NC}"
        exit 1
    }
    echo -e "${GREEN}✓ POS Terminal E2E tests passed${NC}"
    echo ""
else
    echo -e "${YELLOW}Skipping terminal tests (SKIP_TERMINAL=1)${NC}"
    echo ""
fi

echo -e "${GREEN}=== All Feature Library E2E Tests Passed ===${NC}"
