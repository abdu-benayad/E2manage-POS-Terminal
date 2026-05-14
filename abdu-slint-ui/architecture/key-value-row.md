# KeyValueRow — Design

> Per-component design doc. Sibling docs live under `abdu-slint-ui/architecture/`.
> Role: the *what and why* for KeyValueRow. Implementation details for the
> primitive family used to compose it (`Segment`, `SegmentColumn`, `Badge`)
> live in [`segment-pattern.md`](./segment-pattern.md) — this doc cites that
> reference for everything pattern-level and stays focused on KeyValueRow's
> own decisions.

---

## Purpose

A two-content display row: a `label` on the leading side, a `value` on the trailing side, with optional supporting elements (label-icon, description-under-label, value-icon, value-unit, leading status indicator, trailing disclosure indicator). The building block for totals, settings rows, summary breakdowns, key-value tables, dashboard rows — anywhere a single row needs to show "X: Y" with consistent typography, spacing, and direction handling.

**Why KeyValueRow exists as a primitive** (vs. consumers writing `HorizontalLayout { Text { ... } Rectangle { stretch } Text { ... } }` inline):

1. **It's the canonical incarnation of the single-script segmentation principle.** Every other place in the codebase that displays a labelled value today (settings screens, the various total rows in checkout, Z-report breakdowns) concatenates the label and value into one string or one Text element. That works in pure-Latin and pure-Arabic builds and breaks the moment a mixed-script case appears. KeyValueRow makes the correct pattern — separate Segments per content piece — the path of least resistance.
2. **Typography decisions belong in one place.** Emphasis levels (`subtle / normal / strong / total`) map to specific font weights, sizes, and colors. Encoding those in the component means every settings row, every total, every summary line picks the right typography without the consumer deciding.
3. **The RTL flip is non-trivial.** Label-icon position relative to label flips with `Locale.rtl`. Value cluster position in the row flips with `Locale.rtl`. Value cluster *internal* order flips with the XNOR of `Locale.rtl` and `unit-position`. Three rules, easy to get wrong; KeyValueRow encodes them once via the segment-as-cell pattern.
4. **Density and divider are settings-screen primitives.** Without them, consumers reach for inline `padding: ...` and `if last-row { ... }` patterns that the library exists to eliminate.

KeyValueRow shares with Card:

- **Display-only by default.** No `interactive` flag in v1.0 — if a consumer needs clickable rows, they wrap KeyValueRow inside an interactive Card. Row-level `interactive` + `clicked()` is planned for Material-parity slice 3.
- **`debug-bounds` instrumentation.** Magenta border around the row + 1px outlines around each cell when set.
- **No `variant` / no `tone` on the surface.** Only the *value* takes a tone (the label is always `Theme.muted-foreground`). Coloring the whole row implies semantics that aren't there.

KeyValueRow is **not** a composition of Card. A KeyValueRow inside a list does not have a per-row surface, border, or shadow — the *list* (typically a Card or SectionCard in Phase 2) carries those. KeyValueRow is a layout + typography primitive that renders directly into whatever surface contains it.

---

## Scope

**In scope (v1.0):**

- Two primary content slots (`label`, `value`) rendered as separate cells via Segment.
- Optional `label-icon` rendered in the leading cluster, position-flipped per `Locale.rtl`.
- Optional `description` rendered below `label` (two-line label cell via SegmentColumn).
- Optional `value-icon` rendered adjacent to `value` in the trailing cluster.
- Optional `value-unit` rendered adjacent to `value` in the trailing cluster.
- A `unit-position: UnitPosition { trailing, leading }` knob choosing the unit's reading position relative to value.
- Optional **status indicator** (small colored dot at the row's leading edge) gated by `show-status` and colored by `status-tone`.
- Optional **disclosure indicator** (`›` chevron or `↗` external glyph) at the row's trailing edge.
- `emphasis: Emphasis` mapping to font weight + size + color (`subtle / normal / strong / total`).
- `value-tone: Tone` colouring the value cluster (value text, value unit, value icon share the tone).
- `density: Density` mapping to vertical padding.
- `value-monospace: bool` for tabular-figures numeric values (uses `Typography.font-family-monospace`).
- `wrap: bool` deciding between elide-on-overflow (default) and word-wrap-with-growing-row.
- `show-divider: bool` rendering a 1px bottom border in `Theme.border`.
- `tooltip: string` for hover discoverability.
- `debug-bounds: bool` for layout debugging.

**Explicitly out of scope:**

- `interactive` / `clicked()`. Use Card with `interactive: true` wrapping a KeyValueRow. Adding row-level interactivity duplicates Card's machinery and re-opens accessibility decisions Card already settled. Tracked in `key-value-row-material-parity.md` slice 3.
- `loading`. Settings rows don't spin; settings *values* might be unknown ("—"), which is a string the consumer provides.
- `label-tone`. The label is always `Theme.muted-foreground`.
- `label-min-width` / column alignment between rows. Consumers wanting aligned columns across multiple rows wrap their rows in a parent layout that constrains widths.
- A `value` slot accepting `@children`. Tracked in `key-value-row-material-parity.md` slice 2.

---

## Public API (18 properties total)

Heavier than Material's `ListTile` (9 properties) and Fluent's `ListItem` (3 + slots) because KeyValueRow does work both libraries leave to consumer composition — locale-aware horizontal placement, two-line label structure, value-and-unit reading-order semantics, status/disclosure affordances. CLAUDE.md allows 15–25 properties for interactive primitives; 18 sits inside that range for a display-only primitive that's filling a real composition gap.

### Content (6)

| Property      | Type     | Default | Notes |
|---------------|----------|---------|-------|
| `label`       | `string` | `""`    | Leading-side primary text. **Single-script content** — do not concatenate bidi-mixed text into this string (Slint issue #7267). |
| `label-icon`  | `string` | `""`    | Optional icon name (resolved through `IconFont.resolve`). Rendered adjacent to `label`. Position flips with `Locale.rtl` (LTR: icon-before-label; RTL: label-before-icon). |
| `description` | `string` | `""`    | Optional secondary text rendered below `label`. When non-empty, the label cell becomes a [SegmentColumn](./segment-pattern.md#segmentcolumn) with two Segments (label primary + description secondary). When empty, the cell collapses to single-line via Segment's empty-state self-zeroing. Smaller font, muted color. |
| `value`       | `string` | `""`    | Trailing-side primary text. **Single-script content**. |
| `value-unit`  | `string` | `""`    | Optional unit/suffix rendered adjacent to `value` (e.g. `"kg"`, `"%"`, `"SAR"`). Reading position relative to `value` is controlled by `unit-position`. |
| `value-icon`  | `string` | `""`    | Optional icon rendered in the value cluster (trend arrows, status glyphs). Resolved through `IconFont.resolve`. |

### Typography & tone (3)

| Property         | Type       | Default | Notes |
|------------------|------------|---------|-------|
| `emphasis`       | `Emphasis` | `normal` | `subtle` / `normal` / `strong` / `total` — typography preset. See [Emphasis resolution](#emphasis-resolution). |
| `value-tone`     | `Tone`     | `default` | Colors the value cluster (value text + unit + icon). **Label is never toned** — stays `Theme.muted-foreground` regardless. |
| `value-monospace`| `bool`     | `false` | When true, value cell uses `Typography.font-family-monospace`. Critical for columns of numeric values that should align by decimal point (`42.00` / `6.30` / `2.50` / `50.80`). Unit and icon cells unaffected (their content has consistent width anyway). |

### Affordances (3)

| Property      | Type                  | Default | Notes |
|---------------|-----------------------|---------|-------|
| `show-status` | `bool`                | `false` | When true, renders a small colored dot (`●` at `Sizes.icon-xs = 12px`) at the row's leading edge (before label-icon). Useful for sync state, queue state, "unread" indicators, dashboard row status. **Dot, not pill** — see [Status indicator: dot, not pill](#status-indicator-dot-not-pill). |
| `status-tone` | `Tone`                | `Tone.muted` | Color of the status dot's glyph. Resolves via the same mapping as `value-tone` — `success` → `Theme.success`, `destructive` → `Theme.destructive`, etc. Only meaningful when `show-status: true`. |
| `disclosure`  | `DisclosureIndicator` | `none`  | `none` / `chevron` / `external`. `chevron` renders `›` in LTR, `‹` in RTL (auto-flips with locale). `external` renders `↗` in LTR, `↖` in RTL (drift toward physical-trailing edge of the row). Both sit at the row's trailing edge in their respective branches. |

### Layout & behaviour (5)

| Property        | Type           | Default    | Notes |
|-----------------|----------------|------------|-------|
| `density`       | `Density`      | `default`  | `compact` → `padding-y: Spacing.sm (8px)`, `default` → `padding-y: Spacing.md (12px)`, `comfortable` → `padding-y: Spacing.lg (16px)`. Horizontal padding is always `0px` — KeyValueRow is meant to be placed inside a padded surface (Card / SectionCard). |
| `unit-position` | `UnitPosition` | `trailing` | Chooses the reading position of `value-unit` relative to `value`. **`trailing` (default)** = unit reads AFTER value in both locales. **`leading`** = unit reads BEFORE value (currency-prefix style). See [Unit position and the segmentation principle](#unit-position-and-the-segmentation-principle). |
| `wrap`          | `bool`         | `false`    | Row-global. When `false`, label and value are single-line and elide on overflow. When `true`, both wrap; row grows vertically; height lock relaxes. `description`, when non-empty, also wraps according to this property. |
| `show-divider` | `bool`          | `false`    | Renders a 1px `Theme.border` bottom border. Consumers showing a column of rows typically set this on every row except the last. |
| `tooltip`      | `string`        | `""`       | Hover text. Useful when `value` truncates due to constrained width. Empty disables. Renders a TouchArea spanning the row — **a tooltip blocks click propagation to a wrapping interactive Card** (choose one: tooltip or clickable-row-via-Card). |

### Debug (1)

| Property       | Type   | Default | Notes |
|----------------|--------|---------|-------|
| `debug-bounds` | `bool` | `false` | Magenta 2px solid border around the row root. **v1.0 is row-only** — per-cell outlines deferred. See [Debug bounds](#debug-bounds). |

### Callbacks

None. KeyValueRow is display-only in v1.0. Hover/press/click handling lives in the parent Card if the row is meant to be interactive.

---

## New types introduced

### `DisclosureIndicator` enum

```slint
export enum DisclosureIndicator {
    none,        // no indicator
    chevron,     // `›` LTR, `‹` RTL
    external,    // `↗` LTR, `↖` RTL
}
```

Lives in `enums.slint`, re-exported from `lib.slint`. Two visible variants plus `none` (default). Glyph selection is locale-aware (the row computes the actual glyph as a derived property and binds it to the disclosure Segment's `text`).

### `Typography.font-family-monospace` global token

New token added to `globals/typography.slint`:

```slint
in-out property <string> font-family-monospace: "DejaVu Sans Mono";
```

Default chosen for Linux availability. Consumers override at startup if they bundle a different monospaced font (`"SF Mono"`, `"Cascadia Mono"`, `"JetBrains Mono"`, etc.). Slint 1.14 doesn't expose `font-variant-numeric: tabular-nums`, so "tabular figures" here means "pick a font whose glyphs are naturally equal-width."

---

## Composition over the segment-pattern foundation

KeyValueRow is composed entirely of primitives from [`segment-pattern.md`](./segment-pattern.md): `Segment`, `SegmentColumn`, `Badge`, plus the bare slack `Rectangle`. No new private helpers needed.

Each consumer-facing property routes through one or more cells. The mapping:

| Consumer property | Cell(s) in the row | Notes |
|---|---|---|
| `label` | `Segment` inside a `SegmentColumn` (label cell) | Primary line of the label cell. |
| `description` | `Segment` inside the same `SegmentColumn` | Secondary line. Self-zeros when empty; column collapses to single-line. |
| `label-icon` | `Segment` (icon font) | Position flips with locale per branch. |
| `value` | `Segment` | Center of the value-side group. |
| `value-unit` | `Segment` (one of two value-side slot positions) | Slot resolved by `unit-position` XNOR `Locale.rtl`. |
| `value-icon` | `Segment` (icon font, the other value-side slot) | Slot resolved as the opposite of `value-unit`'s slot. |
| `show-status` + `status-tone` | `Segment` (icon font glyph `●`) at the row's leading edge | The Segment's `text` is `"●"` when `show-status: true`, `""` otherwise. Empty `text` self-zeros the cell (no Badge `show: bool` coordination needed). `text-color` resolves from `status-tone`. Renders as a colored dot, not a pill — no background, no surrounding chrome. See [Status indicator: dot, not pill](#status-indicator-dot-not-pill). |
| `disclosure` | `Segment` (icon font, glyph from row-derived) | `text` is empty when `disclosure: none`, so the cell self-zeros (no separate `show` gate needed). |

The row's outer `HorizontalLayout` is a single sequence of cells with one `if Locale.rtl` branch (Invariant 4). Each cell is declared twice — once per branch — with the per-branch differences encoded at the call site (mainly `align-h`). Row-level derived properties provide the shared values (font, color, size, glyph) that both branches reference.

Per [Invariant 7](./segment-pattern.md#the-seven-invariants), the only stretching child of the row's outer HorizontalLayout is a bare `Rectangle { horizontal-stretch: 1; }` placed between the label-side and value-side cells.

For the structural details of how Segment, SegmentColumn, and Badge work — their property surfaces, empty-state behavior, the `show: bool` convention, the seven invariants — see [`segment-pattern.md`](./segment-pattern.md). This doc does not re-derive any of it.

---

## Row-level derived state

The properties below are computed once at the row level and referenced by both LTR and RTL cell call sites. Lifting everything liftable keeps each per-branch cell declaration short and trivial.

```slint
// ===== Font & icon-font selection ====================================

// Per-row font selection — both clusters use the same family, chosen by locale.
property <string> row-font:
    Locale.current == "ar" ? Typography.font-family-ar : Typography.font-family;

// ===== Density → vertical padding ====================================
//
// Tighter than Card's mapping (Card: md/lg/xl). KeyValueRow is content, not container.

property <length> padding-y:
      root.density == Density.compact     ? Spacing.sm
    : root.density == Density.comfortable ? Spacing.lg
    :                                       Spacing.md;

// ===== Emphasis → typography sizes & weights =========================
//
// See "Emphasis resolution" below for the full mapping.

property <length> label-font-size: Typography.text-sm;       // always text-sm regardless of emphasis

property <int> label-font-weight:
      root.emphasis == Emphasis.total ? Typography.weight-medium
    :                                   Typography.weight-regular;

property <length> description-font-size: Typography.text-xs; // smaller than label

property <length> value-font-size:
      root.emphasis == Emphasis.subtle ? Typography.text-sm
    : root.emphasis == Emphasis.total  ? Typography.text-lg
    :                                    Typography.text-base;

property <length> value-unit-font-size:
      root.emphasis == Emphasis.subtle ? Typography.text-xs
    : root.emphasis == Emphasis.total  ? Typography.text-base
    :                                    Typography.text-sm;

property <int> value-font-weight:
      root.emphasis == Emphasis.strong ? Typography.weight-semibold
    : root.emphasis == Emphasis.total  ? Typography.weight-bold
    :                                    Typography.weight-regular;

property <length> icon-size:
      root.emphasis == Emphasis.total ? Sizes.icon-md
    :                                   Sizes.icon-sm;

// ===== Color resolution ==============================================

property <color> label-color: Theme.muted-foreground;        // never toned

property <color> value-default-color:
      root.emphasis == Emphasis.subtle ? Theme.muted-foreground
    :                                    Theme.foreground;

property <color> value-color:
      root.value-tone == Tone.primary     ? Theme.primary
    : root.value-tone == Tone.success     ? Theme.success
    : root.value-tone == Tone.warning     ? Theme.warning
    : root.value-tone == Tone.destructive ? Theme.destructive
    : root.value-tone == Tone.info        ? Theme.info
    : root.value-tone == Tone.muted       ? Theme.muted-foreground
    :                                       root.value-default-color;

// ===== Status indicator state ========================================
//
// The status indicator is a Segment with the glyph `●`, not a Badge.
// One derived property for the glyph string — empty when show-status
// is false (which self-zeros the Segment cell), `"●"` when true. The
// dot's color is the same tone resolution as value-tone, applied to
// the Segment's text-color. No background, no padding around the dot —
// it's a colored glyph, not a pill.

property <string> status-glyph: root.show-status ? "●" : "";

property <color>  status-color:
      root.status-tone == Tone.primary     ? Theme.primary
    : root.status-tone == Tone.success     ? Theme.success
    : root.status-tone == Tone.warning     ? Theme.warning
    : root.status-tone == Tone.destructive ? Theme.destructive
    : root.status-tone == Tone.info        ? Theme.info
    :                                        Theme.muted-foreground;

property <length> status-size: Sizes.icon-xs;     // 12px — smaller than label-icon (16px) so the dot reads as indicator, not content

// ===== Disclosure glyph (locale-aware) ===============================

property <string> disclosure-glyph:
      root.disclosure == DisclosureIndicator.chevron  && !Locale.rtl ? "›"
    : root.disclosure == DisclosureIndicator.chevron  &&  Locale.rtl ? "‹"
    : root.disclosure == DisclosureIndicator.external && !Locale.rtl ? "↗"
    : root.disclosure == DisclosureIndicator.external &&  Locale.rtl ? "↖"
    :                                                                  "";

// ===== Label-icon glyph ==============================================

property <string> label-icon-glyph:
    root.label-icon != "" ? IconFont.resolve(root.label-icon) : "";

// ===== Value-cluster slot resolution =================================
//
// The XNOR rule from segment-pattern.md → Appendix B. Determines which
// of the two value-side slots (cluster-leading, cluster-trailing) gets
// the unit and which gets the icon. The cluster's three Segments
// (icon, value, unit) always render in source order; only WHICH slot
// holds the icon vs the unit changes per branch + unit-position.

property <bool> leading-flip: root.unit-position == UnitPosition.leading;
property <bool> icon-leads-value-cluster: Locale.rtl == leading-flip;

property <string> value-side-a-text:
    icon-leads-value-cluster
        ? (root.value-icon != "" ? IconFont.resolve(root.value-icon) : "")
        : root.value-unit;
property <string> value-side-a-font:
    icon-leads-value-cluster ? IconFont.font-family-name() : row-font;
property <length> value-side-a-size:
    icon-leads-value-cluster ? icon-size : value-unit-font-size;

property <string> value-side-b-text:
    icon-leads-value-cluster
        ? root.value-unit
        : (root.value-icon != "" ? IconFont.resolve(root.value-icon) : "");
property <string> value-side-b-font:
    icon-leads-value-cluster ? row-font : IconFont.font-family-name();
property <length> value-side-b-size:
    icon-leads-value-cluster ? value-unit-font-size : icon-size;

// ===== Row sizing ====================================================
//
// Locked-height for locale stability. See "Locked height" below.

property <length> row-content-height: root.value-font-size * 1.6;
property <length> row-total-height:   2 * root.padding-y + root.row-content-height;
```

Per-branch cell declarations reference these properties at the call site. Each cell is then `~6–8 lines` of trivial bindings — `text: status-glyph; font-family: row-font; font-size: ...; text-color: ...; align-h: left; padding-h: Spacing.xs;` — and reads top-to-bottom.

---

## Unit position and the segmentation principle

This is the load-bearing design decision in KeyValueRow's public API. The rest of the API is straightforward.

### What `unit-position` chooses

`unit-position: UnitPosition { trailing, leading }` chooses the **reading position** of `value-unit` relative to `value` inside the value-side cluster. It is a semantic property — the consumer thinks in reading order ("does the unit come AFTER the value, or BEFORE it?"), not in physical sides. The library translates the reading-order choice into the right physical layout for the active locale.

The two values:

- **`trailing` (default)** — unit reads AFTER value. The standard convention for prices, quantities, percentages, unit-suffixed numbers. In LTR the cluster renders `[icon, value, unit]` (unit on physical-RIGHT). In RTL the cluster mirrors to `[unit, value, icon]` (unit on physical-LEFT). In both locales the reader's eye reaches the unit AFTER the value.
- **`leading`** — unit reads BEFORE value. The currency-prefix / accounting convention (`$12.50`, `SAR 100`). In LTR the cluster renders `[unit, value, icon]` (unit on physical-LEFT). In RTL the cluster mirrors to `[icon, value, unit]` (unit on physical-RIGHT). In both locales the reader's eye reaches the unit BEFORE the value.

The flip is **symmetric across locales**:

|                            | LTR cluster order        | RTL cluster order        |
|----------------------------|--------------------------|--------------------------|
| `unit-position: trailing`  | `[icon, value, unit]`    | `[unit, value, icon]`    |
| `unit-position: leading`   | `[unit, value, icon]`    | `[icon, value, unit]`    |

The cluster's *position in the outer row* still follows `Locale.rtl` alone — trailing physical side in LTR, leading physical side in RTL. `unit-position` only governs the cluster's *internal* order, via the `icon-leads-value-cluster` row-derived predicate that maps to slot-A / slot-B in the value-side cell declarations.

### Why a `UnitPosition` enum and not a `numeric: bool`

The previous shipped API used `numeric: bool`. It captured the textual-vs-numeric case but had two problems: asymmetric semantics (it only changed behavior in RTL — a no-op in LTR), and no expression for the currency-prefix case (locked the cluster to `[icon, value, unit]` regardless of locale, which is correct for `12.50 SAR` but wrong for `SAR 12.50`).

`UnitPosition` is a single consumer choice that produces a predictable result in both locales and covers both readings. It is a semantic property (unit's *reading position*) rather than a behavioral one (LTR-atomic *yes or no*).

### When to set `unit-position: leading`

Default (`trailing`) is correct for almost every POS row — totals, line items, quantities, percentages, taxes, balances. The reader sees the magnitude first and the unit as a suffix.

Set `leading` when the convention is currency-prefix: `SAR 12.50`, `$100`, `€50`. Common in accounting reports, formal invoices, and a subset of POS conventions.

---

## Status indicator: dot, not pill

The status indicator is a **dot** — a colored glyph (`●`) sitting at the row's leading edge — not a pill (a chip with background and surrounding padding). Two reasons.

**Visual idiom.** Dots are the established convention for row-state indicators in iOS, macOS, and dashboard UI (mail unread state, presence indicators, sync status, queue position). Pills are the Material chip / counter idiom — they belong on the value side or as a separate badge primitive when the indicator carries quantitative content (`5 errors`, `3 unread`). KeyValueRow's status is qualitative state, not a count.

**Compositional simplicity.** A dot is a `Segment` with `text: "●"` and `text-color: status-color`. The Segment self-zeros when its `text` is empty (which happens when `show-status: false`), so the `show: bool` decorator-coordination convention from the segment pattern is not required for this property. A pill would need `Badge` wrapping the Segment, with `show: has-status` threaded through and `background-color` + `text-color` both derived from `status-tone` — more derived state, more cells per row, more potential for drift between the bool and the content.

If a quantitative status indicator becomes a real need (count badges, multi-character status pills), the right answer is a separate `value-side` slot — not a re-design of `show-status`. Keeping the dot/pill distinction prevents one property from drifting into two semantic roles.

The dot's size is `Sizes.icon-xs` (12px) — smaller than `label-icon` (which uses `Sizes.icon-sm`, 16px). This makes the dot read as an indicator, not as content competing with the icon. The size token is fixed; consumers don't get to resize the status dot.

---

## Emphasis resolution

`emphasis` is the typography preset gate. Four levels, each mapping to a coherent typography set:

| Emphasis  | Label font-size | Label weight | Value font-size | Value weight | Value-unit font-size | Default value color |
|-----------|-----------------|--------------|------------------|---------------|-----------------------|---------------------|
| `subtle`  | `text-sm` (14) | `regular`   | `text-sm` (14)   | `regular`     | `text-xs` (12)        | `muted-foreground`  |
| `normal`  | `text-sm` (14) | `regular`   | `text-base` (16) | `regular`     | `text-sm` (14)        | `foreground`        |
| `strong`  | `text-sm` (14) | `regular`   | `text-base` (16) | `semibold`    | `text-sm` (14)        | `foreground`        |
| `total`   | `text-sm` (14) | `medium`    | `text-lg` (18)   | `bold`        | `text-base` (16)      | `foreground`        |

The label font-size stays `text-sm` across all emphasis levels — the value carries the typographic weight; the label provides context. At `total`, the label weight bumps to `medium` so the bolded grand-total value has a label that holds its own.

`value-tone != default` overrides the value color (`Theme.success`, `Theme.destructive`, etc.); the emphasis resolution still drives font-size and weight.

The `description` line (when present) is always `text-xs / regular / muted-foreground` regardless of emphasis — secondary text doesn't compete with the primary line's emphasis.

---

## Locked row height

For locale stability, the row's height is **locked to a font-size-based computation** rather than tracking the inner layout's `preferred-height`:

```slint
property <length> row-content-height: root.value-font-size * 1.6;
property <length> row-total-height:   2 * root.padding-y + root.row-content-height;

preferred-height: root.wrap || root.description != "" ? layout.preferred-height : row-total-height;
min-height:       row-total-height;
max-height:       root.wrap || root.description != "" ? 99999px               : row-total-height;
```

Without this, the row's height tracks Text natural line-height, which differs per font. Noto Sans Arabic has a taller natural line-height than Inter/Noto Sans Latin, so toggling `Locale.current` between `"en"` and `"ar"` would resize the Card containing the row. Locking to `value-font-size × 1.6` produces locale-stable row heights at the cost of minor glyph clipping for fonts with extreme line-height metrics (acceptable for UI body fonts; descender-heavy display fonts are not the library's target).

**The lock relaxes when `wrap: true` OR `description != ""`** — wrap allows text to grow vertically; description introduces a second line that demands more height. In both cases, the locked height becomes a floor (single-line content still hits `min-height`), and the row grows as needed.

---

## Density semantics

KeyValueRow's `density` maps to `padding-y` values that are intentionally tighter than Card's `padding` mapping at the same nominal level:

| Density       | KeyValueRow padding-y | Card padding |
|---------------|------------------------|--------------|
| `compact`     | `Spacing.sm` (8px)     | `Spacing.md` (12px) |
| `default`     | `Spacing.md` (12px)    | `Spacing.lg` (16px) |
| `comfortable` | `Spacing.lg` (16px)    | `Spacing.xl` (24px) |

The same `Density` enum value produces different absolute pixel values in Card vs KeyValueRow. **This is the convention, not a contract violation** — see CLAUDE.md's "Density is per-component-tuned." Container components and content components have different role-appropriate insets even at the same nominal density.

Composition works correctly: a `Card { padding-density: compact }` containing `KeyValueRow { density: compact }` produces `12px` (Card) + `8px` (Row) = `20px` from the Card's edge to the row's content. "Tighter" composed with "tighter" gives less total whitespace than "default" composed with "default."

Horizontal padding is always `0px` in KeyValueRow — the row expects to be placed inside a padded surface (Card / SectionCard) that owns the horizontal inset.

---

## Accessibility

Both label and value (and description, when present) are rendered as native Text elements via Segment. They're naturally accessible: a screen reader walking the row reads "Total" then "12.50 SAR" then any description in natural reading order. No `accessible-role` is set on the row itself — KeyValueRow has no AT role; it's just two-or-three adjacent accessible Texts.

The status indicator and disclosure indicator are decorative glyphs without semantic AT meaning by themselves. If a row needs to convey "this is a settings link" or "this is a navigable item," the consumer wraps it in `Card { interactive: true, aria-label: "..." }` which provides the AT role at the container level.

A combined `aria-label` synthesising "Total is 12.50 SAR" is deferred to `key-value-row-material-parity.md` slice 3 (lands together with `interactive` + `clicked()`).

---

## Tooltip TouchArea — gated on `tooltip != ""`

The tooltip overlay (and its TouchArea) renders **only when `tooltip != ""`**. This matters for composition with an interactive Card:

```slint
Card {
    interactive: true;
    clicked => { /* row clicked */ }
    KeyValueRow { label: "Setting"; value: "On"; /* no tooltip */ }
}
```

With no tooltip, there's no row-level TouchArea, so the Card's TouchArea receives the click — the row is clickable through the Card. Setting a tooltip changes this: the row's TouchArea spans the row's bounds and consumes hover events. Clicks may or may not propagate depending on Slint's event order; the design constraint is: **choose tooltip OR clickable-row-via-Card, not both**.

The TouchArea also reserves space (it's a transparent overlay), which means a tooltip-bearing row inside a non-interactive Card stays inert for clicks (correct — the Card doesn't accept them anyway).

---

## Debug bounds

`debug-bounds: true` activates a **2px magenta border on the row root**. That's it for v1.0.

**Per-cell outlines are explicitly deferred.** The earlier design called for wrapping each cell in a conditional Rectangle so empty cells visually disappear while populated cells render a 1px magenta outline — useful for diagnosing "which cell is contributing this unexpected width." Two reasons to defer:

1. **Cost.** ~100 lines of wrapping Rectangle declarations across 16 cell positions (one per branch × 8 cells). And the wrapping reintroduces an intrinsic-size propagation risk that the segment pattern's `preferred-width` bindings were carefully designed to avoid.
2. **Demand.** During the foundation primitive work (Segment, SegmentColumn, Badge), per-cell outlines were never needed — diagnostic problems were solved via bisection or via the foundation previews' `Bound` helper (a preview-time visualization, not a runtime property). The diagnostic value is real but specific; it doesn't earn its keep until a concrete per-cell debugging need surfaces.

If a real need surfaces post-v1.0, the clean path is to extend `Segment` with an opt-in `debug-outline: bool` property — accepting the foundation surface creep as a controlled cost when there's cause. The escape hatch exists; we don't pre-pay for it.

For diagnosing cell-level contributions in the meantime: `architecture/screenshots/_segment-preview.png` and the live `previews/_segment.slint` already visualize cell boundaries clearly via the `Bound` helper. Most cell-level debugging during composition work can use the foundation previews directly.

No aria badge — KeyValueRow has no AT role.

---

## Globals consumed

- `Theme` — `foreground`, `muted-foreground`, `border`, `primary`, `success`, `warning`, `destructive`, `info`, `tooltip-background`, `tooltip-foreground`
- `Typography` — `font-family`, `font-family-ar`, `font-family-monospace`, `text-xs / sm / base / lg`, `weight-regular / medium / semibold / bold`
- `Spacing` — `sm`, `md`, `lg`, `xs`
- `Sizes` — `icon-xs` (status dot), `icon-sm` (label-icon, value-icon at most emphasis levels, disclosure glyph), `icon-md` (icons at `emphasis: total`)
- `Animation` — none (display-only, no transitions)
- `Locale` — `rtl`, `current`
- `IconFont` — `resolve`, `font-family-name`

---

## Acceptance criteria (visual validation gate)

A KeyValueRow build is "done" only when the `previews/key-value-row.slint` preview demonstrates every variant × size × state × locale combination correctly:

- **All 4 `emphasis` values** (subtle / normal / strong / total) render with the right typography per [Emphasis resolution](#emphasis-resolution).
- **All 7 `value-tone` values** (default / primary / success / warning / destructive / info / muted) color the value cluster correctly; label stays muted-foreground.
- **All 3 `density` values** map to the documented `padding-y`.
- **`unit-position` flip** verifies the XNOR truth table in both locales (4 cells of the matrix).
- **`description`** renders correctly when set, collapses cleanly when empty.
- **`show-status` + `status-tone`** renders status dot in the right color at the row's leading edge in both locales.
- **`disclosure: chevron` and `disclosure: external`** auto-flip glyphs with locale.
- **`value-monospace`** produces tabular alignment when columns of values stack vertically.
- **`wrap: true`** grows the row vertically; ellipsis on overflow when `wrap: false`.
- **`show-divider`** renders 1px Theme.border at the bottom.
- **`tooltip`** appears on hover.
- **`debug-bounds`** outlines every cell.
- **Locale toggle** (en ↔ ar) preserves row height (no resize when font changes from Inter to Noto Sans Arabic).
- **Dark mode toggle** flips Theme colors correctly across all variants.

The playground section (`abdu-slint-ui-playground/ui/sections/key-value-row.slint`) exposes every public property as an interactive control and renders the live code snippet.

---

## Open questions deferred to later slices

Tracked in `key-value-row-material-parity.md`:

- Slice 2: trailing `@children` slot for inline Toggle/IconButton/Badge composition. Coexists with `value` / `value-unit` / `value-icon`.
- Slice 3: row-level interactivity (`interactive: bool`, `clicked()`, `pressed-changed()`, `disabled: bool`, `aria-label: string`).

---

## Build status

**Foundation primitives shipped:** `Segment`, `SegmentColumn`, `Badge` — see `components/_*.slint`, regression previews in `previews/_*.slint`, durable screenshots in `architecture/screenshots/`.

**KeyValueRow itself:** unshipped at the time of this doc revision. The cluster-pattern implementation that previously existed at `components/key-value-row.slint` is broken (RTL + unit-position=trailing rendering bug, captured as the motivating bug in `segment-pattern.md` → Appendix A) and is scheduled for deletion before the rewrite.

Rewrite sequencing: delete old code → write `key-value-row-impl.md` → execute IMPL steps with verification between each → playground integration.

---

## References

- [`segment-pattern.md`](./segment-pattern.md) — Segment / SegmentColumn / Badge primitive family, the seven invariants, the `show: bool` convention, Slint 1.14 idioms.
- [`segment-pattern-consultation.md`](./segment-pattern-consultation.md) — reviewer questions on the pattern (sent in parallel; KeyValueRow proceeds while consultation runs).
- [`architecture/screenshots/`](./screenshots/) — durable visual evidence for all three foundation primitives.
- [`key-value-row-material-parity.md`](./key-value-row-material-parity.md) — slice 2 and 3 specifications for `@children` slot and row-level interactivity.
- CLAUDE.md → "Density is per-component-tuned" convention.
- CLAUDE.md → "Single-script segmentation principle."
