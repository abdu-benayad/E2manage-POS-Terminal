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
3. **The RTL flip is non-trivial.** Label-icon position relative to label flips with `Locale.rtl`. Value cluster position in the row flips. Value cluster *internal* order may or may not flip depending on whether the content is numeric. Three rules, easy to get wrong; KeyValueRow encodes them once.
4. **Density and divider are settings-screen primitives.** Without them, consumers reach for inline `padding: ...` and `if last-row { ... }` patterns that the library exists to eliminate.

KeyValueRow shares with Card:

- **Display-only by default.** No `interactive` flag in v1 — if a consumer needs clickable rows, they wrap KeyValueRow inside an interactive Card (this is exactly the dominant "settings item row" pattern in iOS).
- **`debug-bounds` instrumentation.** Magenta border + cluster outlines when set, matching the other primitives' debug story.
- **No `variant` / no `tone` on the surface.** Only the *value* takes a tone (the label is always Theme.muted-foreground). Coloring the whole row implies semantics that aren't there.

KeyValueRow is **not** a composition of Card. A KeyValueRow inside a list does not have a per-row surface, border, or shadow — the *list* (typically a Card or SectionCard in Phase 2) carries those. KeyValueRow is a layout + typography primitive that renders directly into whatever surface contains it.

---

## Scope

**In scope (v1):**

- Two primary content slots (`label`, `value`) rendered as **separately-anchored Text elements** — the segmentation principle made concrete.
- An optional `label-icon` rendered in the leading cluster, position-flipped per `Locale.rtl` (matches Button's `icon-leading` behaviour).
- An optional `value-icon` rendered in the trailing cluster, position fixed within the value cluster regardless of `Locale.rtl`.
- An optional `value-unit` rendered immediately adjacent to `value`, anchoring the LTR-atomic numeric pair (the segmentation principle's specific-case rule).
- A `numeric: bool` switch that gates LTR-atomic behaviour for the entire value cluster. `false` = value cluster respects `Locale.rtl` like the label cluster. `true` = value cluster stays LTR-atomic regardless. See [Numeric mode](#numeric-mode-and-the-segmentation-principle).
- `emphasis: Emphasis` mapping to font weight + size + color (`subtle / normal / strong / total`).
- `value-tone: Tone` colouring the value cluster (value text, value unit, value icon all share the tone).
- `density: Density` mapping to vertical padding (`compact / default / comfortable` → `Spacing.sm / md / lg`).
- `show-divider: bool` rendering a 1px bottom border in `Theme.border`.
- `tooltip: string` for hover discoverability (useful when `value` is truncated).
- `debug-bounds: bool` for layout debugging.

**Explicitly out of scope:**

- `interactive` / `clicked()`. Use Card with `interactive: true` wrapping a KeyValueRow. Adding an interactive variant duplicates Card's machinery and re-opens accessibility decisions that Card already settled.
- `loading`. Settings rows don't spin; settings *values* might be unknown ("—"), which is a string the consumer provides. If async-gated row content becomes a real need, the consumer renders a skeleton inside their list.
- `label-tone`. The label is always `Theme.muted-foreground` — settings screens with toned labels read as gimmicky. If a real screen needs it, revisit in v1.1; for v1, hold the line.
- `label-min-width` / column alignment between rows. Consumers wanting aligned columns across multiple rows wrap their rows in a parent layout (GridLayout if introduced, or a custom column constraint). KeyValueRow's job is to render one row correctly; alignment across rows is the parent's job.
- A `value` slot accepting `@children`. Tempting (drop a `Money` or a `StatusPill` in as the value), but the `value-unit` + `value-icon` properties cover the dominant inline cases. Slot-based composition is a v1.1 candidate once Phase 2's `Money` / `StatusPill` exist and the slot use case can be evaluated against concrete consumers.
- A grand-total bar / horizontal-rule variant. The `emphasis: total` value already covers this typographically (heavier weight, larger size, deeper foreground). If a screen needs a stylistic-final-row treatment beyond `emphasis: total`, that's `SectionCard.footer` territory in Phase 2.
- `aria-label` override combining label + value into one screen-reader string. Both Texts are natively accessible — a screen reader walks the row and reads "Total" then "12.50 SAR" in natural order. Combining them with a synthesised "Total is 12.50 SAR" is a v1.1 candidate if real screen-reader testing finds the natural walk disorienting.

---

## Public API

### Properties (12 total)

A display-only primitive. The CLAUDE.md guidance ("5–10 properties for display-only") sits at the lower bound here; 12 is justified by the segmentation handling (the `numeric` switch + `value-unit` + `value-icon` triad) and the density / divider machinery that turns this into a real settings-row primitive rather than a typography wrapper.

**Content**

| Property      | Type     | Default | Notes |
|---------------|----------|---------|-------|
| `label`       | `string` | `""`    | Leading-side text. **Single-script content.** The consumer is responsible for not concatenating bidi-mixed text into this property; bidi content inside a single Text element triggers Slint issue #7267. |
| `label-icon`  | `string` | `""`    | Optional icon name (resolved through `IconFont.resolve`). Rendered adjacent to `label`. Position flips with `Locale.rtl` (LTR: icon-before-label; RTL: label-before-icon — mirroring Button's `icon-leading`). |
| `value`       | `string` | `""`    | Trailing-side text. **Single-script content** (same constraint as `label`). |
| `value-unit`  | `string` | `""`    | Optional unit/suffix rendered immediately after `value` in the value cluster (e.g. `"kg"`, `"%"`, `"SAR"`). Same `value-tone`, sized one step smaller via `Typography`. Position within cluster is fixed (always after value) — does not flip with locale. The LTR-atomic anchor that motivates `numeric: true`. |
| `value-icon`  | `string` | `""`    | Optional icon rendered at the start of the value cluster (e.g. trend arrows). Resolved through `IconFont.resolve`. Position within the cluster is fixed regardless of `numeric` (always cluster-leading); position of the *whole cluster* in the row follows the row's direction rules. |

**Typography & tone**

| Property      | Type       | Default   | Notes |
|---------------|------------|-----------|-------|
| `emphasis`    | `Emphasis` | `normal`  | `subtle` → `text-sm`, `weight-regular`, `Theme.muted-foreground` for both label and value (de-emphasised row). `normal` → `text-base` value, `text-sm` label, `weight-regular` value, `weight-regular` label. `strong` → `text-base` value, `weight-semibold` value (label unchanged). `total` → `text-lg` value, `weight-bold` value, `Theme.foreground` value (label unchanged). See [Emphasis resolution](#emphasis-resolution). |
| `value-tone`  | `Tone`     | `default` | Colours the value cluster (text + unit + icon). `default` resolves to whatever the emphasis dictates (`muted-foreground` for subtle, `foreground` for normal/strong/total). Other tones override: `success` → `Theme.success`, `destructive` → `Theme.destructive`, `warning` → `Theme.warning`, `info` → `Theme.info`, `muted` → `Theme.muted-foreground`, `primary` → `Theme.primary`. **Label is never toned** — it stays at `Theme.muted-foreground` regardless. |

**Layout & behaviour**

| Property        | Type        | Default | Notes |
|-----------------|-------------|---------|-------|
| `density`       | `Density`   | `default` | `compact` → `padding-y: Spacing.sm (8px)`, `default` → `padding-y: Spacing.md (12px)`, `comfortable` → `padding-y: Spacing.lg (16px)`. Horizontal padding is always `0px` — KeyValueRow is meant to be placed inside a padded surface (Card / SectionCard) that owns the horizontal inset. |
| `numeric`       | `bool`      | `false`   | When `true`, the value cluster is LTR-atomic: `[value-icon, value, value-unit]` renders in that order regardless of `Locale.rtl`. When `false`, the cluster respects `Locale.rtl` (icon flips to the cluster-trailing side in RTL). **Set `true` whenever the value is a number** — see [Numeric mode](#numeric-mode-and-the-segmentation-principle). |
| `show-divider` | `bool`       | `false`   | Renders a 1px `Theme.border` bottom border. Consumers showing a column of rows typically set this on every row except the last; the `show-divider: false` default optimises for the "single row inside a Card" case. |
| `tooltip`      | `string`     | `""`      | Hover text. Useful when `value` truncates due to constrained width. Empty string disables. Hovers anywhere in the row (label or value cluster). |

**Debug**

| Property       | Type   | Default | Notes |
|----------------|--------|---------|-------|
| `debug-bounds` | `bool` | `false` | Magenta 2px solid border around the row + magenta 1px solid outline around each cluster (leading + trailing) to make the segmentation visible. Hierarchy is conveyed by thickness alone (2px root vs 1px clusters) — Slint 1.14's Rectangle border has no `border-style`, so dashed/dotted differentiation isn't available. No aria badge — KeyValueRow has no AT role (it is rendered as two adjacent accessible Text elements). |

### Callbacks

None. KeyValueRow is display-only. Hover/press/click handling lives in the parent Card if the row is meant to be interactive.

### What is **not** here

`interactive`, `clicked()`, `loading`, `disabled`, `aria-label`, `label-tone`, `value-direction` (an enum was considered; `numeric: bool` replaced it — see [Why a `numeric` bool and not a `value-direction` enum](#why-a-numeric-bool-and-not-a-value-direction-enum)), `variant`, `shape`, `bordered`, `elevated`, depth properties, `padding-override`, `max-width`, `min-height`, `label-min-width`, `value-slot @children`. See [Scope → out of scope](#scope) for rationale.

---

## New enum

None. KeyValueRow uses existing `Emphasis`, `Tone`, `Density`.

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

**Option D — Per-cluster font driven by `numeric` mode.** Tempting (numeric values are Latin → use Latin font; everything else → use locale font), and someone will propose it. Rejected for three reasons:

1. **Couples direction with script.** `numeric` is a direction property (LTR-atomic vs locale-aware). Using it as a script signal conflates two orthogonal concerns. A future "RTL numeric script" case (Eastern Arabic digits `٠١٢٣` with `numeric: true` for direction) would render in the wrong font under this rule.
2. **Breaks textual-Arabic-value symmetrically.** `value: "داكن"` renders in Arabic font via option A. Under option D, a consumer who set `numeric: true` mistakenly (or who has a numeric value formatted with Arabic digits) would get the wrong font. The failure mode is silent — visually wrong but compiles cleanly.
3. **iOS doesn't do this.** SF Arabic renders Latin digits inside Arabic-locale UI in the Arabic font family. The Arabic font's Latin glyphs are designed to coexist with the Arabic glyphs at matching x-height and weight. Forcing a Latin font into Arabic-locale rows breaks that visual coexistence.

Option A is correct. The script-vs-font choice stays the consumer's responsibility via pre-localization at the boundary (the consumer passes `label` and `value` already in the appropriate script for the locale); the library picks the matching font.

---

## Numeric mode and the segmentation principle

This is the load-bearing design decision in KeyValueRow. The rest of the API is straightforward; this is where the segmentation principle from CLAUDE.md becomes concrete.

### The two cases

Consider two real settings-screen rows:

```
Row A:  Theme              Dark            ← textual value
Row B:  Total              12.50 SAR       ← numeric value with unit
```

In RTL, with translated content:

```
Row A:                 المظهر    داكن       ← textual value, Arabic
Row B:                المجموع    12.50 ر.س  ← numeric value, Arabic label
```

The label cluster behaves the same in both cases: `[label-icon, label]` mirrors to `[label, label-icon]` in RTL (the icon stays on the leading edge, which is the right in RTL). Standard.

The value cluster is where the cases diverge:

- **Row A (textual value):** the value is plain text in the user's reading direction. The cluster `[value-icon, value, value-unit]` should mirror in RTL — icon to the right-of-text becomes icon to the left-of-text, matching the user's natural reading order.
- **Row B (numeric value):** the value is a number. Numbers are LTR-atomic per CLAUDE.md's segmentation rule (and per how Slint's bidi engine renders them anyway — Latin digits inside an Arabic paragraph render LTR-internally). The cluster `[value-icon, value, value-unit]` must NOT mirror — `"12.50 ر.س"` rendered as `"ر.س 12.50"` reverses the relationship between value and unit and reads as a different number entirely.

The library cannot detect "is this a number" by inspecting `value` (it's an opaque string). The consumer must tell the library.

### Why a `numeric` bool and not a `value-direction` enum

Options considered:

1. **`value-direction: Direction { auto, ltr, rtl }`** (enum, `auto = follow Locale.rtl`). Rejected. Introduces a new enum (`Direction`) only used by this one component, exposes three values when only two are useful (`rtl` explicit is never needed — that's what `auto` resolves to in RTL locales), and the property name doesn't communicate *why* the consumer would set it.
2. **`numeric: bool`** (chosen). Communicates intent directly. The consumer thinks "this row shows a number," they set `numeric: true`, and the library does the right thing. Discoverable via doc-comment ("set true when the value is a number — keeps the value, unit, and icon in left-to-right order regardless of locale").
3. **Sniff for digits at runtime** (`if value.starts-with-digit -> ltr-atomic`). Rejected. Slint has no string introspection, and even if it did, "starts with a digit" is a lossy proxy (negative numbers start with `-`, percentages can be `<1%`, etc.).
4. **Default `numeric: true`** (most POS values are numeric). Considered. Rejected because the default behaviour silently breaks textual rows in RTL — a "Theme: Dark" row in Arabic would render with the value icon on the wrong side. The cost of typing `numeric: true` on numeric rows is low; the cost of silently broken textual rows is high. Default to the safer behaviour (locale-aware) and require an explicit opt-in for LTR-atomic.

### The rule, restated

For consumers of KeyValueRow:

- **Value is text** (any script, any language) → leave `numeric: false`.
- **Value is a number, percentage, currency, quantity, date, code, or any other LTR-rendered content** → set `numeric: true`.

For the library implementation:

- The value cluster is a `HorizontalLayout` whose internal child order is `[value-icon, value, value-unit]`.
- That cluster's position in the outer row layout follows `Locale.rtl` (trailing side: physical right in LTR, physical left in RTL).
- When `numeric: true`, the cluster itself does not mirror — children stay in `[icon, value, unit]` order.
- When `numeric: false`, the cluster mirrors in RTL — children render in `[unit, value, icon]` order.

### Why this doesn't apply to the label cluster

The label cluster (`[label-icon, label]`) always respects `Locale.rtl` — there's no LTR-atomic case for labels. Labels are always reading-direction text; there's no analog to "numeric label." If a consumer puts a number in the label (`label: "1."` for a list-numbered row), the number renders LTR-internally inside the Text element regardless of cluster direction — same way Slint renders Latin digits in an Arabic paragraph today. No special handling needed.

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

KeyValueRow sizes to content vertically (label/value height + density padding) and stretches horizontally to fill its parent.

- **`horizontal-stretch: 1.0`** — fills available width.
- **`preferred-height`** = max(`label-cluster.preferred-height`, `value-cluster.preferred-height`) + `2 × density-padding-y`.
- **Spacer between clusters** — a `Rectangle { horizontal-stretch: 1.0; }` between the leading and trailing clusters absorbs excess width, pushing label to the leading edge and value cluster to the trailing edge.
- **No `max-width`** in v1. Consumers needing a constrained-width row wrap KeyValueRow in a sized parent. This matches Card's removal of width-management from interior primitives — width concerns live in the surface, not in the content.

### Content-width inheritance (Slint quirk)

If KeyValueRow ends up needing to drive `preferred-width` on the root Rectangle (because the parent layout doesn't propagate intrinsic content size), apply Card's pattern: bind `root.preferred-width` to the inner `HorizontalLayout.preferred-width`. This is the workaround documented in HANDOVER quirk #15.

---

## Internal visual structure

```
KeyValueRow (root Rectangle — transparent, sizing to content)
├── leading-cluster HorizontalLayout
│   ├── if Locale.rtl: label Text         ← RTL: label first, icon second
│   ├── if Locale.rtl && label-icon != "": label-icon Text (icon font)
│   ├── if !Locale.rtl && label-icon != "": label-icon Text
│   └── if !Locale.rtl: label Text        ← LTR: icon first, label second
│
├── Rectangle { horizontal-stretch: 1.0; }    ← spacer
│
├── trailing-cluster HorizontalLayout
│   │   When numeric: true → never mirrors regardless of Locale.rtl
│   │   When numeric: false → mirrors in RTL (children in reverse order)
│   │
│   ├── (cluster-leading) value-icon Text
│   ├── (cluster-middle)  value Text
│   └── (cluster-trailing) value-unit Text
│
├── divider Rectangle  (only when show-divider: true)
│   ├── y: parent.height - 1px
│   ├── height: 1px
│   ├── width: parent.width
│   └── background: Theme.border
│
├── debug-bounds outlines  (only when debug-bounds: true)
│   ├── magenta 2px border around root
│   ├── magenta 1px outline around leading-cluster
│   └── magenta 1px outline around trailing-cluster
│
└── tooltip Rectangle (only when tooltip != "" && hovered)
```

### Cluster mirroring strategy

Slint has no `direction: rtl` on layouts. Mirroring is achieved by branching the children declarations on `Locale.rtl`. Two cluster patterns:

**Leading cluster** — always respects `Locale.rtl`:

```slint
leading-cluster := HorizontalLayout {
    spacing: Spacing.xs;
    if !Locale.rtl && root.label-icon != "" : Text { /* label-icon */ }
    if !Locale.rtl                          : label-text := Text { /* label */ }
    if  Locale.rtl                          : label-text-rtl := Text { /* label */ }
    if  Locale.rtl && root.label-icon != "" : Text { /* label-icon */ }
}
```

**Trailing cluster** — branches on `numeric` AND `Locale.rtl`:

```slint
trailing-cluster := HorizontalLayout {
    spacing: Spacing.xs;
    // Effective direction: respect Locale.rtl ONLY when not numeric
    property <bool> mirror: Locale.rtl && !root.numeric;

    if !mirror && root.value-icon != "" : Text { /* value-icon */ }
    if !mirror                          : Text { /* value */ }
    if !mirror && root.value-unit != "" : Text { /* value-unit */ }

    if  mirror && root.value-unit != "" : Text { /* value-unit */ }
    if  mirror                          : Text { /* value */ }
    if  mirror && root.value-icon != "" : Text { /* value-icon */ }
}
```

The duplication is unfortunate but unavoidable without a `direction` property on Slint layouts. Pattern matches Button's `icon-leading` / `icon-trailing` branching. The named `mirror` property keeps the logic readable.

### Accessibility

KeyValueRow has **no parent `accessible-role`** — the root Rectangle stays transparent to the AT tree. Each Text element is natively accessible: a screen reader walking the page reads "Total" then "12.50 SAR" in document order, which (because the trailing cluster sits after the leading cluster in the children list, regardless of which side it visually renders on) matches the visual reading flow.

**Why no parent role.** Slint's `AccessibleRole` enum has no `definition` / `term` / `description` value. The closest mappings — `text`, `none` — don't add information beyond what the Text elements already expose. Synthesising an aria-label like `"Total: 12.50 SAR"` is the v1.1 candidate flagged in [Scope → out of scope](#scope); for v1, the natural walk is correct.

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

Consumers wrapping KeyValueRow in an interactive Card and *also* wanting per-row tooltips have a real conflict: the row's TouchArea will block the Card's click. The v1 answer is "pick one — tooltip OR clickable row." If both are genuinely needed, the consumer can render the tooltip themselves via a parent TouchArea or wait for a v1.1 resolution (likely a `tooltip-mode: enum { hover-area, on-cluster }` letting the consumer scope the hover trigger more narrowly).

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
- **Numeric (2):** `numeric: true / false` — both render correctly in LTR (visually identical). In RTL, `numeric: true` keeps `[icon, value, unit]` order; `numeric: false` mirrors to `[unit, value, icon]`. **This is the rule that justifies the entire `numeric` property; the preview must include side-by-side RTL rows showing both.**
- **RTL × numeric matrix:** the four combinations `{LTR, RTL} × {numeric: true, numeric: false}` rendered together. Numeric rows in RTL keep their cluster order; textual rows in RTL mirror their cluster order. Validates the segmentation principle as built.
- **Tooltip:** hover over a row with `tooltip: "Sample tooltip"` — tooltip appears after the standard delay.
- **Debug-bounds:** toggle on — three magenta outlines visible (root + two clusters). Toggle off — no outlines.
- **Composition smoke check:** at least one preview row showing KeyValueRow inside a Card (the dominant real-world composition). Padding inside the Card + zero horizontal padding on the row should produce the iOS-style settings-row look.

---

## Open questions deferred to Phase 1.5 / Phase 2

1. **`@children` value slot.** Allow consumers to pass `Money`, `StatusPill`, or any composite as the value. Currently held back because `value-unit` + `value-icon` cover the dominant cases and adding a slot complicates the segmentation handling (slot content's RTL behaviour is the consumer's problem, but the *position* of the slot in the row is still ours). Revisit after Phase 2 when `Money` and `StatusPill` exist and concrete use cases can be evaluated.
2. **Synthesised aria-label.** Currently the screen reader walks `label` then `value` as separate accessible nodes. If real screen-reader testing finds the walk disorienting (e.g., "12.50 SAR" being read without the preceding "Total" context in long lists), add a `aria-label` property that, when set, combines the contents into a single accessible label on a wrapping Rectangle with `accessible-role: text`.
3. **Multi-line value support.** Currently both label and value are single-line (`wrap: no-wrap`). Some settings descriptions need wrapping ("Backup runs automatically every 24 hours when the terminal is connected to Wi-Fi"). The Toggle component handles this via a `description` property; KeyValueRow could either grow a `value-description` property or rely on `wrap: word-wrap` on the value Text when content overflows. Defer to v1.1; for v1, long descriptions belong on Toggle / SectionCard rows, not KeyValueRow.
4. **Label column alignment across rows.** Settings screens sometimes want all labels left-aligned at a consistent x-position regardless of label length. KeyValueRow's `horizontal-stretch: 1.0` spacer puts the value at the trailing edge of *each row independently*. For aligned-label columns, the v1 answer is "use a GridLayout in the consumer." If real consumers reach for this pattern repeatedly, a `label-min-width: length` property is the v1.1 candidate.
5. **Divider tone / inset.** Currently `Theme.border`, full-width. iOS list dividers are typically inset (don't extend under the leading icon column) and slightly muted. v1 keeps the divider simple; revisit when the smoke-test rendering of `settings-display.slint` reveals whether this matters in practice.
6. **`value-icon-trailing`.** The original IMPL spec had a single `value-icon` (cluster-leading). If trend-down indicators or "edit pencil" affordances at the cluster-trailing position become common, add as v1.1.

---

## Build order

Two commits, matching the IconButton / Toggle / Card template:

### Commit 1 — `docs(abdu-slint-ui): KeyValueRow design contract`

1. Add this file (`architecture/key-value-row.md`).
2. No code changes. User reviews the doc — particular attention to:
   - The `numeric: bool` decision (vs. an enum, vs. default-true).
   - The "label is never toned" rule.
   - The 12-property surface (display-only primitives typically run 5–10; we're justifying 12).
3. User approves before Commit 2 lands.

### Commit 2 — `feat(abdu-slint-ui): KeyValueRow component + preview + playground section`

1. Write `components/key-value-row.slint` (~180 lines expected — smaller than Card because no surface / shadow / interactivity / focus machinery).
2. Re-export from `lib.slint`.
3. Write `previews/key-value-row.slint` covering the emphasis × tone × density × numeric × RTL matrix.
4. Write `abdu-slint-ui-playground/ui/sections/key-value-row.slint` exposing every public property as a control. Include a demo that renders 5 rows inside a Card to validate the dominant composition.
5. Wire the section into the playground sidebar.
6. `cargo check` (library) + `cargo build` (playground) clean.
7. User runs the playground, exercises the matrix, confirms visual quality (especially the numeric-in-RTL case — this is the validation that the whole `numeric` property is worth its weight).

After Commit 2: Phase 1 components are complete. The next step is the smoke test (`examples/settings-display.slint`), tracked separately under IMPL.md §1.7.

---

## Risks

- **The `numeric` property's discoverability.** Consumers building POS screens will see "KeyValueRow has 12 properties," skim the docs, and miss `numeric` until they hit an RTL rendering bug. Mitigation: the playground section MUST default the numeric demo rows to `numeric: true` (since most demo content will be numeric), and the doc-comment on the `numeric` property must lead with the use case ("set true when the value is a number") rather than the mechanism ("controls LTR-atomic behaviour"). The smoke test (`settings-display.slint`) will exercise both numeric and textual values; if `numeric` is consistently set wrong in the first pass, the property name or default needs revisiting before Phase 2.
- **Cluster mirroring duplication.** The trailing cluster branches on `mirror = Locale.rtl && !numeric`, with three children declared twice (once for each direction). This doubles the children count in Slint's view tree but each branch's `if` gates them — only one set is ever active. If Slint's compiler doesn't dead-code-eliminate inactive `if` branches efficiently, this could matter at scale (a settings screen with 30 rows = 60 cluster `if` blocks). Mitigation: profile the smoke test if it feels sluggish; otherwise accept.
- **Value-unit Typography step-down.** Currently spec'd as "one step smaller than value emphasis." If the Typography scale doesn't have a clean "one step smaller" mapping (e.g., `text-sm → text-xs` is the smallest step, but `text-lg → text-base` is a larger relative drop), the unit may look proportionally inconsistent across emphasis levels. Mitigation: pick the unit size per emphasis directly in the resolution rather than via a "one step down" rule; codify in the implementation as a 4-case match (`subtle/normal/strong/total → unit size`).
- **No vertical alignment between label and value when their heights differ.** If `label-icon` is taller than the label text (or vice versa for the value cluster), Slint's HorizontalLayout will center-align by default. For mixed icon+text rows, this may look off. Mitigation: pin `alignment: center` explicitly on both clusters; if center-alignment looks wrong in preview, switch to `alignment: baseline` (if Slint supports it — verify in implementation).
- **Divider rendering inside a Card's clipped surface.** Card has `clip: true` on its surface, which means a 1px divider at `y: row.height - 1px` will render correctly. But if Card's `padding-override` is `0.001px` (zero padding), the divider would touch the Card's rounded corners — visually awkward. Mitigation: when used inside a Card with non-zero padding, the divider sits inside the padded region, which looks correct. Document this in the doc-comment on `show-divider`: "The divider renders at the bottom edge of the row's bounding box. When placed inside a Card with rounded corners and zero padding, the divider may visually clip into the corner radius; in that case, set `show-divider: false` on the last row of the list."
