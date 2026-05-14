# Segment-as-Cell Pattern

> Canonical reference. The structural pattern for direction-aware row primitives in `abdu-slint-ui`. Applies to `KeyValueRow`, future `ListTile`, `MoneyRow`, `SectionRow`, and any other row-shaped primitive that has to render correctly in both LTR and RTL locales.

---

## Scope

This pattern applies to **direction-aware row primitives** — single-row horizontal compositions whose internal element order must flip with `Locale.rtl`, and whose individual content pieces (text, icons, glyphs) need independent typography, color, alignment, and padding.

**In scope:**

- `KeyValueRow` (labelled-value row, the canonical case).
- Future row primitives: `ListTile` (avatar + primary text + secondary text + trailing action), `MoneyRow` (currency-prefix + amount + cents-suffix), `SectionRow` (header + chevron), and similar.
- Direction-aware cells inside those rows.
- Decorator primitives that wrap cells to add visual chrome or interactivity (`Badge`, `Tooltip`, `Pressable`).

**Out of scope** (not addressed by this pattern; tracked in the consultation companion):

- Grid-like multi-row layouts where rows align by columns.
- Vertically-stacked composite content within a single conceptual row (e.g., a label with a description below it). The pattern's cells are single-line by design; vertical stacking inside a cell breaks the cell-isolation guarantee.
- Bidi-mixed content inside one Text element. Per the library's segmentation principle, every Text holds one script direction; this pattern enforces that structurally.
- Non-row layouts (panels, grids, free-form compositions).

---

## The primitive family

| Primitive | Inherits | Role | Status |
|---|---|---|---|
| `Segment` | `Rectangle` | One piece of static content with controllable typography. The atomic cell primitive. | Canonical |
| `SegmentColumn` | `Rectangle` | Cell that vertically stacks N Segments via `@children`. Used wherever a cell needs two (or more) lines of content: KeyValueRow's `description`, RadioGroup/CheckboxGroup item labels with sub-text, DataTable cells with primary + supporting detail, ListTile's primary + supporting text. | Canonical |
| `Badge` | `Rectangle` | Wrapping decorator adding visual chrome (background, border-radius, padding) around `@children`. | Canonical |
| `Tooltip` | `Rectangle` | Wrapping decorator adding hover-tooltip behavior. Design deferred (see [Decorator status](#decorator-status)). | Planned |
| `Pressable` | `Rectangle` | Wrapping decorator adding click + hover + focus state. Design deferred. | Planned |
| slack `Rectangle` | (inline) | A bare `Rectangle { horizontal-stretch: 1; }` between the label-side and value-side cells in a row. The only stretching child of the row's outer layout. | Canonical |

`Segment`, `SegmentColumn`, and `Badge` are private library helpers (`components/_segment.slint`, `components/_segment-column.slint`, `components/_badge.slint`); they are not re-exported from `lib.slint`. Consumers compose with the public row primitives, not with cells directly.

---

## Segment

`Segment` renders one piece of content (a string of text, or an icon glyph treated as text) with independent typography, color, alignment, and horizontal padding. Each call site declares all properties explicitly; properties travel with the cell across LTR/RTL branches.

### Properties (10)

**Default-value philosophy:** content properties (`text`, `font-family`, `font-size`, `text-color`) have sentinel defaults that produce visibly-broken output when left unset (invisible 0px text in transparent color, system fallback font) — leaving them unset is a loud-failure signal that the row author forgot to specify the cell's role. Behavior properties (`font-weight`, `align-h`, `align-v`, `padding-h`, `wrap`, `elide`) have sensible defaults that match the dominant case and are usually fine to omit.

| Property | Type | Default | Purpose |
|---|---|---|---|
| `text` | `string` | `""` | The content. Empty string means the cell contributes zero width and is invisible. |
| `font-family` | `string` | `""` | Font family. Use `Typography.font-family` for Latin, `Typography.font-family-ar` for Arabic, `IconFont.font-family-name()` for icon glyphs. Empty falls back to Slint's system default font (visibly wrong — loud signal). |
| `font-size` | `length` | `0px` | Typography token. `0px` renders invisibly (loud signal that the role didn't set it). |
| `font-weight` | `int` | `400` | Typography token. |
| `text-color` | `color` | `transparent` | Text color. Named `text-color` rather than `color` because Slint reserves `color` on `Rectangle` (deprecated alias for `background`); re-declaring as an `in property` is a compile error. |
| `align-h` | `TextHorizontalAlignment` | `center` | Horizontal alignment of the Text inside the cell. See [Align-h rule](#align-h-rule) for which value to set per cell. Default is locale-neutral. |
| `align-v` | `TextVerticalAlignment` | `center` | Vertical alignment. Almost always `center`. |
| `padding-h` | `length` | `0px` | Per-cell horizontal padding. The cell owns its gap to its neighbors; the row's `HorizontalLayout` uses `spacing: 0`. |
| `wrap` | `bool` | `false` | When true, the text wraps; the cell's intrinsic height grows. |
| `elide` | `bool` | `false` | When true, the text elides with `…` on overflow. Off by default — the value Text being elide-on by default was the trigger for the bug class that motivated this pattern. |

### Surface category — what may and may not be added

Segment's surface is the **typography and intra-cell alignment of one piece of content**. Future properties may be added if they belong to this category — for example, `letter-spacing` (typography) or `line-height` (typography) are valid candidates if real needs arise.

The following are explicitly **not** Segment's responsibility and must not be added:

- **Visual chrome:** `background`, `border-color`, `border-width`, `border-radius`, `drop-shadow-*`. Use `Badge` (composition).
- **Spatial geometry beyond the cell's content box:** `padding-v` (the row owns vertical padding), `min-width` / `max-width` as consumer-controllable (cells size intrinsically), `horizontal-stretch` as a positive value (cells are inelastic per Invariant 7).
- **Interactivity:** `clicked()`, `pressed`, `has-focus`, `tooltip`. Use `Pressable` or `Tooltip` (composition).
- **Multi-line vertical content:** `description`-under-label or any other vertically-stacked sibling text. Cells are single-Text by design; vertical stacking breaks cell isolation (Invariant 2).

The rule isn't "no additions." It's "additions only within typography-and-intra-cell-alignment." That keeps the line principled instead of count-based.

### Implementation

```slint
component Segment inherits Rectangle {
    in property <string>                  text;
    in property <string>                  font-family;
    in property <length>                  font-size;
    in property <int>                     font-weight: 400;
    in property <color>                   text-color;
    in property <TextHorizontalAlignment> align-h: center;
    in property <TextVerticalAlignment>   align-v: center;
    in property <length>                  padding-h: 0px;
    in property <bool>                    wrap: false;
    in property <bool>                    elide: false;

    background:        transparent;
    horizontal-stretch: 0;
    visible:           root.text != "";
    preferred-width:   root.text != "" ? inner.preferred-width + 2 * root.padding-h : 0px;
    preferred-height:  root.text != "" ? inner.preferred-height                     : 0px;
    min-width:         root.text != "" ? inner.min-width      + 2 * root.padding-h : 0px;
    min-height:        root.text != "" ? inner.min-height                          : 0px;

    inner := Text {
        x: root.padding-h;
        width:  parent.width - 2 * root.padding-h;
        height: parent.height;
        text:        root.text;
        font-family: root.font-family;
        font-size:   root.font-size;
        font-weight: root.font-weight;
        color:       root.text-color;
        horizontal-alignment: root.align-h;
        vertical-alignment:   root.align-v;
        wrap:     root.wrap  ? word-wrap : no-wrap;
        overflow: root.elide ? TextOverflow.elide : TextOverflow.clip;
    }
}
```

Segment inherits Rectangle (not Text), even though the cell renders one Text. Inheriting Text directly produces blank renders in Slint 1.14 — the cell needs a Rectangle wrapper around a Text child regardless. This is a Slint 1.14 component-construction idiom, distinct from the intrinsic-size propagation issue tracked in [Slint version portability](#slint-version-portability).

Segments with `wrap: true` require a width-bounded parent. Slint's layout pass asks the Text for its preferred-height assuming an unbounded width; if the parent is also unbounded, the height resolves to one-line, then the width shrinks during the layout pass, wrap activates, height grows, and the layout pass iterates — producing a runaway preferred-height (observed in iteration: a preview window reporting 17000+ pixels of preferred-height for what should be a 700px viewport). This is a preview-time hazard (preview parents are often unbounded) and a discipline note for consumers: any page hosting a wrapping row must provide an explicit width via a containing `Card`, `ScrollView`, or width-constrained layout. Production composition through KeyValueRow inside a Card satisfies this naturally; standalone Segment usage in a preview does not, so preview files declare `wrap: no-wrap` on header/caption Texts even when the content would benefit from wrapping visually.

The inner Text is **absolutely positioned** inside the Rectangle (not a child of any layout). It has no Text siblings within the cell. Whatever Slint's layout pass does to "compress siblings" cannot apply because there are none. This is the structural escape from the original bug class.

### Align-h rule

Align-h matches the cell **content's natural reading direction**, which is sometimes branch-dependent and sometimes not:

- **Locale-dependent text** (label, description): align-h matches the branch. LTR branch → `left`; RTL branch → `right`. Under overflow this puts the ellipsis on the reading-trailing edge.
- **Locale-independent content** (numerals, icons, glyphs): align-h is fixed to the content's script direction. For Latin digits and icons that's `left` in both branches. The value `12.50` reads LTR-internal even in an RTL row.
- **Geometric-anchor cells** (the disclosure cell at the row's trailing edge, the status cell at the row's leading edge): align-h is set per branch to push the glyph against the row's outer edge. The choice is driven by the row's overall geometry, not the glyph's reading direction.

For single-glyph cells (label-icon, status-dot, disclosure) under normal-fit conditions, cell width equals glyph width plus padding, so align-h has no visible effect. The rule still applies — under overflow or if a future row hand-sets a wider min-width, align-h decides which edge stays anchored.

### Inner.preferred-width vs inner.width — not a cycle

Segment binds `preferred-width: inner.preferred-width + 2 * padding-h`, while inner Text reads `width: parent.width - 2 * padding-h`. This looks like a cycle but isn't:

- `preferred-width` is the **intrinsic** size — "this is how wide I want to be." Flows leaf → root.
- `width` is the **assigned** size — "this is how wide my parent gave me." Flows root → leaf.

Different properties, different propagation directions. No feedback loop.

This explicit binding is required because Slint 1.14 does not automatically propagate intrinsic content size through nested Rectangles (the previous `KeyValueRow` cluster pattern relied on the same explicit binding; see HANDOVER quirk #15). The pattern's Slint-version portability depends on this propagation behavior remaining consistent; future Slint versions may change intrinsic-size resolution, and this code would need re-verification.

---

## Badge

`Badge` is a wrapping decorator adding visual chrome — background, border-radius, padding — around any cell or set of cells placed in its `@children` slot. The wrapped cell stays a normal Segment; Badge owns the chrome.

### Properties (5)

| Property | Type | Default | Purpose |
|---|---|---|---|
| `show` | `bool` | `true` | Visibility gate. When `false`, the Badge contributes zero width and is invisible. See [The show: bool convention](#the-show-bool-convention). |
| `background-color` | `color` | `transparent` | Fill color. |
| `corner-radius` | `length` | `0px` | Corner radius. Named `corner-radius` (not `border-radius`) because Slint reserves `border-radius` on Rectangle; re-declaring it as an `in property <length>` is a compile error. Same idiom as Segment's `text-color` rename. |
| `padding-h` | `length` | `0px` | Horizontal padding inside the Badge, between its edges and its content. |
| `padding-v` | `length` | `0px` | Vertical padding inside the Badge. |

### Implementation

```slint
component Badge inherits Rectangle {
    in property <bool>   show: true;
    in property <color>  background-color: transparent;
    in property <length> corner-radius:     0px;
    in property <length> padding-h:         0px;
    in property <length> padding-v:         0px;

    background:        root.background-color;
    border-radius:     root.corner-radius;
    horizontal-stretch: 0;
    visible:           root.show;
    preferred-width:   root.show ? layout.preferred-width  : 0px;
    preferred-height:  root.show ? layout.preferred-height : 0px;
    min-width:         root.show ? layout.min-width        : 0px;
    min-height:        root.show ? layout.min-height       : 0px;

    layout := HorizontalLayout {
        padding-left:   root.padding-h;
        padding-right:  root.padding-h;
        padding-top:    root.padding-v;
        padding-bottom: root.padding-v;
        @children
    }
}
```

### Composition mechanism

A Badge wraps a Segment (or any other content) at the row call site:

```slint
Badge {
    show:             has-status;
    background-color: Theme.warning-soft;
    corner-radius:    Radius.sm;
    padding-h:        Spacing.xs;
    padding-v:        Spacing.xs;     // smallest defined Spacing tier
    Segment {
        text:        root.status-text;
        font-family: row-font;
        font-size:   Typography.text-xs;
        text-color:  Theme.warning-fg;
        align-h:     center;
        padding-h:   0px;     // Badge already padded
    }
}
```

The outer row HorizontalLayout sees Badge as one child with its computed preferred-width. Badge propagates intrinsic size from its inner HorizontalLayout (which contains the Segment via `@children`). No additional Slint feature is required; the pattern works in 1.14 with the same `@children` slot composition that `Card` uses.

---

## SegmentColumn

`SegmentColumn` is a cell that vertically stacks N Segments via `@children`. It's the second cell primitive in the family (alongside `Segment`). From the row's HorizontalLayout perspective, a SegmentColumn is one cell — a single slot in the horizontal sequence. From its own perspective, it's a vertical layout container for two-or-more-line content.

Common consumers: KeyValueRow's `description` (label + secondary text below), RadioGroup item labels (option name + description), CheckboxGroup item labels, DataTable cells (primary value + supporting detail), ListTile (primary + supporting text).

### Why this exists as a primitive (and not as inline Segments + a row-level 2-row VerticalLayout)

We considered two alternatives before settling on SegmentColumn:

1. **Per-row vertical layout** — make the row primitive a `VerticalLayout` of two `HorizontalLayout` rows when description-like content is present. Every consumer row primitive would re-implement vertical stacking, with its own height-lock semantics, its own description-presence predicate, its own per-state structure flip.
2. **Parameterized Segment** — add `description: string` and a built-in second-Text to Segment. Reverts to the parameterization failure mode (`Segment` is no longer pure typography, invariant 5 grows compound, chrome creep follows).

SegmentColumn is the third path: factor the vertical-stacking machinery into one primitive that's reused across every row that needs two-line cell content. Same composition principle as Badge — a thin wrapper that adds one structural responsibility and stays out of the way otherwise.

### Properties (2)

| Property | Type | Default | Purpose |
|---|---|---|---|
| `show` | `bool` | `true` | Visibility gate, same convention as Badge. When `false`, the column contributes zero width and is invisible. |
| `vstack-spacing` | `length` | `0px` | Vertical gap between stacked children. Default is `0px` because adjacent text lines typically share a line-height-derived natural gap; consumers set this only when they want extra breathing room. |

### Implementation

```slint
component SegmentColumn inherits Rectangle {
    in property <bool>   show: true;
    in property <length> vstack-spacing: 0px;

    background:        transparent;
    horizontal-stretch: 0;
    visible:           root.show;
    preferred-width:   root.show ? stack.preferred-width  : 0px;
    preferred-height:  root.show ? stack.preferred-height : 0px;
    min-width:         root.show ? stack.min-width        : 0px;
    min-height:        root.show ? stack.min-height       : 0px;

    stack := VerticalLayout {
        alignment: start;
        spacing: root.vstack-spacing;
        @children
    }
}
```

`alignment: start` is load-bearing: when a parent allocates the SegmentColumn more vertical space than its preferred-height (which routinely happens when the column is one cell among taller siblings in a HorizontalLayout), Slint's default `alignment: stretch` distributes the slack proportionally between children — pushing the secondary line away from the primary with an unwanted gap. `alignment: start` packs children top-down at their preferred heights and lets the slack become trailing whitespace below the last child. This matches the "stack" semantics consumers expect.

### Composition mechanism

A SegmentColumn wraps two or more Segments at the row call site:

```slint
SegmentColumn {
    Segment {
        text:        root.label;
        font-family: row-font;
        font-size:   label-font-size;
        font-weight: label-font-weight;
        text-color:  label-color;
        align-h:     left;
    }
    Segment {
        text:        root.description;     // self-zeros when ""
        font-family: row-font;
        font-size:   description-font-size;
        font-weight: Typography.weight-regular;
        text-color:  Theme.muted-foreground;
        align-h:     left;
    }
}
```

The consumer brings the typography — primary line uses its own font-size and color, secondary line uses smaller font and muted color. The column adds vertical stacking with zero per-cell knobs to configure typography.

### Empty-state behavior

When all children Segments are empty (their `text` is `""`), each contributes zero width and zero height (per Segment's empty-state behavior). The column's `stack.preferred-width = max(0, 0, ...) = 0` and `stack.preferred-height = sum of zeros plus spacings`. With width zero, the column renders as nothing visible regardless of height.

For consumers that want to explicitly hide the entire column (rather than relying on emptiness), the `show: bool` convention applies — same as Badge. Use the [row-derived-predicate idiom](#the-row-derived-predicate-idiom) to lift the predicate to a row-level derived property.

### Why `@children` instead of `primary-` / `secondary-` prefixed properties

A version of SegmentColumn that parameterized two slots (`primary-text`, `primary-font-family`, `primary-font-size`, ..., `secondary-text`, `secondary-font-family`, ...) would have ~20 properties. Each parameter prefixed twice. The API would also lock the column to exactly two children, breaking the future case where a DataTable cell wants three lines (value + delta + timestamp).

`@children` keeps the column thin (one real property, `vstack-spacing`), reuses Segment's typography contract for every child, and generalizes to N ≥ 2 segments. The trade-off is the consumer brings the Segments explicitly — but that consumer is the row primitive's source, which is already structured around per-cell call-site configuration. Same pattern, applied at one more layer.

### Composes with Badge and the slack Rectangle

`SegmentColumn`, `Segment`, and `Badge` compose freely. A two-line cell whose secondary line is a colored chip is just nested composition:

```slint
SegmentColumn {
    Segment    { text: root.label;        ... }
    Badge      {
        background-color: Theme.warning-soft;
        corner-radius:    Radius.sm;
        padding-h:        Spacing.xs;
        Segment { text: root.status; text-color: Theme.warning-fg; ... }
    }
}
```

Same `@children` slot mechanism throughout. No primitive special-cases another.

---

## The show: bool convention

Wrapping decorators (Badge, Tooltip, Pressable, future Appear) cannot infer emptiness from their `@children` because Slint 1.14 does not expose child intrinsic-size predicates to the parent at binding time. Without explicit coordination, an empty-content Badge still renders as a thin chrome strip (verified empirically: a Badge with `padding-h: 8px` wrapping an empty Segment renders as a 16px wide visible background).

**The convention:** every wrapping decorator exposes a `show: bool` property (default `true`) and gates `preferred-width`, `preferred-height`, `min-width`, `min-height`, and `visible` on it. Call sites set `show` to the same predicate driving the wrapped cell's content.

This is the composition equivalent of Segment's `text != ""` self-zeroing. Decorators externalize visibility coordination to the call site; cells handle it internally. Both produce a cell stack that contributes zero width when there's nothing to show.

### The row-derived-predicate idiom

When a decorator-wrapped cell's visibility depends on data, lift the predicate to a row-level derived property and reference it in both the decorator's `show` and the wrapped Segment's content:

```slint
// At the row level — single source of truth.
// Two derived properties because they have different types and bind to different
// consumer-facing properties — `has-status` is a bool that gates Badge.show;
// `status-text` is the string that fills Segment.text. Both derive from the
// same root state (root.show-status, root.status-label). Do NOT collapse them
// into one — the decorator needs a bool, the cell needs a string, and Segment's
// own `text != ""` self-zeroing keys off the string property.
property <bool>   has-status:  root.show-status && root.status-label != "";
property <string> status-text: root.show-status ? root.status-label : "";

// At each branch's call site — two declarative references to the lifted state.
Badge {
    show: has-status;
    background-color: status-color;
    ...
    Segment {
        text: status-text;
        ...
    }
}
```

The repetition is the price; making it formulaic — one source of truth, two declarative references — makes it auditable. Reviewers grep for the row-level derived property and confirm it's referenced in both places.

---

## The seven invariants

These are the structural guarantees the pattern provides. Every row primitive built on this pattern must satisfy all seven.

1. **One Text per cell.** Cells contain exactly one Text element, absolutely positioned inside the cell's Rectangle. No conditional Text siblings inside a cell. The bug class from compound-predicate sibling Texts is structurally impossible.

2. **Cells don't interact.** Touching one cell's properties never affects another cell. Layout-engine surprises are bounded to one cell at a time.

3. **The row never reaches into a cell.** Cell properties (text, font, color, alignment, padding) are bound at the cell's call site in the row's source. The row's outer layout decides cell order and slack position only.

4. **Direction handling lives in exactly one place.** The single `if Locale.rtl` at the row level chooses between two ordered cell sequences. No nested direction branches, no compound predicates inside layouts. Cells and decorators have no `Locale.rtl` references; the branch context determines what each call site declares.

5. **Empty cells contribute zero width.** Segment via `preferred-width: text != "" ? ... : 0px` (self-contained). Decorators via the `show: bool` convention (call-site coordinated). Empty content takes no layout space — even though the row's HorizontalLayout doesn't filter empty cells out.

6. **Each cell's role is encoded at its call site.** A cell that renders the label has its `text: root.label`, `font-family: row-font`, `font-size: label-font-size`, `color: label-color` written at the cell's declaration. Properties travel with the cell across LTR/RTL branches. When RTL flips the order, the cells themselves move in source order — content is never remapped between fixed-position slots.

7. **The row has exactly one stretching child — the slack Rectangle.** The auditable contract lives at the row's outer HorizontalLayout: `grep -A 30 "row := HorizontalLayout" key-value-row.slint` and confirm that only the bare slack `Rectangle { horizontal-stretch: 1; }` carries non-zero stretch. Cells and decorators declare `horizontal-stretch: 0` explicitly as defense-in-depth (Slint Rectangle defaults to 1, a verified landmine), but the auditable rule is at the call site, not per-primitive.

---

## Row composition contract

A row primitive built on this pattern follows the structure below. The example uses placeholders for what the row's specific cells render; concrete bindings live in the per-row design doc.

```slint
KeyValueRow {                          // pure-Slint Rectangle
    // Row-level derived state — properties computed once and referenced
    // by both LTR and RTL cell call sites. Lift everything that can be lifted.
    property <string> row-font:           Locale.current == "ar" ? font-ar : font-latin;
    property <length> label-font-size:    /* emphasis-driven */ ;
    property <color>  label-color:        Theme.muted-foreground;
    property <color>  value-color:        /* tone-resolved */ ;
    property <string> label-icon-glyph:   /* IconFont.resolve(label-icon) when non-empty */ ;
    property <bool>   has-status:         /* show-status && status-text != "" */ ;
    property <bool>   has-disclosure:     /* disclosure != none */ ;
    // ... etc

    row := HorizontalLayout {
        spacing: 0;                                  // cells own gaps via padding-h
        padding-top:    padding-y;
        padding-bottom: padding-y;

        // LTR branch — label-side leads, value-side trails.
        // The label cell is ALWAYS a SegmentColumn (not a bare Segment), so
        // description=""  vs description!="" doesn't require a 4-way branch.
        // The secondary Segment self-zeros when description is empty.
        if !Locale.rtl: Badge         { show: has-status; ... Segment { ... } }
        if !Locale.rtl: Segment       { /* label-icon */ ... }
        if !Locale.rtl: SegmentColumn { Segment { text: root.label; ... } Segment { text: root.description; ... } }
        if !Locale.rtl: Rectangle     { horizontal-stretch: 1; }    // slack
        if !Locale.rtl: Segment       { /* value-side-A */ ... }
        if !Locale.rtl: Segment       { /* value */      ... }
        if !Locale.rtl: Segment       { /* value-side-B */ ... }
        if !Locale.rtl: Segment       { /* disclosure */ ... }

        // RTL branch — mirror sequence.
        if  Locale.rtl: Segment       { /* disclosure */ ... }
        if  Locale.rtl: Segment       { /* value-side-A */ ... }
        if  Locale.rtl: Segment       { /* value */      ... }
        if  Locale.rtl: Segment       { /* value-side-B */ ... }
        if  Locale.rtl: Rectangle     { horizontal-stretch: 1; }    // slack
        if  Locale.rtl: SegmentColumn { Segment { text: root.label; align-h: right; ... } Segment { text: root.description; align-h: right; ... } }
        if  Locale.rtl: Segment       { /* label-icon */ ... }
        if  Locale.rtl: Badge         { show: has-status; ... Segment { ... } }
    }
}
```

Each cell is declared twice — once per branch. The expensive logic (which font, which size, which color, what glyph) is computed once at the row level via derived properties. Each per-branch cell declaration is then short, declarative, and self-explanatory.

The duplication is the cost the pattern accepts in exchange for cell isolation and the seven invariants. The cost is bounded — adding a new cell adds two declarations (one per branch) and one or more row-level derived properties, never a refactor of existing cells.

---

## Decorator status

| Decorator | Status | What's verified | What's deferred |
|---|---|---|---|
| `Badge` | Ready to ship | Composition mechanism (`@children` slot inside HorizontalLayout) verified empirically. Empty-content wart confirmed and resolved via `show: bool` convention. Explicit `horizontal-stretch: 0` confirmed necessary. | Nothing remaining for v1. Border-color / border-width may be added if real need arises (still within visual-chrome category). |
| `Tooltip` | Design deferred | Composition shape inherits from Badge: `@children` wrapper, `show: bool`, `horizontal-stretch: 0`. | Specific hard sub-problem: Slint 1.14's `PopupWindow` clips incorrectly when the host is inside a `ScrollView` — a concrete known issue. Three design options remain open: (a) render the popup as an absolutely-positioned Rectangle in the row's coordinate space (no clipping but limited to the row's bounds), (b) use `PopupWindow` and accept the ScrollView clip, (c) propagate tooltip state to a row-level or window-level overlay. Each has implications for tooltip-anchored-to-cell positioning, event handling, and accessibility. |
| `Pressable` | Design deferred | Composition shape inherits from Badge: `@children` wrapper, `show: bool`, `horizontal-stretch: 0`. | Specific hard sub-problems: (1) `TouchArea` inside nested HorizontalLayouts has order-of-declaration sensitivity for `clicked` event propagation — sibling TouchAreas can swallow events depending on z-order. (2) Hover-state scope is ambiguous: does the cell highlight, the whole row, or both? (3) Click-event ownership: cell-level click vs row-level click vs both. Each decision affects the public API and the accessibility model. |

---

## Caveats and known limitations

### Elide-under-extreme-overflow

Cells with `elide: true` have `inner.min-width ≈ 0` (a Text with elide can shrink to its ellipsis glyph). The pattern eliminates the bug class where elide cells collapse due to **sibling interactions inside the same layout** — Segment isolates each Text inside its own Rectangle, so sibling pressure can't reach the elide Text directly.

Under extreme overflow (the row is narrower than the sum of non-slack cells' preferred widths), elide cells still shrink toward zero. This is correct, intentional behavior — the slack Rectangle absorbs slack first by `horizontal-stretch` ordering; past that point, cells shrink in proportion to `preferred-width - min-width`, and elide cells naturally have the largest such delta.

The pattern doesn't prevent this. It prevents it from triggering on normal-fit layouts where there is no real overflow. The old cluster pattern was triggering this collapse on normal-fit layouts via sibling interactions, which is what made the regression baffling.

### Slint version portability

The pattern depends on three Slint 1.14 behaviors that are not specified to be stable across versions:

1. **Explicit propagation of intrinsic content size through nested Rectangles** (HANDOVER quirk #15). Segment and Badge bind `preferred-width: inner.preferred-width + ...` explicitly because Slint 1.14 doesn't auto-propagate. Future Slint versions may change this — usually in the safer direction (more auto-propagation), but the bindings still need re-verification.
2. **Rectangle default `horizontal-stretch: 1`.** Slint's default makes a bare Rectangle absorb slack in a HorizontalLayout. The pattern requires explicit `horizontal-stretch: 0` on every cell-shaped primitive. If Slint changes the default to 0, the explicit declarations become defensive; if it keeps it at 1, they remain mandatory.
3. **`@children` slot composition inside HorizontalLayout contexts.** Verified working in 1.14 by analogy to `Card` and by the empirical Badge test. The pattern requires it; a future Slint that changed how `@children` propagates intrinsic size would break Badge composition.

A version bump to Slint 1.x should run the empirical Badge test (5-line file: empty Segment inside Badge, measure preferred-width) before assuming the pattern still holds.

**Note on code blocks.** All code blocks in this document have been compiled against Slint 1.14. Specific syntactic constraints surfaced during verification are noted at the relevant property (e.g., `text-color`'s rename rationale, `TextOverflow.elide` / `TextOverflow.clip` enum qualification). Future doc updates that change code blocks should re-compile before shipping; a doc reader's first compile error against the canonical reference shifts the conversation from "is the pattern sound?" to "does this code even work?" — the wrong axis to lose credibility on.

### Mixed-height cells in a HorizontalLayout — optical alignment problem

When a row primitive's HorizontalLayout contains both single-line cells (e.g. value, value-unit, disclosure) AND a multi-line cell (e.g. a SegmentColumn holding primary + supporting text), the layout sizes every cell to the multi-line preferred-height. Single-line cells then render *inside* a taller allocated area, and the choice of `align-v` produces a visible defect either way:

- `align-v: center` (Segment's default): single-line cells render vertically centered in the tall allocated area — sitting in the gap between the multi-line cell's two lines, not next to either of them.
- `align-v: top`: each single-line cell aligns its *bounding-box top* to the layout top. But each font/icon has different leading above the cap-height, so the visible glyph-tops sit at visibly different Y coordinates depending on font-size and font family. Trailing content reads as a zigzag.
- `align-v: bottom`: same issue with descender-line variance.

Slint 1.14 has no first-class baseline alignment, so no `align-v` value fully solves this for mixed-height cells.

**Workaround**: split the row primitive into a primary HorizontalLayout (containing only single-line cells, all the same effective height) plus a second HorizontalLayout for the multi-line content rendered below the primary row. KeyValueRow uses this split — see [`key-value-row.md`](./key-value-row.md) → "Split-row structure". The leading content's "secondary line" is rendered as an indented row below the primary row rather than as the second line of a SegmentColumn cell inside the primary row.

This is a row-author decision, not a pattern requirement. SegmentColumn remains the correct primitive when every cell in the parent layout is multi-line (symmetric case). The split is needed only when the row has *asymmetric* cell heights — multi-line on one side, single-line on the other.

### What the pattern does not solve

- **Vertical stacking is solved by `SegmentColumn`** (see [SegmentColumn](#segmentcolumn)) — not a limitation. The pattern's atomic cell (`Segment`) is single-Text, but a two-line cell is composed via SegmentColumn wrapping two Segments. Cell isolation is preserved at the column boundary; the row's HorizontalLayout sees one cell regardless of internal line count. The optical-alignment caveat for *asymmetric* mixed-height rows is documented in [Mixed-height cells in a HorizontalLayout](#mixed-height-cells-in-a-horizontallayout--optical-alignment-problem) above.
- **Grid-like alignment across rows.** Settings sections where the value column is aligned across all rows require a parent grid context, which is row-external. The pattern is row-local; column alignment across multiple rows is not its job.
- **Mixed-script content inside one cell.** Per the library's segmentation principle (CLAUDE.md), each Text holds one script direction. A cell with `"إجمالي 12.50"` would render incorrectly in Slint 1.14 (issue #7267); the pattern enforces single-script-per-cell by structure but does not detect violations at compile time. The consumer is responsible for not concatenating bidi-mixed text into a single cell's `text` property.

---

## Appendix A — Origins (why this exists)

The previous `KeyValueRow` implementation used a different structural pattern: two cluster sub-components (`LeadingCluster`, `TrailingCluster`) duplicated across `if Locale.rtl` outer branches, with the trailing cluster internally containing four conditional Text positions plus an always-present value Text gated by compound predicates like `if (ltr-order && value-icon != "")`.

Under the state `Locale.rtl == true` and `unit-position == trailing` (so `ltr-order == false`), the rendered children of the trailing cluster became `[unit, value]`. Both Texts should have been visible. In practice, the value Text rendered as zero-width and disappeared, leaving only the unit on screen.

The root cause was the interaction of three things:

1. Compound `if` predicates inside the trailing cluster's `HorizontalLayout` produced different rendered-child sets per state.
2. The always-present value Text carried `overflow: TextOverflow.elide`, which makes a Text width-shrinkable to zero in Slint's layout pass.
3. When the elide-flagged value Text was preceded by a non-elide unit Text inside a layout where the parent absorbed slack via `horizontal-stretch: 1`, the layout pass gave the unit its natural width and assigned the elide Text its `min-width` — effectively zero.

The combination was fragile and asymmetric: two state branches of the same component produced different layout outcomes for what should have been a symmetric flip.

The bug class is **any HorizontalLayout containing both conditional children gated by compound predicates and an elide-flagged Text alongside non-elide siblings.** The old code was a textbook case.

The pattern in this document closes the bug class by structure:

- **One Text per cell** (Invariant 1) — no sibling Texts in any layout, ever. The compound-predicate path can't produce the bug because there are no siblings to interact with.
- **No compound predicates inside layouts** — the only `if` in the row's layout is the simple `if !Locale.rtl:` / `if Locale.rtl:` outer branch.
- **`overflow: elide` becomes opt-in** — Segment defaults `elide: false`. Cells that need elide opt in explicitly, and the structural isolation prevents the bug class from triggering.

The previous design's other features — locale-stable row height, per-locale font switch, the `unit-position` XOR semantic — are orthogonal and preserved as derived row-level state that flows into cell call sites.

---

## Appendix B — Worked example: KeyValueRow LTR branch

For reference, here is what one branch of a complete `KeyValueRow` body looks like under this pattern. The RTL branch is the mirror sequence with locale-dependent align-h values flipped per the [Align-h rule](#align-h-rule).

The example uses placeholder identifiers (`value-side-a-text`, `value-side-a-font`, `value-side-a-size`, `disclosure-glyph`, etc.) for the row-level derived properties — concrete bindings for `KeyValueRow` will live in `architecture/key-value-row.md`. The placeholders here keep the example general; what matters for the pattern's exposition is the shape of each cell declaration, not the specific contents of the derived properties.

> **IMPL-author note:** the tokens (`Spacing.*`, `Sizes.*`, `Theme.*`, `Typography.*`) shown below are illustrative. Before copy-pasting, verify each token exists by checking the active component's design doc and "Globals consumed" list against `globals/`. Some tokens that read intuitively (e.g., `Spacing.xxs`) may not be defined; substitute the smallest existing tier (`Spacing.xs`) or add the token to the global if the component genuinely needs a new tier.

```slint
// Status dot — leading edge of the row. A Segment with the status glyph.
// No Badge wrapper, no background — KeyValueRow's status is a colored dot,
// not a pill. The Segment self-zeros when `status-glyph` is empty (which
// happens when `show-status: false`), so no separate `show: bool` decorator
// coordination is needed for this cell.
if !Locale.rtl: Segment {
    text:        status-glyph;          // "●" when show-status; "" otherwise
    font-family: row-font;
    font-size:   Sizes.icon-xs;         // 12px — smaller than label-icon
    text-color:  status-color;
    align-h:     center;
    padding-h:   Spacing.xs;
}

// Label-icon — adjacent to label on the leading side.
if !Locale.rtl: Segment {
    text:        label-icon-glyph;
    font-family: IconFont.font-family-name();
    font-size:   icon-size;
    text-color:  label-color;
    align-h:     left;
    padding-h:   Spacing.xs;
}

// Label cell — primary label text plus optional description below.
// Always a SegmentColumn (not a bare Segment) so description=""  vs description!=""
// doesn't require branching the row layout. The description Segment self-zeros
// when empty, and the column collapses to look like a single-line cell.
if !Locale.rtl: SegmentColumn {
    Segment {
        text:        root.label;
        font-family: row-font;
        font-size:   label-font-size;
        font-weight: label-font-weight;
        text-color:  label-color;
        align-h:     left;
        padding-h:   Spacing.xs;
        wrap:        root.wrap;
    }
    Segment {
        text:        root.description;          // self-zeros when ""
        font-family: row-font;
        font-size:   description-font-size;     // text-xs / text-sm — smaller than label
        font-weight: Typography.weight-regular;
        text-color:  Theme.muted-foreground;
        align-h:     left;
        padding-h:   Spacing.xs;
        wrap:        root.wrap;
    }
}

// Slack absorber — the only stretching child of this layout.
if !Locale.rtl: Rectangle { horizontal-stretch: 1; }

// Value-side cell A — icon if unit-position is trailing, unit if leading.
if !Locale.rtl: Segment {
    text:        value-side-a-text;
    font-family: value-side-a-font;
    font-size:   value-side-a-size;
    text-color:  value-color;
    align-h:     left;
    padding-h:   Spacing.xs;
}

// Value — primary value text.
if !Locale.rtl: Segment {
    text:        root.value;
    font-family: root.value-monospace ? Typography.font-family-monospace : row-font;
    font-size:   value-font-size;
    font-weight: value-font-weight;
    text-color:  value-color;
    align-h:     left;
    padding-h:   Spacing.xs;
    wrap:        root.wrap;
}

// Value-side cell B — unit if unit-position is trailing, icon if leading.
if !Locale.rtl: Segment {
    text:        value-side-b-text;
    font-family: value-side-b-font;
    font-size:   value-side-b-size;
    text-color:  value-color;
    align-h:     left;
    padding-h:   Spacing.xs;
}

// Disclosure indicator — trailing edge of the row.
if !Locale.rtl: Segment {
    text:        disclosure-glyph;
    font-family: row-font;
    font-size:   icon-size;
    text-color:  Theme.muted-foreground;
    align-h:     right;
    padding-h:   Spacing.xs;
}
```

Nine cells: one optional Badge (status), one Segment (label-icon), one SegmentColumn wrapping two Segments (label + description), one slack Rectangle, four Segments (value-side-A, value, value-side-B, disclosure). Each ~8 lines of property bindings, reading top-to-bottom. No nested conditionals. No compound predicates. No `Locale.rtl` references inside any cell or decorator. The RTL branch is a similar block with the cell order reversed and align-h adjusted per [Align-h rule](#align-h-rule).

The row-level derived properties (`row-font`, `label-color`, `value-color`, `label-icon-glyph`, `has-status`, `value-side-a-text`, `value-side-a-font`, `value-side-a-size`, `disclosure-glyph`, etc.) live in a declared block above the layout and are computed once. Adding a new cell adds two per-branch declarations and one or more derived properties; it never requires refactoring existing cells.
