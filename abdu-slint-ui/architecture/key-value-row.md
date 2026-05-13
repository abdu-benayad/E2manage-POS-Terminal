# KeyValueRow — Design

> Per-component design doc. Sibling docs live under `abdu-slint-ui/architecture/`.
> Role: the *what and why* for KeyValueRow. Implementation steps live in `IMPL.md`
> (Component 5 — superseded by the API surface below in the same way Button's,
> IconButton's, Toggle's, and Card's specs were).

---

## Purpose

A two-content display row: a `label` on the leading side, a `value` on the trailing side. The building block for totals, settings rows, summary breakdowns, key-value tables — anywhere a single row needs to show "X: Y" with consistent typography, spacing, and direction handling.

**Why KeyValueRow exists as a primitive** (vs. consumers writing `HorizontalLayout { Text { ... } Rectangle { stretch } Text { ... } }` inline):

1. **It's the canonical incarnation of the single-script segmentation principle.** Every other place in the codebase that displays a labelled value today (`ui/screens/settings/display.slint`, the various total rows in checkout, Z-report breakdowns) concatenates the label and value into one string or one Text element. That works in pure-Latin and pure-Arabic builds and breaks the moment a mixed-script case appears. KeyValueRow makes the correct pattern — two anchored Text elements — the path of least resistance.
2. **Typography decisions belong in one place.** Emphasis levels (`subtle / normal / strong / total`) map to specific font weights and sizes. Encoding those in the component means every settings row, every total, every summary line picks the right weight without the consumer deciding.
3. **The RTL flip is non-trivial.** Label-icon position relative to label flips with `Locale.rtl`. Value cluster position in the row flips with `Locale.rtl`. Value cluster *internal* order flips with the XNOR of `Locale.rtl` and `unit-position`. Three rules, easy to get wrong; KeyValueRow encodes them once.
4. **Density and divider are settings-screen primitives.** Without them, consumers reach for inline `padding: ...` and `if last-row { ... }` patterns that the library exists to eliminate.

KeyValueRow shares with Card:

- **Display-only by default.** No `interactive` flag at present — if a consumer needs clickable rows, they wrap KeyValueRow inside an interactive Card (this is exactly the dominant "settings item row" pattern in iOS). Row-level `interactive` + `clicked()` is planned for Material-parity slice 3.
- **`debug-bounds` instrumentation.** Magenta border + cluster outlines when set, matching the other primitives' debug story.
- **No `variant` / no `tone` on the surface.** Only the *value* takes a tone (the label is always Theme.muted-foreground). Coloring the whole row implies semantics that aren't there.

KeyValueRow is **not** a composition of Card. A KeyValueRow inside a list does not have a per-row surface, border, or shadow — the *list* (typically a Card or SectionCard in Phase 2) carries those. KeyValueRow is a layout + typography primitive that renders directly into whatever surface contains it.

---

## Scope

**In scope (v1):**

- Two primary content slots (`label`, `value`) rendered as **separately-anchored Text elements** — the segmentation principle made concrete.
- An optional `label-icon` rendered in the leading cluster, position-flipped per `Locale.rtl` (matches Button's `icon-leading` behaviour).
- An optional `value-icon` rendered in the trailing cluster, position fixed within the value cluster regardless of `Locale.rtl`.
- An optional `value-unit` rendered immediately adjacent to `value` inside the trailing cluster.
- A `unit-position: UnitPosition { trailing, leading }` knob that chooses the unit's reading position relative to the value. `trailing` (default) = unit reads AFTER value in both locales (the standard pattern). `leading` = unit reads BEFORE value (currency-prefix style). The flip is symmetric across locales — see [Unit position and the segmentation principle](#unit-position-and-the-segmentation-principle).
- `emphasis: Emphasis` mapping to font weight + size + color (`subtle / normal / strong / total`).
- `value-tone: Tone` colouring the value cluster (value text, value unit, value icon all share the tone).
- `density: Density` mapping to vertical padding (`compact / default / comfortable` → `Spacing.sm / md / lg`).
- `wrap: bool` deciding between elide-on-overflow (default) and word-wrap-with-growing-row.
- `show-divider: bool` rendering a 1px bottom border in `Theme.border`.
- `tooltip: string` for hover discoverability (useful when `value` is truncated).
- `debug-bounds: bool` for layout debugging.

**Explicitly out of scope:**

- `interactive` / `clicked()`. Use Card with `interactive: true` wrapping a KeyValueRow. Adding an interactive variant duplicates Card's machinery and re-opens accessibility decisions that Card already settled.
- `loading`. Settings rows don't spin; settings *values* might be unknown ("—"), which is a string the consumer provides. If async-gated row content becomes a real need, the consumer renders a skeleton inside their list.
- `label-tone`. The label is always `Theme.muted-foreground` — settings screens with toned labels read as gimmicky. If a real screen needs it, revisit later; for now, hold the line.
- `label-min-width` / column alignment between rows. Consumers wanting aligned columns across multiple rows wrap their rows in a parent layout (GridLayout if introduced, or a custom column constraint). KeyValueRow's job is to render one row correctly; alignment across rows is the parent's job.
- A `value` slot accepting `@children`. Tracked as Material-parity slice 2 (`architecture/key-value-row-material-parity.md`) — adds a trailing `@children` slot for Toggle / IconButton / badge composition. Coexists with `value` / `value-unit` / `value-icon`; not yet implemented.
- A grand-total bar / horizontal-rule variant. The `emphasis: total` value already covers this typographically (heavier weight, larger size, deeper foreground). If a screen needs a stylistic-final-row treatment beyond `emphasis: total`, that's `SectionCard.footer` territory in Phase 2.
- `aria-label` override combining label + value into one screen-reader string. Both Texts are natively accessible — a screen reader walks the row and reads "Total" then "12.50 SAR" in natural order. Combining them with a synthesised "Total is 12.50 SAR" is tracked as Material-parity slice 3 (`architecture/key-value-row-material-parity.md`) — lands together with `interactive` + `clicked()`.

---

## Public API

### Properties (13 total)

A display-only primitive. The CLAUDE.md guidance ("5–10 properties for display-only") sits at the lower bound here; 13 is justified by the segmentation handling (the `unit-position` knob + `value-unit` + `value-icon` triad), the `wrap` / density / divider machinery that turns this into a real settings-row primitive, and the locale-stable height lock that the row-level properties drive.

**Content**

| Property      | Type     | Default | Notes |
|---------------|----------|---------|-------|
| `label`       | `string` | `""`    | Leading-side text. **Single-script content.** The consumer is responsible for not concatenating bidi-mixed text into this property; bidi content inside a single Text element triggers Slint issue #7267. |
| `label-icon`  | `string` | `""`    | Optional icon name (resolved through `IconFont.resolve`). Rendered adjacent to `label`. Position flips with `Locale.rtl` (LTR: icon-before-label; RTL: label-before-icon — mirroring Button's `icon-leading`). |
| `value`       | `string` | `""`    | Trailing-side text. **Single-script content** (same constraint as `label`). |
| `value-unit`  | `string` | `""`    | Optional unit/suffix rendered immediately adjacent to `value` in the trailing cluster (e.g. `"kg"`, `"%"`, `"SAR"`). Same `value-tone`, sized one step smaller via `Typography`. Reading position relative to `value` is controlled by `unit-position`. |
| `value-icon`  | `string` | `""`    | Optional icon rendered at the cluster-leading slot of the trailing cluster (e.g. trend arrows). Resolved through `IconFont.resolve`. Sits opposite the cluster-trailing `value-unit` slot — both flip physical sides together when `unit-position: leading` or under RTL with `unit-position: trailing`. |

**Typography & tone**

| Property      | Type       | Default   | Notes |
|---------------|------------|-----------|-------|
| `emphasis`    | `Emphasis` | `normal`  | `subtle` → `text-sm`, `weight-regular`, `Theme.muted-foreground` for both label and value (de-emphasised row). `normal` → `text-base` value, `text-sm` label, `weight-regular` value, `weight-regular` label. `strong` → `text-base` value, `weight-semibold` value (label unchanged). `total` → `text-lg` value, `weight-bold` value, `Theme.foreground` value (label unchanged). See [Emphasis resolution](#emphasis-resolution). |
| `value-tone`  | `Tone`     | `default` | Colours the value cluster (text + unit + icon). `default` resolves to whatever the emphasis dictates (`muted-foreground` for subtle, `foreground` for normal/strong/total). Other tones override: `success` → `Theme.success`, `destructive` → `Theme.destructive`, `warning` → `Theme.warning`, `info` → `Theme.info`, `muted` → `Theme.muted-foreground`, `primary` → `Theme.primary`. **Label is never toned** — it stays at `Theme.muted-foreground` regardless. |

**Layout & behaviour**

| Property        | Type        | Default | Notes |
|-----------------|-------------|---------|-------|
| `density`       | `Density`        | `default`            | `compact` → `padding-y: Spacing.sm (8px)`, `default` → `padding-y: Spacing.md (12px)`, `comfortable` → `padding-y: Spacing.lg (16px)`. Horizontal padding is always `0px` — KeyValueRow is meant to be placed inside a padded surface (Card / SectionCard) that owns the horizontal inset. |
| `unit-position` | `UnitPosition`   | `trailing`           | Chooses the reading position of `value-unit` relative to `value`. **`trailing` (default)** = unit reads AFTER value in both locales (LTR places unit on physical-RIGHT, RTL on physical-LEFT — the cluster mirrors with locale). **`leading`** = unit reads BEFORE value in both locales (currency-prefix style: `SAR 12.50` in LTR, `12.50 SAR` in RTL — cluster physical order flips relative to the default in BOTH locales). The flip is symmetric. See [Unit position and the segmentation principle](#unit-position-and-the-segmentation-principle). |
| `wrap`          | `bool`           | `false`              | When `false`, label and value use `no-wrap` + `elide` — long content truncates with `…` and the row stays single-line at the locked height. When `true`, both use `word-wrap` and the row grows vertically with the wrapped content; the locked height becomes a floor (single-line content still occupies one row's height). |
| `show-divider`  | `bool`           | `false`              | Renders a 1px `Theme.border` bottom border. Consumers showing a column of rows typically set this on every row except the last; the `show-divider: false` default optimises for the "single row inside a Card" case. |
| `tooltip`       | `string`         | `""`                 | Hover text. Useful when `value` truncates due to constrained width. Empty string disables. Hovers anywhere in the row (label or value cluster). |

**Debug**

| Property       | Type   | Default | Notes |
|----------------|--------|---------|-------|
| `debug-bounds` | `bool` | `false` | Magenta 2px solid border around the row + magenta 1px solid outline around each cluster (leading + trailing) to make the segmentation visible. Hierarchy is conveyed by thickness alone (2px root vs 1px clusters) — Slint 1.14's Rectangle border has no `border-style`, so dashed/dotted differentiation isn't available. No aria badge — KeyValueRow has no AT role (it is rendered as two adjacent accessible Text elements). |

### Callbacks

None. KeyValueRow is display-only. Hover/press/click handling lives in the parent Card if the row is meant to be interactive.

### What is **not** here

`interactive`, `clicked()`, `loading`, `disabled`, `aria-label`, `label-tone`, `numeric: bool` (replaced by the `unit-position` enum — see [Why an enum and not a `numeric` bool](#why-an-enum-and-not-a-numeric-bool)), `variant`, `shape`, `bordered`, `elevated`, depth properties, `padding-override`, `max-width`, `label-min-width`, `value-slot @children`. The interactive stack (`interactive` + `clicked()` + `disabled` + `aria-label`), the `@children` trailing slot, and `description` are tracked in `architecture/key-value-row-material-parity.md` for the next slices. See [Scope → out of scope](#scope) for the rest.

---

## New enum

`UnitPosition { trailing, leading }`. KeyValueRow also uses the existing `Emphasis`, `Tone`, `Density`.

`UnitPosition` lives in `enums.slint` and is re-exported from `lib.slint`. Default value: `trailing`. The enum is KeyValueRow-specific in v0.0.1 but is positioned where future numeric primitives (a hypothetical `Money` row, or a price-label component) can adopt it without redefinition.

---

## Density semantics

KeyValueRow's `density` maps to `padding-y` values that are intentionally tighter than Card's `padding` mapping at the same nominal level: `compact → Spacing.sm (8px)`, `default → Spacing.md (12px)`, `comfortable → Spacing.lg (16px)`. Card maps to `md / lg / xl` (12/16/24).

The same `Density` enum value produces different absolute pixel values in Card vs KeyValueRow. **This is the convention, not a contract violation.** `Density { compact, default, comfortable }` names the same relative gradient across all components — "tighter / standard / looser" — but the absolute pixel values are calibrated to each component's role. A container component (Card) and a content component (KeyValueRow) have different role-appropriate insets even at the same nominal density.

The composition works correctly: a `Card { padding: compact }` containing `KeyValueRow { density: compact }` produces `12px` (Card) + `8px` (Row) = `20px` from the Card's edge to the row's content. That is the consumer asking for "tighter than default everywhere," and it is what they get. There is no scenario where role-appropriate density mappings produce a visually wrong composition — by construction, "tighter" composed with "tighter" gives less total whitespace than "default" composed with "default."

This convention is codified in CLAUDE.md ("Density is per-component-tuned"); the per-component absolute mapping lives in each component's design doc.

---

## Font selection

Both Text elements (label and value clusters) use the same font family, selected per row by `Locale.current`: `Typography.font-family-ar` when `Locale.current == "ar"`, `Typography.font-family` otherwise. This is the only sensible default; the alternatives don't survive scrutiny.

### Options considered

**Option A — Per-row, locale-determined (chosen).** Both clusters render in the locale's font. In an Arabic-locale row, Latin digits like `"12.50"` render in the Arabic font's Latin glyphs (Cairo, Tajawal, and Noto Sans Arabic all include matched-x-height Latin glyphs). Matches iOS Arabic behaviour (SF Arabic renders Latin digits in Arabic-locale UI). Simplest implementation; one font property per row.

**Option B — Per-Text intrinsic, label-tracks-locale + value-always-Latin.** Rejected. Breaks the textual-Arabic-value case: `value: "داكن"` (the "Dark" theme value translated) would render in `Typography.font-family` (a Latin font like Inter), which either has no Arabic glyphs at all or substitutes via the system fallback — visually inconsistent across rows in the same screen.

**Option C — Per-Text consumer choice, `label-locale` / `value-locale` properties.** Rejected. Two extra properties on a display-only primitive for an edge case that option A handles. Pushes a font-selection problem to the consumer when iOS, Android, and the web have all settled on locale-determined font selection at the surface level.

**Option D — Per-cluster font driven by a direction/unit-position signal.** Tempting (numeric values are Latin → use Latin font; everything else → use locale font), and someone will propose it. Rejected for three reasons:

1. **Couples direction with script.** `unit-position` is a direction property (which side the unit reads on). Using it as a script signal conflates two orthogonal concerns. A future "Eastern Arabic digits" case (`٠١٢٣` with `unit-position: trailing` for currency-like layout) would render in the wrong font under this rule.
2. **Breaks textual-Arabic-value symmetrically.** `value: "داكن"` renders in Arabic font via option A. Under option D, any rule based on a direction property would mis-select the font whenever the value's script doesn't match the property's implied script (e.g., a numeric value formatted with Arabic digits). The failure mode is silent — visually wrong but compiles cleanly.
3. **iOS doesn't do this.** SF Arabic renders Latin digits inside Arabic-locale UI in the Arabic font family. The Arabic font's Latin glyphs are designed to coexist with the Arabic glyphs at matching x-height and weight. Forcing a Latin font into Arabic-locale rows breaks that visual coexistence.

Option A is correct. The script-vs-font choice stays the consumer's responsibility via pre-localization at the boundary (the consumer passes `label` and `value` already in the appropriate script for the locale); the library picks the matching font.

---

## Unit position and the segmentation principle

This is the load-bearing design decision in KeyValueRow. The rest of the API is straightforward; this is where the segmentation principle from CLAUDE.md becomes concrete.

### What `unit-position` chooses

`unit-position: UnitPosition { trailing, leading }` chooses the **reading position** of `value-unit` relative to `value` inside the trailing cluster. It is a semantic property — the consumer thinks in reading order ("does the unit come AFTER the value, or BEFORE it?"), not in physical sides. The library translates the reading-order choice into the right physical layout for the active locale.

The two values:

- **`trailing` (default)** — unit reads AFTER value. The standard convention for prices, quantities, percentages, and unit-suffixed numbers. In LTR the cluster renders `[icon, value, unit]` (unit on physical-RIGHT). In RTL the cluster mirrors to `[unit, value, icon]` (unit on physical-LEFT). In both cases the local reader's eye reaches the unit AFTER the value.
- **`leading`** — unit reads BEFORE value. The currency-prefix / accounting convention (`$12.50`, `SAR 100`). In LTR the cluster renders `[unit, value, icon]` (unit on physical-LEFT). In RTL the cluster mirrors to `[icon, value, unit]` (unit on physical-RIGHT). In both cases the local reader's eye reaches the unit BEFORE the value.

The flip is **symmetric across locales**. Locale picks the standard side of the cluster (which physical edge "reading-trailing" maps to); `unit-position` chooses whether the unit sits on that side or the opposite. The four observable layouts:

|                            | LTR cluster order        | RTL cluster order        |
|----------------------------|--------------------------|--------------------------|
| `unit-position: trailing`  | `[icon, value, unit]`    | `[unit, value, icon]`    |
| `unit-position: leading`   | `[unit, value, icon]`    | `[icon, value, unit]`    |

### Implementation rule

Inside `TrailingCluster`:

```slint
property <bool> leading-flip: unit-position == UnitPosition.leading;
property <bool> ltr-order:    Locale.rtl == leading-flip;   // XNOR
```

`ltr-order: true` → cluster renders `[icon, value, unit]`; `ltr-order: false` → cluster renders `[unit, value, icon]`. The XNOR truth table matches the matrix above. The variable is named `ltr-order` (preserves naming continuity with the prior `numeric`-era code); semantically it reads "icon-leads-the-cluster."

The cluster's *position in the outer row* still follows `Locale.rtl` alone — trailing physical side in LTR, leading physical side in RTL. `unit-position` only governs the cluster's *internal* order.

### Why an enum and not a `numeric` bool

The earlier version of this component shipped a `numeric: bool` switch that meant "the value cluster is LTR-atomic — never mirror it." That captured the textual-vs-numeric case but had two problems:

1. **Asymmetric semantics.** `numeric: true` only changed behavior in RTL — in LTR it was a no-op. A consumer reading the property name had no way to predict that. The cluster's internal order was an emergent property of two flags (`Locale.rtl` AND `numeric`) instead of one consumer choice.
2. **No currency-prefix path.** `numeric: true` locked the cluster to `[icon, value, unit]` regardless of locale, which is correct for `12.50 SAR` but wrong for `SAR 12.50` — there was no way to express the unit-before-value reading order at all.

`unit-position` is a single consumer choice that produces a predictable result in both locales and covers both readings. It is also a semantic property (unit's *reading position*) rather than a behavioral one (LTR-atomic *yes or no*), which keeps the API legible to consumers who don't know about Slint's bidi handling.

Options considered before settling on the enum:

1. **`value-direction: Direction { auto, ltr, rtl }`**. Rejected. Introduces a new enum (`Direction`) shared with no one, exposes three values when only two are useful (`rtl` explicit is never needed), and the property name describes the *mechanism* (direction) rather than the *user intent* (which side the unit reads on).
2. **`numeric: bool`** (the previous shipped API). Rejected on the two grounds above.
3. **Sniff for digits at runtime** (`if value.starts-with-digit -> ltr-atomic`). Rejected. Slint has no string introspection, and even if it did, "starts with a digit" is a lossy proxy (negative numbers start with `-`, percentages can be `<1%`, etc.).
4. **`unit-position: UnitPosition`** (chosen). Two discrete values, semantic names, symmetric behavior across locales, covers both conventions.

### When to set `unit-position: leading`

Default (`trailing`) is correct for almost every POS row — totals, line items, quantities, percentages, taxes, balances. The reader sees the magnitude first and the unit as a suffix.

Set `leading` when the convention is currency-prefix: `SAR 12.50`, `$100`, `€50`. The reader sees the currency tag first and the magnitude as a continuation. This pattern is common in accounting reports, formal invoices, and a subset of POS conventions.

### Why this doesn't apply to the label cluster

The label cluster (`[label-icon, label]`) always respects `Locale.rtl`. There is no analog to "unit position" for labels because the label has no second textual element to position. Labels are always reading-direction text; the icon sits at the cluster's reading-leading edge in both locales (mirroring Button's `icon-leading`). If a consumer puts a number in the label (`label: "1."` for a list-numbered row), the number renders LTR-internally inside the Text element regardless of cluster direction — same way Slint renders Latin digits in an Arabic paragraph today. No special handling needed.

### Relationship to CLAUDE.md's segmentation principle

The library-wide rule is: **no library component renders mixed-script content inside a single `Text` element.** Label and value are separate properties handled by separate Text elements; the value-and-unit pair is a single LTR-atomic sub-flow inside the trailing cluster, split into two Text elements (value + unit) so neither one ever holds bidi-mixed content.

LTR-atomic numeric rendering — the rule that a value+unit pair stays in the consumer's chosen reading order regardless of locale — is what `unit-position` implements. The choice of which reading order to use is now the consumer's call (`trailing` for value-then-unit, `leading` for unit-then-value), not the library's. The segmentation principle still holds without exception; LTR-atomic is now a per-row opt-in expressed via `unit-position` rather than a hardcoded behavior.

---

## Emphasis resolution

| Emphasis  | Label size / weight / color                    | Value size / weight / color (when `value-tone: default`) |
|-----------|------------------------------------------------|----------------------------------------------------------|
| `subtle`  | `text-sm` / `regular` / `muted-foreground`     | `text-sm` / `regular` / `muted-foreground`               |
| `normal`  | `text-sm` / `regular` / `muted-foreground`     | `text-base` / `regular` / `foreground`                   |
| `strong`  | `text-sm` / `regular` / `muted-foreground`     | `text-base` / `semibold` / `foreground`                  |
| `total`   | `text-sm` / `medium` / `muted-foreground`      | `text-lg` / `bold` / `foreground`                        |

**Why label only takes a weight bump at `total`.** A "TOTAL" row has a label that should hold its own next to the bolded value. For `strong`, the value is loud enough that re-weighting the label distracts from the contrast. For `subtle`/`normal`, the muted label is correct.

**Why label never gets a tone.** `value-tone: destructive` reads as "this value is negative / dangerous." `label-tone: destructive` would read as "this entire row is destructive" — which is what the *parent's* background tint would communicate (a destructive Card), not a per-row signal. Removing label-tone from the API removes the possibility of consumers using it to mean things it shouldn't.

**Why `value-unit` is one step smaller than `value`.** Unit suffixes (`kg`, `%`, `SAR`) are visually subordinate to the numeric magnitude. Material, iOS, and shadcn all render units smaller than the number. KeyValueRow renders the unit at one Typography step below the value's emphasis size (e.g., `value: text-base → unit: text-sm`; `value: text-lg → unit: text-base`).

---

## Sizing rules

KeyValueRow stretches horizontally to fill its parent and has a **locale-stable locked height** vertically. The lock is the load-bearing decision; the rest follows from it.

### Locked height

```
row-content-height = value-font-size × 1.6
row-total-height   = 2 × padding-y + row-content-height
```

When `wrap: false` (default), `preferred-height = min-height = max-height = row-total-height`. The row's height does NOT track `inner.preferred-height` — that would let the row grow when the active locale's font has a taller natural line-height than the previous locale's, which is exactly what happens between Inter and Noto Sans Arabic. Locking to a font-size multiplier produces a row whose height stays identical across en↔ar toggles.

The `1.6` multiplier is tight but works for the library's target fonts (Inter, Noto Sans Arabic, Cairo, Tajawal) at body sizes. Display fonts with extreme descenders may clip; if you discover clipping on a target font, bump the multiplier in `row-content-height` — the cost is a uniformly taller row at every density.

When `wrap: true`, the lock relaxes: `max-height` is removed and `preferred-height = layout.preferred-height`, so the row grows vertically with the wrapped content. `min-height` stays at `row-total-height` so single-line wrapped content still occupies the locked floor.

### Width and the no-spacer layout

```
[ LeadingCluster (stretch: 1.0, label anchored to reading-leading) | TrailingCluster ]
```

There is **no explicit spacer Rectangle** between the clusters. The leading cluster's `horizontal-stretch: 1.0` absorbs the remaining row width, and its inner `HorizontalLayout` sets `alignment: Locale.rtl ? LayoutAlignment.end : LayoutAlignment.start` so the label anchors to the reading-leading edge of the cluster (physical-LEFT in LTR, physical-RIGHT in RTL). The trailing cluster is intrinsic-width and pinned to the row's reading-trailing edge.

This layout is what makes elision work. With a stretch-Rectangle spacer between two non-stretching clusters, a long label grows the leading cluster's intrinsic width and pushes the row wider instead of eliding. With the leading cluster itself stretching, its label Text receives a bounded width and elides (or wraps, when `wrap: true`) cleanly inside it.

- **`horizontal-stretch: 1.0`** on the row root — fills available parent width.
- **No `max-width`** for now. Consumers needing a constrained-width row wrap KeyValueRow in a sized parent. This matches Card's removal of width-management from interior primitives — width concerns live in the surface, not in the content.

### Content-width inheritance (Slint quirk)

The root Rectangle binds `preferred-width: layout.preferred-width` explicitly. Slint does NOT auto-propagate intrinsic content size through nested Rectangles — without the explicit binding, an unbounded parent layout would see KeyValueRow as zero-width. Documented in HANDOVER quirk #15.

---

## Internal visual structure

```
KeyValueRow (root Rectangle — transparent; height locked when wrap is off)
├── layout HorizontalLayout
│   │   Outer LTR / RTL branching duplicates only the PLACEMENT of the two
│   │   clusters; the cluster bodies are defined once as inline sub-components.
│   │
│   ├── if !Locale.rtl: LeadingCluster   ← stretches (horizontal-stretch: 1.0)
│   ├── if !Locale.rtl: TrailingCluster  ← intrinsic width
│   ├── if  Locale.rtl: TrailingCluster
│   └── if  Locale.rtl: LeadingCluster
│
├── LeadingCluster (inline sub-component, Rectangle)
│   └── inner HorizontalLayout
│       │   alignment: Locale.rtl ? end : start
│       │   ── anchors label to the reading-leading edge of the cluster
│       ├── if !Locale.rtl && label-icon != "": Text (label-icon)
│       ├── Text (label, horizontal-stretch: 1.0, wraps or elides per `wrap`)
│       └── if  Locale.rtl && label-icon != "": Text (label-icon)
│
├── TrailingCluster (inline sub-component, Rectangle)
│   │   ltr-order := Locale.rtl == leading-flip   (XNOR)
│   │   true  → cluster renders [icon, value, unit]
│   │   false → cluster renders [unit, value, icon]
│   │
│   └── inner HorizontalLayout
│       │   alignment: center
│       ├── if  ltr-order && value-icon != "": Text (value-icon)   ← cluster-leading
│       ├── if !ltr-order && value-unit != "": Text (value-unit)   ← cluster-leading
│       ├── Text (value, wraps or elides per `wrap`)               ← cluster-middle
│       ├── if  ltr-order && value-unit != "": Text (value-unit)   ← cluster-trailing
│       └── if !ltr-order && value-icon != "": Text (value-icon)   ← cluster-trailing
│
├── divider Rectangle  (only when show-divider: true)
│   ├── y: parent.height - 1px
│   ├── height: 1px
│   ├── width: parent.width
│   └── background: Theme.border
│
├── debug-bounds outlines  (only when debug-bounds: true)
│   ├── magenta 2px border around root
│   ├── magenta 1px outline around LeadingCluster Rectangle
│   └── magenta 1px outline around TrailingCluster Rectangle
│
└── tooltip Rectangle (only when tooltip != "")
    ├── full-width TouchArea (consumes pointer events — see tooltip note)
    └── if hover: popup Rectangle with tooltip text
```

### Cluster mirroring strategy

Slint has no `direction: rtl` on layouts. Mirroring is achieved by branching the children declarations on a single flag per cluster. Two patterns:

**Leading cluster** — direction follows `Locale.rtl`:

```slint
inner := HorizontalLayout {
    spacing: Spacing.xs;
    alignment: Locale.rtl ? LayoutAlignment.end : LayoutAlignment.start;

    if !Locale.rtl && label-icon != "" : Text { /* label-icon */ }
    Text { /* label, horizontal-stretch: 1.0 */ }
    if  Locale.rtl && label-icon != "" : Text { /* label-icon */ }
}
```

The label Text appears once. In LTR the icon precedes it (declaration order = layout order); in RTL the icon follows it. The `alignment` flip anchors the label to the reading-leading edge regardless of which side has the icon.

**Trailing cluster** — direction follows the `ltr-order` XNOR of locale and unit-position:

```slint
property <bool> leading-flip: unit-position == UnitPosition.leading;
property <bool> ltr-order:    Locale.rtl == leading-flip;

inner := HorizontalLayout {
    spacing: Spacing.xs;
    alignment: center;

    // Cluster-leading slot (icon when ltr-order, unit when reversed).
    if  ltr-order && value-icon != "" : Text { /* value-icon */ }
    if !ltr-order && value-unit != "" : Text { /* value-unit */ }

    // Value always sits in the cluster middle.
    Text { /* value */ }

    // Cluster-trailing slot (unit when ltr-order, icon when reversed).
    if  ltr-order && value-unit != "" : Text { /* value-unit */ }
    if !ltr-order && value-icon != "" : Text { /* value-icon */ }
}
```

The value Text is declared once; the icon and unit are declared twice each (once at the cluster-leading slot, once at the cluster-trailing slot, gated by `ltr-order`). Only one of each pair is ever active. Pattern matches Button's `icon-leading` / `icon-trailing` branching. The named `ltr-order` property keeps the logic readable.

### Accessibility

KeyValueRow has **no parent `accessible-role`** — the root Rectangle stays transparent to the AT tree. Each Text element is natively accessible: a screen reader walking the page reads "Total" then "12.50 SAR" in document order, which (because the trailing cluster sits after the leading cluster in the children list, regardless of which side it visually renders on) matches the visual reading flow.

**Why no parent role.** Slint's `AccessibleRole` enum has no `definition` / `term` / `description` value. The closest mappings — `text`, `none` — don't add information beyond what the Text elements already expose. Synthesising an aria-label like `"Total: 12.50 SAR"` is the candidate flagged in [Scope → out of scope](#scope) and lands together with the interactive stack in Material-parity slice 3; for now, the natural walk is correct.

### Debug bounds

Three layers of magenta outline when `debug-bounds: true`:

1. **Root border** — 2px solid magenta around the row. Same convention as Card.
2. **Leading-cluster outline** — 1px solid magenta around the leading cluster.
3. **Trailing-cluster outline** — 1px solid magenta around the trailing cluster.

Hierarchy is conveyed by thickness alone (2px vs 1px), not border style — Slint 1.14's `Rectangle` exposes `border-width / border-color / border-radius` but no `border-style`, so dashed/dotted differentiation isn't available. The 2:1 ratio is sufficient to distinguish root from clusters at typical viewing sizes.

The cluster outlines are KeyValueRow-specific: the segmentation principle's whole point is that the two clusters are independent, and `debug-bounds` should make that visible at runtime. When investigating a layout bug ("why is my value rendering on the wrong side?"), seeing the cluster boundaries answers it immediately.

No aria badge — KeyValueRow has no AT role to be missing a name for. (Card uses the badge because it gains a role when `interactive`; KeyValueRow never does.)

### Tooltip TouchArea — gated on `tooltip != ""`

The tooltip requires a hover-detecting TouchArea spanning the row. **The TouchArea is rendered only when `tooltip != ""`.** Rows without a tooltip have no TouchArea at all.

This is a normative design decision, not an optimization. The dominant composition for KeyValueRow is inside an interactive Card — a settings-list pattern where each row sits inside a `Card { interactive: true; clicked => navigate(); }`. For the Card's `clicked()` to fire when the user taps anywhere on the row, the row must not have a TouchArea swallowing pointer events.

Slint's TouchArea, when present, captures pointer events within its bounds. Even when set to `enabled: false`, the event-propagation behaviour to parent TouchAreas is not reliable across Slint versions. The robust solution is structural: the TouchArea simply does not exist on rows without a tooltip.

Implementation:

```slint
// At the row's structural top, after the divider:
if root.tooltip != "" : tooltip-area := TouchArea {
    // Sized to cover the row; emits has-hover only — no clicked handler.
    // Tooltip rendering is gated on tooltip-area.has-hover.
}
```

Consumers wrapping KeyValueRow in an interactive Card and *also* wanting per-row tooltips have a real conflict: the row's TouchArea will block the Card's click. The current answer is "pick one — tooltip OR clickable row via the wrapping Card." Material-parity slice 3 introduces row-level `interactive` + `clicked()`, at which point the single row-level TouchArea handles BOTH hover (for the tooltip) AND click (for the row callback) — see `architecture/key-value-row-material-parity.md`. For consumers who need both Card-level interactivity AND a per-row tooltip, the open path is to render the tooltip from a parent TouchArea or wait for a later `tooltip-mode: enum { hover-area, on-cluster }` resolution that scopes the hover trigger more narrowly.

This trade-off is documented in `tooltip`'s doc-comment.

---

## Globals consumed

`Theme` (foreground, muted-foreground, border, tone colours via the `value-tone` resolution, tooltip-*), `Typography` (text-sm, text-base, text-lg, weights, font-family / font-family-ar via Locale switch), `Spacing` (Density preset mappings, cluster spacing `xs`), `Sizes` (`border-thin` for divider, icon sizes for label-icon / value-icon), `Animation` (tooltip fade only — no state animations otherwise), `Locale` (rtl, font selection), `IconFont` (resolve label-icon and value-icon names to codepoints).

Not consumed: `Depth` (no shadow), `CurrencyFormat` (KeyValueRow is generic; Money handles currency formatting and composes the rendered string the consumer passes in as `value` + `value-unit`), `Radius` (no rounded corners — KeyValueRow has no surface).

---

## Acceptance criteria (visual validation gate)

KeyValueRow is done when **every** cell of the matrix below renders correctly in `previews/key-value-row.slint` and in the playground section:

- **Emphasis (4):** `subtle / normal / strong / total` — each renders with the correct size/weight/colour for both label and value. The `total` row visibly stands out from the others when stacked in a list.
- **Value-tone (7):** `default / primary / success / destructive / warning / info / muted` — each tones the value cluster (value text + value-unit + value-icon) consistently without leaking into the label.
- **Density (3):** `compact / default / comfortable` — visible vertical-padding difference between the three; text size unchanged.
- **Show-divider (2):** `true / false` — divider renders at the bottom in `Theme.border` when true, absent when false. A 5-row column with divider-on-all-but-last reads as a clean list.
- **Label-icon (2):** with and without `label-icon`. With icon: icon sits on the leading edge of the leading cluster, flipping side with `Locale.rtl`.
- **Value-icon + value-unit (4):** `{icon, no-unit}`, `{no-icon, unit}`, `{icon, unit}`, `{no-icon, no-unit}` — each combination renders correctly within the trailing cluster.
- **Unit-position (2):** `unit-position: trailing / leading`. In LTR, `trailing` renders `[icon, value, unit]` and `leading` renders `[unit, value, icon]`. In RTL, `trailing` renders `[unit, value, icon]` (mirrors with locale) and `leading` renders `[icon, value, unit]` (cluster flipped from locale default). **The preview must include side-by-side rows showing both positions in BOTH locales — this is the rule that justifies the entire `unit-position` enum.**
- **Locale × unit-position matrix:** the four combinations `{LTR, RTL} × {trailing, leading}` rendered together. Validates the symmetric-flip semantic.
- **Wrap (2):** `wrap: false` — long label and long value both elide with `…`; row stays at the locked height. `wrap: true` — both grow vertically; row height grows with the wrapped content; single-line rows still occupy the locked floor.
- **Locale-stable height:** the same KeyValueRow rendered with `Locale.current = "en"` and `Locale.current = "ar"` has the same row-total-height pixel value at every density. Toggling Locale inside a Card containing rows must NOT resize the Card vertically.
- **Tooltip:** hover over a row with `tooltip: "Sample tooltip"` — tooltip appears after the standard delay.
- **Debug-bounds:** toggle on — three magenta outlines visible (root + two clusters). Toggle off — no outlines.
- **Composition smoke check:** at least one preview row showing KeyValueRow inside a Card (the dominant real-world composition). Padding inside the Card + zero horizontal padding on the row should produce the iOS-style settings-row look.

---

## Open questions deferred to a later slice

1. **`@children` value slot, `interactive` + `clicked()`, `description`, `avatar-image`.** All tracked in `architecture/key-value-row-material-parity.md` as ordered slices. All architectural decisions resolved there; awaiting implementation.
2. **Synthesised aria-label.** Currently the screen reader walks `label` then `value` as separate accessible nodes. If real screen-reader testing finds the walk disorienting (e.g., "12.50 SAR" being read without the preceding "Total" context in long lists), add an `aria-label` property that, when set, combines the contents into a single accessible label on a wrapping Rectangle with `accessible-role: text`. Lands together with the interactive stack in Material-parity slice 3.
3. **Label column alignment across rows.** Settings screens sometimes want all labels left-aligned at a consistent x-position regardless of label length. KeyValueRow's `LeadingCluster` stretches per row, putting the value at the trailing edge of *each row independently*. For aligned-label columns, the current answer is "use a GridLayout in the consumer." If real consumers reach for this pattern repeatedly, a `label-min-width: length` property is the candidate.
4. **Divider tone / inset.** Currently `Theme.border`, full-width. iOS list dividers are typically inset (don't extend under the leading icon column) and slightly muted. The current divider stays simple; revisit when the smoke-test rendering of `settings-display.slint` reveals whether this matters in practice.
5. **`value-icon-trailing`.** The current API has a single `value-icon` that sits at the cluster-leading slot. If trend-down indicators or "edit pencil" affordances at the cluster-trailing position become common, add later — or absorb into the `@children` slot from Material-parity slice 2.

---

## Build status (shipped)

KeyValueRow shipped across three commits:

1. **`docs(abdu-slint-ui): KeyValueRow design contract`** — earlier revision of this file (described the now-superseded `numeric: bool` API). Updated in place by this doc-sync pass.
2. **`feat(abdu-slint-ui): dark mode support + KeyValueRow scaffolding`** — initial component + preview + playground section, plus dark-mode rollout, landed in the same commit because the two slices were entangled at `lib.slint` and `playground.slint`.
3. **`feat(abdu-slint-ui): KeyValueRow unit-position semantic + locale-stable row height + wrap`** — `numeric: bool` → `unit-position: UnitPosition` rewrite, locked row height, and `wrap` property.

What's next, per `architecture/key-value-row-material-parity.md`:

1. **Slice 1 — `description: string`** (smallest surgery, biggest UX win).
2. **Slice 2 — `@children` trailing slot.**
3. **Slice 3 — `interactive` + `clicked()` + `disabled` + `aria-label`** (biggest surgery; mirrors Card's accessibility cascade).
4. **Slice 4 — `avatar-image` + companions** (lowest priority; defer if not POS-critical).

After the four slices: Phase 1 components are functionally complete, and the next step is the smoke test (`examples/settings-display.slint`) tracked separately under IMPL.md §1.7.

---

## Risks

- **The `unit-position` property's discoverability.** Consumers building POS screens will see "KeyValueRow has 13 properties," skim the docs, and miss `unit-position` until they hit an RTL rendering bug or a currency-prefix layout request. Mitigation: the playground section's code-snippet panel must always emit `unit-position` when it differs from the default, and the doc-comment must lead with the consumer's mental model ("which side does the unit READ on?") rather than the mechanism (XNOR with locale). The smoke test (`settings-display.slint`) will exercise both positions in both locales; if consumers consistently set the wrong value in the first pass, the property name or the documentation needs revisiting.
- **Cluster mirroring duplication.** The trailing cluster branches on `ltr-order = (Locale.rtl == leading-flip)`, with icon and unit declared twice each (cluster-leading and cluster-trailing slots, gated by `ltr-order`). Each pair has only one active member, but Slint's view tree sees both `if` blocks. If Slint's compiler doesn't dead-code-eliminate inactive `if` branches efficiently, this could matter at scale (a settings screen with 30 rows = 120 cluster `if` blocks). Mitigation: profile the smoke test if it feels sluggish; otherwise accept.
- **Locked row height clips display fonts.** `row-content-height = value-font-size × 1.6` is tight. Inter / Noto Sans Arabic / Cairo / Tajawal fit at body sizes; descender-heavy display fonts (handwriting, ornamental) may clip. Mitigation: bump the multiplier in `row-content-height` if real consumer fonts clip — the cost is a uniformly taller row at every density.
- **`wrap: true` defeats locale-stable height.** When a consumer enables wrap, the row's height becomes a function of how the active font wraps the content, which differs across locales. A 30-character label may wrap to two lines in Latin and three in Arabic, growing the row. This is the consumer's explicit opt-in to "let content drive height"; nothing the library can do about it without re-clamping wrapped content, which would defeat the purpose. Document in the doc-comment that locale-stable height only holds when `wrap: false`.
- **Value-unit Typography step-down.** Spec'd as "one Typography step below value." The implementation hardcodes a 4-case match (`subtle → text-xs`, `normal/strong → text-sm`, `total → text-base`) rather than computing the step at runtime, because Typography's scale doesn't have uniform "step-below" gaps that produce consistent visual proportion. Any future emphasis levels need a matching entry in `value-unit-font-size`.
- **No vertical alignment between label and value when their heights differ.** If `label-icon` is taller than the label text (or vice versa for the value cluster), Slint's HorizontalLayout center-aligns by default. For mixed icon+text rows this looks correct. If a future case (e.g., a label with `description` from Material-parity slice 1) needs baseline alignment between clusters, revisit at that point.
- **Divider rendering inside a Card's clipped surface.** Card has `clip: true` on its surface, which means a 1px divider at `y: row.height - 1px` renders correctly. But if Card's `padding-override` is `0.001px` (zero padding), the divider would touch the Card's rounded corners — visually awkward. Mitigation: when used inside a Card with non-zero padding, the divider sits inside the padded region, which looks correct. Document this in the doc-comment on `show-divider`: "The divider renders at the bottom edge of the row's bounding box. When placed inside a Card with rounded corners and zero padding, the divider may visually clip into the corner radius; in that case, set `show-divider: false` on the last row of the list."
- **Tooltip TouchArea blocks wrapping interactive Card.** The TouchArea rendered when `tooltip != ""` captures pointer events. A consumer wrapping a tooltip-bearing KeyValueRow inside a `Card { interactive: true; clicked => ... }` will find that the Card's `clicked` never fires. The current answer is "pick one — tooltip OR clickable row." Material-parity slice 3 introduces row-level `interactive` + `clicked()` and merges the tooltip's TouchArea with the interactive one, resolving this conflict at the row level for consumers who pick row-level interactivity over Card-level.
