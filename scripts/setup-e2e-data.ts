/**
 * E2E Test Data Setup Script
 *
 * Creates test data for POS Terminal E2E API tests.
 * Run this before running the E2E tests.
 *
 * Usage:
 *   cd wadi-dms-api && npx tsx ../e2manage-pos-terminal/scripts/setup-e2e-data.ts
 *
 * Output:
 *   Exports environment variables for the E2E test script
 */

import { PrismaClient } from '@prisma/client';
import * as bcrypt from 'bcrypt';
import * as jwt from 'jsonwebtoken';
import { randomUUID } from 'crypto';

const prisma = new PrismaClient();

// Configuration
const JWT_SECRET = process.env.JWT_SECRET || 'test-jwt-secret-key-for-e2e-tests-minimum-32-characters-required';
const TIMESTAMP = Date.now();
const UNIQUE_SUFFIX = TIMESTAMP.toString().slice(-6);

// Test data IDs
const TEST_COMPANY_ID = randomUUID();
const TEST_USER_ID = randomUUID();
const TEST_TERMINAL_ID = randomUUID();
const TEST_OPERATOR_ID = randomUUID();

// Test credentials
const TEST_HARDWARE_ID = `HW-E2E-${UNIQUE_SUFFIX}`;
const TEST_TERMINAL_SECRET = `e2e-terminal-secret-${UNIQUE_SUFFIX}`;
const TEST_USER_EMAIL = `e2e-admin-${UNIQUE_SUFFIX}@test.com`;
const TEST_USER_PASSWORD = 'test123456';

async function setupTestData() {
  console.log('🔧 Setting up E2E test data...\n');

  try {
    // 1. Create test company
    console.log('1. Creating test company...');
    const company = await prisma.company.create({
      data: {
        id: TEST_COMPANY_ID,
        code: `E2E-${UNIQUE_SUFFIX}`,
        name: `E2E Test Company ${UNIQUE_SUFFIX}`,
        nameAr: 'شركة اختبار',
        type: 'DISTRIBUTOR',
        taxId: `TAX-${UNIQUE_SUFFIX}`,
        isActive: true,
      },
    });
    console.log(`   ✓ Company created: ${company.code}`);

    // 2. Create POS tenant configuration
    console.log('2. Creating POS tenant configuration...');
    await prisma.pOS_TenantConfiguration.create({
      data: {
        tenantId: TEST_COMPANY_ID,
        enabled: true,
        baseCurrency: 'LYD',
        requireShift: false,
        taxRate: 0.15,
      },
    });
    console.log('   ✓ POS tenant config created');

    // 3. Create test user with password
    console.log('3. Creating test admin user...');
    const passwordHash = await bcrypt.hash(TEST_USER_PASSWORD, 10);
    const user = await prisma.user.create({
      data: {
        id: TEST_USER_ID,
        email: TEST_USER_EMAIL,
        password: passwordHash,
        firstName: 'E2E',
        lastName: 'Admin',
        companyId: TEST_COMPANY_ID,
        isActive: true,
      },
    });
    console.log(`   ✓ User created: ${user.email}`);

    // 4. Create role and permissions for user
    console.log('4. Setting up RBAC permissions...');
    const role = await prisma.role.upsert({
      where: { name_companyId: { name: 'POS_ADMIN', companyId: TEST_COMPANY_ID } },
      update: {},
      create: {
        name: 'POS_ADMIN',
        description: 'POS Administrator',
        companyId: TEST_COMPANY_ID,
        isSystem: false,
      },
    });

    // Create POS permissions
    const posPermissions = [
      'POS_CREATE', 'POS_READ', 'POS_UPDATE', 'POS_DELETE',
      'POS_VOID', 'POS_MANAGE', 'POS_REPORT', 'POS_REFUND'
    ];

    for (const permName of posPermissions) {
      const permission = await prisma.permission.upsert({
        where: { name: permName },
        update: {},
        create: {
          name: permName,
          description: `POS permission: ${permName}`,
          module: 'POS',
        },
      });

      await prisma.rolePermission.upsert({
        where: { roleId_permissionId: { roleId: role.id, permissionId: permission.id } },
        update: {},
        create: {
          roleId: role.id,
          permissionId: permission.id,
        },
      });
    }

    // Assign role to user
    await prisma.userRole.upsert({
      where: { userId_roleId: { userId: TEST_USER_ID, roleId: role.id } },
      update: {},
      create: {
        userId: TEST_USER_ID,
        roleId: role.id,
      },
    });
    console.log('   ✓ RBAC permissions set up');

    // 5. Create test terminal with secret
    console.log('5. Creating test terminal...');
    const secretHash = await bcrypt.hash(TEST_TERMINAL_SECRET, 10);
    const terminal = await prisma.pOS_Terminal.create({
      data: {
        id: TEST_TERMINAL_ID,
        tenantId: TEST_COMPANY_ID,
        terminalId: `TERM-E2E-${UNIQUE_SUFFIX}`,
        hardwareId: TEST_HARDWARE_ID,
        terminalName: `E2E Test Terminal ${UNIQUE_SUFFIX}`,
        terminalType: 'STANDARD',
        status: 'ACTIVE',
        secretHash: secretHash,
        isOnline: false,
        businessSector: 'RETAIL',
      },
    });
    console.log(`   ✓ Terminal created: ${terminal.terminalId}`);

    // 6. Create test operator (cashier)
    console.log('6. Creating test operator...');
    const pinHash = await bcrypt.hash('1234', 10);
    await prisma.pOS_Operator.create({
      data: {
        id: TEST_OPERATOR_ID,
        tenantId: TEST_COMPANY_ID,
        code: `OP-E2E-${UNIQUE_SUFFIX}`,
        name: 'E2E Test Cashier',
        nameAr: 'كاشير اختبار',
        pinHash: pinHash,
        role: 'CASHIER',
        isActive: true,
      },
    });
    console.log('   ✓ Operator created');

    // 7. Generate JWT token for user
    console.log('7. Generating auth token...');
    const authToken = jwt.sign(
      {
        userId: TEST_USER_ID,
        email: TEST_USER_EMAIL,
        companyId: TEST_COMPANY_ID,
        permissions: posPermissions,
      },
      JWT_SECRET,
      { expiresIn: '24h' }
    );

    // Output credentials
    console.log('\n' + '═'.repeat(60));
    console.log('✅ E2E Test Data Setup Complete!');
    console.log('═'.repeat(60));
    console.log('\nExport these environment variables before running tests:\n');
    console.log(`export TEST_AUTH_TOKEN="${authToken}"`);
    console.log(`export TEST_HARDWARE_ID="${TEST_HARDWARE_ID}"`);
    console.log(`export TEST_TERMINAL_SECRET="${TEST_TERMINAL_SECRET}"`);
    console.log(`export TEST_TERMINAL_ID="${terminal.id}"`);
    console.log(`export TEST_COMPANY_ID="${TEST_COMPANY_ID}"`);
    console.log(`export TEST_OPERATOR_ID="${TEST_OPERATOR_ID}"`);
    console.log('\nOr run the E2E tests with:\n');
    console.log(`TEST_AUTH_TOKEN="${authToken}" \\`);
    console.log(`TEST_HARDWARE_ID="${TEST_HARDWARE_ID}" \\`);
    console.log(`TEST_TERMINAL_SECRET="${TEST_TERMINAL_SECRET}" \\`);
    console.log(`TEST_TERMINAL_ID="${terminal.id}" \\`);
    console.log(`./scripts/test-api-e2e.sh`);
    console.log('\n' + '═'.repeat(60));

    // Save to file for easy sourcing
    const envContent = `# E2E Test Environment Variables
# Generated: ${new Date().toISOString()}
export TEST_AUTH_TOKEN="${authToken}"
export TEST_HARDWARE_ID="${TEST_HARDWARE_ID}"
export TEST_TERMINAL_SECRET="${TEST_TERMINAL_SECRET}"
export TEST_TERMINAL_ID="${terminal.id}"
export TEST_COMPANY_ID="${TEST_COMPANY_ID}"
export TEST_OPERATOR_ID="${TEST_OPERATOR_ID}"
export TEST_USER_EMAIL="${TEST_USER_EMAIL}"
`;

    const fs = await import('fs');
    fs.writeFileSync('/tmp/e2e-test-env.sh', envContent);
    console.log('Environment saved to /tmp/e2e-test-env.sh');
    console.log('Source it with: source /tmp/e2e-test-env.sh');

  } catch (error) {
    console.error('❌ Setup failed:', error);
    throw error;
  } finally {
    await prisma.$disconnect();
  }
}

// Cleanup function
async function cleanupTestData() {
  console.log('🧹 Cleaning up old test data...');

  try {
    // Delete old E2E test data (by pattern matching)
    await prisma.pOS_Payment.deleteMany({
      where: { tenantId: { in: await getE2ECompanyIds() } }
    });
    await prisma.pOS_TransactionItem.deleteMany({
      where: { tenantId: { in: await getE2ECompanyIds() } }
    });
    await prisma.pOS_Return.deleteMany({
      where: { tenantId: { in: await getE2ECompanyIds() } }
    });
    await prisma.pOS_Transaction.deleteMany({
      where: { tenantId: { in: await getE2ECompanyIds() } }
    });
    await prisma.pOS_CashDrawerEvent.deleteMany({
      where: { tenantId: { in: await getE2ECompanyIds() } }
    });
    await prisma.pOS_OfflineQueue.deleteMany({
      where: { tenantId: { in: await getE2ECompanyIds() } }
    });

    // Clear shift references
    await prisma.pOS_Terminal.updateMany({
      where: { tenantId: { in: await getE2ECompanyIds() } },
      data: { currentShiftId: null }
    });

    await prisma.pOS_Shift.deleteMany({
      where: { tenantId: { in: await getE2ECompanyIds() } }
    });
    await prisma.pOS_Terminal.deleteMany({
      where: { tenantId: { in: await getE2ECompanyIds() } }
    });
    await prisma.pOS_Operator.deleteMany({
      where: { tenantId: { in: await getE2ECompanyIds() } }
    });
    await prisma.pOS_TenantConfiguration.deleteMany({
      where: { tenantId: { in: await getE2ECompanyIds() } }
    });

    console.log('   ✓ Cleanup complete');
  } catch (error) {
    console.log('   ⚠ Cleanup warning:', (error as Error).message);
  }
}

async function getE2ECompanyIds(): Promise<string[]> {
  const companies = await prisma.company.findMany({
    where: { code: { startsWith: 'E2E-' } },
    select: { id: true }
  });
  return companies.map(c => c.id);
}

// Run setup
async function main() {
  const args = process.argv.slice(2);

  if (args.includes('--cleanup')) {
    await cleanupTestData();
  } else {
    // Cleanup old data first
    await cleanupTestData();
    // Then create new data
    await setupTestData();
  }
}

main().catch(console.error);
