# Feature-Based Screen Library - Implementation Prompts

> Use these prompts to implement the Feature-Based Screen Library step by step.
> Each prompt is self-contained and follows TDD methodology.

---

## Phase 1: Schema - Feature Model

```
Implement Feature Library: Phase 1 - Schema Feature Model

TYPE: Backend (Schema + Migration)

1. READ FIRST:
   /home/admin/projects/WadiDMS/e2manage-pos-terminal/FEATURE-BASED-SCREEN-LIBRARY-PLAN/Phase-1-Schema-Feature-Model.md
   /home/admin/projects/WadiDMS/docs/e2manage_Development_Standards.md

2. WORKING DIRECTORY:
   /home/admin/projects/WadiDMS/wadi-dms-api/

3. REFERENCE PATTERNS:
   /home/admin/projects/WadiDMS/wadi-dms-api/prisma/pos.prisma
   /home/admin/projects/WadiDMS/wadi-dms-api/prisma/seeds/

4. REQUIREMENTS:
   - Add POS_Feature model to pos.prisma
   - Modify POS_Screen to add featureId, isEntryPoint, nextScreen
   - Add allowDrafts, allowReports to POS_TenantConfiguration
   - Run migration: npx prisma migrate dev --name add_pos_feature_model
   - Create seed file: prisma/seeds/pos-features.seed.ts
   - Seed 9 features (auth, checkout, payment, shifts, drafts, returns, reports, settings, discounts)
   - Run seed: npx ts-node prisma/seeds/pos-features.seed.ts

5. VERIFICATION:
   npm run typecheck
   npx prisma migrate status
   npx prisma studio  # Check pos_features has 9 records

6. WHEN COMPLETE:
   Update DEPLOYMENT-STATUS.md
   Mark Phase 1 checkboxes complete

Permanent fixes only. No workarounds.
```

---

## Phase 2: Translations - Feature Library

```
Implement Feature Library: Phase 2 - Translations

TYPE: Backend (Database Translations)

1. READ FIRST:
   /home/admin/projects/WadiDMS/e2manage-pos-terminal/FEATURE-BASED-SCREEN-LIBRARY-PLAN/Phase-2-Translations-Feature-Library.md
   /home/admin/projects/WadiDMS/docs/UI_I18N_QUICK_REF.md

2. WORKING DIRECTORY:
   /home/admin/projects/WadiDMS/wadi-dms-api/

3. REFERENCE PATTERNS:
   /home/admin/projects/WadiDMS/wadi-dms-api/sql/
   Existing translation SQL files

4. REQUIREMENTS:
   - Create sql/pos-feature-translations.sql
   - Add feature name translations (en, ar, fr) for all 9 features
   - Add feature description translations
   - Add UI label translations (toggle, badges, config)
   - Use namespace 'pos' with prefix 'pos.feature.'
   - Use ON CONFLICT DO UPDATE pattern
   - Run SQL: psql -U wadi_user -d wadi_dms -f sql/pos-feature-translations.sql

5. VERIFICATION:
   psql -c "SELECT language, COUNT(*) FROM translations WHERE namespace='pos' AND key LIKE 'pos.feature.%' GROUP BY language;"
   # Expected: en=27, ar=27, fr=27

6. WHEN COMPLETE:
   Update DEPLOYMENT-STATUS.md
   Mark Phase 2 checkboxes complete

Permanent fixes only. No workarounds.
```

---

## Phase 3: Backend - Feature Query Handler

```
Implement Feature Library: Phase 3 - Feature Query Handler

TYPE: Backend (Application Layer)

1. READ FIRST:
   /home/admin/projects/WadiDMS/e2manage-pos-terminal/FEATURE-BASED-SCREEN-LIBRARY-PLAN/Phase-3-Backend-Feature-Query-Handler.md
   /home/admin/projects/WadiDMS/docs/e2manage_Development_Standards.md

2. WORKING DIRECTORY:
   /home/admin/projects/WadiDMS/wadi-dms-api/

3. REFERENCE PATTERNS:
   /home/admin/projects/WadiDMS/wadi-dms-api/src/modules/pos/application/queries/
   /home/admin/projects/WadiDMS/wadi-dms-api/src/modules/pos/application/dto/

4. REQUIREMENTS:
   - Write TDD tests FIRST in __tests__/get-features.handler.test.ts
   - Create application/dto/feature.dto.ts with FeatureDto, FeaturesResponseDto
   - Create application/queries/get-features.handler.ts
   - GetFeaturesHandler takes tenantId, businessSector, includeScreens
   - Core features (isCore=true) always enabled
   - Optional features check configKey against POS_TenantConfiguration
   - Calculate version hash for ETag caching
   - Export from queries/index.ts

5. VERIFICATION:
   npm run typecheck
   npm test -- src/modules/pos/application/queries/__tests__/get-features.handler.test.ts

6. WHEN COMPLETE:
   Update DEPLOYMENT-STATUS.md
   Mark Phase 3 checkboxes complete

Permanent fixes only. No workarounds.
```

---

## Phase 4: Backend - Auth Handler Update

```
Implement Feature Library: Phase 4 - Auth Handler Update

TYPE: Backend (Refactor)

1. READ FIRST:
   /home/admin/projects/WadiDMS/e2manage-pos-terminal/FEATURE-BASED-SCREEN-LIBRARY-PLAN/Phase-4-Backend-Auth-Handler-Update.md
   /home/admin/projects/WadiDMS/docs/e2manage_Development_Standards.md

2. WORKING DIRECTORY:
   /home/admin/projects/WadiDMS/wadi-dms-api/

3. REFERENCE PATTERNS:
   /home/admin/projects/WadiDMS/wadi-dms-api/src/modules/pos/application/commands/authenticate-terminal.handler.ts

4. REQUIREMENTS:
   - Write TDD tests FIRST for feature integration
   - Add getEnabledFeatures private method using GetFeaturesHandler
   - Replace hardcoded feature building with database query
   - Keep SPLIT_PAYMENT logic for multi-payment configs
   - Filter features by terminal's businessSector
   - Maintain backward compatibility (same response format)

5. VERIFICATION:
   npm run typecheck
   npm test -- src/modules/pos/application/commands/__tests__/authenticate-terminal.handler.test.ts
   npm test -- src/modules/pos/

6. WHEN COMPLETE:
   Update DEPLOYMENT-STATUS.md
   Mark Phase 4 checkboxes complete

Permanent fixes only. No workarounds.
```

---

## Phase 5: Backend - Feature Controller

```
Implement Feature Library: Phase 5 - Feature Controller

TYPE: Backend (Presentation Layer)

1. READ FIRST:
   /home/admin/projects/WadiDMS/e2manage-pos-terminal/FEATURE-BASED-SCREEN-LIBRARY-PLAN/Phase-5-Backend-Feature-Controller.md
   /home/admin/projects/WadiDMS/docs/e2manage_Development_Standards.md

2. WORKING DIRECTORY:
   /home/admin/projects/WadiDMS/wadi-dms-api/

3. REFERENCE PATTERNS:
   /home/admin/projects/WadiDMS/wadi-dms-api/src/modules/pos/presentation/controllers/

4. REQUIREMENTS:
   - Write TDD tests FIRST
   - Create presentation/controllers/feature.controller.ts
   - GET /api/pos/features - List all features (admin only)
   - GET /api/pos/features/terminal - Get enabled features (terminal auth, ETag support)
   - GET /api/pos/features/:featureId - Get single feature
   - PUT /api/pos/features/:featureId - Update feature (admin only)
   - Create presentation/routes/feature.routes.ts
   - Register in pos.module.ts
   - Apply terminal auth for /terminal endpoint
   - Apply admin auth for other endpoints

5. VERIFICATION:
   npm run typecheck
   npm test -- src/modules/pos/presentation/controllers/__tests__/feature.controller.test.ts
   # Manual: curl tests for all endpoints

6. WHEN COMPLETE:
   Update DEPLOYMENT-STATUS.md
   Mark Phase 5 checkboxes complete

Permanent fixes only. No workarounds.
```

---

## Phase 6: POS - DB Schema Features

```
Implement Feature Library: Phase 6 - POS DB Schema

TYPE: POS Terminal (Rust)

1. READ FIRST:
   /home/admin/projects/WadiDMS/e2manage-pos-terminal/FEATURE-BASED-SCREEN-LIBRARY-PLAN/Phase-6-POS-DB-Schema-Features.md
   /home/admin/projects/WadiDMS/e2manage-pos-terminal/CLAUDE.md

2. WORKING DIRECTORY:
   /home/admin/projects/WadiDMS/e2manage-pos-terminal/

3. REFERENCE PATTERNS:
   crates/pos-db/src/
   crates/pos-models/src/

4. REQUIREMENTS:
   - Write TDD tests FIRST in crates/pos-db/src/features_tests.rs
   - Create crates/pos-models/src/feature.rs with Feature, FeatureScreen, FeaturesResponse
   - Create crates/pos-db/src/features.rs with SQLite schema
   - Create tables: features, feature_screens
   - Implement upsert_feature, upsert_feature_screen
   - Implement get_feature, get_enabled_features, get_feature_screens
   - Implement is_screen_enabled, clear_features
   - Add init_features_schema to migration
   - Export from lib.rs files

5. VERIFICATION:
   cargo test --package pos-db features
   cargo test --package pos-models
   cargo check
   cargo build

6. WHEN COMPLETE:
   Update DEPLOYMENT-STATUS.md
   Mark Phase 6 checkboxes complete

Permanent fixes only. No workarounds.
```

---

## Phase 7: POS - Feature Service

```
Implement Feature Library: Phase 7 - POS Feature Service

TYPE: POS Terminal (Rust)

1. READ FIRST:
   /home/admin/projects/WadiDMS/e2manage-pos-terminal/FEATURE-BASED-SCREEN-LIBRARY-PLAN/Phase-7-POS-Feature-Service.md
   /home/admin/projects/WadiDMS/e2manage-pos-terminal/CLAUDE.md

2. WORKING DIRECTORY:
   /home/admin/projects/WadiDMS/e2manage-pos-terminal/

3. REFERENCE PATTERNS:
   crates/pos-services/src/

4. REQUIREMENTS:
   - Write TDD tests FIRST in crates/pos-services/src/feature_service_tests.rs
   - Create crates/pos-services/src/feature_service.rs
   - Implement FeatureService with Arc<Database>
   - is_screen_enabled(screen_id) -> bool
   - is_feature_enabled(feature_id) -> bool
   - get_enabled_screen_ids() -> Vec<String>
   - get_next_screen(current_screen) -> Option<String>
   - get_feature_entry_screen(feature_id) -> Option<String>
   - get_screen_feature(screen_id) -> Option<String>
   - can_navigate_to(target_screen) -> bool
   - Export from lib.rs

5. VERIFICATION:
   cargo test --package pos-services feature_service
   cargo check --package pos-services
   cargo build

6. WHEN COMPLETE:
   Update DEPLOYMENT-STATUS.md
   Mark Phase 7 checkboxes complete

Permanent fixes only. No workarounds.
```

---

## Phase 8: POS - Sync Features

```
Implement Feature Library: Phase 8 - POS Sync Features

TYPE: POS Terminal (Rust)

1. READ FIRST:
   /home/admin/projects/WadiDMS/e2manage-pos-terminal/FEATURE-BASED-SCREEN-LIBRARY-PLAN/Phase-8-POS-Sync-Features.md
   /home/admin/projects/WadiDMS/e2manage-pos-terminal/CLAUDE.md

2. WORKING DIRECTORY:
   /home/admin/projects/WadiDMS/e2manage-pos-terminal/

3. REFERENCE PATTERNS:
   crates/pos-api/src/
   crates/pos-services/src/sync_service.rs

4. REQUIREMENTS:
   - Write TDD tests FIRST
   - Create crates/pos-api/src/features.rs with get_features(etag) -> Option<FeaturesResponse>
   - Handle 304 Not Modified response
   - Add sync_features() to SyncService
   - Clear old features before storing new ones
   - Store features and screens in SQLite
   - Store/retrieve ETag from settings
   - Emit SyncEvent::FeaturesUpdated
   - Add to run_sync_cycle()
   - Export from lib.rs

5. VERIFICATION:
   cargo test --package pos-api features
   cargo test --package pos-services sync
   cargo check
   cargo build

6. WHEN COMPLETE:
   Update DEPLOYMENT-STATUS.md
   Mark Phase 8 checkboxes complete

Permanent fixes only. No workarounds.
```

---

## Phase 9: POS - Navigation Integration

```
Implement Feature Library: Phase 9 - POS Navigation Integration

TYPE: POS Terminal (Rust)

1. READ FIRST:
   /home/admin/projects/WadiDMS/e2manage-pos-terminal/FEATURE-BASED-SCREEN-LIBRARY-PLAN/Phase-9-POS-Navigation-Integration.md
   /home/admin/projects/WadiDMS/e2manage-pos-terminal/CLAUDE.md

2. WORKING DIRECTORY:
   /home/admin/projects/WadiDMS/e2manage-pos-terminal/

3. REFERENCE PATTERNS:
   src/main.rs
   src/ui/

4. REQUIREMENTS:
   - Write TDD tests FIRST in tests/navigation_tests.rs
   - Create src/ui/navigation.rs with Navigator struct
   - NavigationResult enum: Success, Blocked, NotFound
   - navigate_to(screen_id) checks feature status
   - navigate_next() follows next_screen chain
   - navigate_to_feature(feature_id) goes to entry point
   - Integrate Navigator into main.rs
   - Screen state is an enum, never a string keyed through a router
   - Expose is_feature_enabled / can_navigate_to to the view layer
   - Hide disabled features in UI

5. VERIFICATION:
   cargo test --test navigation_tests
   cargo check
   cargo build
   cargo run  # Test navigation manually

6. WHEN COMPLETE:
   Update DEPLOYMENT-STATUS.md
   Mark Phase 9 checkboxes complete

Permanent fixes only. No workarounds.
```

---

## Phase 10: Frontend - Feature Config UI

```
Implement Feature Library: Phase 10 - Frontend Feature Config UI

TYPE: Frontend (React)

1. READ FIRST:
   /home/admin/projects/WadiDMS/e2manage-pos-terminal/FEATURE-BASED-SCREEN-LIBRARY-PLAN/Phase-10-Frontend-Feature-Config-UI.md
   /home/admin/projects/WadiDMS/docs/UI_I18N_QUICK_REF.md

2. WORKING DIRECTORY:
   /home/admin/projects/WadiDMS/wadi-dms-ui/

3. REFERENCE PATTERNS:
   /home/admin/projects/WadiDMS/wadi-dms-ui/src/pages/pos/
   /home/admin/projects/WadiDMS/wadi-dms-ui/src/services/

4. REQUIREMENTS:
   - Write TDD tests FIRST
   - Create src/types/pos/feature.types.ts
   - Create src/services/pos/feature.service.ts
   - Create src/components/pos/FeatureToggles.tsx
   - Show features with toggle switches
   - Core features show badge and disabled toggle
   - Optional features toggle updates tenant config
   - Show screen count per feature
   - Integrate into POSConfigPage.tsx
   - Use Tanstack Query for data fetching
   - Use i18n for all labels (no hardcoded strings)
   - Support RTL for Arabic

5. VERIFICATION:
   npm run typecheck
   npm test -- POSFeatureConfig
   npm run dev  # Navigate to POS config page

6. WHEN COMPLETE:
   Update DEPLOYMENT-STATUS.md
   Mark Phase 10 checkboxes complete

Permanent fixes only. No workarounds.
```

---

## Phase 11: Integration - E2E Tests

```
Implement Feature Library: Phase 11 - E2E Tests

TYPE: Integration Testing

1. READ FIRST:
   /home/admin/projects/WadiDMS/e2manage-pos-terminal/FEATURE-BASED-SCREEN-LIBRARY-PLAN/Phase-11-Integration-E2E-Tests.md
   /home/admin/projects/WadiDMS/docs/e2manage_Development_Standards.md

2. WORKING DIRECTORIES:
   /home/admin/projects/WadiDMS/wadi-dms-api/
   /home/admin/projects/WadiDMS/e2manage-pos-terminal/

3. REFERENCE PATTERNS:
   Existing E2E test files

4. REQUIREMENTS:
   - Create Backend E2E: src/modules/pos/__tests__/e2e/feature-library.e2e.test.ts
   - Test GET /api/pos/features
   - Test GET /api/pos/features/terminal with ETag
   - Test feature toggle flow
   - Test auth response includes features
   - Create POS Terminal E2E: tests/e2e_feature_sync.rs
   - Test feature sync from backend
   - Test navigation with features
   - Test ETag caching
   - Create test script: scripts/run-feature-e2e.sh
   - Create test seed data

5. VERIFICATION:
   # Backend E2E
   cd wadi-dms-api && npm run test:e2e -- feature-library

   # POS Terminal E2E
   cd e2manage-pos-terminal
   BACKEND_URL=http://localhost:3000 cargo test --test e2e_feature_sync -- --ignored

6. WHEN COMPLETE:
   Update DEPLOYMENT-STATUS.md
   Mark Phase 11 checkboxes complete
   PLAN COMPLETE!

Permanent fixes only. No workarounds.
```

---

## Quick Commands Reference

### Backend (wadi-dms-api)

```bash
# Type check
npm run typecheck

# Run specific test
npm test -- [test-file]

# Run all POS tests
npm test -- src/modules/pos/

# E2E tests
npm run test:e2e -- feature-library

# Prisma commands
npx prisma migrate dev --name [name]
npx prisma generate
npx prisma studio

# Run seed
npx ts-node prisma/seeds/pos-features.seed.ts

# Run SQL
psql -U wadi_user -d wadi_dms -f sql/pos-feature-translations.sql
```

### POS Terminal (e2manage-pos-terminal)

```bash
# Check compilation
cargo check

# Build
cargo build

# Run tests
cargo test

# Run specific package tests
cargo test --package pos-db
cargo test --package pos-services

# Run specific test
cargo test test_name

# Run with output
cargo test -- --nocapture

# E2E tests (requires backend)
BACKEND_URL=http://localhost:3000 cargo test --test e2e_feature_sync -- --ignored
```

### Frontend (wadi-dms-ui)

```bash
# Type check
npm run typecheck

# Run tests
npm test -- [component]

# Dev server
npm run dev
```

---

## Files to Create/Modify

### Backend (wadi-dms-api)

| Phase | Action | File |
|-------|--------|------|
| 1 | MODIFY | prisma/pos.prisma |
| 1 | CREATE | prisma/seeds/pos-features.seed.ts |
| 2 | CREATE | sql/pos-feature-translations.sql |
| 3 | CREATE | src/modules/pos/application/dto/feature.dto.ts |
| 3 | CREATE | src/modules/pos/application/queries/get-features.handler.ts |
| 3 | CREATE | src/modules/pos/application/queries/__tests__/get-features.handler.test.ts |
| 4 | MODIFY | src/modules/pos/application/commands/authenticate-terminal.handler.ts |
| 5 | CREATE | src/modules/pos/presentation/controllers/feature.controller.ts |
| 5 | CREATE | src/modules/pos/presentation/routes/feature.routes.ts |
| 5 | MODIFY | src/modules/pos/pos.module.ts |
| 11 | CREATE | src/modules/pos/__tests__/e2e/feature-library.e2e.test.ts |

### POS Terminal (e2manage-pos-terminal)

| Phase | Action | File |
|-------|--------|------|
| 6 | CREATE | crates/pos-models/src/feature.rs |
| 6 | MODIFY | crates/pos-models/src/lib.rs |
| 6 | CREATE | crates/pos-db/src/features.rs |
| 6 | MODIFY | crates/pos-db/src/lib.rs |
| 7 | CREATE | crates/pos-services/src/feature_service.rs |
| 7 | MODIFY | crates/pos-services/src/lib.rs |
| 8 | CREATE | crates/pos-api/src/features.rs |
| 8 | MODIFY | crates/pos-api/src/lib.rs |
| 8 | MODIFY | crates/pos-services/src/sync_service.rs |
| 9 | CREATE | src/ui/navigation.rs |
| 9 | MODIFY | src/main.rs |
| 11 | CREATE | tests/e2e_feature_sync.rs |
| 11 | CREATE | scripts/run-feature-e2e.sh |

### Frontend (wadi-dms-ui)

| Phase | Action | File |
|-------|--------|------|
| 10 | CREATE | src/types/pos/feature.types.ts |
| 10 | CREATE | src/services/pos/feature.service.ts |
| 10 | CREATE | src/components/pos/FeatureToggles.tsx |
| 10 | MODIFY | src/pages/pos/POSConfigPage.tsx |

---

## API Endpoints

| Method | Endpoint | Auth | Description |
|--------|----------|------|-------------|
| GET | /api/pos/features | Admin | List all features |
| GET | /api/pos/features/terminal | Terminal | Get enabled features (ETag) |
| GET | /api/pos/features/:featureId | Admin | Get single feature |
| PUT | /api/pos/features/:featureId | Admin | Update feature metadata |

---

## Database Tables

### PostgreSQL (Backend)

```
pos_features
├── id (UUID)
├── feature_id (VARCHAR, UNIQUE)
├── name (VARCHAR)
├── name_ar (VARCHAR)
├── config_key (VARCHAR, nullable)
├── is_core (BOOLEAN)
├── business_sectors (ARRAY)
├── icon (VARCHAR)
├── display_order (INT)
├── created_at
└── updated_at

pos_screens (modified)
├── ... existing fields ...
├── feature_id (UUID, FK) [NEW]
├── is_entry_point (BOOLEAN) [NEW]
└── next_screen (VARCHAR) [NEW]

pos_tenant_configurations (modified)
├── ... existing fields ...
├── allow_drafts (BOOLEAN) [NEW]
└── allow_reports (BOOLEAN) [NEW]
```

### SQLite (POS Terminal)

```
features
├── feature_id (TEXT, PK)
├── name (TEXT)
├── name_ar (TEXT)
├── config_key (TEXT)
├── is_core (INTEGER)
├── is_enabled (INTEGER)
├── icon (TEXT)
├── display_order (INTEGER)
└── updated_at (TEXT)

feature_screens
├── id (INTEGER, PK)
├── feature_id (TEXT, FK)
├── screen_id (TEXT, UNIQUE with feature_id)
├── name (TEXT)
├── name_ar (TEXT)
├── is_entry_point (INTEGER)
├── next_screen (TEXT)
└── display_order (INTEGER)
```

---

## Success Criteria Summary

1. Tenant can toggle feature flags in POSConfigPage
2. POS app syncs features on startup and every sync interval
3. Disabled features hide their screens in POS navigation
4. Core features cannot be disabled
5. Business sector filtering works correctly
6. Offline mode works with cached features
7. Existing authentication flow continues to work
8. All existing screens continue to work
9. No hardcoded strings (DB translations)
10. Multi-tenant isolation maintained
11. ETag caching works for efficient sync
12. All E2E tests pass

---

**Track progress in:** `DEPLOYMENT-STATUS.md`
