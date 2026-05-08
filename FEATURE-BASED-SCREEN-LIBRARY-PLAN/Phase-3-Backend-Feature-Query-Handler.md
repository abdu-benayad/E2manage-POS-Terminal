# Phase 3: Backend - Feature Query Handler

**Time:** 1.5 hours
**Type:** Backend

Create query handler to fetch features filtered by tenant configuration and business sector.

---

## Pre-Flight Checklist

- [ ] Phase 1 completed (schema exists)
- [ ] Phase 2 completed (translations exist)
- [ ] POS module structure exists

---

## 1. Tests First (TDD)

**File:** `wadi-dms-api/src/modules/pos/application/queries/__tests__/get-features.handler.test.ts`

```typescript
import { GetFeaturesHandler, GetFeaturesQuery } from '../get-features.handler';
import { POS_BusinessSector } from '@prisma/client';

describe('GetFeaturesHandler', () => {
  let handler: GetFeaturesHandler;
  let mockPrisma: jest.Mocked<any>;

  beforeEach(() => {
    mockPrisma = {
      pOS_Feature: {
        findMany: jest.fn(),
      },
      pOS_TenantConfiguration: {
        findUnique: jest.fn(),
      },
    };
    handler = new GetFeaturesHandler(mockPrisma);
  });

  it('should return all core features when no config exists', async () => {
    mockPrisma.pOS_TenantConfiguration.findUnique.mockResolvedValue(null);
    mockPrisma.pOS_Feature.findMany.mockResolvedValue([
      { featureId: 'auth', isCore: true, configKey: null, screens: [] },
      { featureId: 'checkout', isCore: true, configKey: null, screens: [] },
    ]);

    const query: GetFeaturesQuery = {
      tenantId: 'tenant-123',
      businessSector: POS_BusinessSector.RETAIL,
    };

    const result = await handler.execute(query);

    expect(result.features).toHaveLength(2);
    expect(result.features.every(f => f.enabled)).toBe(true);
  });

  it('should filter optional features by tenant config', async () => {
    mockPrisma.pOS_TenantConfiguration.findUnique.mockResolvedValue({
      allowReturns: true,
      allowDrafts: false,
    });
    mockPrisma.pOS_Feature.findMany.mockResolvedValue([
      { featureId: 'returns', isCore: false, configKey: 'allowReturns', screens: [] },
      { featureId: 'drafts', isCore: false, configKey: 'allowDrafts', screens: [] },
    ]);

    const query: GetFeaturesQuery = {
      tenantId: 'tenant-123',
      businessSector: POS_BusinessSector.RETAIL,
    };

    const result = await handler.execute(query);
    const returns = result.features.find(f => f.featureId === 'returns');
    const drafts = result.features.find(f => f.featureId === 'drafts');

    expect(returns?.enabled).toBe(true);
    expect(drafts?.enabled).toBe(false);
  });

  it('should filter by business sector', async () => {
    mockPrisma.pOS_TenantConfiguration.findUnique.mockResolvedValue({});
    mockPrisma.pOS_Feature.findMany.mockResolvedValue([]);

    const query: GetFeaturesQuery = {
      tenantId: 'tenant-123',
      businessSector: POS_BusinessSector.RESTAURANT,
    };

    await handler.execute(query);

    expect(mockPrisma.pOS_Feature.findMany).toHaveBeenCalledWith(
      expect.objectContaining({
        where: {
          businessSectors: { has: POS_BusinessSector.RESTAURANT },
        },
      })
    );
  });

  it('should include version hash for caching', async () => {
    mockPrisma.pOS_TenantConfiguration.findUnique.mockResolvedValue({});
    mockPrisma.pOS_Feature.findMany.mockResolvedValue([
      { featureId: 'auth', isCore: true, screens: [], updatedAt: new Date() },
    ]);

    const query: GetFeaturesQuery = {
      tenantId: 'tenant-123',
      businessSector: POS_BusinessSector.RETAIL,
    };

    const result = await handler.execute(query);

    expect(result.version).toBeDefined();
    expect(typeof result.version).toBe('string');
  });
});
```

**Run (expect fail):**

```bash
npm test -- src/modules/pos/application/queries/__tests__/get-features.handler.test.ts
```

---

## 2. DTOs

**File:** `wadi-dms-api/src/modules/pos/application/dto/feature.dto.ts`

```typescript
import { POS_BusinessSector } from '@prisma/client';

export interface FeatureScreenDto {
  screenId: string;
  name: string;
  nameAr: string | null;
  isEntryPoint: boolean;
  nextScreen: string | null;
  displayOrder: number;
}

export interface FeatureDto {
  featureId: string;
  name: string;
  nameAr: string | null;
  description: string | null;
  configKey: string | null;
  isCore: boolean;
  enabled: boolean;
  icon: string | null;
  displayOrder: number;
  businessSectors: POS_BusinessSector[];
  screens: FeatureScreenDto[];
}

export interface FeaturesResponseDto {
  features: FeatureDto[];
  version: string;
  syncedAt: string;
}
```

---

## 3. Query Handler

**File:** `wadi-dms-api/src/modules/pos/application/queries/get-features.handler.ts`

```typescript
import * as crypto from 'crypto';
import type { PrismaClient, POS_BusinessSector } from '@prisma/client';
import type { FeatureDto, FeaturesResponseDto } from '../dto/feature.dto';

export interface GetFeaturesQuery {
  tenantId: string;
  businessSector: POS_BusinessSector;
  includeScreens?: boolean;
}

export class GetFeaturesHandler {
  constructor(private readonly prisma: PrismaClient) {}

  async execute(query: GetFeaturesQuery): Promise<FeaturesResponseDto> {
    // 1. Get tenant configuration
    const tenantConfig = await this.prisma.pOS_TenantConfiguration.findUnique({
      where: { tenantId: query.tenantId },
    });

    // 2. Get all features for this business sector
    const features = await this.prisma.pOS_Feature.findMany({
      where: {
        businessSectors: { has: query.businessSector },
      },
      include: query.includeScreens !== false ? {
        screens: {
          where: { isActive: true },
          orderBy: { displayOrder: 'asc' },
          select: {
            screenId: true,
            name: true,
            nameAr: true,
            isEntryPoint: true,
            nextScreen: true,
            displayOrder: true,
          },
        },
      } : undefined,
      orderBy: { displayOrder: 'asc' },
    });

    // 3. Map to DTOs with enabled status
    const featureDtos: FeatureDto[] = features.map(f => ({
      featureId: f.featureId,
      name: f.name,
      nameAr: f.nameAr,
      description: f.description,
      configKey: f.configKey,
      isCore: f.isCore,
      enabled: this.isFeatureEnabled(f, tenantConfig),
      icon: f.icon,
      displayOrder: f.displayOrder,
      businessSectors: f.businessSectors,
      screens: (f.screens || []).map(s => ({
        screenId: s.screenId,
        name: s.name,
        nameAr: s.nameAr,
        isEntryPoint: s.isEntryPoint,
        nextScreen: s.nextScreen,
        displayOrder: s.displayOrder,
      })),
    }));

    // 4. Calculate version hash for caching
    const version = this.calculateVersionHash(featureDtos);

    return {
      features: featureDtos,
      version,
      syncedAt: new Date().toISOString(),
    };
  }

  private isFeatureEnabled(
    feature: { isCore: boolean; configKey: string | null },
    config: Record<string, unknown> | null
  ): boolean {
    // Core features are always enabled
    if (feature.isCore) return true;

    // No config key means always enabled
    if (!feature.configKey) return true;

    // No tenant config means use defaults (enabled)
    if (!config) return true;

    // Check the config flag
    const value = config[feature.configKey];
    return value === true;
  }

  private calculateVersionHash(features: FeatureDto[]): string {
    const content = JSON.stringify(
      features.map(f => ({
        id: f.featureId,
        enabled: f.enabled,
        screens: f.screens.map(s => s.screenId),
      }))
    );
    return crypto.createHash('md5').update(content).digest('hex').substring(0, 8);
  }
}
```

---

## 4. Export Handler

**File:** `wadi-dms-api/src/modules/pos/application/queries/index.ts`

Add export:

```typescript
export * from './get-features.handler';
```

---

## 5. Verification

```bash
# Type check
cd wadi-dms-api
npm run typecheck

# Run tests (expect pass)
npm test -- src/modules/pos/application/queries/__tests__/get-features.handler.test.ts

# Verify no regressions
npm test -- src/modules/pos/
```

---

## Success Criteria

- [ ] Tests written and passing
- [ ] GetFeaturesHandler created
- [ ] DTOs defined
- [ ] Core features always enabled
- [ ] Optional features respect tenant config
- [ ] Business sector filtering works
- [ ] Version hash calculated for caching
- [ ] `npm run typecheck` passes

---

## Rollback

```bash
rm src/modules/pos/application/queries/get-features.handler.ts
rm src/modules/pos/application/dto/feature.dto.ts
rm src/modules/pos/application/queries/__tests__/get-features.handler.test.ts
```

---

## Next Phase

Read and follow **Phase-4-Backend-Auth-Handler-Update.md**
