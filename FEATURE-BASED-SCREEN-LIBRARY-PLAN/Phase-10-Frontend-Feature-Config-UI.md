# Phase 10: Frontend - Feature Config UI

**Time:** 2 hours
**Type:** Frontend (React)

Add feature toggles to POS tenant configuration page.

---

## Pre-Flight Checklist

- [ ] Phase 5 completed (backend API ready)
- [ ] POSConfigPage exists
- [ ] Tanstack Query configured
- [ ] Tailwind CSS available

---

## 1. Tests First (TDD)

**File:** `wadi-dms-ui/src/pages/pos/__tests__/POSFeatureConfig.test.tsx`

```tsx
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { FeatureToggles } from '../components/FeatureToggles';
import { I18nextProvider } from 'react-i18next';
import i18n from '../../../i18n';

const mockFeatures = [
  {
    featureId: 'checkout',
    name: 'Checkout',
    isCore: true,
    enabled: true,
    configKey: null,
    screens: [{ screenId: 'checkout', name: 'Checkout' }],
  },
  {
    featureId: 'returns',
    name: 'Returns & Refunds',
    isCore: false,
    enabled: true,
    configKey: 'allowReturns',
    screens: [
      { screenId: 'return-entry', name: 'Return Entry' },
      { screenId: 'return-items', name: 'Return Items' },
    ],
  },
  {
    featureId: 'drafts',
    name: 'Draft Orders',
    isCore: false,
    enabled: false,
    configKey: 'allowDrafts',
    screens: [{ screenId: 'save-draft', name: 'Save Draft' }],
  },
];

const queryClient = new QueryClient({
  defaultOptions: { queries: { retry: false } },
});

const renderWithProviders = (ui: React.ReactElement) => {
  return render(
    <QueryClientProvider client={queryClient}>
      <I18nextProvider i18n={i18n}>{ui}</I18nextProvider>
    </QueryClientProvider>
  );
};

describe('FeatureToggles', () => {
  const mockOnToggle = jest.fn();

  beforeEach(() => {
    mockOnToggle.mockClear();
  });

  it('renders all features', () => {
    renderWithProviders(
      <FeatureToggles features={mockFeatures} onToggle={mockOnToggle} />
    );

    expect(screen.getByText('Checkout')).toBeInTheDocument();
    expect(screen.getByText('Returns & Refunds')).toBeInTheDocument();
    expect(screen.getByText('Draft Orders')).toBeInTheDocument();
  });

  it('shows Core badge for core features', () => {
    renderWithProviders(
      <FeatureToggles features={mockFeatures} onToggle={mockOnToggle} />
    );

    const coreBadges = screen.getAllByText(/Core|أساسي/);
    expect(coreBadges.length).toBeGreaterThan(0);
  });

  it('disables toggle for core features', () => {
    renderWithProviders(
      <FeatureToggles features={mockFeatures} onToggle={mockOnToggle} />
    );

    // Core feature toggle should be disabled
    const checkoutToggle = screen.getByRole('switch', { name: /checkout/i });
    expect(checkoutToggle).toBeDisabled();
  });

  it('allows toggling optional features', async () => {
    renderWithProviders(
      <FeatureToggles features={mockFeatures} onToggle={mockOnToggle} />
    );

    const draftsToggle = screen.getByRole('switch', { name: /draft/i });
    fireEvent.click(draftsToggle);

    expect(mockOnToggle).toHaveBeenCalledWith('allowDrafts', true);
  });

  it('shows screen count per feature', () => {
    renderWithProviders(
      <FeatureToggles features={mockFeatures} onToggle={mockOnToggle} />
    );

    expect(screen.getByText(/2\s*screens/i)).toBeInTheDocument();
    expect(screen.getByText(/1\s*screen/i)).toBeInTheDocument();
  });
});
```

**Run (expect fail):**

```bash
cd wadi-dms-ui
npm test -- POSFeatureConfig.test.tsx
```

---

## 2. Types

**File:** `wadi-dms-ui/src/types/pos/feature.types.ts`

```typescript
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
  businessSectors: string[];
  screens: FeatureScreenDto[];
}

export interface FeaturesResponse {
  features: FeatureDto[];
  version: string;
  syncedAt: string;
}
```

---

## 3. API Service

**File:** `wadi-dms-ui/src/services/pos/feature.service.ts`

```typescript
import { api } from '../api';
import type { FeatureDto, FeaturesResponse } from '../../types/pos/feature.types';

const BASE_URL = '/api/pos/features';

export const featureService = {
  /**
   * List all features (admin)
   */
  async getFeatures(): Promise<FeatureDto[]> {
    const response = await api.get<{ features: FeatureDto[] }>(BASE_URL);
    return response.data.features;
  },

  /**
   * Get single feature
   */
  async getFeature(featureId: string): Promise<FeatureDto> {
    const response = await api.get<{ feature: FeatureDto }>(
      `${BASE_URL}/${featureId}`
    );
    return response.data.feature;
  },

  /**
   * Update feature metadata
   */
  async updateFeature(
    featureId: string,
    data: Partial<Pick<FeatureDto, 'name' | 'nameAr' | 'description' | 'icon'>>
  ): Promise<FeatureDto> {
    const response = await api.put<{ feature: FeatureDto }>(
      `${BASE_URL}/${featureId}`,
      data
    );
    return response.data.feature;
  },
};
```

---

## 4. Feature Toggle Component

**File:** `wadi-dms-ui/src/components/pos/FeatureToggles.tsx`

```tsx
import React from 'react';
import { useTranslation } from 'react-i18next';
import { Switch } from '@headlessui/react';
import { LucideIcon, Settings, ShoppingCart, RotateCcw, FileText, BarChart, Percent, Clock, Key } from 'lucide-react';
import type { FeatureDto } from '../../types/pos/feature.types';

const iconMap: Record<string, LucideIcon> = {
  'settings': Settings,
  'shopping-cart': ShoppingCart,
  'rotate-ccw': RotateCcw,
  'file-text': FileText,
  'bar-chart': BarChart,
  'percent': Percent,
  'clock': Clock,
  'key': Key,
};

interface FeatureTogglesProps {
  features: FeatureDto[];
  onToggle: (configKey: string, enabled: boolean) => void;
  loading?: boolean;
}

export function FeatureToggles({ features, onToggle, loading }: FeatureTogglesProps) {
  const { t, i18n } = useTranslation();
  const isRTL = i18n.language === 'ar';

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <h3 className="text-lg font-medium text-gray-900 dark:text-gray-100">
          {t('pos.feature.config.title')}
        </h3>
        <span className="text-sm text-gray-500">
          {features.filter(f => f.enabled).length} / {features.length} {t('pos.feature.toggle.enable').toLowerCase()}d
        </span>
      </div>

      <p className="text-sm text-gray-500 dark:text-gray-400">
        {t('pos.feature.config.subtitle')}
      </p>

      <div className="divide-y divide-gray-200 dark:divide-gray-700">
        {features.map((feature) => (
          <FeatureToggleRow
            key={feature.featureId}
            feature={feature}
            onToggle={onToggle}
            loading={loading}
            isRTL={isRTL}
          />
        ))}
      </div>
    </div>
  );
}

interface FeatureToggleRowProps {
  feature: FeatureDto;
  onToggle: (configKey: string, enabled: boolean) => void;
  loading?: boolean;
  isRTL: boolean;
}

function FeatureToggleRow({ feature, onToggle, loading, isRTL }: FeatureToggleRowProps) {
  const { t, i18n } = useTranslation();
  const Icon = feature.icon ? iconMap[feature.icon] : Settings;
  const displayName = i18n.language === 'ar' && feature.nameAr ? feature.nameAr : feature.name;

  const handleToggle = (enabled: boolean) => {
    if (feature.configKey && !feature.isCore) {
      onToggle(feature.configKey, enabled);
    }
  };

  return (
    <div className="flex items-center justify-between py-4">
      <div className={`flex items-center gap-3 ${isRTL ? 'flex-row-reverse' : ''}`}>
        <div className="flex-shrink-0">
          <div className={`
            w-10 h-10 rounded-lg flex items-center justify-center
            ${feature.enabled ? 'bg-primary-100 text-primary-600' : 'bg-gray-100 text-gray-400'}
          `}>
            <Icon className="w-5 h-5" />
          </div>
        </div>

        <div>
          <div className="flex items-center gap-2">
            <span className="font-medium text-gray-900 dark:text-gray-100">
              {displayName}
            </span>
            {feature.isCore && (
              <span className="px-2 py-0.5 text-xs font-medium bg-blue-100 text-blue-700 rounded">
                {t('pos.feature.core.badge')}
              </span>
            )}
          </div>
          <p className="text-sm text-gray-500">
            {feature.screens.length} {t('pos.feature.screens.count')}
          </p>
        </div>
      </div>

      <Switch
        checked={feature.enabled}
        onChange={handleToggle}
        disabled={feature.isCore || loading}
        aria-label={displayName}
        className={`
          ${feature.enabled ? 'bg-primary-600' : 'bg-gray-200'}
          ${feature.isCore ? 'opacity-50 cursor-not-allowed' : 'cursor-pointer'}
          relative inline-flex h-6 w-11 items-center rounded-full transition-colors
          focus:outline-none focus:ring-2 focus:ring-primary-500 focus:ring-offset-2
        `}
      >
        <span
          className={`
            ${feature.enabled ? (isRTL ? 'translate-x-1' : 'translate-x-6') : (isRTL ? 'translate-x-6' : 'translate-x-1')}
            inline-block h-4 w-4 transform rounded-full bg-white transition-transform
          `}
        />
      </Switch>
    </div>
  );
}
```

---

## 5. Integrate into POSConfigPage

**File:** `wadi-dms-ui/src/pages/pos/POSConfigPage.tsx`

Add features section:

```tsx
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { featureService } from '../../services/pos/feature.service';
import { posConfigService } from '../../services/pos/pos-config.service';
import { FeatureToggles } from '../../components/pos/FeatureToggles';
import { toast } from 'react-hot-toast';

export function POSConfigPage() {
  const queryClient = useQueryClient();
  const { t } = useTranslation();

  // Existing config query
  const { data: config } = useQuery({
    queryKey: ['pos-config', tenantId],
    queryFn: () => posConfigService.getConfig(tenantId),
  });

  // Features query
  const { data: features = [], isLoading: featuresLoading } = useQuery({
    queryKey: ['pos-features'],
    queryFn: () => featureService.getFeatures(),
  });

  // Update config mutation
  const updateConfigMutation = useMutation({
    mutationFn: (data: Partial<POSConfig>) =>
      posConfigService.updateConfig(tenantId, data),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['pos-config'] });
      queryClient.invalidateQueries({ queryKey: ['pos-features'] });
      toast.success(t('pos.feature.sync.success'));
    },
    onError: () => {
      toast.error(t('pos.feature.sync.error'));
    },
  });

  // Handle feature toggle
  const handleFeatureToggle = (configKey: string, enabled: boolean) => {
    updateConfigMutation.mutate({ [configKey]: enabled });
  };

  // Merge enabled status from config into features
  const featuresWithStatus = features.map((feature) => ({
    ...feature,
    enabled: feature.isCore || (feature.configKey ? config?.[feature.configKey] ?? true : true),
  }));

  return (
    <div className="space-y-8">
      {/* Existing config sections */}

      {/* Features Section */}
      <section className="bg-white dark:bg-gray-800 rounded-lg shadow p-6">
        <FeatureToggles
          features={featuresWithStatus}
          onToggle={handleFeatureToggle}
          loading={updateConfigMutation.isPending}
        />
      </section>
    </div>
  );
}
```

---

## 6. Update Exports

**File:** `wadi-dms-ui/src/types/pos/index.ts`

```typescript
export * from './feature.types';
```

**File:** `wadi-dms-ui/src/services/pos/index.ts`

```typescript
export * from './feature.service';
```

---

## 7. Verification

```bash
cd wadi-dms-ui

# Run tests
npm test -- POSFeatureConfig.test.tsx

# Type check
npm run typecheck

# Start dev server
npm run dev
# Navigate to POS config page and test feature toggles
```

---

## Success Criteria

- [ ] Tests pass
- [ ] Features displayed with toggles
- [ ] Core features show badge and disabled toggle
- [ ] Optional features can be toggled
- [ ] Toggle updates tenant config
- [ ] Screen count shown per feature
- [ ] RTL layout works (Arabic)
- [ ] `npm run typecheck` passes

---

## Rollback

```bash
rm src/components/pos/FeatureToggles.tsx
rm src/types/pos/feature.types.ts
rm src/services/pos/feature.service.ts
# Revert POSConfigPage changes
git checkout -- src/pages/pos/POSConfigPage.tsx
```

---

## Next Phase

Read and follow **Phase-11-Integration-E2E-Tests.md**
