# E2Manage POS Feature-Based Screen Library System

## Overview

Implement a **formal feature model** that defines POS features and their screens, integrated with the existing `POS_TenantConfiguration` feature flags.

**Key Insight**: Feature flags already exist in `POS_TenantConfiguration` (`allowReturns`, `allowDiscounts`, etc.). The formal model adds structure without replacing what works.

---

## Architecture: Integration with Existing Patterns

```
┌─────────────────────────────────────────────────────────────┐
│                   POS_TenantConfiguration                   │
│  (EXISTING - Source of truth for enabled/disabled)         │
│  ┌─────────────────────────────────────────────────────┐   │
│  │ allowReturns: true        ← Enables RETURNS feature │   │
│  │ allowDiscounts: true      ← Enables DISCOUNTS       │   │
│  │ requireShift: true        ← Enables SHIFTS          │   │
│  │ ...                                                 │   │
│  └─────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────┘
                              ↓ references
┌─────────────────────────────────────────────────────────────┐
│                     POS_Feature (NEW)                       │
│  (Formal definition of features and their screens)         │
│  ┌─────────────────────────────────────────────────────┐   │
│  │ featureId: "returns"                                │   │
│  │ configKey: "allowReturns"  ← Links to tenant config │   │
│  │ screens: [return-entry, return-items, refund]       │   │
│  │ businessSectors: [SUPERMARKET, RETAIL]              │   │
│  └─────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────┘
                              ↓ contains
┌─────────────────────────────────────────────────────────────┐
│                    POS_Screen (EXISTING)                    │
│  (Individual screen definitions)                            │
│  ┌─────────────────────────────────────────────────────┐   │
│  │ screenId: "return-entry"                            │   │
│  │ featureId: "returns"  ← NEW: Links to feature       │   │
│  │ definition: { ... }                                 │   │
│  └─────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────┘
```

---

## Feature Definitions

| Feature ID | Config Key | Name | Screens | Core? |
|------------|------------|------|---------|-------|
| `auth` | (always enabled) | Authentication | splash, setup, login-select, pin-entry | Yes |
| `checkout` | (always enabled) | Checkout | checkout, product-search | Yes |
| `payment` | (always enabled) | Payments | payment-methods, cash-payment, card-payment, qr-payment, split-payment | Yes |
| `shifts` | `requireShift` | Shift Management | start-shift, end-shift | Yes |
| `drafts` | `allowDrafts` (NEW) | Draft Orders | save-draft, recall-draft | No |
| `returns` | `allowReturns` | Returns & Refunds | return-entry, return-items, refund | No |
| `reports` | `allowReports` (NEW) | Reports | x-report, z-report | No |
| `settings` | (always enabled) | Settings | settings-home, printer-settings, display-settings | Yes |
| `discounts` | `allowDiscounts` | Discounts | (in-screen feature flag) | No |

---

## Phase 1: Backend Schema Changes

### 1.1 Add POS_Feature Model

**File**: `wadi-dms-api/prisma/pos.prisma`

```prisma
model POS_Feature {
  id              String   @id @default(dbgenerated("uuid_generate_v4()")) @db.Uuid
  featureId       String   @unique @map("feature_id") @db.VarChar(50)
  name            String   @db.VarChar(100)
  nameAr          String?  @map("name_ar") @db.VarChar(100)
  description     String?

  // Link to POS_TenantConfiguration field name
  configKey       String?  @map("config_key") @db.VarChar(50)  // e.g., "allowReturns"

  // If null, feature is always enabled (core feature)
  isCore          Boolean  @default(false) @map("is_core")

  // Targeting
  businessSectors POS_BusinessSector[] @map("business_sectors")
  requiredRoles   String[] @map("required_roles")

  // Display
  icon            String?  @db.VarChar(50)
  displayOrder    Int      @default(100) @map("display_order")

  // Screens
  screens         POS_Screen[]

  createdAt       DateTime @default(now()) @map("created_at")
  updatedAt       DateTime @default(now()) @updatedAt @map("updated_at")

  @@map("pos_features")
}
```

### 1.2 Enhance POS_Screen Model

**File**: `wadi-dms-api/prisma/pos.prisma` (modify existing)

```prisma
model POS_Screen {
  // ... existing fields ...

  // NEW: Link to feature
  featureId    String?     @map("feature_id") @db.Uuid
  feature      POS_Feature? @relation(fields: [featureId], references: [id])

  // NEW: Navigation within feature
  isEntryPoint Boolean     @default(false) @map("is_entry_point")
  nextScreen   String?     @map("next_screen") @db.VarChar(50)

  // ... rest of existing fields ...
}
```

### 1.3 Add Missing Config Flags

**File**: `wadi-dms-api/prisma/pos.prisma` (add to POS_TenantConfiguration)

```prisma
model POS_TenantConfiguration {
  // ... existing fields ...

  // NEW: Additional feature flags
  allowDrafts    Boolean @default(true) @map("allow_drafts")
  allowReports   Boolean @default(true) @map("allow_reports")

  // ... rest of existing fields ...
}
```

### 1.4 Seed Default Features

**File**: `wadi-dms-api/prisma/seeds/pos-features.seed.ts`

```typescript
const features = [
  { featureId: 'auth', name: 'Authentication', isCore: true, configKey: null },
  { featureId: 'checkout', name: 'Checkout', isCore: true, configKey: null },
  { featureId: 'payment', name: 'Payments', isCore: true, configKey: null },
  { featureId: 'shifts', name: 'Shift Management', isCore: true, configKey: 'requireShift' },
  { featureId: 'drafts', name: 'Draft Orders', isCore: false, configKey: 'allowDrafts' },
  { featureId: 'returns', name: 'Returns & Refunds', isCore: false, configKey: 'allowReturns' },
  { featureId: 'reports', name: 'Reports', isCore: false, configKey: 'allowReports' },
  { featureId: 'settings', name: 'Settings', isCore: true, configKey: null },
];
```

---

## Phase 2: Backend API Changes

### 2.1 Feature Query Handler

**File**: `wadi-dms-api/src/modules/pos/application/queries/get-features.handler.ts`

```typescript
async execute(query: GetFeaturesQueryDto): Promise<FeaturesResponseDto> {
  // 1. Get tenant configuration
  const tenantConfig = await this.prisma.pOS_TenantConfiguration.findUnique({
    where: { tenantId: query.tenantId }
  });

  // 2. Get all features with screens
  const features = await this.prisma.pOS_Feature.findMany({
    where: {
      businessSectors: { has: query.businessSector }
    },
    include: { screens: true }
  });

  // 3. Filter by enabled config flags
  const enabledFeatures = features.filter(f => {
    if (f.isCore) return true;  // Core features always enabled
    if (!f.configKey) return true;  // No config key = always enabled
    return tenantConfig?.[f.configKey] === true;  // Check tenant flag
  });

  return { features: enabledFeatures, version: this.calculateHash(enabledFeatures) };
}
```

### 2.2 Update Authentication Handler

**File**: `wadi-dms-api/src/modules/pos/application/commands/authenticate-terminal.handler.ts`

Replace hardcoded feature extraction with query:
```typescript
// BEFORE (hardcoded):
if (tenantConfig?.allowReturns) features.push('RETURNS');

// AFTER (from POS_Feature):
const enabledFeatures = await this.getEnabledFeatures(terminal.tenantId, terminal.businessSector);
const features = enabledFeatures.map(f => f.featureId.toUpperCase());
```

### 2.3 Feature Controller

**File**: `wadi-dms-api/src/modules/pos/presentation/controllers/feature.controller.ts`

| Endpoint | Method | Purpose |
|----------|--------|---------|
| `GET /api/pos/features` | GET | List all features (admin) |
| `GET /api/pos/features/terminal` | GET | Get enabled features for terminal (with ETag) |
| `PUT /api/pos/features/:featureId` | PUT | Update feature metadata (admin) |

---

## Phase 3: POS App Changes (Rust)

### 3.1 Update Database Schema

**File**: `crates/pos-db/src/schema.rs`

```sql
CREATE TABLE IF NOT EXISTS features (
    feature_id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    name_ar TEXT,
    config_key TEXT,
    is_core INTEGER DEFAULT 0,
    icon TEXT,
    display_order INTEGER DEFAULT 100,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS feature_screens (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    feature_id TEXT NOT NULL,
    screen_id TEXT NOT NULL,
    name TEXT NOT NULL,
    is_entry_point INTEGER DEFAULT 0,
    next_screen TEXT,
    display_order INTEGER DEFAULT 100,
    FOREIGN KEY (feature_id) REFERENCES features(feature_id),
    UNIQUE(feature_id, screen_id)
);
```

### 3.2 Add Feature DTOs

**File**: `crates/pos-api/src/sync.rs`

```rust
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FeaturesResponse {
    pub features: Vec<FeatureDto>,
    pub version: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FeatureDto {
    pub feature_id: String,
    pub name: String,
    pub is_core: bool,
    pub screens: Vec<FeatureScreenDto>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FeatureScreenDto {
    pub screen_id: String,
    pub name: String,
    pub is_entry_point: bool,
    pub next_screen: Option<String>,
}
```

### 3.3 Implement FeatureService

**File**: `crates/pos-services/src/feature_service.rs` (NEW)

```rust
pub struct FeatureService {
    db: Arc<Database>,
}

impl FeatureService {
    /// Check if a screen is accessible (its feature is enabled)
    pub fn is_screen_enabled(&self, screen_id: &str) -> Result<bool> {
        // Lookup screen's feature, check if feature is in local cache
    }

    /// Get all enabled screens
    pub fn get_enabled_screens(&self) -> Result<Vec<String>> {
        // Return all screen_ids from enabled features
    }

    /// Get next screen in navigation
    pub fn get_next_screen(&self, current_screen: &str) -> Result<Option<String>> {
        // Lookup from feature_screens table
    }
}
```

### 3.4 Implement sync_features()

**File**: `crates/pos-services/src/sync_service.rs`

```rust
async fn sync_features(&self, tx: &broadcast::Sender<SyncEvent>) -> Result<()> {
    let response = self.api.get_features().await?;

    // Store features and screens in local DB
    self.db.upsert_features(&response.features)?;

    let _ = tx.send(SyncEvent::FeaturesUpdated {
        count: response.features.len()
    });

    Ok(())
}
```

### 3.5 Update Navigation

**File**: `src/main.rs`

```rust
fn navigate_to(&self, screen_id: &str) -> Result<()> {
    // Check if screen's feature is enabled
    if !self.feature_service.is_screen_enabled(screen_id)? {
        warn!("Screen {} is disabled (feature not enabled)", screen_id);
        return Ok(());
    }

    self.window.set_current_screen(screen_id.into());
    Ok(())
}
```

---

## Phase 4: Tenant UI (React)

### 4.1 Enhance POSConfigPage

**File**: `wadi-dms-ui/src/pages/pos/POSConfigPage.tsx`

Add a "Features" tab showing:
- Feature list with toggles (non-core features only)
- Each toggle updates the corresponding `POS_TenantConfiguration` flag
- Shows screen count per feature

```tsx
// Features Section
<FeatureToggles>
  <FeatureToggle
    label="Draft Orders"
    description="Allow saving and recalling draft orders"
    enabled={config.allowDrafts}
    onChange={(v) => updateConfig({ allowDrafts: v })}
    screenCount={2}
  />
  <FeatureToggle
    label="Returns & Refunds"
    enabled={config.allowReturns}
    onChange={(v) => updateConfig({ allowReturns: v })}
    screenCount={3}
  />
  // ...
</FeatureToggles>
```

### 4.2 Feature Management Page (Admin Only)

**File**: `wadi-dms-ui/src/pages/pos/FeatureListPage.tsx`

For system administrators to manage feature definitions:
- List all features
- Edit feature metadata (name, icon, description)
- Manage screen assignments
- Set business sector targeting

---

## Phase 5: Implementation Order

### Week 1: Backend
1. [ ] Add `POS_Feature` model to Prisma
2. [ ] Modify `POS_Screen` to add `featureId`, `isEntryPoint`, `nextScreen`
3. [ ] Add `allowDrafts`, `allowReports` to `POS_TenantConfiguration`
4. [ ] Run migration
5. [ ] Create seed data for 8 features + screen assignments
6. [ ] Implement `GetFeaturesHandler`
7. [ ] Update `AuthenticateTerminalHandler` to use features from DB
8. [ ] Add `/api/pos/features/terminal` endpoint

### Week 2: POS App
1. [ ] Update SQLite schema
2. [ ] Add Feature DTOs to pos-api
3. [ ] Implement FeatureService
4. [ ] Implement sync_features() in SyncService
5. [ ] Add screen access check to navigation
6. [ ] Test offline behavior

### Week 3: Tenant UI + Testing
1. [ ] Add Features section to POSConfigPage
2. [ ] Create FeatureListPage for admin
3. [ ] Add i18n translations
4. [ ] End-to-end testing
5. [ ] Documentation

---

## Files to Modify/Create

### Backend (wadi-dms-api)
| Action | File |
|--------|------|
| MODIFY | `prisma/pos.prisma` (add POS_Feature, modify POS_Screen, modify POS_TenantConfiguration) |
| CREATE | `prisma/seeds/pos-features.seed.ts` |
| CREATE | `src/modules/pos/application/queries/get-features.handler.ts` |
| CREATE | `src/modules/pos/application/dto/feature.dto.ts` |
| MODIFY | `src/modules/pos/application/commands/authenticate-terminal.handler.ts` |
| MODIFY | `src/modules/pos/presentation/controllers/screen.controller.ts` (add feature endpoints) |

### POS App (e2manage-pos-terminal)
| Action | File |
|--------|------|
| MODIFY | `crates/pos-db/src/schema.rs` |
| CREATE | `crates/pos-db/src/features.rs` |
| MODIFY | `crates/pos-api/src/sync.rs` |
| CREATE | `crates/pos-services/src/feature_service.rs` |
| MODIFY | `crates/pos-services/src/sync_service.rs` |
| MODIFY | `src/main.rs` |

### Tenant UI (wadi-dms-ui)
| Action | File |
|--------|------|
| MODIFY | `src/pages/pos/POSConfigPage.tsx` (add Features section) |
| CREATE | `src/pages/pos/FeatureListPage.tsx` (admin only) |
| MODIFY | `src/types/pos/config.types.ts` |

---

## Key Design Decisions

1. **Integration over duplication**: Use existing `POS_TenantConfiguration` flags as source of truth for enabled/disabled, `POS_Feature` adds structure

2. **configKey pattern**: Each feature links to its tenant config flag via `configKey`, enabling dynamic lookup

3. **Core features**: Features with `isCore: true` or `configKey: null` are always enabled

4. **Screen grouping**: Screens belong to features via `featureId`, enabling bulk enable/disable

5. **Backward compatible**: Existing authentication response format unchanged, features array populated from DB instead of hardcoded

---

## Success Criteria

1. ✅ Tenant can toggle feature flags in POSConfigPage
2. ✅ POS app syncs features on startup and every 10 minutes
3. ✅ Disabled features hide their screens in POS navigation
4. ✅ Core features cannot be disabled
5. ✅ Business sector filtering works correctly
6. ✅ Offline mode works with cached features
7. ✅ Existing authentication flow continues to work
8. ✅ All 27 existing screens continue to work
