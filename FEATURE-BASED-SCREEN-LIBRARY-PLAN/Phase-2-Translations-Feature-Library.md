# Phase 2: Translations - Feature Library

**Time:** 1 hour
**Type:** Backend (Translations)

Add database translations for feature names, descriptions, and UI labels.

---

## Pre-Flight Checklist

- [ ] Phase 1 completed (schema exists)
- [ ] Database access available
- [ ] Translations table exists

---

## 1. Feature Translations

**File:** `wadi-dms-api/sql/pos-feature-translations.sql`

```sql
-- POS Feature Library Translations
-- Namespace: pos

-- =====================
-- FEATURE NAMES
-- =====================

INSERT INTO translations (key, language, value, namespace) VALUES
-- English
('pos.feature.auth.name', 'en', 'Authentication', 'pos'),
('pos.feature.checkout.name', 'en', 'Checkout', 'pos'),
('pos.feature.payment.name', 'en', 'Payments', 'pos'),
('pos.feature.shifts.name', 'en', 'Shift Management', 'pos'),
('pos.feature.drafts.name', 'en', 'Draft Orders', 'pos'),
('pos.feature.returns.name', 'en', 'Returns & Refunds', 'pos'),
('pos.feature.reports.name', 'en', 'Reports', 'pos'),
('pos.feature.settings.name', 'en', 'Settings', 'pos'),
('pos.feature.discounts.name', 'en', 'Discounts', 'pos'),

-- Arabic
('pos.feature.auth.name', 'ar', 'المصادقة', 'pos'),
('pos.feature.checkout.name', 'ar', 'نقطة البيع', 'pos'),
('pos.feature.payment.name', 'ar', 'المدفوعات', 'pos'),
('pos.feature.shifts.name', 'ar', 'إدارة الورديات', 'pos'),
('pos.feature.drafts.name', 'ar', 'الطلبات المعلقة', 'pos'),
('pos.feature.returns.name', 'ar', 'المرتجعات والاسترداد', 'pos'),
('pos.feature.reports.name', 'ar', 'التقارير', 'pos'),
('pos.feature.settings.name', 'ar', 'الإعدادات', 'pos'),
('pos.feature.discounts.name', 'ar', 'الخصومات', 'pos'),

-- French
('pos.feature.auth.name', 'fr', 'Authentification', 'pos'),
('pos.feature.checkout.name', 'fr', 'Caisse', 'pos'),
('pos.feature.payment.name', 'fr', 'Paiements', 'pos'),
('pos.feature.shifts.name', 'fr', 'Gestion des Quarts', 'pos'),
('pos.feature.drafts.name', 'fr', 'Commandes en Attente', 'pos'),
('pos.feature.returns.name', 'fr', 'Retours et Remboursements', 'pos'),
('pos.feature.reports.name', 'fr', 'Rapports', 'pos'),
('pos.feature.settings.name', 'fr', 'Paramètres', 'pos'),
('pos.feature.discounts.name', 'fr', 'Remises', 'pos')
ON CONFLICT (key, language) DO UPDATE SET value = EXCLUDED.value, updated_at = NOW();

-- =====================
-- FEATURE DESCRIPTIONS
-- =====================

INSERT INTO translations (key, language, value, namespace) VALUES
-- English
('pos.feature.auth.description', 'en', 'Terminal and operator authentication', 'pos'),
('pos.feature.checkout.description', 'en', 'Product scanning and cart management', 'pos'),
('pos.feature.payment.description', 'en', 'Cash, card, and mobile payment processing', 'pos'),
('pos.feature.shifts.description', 'en', 'Open and close cashier shifts with reconciliation', 'pos'),
('pos.feature.drafts.description', 'en', 'Save and recall held orders', 'pos'),
('pos.feature.returns.description', 'en', 'Process product returns and refunds', 'pos'),
('pos.feature.reports.description', 'en', 'X-Report and Z-Report generation', 'pos'),
('pos.feature.settings.description', 'en', 'Terminal and printer configuration', 'pos'),
('pos.feature.discounts.description', 'en', 'Apply manual and promotional discounts', 'pos'),

-- Arabic
('pos.feature.auth.description', 'ar', 'مصادقة المحطة والمشغل', 'pos'),
('pos.feature.checkout.description', 'ar', 'مسح المنتجات وإدارة السلة', 'pos'),
('pos.feature.payment.description', 'ar', 'معالجة الدفع نقداً والبطاقة والجوال', 'pos'),
('pos.feature.shifts.description', 'ar', 'فتح وإغلاق ورديات الكاشير مع المطابقة', 'pos'),
('pos.feature.drafts.description', 'ar', 'حفظ واستعادة الطلبات المعلقة', 'pos'),
('pos.feature.returns.description', 'ar', 'معالجة مرتجعات المنتجات والاسترداد', 'pos'),
('pos.feature.reports.description', 'ar', 'إنشاء تقارير X و Z', 'pos'),
('pos.feature.settings.description', 'ar', 'إعدادات المحطة والطابعة', 'pos'),
('pos.feature.discounts.description', 'ar', 'تطبيق الخصومات اليدوية والترويجية', 'pos'),

-- French
('pos.feature.auth.description', 'fr', 'Authentification du terminal et de l''opérateur', 'pos'),
('pos.feature.checkout.description', 'fr', 'Scan de produits et gestion du panier', 'pos'),
('pos.feature.payment.description', 'fr', 'Traitement des paiements en espèces, carte et mobile', 'pos'),
('pos.feature.shifts.description', 'fr', 'Ouverture et fermeture des quarts avec rapprochement', 'pos'),
('pos.feature.drafts.description', 'fr', 'Sauvegarder et rappeler les commandes en attente', 'pos'),
('pos.feature.returns.description', 'fr', 'Traiter les retours et remboursements', 'pos'),
('pos.feature.reports.description', 'fr', 'Génération des rapports X et Z', 'pos'),
('pos.feature.settings.description', 'fr', 'Configuration du terminal et de l''imprimante', 'pos'),
('pos.feature.discounts.description', 'fr', 'Appliquer des remises manuelles et promotionnelles', 'pos')
ON CONFLICT (key, language) DO UPDATE SET value = EXCLUDED.value, updated_at = NOW();

-- =====================
-- UI LABELS
-- =====================

INSERT INTO translations (key, language, value, namespace) VALUES
-- English
('pos.feature.toggle.enable', 'en', 'Enable Feature', 'pos'),
('pos.feature.toggle.disable', 'en', 'Disable Feature', 'pos'),
('pos.feature.core.badge', 'en', 'Core', 'pos'),
('pos.feature.optional.badge', 'en', 'Optional', 'pos'),
('pos.feature.screens.count', 'en', 'screens', 'pos'),
('pos.feature.config.title', 'en', 'Feature Configuration', 'pos'),
('pos.feature.config.subtitle', 'en', 'Enable or disable POS features for this tenant', 'pos'),
('pos.feature.sync.success', 'en', 'Features synchronized successfully', 'pos'),
('pos.feature.sync.error', 'en', 'Failed to sync features', 'pos'),

-- Arabic
('pos.feature.toggle.enable', 'ar', 'تفعيل الميزة', 'pos'),
('pos.feature.toggle.disable', 'ar', 'تعطيل الميزة', 'pos'),
('pos.feature.core.badge', 'ar', 'أساسي', 'pos'),
('pos.feature.optional.badge', 'ar', 'اختياري', 'pos'),
('pos.feature.screens.count', 'ar', 'شاشات', 'pos'),
('pos.feature.config.title', 'ar', 'إعدادات الميزات', 'pos'),
('pos.feature.config.subtitle', 'ar', 'تفعيل أو تعطيل ميزات نقاط البيع لهذا المستأجر', 'pos'),
('pos.feature.sync.success', 'ar', 'تمت مزامنة الميزات بنجاح', 'pos'),
('pos.feature.sync.error', 'ar', 'فشل مزامنة الميزات', 'pos'),

-- French
('pos.feature.toggle.enable', 'fr', 'Activer la Fonctionnalité', 'pos'),
('pos.feature.toggle.disable', 'fr', 'Désactiver la Fonctionnalité', 'pos'),
('pos.feature.core.badge', 'fr', 'Principal', 'pos'),
('pos.feature.optional.badge', 'fr', 'Optionnel', 'pos'),
('pos.feature.screens.count', 'fr', 'écrans', 'pos'),
('pos.feature.config.title', 'fr', 'Configuration des Fonctionnalités', 'pos'),
('pos.feature.config.subtitle', 'fr', 'Activer ou désactiver les fonctionnalités POS pour ce locataire', 'pos'),
('pos.feature.sync.success', 'fr', 'Fonctionnalités synchronisées avec succès', 'pos'),
('pos.feature.sync.error', 'fr', 'Échec de la synchronisation des fonctionnalités', 'pos')
ON CONFLICT (key, language) DO UPDATE SET value = EXCLUDED.value, updated_at = NOW();
```

---

## 2. Run SQL

```bash
cd wadi-dms-api
psql -h localhost -U postgres -d wadi_dms -f sql/pos-feature-translations.sql
```

Or via API/Prisma:

```bash
npx ts-node -e "
const { PrismaClient } = require('@prisma/client');
const fs = require('fs');
const prisma = new PrismaClient();
const sql = fs.readFileSync('sql/pos-feature-translations.sql', 'utf8');
prisma.\$executeRawUnsafe(sql).then(() => {
  console.log('Translations inserted');
  prisma.\$disconnect();
}).catch(e => {
  console.error(e);
  prisma.\$disconnect();
});
"
```

---

## 3. Verification

```bash
# Verify translations count
psql -h localhost -U postgres -d wadi_dms -c "
  SELECT language, COUNT(*)
  FROM translations
  WHERE namespace = 'pos' AND key LIKE 'pos.feature.%'
  GROUP BY language;
"
# Expected: en=27, ar=27, fr=27

# Test translation lookup
psql -h localhost -U postgres -d wadi_dms -c "
  SELECT key, value
  FROM translations
  WHERE key = 'pos.feature.returns.name';
"
```

---

## Success Criteria

- [ ] 27 translations per language (81 total for pos.feature.*)
- [ ] All 9 features have name translations in en, ar, fr
- [ ] All 9 features have description translations
- [ ] UI labels translated (toggle, badges, config)

---

## Rollback

```sql
DELETE FROM translations WHERE namespace = 'pos' AND key LIKE 'pos.feature.%';
```

---

## Next Phase

Read and follow **Phase-3-Backend-Feature-Query-Handler.md**
