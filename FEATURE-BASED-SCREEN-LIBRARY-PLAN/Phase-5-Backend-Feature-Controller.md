# Phase 5: Backend - Feature Controller

**Time:** 1.5 hours
**Type:** Backend

Create REST endpoints for feature management and terminal feature sync.

---

## Pre-Flight Checklist

- [ ] Phase 4 completed
- [ ] POS controller structure exists
- [ ] Terminal auth middleware available

---

## 1. Tests First (TDD)

**File:** `wadi-dms-api/src/modules/pos/presentation/controllers/__tests__/feature.controller.test.ts`

```typescript
import request from 'supertest';
import { app } from '../../../../../app';
import { POS_BusinessSector } from '@prisma/client';

describe('FeatureController', () => {
  let authToken: string;
  let terminalToken: string;
  let tenantId: string;

  beforeAll(async () => {
    // Setup test data and get tokens
    const { token, tenant } = await setupTestTenant();
    authToken = token;
    tenantId = tenant.id;

    const terminal = await setupTestTerminal(tenantId);
    terminalToken = terminal.sessionToken;
  });

  describe('GET /api/pos/features', () => {
    it('should list all features for admin', async () => {
      const res = await request(app)
        .get('/api/pos/features')
        .set('Authorization', `Bearer ${authToken}`)
        .expect(200);

      expect(res.body.features).toBeDefined();
      expect(Array.isArray(res.body.features)).toBe(true);
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
    });

    it('should support ETag caching', async () => {
      const res1 = await request(app)
        .get('/api/pos/features/terminal')
        .set('X-Terminal-Token', terminalToken)
        .expect(200);

      const etag = res1.headers.etag;

      const res2 = await request(app)
        .get('/api/pos/features/terminal')
        .set('X-Terminal-Token', terminalToken)
        .set('If-None-Match', etag)
        .expect(304);
    });

    it('should filter by business sector from terminal', async () => {
      const res = await request(app)
        .get('/api/pos/features/terminal')
        .set('X-Terminal-Token', terminalToken)
        .expect(200);

      // All features should match terminal's business sector
      const terminal = await getTerminalFromToken(terminalToken);
      res.body.features.forEach((f: any) => {
        expect(f.businessSectors).toContain(terminal.businessSector);
      });
    });
  });

  describe('PUT /api/pos/features/:featureId', () => {
    it('should update feature metadata (admin only)', async () => {
      await request(app)
        .put('/api/pos/features/returns')
        .set('Authorization', `Bearer ${authToken}`)
        .send({
          name: 'Updated Returns',
          icon: 'new-icon',
        })
        .expect(200);

      const feature = await prisma.pOS_Feature.findUnique({
        where: { featureId: 'returns' },
      });
      expect(feature?.name).toBe('Updated Returns');
    });

    it('should reject update without admin role', async () => {
      await request(app)
        .put('/api/pos/features/returns')
        .set('X-Terminal-Token', terminalToken)
        .send({ name: 'Hacked' })
        .expect(403);
    });
  });
});
```

**Run (expect fail):**

```bash
npm test -- src/modules/pos/presentation/controllers/__tests__/feature.controller.test.ts
```

---

## 2. Controller

**File:** `wadi-dms-api/src/modules/pos/presentation/controllers/feature.controller.ts`

```typescript
import { Router, Request, Response, NextFunction } from 'express';
import type { PrismaClient, POS_BusinessSector } from '@prisma/client';
import { GetFeaturesHandler } from '../../application/queries/get-features.handler';
import { validateRequest } from '../../../../shared/middleware/validate-request';
import { z } from 'zod';

const updateFeatureSchema = z.object({
  name: z.string().max(100).optional(),
  nameAr: z.string().max(100).optional(),
  description: z.string().optional(),
  icon: z.string().max(50).optional(),
  displayOrder: z.number().int().min(0).max(1000).optional(),
});

export function createFeatureController(prisma: PrismaClient): Router {
  const router = Router();
  const handler = new GetFeaturesHandler(prisma);

  /**
   * GET /api/pos/features
   * List all features (admin only)
   */
  router.get(
    '/',
    async (req: Request, res: Response, next: NextFunction) => {
      try {
        const features = await prisma.pOS_Feature.findMany({
          include: {
            screens: {
              select: { screenId: true, name: true },
            },
          },
          orderBy: { displayOrder: 'asc' },
        });

        res.json({ features });
      } catch (error) {
        next(error);
      }
    }
  );

  /**
   * GET /api/pos/features/terminal
   * Get enabled features for terminal (with ETag caching)
   */
  router.get(
    '/terminal',
    async (req: Request, res: Response, next: NextFunction) => {
      try {
        const terminal = (req as any).terminal;
        if (!terminal) {
          return res.status(401).json({ error: 'Terminal not authenticated' });
        }

        const result = await handler.execute({
          tenantId: terminal.tenantId,
          businessSector: terminal.businessSector as POS_BusinessSector || 'RETAIL',
          includeScreens: true,
        });

        // Set ETag for caching
        const etag = `"${result.version}"`;
        res.set('ETag', etag);

        // Check If-None-Match
        const clientEtag = req.headers['if-none-match'];
        if (clientEtag === etag) {
          return res.status(304).end();
        }

        res.json(result);
      } catch (error) {
        next(error);
      }
    }
  );

  /**
   * GET /api/pos/features/:featureId
   * Get single feature
   */
  router.get(
    '/:featureId',
    async (req: Request, res: Response, next: NextFunction) => {
      try {
        const { featureId } = req.params;

        const feature = await prisma.pOS_Feature.findUnique({
          where: { featureId },
          include: {
            screens: {
              where: { isActive: true },
              orderBy: { displayOrder: 'asc' },
            },
          },
        });

        if (!feature) {
          return res.status(404).json({ error: 'Feature not found' });
        }

        res.json({ feature });
      } catch (error) {
        next(error);
      }
    }
  );

  /**
   * PUT /api/pos/features/:featureId
   * Update feature metadata (admin only)
   */
  router.put(
    '/:featureId',
    validateRequest(updateFeatureSchema),
    async (req: Request, res: Response, next: NextFunction) => {
      try {
        const { featureId } = req.params;
        const updates = req.body;

        const feature = await prisma.pOS_Feature.update({
          where: { featureId },
          data: updates,
        });

        res.json({ feature });
      } catch (error) {
        next(error);
      }
    }
  );

  return router;
}
```

---

## 3. Route Registration

**File:** `wadi-dms-api/src/modules/pos/presentation/routes/feature.routes.ts`

```typescript
import { Router } from 'express';
import type { PrismaClient } from '@prisma/client';
import { createFeatureController } from '../controllers/feature.controller';
import { terminalAuthMiddleware } from '../../infrastructure/middleware/terminal-auth.middleware';
import { authMiddleware } from '../../../../shared/middleware/auth.middleware';
import { roleGuard } from '../../../../shared/middleware/role-guard.middleware';

export function createFeatureRoutes(prisma: PrismaClient): Router {
  const router = Router();
  const controller = createFeatureController(prisma);

  // Terminal endpoint (terminal auth)
  router.get('/terminal', terminalAuthMiddleware(prisma), controller);

  // Admin endpoints (user auth + role)
  router.use(authMiddleware, roleGuard(['ADMIN', 'POS_ADMIN']));
  router.get('/', controller);
  router.get('/:featureId', controller);
  router.put('/:featureId', controller);

  return router;
}
```

---

## 4. Register in POS Module

**File:** `wadi-dms-api/src/modules/pos/pos.module.ts`

Add to module setup:

```typescript
import { createFeatureRoutes } from './presentation/routes/feature.routes';

// In module initialization:
const featureRoutes = createFeatureRoutes(prisma);
router.use('/features', featureRoutes);
```

---

## 5. Verification

```bash
# Type check
cd wadi-dms-api
npm run typecheck

# Run tests
npm test -- src/modules/pos/presentation/controllers/__tests__/feature.controller.test.ts

# Manual API tests
# List features (admin)
curl -X GET http://localhost:3000/api/pos/features \
  -H "Authorization: Bearer $ADMIN_TOKEN"

# Get terminal features
curl -X GET http://localhost:3000/api/pos/features/terminal \
  -H "X-Terminal-Token: $TERMINAL_TOKEN"

# Test ETag caching
ETAG=$(curl -sI http://localhost:3000/api/pos/features/terminal \
  -H "X-Terminal-Token: $TERMINAL_TOKEN" | grep -i etag | cut -d' ' -f2)
curl -X GET http://localhost:3000/api/pos/features/terminal \
  -H "X-Terminal-Token: $TERMINAL_TOKEN" \
  -H "If-None-Match: $ETAG" -w "%{http_code}"
# Should return 304
```

---

## Success Criteria

- [ ] Tests pass
- [ ] GET /api/pos/features returns all features
- [ ] GET /api/pos/features/terminal returns enabled features
- [ ] ETag caching works (304 on unchanged)
- [ ] PUT /api/pos/features/:id updates feature
- [ ] Terminal auth required for /terminal endpoint
- [ ] Admin auth required for other endpoints
- [ ] `npm run typecheck` passes

---

## Rollback

```bash
rm src/modules/pos/presentation/controllers/feature.controller.ts
rm src/modules/pos/presentation/routes/feature.routes.ts
# Revert changes in pos.module.ts
```

---

## Next Phase

Read and follow **Phase-6-POS-DB-Schema-Features.md**
