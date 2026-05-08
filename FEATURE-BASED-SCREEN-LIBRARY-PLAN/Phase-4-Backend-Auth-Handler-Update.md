# Phase 4: Backend - Auth Handler Update

**Time:** 1 hour
**Type:** Backend

Replace hardcoded feature extraction in AuthenticateTerminalHandler with database-driven feature query.

---

## Pre-Flight Checklist

- [ ] Phase 3 completed (GetFeaturesHandler exists)
- [ ] Existing auth tests passing
- [ ] authenticate-terminal.handler.ts exists

---

## 1. Tests First (TDD)

**File:** `wadi-dms-api/src/modules/pos/application/commands/__tests__/authenticate-terminal.handler.test.ts`

Add new test cases:

```typescript
describe('AuthenticateTerminalHandler - Features', () => {
  it('should return features from database instead of hardcoded values', async () => {
    // Setup: Create terminal and tenant config
    const terminal = await createTestTerminal();
    await prisma.pOS_TenantConfiguration.create({
      data: {
        tenantId: terminal.tenantId,
        enabled: true,
        allowReturns: true,
        allowDiscounts: false,
        allowDrafts: true,
      },
    });

    // Seed features
    await prisma.pOS_Feature.createMany({
      data: [
        { featureId: 'checkout', name: 'Checkout', isCore: true },
        { featureId: 'returns', name: 'Returns', isCore: false, configKey: 'allowReturns' },
        { featureId: 'discounts', name: 'Discounts', isCore: false, configKey: 'allowDiscounts' },
      ],
    });

    const result = await handler.execute({
      hardwareId: terminal.hardwareId,
      secret: 'test-secret',
      tenantId: terminal.tenantId,
    });

    expect(result.features).toContain('CHECKOUT');
    expect(result.features).toContain('RETURNS');
    expect(result.features).not.toContain('DISCOUNTS');
  });

  it('should filter features by terminal business sector', async () => {
    const terminal = await createTestTerminal({ businessSector: 'RESTAURANT' });

    await prisma.pOS_Feature.createMany({
      data: [
        {
          featureId: 'drafts',
          name: 'Drafts',
          isCore: false,
          configKey: 'allowDrafts',
          businessSectors: ['RESTAURANT', 'FAST_FOOD'],
        },
        {
          featureId: 'returns',
          name: 'Returns',
          isCore: false,
          configKey: 'allowReturns',
          businessSectors: ['RETAIL', 'SUPERMARKET'],
        },
      ],
    });

    await prisma.pOS_TenantConfiguration.create({
      data: {
        tenantId: terminal.tenantId,
        enabled: true,
        allowReturns: true,
        allowDrafts: true,
      },
    });

    const result = await handler.execute({
      hardwareId: terminal.hardwareId,
      secret: 'test-secret',
      tenantId: terminal.tenantId,
    });

    expect(result.features).toContain('DRAFTS');
    expect(result.features).not.toContain('RETURNS'); // Not for RESTAURANT
  });
});
```

**Run (expect fail):**

```bash
npm test -- src/modules/pos/application/commands/__tests__/authenticate-terminal.handler.test.ts
```

---

## 2. Update Handler

**File:** `wadi-dms-api/src/modules/pos/application/commands/authenticate-terminal.handler.ts`

### 2.1 Add Import

```typescript
import { GetFeaturesHandler, GetFeaturesQuery } from '../queries/get-features.handler';
```

### 2.2 Add Private Method

Add before `execute()`:

```typescript
private async getEnabledFeatures(
  tenantId: string,
  businessSector: string | null
): Promise<string[]> {
  if (!this.prisma) return [];

  const handler = new GetFeaturesHandler(this.prisma);
  const query: GetFeaturesQuery = {
    tenantId,
    businessSector: (businessSector as POS_BusinessSector) || 'RETAIL',
    includeScreens: false,
  };

  const result = await handler.execute(query);
  return result.features
    .filter(f => f.enabled)
    .map(f => f.featureId.toUpperCase());
}
```

### 2.3 Replace Hardcoded Feature Building

Find this code block in `execute()`:

```typescript
// Build features list from config
const features: string[] = [];
if (tenantConfig?.allowReturns) features.push('RETURNS');
if (tenantConfig?.allowDiscounts) features.push('DISCOUNTS');
if (tenantConfig?.paymentMethods) {
  const methods = tenantConfig.paymentMethods as string[];
  if (methods.length > 1) features.push('SPLIT_PAYMENT');
}
```

Replace with:

```typescript
// Get enabled features from database
const features = await this.getEnabledFeatures(
  terminal.tenantId,
  terminal.businessSector
);

// Add SPLIT_PAYMENT if multiple payment methods configured
if (tenantConfig?.paymentMethods) {
  const methods = tenantConfig.paymentMethods as string[];
  if (methods.length > 1 && !features.includes('SPLIT_PAYMENT')) {
    features.push('SPLIT_PAYMENT');
  }
}
```

---

## 3. Add Missing Import

At the top of the file:

```typescript
import { POS_BusinessSector } from '@prisma/client';
```

---

## 4. Verification

```bash
# Type check
cd wadi-dms-api
npm run typecheck

# Run updated tests
npm test -- src/modules/pos/application/commands/__tests__/authenticate-terminal.handler.test.ts

# Run all POS tests
npm test -- src/modules/pos/

# Manual API test (if backend running)
curl -X POST http://localhost:3000/api/pos/terminal/authenticate \
  -H "Content-Type: application/json" \
  -d '{
    "hardwareId": "TEST-001",
    "secret": "test-secret",
    "tenantId": "your-tenant-id"
  }'
```

---

## Success Criteria

- [ ] Tests pass
- [ ] Features returned from database, not hardcoded
- [ ] Business sector filtering works
- [ ] SPLIT_PAYMENT still added for multi-payment configs
- [ ] Existing auth flow unchanged (backward compatible)
- [ ] `npm run typecheck` passes

---

## Rollback

Revert changes in authenticate-terminal.handler.ts to use hardcoded feature building:

```bash
git checkout -- src/modules/pos/application/commands/authenticate-terminal.handler.ts
```

---

## Next Phase

Read and follow **Phase-5-Backend-Feature-Controller.md**
