#!/bin/bash
# Setup Test Environment for POS Terminal E2E Tests
#
# This script creates test data in the E2Manage backend database
# for running the POS terminal E2E tests.

set -e

# Colors
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m'

DB_URL="postgresql://wadi_user:wadi_password123@localhost:5432/wadi_dms"

# Use existing WADI company ID
COMPANY_ID="56e9f77b-aa31-463b-9353-1aef3df86d4e"

echo -e "${YELLOW}=== Setting up POS Terminal E2E Test Environment ===${NC}"
echo "Using company: WADI Distribution Co. (${COMPANY_ID})"

# Create test data
PGPASSWORD=wadi_password123 psql -h localhost -U wadi_user -d wadi_dms << 'EOF'
-- Use existing WADI company (56e9f77b-aa31-463b-9353-1aef3df86d4e)

-- Create POS Admin role for WADI company with proper RBAC permissions
INSERT INTO roles (id, company_id, name, description, is_system, is_active, created_at, updated_at)
VALUES (
  'e2e00000-0000-0000-0000-000000000001',
  '56e9f77b-aa31-463b-9353-1aef3df86d4e',
  'POS E2E Admin',
  'Role for POS E2E testing with full POS permissions',
  false,
  true,
  NOW(),
  NOW()
)
ON CONFLICT (company_id, name) DO UPDATE SET
  is_active = true,
  updated_at = NOW();

-- Grant POS permissions to the role
INSERT INTO role_permissions (role_id, permission_id, created_at)
SELECT
  'e2e00000-0000-0000-0000-000000000001'::uuid,
  p.id,
  NOW()
FROM permissions p
WHERE p.code IN ('POS_MANAGE', 'POS_READ', 'POS_CREATE', 'POS_UPDATE', 'POS_DELETE', 'POS_VOID', 'POS_REFUND', 'POS_REPORT')
ON CONFLICT (role_id, permission_id) DO NOTHING;

-- Create test user with bcrypt hash for 'TestPass123!'
-- $2b$10$XRLqo0LKfJ2c3f5X8XJ44OaocxLTxx7XUS4kWZXKO/MP7cIhSmQdi
INSERT INTO users (
  id, username, email, password_hash, company_id,
  first_name, last_name, role, permissions, is_active,
  email_verified, created_at, updated_at
)
VALUES (
  'e2e00000-0000-0000-0000-000000000002',
  'pos-e2e-admin',
  'pos-e2e-admin@test.com',
  '$2b$10$XRLqo0LKfJ2c3f5X8XJ44OaocxLTxx7XUS4kWZXKO/MP7cIhSmQdi',
  '56e9f77b-aa31-463b-9353-1aef3df86d4e',
  'POS',
  'Admin',
  'ADMIN',
  '["POS_MANAGE", "POS_READ", "POS_TERMINAL_MANAGE"]'::jsonb,
  true,
  true,
  NOW(),
  NOW()
)
ON CONFLICT (email) DO UPDATE SET
  password_hash = EXCLUDED.password_hash,
  permissions = EXCLUDED.permissions,
  is_active = true,
  email_verified = true,
  updated_at = NOW();

-- Get the user ID (may have been created earlier with different ID)
-- and assign the role
INSERT INTO user_roles (user_id, role_id, created_at)
SELECT
  u.id,
  'e2e00000-0000-0000-0000-000000000001'::uuid,
  NOW()
FROM users u
WHERE u.email = 'pos-e2e-admin@test.com'
ON CONFLICT (user_id, role_id) DO NOTHING;

-- Create POS tenant configuration for WADI company
INSERT INTO pos_tenant_configurations (
  id, tenant_id, enabled,
  offline_mode, conflict_policy, sync_strategy,
  sync_interval, full_sync_on_start, max_offline_hours,
  payment_methods, receipt_format, email_receipts, sms_receipts,
  print_copies, receipt_footer,
  real_time_stock, allow_negative_stock, stock_sync_mode,
  low_stock_warning, low_stock_threshold,
  allow_returns, return_window, require_receipt, require_manager,
  allow_discounts, max_discount_percent, require_manager_override,
  require_shift, force_cash_count, audit_cash_drawer, camera_snapshot,
  base_currency, allow_multi_currency, tax_enabled, tax_rate, tax_inclusive,
  created_at, updated_at
)
VALUES (
  uuid_generate_v4(),
  '56e9f77b-aa31-463b-9353-1aef3df86d4e',
  true,
  'OPTIMISTIC', 'SERVER_WINS', 'INCREMENTAL',
  5, true, 24,
  '["cash", "card"]'::jsonb, 'THERMAL_80MM', false, false,
  1, 'Thank you for shopping with us!',
  false, false, 'CACHED',
  true, 10,
  true, 30, true, false,
  true, 20.00, true,
  true, true, true, false,
  'LYD', false, true, 0.15, false,
  NOW(), NOW()
)
ON CONFLICT (tenant_id) DO UPDATE SET
  enabled = true,
  updated_at = NOW();

-- Verify setup
SELECT 'User created: ' || username as result FROM users WHERE username = 'pos-e2e-admin';
SELECT 'POS config enabled: ' || enabled::text as result FROM pos_tenant_configurations WHERE tenant_id = '56e9f77b-aa31-463b-9353-1aef3df86d4e';
SELECT 'Products available: ' || COUNT(*)::text as result FROM products WHERE company_id = '56e9f77b-aa31-463b-9353-1aef3df86d4e';
EOF

echo ""
echo -e "${GREEN}✓ Test environment created${NC}"
echo ""
echo "Test credentials:"
echo "  Username: pos-e2e-admin"
echo "  Password: TestPass123!"
echo "  Company:  Wadi Distribution Co. (WADI)"
echo "  Company ID: ${COMPANY_ID}"
echo ""
echo "Run tests with:"
echo "  ./scripts/run-e2e-tests.sh"
