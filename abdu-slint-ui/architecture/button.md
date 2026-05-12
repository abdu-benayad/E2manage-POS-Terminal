# Button — Design

> **Retroactive consolidation.** Button was built before this `architecture/` convention existed; its design rationale was scattered across `HANDOVER.md` (API reference), `IMPL.md` (superseded original spec), source-file doc comments, and ~14 commits of iteration. This document captures the *why* of every load-bearing decision in one place. Where this doc disagrees with the source, the source wins — open an issue. Where it disagrees with the original `IMPL.md` Button section, this doc wins (`IMPL.md` is flagged as superseded).

---

## Purpose

The library's foundation interactive primitive. Every screen built against this library starts with Button. Every later primitive that needs a clickable surface inherits Button's patterns (depth math via `Depth`, accessibility cascade, two-layer surface/face for press feel).

Button is the load-bearing API of the library. Its public surface is the most-touched contract; changes to it cascade into every consumer screen.

---

## Scope

**In scope (v1):**

- Six visual variants: `default / destructive / outline / secondary / ghost / link`.
- Eight sizes: `xs / sm / md / lg / xl / xxl / hero / icon`. Heights 32 / 38 / 44 / 52 / 60 / 72 / 88 / 44px. The `icon` variant produces a square button — preserved for backward compatibility, though new icon-only callers should prefer `IconButton`.
- Four shape resolutions: `default / rounded / pill / square`. Default resolves against `Theme.button-shape`.
- Tone override (`primary / success / warning / destructive / info / muted`) that recolors any variant — gives consumers a "green outline button" without a new variant.
- Leading and trailing icons (`icon-leading`, `icon-trailing`) that respect RTL.
- Loading state: replaces content with a rotating spinner, blocks click, preserves width.
- Disabled state: visually muted, blocks click + keyboard activation.
- Toggle behaviour via `checkable` + `checked` (in-out).
- Tooltip with internal hover delay.
- `full-width` stretching via `horizontal-stretch`.
- Lower bound on width via `min-content-width` (renamed from `min-width` because Rectangle reserves that name).
- Full depth/lighting system: `elevated`, `shadow-elevation`, `shadow-color`, `shadow-direction`, `thickness`, `press-animation`.
- Escape hatches: `bg-color` (direct background override), `height-override` (free-form size with proportional font/icon/padding scaling).
- Accessibility cascade (`aria-label → tooltip → label → "Button"`) wired through Slint's `accessible-*` properties.
- `debug-bounds` instrumentation: magenta border on the surface Rectangle + magenta corner dot when the accessibility cascade is genuinely falling through to `"Button"`.

**Explicitly out of scope:**

- A separate "icon-only" primitive — that's `IconButton`. Button's `size: ButtonSize.icon` exists for callers who want the icon-button look but already have a `Button` instance for other reasons (e.g. inside a tightly-coupled control set).
- Compound interactive controls (split buttons, button groups, menu buttons) — these are domain compositions, not primitives.
- Built-in keyboard shortcuts (e.g. `accelerator: "Cmd+S"`) — that's an app-level concern.

---

## Public API

Button ships **25 public properties**. This is large relative to the original `IMPL.md` spec (15) because Button accreted convenience features during Phase 1 iteration — `tone` (variant×color decoupling), depth/lighting (`shadow-*`, `thickness`, `press-animation`), escape hatches (`bg-color`, `height-override`), and debug instrumentation (`debug-bounds`).

The 25-property surface is intentional: per `CLAUDE.md`, library primitives target 15–25 properties for discoverability and call-site ergonomics. Material UI's Button ships 18; Mantine's 25; Ant Design's 22. Button sits in the middle of this range.

### Identity & content

| Property         | Type      | Default | Notes                                                                                |
| ---------------- | --------- | ------- | ------------------------------------------------------------------------------------ |
| `label`          | `string`  | `""`    | Visible text. Empty renders an icon-only button (prefer `IconButton` for that case). |
| `icon-leading`   | `string`  | `""`    | Library-canonical name (resolved via `IconFont`) or raw codepoint / emoji. RTL-aware. |
| `icon-trailing`  | `string`  | `""`    | Same as `icon-leading` but on the reading-end side.                                  |
| `aria-label`     | `string`  | `""`    | Explicit accessibility name. See [Accessibility](#accessibility) below.              |

### Visual

| Property  | Type             | Default   | Notes                                                                  |
| --------- | ---------------- | --------- | ---------------------------------------------------------------------- |
| `variant` | `ButtonVariant`  | `default` | One of six. See [Variant × tone resolution](#variant--tone-resolution). |
| `size`    | `ButtonSize`     | `md`      | Resolves to `Sizes.button-{size}` for the preset height path.          |
| `shape`   | `Shape`          | `default` | `default` follows `Theme.button-shape`. `pill` → `radius = height/2`.  |
| `tone`    | `Tone`           | `default` | Overrides the variant's color family without changing its variant identity. |

### State

| Property     | Type            | Default | Notes                                                                       |
| ------------ | --------------- | ------- | --------------------------------------------------------------------------- |
| `disabled`   | `bool`          | `false` | Visually muted; blocks click and keyboard activation.                       |
| `loading`    | `bool`          | `false` | Rotating spinner replaces content; blocks click; preserves width.           |
| `full-width` | `bool`          | `false` | Stretches to parent's available width via `horizontal-stretch: 1.0`.        |
| `checkable`  | `bool`          | `false` | Toggle-button behaviour: clicks flip `checked`.                             |
| `checked`    | `bool` (in-out) | `false` | Controlled checked state. Drives the pressed visual when `checkable: true`. |
| `tooltip`    | `string`        | `""`    | Hover text; also feeds the accessibility cascade.                           |
| `min-content-width` | `length` | `0px`  | Lower bound on the button's `preferred-width`. Named to avoid Rectangle's reserved `min-width`. |

### Depth / lighting

Delegated to the `Depth` global. Each property here is a public input; resolution math is shared.

| Property            | Type        | Default       | Notes                                                                  |
| ------------------- | ----------- | ------------- | ---------------------------------------------------------------------- |
| `elevated`          | `bool`      | `true`        | Master shadow gate.                                                    |
| `shadow-elevation`  | `Elevation` | `sm`          | `none / sm / md / lg / xl`. Hover bumps one step via `Depth.bumped()`. |
| `shadow-color`      | `color`     | `transparent` | Transparent = Theme token for the level wins.                          |
| `shadow-direction`  | `int`       | `0`           | Degrees [0, 359]. 0 = light from above (shadow falls below).           |
| `thickness`         | `length`    | `0px`         | Visible 3D extrusion depth. >0 enables the two-layer surface/face.     |
| `press-animation`   | `bool`      | `true`        | Face slides down by 70% of `thickness` on press.                       |

### Escape hatches

These exist because real POS use cases occasionally need to override the curated palette/size. Their existence is a deliberate concession, not an oversight. IconButton intentionally omits `bg-color` as a first step toward tightening the curated surface; `height-override` carried over to IconButton because square sizing dictates that the same coefficient story applies.

| Property          | Type     | Default | Notes                                                                  |
| ----------------- | -------- | ------- | ---------------------------------------------------------------------- |
| `bg-color`        | `color`  | `transparent` | Direct background override. Bypasses variant. Variant still controls border and foreground color. |
| `height-override` | `length` | `0px`   | Free-form height; font / icon / padding scale proportionally with coefficients tuned at h=40px (md). |

### Debug

| Property        | Type   | Default | Notes                                                                  |
| --------------- | ------ | ------- | ---------------------------------------------------------------------- |
| `debug-bounds`  | `bool` | `false` | Magenta border on the surface Rectangle + magenta corner dot when the accessibility cascade is falling through to `"Button"`. Invisible in production. |

### Callbacks

| Callback                 | Fires when                                       |
| ------------------------ | ------------------------------------------------ |
| `clicked()`              | Tap, click, or Enter/Space while focused.        |
| `pressed-changed(bool)`  | Physical press state transitions.                |
| `hover-changed(bool)`    | Mouse enters / leaves.                           |
| `focus-changed(bool)`    | Keyboard focus gained / lost.                    |

---

## Internal visual structure

Button is **not a single Rectangle**. The structure is layered to make the depth/lighting system work physically rather than as a flat trick:

```
Button (root, transparent, sizing + events only)
├── focus-ring Rectangle (drawn outside the surface)
├── surface Rectangle  ← "base / skirt"
│   ├── full width × full height
│   ├── background = active-bg.darker(0.4)   (the visible side wall when thickness > 0)
│   ├── border-radius = resolved-radius
│   ├── outline border (variant == outline) OR debug magenta (debug-bounds)
│   ├── drop-shadow-* via Depth.*
│   │
│   └── face Rectangle  ← "top face"
│       ├── y = (visually-pressed && press-animation) ? thickness * 0.7 : 0
│       ├── height = parent.height - thickness
│       ├── background = filled-prominent ? @linear-gradient(face-bg) : active-bg
│       ├── animated y, height, background
│       │
│       ├── highlight Rectangle  (top half, filled-prominent only)
│       │   white-to-transparent vertical gradient — glossy sheen
│       │
│       └── content HorizontalLayout
│           ├── icon-leading Text  (RTL-aware position)
│           ├── label Text         (Locale-aware font family for Arabic)
│           ├── icon-trailing Text (RTL-aware position)
│           └── loading spinner Text (replaces all of the above when loading)
│
├── debug aria badge (debug-bounds + cascade-falls-through)
├── link underline Rectangle (variant == link && hovered)
├── TouchArea
├── FocusScope
└── tooltip Rectangle (when tooltip != "" && hovered && !disabled)
```

### Why two layers instead of one

A single-Rectangle Button with a bottom border or stripe cannot express physical depth. The visible "wall" of an extruded shape needs to be a darker color that's actually *behind* the face, not painted as a flourish on top. The two-layer structure:

- The `surface` is the full footprint of the button. It's a darker shade (`active-bg.darker(0.4)`) and carries the drop-shadow.
- The `face` sits on top, shorter by `thickness`, with the gradient and highlight. When `thickness > 0`, the bottom edge of the surface becomes the visible side wall.
- On press, the face slides down by 70% of thickness — the button visibly depresses into its own base. The base stays put; the shadow stays put; only the face moves. This is the difference between a button that "looks like" it depresses and one that *behaves* like it depresses.

When `thickness == 0`, the face fully covers the surface and the result is a flat button — visually indistinguishable from a single Rectangle. The two-layer structure carries no visual cost in the flat case.

### Why shadow lives on the surface and not the root

Slint's `lower_shadows` compiler pass silently drops `drop-shadow-*` properties on the root element of any component (`vendor/i-slint-compiler/passes/lower_shadows.rs:110-117`). Discovered the hard way; documented as HANDOVER quirk #1. The surface Rectangle is the lowest level where the shadow renders. The root keeps only sizing, opacity, focus-ring positioning, and event-scope responsibilities.

---

## Variant × tone resolution

Variants and tones compose orthogonally. A `default` variant with `Tone.success` becomes a green primary button; an `outline` variant with `Tone.success` becomes a green-bordered, green-text outline button. The matrix is computed in derived properties on the root:

```
variant-base-bg, variant-hover-bg, variant-foreground   ← from variant alone
tone-color, tone-foreground                              ← from tone alone
variant-is-solid, variant-is-filled-prominent            ← variant classification
base-bg, hover-bg, foreground-color                      ← variant × tone composition
resolved-border-color                                    ← only relevant for outline variant
```

### Rules of composition

- **No tone (`Tone.default`):** variant colors apply unchanged.
- **Tone set, solid variant** (`default / destructive / secondary`): tone replaces the base background. Hover background is `tone-color.darker(15%)`. Foreground uses `tone-foreground` for contrast.
- **Tone set, outline variant:** tone colors the border and text; background stays `Theme.surface`. Hover background is `tone-color.with-alpha(0.12)` (a tinted wash).
- **Tone set, ghost variant:** tone colors the text; background stays transparent. Hover background is the tinted wash.
- **Tone set, link variant:** tone colors the text and the hover underline. No background.

### Filled-prominent classification

`variant-is-filled-prominent = (variant == default || variant == destructive)`. These are the two variants that get the gradient face and the top-half highlight overlay. `secondary` is solid but quieter — no gradient, flat fill, to keep it visually subordinate to `default`. `outline / ghost / link` are non-solid and get flat backgrounds (or transparent).

---

## Depth integration

The six public depth properties are direct inputs. The visual layer threads them through three derived properties and four `Depth.*` function calls:

```slint
property <Elevation> eff-level:     Depth.bumped(root.shadow-elevation, touch.has-hover);
property <bool>      apply-shadow:  Depth.applies(root.elevated, root.disabled, root.shadow-elevation);
property <length>    eff-magnitude: Depth.magnitude(root.eff-level);

surface := Rectangle {
    drop-shadow-blur:     root.apply-shadow ? Depth.blur(root.eff-level) : 0px;
    drop-shadow-offset-x: root.apply-shadow ? Depth.offset-x(root.shadow-direction, root.eff-magnitude) : 0px;
    drop-shadow-offset-y: root.apply-shadow ? Depth.offset-y(root.shadow-direction, root.eff-magnitude) : 0px;
    drop-shadow-color:    root.apply-shadow ? Depth.color-of(root.eff-level, root.shadow-color) : #00000000;
    ...
}
```

`eff-magnitude` is hoisted out as a property because both `offset-x` and `offset-y` depend on it, and we want the angle projection computed from a single resolved magnitude. The hover bump (`Depth.bumped`) and disable gating (`Depth.applies`) are pure functions reading their inputs explicitly.

`thickness` and `press-animation` are visual-structure properties, not shadow properties — handled directly on the face Rectangle (its `y` and `height` bindings).

---

## Accessibility

Button wires Slint's `accessible-*` properties to the platform AT tree (AT-SPI on Linux, UI Automation on Windows, NSAccessibility on macOS):

```
accessible-role: button;
accessible-label:
      root.aria-label != "" ? root.aria-label
    : root.tooltip    != "" ? root.tooltip
    : root.label      != "" ? root.label
    : "Button";
accessible-checkable: root.checkable;
accessible-checked:   root.checked;
accessible-enabled:   !root.disabled && !root.loading;
accessible-action-default => {
    if (root.checkable) { root.checked = !root.checked; }
    root.clicked();
}
```

### Cascade rationale

1. **`aria-label`** — explicit caller intent. Always wins.
2. **`tooltip`** — already user-facing copy in the caller's locale. If a developer wrote a tooltip, it's a high-quality screen-reader name. The tooltip doc-comment notes that the string may be spoken.
3. **`label`** — the visible button text. The natural fallback when neither `aria-label` nor `tooltip` is set.
4. **`"Button"`** — pathological case. The component has nothing visible anyway; the AT tree gets a labeled node instead of a nameless one.

### Debug surfacing

When `debug-bounds: true` AND the cascade is genuinely falling through to `"Button"` (all three of `aria-label`, `tooltip`, `label` are empty), a 6×6px magenta dot renders at the top-right corner. Invisible in production (consumers don't enable debug-bounds), conspicuous during component authoring. The stricter condition — not just `aria-label == ""` — avoids false positives: a button with a working `label` already feeds the AT tree a sensible name through the cascade.

---

## Sizing

### Preset path

```
size  → height  font-size  icon-size  padding        source
xs    → 32px    text-xs    icon-xs    Spacing.md     Sizes.button-xs
sm    → 38px    text-sm    icon-sm    Spacing.lg     Sizes.button-sm
md    → 44px    text-base  icon-sm    Spacing.xl     Sizes.button-md  (HIG minimum)
lg    → 52px    text-base  icon-md    Spacing.xl     Sizes.button-lg
xl    → 60px    text-lg    icon-md    Spacing.xxl    Sizes.button-xl
xxl   → 72px    text-xl    icon-lg    Spacing.xxl    Sizes.button-xxl
hero  → 88px    text-xxl   icon-xl    Spacing.xxxl   Sizes.button-hero
icon  → 40px    text-sm    icon-sm    0              Sizes.icon-button-square
```

`md = 44px` is the Apple HIG minimum tap target. `hero = 88px` is the PAY-button class — full-width terminal buttons on tablet POS hardware. `icon` produces a 40×40 square button (its own `Sizes.icon-button-square` token, distinct from `Sizes.button-md`); intentionally pre-dates the `IconButton` primitive and is now mostly redundant — `IconButton` is the right choice for new icon-only call sites.

### Override path

When `height-override > 0`, the preset table is bypassed. The override becomes the height, and:
- `font-size = max(10px, height * 0.36)`
- `icon-size = max(12px, height * 0.42)`
- `padding   = height * 0.50` (or 0 for icon variant)

Coefficients tuned to match the preset ratios at h=40px (close to `md`). The override makes Button usable at arbitrary heights when a screen genuinely needs a non-preset size.

### Resolution helpers

`resolved-height`, `resolved-font-size`, `resolved-icon-size`, `resolved-padding`, `resolved-radius` all collapse the preset/override branches into single values the visual layer consumes. `resolved-radius` depends on `resolved-shape`: `pill` → `height/2`, `rounded` → `Radius.md`, `square` → `0px`.

### Width

- `preferred-width` is `content.preferred-width + 2 * resolved-padding`, clamped to at least `min-content-width`. For `size: icon`, it's locked to `resolved-height` (square).
- `horizontal-stretch: 1.0` when `full-width: true`, otherwise `0.0`.

---

## State semantics

### `visually-pressed`

The single source of truth for "is the button rendering its pressed look right now":

```
visually-pressed = (checkable && checked) || touch.pressed
```

Drives the opacity dim (`0.85` when pressed) and the face-slide animation. Note: this is *not* gated by `loading` on Button — only IconButton needed that gate. On Button, the spinner replaces the content entirely, and the `TouchArea` is disabled during loading so `touch.pressed` can't fire anyway. `checked + loading` simultaneously is a caller error.

### Hover semantics

`touch.has-hover` is read directly. It feeds into `active-bg-resolved`, the `Depth.bumped` elevation step, and (for link variant) the underline render.

### Disabled

`disabled` is the master "non-interactive" flag. It:
- Sets opacity to `0.5`.
- Disables both `TouchArea` and `FocusScope` (no click, no keyboard activation, no focus).
- Disables the drop-shadow (`Depth.applies` returns false).
- Wins over `loading` in the visual hierarchy (a disabled-loading button renders disabled).

### Loading

`loading`:
- Replaces the content row's icon-leading / label / icon-trailing with a single rotating spinner Text.
- Disables the `TouchArea` (no click).
- Preserves the button's width (the spinner has the same intrinsic size as the previously-rendered content row, more or less — minor width changes can occur at extreme size variants but are within layout tolerance).
- Rotation period reads from `Animation.spinner-period` (1200ms default). One full revolution per period.

### Tooltip timing

Tooltip renders when `tooltip != "" && touch.has-hover && !disabled`. Slint's `has-hover` already incorporates the platform's hover-delay heuristic; the library doesn't add its own delay timer. Position is always above the button (negative y) — POS layouts often have dense content directly below interactive controls, and a tooltip below tends to collide with siblings or get z-ordered behind them.

---

## Trapdoors (Slint quirks that bit Button specifically)

These are the Button-specific cases of the library-wide quirks documented in `HANDOVER.md → Slint quirks learned`:

1. **Drop-shadow on the root.** Forced the surface Rectangle split. See HANDOVER quirk #1.
2. **`min-width` naming collision.** Forced renaming to `min-content-width`. See HANDOVER quirk #2.
3. **Loader rotation origin.** Default `transform-origin` is `(0, 0)` so the spinner orbited its corner before we pinned it to the center. See HANDOVER quirk #3.
4. **`parent.width` in root bindings rejected.** Forced `horizontal-stretch: 1.0` for `full-width`. See HANDOVER quirk #4.
5. **Color literal comparison.** `bg-color != transparent` doesn't compile; `bg-color.alpha > 0` is the workaround. See HANDOVER quirk #5.
6. **`Math.sin / cos` on degrees.** Multiply integer by `1deg` to convert: `direction * 1deg`. Used in the angle projection for `shadow-direction`. See HANDOVER quirk #10.

---

## Open questions (deferred)

These are real questions about Button's API surface that we chose not to settle during Phase 1 because the evidence isn't in yet.

1. **Should `bg-color` and `height-override` move to a private extension?** They're escape hatches that real POS sometimes needs, but they're also the least curated parts of the API. IconButton intentionally dropped `bg-color` as a first step. Decision: keep them on Button for v1.0; revisit at v2.0 if real-world consumers stop using them.
2. **Should `size: ButtonSize.icon` be deprecated?** With `IconButton` now shipping, there's no good reason to use `Button { size: icon; label: "" }`. Decision: keep for v1.0 (backward compat with any existing pre-IconButton code); deprecate at v1.x with a doc-comment, remove at v2.0.
3. **Should `tone` apply to `link` variant in the same way as outline/ghost?** Currently it tints the text (good) and the hover underline (correct). No real issue; flagging for awareness.
4. **Variant × tone reachability matrix.** Every combination compiles, but not every combination is visually defensible. A playground tour should confirm `variant: link; tone: muted` and similar fringes look acceptable, or document which combinations are "supported but not recommended."
