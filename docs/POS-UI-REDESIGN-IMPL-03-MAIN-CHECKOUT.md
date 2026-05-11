# POS UI Redesign — IMPL 03: Main checkout screen (skeleton)

**Status:** Implementation plan, ready for Worker stream.
**Date:** 2026-05-11.
**Owner:** Abdu.
**Supersedes:** none (legacy `ui/screens/checkout/main.slint` stays running until Plan 4 migrates routing).
**Design source:** `docs/POS-UI-REDESIGN.md §5.1` (the four-zone checkout layout).
**Depends on:** Plan 1 (tokens, fonts, Layout helpers) and Plan 2 (eight atomic components). Both shipped.

---

## 1. Goal

Compose the eight atomic components from Plan 2 into the main checkout screen described in §5.1 of the redesign spec, as a **skeleton with mock data**:

- All four zones (categories rail, products area, operations column, cart panel) plus full-width header and footer render in a new, parallel screen file. The legacy `MainCheckoutScreen` is not touched.
- Mock data is provided in-screen; the shape of each mock record mirrors the corresponding Rust domain type (`pos_models::Product`, `pos_models::CartItem`, `pos_models::Category`) one-to-one, so Plan 4's wiring step is a 1:1 substitution and not a reshape.
- The new screen is reachable only through a new dev binary entry: `cargo run -- --checkout-preview`. The cashier-facing app still routes to the legacy screen. Plan 4 will flip the routing.

Out of scope here: any binding to `CartService`, `ProductService`, `SyncService`, hardware (scanner/printer), payment screens (§5.2), success modal (§5.3), or state-design overlays (§6.x). Static surfaces only.

---

## 2. What already exists

Verify these before starting; the document references them by file path:

- **Token globals** (Plan 1):
  - `Theme` (`mode: "light"|"dark"`, `is-dark`, `is-light`) — `ui/tokens/theme.slint`.
  - `Surfaces` (4 tiers: bg / panel / surface / inset top, bottom, border, shadow) — `ui/tokens/surfaces.slint`.
  - `Fonts` (sans, sans-arabic, mono family names) — `ui/tokens/fonts.slint`.
  - `Layout` (rtl flag, `is-rtl`, `is-ltr`, scalar/colour `leading()`/`trailing()` helpers) — defined in `ui/theme.slint`.
  - `Locale` (current: `"ar"|"en"|"fr"`) — defined in `ui/theme.slint`.
  - `Colors`, `Typography`, `Spacing`, `Sizes`, `Radius`, `Animation`, `Responsive` — defined in `ui/theme.slint`.
- **Atomic components** (Plan 2), all in `ui/components/atomic/`:
  - `Panel { content-padding }` — tier-2 surface with @children slot.
  - `Button { label, variant: "primary"|"secondary"|"danger"|"ghost", disabled; clicked }`.
  - `SearchInput { placeholder, value <-> string; changed(string), cleared, submitted(string) }`.
  - `OpsButton { glyph, label, variant: "primary"|"neutral"|"danger", disabled; clicked }`.
  - `StatusLED { state: "online"|"offline"|"syncing" }` — pulse driven by an internal Timer when syncing.
  - `PayButton { label, total, currency, disabled; clicked }`.
  - `ProductTile { name, price, currency, category-accent: color, disabled, out-of-stock; clicked }`.
  - `CartLine { name, qty: int, unit-price, line-total, currency, selected; clicked }`.
  - Exported from `ui/components/atomic/mod.slint`.
- **Dev binary pattern** (Plan 1 + Plan 2):
  - `cargo run -- --theme-harness` → `src/dev_harness.rs` → `ThemeHarnessWindow` re-exported from `ui/main.slint`.
  - `cargo run -- --component-gallery` → `src/component_gallery.rs` → `ComponentGalleryWindow` re-exported from `ui/main.slint`.
  - The argv match lives in `src/main.rs` near lines 44–51.
  - Initial state in `component_gallery.rs` reads `crate::locale_detect::detect_locale()`.
- **Rust domain models** (no changes needed for this plan, but the mock data must mirror them):
  - `pos_models::product::Product` — `id, sku, barcode, name, name_ar, description, price (Decimal), cost, tax_rate, tax_inclusive, category_id, category_name, unit (ProductUnit), stock_qty, min_stock, allow_negative_stock, image_url, is_weighable, is_serialized, is_active, product_type, track_inventory, product_nature`.
  - `pos_models::product::Category` — `id, parent_id, name, name_ar, color, icon, image_url, display_order, is_active`.
  - `pos_models::cart::CartItem` — `id, product_id, product_name, product_name_ar, sku, barcode, quantity (Decimal), unit, unit_price (Decimal), tax_rate, tax_inclusive, discount_amount, discount_percent, product_type, track_inventory, note, line_subtotal (Decimal), line_tax (Decimal), line_total (Decimal), line_discount (Decimal)`.
  - `pos_models::cart::Cart` — `items, customer_id, customer_name, customer_balance, subtotal (Decimal), tax_total (Decimal), discount_total (Decimal), grand_total (Decimal), cart_discount_percent, cart_discount_amount, note`.
- **Legacy screen** (do not modify in this plan): `ui/screens/checkout/main.slint` exports `MainCheckoutScreen` and is currently wired into `ui/main.slint`. It stays as the production checkout screen until Plan 4.

---

## 3. Architecture in plain English

The cashier opens the checkout screen after starting a shift (or after a completed sale). The screen renders five regions: a full-width header strip, then four side-by-side zones, then a full-width footer strip.

- The **header** carries the brand mark, terminal ID, current operator, online/offline LED, and a clock. Its state is fed by mock data in skeleton mode.
- The **categories rail** is the leading-edge column (left in LTR, right in RTL). It is a vertical strip of icon buttons: an "All" icon at the top, then the tenant's 4–8 top categories, then a "⋯" overflow that opens a sheet (sheet is out of scope for skeleton; the button is rendered but its `clicked` handler is a no-op log). The bottom of the rail pins two icons: back/exit and settings.
- The **products area** is the largest zone. It has a search row at the top (`SearchInput` plus two action buttons: PLU and Price-check) and a product grid below. The grid uses four columns by default. Each tile is a `ProductTile` whose `category-accent` resolves to the tile's owning category colour.
- The **operations column** is six `OpsButton` instances stacked vertically in fixed order: `+1 / −1 / ×n / % / ✎ / ⌫`. The first variant is `primary` (lime), the last is `danger`; the middle four are `neutral`. Above the column, a small caption reads "ON SELECTED" (Arabic equivalent in RTL). Below, a single "MORE" button.
- The **cart panel** is the trailing-edge column. It is a `Panel` containing: optional customer chip, a vertical list of `CartLine` instances, a totals block (subtotal, tax, total), and a `PayButton` strip at the bottom. The most-recently-added line is auto-selected on add; tapping any line moves the selection. In skeleton mode "auto-select" is approximated by initializing `selected-line-id` to the last item in the mock cart.
- The **footer** carries the offline-queue summary and the global "MORE" overflow.

Data flow inside the skeleton:

1. The screen owns the mock arrays (`mock-categories`, `mock-products`, `mock-cart-lines`) and a few scalar properties (`selected-line-id`, `header-state`, `footer-state`).
2. Tapping a `ProductTile` fires the screen-level callback `product-tapped(id: string)`; in preview, the handler logs the id (no real `add_item` call).
3. Tapping a `CartLine` sets `selected-line-id` to that line's id locally — this is the only state mutation handled inside the screen itself.
4. Tapping any `OpsButton` fires the screen-level callback `op-requested(op: string, line-id: string)`; in preview the handler logs.
5. Tapping `PayButton` fires `pay-pressed`; in preview the handler logs.

The screen is otherwise pure and free of side effects. All real wiring lands in Plan 4.

---

## 4. Conventions

These are the rules every task in this plan must follow. They are documented here rather than re-quoted per task.

- **RTL mirroring — preferred pattern.** When a horizontal layout has children that need to swap order between LTR and RTL, prefer **`if Layout.is-rtl: Element { … }`** branches that *remove* children from the layout, over `visible: false` dual-slot tricks. The PayButton retrofit in Plan 2 confirmed that `visible: false` slots interact badly with `alignment: space-between` (the invisible slot still consumes a space-between gap). The `if`-gated form is the default for new code in Plan 3 and later. Exceptions allowed only for plain `HorizontalLayout` with no `space-between` alignment, where the dual-slot trick has no observable bug; in that case either form is acceptable but a comment must explain why.
- **Press-feedback opacity.** Plan 2 verification reported the press affordance on `Button`, `OpsButton`, and `PayButton` as "difficult to notice" (dip from 1.0 → 0.85–0.88). Task 0 of this plan retunes the dip to `0.70` across all three components. New touchable components introduced in Plan 3 follow the same target.
- **No edits to legacy components.** `ui/components/*.slint` (top-level, non-atomic) and `ui/screens/checkout/main.slint` are off-limits for this plan. The new screen lives under `ui/screens/checkout/` alongside the legacy file, with distinct names so both compile in the same binary.
- **No globals defined in this plan.** All new state lives as in-out properties on screen components. If a piece of state ends up needed by Plan 4 wiring, Plan 4 introduces the global. We do not pre-introduce one in skeleton mode.
- **One component per file.** Same rule as Plan 2.
- **Numerics are pre-formatted strings.** `Decimal` → display string conversion is the binding step's responsibility. In skeleton mode every numeric field on a mock record is a hand-written string like `"12.500"`. The atomic components already accept strings for `price`, `unit-price`, `line-total`, and `total`, so this is consistent.
- **Locale resolution at the binding boundary for data, not chrome.** *Data* fields (product names, category names, cart-line names) are locale-resolved at the binding boundary — in the preview window's mock-data initializer for Plan 3, in the Rust binding layer for Plan 4 — and the screen consumes already-resolved strings. *UI chrome* strings (column captions like "ON SELECTED", totals labels like "Subtotal" / "Tax" / "Total", button labels like "PAY" / "MORE") may branch on `Locale.current` inline at their definition site; passing each one in as a property would explode the screen surface without buying anything. The split keeps domain data out of the component tree while letting chrome stay readable.
- **Per-Task commit.** Every task ends with one commit on `worktree-pos-ui-redesign-foundation`. No squash. Commit subject template: `feat(ui): plan 3 task N — <short description>`. Exception: Task 0 is a fix, subject `fix(ui): tune atomic press opacity to 0.70 for legibility`. Each commit uses a HEREDOC body explaining the *why* (per Abdu's CLAUDE.md "Commit body: why, not what") and ends with the trailer `Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>` (per repo convention used across Plans 1 and 2). The exact form is shown in each Task's commit step below; do not collapse to `git commit -m "<subject>"`.

---

## 5. Data shapes — Slint structs that mirror Rust domain types

Three Slint structs live in `ui/screens/checkout/checkout_types.slint`. They are introduced in Task 1 and consumed by Tasks 2–7.

Each struct only carries the subset of the corresponding Rust type that the screen actually displays. The remaining Rust fields stay in `CartService`/`ProductService` until Plan 4 needs them. Field names use **snake_case** in Slint structs to match the existing legacy structs (`CartItemData`, `ProductData`, `CategoryData` use `snake_case` — see `ui/components/cart_item.slint`, `ui/components/product_tile.slint`, `ui/components/category_tile.slint`).

```slint
// Mirrors pos_models::cart::CartItem subset.
// id            ← CartItem.id
// product_id    ← CartItem.product_id
// name          ← CartItem.product_name OR product_name_ar (resolved by binding step)
// qty           ← CartItem.quantity.to_i32() (skeleton uses int; fractional-qty
//                 weighable items will need a string-format split in Plan 4)
// unit_price    ← CartItem.unit_price formatted to 3 decimals
// line_total    ← CartItem.line_total formatted to 3 decimals
// currency      ← Cart.currency (per-cart in real wiring; duplicated per line
//                 here to match the CartLine atomic API which takes currency
//                 per instance)
export struct CheckoutLineData {
    id: string,
    product_id: string,
    name: string,
    qty: int,
    unit_price: string,
    line_total: string,
    currency: string,
}
```

```slint
// Mirrors pos_models::product::Product subset.
// id              ← Product.id
// name            ← Product.name OR name_ar (resolved by binding step)
// price           ← Product.price formatted to 3 decimals
// currency        ← Cart.currency (or tenant config in real wiring)
// category_id     ← Product.category_id
// category_accent ← resolved at the binding boundary from Product.category_id
//                   to one of Colors.cat-* (coffee/bakery/cold/food/...)
// disabled        ← !Product.is_active
// out_of_stock    ← Product.stock_qty <= 0 && !Product.allow_negative_stock
//                   && Product.track_inventory
export struct CheckoutTileData {
    id: string,
    name: string,
    price: string,
    currency: string,
    category_id: string,
    category_accent: color,
    disabled: bool,
    out_of_stock: bool,
}
```

```slint
// Mirrors pos_models::product::Category subset.
// id        ← Category.id
// name      ← Category.name OR name_ar (resolved by binding step)
// icon      ← Category.icon (a glyph string; in skeleton we use Unicode
//             pictographs; in real wiring this becomes either a glyph or
//             an asset path, TBD by tenant config)
// accent    ← Category.color parsed to color, falling back to a default
export struct CheckoutCategoryData {
    id: string,
    name: string,
    icon: string,
    accent: color,
}
```

A reviewer should be able to map every field on every struct above to a specific Rust field. If a field appears here that has no counterpart in Rust, that is a bug in this plan — surface it before continuing.

---

## 6. File layout

New files this plan creates:

```
ui/screens/checkout/
    checkout_types.slint              (Task 1)
    categories_rail.slint             (Task 2)
    products_area.slint               (Task 3)
    ops_column.slint                  (Task 4)
    cart_panel_area.slint             (Task 5)
    checkout_chrome.slint             (Task 6)  — Header + Footer subcomponents
    checkout_v2.slint                 (Task 7)  — composes all the above

ui/screens/dev/
    checkout_preview.slint            (Task 8)  — Rectangle with toolbar + mock data
    checkout_preview_window.slint     (Task 8)  — Window wrapper

src/
    checkout_preview.rs               (Task 9)  — argv entry, locale init, callbacks
```

Files this plan modifies:

```
ui/components/atomic/button.slint     (Task 0, opacity 0.85 → 0.70)
ui/components/atomic/ops_button.slint (Task 0, opacity 0.85 → 0.70)
ui/components/atomic/pay_button.slint (Task 0, opacity 0.85 → 0.70)

ui/screens/dev/mod.slint              (Task 8, add 2 exports)
ui/main.slint                         (Task 9, add 1 export re-shipment)
src/main.rs                           (Task 9, declare `mod checkout_preview;` + add `--checkout-preview` argv branch)

docs/POS-UI-REDESIGN-FINDINGS-01-SLINT-RTL.md (Task 10, append verification matrix)
```

Files this plan **does not touch**:

- `ui/components/*.slint` except the three atomic files explicitly listed.
- `ui/screens/checkout/main.slint` (legacy).
- `ui/screens/checkout/search.slint` (legacy product search).
- Anything under `crates/*`.

---

## 7. Tasks

### Task 0: Retune press-feedback opacity on three atomic components

Plan 2 verification flagged the press dip as "difficult to notice" on a 24″ cashier display. Drop the pressed-state opacity from `0.85` / `0.88` to `0.70` across `Button`, `OpsButton`, and `PayButton`. No structural changes — three single-character edits.

**Files:**
- Modify: `ui/components/atomic/button.slint:30`
- Modify: `ui/components/atomic/ops_button.slint:29`
- Modify: `ui/components/atomic/pay_button.slint:38`

- [ ] **Step 1: Verify current opacities**

Run: `grep -n "opacity: 0\." ui/components/atomic/button.slint ui/components/atomic/ops_button.slint ui/components/atomic/pay_button.slint`
Expected: at least three lines, each ending in `opacity: 0.85;` or `opacity: 0.88;` inside a `states [` block.

- [ ] **Step 2: Apply edit to Button**

Change the line inside the `states [ pressed when touch.pressed && !disabled: { … } ]` block in `ui/components/atomic/button.slint` from `opacity: 0.85;` to `opacity: 0.70;`. Leave the adjacent `inner-press-scale: 0.97;` unchanged.

- [ ] **Step 3: Apply edit to OpsButton**

Change the line inside the `states [ pressed … ]` block in `ui/components/atomic/ops_button.slint` from `opacity: 0.85;` to `opacity: 0.70;`.

- [ ] **Step 4: Apply edit to PayButton**

Change the line inside the `states [ pressed … ]` block in `ui/components/atomic/pay_button.slint` from `opacity: 0.85;` to `opacity: 0.70;`.

- [ ] **Step 5: Verify edits**

Run: `grep -n "opacity: 0.70" ui/components/atomic/button.slint ui/components/atomic/ops_button.slint ui/components/atomic/pay_button.slint`
Expected: exactly three matches, one per file.

- [ ] **Step 6: Compile check**

Run: `cargo check -p e2manage-pos-terminal 2>&1 | tail -20`
Expected: clean exit, no warnings. CI is gated on `cargo fmt --all --check` and `cargo clippy --workspace --all-targets -- -D warnings` both passing with no allow-list as of the Plan 2 cleanup; treat any warning as a real regression introduced by this task.

- [ ] **Step 7: Commit**

```bash
git add ui/components/atomic/button.slint ui/components/atomic/ops_button.slint ui/components/atomic/pay_button.slint
git commit -m "$(cat <<'EOF'
fix(ui): tune atomic press opacity to 0.70 for legibility

Plan 2 operator verification reported the pressed dip as too subtle on a
24-inch cashier display. 0.85/0.88 → 0.70 increases the perceived feedback
without affecting variant-specific tones.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
git branch --show-current
```

Expected last line: `worktree-pos-ui-redesign-foundation`.

---

### Task 1: Define `CheckoutLineData`, `CheckoutTileData`, `CheckoutCategoryData`

Create the three Slint structs documented in §5 of this plan. Single new file; nothing else imports it yet — that happens in Tasks 2–5.

**Files:**
- Create: `ui/screens/checkout/checkout_types.slint`

- [ ] **Step 1: Confirm parent directory exists**

Run: `ls ui/screens/checkout/`
Expected: at minimum `main.slint` and `search.slint` are listed. (`mod.slint` may or may not exist; this plan does not depend on it.)

- [ ] **Step 2: Write `checkout_types.slint`**

```slint
// ============================================================================
// Plan 3 — Mock data shapes for the new checkout skeleton.
//
// Each struct mirrors a subset of a Rust domain type from
// `crates/pos-models/src/{cart,product}.rs`. See
// `docs/POS-UI-REDESIGN-IMPL-03-MAIN-CHECKOUT.md §5` for the per-field
// mapping. Field names use snake_case to match the legacy
// CartItemData / ProductData / CategoryData structs already in
// ui/components/.
//
// In skeleton mode (Plan 3) these structs are populated by hand in the
// preview window. In Plan 4, a Rust binding layer will format Decimal
// fields to strings and resolve locale before populating them.
// ============================================================================

export struct CheckoutLineData {
    id: string,
    product_id: string,
    name: string,
    qty: int,
    unit_price: string,
    line_total: string,
    currency: string,
}

export struct CheckoutTileData {
    id: string,
    name: string,
    price: string,
    currency: string,
    category_id: string,
    category_accent: color,
    disabled: bool,
    out_of_stock: bool,
}

export struct CheckoutCategoryData {
    id: string,
    name: string,
    icon: string,
    accent: color,
}
```

- [ ] **Step 3: Compile check**

Run: `cargo check -p e2manage-pos-terminal 2>&1 | tail -10`
Expected: clean. (The file is not yet imported by anything, but Slint's compiler should not complain about an unimported file under the resolved tree.)

- [ ] **Step 4: Commit**

```bash
git add ui/screens/checkout/checkout_types.slint
git commit -m "$(cat <<'EOF'
feat(ui): plan 3 task 1 — checkout mock-data structs

Establishes the data contract between Plan 3 (skeleton) and Plan 4
(wiring). Field names mirror pos_models::{CartItem, Product, Category} so
the wiring step is a 1:1 substitution rather than a reshape — the slicing
decision the operator locked in before this plan started.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
git branch --show-current
```

Expected last line: `worktree-pos-ui-redesign-foundation`.

---

### Task 2: `CategoriesRail`

Vertical icon column. Width 56 dp (matches §5.1's "~52 dp" within the 4 dp grid). Top: "All" icon. Middle: up to 8 categories from the input array. Bottom: an overflow "⋯" if the array would exceed 8 (we render a fixed 8 + overflow), plus pinned back-arrow and settings-cog at the very bottom.

The rail itself is a `Panel`; tiles inside are 56×56 dp rectangles. Each tile shows the category glyph; the selected one carries a 3 dp lime stripe on the trailing-inside edge (so the stripe always faces the products area, not the screen edge).

**Files:**
- Create: `ui/screens/checkout/categories_rail.slint`

- [ ] **Step 1: Write `categories_rail.slint`**

```slint
import { Theme, Surfaces } from "../../tokens/mod.slint";
import { Colors, Typography, Spacing, Radius, Animation, Layout, Locale } from "../../theme.slint";
import { Panel } from "../../components/atomic/mod.slint";
import { CheckoutCategoryData } from "checkout_types.slint";

// CategoriesRail — leading-edge vertical column of category icons.
// Width is fixed at 56 dp. The first tile is always "All" (selected by
// default at startup). Overflow ("⋯") opens a sheet in production; here
// it is a no-op log.
//
// Selection state is owned by the screen above (selected-category-id);
// the rail emits `category-selected(id)` and `overflow-pressed`.
export component CategoriesRail inherits Panel {
    width: 56px;
    content-padding: Spacing.xs;

    in property <[CheckoutCategoryData]> categories: [];
    in property <string> selected-id: "all";

    callback category-selected(string);
    callback overflow-pressed;
    callback back-pressed;
    callback settings-pressed;

    VerticalLayout {
        spacing: Spacing.xs;
        alignment: start;

        // "All" tile, always present.
        all-tile := Rectangle {
            width: 48px;
            height: 48px;
            border-radius: Radius.sm;
            background: root.selected-id == "all" ? Colors.accent-lime : Surfaces.surface-top;
            border-color: Surfaces.surface-border;
            border-width: 1px;

            Text {
                text: "⌗";
                font-family: Typography.font-family-mono;
                font-size: Typography.heading;
                color: root.selected-id == "all" ? #0B0D10 : Colors.text-primary;
                horizontal-alignment: center;
                vertical-alignment: center;
            }

            TouchArea {
                clicked => { root.category-selected("all"); }
            }
        }

        // Category tiles (capped at 8 — overflow goes to the sheet).
        for cat[i] in root.categories: Rectangle {
            visible: i < 8;
            width: 48px;
            height: 48px;
            border-radius: Radius.sm;
            background: root.selected-id == cat.id ? cat.accent : Surfaces.surface-top;
            border-color: Surfaces.surface-border;
            border-width: 1px;

            Text {
                text: cat.icon;
                font-family: Typography.font-family-mono;
                font-size: Typography.heading;
                color: root.selected-id == cat.id ? #0B0D10 : Colors.text-primary;
                horizontal-alignment: center;
                vertical-alignment: center;
            }

            TouchArea {
                clicked => { root.category-selected(cat.id); }
            }
        }

        // Overflow.
        Rectangle {
            visible: root.categories.length > 8;
            width: 48px;
            height: 32px;
            border-radius: Radius.sm;
            background: Surfaces.surface-top;
            border-color: Surfaces.surface-border;
            border-width: 1px;

            Text {
                text: "⋯";
                font-family: Typography.font-family;
                font-size: Typography.heading;
                color: Colors.text-secondary;
                horizontal-alignment: center;
                vertical-alignment: center;
            }

            TouchArea {
                clicked => { root.overflow-pressed(); }
            }
        }

        // Spacer.
        Rectangle { vertical-stretch: 1; }

        // Pinned bottom: back, settings.
        Rectangle {
            width: 48px;
            height: 40px;
            border-radius: Radius.sm;
            background: Surfaces.surface-top;
            border-color: Surfaces.surface-border;
            border-width: 1px;

            Text {
                text: Layout.is-rtl ? "›" : "‹";
                font-family: Typography.font-family;
                font-size: Typography.heading;
                color: Colors.text-secondary;
                horizontal-alignment: center;
                vertical-alignment: center;
            }

            TouchArea {
                clicked => { root.back-pressed(); }
            }
        }

        Rectangle {
            width: 48px;
            height: 40px;
            border-radius: Radius.sm;
            background: Surfaces.surface-top;
            border-color: Surfaces.surface-border;
            border-width: 1px;

            Text {
                text: "⚙";
                font-family: Typography.font-family;
                font-size: Typography.heading;
                color: Colors.text-secondary;
                horizontal-alignment: center;
                vertical-alignment: center;
            }

            TouchArea {
                clicked => { root.settings-pressed(); }
            }
        }
    }
}
```

- [ ] **Step 2: Compile check**

Run: `cargo check -p e2manage-pos-terminal 2>&1 | tail -10`
Expected: clean.

- [ ] **Step 3: Commit**

```bash
git add ui/screens/checkout/categories_rail.slint
git commit -m "$(cat <<'EOF'
feat(ui): plan 3 task 2 — categories rail

Leading-edge column from §5.1. Width 56 dp lands on the 4 dp grid; the
"All" tile, up to eight categories, and an overflow trigger stack
vertically; back and settings tiles pin to the bottom. Selection state is
owned by the screen above so the rail stays pure.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
git branch --show-current
```

---

### Task 3: `ProductsArea`

The largest zone. Top row: `SearchInput` (full-width minus action buttons), PLU action button, Price-check action button. Below: a 4-column GridLayout of `ProductTile` instances, scrollable when content overflows.

Slint's `GridLayout` lays children left-to-right by default; in RTL the layout flips automatically via the parent screen's RTL handling (we do not need to manually invert here since `GridLayout` does not have a `space-between` style alignment trap). However, the search row uses `HorizontalLayout` and *does* need RTL-aware ordering of search ↔ actions — we apply the `if Layout.is-rtl` convention from §4.

**Files:**
- Create: `ui/screens/checkout/products_area.slint`

- [ ] **Step 1: Write `products_area.slint`**

```slint
import { ScrollView } from "std-widgets.slint";
import { Theme, Surfaces } from "../../tokens/mod.slint";
import { Colors, Typography, Spacing, Radius, Animation, Layout, Locale } from "../../theme.slint";
import { Panel, Button, SearchInput, ProductTile } from "../../components/atomic/mod.slint";
import { CheckoutTileData } from "checkout_types.slint";

// ProductsArea — central zone. Search row on top, product grid below.
// State for the search query lives on this component as an in-out string;
// real filtering will happen in Plan 4 when the screen binds to
// ProductService.
export component ProductsArea inherits Panel {
    content-padding: Spacing.md;

    in property <[CheckoutTileData]> products: [];
    in-out property <string> query: "";

    callback search-changed(string);
    callback search-submitted(string);
    callback product-tapped(string);
    callback plu-pressed;
    callback price-check-pressed;

    VerticalLayout {
        spacing: Spacing.md;

        // Search row. RTL convention §4: `if Layout.is-rtl` to swap order
        // of [search] ↔ [PLU, Price-check].
        HorizontalLayout {
            spacing: Spacing.sm;
            alignment: stretch;

            if !Layout.is-rtl: SearchInput {
                horizontal-stretch: 1;
                placeholder: Locale.current == "ar" ? "ابحث عن المنتجات…" : "Search products…";
                value <=> root.query;
                changed(s) => { root.search-changed(s); }
                submitted(s) => { root.search-submitted(s); }
            }

            if !Layout.is-rtl: Button {
                label: Locale.current == "ar" ? "رمز" : "PLU";
                variant: "secondary";
                clicked => { root.plu-pressed(); }
            }

            if !Layout.is-rtl: Button {
                label: Locale.current == "ar" ? "السعر" : "PRICE?";
                variant: "secondary";
                clicked => { root.price-check-pressed(); }
            }

            // RTL: actions first (visual right edge = leading in RTL? no —
            // in RTL the leading edge is the visual right, so the search
            // should still be the wide leading element. Render search last
            // here so it ends up visually on the right.
            if Layout.is-rtl: Button {
                label: Locale.current == "ar" ? "السعر" : "PRICE?";
                variant: "secondary";
                clicked => { root.price-check-pressed(); }
            }

            if Layout.is-rtl: Button {
                label: Locale.current == "ar" ? "رمز" : "PLU";
                variant: "secondary";
                clicked => { root.plu-pressed(); }
            }

            if Layout.is-rtl: SearchInput {
                horizontal-stretch: 1;
                placeholder: Locale.current == "ar" ? "ابحث عن المنتجات…" : "Search products…";
                value <=> root.query;
                changed(s) => { root.search-changed(s); }
                submitted(s) => { root.search-submitted(s); }
            }
        }

        // Product grid, scrollable. Four columns at any width above ~720 dp.
        ScrollView {
            vertical-stretch: 1;
            VerticalLayout {
                spacing: Spacing.sm;

                // Render in rows of 4. Slint has no native 'chunks' iter,
                // so we render every tile and let the grid wrap by
                // computing column index. For a skeleton with <= 24 tiles
                // we hard-render four columns via 6 HorizontalLayouts
                // bounded by the input length. Simpler: a single
                // HorizontalLayout with `wrap` is not a Slint primitive,
                // so we use a GridLayout with explicit `col`/`row`
                // assignments.
                GridLayout {
                    spacing: Spacing.sm;
                    for prod[i] in root.products: ProductTile {
                        col: mod(i, 4);
                        row: floor(i / 4);
                        height: 120px;
                        name: prod.name;
                        price: prod.price;
                        currency: prod.currency;
                        category-accent: prod.category_accent;
                        disabled: prod.disabled;
                        out-of-stock: prod.out_of_stock;
                        clicked => { root.product-tapped(prod.id); }
                    }
                }
            }
        }
    }
}
```

- [ ] **Step 2: Compile check**

Run: `cargo check -p e2manage-pos-terminal 2>&1 | tail -20`
Expected: clean. If Slint rejects the inline `mod`/`floor` calls because they aren't recognised in this version, fall back to providing the products pre-arranged into rows from the screen above — STOP and surface to the messenger before changing the API.

- [ ] **Step 3: Commit**

```bash
git add ui/screens/checkout/products_area.slint
git commit -m "$(cat <<'EOF'
feat(ui): plan 3 task 3 — products area

Central zone from §5.1. Search row uses if-gated RTL mirroring per the
Plan 2 PayButton lesson (visible: false dual-slots interact badly with
space-between); product grid relies on Slint's GridLayout for the LTR/RTL
flip. Query state is owned locally; Plan 4 binds the filter to
ProductService.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
git branch --show-current
```

---

### Task 4: `OpsColumn`

Six `OpsButton`s in a fixed vertical order, plus a small "ON SELECTED" caption above and a "MORE" button below. Width is 96 dp (§5.1).

**Files:**
- Create: `ui/screens/checkout/ops_column.slint`

- [ ] **Step 1: Write `ops_column.slint`**

```slint
import { Theme, Surfaces } from "../../tokens/mod.slint";
import { Colors, Typography, Spacing, Radius, Animation, Layout, Locale } from "../../theme.slint";
import { Panel, Button, OpsButton } from "../../components/atomic/mod.slint";

// OpsColumn — fixed-order operations against the currently selected cart
// line. The button order (+1, −1, ×n, %, ✎, ⌫) is sacred per §3.8 of the
// design spec and must not move without explicit cashier configuration.
// `selected-line-id` is consumed only to dim the column when no line is
// selected; the actual mutation is delegated upwards via `op-requested`.
export component OpsColumn inherits Panel {
    width: 112px;
    content-padding: Spacing.sm;

    in property <string> selected-line-id: "";
    callback op-requested(string /* op */, string /* line-id */);
    callback more-pressed;

    property <bool> any-selected: root.selected-line-id != "";

    VerticalLayout {
        spacing: Spacing.sm;
        alignment: start;

        Text {
            text: Locale.current == "ar" ? "على المحدد" : "ON SELECTED";
            font-family: Typography.font-family;
            font-size: Typography.tiny;
            font-weight: Typography.semi-bold;
            color: Colors.text-secondary;
            horizontal-alignment: center;
        }

        OpsButton {
            glyph: "+1"; label: Locale.current == "ar" ? "إضافة" : "ADD";
            variant: "primary"; disabled: !root.any-selected;
            clicked => { root.op-requested("inc", root.selected-line-id); }
        }
        OpsButton {
            glyph: "−1"; label: Locale.current == "ar" ? "إزالة" : "REMOVE";
            disabled: !root.any-selected;
            clicked => { root.op-requested("dec", root.selected-line-id); }
        }
        OpsButton {
            glyph: "×n"; label: Locale.current == "ar" ? "كمية" : "QTY";
            disabled: !root.any-selected;
            clicked => { root.op-requested("qty", root.selected-line-id); }
        }
        OpsButton {
            glyph: "%"; label: Locale.current == "ar" ? "خصم" : "DISC";
            disabled: !root.any-selected;
            clicked => { root.op-requested("disc", root.selected-line-id); }
        }
        OpsButton {
            glyph: "✎"; label: Locale.current == "ar" ? "تعديل" : "EDIT";
            disabled: !root.any-selected;
            clicked => { root.op-requested("edit", root.selected-line-id); }
        }
        OpsButton {
            glyph: "⌫"; label: Locale.current == "ar" ? "حذف" : "VOID";
            variant: "danger"; disabled: !root.any-selected;
            clicked => { root.op-requested("void", root.selected-line-id); }
        }

        Rectangle { vertical-stretch: 1; }

        Button {
            label: Locale.current == "ar" ? "المزيد" : "MORE";
            variant: "secondary";
            clicked => { root.more-pressed(); }
        }
    }
}
```

- [ ] **Step 2: Compile check**

Run: `cargo check -p e2manage-pos-terminal 2>&1 | tail -10`
Expected: clean.

- [ ] **Step 3: Commit**

```bash
git add ui/screens/checkout/ops_column.slint
git commit -m "$(cat <<'EOF'
feat(ui): plan 3 task 4 — ops column

The new operations pattern from §3.3 / §5.1. Button order is sacred per
§3.8 — +1 / −1 / ×n / % / ✎ / ⌫ — so it lives as positional source order,
not a configurable array. Disabled state propagates from
selected-line-id; ops mutations are emitted upward as op-requested and
performed by the screen (Plan 3: log only; Plan 4: route to CartService).

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
git branch --show-current
```

---

### Task 5: `CartPanelArea`

A `Panel` containing optional customer chip, a vertical list of `CartLine`s (scrollable), a totals block, and a `PayButton` strip at the bottom. Width 280 dp (§5.1 says ~250 dp, but 280 lands exactly on the 4 dp grid and gives the cart line text room without truncating a 24-char product name).

**Files:**
- Create: `ui/screens/checkout/cart_panel_area.slint`

- [ ] **Step 1: Write `cart_panel_area.slint`**

```slint
import { ScrollView } from "std-widgets.slint";
import { Theme, Surfaces } from "../../tokens/mod.slint";
import { Colors, Typography, Spacing, Radius, Animation, Layout, Locale } from "../../theme.slint";
import { Panel, PayButton, CartLine } from "../../components/atomic/mod.slint";
import { CheckoutLineData } from "checkout_types.slint";

// CartPanelArea — the trailing-edge zone. Cart line list + totals + Pay.
// `selected-line-id` is two-way: tapping a CartLine updates it; the screen
// uses it to drive the OpsColumn's disabled state.
export component CartPanelArea inherits Panel {
    width: 280px;
    content-padding: Spacing.md;

    in property <[CheckoutLineData]> lines: [];
    in property <string> subtotal: "0.000";
    in property <string> tax: "0.000";
    in property <string> total: "0.000";
    in property <string> currency: "LYD";
    in property <string> customer-name: "";
    in-out property <string> selected-line-id: "";

    callback pay-pressed;

    VerticalLayout {
        spacing: Spacing.md;

        // Optional customer chip.
        Rectangle {
            visible: root.customer-name != "";
            height: 36px;
            border-radius: Radius.sm;
            background: Surfaces.inset-top;
            border-color: Surfaces.inset-border;
            border-width: 1px;
            HorizontalLayout {
                padding-left: Spacing.sm;
                padding-right: Spacing.sm;
                spacing: Spacing.xs;
                alignment: start;
                Text {
                    text: "👤";
                    font-family: Typography.font-family;
                    font-size: Typography.body;
                    color: Colors.text-secondary;
                    vertical-alignment: center;
                }
                Text {
                    text: root.customer-name;
                    font-family: Typography.font-family;
                    font-size: Typography.caption;
                    font-weight: Typography.semi-bold;
                    color: Colors.text-primary;
                    vertical-alignment: center;
                }
            }
        }

        // Cart line list.
        ScrollView {
            vertical-stretch: 1;
            VerticalLayout {
                spacing: Spacing.xs;
                for line in root.lines: CartLine {
                    name: line.name;
                    qty: line.qty;
                    unit-price: line.unit_price;
                    line-total: line.line_total;
                    currency: line.currency;
                    selected: root.selected-line-id == line.id;
                    clicked => { root.selected-line-id = line.id; }
                }
            }
        }

        // Totals block.
        Rectangle {
            height: 76px;
            border-radius: Radius.sm;
            background: Surfaces.inset-top;
            border-color: Surfaces.inset-border;
            border-width: 1px;
            VerticalLayout {
                padding: Spacing.sm;
                spacing: Spacing.xxs;

                HorizontalLayout {
                    spacing: Spacing.sm;
                    Text {
                        text: Locale.current == "ar" ? "المجموع الفرعي" : "Subtotal";
                        font-family: Typography.font-family;
                        font-size: Typography.caption;
                        color: Colors.text-secondary;
                        horizontal-stretch: 1;
                        horizontal-alignment: Layout.is-rtl ? right : left;
                    }
                    Text {
                        text: root.subtotal;
                        font-family: Typography.font-family-mono;
                        font-size: Typography.caption;
                        color: Colors.text-primary;
                    }
                }
                HorizontalLayout {
                    spacing: Spacing.sm;
                    Text {
                        text: Locale.current == "ar" ? "الضريبة" : "Tax";
                        font-family: Typography.font-family;
                        font-size: Typography.caption;
                        color: Colors.text-secondary;
                        horizontal-stretch: 1;
                        horizontal-alignment: Layout.is-rtl ? right : left;
                    }
                    Text {
                        text: root.tax;
                        font-family: Typography.font-family-mono;
                        font-size: Typography.caption;
                        color: Colors.text-primary;
                    }
                }
                HorizontalLayout {
                    spacing: Spacing.sm;
                    Text {
                        text: Locale.current == "ar" ? "الإجمالي" : "Total";
                        font-family: Typography.font-family;
                        font-size: Typography.body;
                        font-weight: Typography.bold;
                        color: Colors.text-primary;
                        horizontal-stretch: 1;
                        horizontal-alignment: Layout.is-rtl ? right : left;
                    }
                    Text {
                        text: root.total;
                        font-family: Typography.font-family-mono;
                        font-size: Typography.body;
                        font-weight: Typography.bold;
                        color: Colors.text-primary;
                    }
                    Text {
                        text: root.currency;
                        font-family: Typography.font-family;
                        font-size: Typography.tiny;
                        color: Colors.text-secondary;
                        vertical-alignment: bottom;
                    }
                }
            }
        }

        // Pay strip.
        PayButton {
            label: Locale.current == "ar" ? "ادفع" : "PAY";
            total: root.total;
            currency: root.currency;
            disabled: root.lines.length == 0;
            clicked => { root.pay-pressed(); }
        }
    }
}
```

- [ ] **Step 2: Compile check**

Run: `cargo check -p e2manage-pos-terminal 2>&1 | tail -10`
Expected: clean.

- [ ] **Step 3: Commit**

```bash
git add ui/screens/checkout/cart_panel_area.slint
git commit -m "$(cat <<'EOF'
feat(ui): plan 3 task 5 — cart panel area

Trailing-edge zone from §5.1. Width 280 dp (rounded up from spec's
~250 dp to fit on the 4 dp grid and give 24-char product names room).
Selection is two-way bound so the ops column can read it without a
separate global. Pay strip auto-disables when the cart is empty.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
git branch --show-current
```

---

### Task 6: `CheckoutHeader` and `CheckoutFooter`

Two slim full-width chrome strips. Both export from the same file `checkout_chrome.slint` because they share the surface tier and only differ in content. The header carries brand text, terminal ID, operator name, online/offline LED, clock. The footer carries offline-queue summary and a "MORE" trigger. Heights: header 56 dp, footer 32 dp.

**Files:**
- Create: `ui/screens/checkout/checkout_chrome.slint`

- [ ] **Step 1: Write `checkout_chrome.slint`**

```slint
import { Theme, Surfaces } from "../../tokens/mod.slint";
import { Colors, Typography, Spacing, Radius, Animation, Layout, Locale } from "../../theme.slint";
import { StatusLED } from "../../components/atomic/mod.slint";

// CheckoutHeader — full-width strip above the four zones. Brand mark,
// terminal id, operator, online LED, clock. Mock state in skeleton mode;
// Plan 4 binds these to AppState.
export component CheckoutHeader inherits Rectangle {
    in property <string> brand: "E2Manage POS";
    in property <string> terminal-id: "T-001";
    in property <string> operator-name: "—";
    in property <string> clock: "00:00";
    // "online" | "offline" | "syncing"
    in property <string> network-state: "online";

    height: 56px;
    background: Surfaces.panel-top;
    border-color: Surfaces.panel-border;
    border-width: 1px;
    border-radius: Radius.md;
    drop-shadow-color: Surfaces.panel-shadow;
    drop-shadow-blur: Surfaces.panel-shadow-blur;
    drop-shadow-offset-y: Surfaces.panel-shadow-offset-y;

    // Specular top highlight (1 px).
    Rectangle {
        x: 1px; y: 1px;
        width: parent.width - 2px;
        height: 1px;
        background: Surfaces.specular-strong;
    }

    HorizontalLayout {
        padding-left: Spacing.lg;
        padding-right: Spacing.lg;
        spacing: Spacing.lg;
        alignment: center;

        Text {
            text: root.brand;
            font-family: Typography.font-family;
            font-size: Typography.heading;
            font-weight: Typography.bold;
            color: Colors.text-primary;
            vertical-alignment: center;
        }
        Text {
            text: root.terminal-id;
            font-family: Typography.font-family-mono;
            font-size: Typography.caption;
            color: Colors.text-secondary;
            vertical-alignment: center;
        }

        Rectangle { horizontal-stretch: 1; }

        Text {
            text: root.operator-name;
            font-family: Typography.font-family;
            font-size: Typography.caption;
            color: Colors.text-secondary;
            vertical-alignment: center;
        }
        StatusLED {
            state: root.network-state;
        }
        Text {
            text: root.clock;
            font-family: Typography.font-family-mono;
            font-size: Typography.body;
            font-weight: Typography.semi-bold;
            color: Colors.text-primary;
            vertical-alignment: center;
        }
    }
}

// CheckoutFooter — full-width strip below the four zones. Offline queue
// summary (or "ready" indicator) plus a "MORE" trigger.
export component CheckoutFooter inherits Rectangle {
    in property <string> queue-summary: "";
    in property <bool> queue-visible: false;

    callback more-pressed;

    height: 32px;
    background: Surfaces.panel-top;
    border-color: Surfaces.panel-border;
    border-width: 1px;
    border-radius: Radius.md;

    HorizontalLayout {
        padding-left: Spacing.md;
        padding-right: Spacing.md;
        spacing: Spacing.md;
        alignment: center;

        Text {
            text: root.queue-visible ? root.queue-summary :
                (Locale.current == "ar" ? "جاهز" : "Ready");
            font-family: Typography.font-family;
            font-size: Typography.tiny;
            color: root.queue-visible ? Colors.warning : Colors.text-secondary;
            vertical-alignment: center;
        }

        Rectangle { horizontal-stretch: 1; }

        Rectangle {
            width: 64px;
            height: 22px;
            border-radius: Radius.sm;
            background: Surfaces.surface-top;
            border-color: Surfaces.surface-border;
            border-width: 1px;
            TouchArea {
                clicked => { root.more-pressed(); }
            }
            Text {
                text: Locale.current == "ar" ? "المزيد" : "MORE";
                font-family: Typography.font-family;
                font-size: Typography.tiny;
                font-weight: Typography.semi-bold;
                color: Colors.text-primary;
                horizontal-alignment: center;
                vertical-alignment: center;
            }
        }
    }
}
```

- [ ] **Step 2: Compile check**

Run: `cargo check -p e2manage-pos-terminal 2>&1 | tail -10`
Expected: clean.

- [ ] **Step 3: Commit**

```bash
git add ui/screens/checkout/checkout_chrome.slint
git commit -m "$(cat <<'EOF'
feat(ui): plan 3 task 6 — checkout header + footer

Full-width chrome strips that flank the four zones. Header carries the
brand mark, terminal id, operator, online LED, and clock; footer carries
offline-queue summary and the MORE trigger. Both share the panel surface
tier and live in one file to keep the chrome layer findable.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
git branch --show-current
```

---

### Task 7: `MainCheckoutScreenV2`

Compose `CheckoutHeader`, `CategoriesRail`, `ProductsArea`, `OpsColumn`, `CartPanelArea`, `CheckoutFooter` into the four-zone layout. The composition uses a `VerticalLayout` (header / middle / footer) with a `HorizontalLayout` in the middle (rail / products / ops / cart).

Note: in Slint, a `HorizontalLayout`'s child order is rendered left-to-right regardless of `Layout.is-rtl`. To get the §5.1 mirror behaviour (cart on the right in LTR / on the left in RTL), we render the children in source order LTR (`rail, products, ops, cart`) and conditionally reverse them using `if Layout.is-rtl` / `if Layout.is-ltr` blocks. The RTL convention from §4 of this document applies.

**Files:**
- Create: `ui/screens/checkout/checkout_v2.slint`

- [ ] **Step 1: Write `checkout_v2.slint`**

```slint
import { Theme, Surfaces } from "../../tokens/mod.slint";
import { Colors, Typography, Spacing, Radius, Animation, Layout, Locale } from "../../theme.slint";
import {
    CheckoutLineData,
    CheckoutTileData,
    CheckoutCategoryData,
} from "checkout_types.slint";
import { CategoriesRail } from "categories_rail.slint";
import { ProductsArea } from "products_area.slint";
import { OpsColumn } from "ops_column.slint";
import { CartPanelArea } from "cart_panel_area.slint";
import { CheckoutHeader, CheckoutFooter } from "checkout_chrome.slint";

// MainCheckoutScreenV2 — Plan 3 skeleton. The legacy MainCheckoutScreen
// in ui/screens/checkout/main.slint is unchanged and remains the
// production routing target until Plan 4 migrates.
export component MainCheckoutScreenV2 inherits Rectangle {
    background: @linear-gradient(180deg, Surfaces.bg-top 0%, Surfaces.bg-bottom 100%);

    // === Inputs (mock data, supplied by the preview window) ===
    in property <[CheckoutCategoryData]> categories: [];
    in property <[CheckoutTileData]> products: [];
    in property <[CheckoutLineData]> cart-lines: [];

    in property <string> brand: "E2Manage POS";
    in property <string> terminal-id: "T-001";
    in property <string> operator-name: "—";
    in property <string> clock: "00:00";
    in property <string> network-state: "online";

    in property <string> subtotal: "0.000";
    in property <string> tax: "0.000";
    in property <string> total: "0.000";
    in property <string> currency: "LYD";
    in property <string> customer-name: "";

    in property <string> queue-summary: "";
    in property <bool> queue-visible: false;

    // === Internal state ===
    in-out property <string> selected-category-id: "all";
    in-out property <string> selected-line-id: "";

    // === Callbacks (Plan 4 wires these to services) ===
    callback category-selected(string);
    callback product-tapped(string);
    callback op-requested(string /* op */, string /* line-id */);
    callback pay-pressed;
    callback plu-pressed;
    callback price-check-pressed;
    callback more-pressed;
    callback back-pressed;
    callback settings-pressed;
    callback overflow-pressed;
    callback search-changed(string);
    callback search-submitted(string);

    VerticalLayout {
        padding: Spacing.md;
        spacing: Spacing.md;

        // Header.
        CheckoutHeader {
            brand: root.brand;
            terminal-id: root.terminal-id;
            operator-name: root.operator-name;
            clock: root.clock;
            network-state: root.network-state;
        }

        // Middle: four zones. RTL convention §4 — `if` to flip order.
        HorizontalLayout {
            spacing: Spacing.md;
            vertical-stretch: 1;

            if !Layout.is-rtl: CategoriesRail {
                categories: root.categories;
                selected-id: root.selected-category-id;
                category-selected(id) => {
                    root.selected-category-id = id;
                    root.category-selected(id);
                }
                overflow-pressed => { root.overflow-pressed(); }
                back-pressed => { root.back-pressed(); }
                settings-pressed => { root.settings-pressed(); }
            }
            if !Layout.is-rtl: ProductsArea {
                horizontal-stretch: 1;
                products: root.products;
                search-changed(s) => { root.search-changed(s); }
                search-submitted(s) => { root.search-submitted(s); }
                product-tapped(id) => { root.product-tapped(id); }
                plu-pressed => { root.plu-pressed(); }
                price-check-pressed => { root.price-check-pressed(); }
            }
            if !Layout.is-rtl: OpsColumn {
                selected-line-id: root.selected-line-id;
                op-requested(op, line) => { root.op-requested(op, line); }
                more-pressed => { root.more-pressed(); }
            }
            if !Layout.is-rtl: CartPanelArea {
                lines: root.cart-lines;
                subtotal: root.subtotal;
                tax: root.tax;
                total: root.total;
                currency: root.currency;
                customer-name: root.customer-name;
                selected-line-id <=> root.selected-line-id;
                pay-pressed => { root.pay-pressed(); }
            }

            // RTL order: cart, ops, products, rail.
            if Layout.is-rtl: CartPanelArea {
                lines: root.cart-lines;
                subtotal: root.subtotal;
                tax: root.tax;
                total: root.total;
                currency: root.currency;
                customer-name: root.customer-name;
                selected-line-id <=> root.selected-line-id;
                pay-pressed => { root.pay-pressed(); }
            }
            if Layout.is-rtl: OpsColumn {
                selected-line-id: root.selected-line-id;
                op-requested(op, line) => { root.op-requested(op, line); }
                more-pressed => { root.more-pressed(); }
            }
            if Layout.is-rtl: ProductsArea {
                horizontal-stretch: 1;
                products: root.products;
                search-changed(s) => { root.search-changed(s); }
                search-submitted(s) => { root.search-submitted(s); }
                product-tapped(id) => { root.product-tapped(id); }
                plu-pressed => { root.plu-pressed(); }
                price-check-pressed => { root.price-check-pressed(); }
            }
            if Layout.is-rtl: CategoriesRail {
                categories: root.categories;
                selected-id: root.selected-category-id;
                category-selected(id) => {
                    root.selected-category-id = id;
                    root.category-selected(id);
                }
                overflow-pressed => { root.overflow-pressed(); }
                back-pressed => { root.back-pressed(); }
                settings-pressed => { root.settings-pressed(); }
            }
        }

        // Footer.
        CheckoutFooter {
            queue-summary: root.queue-summary;
            queue-visible: root.queue-visible;
            more-pressed => { root.more-pressed(); }
        }
    }
}
```

- [ ] **Step 2: Compile check**

Run: `cargo check -p e2manage-pos-terminal 2>&1 | tail -20`
Expected: clean.

- [ ] **Step 3: Commit**

```bash
git add ui/screens/checkout/checkout_v2.slint
git commit -m "$(cat <<'EOF'
feat(ui): plan 3 task 7 — main checkout screen composition

Composes header + four-zone middle + footer into the cashier-facing
layout. RTL is implemented via paired if-gated child trees on the middle
HorizontalLayout — a deliberately brittle pattern flagged in §10 for
Plan 4 to consolidate. Mock-data inputs and callbacks are wired but not
bound to services; cashier app still routes to the legacy
MainCheckoutScreen until Plan 4.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
git branch --show-current
```

---

### Task 8: Preview window — `CheckoutPreview` + `CheckoutPreviewWindow`

The preview is a Slint-side wrapper that:

- Owns the toolbar (theme/dir/locale toggles, same pattern as `ComponentGallery`).
- Populates `MainCheckoutScreenV2` with mock data (~5 categories, ~12 products, 4 cart lines).
- Renders inside a runnable `Window` so the Rust binary entry can construct it via `ComponentHandle::new()`.

Mock data values are written verbatim below. The Arabic strings should be copied character-for-character into the file — Arabic glyph shaping issues caught in Plan 2 would re-appear if the file was machine-translated.

**Files:**
- Create: `ui/screens/dev/checkout_preview.slint`
- Create: `ui/screens/dev/checkout_preview_window.slint`
- Modify: `ui/screens/dev/mod.slint`

- [ ] **Step 1: Write `checkout_preview.slint`**

```slint
import { Theme, Surfaces } from "../../tokens/mod.slint";
import { Colors, Typography, Spacing, Radius, Layout, Locale } from "../../theme.slint";
import {
    CheckoutLineData,
    CheckoutTileData,
    CheckoutCategoryData,
} from "../checkout/checkout_types.slint";
import { MainCheckoutScreenV2 } from "../checkout/checkout_v2.slint";

// CheckoutPreview — wraps MainCheckoutScreenV2 with a toolbar (theme,
// direction, locale) and a hardcoded mock-data set. Runs behind
// `--checkout-preview`. Not part of the cashier flow.
export component CheckoutPreview inherits Rectangle {
    in-out property <string> mode <=> Theme.mode;
    in-out property <bool> rtl <=> Layout.rtl;
    in-out property <string> locale <=> Locale.current;

    callback toggle-theme;
    callback toggle-rtl;
    callback cycle-locale;

    background: Surfaces.bg-bottom;

    // Emoji caveat: the bundled IBM Plex Sans + Sans Arabic fonts (see Plan 1
    // findings) do not carry emoji glyphs. The pictographs below will render
    // only via system font fallback. On a workstation without an emoji font
    // installed (or on the target hardware in production) they will fall back
    // to tofu boxes. Task 10 visual verification must explicitly check rail
    // glyph rendering and flag it as a finding if fallback fails; Plan 4
    // should replace these with either bundled SVG icons or category-specific
    // ASCII/mono glyphs.
    property <[CheckoutCategoryData]> mock-categories: [
        { id: "coffee",  name: Locale.current == "ar" ? "قهوة" : "Coffee",
          icon: "☕", accent: Colors.cat-coffee },
        { id: "bakery",  name: Locale.current == "ar" ? "مخبوزات" : "Bakery",
          icon: "🥐", accent: Colors.cat-bakery },
        { id: "cold",    name: Locale.current == "ar" ? "مشروبات باردة" : "Cold",
          icon: "🧊", accent: Colors.cat-cold },
        { id: "food",    name: Locale.current == "ar" ? "طعام" : "Food",
          icon: "🍽", accent: Colors.cat-food },
        { id: "service", name: Locale.current == "ar" ? "خدمة" : "Service",
          icon: "🛠", accent: Colors.cat-coffee },
    ];

    property <[CheckoutTileData]> mock-products: [
        { id: "p1",  name: Locale.current == "ar" ? "قهوة لاتيه" : "Café Latte",
          price: "12.500", currency: "LYD", category_id: "coffee",
          category_accent: Colors.cat-coffee, disabled: false, out_of_stock: false },
        { id: "p2",  name: Locale.current == "ar" ? "كابتشينو" : "Cappuccino",
          price: "11.000", currency: "LYD", category_id: "coffee",
          category_accent: Colors.cat-coffee, disabled: false, out_of_stock: false },
        { id: "p3",  name: Locale.current == "ar" ? "إسبريسو" : "Espresso",
          price: "8.500",  currency: "LYD", category_id: "coffee",
          category_accent: Colors.cat-coffee, disabled: false, out_of_stock: false },
        { id: "p4",  name: Locale.current == "ar" ? "كرواسون" : "Croissant",
          price: "6.000",  currency: "LYD", category_id: "bakery",
          category_accent: Colors.cat-bakery, disabled: false, out_of_stock: false },
        { id: "p5",  name: Locale.current == "ar" ? "كعك بالشوكولاتة" : "Chocolate Muffin",
          price: "7.500",  currency: "LYD", category_id: "bakery",
          category_accent: Colors.cat-bakery, disabled: false, out_of_stock: false },
        { id: "p6",  name: Locale.current == "ar" ? "خبز قمح" : "Wheat Loaf",
          price: "4.000",  currency: "LYD", category_id: "bakery",
          category_accent: Colors.cat-bakery, disabled: false, out_of_stock: true },
        { id: "p7",  name: Locale.current == "ar" ? "ماء بارد" : "Cold Water",
          price: "1.500",  currency: "LYD", category_id: "cold",
          category_accent: Colors.cat-cold, disabled: false, out_of_stock: false },
        { id: "p8",  name: Locale.current == "ar" ? "عصير برتقال" : "Orange Juice",
          price: "5.500",  currency: "LYD", category_id: "cold",
          category_accent: Colors.cat-cold, disabled: false, out_of_stock: false },
        { id: "p9",  name: Locale.current == "ar" ? "ساندويش" : "Sandwich",
          price: "18.000", currency: "LYD", category_id: "food",
          category_accent: Colors.cat-food, disabled: false, out_of_stock: false },
        { id: "p10", name: Locale.current == "ar" ? "سلطة سيزر" : "Caesar Salad",
          price: "22.000", currency: "LYD", category_id: "food",
          category_accent: Colors.cat-food, disabled: false, out_of_stock: false },
        { id: "p11", name: Locale.current == "ar" ? "بيتزا مارجريتا" : "Margherita Pizza",
          price: "28.000", currency: "LYD", category_id: "food",
          category_accent: Colors.cat-food, disabled: false, out_of_stock: false },
        { id: "p12", name: Locale.current == "ar" ? "كيس حمل" : "Carrier Bag",
          price: "0.500",  currency: "LYD", category_id: "service",
          category_accent: Colors.cat-coffee, disabled: true, out_of_stock: false },
    ];

    // Hand-computed cart. Each line_total = qty * unit_price; the screen-level
    // subtotal/total properties below must match the sum of line_totals.
    //   L1: 2 × 12.500 = 25.000
    //   L2: 1 × 6.000  =  6.000
    //   L3: 1 × 18.000 = 18.000
    //   L4: 3 × 1.500  =  4.500
    //   Σ              = 53.500
    // If a Worker tweaks any line below, update the subtotal/total in the
    // MainCheckoutScreenV2 block by the same delta. Plan 4 replaces this with
    // derived totals from CartService.
    property <[CheckoutLineData]> mock-cart-lines: [
        { id: "L1", product_id: "p1",
          name: Locale.current == "ar" ? "قهوة لاتيه" : "Café Latte",
          qty: 2, unit_price: "12.500", line_total: "25.000", currency: "LYD" },
        { id: "L2", product_id: "p4",
          name: Locale.current == "ar" ? "كرواسون" : "Croissant",
          qty: 1, unit_price: "6.000",  line_total: "6.000",  currency: "LYD" },
        { id: "L3", product_id: "p9",
          name: Locale.current == "ar" ? "ساندويش بالدجاج المشوي" : "Grilled Chicken Sandwich",
          qty: 1, unit_price: "18.000", line_total: "18.000", currency: "LYD" },
        { id: "L4", product_id: "p7",
          name: Locale.current == "ar" ? "ماء بارد" : "Cold Water",
          qty: 3, unit_price: "1.500",  line_total: "4.500",  currency: "LYD" },
    ];

    VerticalLayout {
        spacing: 0;

        // Toolbar — identical pattern to ComponentGallery.
        Rectangle {
            height: 48px;
            background: Surfaces.panel-top;
            border-color: Surfaces.panel-border;
            border-width: 1px;
            border-radius: Radius.md;

            HorizontalLayout {
                padding-left: Spacing.lg;
                padding-right: Spacing.lg;
                spacing: Spacing.md;
                alignment: center;

                Text {
                    text: "POS — Checkout Preview (Plan 3 skeleton)";
                    font-family: Typography.font-family;
                    font-size: Typography.heading;
                    font-weight: Typography.semi-bold;
                    color: Colors.text-primary;
                    horizontal-stretch: 1;
                    vertical-alignment: center;
                }

                Rectangle {
                    width: 110px; height: 32px;
                    background: Surfaces.surface-top;
                    border-color: Surfaces.surface-border;
                    border-width: 1px;
                    border-radius: Radius.sm;
                    TouchArea { clicked => { root.toggle-theme(); } }
                    Text {
                        text: "Theme: " + Theme.mode;
                        font-family: Typography.font-family;
                        font-size: Typography.caption;
                        color: Colors.text-primary;
                        horizontal-alignment: center;
                        vertical-alignment: center;
                    }
                }
                Rectangle {
                    width: 110px; height: 32px;
                    background: Surfaces.surface-top;
                    border-color: Surfaces.surface-border;
                    border-width: 1px;
                    border-radius: Radius.sm;
                    TouchArea { clicked => { root.toggle-rtl(); } }
                    Text {
                        text: Layout.is-rtl ? "Dir: RTL" : "Dir: LTR";
                        font-family: Typography.font-family;
                        font-size: Typography.caption;
                        color: Colors.text-primary;
                        horizontal-alignment: center;
                        vertical-alignment: center;
                    }
                }
                Rectangle {
                    width: 110px; height: 32px;
                    background: Surfaces.surface-top;
                    border-color: Surfaces.surface-border;
                    border-width: 1px;
                    border-radius: Radius.sm;
                    TouchArea { clicked => { root.cycle-locale(); } }
                    Text {
                        text: "Lang: " + Locale.current;
                        font-family: Typography.font-family;
                        font-size: Typography.caption;
                        color: Colors.text-primary;
                        horizontal-alignment: center;
                        vertical-alignment: center;
                    }
                }
            }
        }

        // The screen.
        MainCheckoutScreenV2 {
            vertical-stretch: 1;
            categories: root.mock-categories;
            products: root.mock-products;
            cart-lines: root.mock-cart-lines;

            brand: "E2Manage POS";
            terminal-id: "T-001";
            operator-name: Locale.current == "ar" ? "عبدو" : "Abdu";
            clock: "14:18";
            network-state: "online";

            subtotal: "53.500";
            tax: "0.000";
            total: "53.500";
            currency: "LYD";
            customer-name: "";

            queue-summary: Locale.current == "ar"
                ? "٣ معاملات في قائمة الانتظار · آخر مزامنة ١٤:١٨"
                : "3 transactions queued · last sync 14:18";
            queue-visible: false;

            // Pre-select the most-recently-added line per §5.1 ("auto-select").
            selected-line-id: "L4";
        }
    }
}
```

- [ ] **Step 2: Write `checkout_preview_window.slint`**

```slint
import { CheckoutPreview } from "checkout_preview.slint";

// Window wrapper so Rust (slint::ComponentHandle) can construct it directly.
// Slint only generates Rust bindings for components that inherit Window.
export component CheckoutPreviewWindow inherits Window {
    title: "POS Checkout Preview";
    preferred-width: 1280px;
    preferred-height: 900px;

    in-out property <string> mode <=> preview.mode;
    in-out property <bool> rtl <=> preview.rtl;
    in-out property <string> locale <=> preview.locale;

    callback toggle-theme <=> preview.toggle-theme;
    callback toggle-rtl <=> preview.toggle-rtl;
    callback cycle-locale <=> preview.cycle-locale;

    preview := CheckoutPreview {
        width: 100%;
        height: 100%;
    }
}
```

- [ ] **Step 3: Add exports to `ui/screens/dev/mod.slint`**

Append two lines to the file. Final content:

```slint
// ============================================================================
// E2Manage POS Terminal - Dev Screens Module
// ============================================================================
// Engineering-only screens. Not part of the cashier flow. Wired in via
// command-line flags (e.g. --theme-harness in dev_harness.rs, Task 9).
// ============================================================================

export { ThemeHarness } from "theme_harness.slint";
export { ThemeHarnessWindow } from "theme_harness_window.slint";
export { ComponentGallery } from "component_gallery.slint";
export { ComponentGalleryWindow } from "component_gallery_window.slint";
export { CheckoutPreview } from "checkout_preview.slint";
export { CheckoutPreviewWindow } from "checkout_preview_window.slint";
```

- [ ] **Step 4: Compile check**

Run: `cargo check -p e2manage-pos-terminal 2>&1 | tail -20`
Expected: clean.

- [ ] **Step 5: Commit**

```bash
git add ui/screens/dev/checkout_preview.slint ui/screens/dev/checkout_preview_window.slint ui/screens/dev/mod.slint
git commit -m "$(cat <<'EOF'
feat(ui): plan 3 task 8 — checkout preview window

Dev-only wrapper around MainCheckoutScreenV2 with hardcoded mock data
and a theme/direction/locale toggle bar. Mirrors the
ComponentGalleryWindow pattern from Plan 2. Cart totals are
hand-computed; an arithmetic comment in the file documents the math
because the preview has no service to derive them.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
git branch --show-current
```

---

### Task 9: Rust binary entry — `--checkout-preview`

Add a Rust module `src/checkout_preview.rs` that constructs `CheckoutPreviewWindow`, wires the three toolbar callbacks (theme/rtl/locale cycle, same pattern as `component_gallery.rs`), and runs it. Hook the argv branch in `src/main.rs`. Re-export the window component from `ui/main.slint`. Declare the module in `src/main.rs` (binary crate), **not** in `src/lib.rs` — the existing `component_gallery` and `dev_harness` modules are declared in `src/main.rs:7-9`, and the new module must follow the same pattern because `src/checkout_preview.rs` references `crate::CheckoutPreviewWindow` (produced by `slint::include_modules!()` at `src/main.rs:5`) and `crate::locale_detect::detect_locale` (the path that resolves from the binary crate root). Placing the module in `src/lib.rs` would orphan those `crate::` paths.

**Files:**
- Create: `src/checkout_preview.rs`
- Modify: `src/main.rs`
- Modify: `ui/main.slint`

- [ ] **Step 1: Write `src/checkout_preview.rs`**

```rust
//! Developer-only checkout preview. Run with
//! `cargo run -- --checkout-preview`. Renders the Plan 3 skeleton of the
//! main checkout screen with hardcoded mock data, in light/dark × LTR/RTL
//! × en/ar. Not part of the cashier flow.

use slint::ComponentHandle;

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let preview = crate::CheckoutPreviewWindow::new()?;

    // Initial state mirrors detected locale (consistent with the live app).
    let (locale_code, rtl) = crate::locale_detect::detect_locale();
    preview.set_mode("light".into());
    preview.set_rtl(rtl);
    preview.set_locale(locale_code.into());

    let weak = preview.as_weak();
    preview.on_toggle_theme(move || {
        if let Some(p) = weak.upgrade() {
            let next = if p.get_mode() == "light" {
                "dark"
            } else {
                "light"
            };
            p.set_mode(next.into());
        }
    });

    let weak = preview.as_weak();
    preview.on_toggle_rtl(move || {
        if let Some(p) = weak.upgrade() {
            p.set_rtl(!p.get_rtl());
        }
    });

    let weak = preview.as_weak();
    preview.on_cycle_locale(move || {
        if let Some(p) = weak.upgrade() {
            let next = match p.get_locale().as_str() {
                "en" => "ar",
                "ar" => "fr",
                _ => "en",
            };
            p.set_locale(next.into());
        }
    });

    preview.run()?;
    Ok(())
}
```

- [ ] **Step 2: Declare the module in `src/main.rs`**

Find the existing `mod component_gallery;` / `mod dev_harness;` / `mod locale_detect;` block near lines 7–9 and add the new module declaration directly below them:

```rust
mod component_gallery;
mod dev_harness;
mod locale_detect;
mod checkout_preview;
```

Do **not** add a corresponding `pub mod checkout_preview;` to `src/lib.rs`. The library crate has no use for this module — it is binary-crate-only, same as `component_gallery` and `dev_harness`.

- [ ] **Step 3: Wire the argv branch in `src/main.rs`**

Find the existing `--component-gallery` branch near line 48 and add the new branch directly below it:

```rust
    if std::env::args().any(|a| a == "--component-gallery") {
        return component_gallery::run();
    }

    if std::env::args().any(|a| a == "--checkout-preview") {
        return checkout_preview::run();
    }
```

- [ ] **Step 4: Re-export the window component from `ui/main.slint`**

Find the existing two dev-window re-exports near line 67–68 and add the new one directly below:

```slint
export { ThemeHarnessWindow } from "screens/dev/mod.slint";
export { ComponentGalleryWindow } from "screens/dev/mod.slint";
export { CheckoutPreviewWindow } from "screens/dev/mod.slint";
```

- [ ] **Step 5: Build check**

Run: `cargo build 2>&1 | tail -20`
Expected: clean build, no warnings. The Plan 2 cleanup landed a one-shot `cargo fmt` and resolved the two prior `src/main.rs` clippy warnings; treat any diagnostic as a real regression.

- [ ] **Step 6: Commit**

```bash
git add src/checkout_preview.rs src/main.rs ui/main.slint
git commit -m "$(cat <<'EOF'
feat(ui): plan 3 task 9 — --checkout-preview binary entry

Adds --checkout-preview alongside --component-gallery and --theme-harness,
with the same locale-detect-then-toggle pattern. Cashier-facing app
(cargo run with no flags) remains routed to the legacy MainCheckoutScreen
until Plan 4 migrates routing.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
git branch --show-current
```

---

### Task 10: Operator visual verification

This task is operator-driven on a workstation with a display server. The agent sandbox is headless and cannot run the binary; the binary launch must happen on the workstation and screenshots forwarded back. The task does not modify Slint or Rust code.

**Files:**
- Create (after operator delivers screenshots): `docs/POS-UI-REDESIGN-SCREENSHOTS-PLAN-03/01-light-ltr-en.png` … `04-dark-rtl-ar.png`
- Modify: `docs/POS-UI-REDESIGN-FINDINGS-01-SLINT-RTL.md` (append a "Visual verification — Plan 3" section, mirroring the existing "Visual verification — Plan 2" section)

- [ ] **Step 1: Operator launches the preview**

On the operator's workstation:

```bash
cargo run -- --checkout-preview
```

Expected: a 1280×900 window opens showing the toolbar and the four-zone checkout layout.

- [ ] **Step 2: Operator captures the four configurations**

For each of `01-light-ltr-en`, `02-light-rtl-ar`, `03-dark-ltr-en`, `04-dark-rtl-ar`:

1. Use the toolbar to switch into the configuration.
2. Capture a screenshot at the gallery's preferred size. If the content exceeds the viewport, vertically stitch two captures the same way Plan 2 verification did.
3. Save into `docs/POS-UI-REDESIGN-SCREENSHOTS-PLAN-03/<NN-config>.png`.

- [ ] **Step 3: Operator verification checklist**

Tick each item against the relevant screenshot. Any FAIL gets a follow-up Worker brief; PASS-with-caveat gets a note.

- Header — brand mark, terminal id, operator name visible. Online LED visible.
- Categories rail — leading edge in LTR (left), trailing edge swap in RTL (right).
- Categories rail — "All" tile first in source order; in RTL the source-first child sits visually on the right by virtue of the screen-level if-gated mirror.
- Products area — search row at top, action buttons right of search in LTR, left of search in RTL.
- Products area — 4-column grid of `ProductTile` with category-accent stripes on the leading inside edge per locale.
- Out-of-stock pill visible on the one product with `out_of_stock: true`.
- Ops column — six buttons in fixed order: +1 / −1 / ×n / % / ✎ / ⌫. Plus "MORE" at the bottom and "ON SELECTED" caption above.
- Ops column — all six ops buttons enabled (selected-line-id is pre-set to "L4" in mock data).
- Cart panel — 4 cart lines, one (L4) showing the lime selected stroke.
- Cart panel — totals block + Pay button at the bottom.
- Pay button — solid green in light, lime gradient + halo in dark. RTL label/total mirror per Plan 2.
- Press feedback — tapping any button visibly dims to ~70% opacity, then springs back. Reads on a 24" display.
- Numerics — every price / total / qty pill renders left-to-right inside RTL containers.
- Arabic shaping — every Arabic string shows connected letterforms; no isolated-form fallbacks; no missing-glyph boxes.
- Category-rail emoji glyphs — ☕ 🥐 🧊 🍽 🛠 render via system font fallback (IBM Plex does not carry emoji). PASS if all five tiles show recognisable glyphs; FAIL or PASS-with-caveat if any render as tofu boxes or the renderings disagree between light and dark themes. Either outcome becomes a Plan 4 input.

- [ ] **Step 4: Operator updates findings doc**

Append a new section to `docs/POS-UI-REDESIGN-FINDINGS-01-SLINT-RTL.md` modelled on the existing "Visual verification — Plan 2" section. Date, host, build profile, configurations (PASS / FAIL / PASS-with-caveat plus notes), per-zone check results, RTL-specific checks, animation checks, follow-ups.

- [ ] **Step 5: Commit**

```bash
git add docs/POS-UI-REDESIGN-SCREENSHOTS-PLAN-03/ docs/POS-UI-REDESIGN-FINDINGS-01-SLINT-RTL.md
git commit -m "$(cat <<'EOF'
docs(pos): plan 3 visual verification — 4 configurations captured

Operator-driven on a workstation; the agent sandbox is headless. Captures
the light/dark × LTR/RTL × en/ar matrix established by Plan 2
verification and appends the matrix to the Slint+RTL findings doc.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
git branch --show-current
```

---

## 8. Verification matrix (cumulative)

After all tasks land, the following must hold:

- `cargo fmt --all --check` exits 0 across the workspace. (CI was gated unconditionally during the Plan 2 cleanup — no allow-list, no carry-forward drift.)
- `cargo clippy --workspace --all-targets -- -D warnings` exits 0.
- `cargo test --workspace -- --skip e2e_` exits 0.
- `cargo run -- --component-gallery` still launches and renders the Plan 2 gallery (no regression).
- `cargo run -- --theme-harness` still launches the Plan 1 theme harness (no regression).
- `cargo run -- --checkout-preview` launches the new screen and visual verification (Task 10) passes across all four configurations.
- The cashier-facing app (`cargo run` with no flags) still routes to the legacy `MainCheckoutScreen` and is otherwise unchanged.

---

## 9. Out of scope (explicit)

The following are deliberately not addressed and will become Plan 4 (wiring) or Plan 5+ (later screens):

- Binding any property on `MainCheckoutScreenV2` to a real Rust service. Specifically: no `CartService`, no `ProductService`, no `SyncService` interaction. Callbacks in the preview log only.
- Replacing the legacy `MainCheckoutScreen` in the routing layer. The router and `ui/main.slint`'s `Pages` enum (or equivalent) stay pointing at the legacy screen.
- Filtering products by selected category. The mock product array is rendered in full regardless of `selected-category-id`. Plan 4 introduces the filter at the Rust binding boundary.
- Implementing the category-overflow sheet, PLU pad, price-check overlay, "MORE" sheet, or any modal. These are Plan 5+ work.
- Designing the payment screen (§5.2), success modal (§5.3), error strips (§6.6, §6.7), empty-cart state (§6.1), skeleton loading tiles (§6.2), or offline banner (§6.4). Plan 3 ships a populated, online, mid-sale state only.
- Shimmer/skeleton loading states for the product grid.
- Keyboard shortcut wiring (F1 PLU, F2 Hold, `/` Search, …).
- Receipt printing, drawer kick, scanner integration, or any hardware.
- Tenant configuration loading (the payment-method catalog, per-category accent colours from the backend, currency code from the backend). All values in Plan 3 are hardcoded in the preview file.
- Compact (< 1024 px) responsive variant (§9 of the design spec). Counter mode only.

---

## 10. Open questions for Plan 4 (do not resolve here)

Listed for the next planning round, so context is not lost:

1. **CartService line-id durability.** `CartItem.id` is a UUID newly generated on `CartItem::new()`. The current legacy screen tracks selection by `item-id` strings as well, so this should be fine, but Plan 4 must confirm that ids survive every cart mutation (`set_quantity`, `apply_discount_*`) — `recalculate` does not change the id, but a re-add via `add_item` of an existing product *might*, depending on the cart-merging policy.
2. **Decimal → display formatting.** Plan 3 hand-writes strings with three decimals (`"12.500"`). Plan 4 needs a single formatting helper (`format_decimal_3` or similar) keyed off tenant config, used uniformly by the binding layer.
3. **Category accent resolution.** Plan 3 hardcodes `Colors.cat-coffee / bakery / cold / food` and assigns them in the preview. Plan 4 must map `Product.category_id` → accent. Tenant config is the natural home, but the seed data work has not happened yet (per §12 of the design doc).
4. **Auto-select rule.** §5.1 says "most-recently-added line is auto-selected". Plan 3 hardcodes the last line in the mock array as selected. Plan 4 needs an explicit `CartService` API or a derived property on the screen that picks the line with the latest `id` (or a separate `last_added_at` field — which CartItem does not currently have, so this may require a `pos_models::CartItem` field addition).
5. **OpsColumn op semantics.** `qty`, `disc`, and `edit` open ambient flows (numeric pad, discount picker, line-edit sheet) in the legacy app. Plan 4 must decide whether the new screen reuses the legacy `NumericKeypad` / `QuantityPad` / `PLUPad` overlays or builds new atomic-component-based replacements.
6. **Compact-mode breakpoint.** §9 specifies < 1024 px = stacked layout. Plan 3 makes no compact-mode provision. Plan 5+ should treat compact as a separate screen file or a parameterised variant — the four-zone HorizontalLayout used in Plan 3's Task 7 will not gracefully degrade to a stacked layout via responsive properties alone.
7. **Task 7 zone duplication.** The middle row in `checkout_v2.slint` instantiates each of the four zones twice — once inside `if !Layout.is-rtl: …` and once inside `if Layout.is-rtl: …` in reverse order. Slint's `if` truly removes the inactive branch from the layout, so this is correct, but it is ~80 lines of bidirectional duplication that drifts the moment any zone gains a property. Plan 4 should consolidate, either via positional `x:` based on `Layout.is-rtl` or a single mirroring container helper. Until then, every API change to a zone must update both branches in lockstep.
8. **Emoji icon strategy.** Plan 3 uses Unicode emoji pictographs (☕ 🥐 🧊 🍽 🛠) as category icons. IBM Plex does not carry emoji glyphs; rendering depends entirely on system font fallback. On target hardware this is unreliable. Plan 4 should pick: bundled SVG icons (asset pipeline work), category-specific mono ASCII glyphs (drops the visual richness), or a curated emoji font registered alongside IBM Plex.

