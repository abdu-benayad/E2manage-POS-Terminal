# Phase 11: Integration - E2E Tests

**Time:** 2 hours
**Type:** Integration Testing

End-to-end tests covering the full feature-based screen library workflow.

---

## Pre-Flight Checklist

- [ ] All previous phases completed
- [ ] Backend API running
- [ ] Database seeded with features
- [ ] POS terminal builds successfully
- [ ] E2E test infrastructure ready

---

## 1. Backend E2E Tests

**File:** `wadi-dms-api/src/modules/pos/__tests__/e2e/feature-library.e2e.test.ts`

```typescript
import request from 'supertest';
import { app, prisma } from '../../../../test-setup';
import { POS_BusinessSector } from '@prisma/client';

describe('Feature Library E2E', () => {
  let adminToken: string;
  let terminalToken: string;
  let tenantId: string;
  let terminalId: string;

  beforeAll(async () => {
    // Setup real test data
    const tenant = await prisma.company.create({
      data: {
        name: 'E2E Test Company',
        code: 'E2E-001',
        // ... required fields
      },
    });
    tenantId = tenant.id;

    // Create tenant config
    await prisma.pOS_TenantConfiguration.create({
      data: {
        tenantId,
        enabled: true,
        allowReturns: true,
        allowDrafts: false,
        allowReports: true,
        allowDiscounts: true,
      },
    });

    // Create terminal
    const terminal = await prisma.pOS_Terminal.create({
      data: {
        tenantId,
        terminalId: 'E2E-TERM-001',
        hardwareId: 'e2e-hw-001',
        secretHash: await bcrypt.hash('test-secret', 10),
        status: 'ACTIVE',
        businessSector: 'RETAIL',
      },
    });
    terminalId = terminal.id;

    // Get admin token
    adminToken = await getAdminToken(tenantId);

    // Authenticate terminal
    const authRes = await request(app)
      .post('/api/pos/terminal/authenticate')
      .send({
        hardwareId: 'e2e-hw-001',
        secret: 'test-secret',
        tenantId,
      });
    terminalToken = authRes.body.sessionToken;
  });

  afterAll(async () => {
    await prisma.pOS_Terminal.deleteMany({ where: { tenantId } });
    await prisma.pOS_TenantConfiguration.deleteMany({ where: { tenantId } });
    await prisma.company.delete({ where: { id: tenantId } });
  });

  describe('GET /api/pos/features', () => {
    it('should return all features for admin', async () => {
      const res = await request(app)
        .get('/api/pos/features')
        .set('Authorization', `Bearer ${adminToken}`)
        .expect(200);

      expect(res.body.features).toBeDefined();
      expect(res.body.features.length).toBeGreaterThan(0);

      // Check feature structure
      const feature = res.body.features[0];
      expect(feature).toHaveProperty('featureId');
      expect(feature).toHaveProperty('name');
      expect(feature).toHaveProperty('isCore');
      expect(feature).toHaveProperty('screens');
    });
  });

  describe('GET /api/pos/features/terminal', () => {
    it('should return enabled features for terminal', async () => {
      const res = await request(app)
        .get('/api/pos/features/terminal')
        .set('X-Terminal-Token', terminalToken)
        .expect(200);

      expect(res.body.features).toBeDefined();
      expect(res.body.version).toBeDefined();

      // Check core features are enabled
      const checkout = res.body.features.find(
        (f: any) => f.featureId === 'checkout'
      );
      expect(checkout).toBeDefined();
      expect(checkout.enabled).toBe(true);

      // Check optional features respect config
      const returns = res.body.features.find(
        (f: any) => f.featureId === 'returns'
      );
      expect(returns.enabled).toBe(true); // allowReturns = true

      const drafts = res.body.features.find(
        (f: any) => f.featureId === 'drafts'
      );
      expect(drafts.enabled).toBe(false); // allowDrafts = false
    });

    it('should support ETag caching', async () => {
      const res1 = await request(app)
        .get('/api/pos/features/terminal')
        .set('X-Terminal-Token', terminalToken)
        .expect(200);

      const etag = res1.headers.etag;
      expect(etag).toBeDefined();

      const res2 = await request(app)
        .get('/api/pos/features/terminal')
        .set('X-Terminal-Token', terminalToken)
        .set('If-None-Match', etag)
        .expect(304);
    });

    it('should filter by terminal business sector', async () => {
      const res = await request(app)
        .get('/api/pos/features/terminal')
        .set('X-Terminal-Token', terminalToken)
        .expect(200);

      // All features should include RETAIL sector
      res.body.features.forEach((f: any) => {
        expect(f.businessSectors).toContain('RETAIL');
      });
    });
  });

  describe('Feature Toggle Flow', () => {
    it('should update feature status when config changes', async () => {
      // Disable returns
      await request(app)
        .put(`/api/pos/config/${tenantId}`)
        .set('Authorization', `Bearer ${adminToken}`)
        .send({ allowReturns: false })
        .expect(200);

      // Verify returns is now disabled
      const res = await request(app)
        .get('/api/pos/features/terminal')
        .set('X-Terminal-Token', terminalToken)
        .expect(200);

      const returns = res.body.features.find(
        (f: any) => f.featureId === 'returns'
      );
      expect(returns.enabled).toBe(false);

      // Re-enable for cleanup
      await request(app)
        .put(`/api/pos/config/${tenantId}`)
        .set('Authorization', `Bearer ${adminToken}`)
        .send({ allowReturns: true })
        .expect(200);
    });
  });

  describe('Authentication Integration', () => {
    it('should include features in auth response', async () => {
      const res = await request(app)
        .post('/api/pos/terminal/authenticate')
        .send({
          hardwareId: 'e2e-hw-001',
          secret: 'test-secret',
          tenantId,
        })
        .expect(200);

      expect(res.body.features).toBeDefined();
      expect(Array.isArray(res.body.features)).toBe(true);

      // Features should be uppercase IDs
      expect(res.body.features).toContain('CHECKOUT');
      expect(res.body.features).toContain('RETURNS');
      expect(res.body.features).not.toContain('DRAFTS'); // Disabled
    });
  });
});
```

**Run:**

```bash
cd wadi-dms-api
npm run test:e2e -- feature-library.e2e.test.ts
```

---

## 2. POS Terminal E2E Tests

**File:** `e2manage-pos-terminal/tests/e2e_feature_sync.rs`

```rust
use e2manage_pos_terminal::*;
use pos_api::ApiClient;
use pos_db::Database;
use pos_services::{FeatureService, SyncService};
use std::sync::Arc;
use tokio::sync::broadcast;

/// Integration test requiring backend to be running
/// Run with: BACKEND_URL=http://localhost:3000 cargo test --test e2e_feature_sync
#[tokio::test]
#[ignore] // Requires backend
async fn test_feature_sync_from_backend() {
    let backend_url = std::env::var("BACKEND_URL")
        .unwrap_or_else(|_| "http://localhost:3000".to_string());

    // Setup
    let db = Arc::new(Database::new(":memory:").unwrap());
    let api = Arc::new(ApiClient::new(&backend_url).unwrap());

    // Authenticate terminal first
    api.authenticate("TEST-HW-001", "test-secret", "test-tenant-id")
        .await
        .expect("Terminal authentication failed");

    // Sync features
    let sync_service = SyncService::new(db.clone(), api);
    let (tx, mut rx) = broadcast::channel(16);

    sync_service.sync_features(&tx).await.expect("Feature sync failed");

    // Verify event
    let event = rx.recv().await.unwrap();
    match event {
        SyncEvent::FeaturesUpdated { count } => {
            assert!(count > 0, "Should have synced at least one feature");
        }
        _ => panic!("Expected FeaturesUpdated event"),
    }

    // Verify features stored
    let feature_service = FeatureService::new(db.clone());
    let features = feature_service.get_enabled_features().unwrap();
    assert!(!features.is_empty(), "Should have stored features");

    // Verify core features present
    let checkout_enabled = feature_service.is_feature_enabled("checkout").unwrap();
    assert!(checkout_enabled, "Checkout should be enabled (core feature)");
}

#[tokio::test]
#[ignore] // Requires backend
async fn test_feature_based_navigation() {
    let backend_url = std::env::var("BACKEND_URL")
        .unwrap_or_else(|_| "http://localhost:3000".to_string());

    // Setup and sync
    let db = Arc::new(Database::new(":memory:").unwrap());
    let api = Arc::new(ApiClient::new(&backend_url).unwrap());

    api.authenticate("TEST-HW-001", "test-secret", "test-tenant-id")
        .await
        .unwrap();

    let sync_service = SyncService::new(db.clone(), api);
    let (tx, _rx) = broadcast::channel(16);
    sync_service.sync_features(&tx).await.unwrap();

    // Test navigation
    let feature_service = FeatureService::new(db.clone());

    // Core screens should be enabled
    assert!(feature_service.is_screen_enabled("checkout").unwrap());
    assert!(feature_service.is_screen_enabled("payment-methods").unwrap());

    // Disabled feature screens should be blocked
    // (depends on tenant config)
    let drafts_enabled = feature_service.is_feature_enabled("drafts").unwrap();
    let save_draft_enabled = feature_service.is_screen_enabled("save-draft").unwrap();

    // If drafts is disabled, save-draft screen should also be disabled
    if !drafts_enabled {
        assert!(!save_draft_enabled);
    }
}

#[tokio::test]
#[ignore] // Requires backend
async fn test_etag_caching() {
    let backend_url = std::env::var("BACKEND_URL")
        .unwrap_or_else(|_| "http://localhost:3000".to_string());

    let db = Arc::new(Database::new(":memory:").unwrap());
    let api = Arc::new(ApiClient::new(&backend_url).unwrap());

    api.authenticate("TEST-HW-001", "test-secret", "test-tenant-id")
        .await
        .unwrap();

    let sync_service = SyncService::new(db.clone(), api.clone());
    let (tx, mut rx) = broadcast::channel(16);

    // First sync
    sync_service.sync_features(&tx).await.unwrap();
    let event1 = rx.recv().await.unwrap();
    assert!(matches!(event1, SyncEvent::FeaturesUpdated { .. }));

    // Second sync should return cached (304)
    sync_service.sync_features(&tx).await.unwrap();
    let event2 = rx.recv().await.unwrap();
    assert!(matches!(event2, SyncEvent::FeaturesSyncSkipped { .. }));
}
```

**Run:**

```bash
cd e2manage-pos-terminal
BACKEND_URL=http://localhost:3000 cargo test --test e2e_feature_sync -- --ignored --nocapture
```

---

## 3. E2E Test Script

**File:** `e2manage-pos-terminal/scripts/run-feature-e2e.sh`

```bash
#!/bin/bash
set -e

BACKEND_URL=${BACKEND_URL:-http://localhost:3000}

echo "=== Feature Library E2E Tests ==="
echo "Backend URL: $BACKEND_URL"
echo ""

# Check backend is running
echo "Checking backend availability..."
if ! curl -s "$BACKEND_URL/api/health" > /dev/null; then
    echo "ERROR: Backend not available at $BACKEND_URL"
    exit 1
fi

echo "Backend is running"
echo ""

# Run backend E2E tests
echo "=== Running Backend E2E Tests ==="
cd ../wadi-dms-api
npm run test:e2e -- feature-library.e2e.test.ts

echo ""

# Run POS terminal E2E tests
echo "=== Running POS Terminal E2E Tests ==="
cd ../e2manage-pos-terminal
BACKEND_URL=$BACKEND_URL cargo test --test e2e_feature_sync -- --ignored --nocapture

echo ""
echo "=== All E2E Tests Passed ==="
```

**Make executable:**

```bash
chmod +x scripts/run-feature-e2e.sh
```

---

## 4. Test Data Setup

**File:** `wadi-dms-api/prisma/seeds/pos-features-test.seed.ts`

```typescript
import { PrismaClient, POS_BusinessSector } from '@prisma/client';

const prisma = new PrismaClient();

export async function seedTestFeatures(tenantId: string) {
  // Seed features with screens for testing
  const features = [
    {
      featureId: 'checkout',
      name: 'Checkout',
      isCore: true,
      businessSectors: [
        POS_BusinessSector.RETAIL,
        POS_BusinessSector.SUPERMARKET,
      ],
      screens: [
        { screenId: 'checkout', name: 'Checkout', isEntryPoint: true },
        { screenId: 'product-search', name: 'Product Search' },
      ],
    },
    {
      featureId: 'returns',
      name: 'Returns',
      isCore: false,
      configKey: 'allowReturns',
      businessSectors: [POS_BusinessSector.RETAIL, POS_BusinessSector.SUPERMARKET],
      screens: [
        { screenId: 'return-entry', name: 'Return Entry', isEntryPoint: true, nextScreen: 'return-items' },
        { screenId: 'return-items', name: 'Return Items', nextScreen: 'refund' },
        { screenId: 'refund', name: 'Refund' },
      ],
    },
    {
      featureId: 'drafts',
      name: 'Drafts',
      isCore: false,
      configKey: 'allowDrafts',
      businessSectors: [POS_BusinessSector.RESTAURANT, POS_BusinessSector.FAST_FOOD],
      screens: [
        { screenId: 'save-draft', name: 'Save Draft', isEntryPoint: true },
        { screenId: 'recall-draft', name: 'Recall Draft' },
      ],
    },
  ];

  for (const f of features) {
    const feature = await prisma.pOS_Feature.upsert({
      where: { featureId: f.featureId },
      create: {
        featureId: f.featureId,
        name: f.name,
        isCore: f.isCore,
        configKey: f.configKey || null,
        businessSectors: f.businessSectors,
      },
      update: {},
    });

    // Link screens
    for (const s of f.screens) {
      await prisma.pOS_Screen.upsert({
        where: { screenId: s.screenId },
        create: {
          screenId: s.screenId,
          name: s.name,
          featureId: feature.id,
          isEntryPoint: s.isEntryPoint || false,
          nextScreen: s.nextScreen || null,
          definition: {},
          businessSectors: f.businessSectors,
        },
        update: {
          featureId: feature.id,
          isEntryPoint: s.isEntryPoint || false,
          nextScreen: s.nextScreen || null,
        },
      });
    }
  }

  console.log('Test features seeded');
}
```

---

## 5. Verification

```bash
# Full E2E test run
./scripts/run-feature-e2e.sh

# Or run individually:

# Backend tests
cd wadi-dms-api
npm run test:e2e -- feature-library

# POS terminal tests
cd e2manage-pos-terminal
BACKEND_URL=http://localhost:3000 cargo test --test e2e_feature_sync -- --ignored
```

---

## Success Criteria

- [ ] Backend E2E tests pass
- [ ] POS terminal E2E tests pass
- [ ] Feature sync from backend works
- [ ] ETag caching works
- [ ] Navigation respects feature status
- [ ] Config toggle changes feature enabled status
- [ ] Auth response includes features
- [ ] Business sector filtering works

---

## Rollback

```bash
rm wadi-dms-api/src/modules/pos/__tests__/e2e/feature-library.e2e.test.ts
rm e2manage-pos-terminal/tests/e2e_feature_sync.rs
rm e2manage-pos-terminal/scripts/run-feature-e2e.sh
```

---

## Completion

All phases complete. Proceed to verify the **DEPLOYMENT-STATUS.md** checklist.
