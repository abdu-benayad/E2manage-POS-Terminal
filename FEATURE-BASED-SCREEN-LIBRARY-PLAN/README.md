# Feature-Based Screen Library Implementation Plan

## Overview

Implement a **formal feature model** that defines POS features and their screens, integrated with the existing `POS_TenantConfiguration` feature flags. This system enables:

- Structured screen grouping by feature
- Tenant-configurable feature toggles
- Business sector targeting
- Offline-capable feature sync to POS terminals

**Key Insight**: Feature flags already exist in `POS_TenantConfiguration` (`allowReturns`, `allowDiscounts`, etc.). The formal model adds structure without replacing what works.

---

## Objectives

1. **Formal Feature Registry** - Database-driven feature definitions linked to tenant config flags
2. **Screen-Feature Binding** - Associate screens with features for bulk enable/disable
3. **Dynamic Navigation** - POS app hides/shows features based on synced configuration
4. **Business Sector Targeting** - Features can be limited to specific sectors (SUPERMARKET, RETAIL, etc.)
5. **Offline Support** - Features sync to SQLite and work offline
6. **Admin UI** - Feature toggle management in tenant configuration

---

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                   POS_TenantConfiguration                   │
│  (EXISTING - Source of truth for enabled/disabled)         │
│  ┌─────────────────────────────────────────────────────┐   │
│  │ allowReturns: true        ← Enables RETURNS feature │   │
│  │ allowDiscounts: true      ← Enables DISCOUNTS       │   │
│  │ requireShift: true        ← Enables SHIFTS          │   │
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
| `drafts` | `allowDrafts` | Draft Orders | save-draft, recall-draft | No |
| `returns` | `allowReturns` | Returns & Refunds | return-entry, return-items, refund | No |
| `reports` | `allowReports` | Reports | x-report, z-report | No |
| `settings` | (always enabled) | Settings | settings-home, printer-settings, display-settings | Yes |
| `discounts` | `allowDiscounts` | Discounts | (in-screen feature flag) | No |

---

## Implementation Phases

### Schema & Foundation (Phases 1-2)

| Phase | Name | Time | Description |
|-------|------|------|-------------|
| 1 | Schema - Feature Model | 1.5h | Add POS_Feature model, modify POS_Screen, add config flags |
| 2 | Translations - Feature Library | 1h | DB translations for feature names and descriptions |

### Backend API (Phases 3-5)

| Phase | Name | Time | Description |
|-------|------|------|-------------|
| 3 | Backend - Feature Query Handler | 1.5h | Get features filtered by tenant config and sector |
| 4 | Backend - Auth Handler Update | 1h | Replace hardcoded feature extraction with DB query |
| 5 | Backend - Feature Controller | 1.5h | REST endpoints for feature management |

### POS Terminal (Rust) (Phases 6-9)

| Phase | Name | Time | Description |
|-------|------|------|-------------|
| 6 | POS - DB Schema Features | 1h | SQLite tables for features and feature_screens |
| 7 | POS - Feature Service | 1.5h | FeatureService for screen access checks |
| 8 | POS - Sync Features | 1h | Add sync_features() to SyncService |
| 9 | POS - Navigation Integration | 1.5h | Screen access control in navigation |

### Frontend (Phase 10)

| Phase | Name | Time | Description |
|-------|------|------|-------------|
| 10 | Frontend - Feature Config UI | 2h | Feature toggles in tenant configuration page |

### Testing (Phase 11)

| Phase | Name | Time | Description |
|-------|------|------|-------------|
| 11 | Integration - E2E Tests | 2h | Full integration tests with real data |

**Total Estimated Time**: ~15 hours

---

## Files to Modify/Create

### Backend (wadi-dms-api)

| Action | File |
|--------|------|
| MODIFY | `prisma/pos.prisma` |
| CREATE | `prisma/seeds/pos-features.seed.ts` |
| CREATE | `src/modules/pos/application/queries/get-features.handler.ts` |
| CREATE | `src/modules/pos/application/dto/feature.dto.ts` |
| MODIFY | `src/modules/pos/application/commands/authenticate-terminal.handler.ts` |
| CREATE | `src/modules/pos/presentation/controllers/feature.controller.ts` |

### POS Terminal (e2manage-pos-terminal)

| Action | File |
|--------|------|
| CREATE | `crates/pos-db/src/features.rs` |
| MODIFY | `crates/pos-db/src/lib.rs` |
| CREATE | `crates/pos-models/src/feature.rs` |
| MODIFY | `crates/pos-models/src/lib.rs` |
| CREATE | `crates/pos-api/src/features.rs` |
| MODIFY | `crates/pos-api/src/lib.rs` |
| CREATE | `crates/pos-services/src/feature_service.rs` |
| MODIFY | `crates/pos-services/src/sync_service.rs` |
| MODIFY | `src/main.rs` |

### Frontend (wadi-dms-ui)

| Action | File |
|--------|------|
| MODIFY | `src/pages/pos/POSConfigPage.tsx` |
| CREATE | `src/components/pos/FeatureToggles.tsx` |

---

## Dependencies

- Existing `POS_TenantConfiguration` model
- Existing `POS_Screen` model
- Existing `POS_BusinessSector` enum
- POS terminal sync infrastructure
- Tenant configuration UI

---

## Success Criteria

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

---

## Key Design Decisions

1. **Integration over duplication** - Use existing `POS_TenantConfiguration` flags as source of truth, `POS_Feature` adds structure
2. **configKey pattern** - Each feature links to its tenant config flag via `configKey`, enabling dynamic lookup
3. **Core features** - Features with `isCore: true` or `configKey: null` are always enabled
4. **Screen grouping** - Screens belong to features via `featureId`, enabling bulk enable/disable
5. **Backward compatible** - Existing authentication response format unchanged
