-- E2E Test Data Setup for POS Terminal API Tests
-- Run this to create test data in the database

-- Variables (update these if needed)
-- Test Company ID: Use an existing company or create one
-- Test User: Create or use existing

BEGIN;

-- 1. Create a test company (if not exists)
INSERT INTO companies (id, name, name_ar, code, type, tax_id, is_active)
VALUES (
    'e2e-test-company-001'::uuid,
    'E2E Test Company',
    'شركة اختبار',
    'E2E-001',
    'DISTRIBUTOR',
    'TAX123456',
    true
)
ON CONFLICT (id) DO NOTHING;

-- 2. Create POS tenant configuration
INSERT INTO pos_tenant_configurations (
    id,
    tenant_id,
    business_name,
    default_locale,
    default_currency,
    tax_inclusive_pricing,
    default_tax_rate,
    receipt_header,
    receipt_footer,
    allow_negative_stock,
    require_customer
)
VALUES (
    'e2e-test-config-001'::uuid,
    'e2e-test-company-001'::uuid,
    'E2E Test Store',
    'ar',
    'LYD',
    false,
    15.00,
    '{"lines": ["E2E Test Store", "123 Test Street"]}',
    '{"lines": ["Thank you for shopping!", "شكراً لتسوقكم"]}',
    false,
    false
)
ON CONFLICT (tenant_id) DO NOTHING;

-- 3. Create test terminal with secret
-- Secret: 'e2e-test-secret-key-12345' (bcrypt hash)
INSERT INTO pos_terminals (
    id,
    terminal_id,
    tenant_id,
    hardware_id,
    terminal_name,
    terminal_type,
    capabilities,
    status,
    is_online,
    business_sector,
    secret_hash
)
VALUES (
    'e2e-test-terminal-001'::uuid,
    'TERM-E2E-001',
    'e2e-test-company-001'::uuid,
    'HW-E2E-TEST-001',
    'E2E Test Terminal',
    'STANDARD',
    '["CASH", "CARD", "OFFLINE"]'::jsonb,
    'ACTIVE',
    false,
    'RETAIL',
    '$2b$10$xH8xJ8qK9L0M1N2O3P4Q5.R6S7T8U9V0W1X2Y3Z4A5B6C7D8E9F0G' -- placeholder hash
)
ON CONFLICT (hardware_id) DO UPDATE SET
    terminal_name = 'E2E Test Terminal',
    status = 'ACTIVE',
    secret_hash = '$2b$10$xH8xJ8qK9L0M1N2O3P4Q5.R6S7T8U9V0W1X2Y3Z4A5B6C7D8E9F0G';

-- 4. Create test user (for admin operations)
INSERT INTO users (
    id,
    email,
    password,
    first_name,
    last_name,
    company_id,
    is_active
)
VALUES (
    'e2e-test-user-001'::uuid,
    'e2e-test@example.com',
    '$2b$10$xH8xJ8qK9L0M1N2O3P4Q5.R6S7T8U9V0W1X2Y3Z4A5B6C7D8E9F0G', -- test123
    'E2E',
    'Tester',
    'e2e-test-company-001'::uuid,
    true
)
ON CONFLICT (email) DO NOTHING;

-- 5. Create test products
INSERT INTO products (
    id,
    sku,
    name,
    name_ar,
    company_id,
    price,
    cost,
    is_active,
    track_inventory
)
VALUES
    ('e2e-prod-001'::uuid, 'E2E-PROD-001', 'E2E Test Product 1', 'منتج اختبار 1', 'e2e-test-company-001'::uuid, 10.00, 5.00, true, false),
    ('e2e-prod-002'::uuid, 'E2E-PROD-002', 'E2E Test Product 2', 'منتج اختبار 2', 'e2e-test-company-001'::uuid, 25.50, 15.00, true, false),
    ('e2e-prod-003'::uuid, 'E2E-PROD-003', 'E2E Test Product 3', 'منتج اختبار 3', 'e2e-test-company-001'::uuid, 100.00, 75.00, true, false)
ON CONFLICT (sku) DO NOTHING;

-- 6. Create test operator (cashier)
INSERT INTO pos_operators (
    id,
    tenant_id,
    code,
    name,
    name_ar,
    pin_hash,
    role,
    is_active
)
VALUES (
    'e2e-operator-001'::uuid,
    'e2e-test-company-001'::uuid,
    'E2E-OP-001',
    'E2E Test Cashier',
    'كاشير اختبار',
    '$2b$10$xH8xJ8qK9L0M1N2O3P4Q5.R6S7T8U9V0W1X2Y3Z4A5B6C7D8E9F0G', -- PIN: 1234
    'CASHIER',
    true
)
ON CONFLICT DO NOTHING;

COMMIT;

-- Print results
SELECT 'Test data setup complete!' as status;
SELECT 'Terminal Hardware ID: HW-E2E-TEST-001' as terminal_info;
SELECT 'Terminal Secret: e2e-test-secret-key-12345' as secret_info;
SELECT 'User Email: e2e-test@example.com' as user_info;
