# IconButton — Design

> Per-component design doc. Sibling docs live under `abdu-slint-ui/architecture/`.
> Role: the *what and why* for IconButton. Implementation steps live in `IMPL.md`.

---

## Purpose

A square, icon-only interactive primitive. Convenience over `Button { size: icon; label: ""; icon-leading: "..." }` for the dominant icon-only case, with:

- A required `aria-label` (no visible text means accessibility cannot be optional).
- A separate size enum without `hero` / `icon` variants (compiler-checked).
- A shape default that resolves against `Theme.icon-button-shape` (`circle` by default — different from Button's `Theme.button-shape`).
- `variant: ghost` as the default (icon buttons are most commonly low-emphasis in-row controls).

IconButton shares the depth, focus, and press machinery with Button but does **not** inherit from Button. They are sibling primitives that both consume the `Depth` global.

---

## Scope

**In scope (v1):**
- Single icon glyph, centered, sized to the button.
- Six variants (`default / destructive / outline / secondary / ghost / link`) — same set as Button. `link` is unusual for icon-only but supported for parity.
- Six sizes (`xs / sm / md / lg / xl / xxl`) → 32 / 38 / 44 / 52 / 60 / 72 px square.
- All shape resolutions: `default → Theme.icon-button-shape`, plus explicit `rounded / pill / circle / square`. On a square button, `pill` and `circle` resolve to the same `border-radius = height/2`.
- Tone overrides (same enum as Button).
- Depth properties: `elevated`, `shadow-elevation`, `shadow-color`, `shadow-direction`, `thickness`, `press-animation`.
- States: `disabled`, `loading` (spinner replaces icon — opacity-pulse fallback, Slint 1.14 cannot rotate `Text`).
- Toggle behaviour: `checkable` + `checked`.
- `tooltip` for hover discoverability — critical when there is no label.
- `height-override` escape hatch (produces a square of that side; icon glyph scales).
- `debug-bounds` outline on the `surface` Rectangle.

**Explicitly out of scope:**
- `label`, `icon-leading`, `icon-trailing` — consolidated into a single `icon`.
- `full-width` — square buttons have no notion of stretching.
- `min-content-width` — `size` and `height-override` are the only width controls.
- `bg-color` direct background override — the curated palette wins for icon buttons. Consumers needing a custom background use `tone` or pick a different `variant`. This is a deliberate divergence from Button.
- Multi-icon layouts (leading + trailing) — that is the Button job.

---

## Public API

### Properties (19 total)

**Identity & accessibility**

| Property      | Type     | Default | Notes                                              |
|---------------|----------|---------|----------------------------------------------------|
| `icon`        | `string` | `""`    | Icon name (resolved via `IconFont`) or raw glyph. |
| `aria-label`  | `string` | `""`    | Required when used in real apps; no visible label exists. |

**Visual**

| Property  | Type             | Default | Notes                                                                   |
|-----------|------------------|---------|-------------------------------------------------------------------------|
| `variant` | `ButtonVariant`  | `ghost` | Different default from Button.                                          |
| `size`    | `IconButtonSize` | `md`    | New enum: `xs / sm / md / lg / xl / xxl`. Compiler rejects `hero`/`icon`. |
| `shape`   | `Shape`          | `default` | `default` → `Theme.icon-button-shape`.                                |
| `tone`    | `Tone`           | `default` | Same semantics as Button.                                             |

**State**

| Property     | Type   | Default | Notes                                              |
|--------------|--------|---------|----------------------------------------------------|
| `disabled`   | `bool` | `false` |                                                    |
| `loading`    | `bool` | `false` | Spinner glyph replaces icon; blocks `clicked`.    |
| `tooltip`    | `string` | `""`  | Primary discoverability for icon-only buttons.    |
| `checkable`  | `bool` | `false` |                                                    |
| `checked`    | `bool` (in-out) | `false` | Favorite / pin / mute / star use cases.    |

**Depth (delegated to `Depth` global)**

| Property            | Type        | Default       | Notes                                          |
|---------------------|-------------|---------------|------------------------------------------------|
| `elevated`          | `bool`      | `true`        | Master shadow gate.                            |
| `shadow-elevation`  | `Elevation` | `sm`          | Hover bumps one step (`sm → md`, etc.).        |
| `shadow-color`      | `color`     | `transparent` | Transparent = Theme token for the level.       |
| `shadow-direction`  | `int`       | `0`           | Degrees; 0 = light from above.                 |
| `thickness`         | `length`    | `0px`         | Extruded depth; same surface/face structure as Button. |
| `press-animation`   | `bool`      | `true`        | Face slides down into base on press.           |

**Escape hatches & debug**

| Property          | Type     | Default | Notes                                                                  |
|-------------------|----------|---------|------------------------------------------------------------------------|
| `height-override` | `length` | `0px`   | `0` = use `size` preset. Positive forces a square of that side; icon glyph scales proportionally. |
| `debug-bounds`    | `bool`   | `false` | Magenta outline on `surface` Rectangle.                                |

### Callbacks

| Callback                 | Fires when                                       |
|--------------------------|--------------------------------------------------|
| `clicked()`              | Tap, click, or Enter/Space while focused.        |
| `pressed-changed(bool)`  | Physical press state transitions.                |
| `hover-changed(bool)`    | Mouse enters / leaves.                           |
| `focus-changed(bool)`    | Keyboard focus gained / lost.                    |

### What is **not** here

`label`, `icon-leading`, `icon-trailing`, `full-width`, `min-content-width`, `bg-color`. See *Scope → out of scope* for rationale.

---

## New enum

```
export enum IconButtonSize {
    xs,    // 32px
    sm,    // 38px
    md,    // 44px  (Apple HIG min tap target — the iOS-pivot default)
    lg,    // 52px
    xl,    // 60px
    xxl,   // 72px  (tablet hero icon actions)
}
```

Lives in `enums.slint` alongside the existing enums.

The size map reads from `Sizes.button-xs / button-sm / ...`. There is no `Sizes.icon-button-xs` etc. — IconButton uses the same physical heights as Button, just constrained to square.

---

## Depth global

Pure, stateless math provider. The component declares the six depth input properties (the public contract); the global owns the resolution.

```
// abdu-slint-ui/globals/depth.slint

import { Elevation } from "../enums.slint";
import { Theme } from "theme.slint";

export global Depth {
    // Hover bumps the effective elevation one step. xl saturates. none stays none.
    pure function bumped(level: Elevation, hovered: bool) -> Elevation;

    // True when all gates pass: elevated, not disabled, level != none.
    pure function applies(elevated: bool, disabled: bool, level: Elevation) -> bool;

    // Theme.shadow-{level}-blur, with none → 0px.
    pure function blur(level: Elevation) -> length;

    // Theme.shadow-{level}-y, used as the radial magnitude.
    pure function magnitude(level: Elevation) -> length;

    // override-color wins when alpha > 0; otherwise the Theme color for the level.
    pure function color-of(level: Elevation, override-color: color) -> color;

    // Projects radial magnitude onto x-axis using shadow-direction (degrees).
    pure function offset-x(direction-deg: int, magnitude: length) -> length;

    // Projects radial magnitude onto y-axis. Screen-y grows downward, so the
    // sign convention matches Button's current math (direction=0 → shadow down).
    pure function offset-y(direction-deg: int, magnitude: length) -> length;
}
```

**Why a global and not inheritance:** stateless. No component "is-a" depth-haver. Each component owns its own 6 properties (the API surface) and its own visual layer; the shared work is the resolution math, which is the only thing worth deduplicating.

**Caller pattern** in a component's visual layer:

```
property <Elevation> eff-level: Depth.bumped(root.shadow-elevation, touch.has-hover);
property <bool>     apply-shadow: Depth.applies(root.elevated, root.disabled, root.shadow-elevation);

surface := Rectangle {
    drop-shadow-blur:     root.apply-shadow ? Depth.blur(root.eff-level) : 0px;
    drop-shadow-offset-x: root.apply-shadow ? Depth.offset-x(root.shadow-direction, Depth.magnitude(root.eff-level)) : 0px;
    drop-shadow-offset-y: root.apply-shadow ? Depth.offset-y(root.shadow-direction, Depth.magnitude(root.eff-level)) : 0px;
    drop-shadow-color:    root.apply-shadow ? Depth.color-of(root.eff-level, root.shadow-color) : transparent;
    ...
}
```

Replaces ~30 lines of derived state in Button with seven function calls. Visual layer becomes declarative.

---

## Internal visual structure

Mirrors Button. The two-layer surface/face extrusion is what makes `thickness` read as physical depth.

```
IconButton (root)
├── root: transparent background, sizing + events only.
│   Slint compiler drops drop-shadow-* on the root — the shadow MUST live on
│   the inner surface Rectangle (HANDOVER §Slint quirks #1).
│
├── focus-ring Rectangle (outside surface, drawn when focus-scope.has-focus)
│
├── surface Rectangle  ← the "base/skirt"
│   ├── full width × full height
│   ├── background = active-bg.darker(0.4)  (the visible side wall when thickness > 0)
│   ├── border-radius = resolved-radius
│   ├── border-width = outline variant ? Sizes.border-thin : (debug-bounds ? 2px : 0px)
│   ├── drop-shadow-* via Depth.*
│   │
│   └── face Rectangle  ← the "top face"
│       ├── y = (visually-pressed && press-animation) ? thickness * 0.7 : 0
│       ├── height = parent.height - thickness
│       ├── background = filled-prominent ? @linear-gradient(...) : active-bg
│       ├── animated y and height
│       │
│       ├── highlight Rectangle  (top half, filled-prominent only)
│       │   white-to-transparent gradient
│       │
│       └── icon Text
│           ├── text = IconFont.resolve(root.icon)  (or "loader" when loading)
│           ├── font-family = IconFont.font-family-name()
│           ├── font-size = resolved-icon-size
│           ├── color = foreground-color
│           ├── opacity pulse via Timer when loading
│           └── horizontal+vertical alignment center
│
├── link underline Rectangle  (variant == link && hovered)
├── TouchArea
├── FocusScope
└── tooltip Rectangle  (tooltip != "" && hovered && !disabled)
```

### Sizing rules

- `size` → square side length from `Sizes.button-{xs..xxl}`.
- `height-override` (when > 0) → forces square of that side; the icon glyph scales as `max(14px, side * 0.5)`.
- Width = height (always). `preferred-width: resolved-side`, `height: resolved-side`.
- `horizontal-stretch: 0.0` always (no `full-width`).

**Why 0.5 (and not 0.42 as in Button).** Button's coefficient is calibrated for the common case where an icon accompanies a text label and plays a supporting visual role. An icon-only button at the same ratio reads as undersized — the glyph looks lost inside the surface. Material's IconButton uses ~0.6, Apple's SF Symbols in circle buttons land around 0.5–0.55. We pick `0.5` as the conservative center: visibly larger than Button's icon, but not so dominant that the button shape disappears behind the glyph. The minimum floor moves from `12px` to `14px` for the same reason (a 12px glyph in a 24px button is invisible at tablet viewing distance).

The same `0.5` coefficient applies to the preset path too — preset icon sizes for IconButton are computed as `preset-side * 0.5` rather than reusing `Sizes.icon-{size}` (which is calibrated for inline-with-text use).

### Shape resolution

```
property <string> resolved-shape:
      root.shape == Shape.default ? Theme.icon-button-shape
    : root.shape == Shape.rounded ? "rounded"
    : root.shape == Shape.pill    ? "pill"
    : root.shape == Shape.circle  ? "circle"
    : root.shape == Shape.square  ? "square"
    : Theme.icon-button-shape;

property <length> resolved-radius:
      root.resolved-shape == "square"  ? 0px
    : root.resolved-shape == "rounded" ? Radius.md
    : root.resolved-side / 2;          // "pill" and "circle" both → height/2
```

### Variant / tone / colors

Identical resolution logic to Button (`variant-base-bg`, `variant-hover-bg`, `variant-foreground`, `tone-color`, `tone-foreground`, `base-bg`, `hover-bg`, `foreground-color`, `resolved-border-color`). Will be duplicated for now — there is a future case for a `Variant` global parallel to `Depth`, but that's a Phase 1.5 / Phase 2 conversation once we see how it lands across three components.

The `variant-is-filled-prominent` gate (controls gradient + highlight) is the same: `default` or `destructive`.

### Press / hover / disabled

Same opacity-dim pattern as Button:
```
opacity:
      root.disabled         ? 0.5
    : root.visually-pressed ? 0.85
    : 1.0;
```

`visually-pressed = !loading && ((checkable && checked) || touch.pressed)`.

**Loading suppresses the press visual.** Button's `TouchArea.enabled` is already `false` during loading, so `touch.pressed` can never become true. But `checkable && checked` is independent of the touch area, so without the `!loading` gate a `checked` icon button entering `loading` would render its face slid down (because `visually-pressed` would still resolve to true). Gating at this derivation keeps the press/hover semantics clean and isolated to one place — the Depth global stays purely about shadow math and never learns about loading.

### Loading

Centered icon glyph swaps to `IconFont.resolve("loader")`, opacity pulses via a 80ms `Timer` (same trick as Button). Width stays the same; click is blocked while loading.

### Tooltip

Same render as Button — Rectangle above the button, `Theme.tooltip-*` colors, soft drop shadow. Always above (never below) because POS layouts often have dense content directly below interactive controls.

### Accessibility

Slint 1.14 has no runtime warning channel, but it does feed `accessible-role` and `accessible-label` into the platform accessibility tree (AT-SPI / UI Automation / NSAccessibility). A missing `aria-label` becomes a nameless interactive node — worse than a degraded name. IconButton therefore resolves the accessible name through a cascade:

```
accessible-role: button;
accessible-label:
      root.aria-label != "" ? root.aria-label
    : root.tooltip    != "" ? root.tooltip
    : root.icon       != "" ? root.icon
    : "Button";
accessible-checkable: root.checkable;
accessible-checked:   root.checked;
accessible-enabled:   !root.disabled && !root.loading;
```

**Cascade rationale:**

1. `aria-label` — explicit caller intent. Always wins.
2. `tooltip` — already user-facing copy in the caller's locale. If a developer wrote a tooltip, it's almost certainly a better screen-reader name than nothing. The `tooltip` doc-comment explicitly notes that the string may be used as the accessibility name when `aria-label` is empty, so consumers don't write decorative-only tooltips by mistake.
3. `icon` — last-ditch. A screen reader announcing "trash" is degraded but not silent. Icon names are kebab-case identifiers; pronunciation is imperfect but better than a nameless node.
4. `"Button"` — pathological case. Component has nothing visible anyway; AT tree gets a labeled node.

**Debug surfacing.** When `debug-bounds: true` and `aria-label == ""`, render a 6×6px magenta dot at the top-right corner of the surface Rectangle. Invisible in production builds (consumers don't ship with debug-bounds enabled); conspicuous in the playground so developers catch missing aria-labels during component authoring.

**Button gets the same cascade.** Button is already shipping in the codebase without `accessible-*` wiring. This is a separate small fix landing as its own commit in the build order below.

**Out of scope for v1:** CI lint for missing `aria-label` on IconButton call sites. A Rust build script could scan `.slint` files; logged as a Phase 3 (API-freeze) task.

---

## Globals consumed

`Theme`, `Sizes`, `Radius`, `Animation`, `IconFont`, `Depth`, plus `Typography` for the tooltip text.

Not consumed: `Spacing` (tooltip padding is hardcoded in Button; preserved for parity), `Locale` (icon glyphs don't mirror in v1), `CurrencyFormat` (not a numeric primitive).

---

## Acceptance criteria (visual validation gate)

The component is done when **every** cell of the matrix below renders correctly in `previews/icon-button.slint` and in the playground section:

- **Variants (6):** default, destructive, outline, secondary, ghost, link.
- **Sizes (6):** xs, sm, md, lg, xl, xxl — all visibly square, side length matches `Sizes.button-{size}`.
- **Shapes (4):** default (→ circle), rounded, pill, square. (`circle` and `pill` look identical; documented.)
- **Tones (7):** default, primary, success, warning, destructive, info, muted — each applies to filled and outline variants.
- **States:** rest, hover, pressed, focus (keyboard tab), disabled, loading, checked (when checkable).
- **Depth:** at least one preview row varies `shadow-elevation` from `none` through `xl`, and one varies `thickness` from `0` through `8px`.
- **Press animation:** with `thickness > 0`, pressing slides the face down by 70% of thickness and back on release.
- **RTL:** flip `Locale.rtl` in the preview — IconButton itself should look identical (square, single glyph), but the tooltip text should render in the locale's natural direction. No mirroring needed for the icon glyph in v1.
- **`aria-label` warning:** if `aria-label` is empty in a real consumer app, ideally surface a runtime hint. Slint has no `console.warn`; for now the discipline is enforced by the playground (the aria-label field is highlighted when empty) and by CLAUDE.md review.

---

## Open questions deferred to Phase 1.5 / Phase 2

1. **Variant resolution as a global.** Three components (Button, IconButton, plus eventually Card-when-interactive and Chip in Phase 2) duplicate the variant→color resolution. After Card lands, revisit whether a `Variant` global with `bg(variant, tone, state) -> color` parallels `Depth` cleanly.
2. **Loading spinner.** The opacity-pulse fallback is a workaround for Slint 1.14's missing `Text` rotation. Phase 2 should investigate whether a Path-based SVG rotation is viable as a shared `Spinner` micro-component.
3. **Mirroring directional icons in RTL.** `chevron-left` displayed in an Arabic locale should arguably auto-mirror to a `chevron-right` glyph. Out of scope for v1; consumers pass the correct icon for the direction.
4. **`debug-bounds` consolidation.** Both Button and IconButton expose `debug-bounds`. Eventually this might become a single global flag (`Debug.bounds-enabled`) toggled from the playground. Phase 2.
5. **`glyph-size-ratio` escape hatch.** v1 hardcodes `0.5`. If POS screens consistently demand a non-default ratio (e.g. specific brand glyphs that render too small at 0.5), we add `in property <float> glyph-size-ratio: 0.5;` as a property in a minor version. Until there's evidence of demand, the preset-with-curated-coefficient path is the API.

---

## Build order

Three commits, separated so each lands reviewable on its own. Order matters: depth foundation first, accessibility wiring second (low-risk and isolated), feature last.

### Commit 1 — `refactor(abdu-slint-ui): extract shadow math into Depth global`

1. Create `globals/depth.slint` with the seven pure functions.
2. Re-export from `lib.slint`.
3. Rewrite Button's `effective-elevation`, `shadow-applies`, `active-shadow-{blur,magnitude,x,y,color}` derived state to call `Depth.*`.
4. `cargo check` clean. Playground renders with **no visible change** to Button. User confirms parity.

### Commit 2 — `feat(abdu-slint-ui): accessible-* wiring on Button with aria fallback cascade`

1. Add `aria-label` cascade to Button (`aria-label → tooltip → label → "Button"`). Button's cascade has a `label` step instead of `icon` since Button has both a label and icons.
2. Set `accessible-role`, `accessible-label`, `accessible-checkable`, `accessible-checked`, `accessible-enabled`.
3. Add the magenta debug-corner-dot when `debug-bounds && aria-label == ""`.
4. `cargo check` clean. Playground renders identically (accessibility properties are non-visual). User confirms.

### Commit 3 — `feat(abdu-slint-ui): IconButton component + preview + playground section`

1. Add `IconButtonSize` to `enums.slint`.
2. Write `components/icon-button.slint` (uses `Depth.*` from day one, ships with the accessibility cascade).
3. Export from `lib.slint`.
4. Write `previews/icon-button.slint` covering the variant × size × shape × state × depth matrix.
5. Write `abdu-slint-ui-playground/src/state/icon_button.rs` and register the module.
6. Write `abdu-slint-ui-playground/ui/sections/icon-button.slint` with every public property exposed.
7. Wire the section into the sidebar in `ui/playground.slint`.
8. `cargo check` (library) + `cargo build` (playground) clean.
9. User runs the playground, exercises the matrix, confirms visual quality.

---

## Risks

- **Depth global breaks Button.** Mitigation: refactor as its own commit; user visually validates before IconButton lands.
- **Variant resolution still duplicated.** Accepted — we want three real call sites before extracting a `Variant` global.
- **`circle` vs `pill` on a square button.** Both produce identical `border-radius`. Mitigation: document in the property doc-comment; the playground shows both options and consumers see they look identical.
- **`height-override` may not pay rent on IconButton.** Square + curated sizes already cover the common cases. Mitigation: it's a low-cost property declaration; if unused after 6 months, drop it in a major version.
