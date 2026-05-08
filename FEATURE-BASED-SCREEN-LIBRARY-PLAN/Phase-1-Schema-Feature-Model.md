# Phase 1: Schema - Feature Model

**Time:** 1.5 hours
**Type:** Backend (Schema)

Add `POS_Feature` model to define features and their screens, modify `POS_Screen` to link to features, and add missing config flags.

---

## Pre-Flight Checklist

- [ ] Database access available
- [ ] Prisma CLI installed
- [ ] No pending migrations

---

## 1. Schema Changes

**File:** `wadi-dms-api/prisma/pos.prisma`

### 1.1 Add POS_Feature Model

Add after existing enums section:

```prisma
// ===================
// FEATURE LIBRARY
// ===================

model POS_Feature {
  id              String   @id @default(dbgenerated("uuid_generate_v4()")) @db.Uuid
  featureId       String   @unique @map("feature_id") @db.VarChar(50)
  name            String   @db.VarChar(100)
  nameAr          String?  @map("name_ar") @db.VarChar(100)
  description     String?

  // Link to POS_TenantConfiguration field name (null = always enabled)
  configKey       String?  @map("config_key") @db.VarChar(50)

  // Core features cannot be disabled
  isCore          Boolean  @default(false) @map("is_core")

  // Business sector targeting
  businessSectors POS_BusinessSector[] @map("business_sectors")

  // Display
  icon            String?  @db.VarChar(50)
  displayOrder    Int      @default(100) @map("display_order")

  // Screens belonging to this feature
  screens         POS_Screen[]

  createdAt       DateTime @default(now()) @map("created_at") @db.Timestamptz(6)
  updatedAt       DateTime @default(now()) @updatedAt @map("updated_at") @db.Timestamptz(6)

  @@index([featureId])
  @@index([isCore])
  @@map("pos_features")
}
```

### 1.2 Modify POS_Screen Model

Add feature relationship to existing `POS_Screen` model:

```prisma
model POS_Screen {
  // ... existing fields ...

  // NEW: Link to feature
  featureId    String?      @map("feature_id") @db.Uuid
  feature      POS_Feature? @relation(fields: [featureId], references: [id])

  // NEW: Navigation within feature
  isEntryPoint Boolean      @default(false) @map("is_entry_point")
  nextScreen   String?      @map("next_screen") @db.VarChar(50)

  // ... rest of existing fields ...
}
```

### 1.3 Add Missing Config Flags

Add to existing `POS_TenantConfiguration` model:

```prisma
model POS_TenantConfiguration {
  // ... existing fields ...

  // NEW: Additional feature flags
  allowDrafts    Boolean @default(true) @map("allow_drafts")
  allowReports   Boolean @default(true) @map("allow_reports")

  // ... rest of existing fields ...
}
```

---

## 2. Run Migration

```bash
cd wadi-dms-api
npx prisma migrate dev --name add_pos_feature_model
npx prisma generate
```

---

## 3. Seed Data

**File:** `wadi-dms-api/prisma/seeds/pos-features.seed.ts`

```typescript
import { PrismaClient, POS_BusinessSector } from '@prisma/client';

const prisma = new PrismaClient();

const features = [
  {
    featureId: 'auth',
    name: 'Authentication',
    nameAr: 'المصادقة',
    isCore: true,
    configKey: null,
    icon: 'key',
    displayOrder: 10,
    businessSectors: [
      POS_BusinessSector.SUPERMARKET,
      POS_BusinessSector.RETAIL,
      POS_BusinessSector.RESTAURANT,
      POS_BusinessSector.FAST_FOOD,
      POS_BusinessSector.DISTRIBUTOR,
    ],
  },
  {
    featureId: 'checkout',
    name: 'Checkout',
    nameAr: 'نقطة البيع',
    isCore: true,
    configKey: null,
    icon: 'shopping-cart',
    displayOrder: 20,
    businessSectors: [
      POS_BusinessSector.SUPERMARKET,
      POS_BusinessSector.RETAIL,
      POS_BusinessSector.RESTAURANT,
      POS_BusinessSector.FAST_FOOD,
      POS_BusinessSector.DISTRIBUTOR,
    ],
  },
  {
    featureId: 'payment',
    name: 'Payments',
    nameAr: 'المدفوعات',
    isCore: true,
    configKey: null,
    icon: 'credit-card',
    displayOrder: 30,
    businessSectors: [
      POS_BusinessSector.SUPERMARKET,
      POS_BusinessSector.RETAIL,
      POS_BusinessSector.RESTAURANT,
      POS_BusinessSector.FAST_FOOD,
      POS_BusinessSector.DISTRIBUTOR,
    ],
  },
  {
    featureId: 'shifts',
    name: 'Shift Management',
    nameAr: 'إدارة الورديات',
    isCore: true,
    configKey: 'requireShift',
    icon: 'clock',
    displayOrder: 40,
    businessSectors: [
      POS_BusinessSector.SUPERMARKET,
      POS_BusinessSector.RETAIL,
      POS_BusinessSector.RESTAURANT,
      POS_BusinessSector.FAST_FOOD,
    ],
  },
  {
    featureId: 'drafts',
    name: 'Draft Orders',
    nameAr: 'الطلبات المعلقة',
    isCore: false,
    configKey: 'allowDrafts',
    icon: 'file-text',
    displayOrder: 50,
    businessSectors: [
      POS_BusinessSector.RESTAURANT,
      POS_BusinessSector.FAST_FOOD,
      POS_BusinessSector.RETAIL,
    ],
  },
  {
    featureId: 'returns',
    name: 'Returns & Refunds',
    nameAr: 'المرتجعات والاسترداد',
    isCore: false,
    configKey: 'allowReturns',
    icon: 'rotate-ccw',
    displayOrder: 60,
    businessSectors: [
      POS_BusinessSector.SUPERMARKET,
      POS_BusinessSector.RETAIL,
    ],
  },
  {
    featureId: 'reports',
    name: 'Reports',
    nameAr: 'التقارير',
    isCore: false,
    configKey: 'allowReports',
    icon: 'bar-chart',
    displayOrder: 70,
    businessSectors: [
      POS_BusinessSector.SUPERMARKET,
      POS_BusinessSector.RETAIL,
      POS_BusinessSector.RESTAURANT,
      POS_BusinessSector.FAST_FOOD,
      POS_BusinessSector.DISTRIBUTOR,
    ],
  },
  {
    featureId: 'settings',
    name: 'Settings',
    nameAr: 'الإعدادات',
    isCore: true,
    configKey: null,
    icon: 'settings',
    displayOrder: 80,
    businessSectors: [
      POS_BusinessSector.SUPERMARKET,
      POS_BusinessSector.RETAIL,
      POS_BusinessSector.RESTAURANT,
      POS_BusinessSector.FAST_FOOD,
      POS_BusinessSector.DISTRIBUTOR,
    ],
  },
  {
    featureId: 'discounts',
    name: 'Discounts',
    nameAr: 'الخصومات',
    isCore: false,
    configKey: 'allowDiscounts',
    icon: 'percent',
    displayOrder: 25,
    businessSectors: [
      POS_BusinessSector.SUPERMARKET,
      POS_BusinessSector.RETAIL,
      POS_BusinessSector.RESTAURANT,
      POS_BusinessSector.FAST_FOOD,
    ],
  },
];

export async function seedPosFeatures() {
  console.log('Seeding POS features...');

  for (const feature of features) {
    await prisma.pOS_Feature.upsert({
      where: { featureId: feature.featureId },
      update: {
        name: feature.name,
        nameAr: feature.nameAr,
        isCore: feature.isCore,
        configKey: feature.configKey,
        icon: feature.icon,
        displayOrder: feature.displayOrder,
        businessSectors: feature.businessSectors,
      },
      create: feature,
    });
  }

  console.log(`Seeded ${features.length} POS features`);
}

// Run if executed directly
if (require.main === module) {
  seedPosFeatures()
    .then(() => prisma.$disconnect())
    .catch((e) => {
      console.error(e);
      prisma.$disconnect();
      process.exit(1);
    });
}
```

**Run seed:**

```bash
npx ts-node prisma/seeds/pos-features.seed.ts
```

---

## 4. Verification

```bash
# Type check
npm run typecheck

# Verify migration applied
npx prisma migrate status

# Verify seed data
npx prisma studio
# Check pos_features table has 9 records
```

---

## Success Criteria

- [ ] Migration creates `pos_features` table
- [ ] `pos_screens` table has new columns: `feature_id`, `is_entry_point`, `next_screen`
- [ ] `pos_tenant_configurations` has new columns: `allow_drafts`, `allow_reports`
- [ ] Seed creates 9 feature records
- [ ] `npm run typecheck` passes

---

## Rollback

```bash
npx prisma migrate reset
# Or manually:
# DROP TABLE pos_features CASCADE;
# ALTER TABLE pos_screens DROP COLUMN feature_id, is_entry_point, next_screen;
```

---

## Next Phase

Read and follow **Phase-2-Translations-Feature-Library.md**
