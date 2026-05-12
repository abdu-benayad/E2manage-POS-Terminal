# abdu-slint-ui — Session Handover

> Snapshot mid-Phase 1. Read this first when starting a fresh session.

---

## TL;DR

- **Project:** `abdu-slint-ui` — a Slint UI component library + companion playground app, built to replace the inline Slint patterns currently bloating `e2manage-pos-terminal` and provide a reusable, well-styled foundation.
- **Phase 0 status:** ✅ complete (design docs).
- **Phase 1 status:** 🔄 in progress. **Foundation + Button + IconButton + Toggle + Card fully built**. Depth global extracted, accessibility cascade pattern established (with `accessible-role: switch` on Toggle, conditional-inner-shim role pattern on Card), rotating loading spinner with global period token, single-script segmentation principle codified. **1 component + smoke test remain (KeyValueRow → settings-display.slint)**.
- **Design language pivoted:** shadcn-inspired → **iOS / SwiftUI**. Larger sizes for POS tablets, glossy gradients, soft drop shadows, system color palette.

---

## Where things live

| Path                                                          | What                              |
| ------------------------------------------------------------- | --------------------------------- |
| `e2manage-pos-terminal/abdu-slint-ui/`                        | **This library** (compiling, Button + IconButton + Toggle + Card shipped) |
| `e2manage-pos-terminal/abdu-slint-ui/architecture/`           | Per-component design docs. Entries: `button.md` (retroactive), `icon-button.md` (forward), `toggle.md` (forward), `card.md` (forward). |
| `e2manage-pos-terminal/abdu-slint-ui-playground/`             | **Playground app** (compiling, Button + IconButton + Toggle + Card sections live) |
| `e2manage-pos-terminal/ui/spike/shadcn_button.slint`          | Original Phase 0 spike (kept for reference) |
| `e2manage-pos-terminal/ui/`                                   | Existing POS UI (untouched, Phase 4 target) |

---

## Document set

1. **`HANDOVER.md`** (this file) — current state
2. **`README.md`** — Phase 0 design contract. **Partially stale** — written with shadcn assumptions before the iOS pivot. The high-level philosophy still applies; the specific styling assertions (button heights, palette, "narrow APIs") have evolved. Revisit before Phase 2.
3. **`CLAUDE.md`** — construction discipline that overrides `~/.claude/CLAUDE.md` inside this directory.
4. **`ROADMAP.md`** — phase plan with decision gates.
5. **`IMPL.md`** — Phase 1 file-creation spec. The Button and IconButton sections are **superseded by the actual implementations** (see API tables below); the other 3 components still match the spec.
6. **`architecture/`** — per-component design docs. Policy: every non-trivial component gets one; trivial display-only primitives (e.g. KeyValueRow) don't. Forward design docs land *before* code (per CLAUDE.md's "Design doc → IMPL doc → code"). Current entries:
   - **`architecture/button.md`** — retroactive consolidation. The depth/lighting system, two-layer surface/face, variant × tone matrix, accessibility cascade, escape hatches — all of Button's load-bearing decisions captured in one place.
   - **`architecture/icon-button.md`** — forward design contract (was written before IconButton's code). Also doubles as the template for the Depth global and the accessibility cascade pattern.
   - **`architecture/toggle.md`** — forward design contract for Toggle. Settles the knob-carries-depth decision, the orthogonal x-slide/y-press-dip composition, the `accessible-role: switch` divergence, and the locale-independent knob geometry (label column flips in RTL; knob does not).
   - **`architecture/card.md`** — forward design contract for Card. Settles the no-variant / no-tone decision (structural primitive, not semantic), opt-in interactivity gating, the dual-sentinel padding-override (0px = preset, 0.001px = explicit zero), and `accessible-role: button` only when interactive. Two v1 simplifications documented inline in the implementation: single-layer surface (no two-layer face/skirt due to circular size dependency on content-driven Cards) and `thickness` / `press-animation` accepted as API parity but inert in v1.

---

## What's built

### Library (`abdu-slint-ui/`)

| Layer                | File(s)                                | Status |
| -------------------- | -------------------------------------- | ------ |
| Crate scaffold       | `Cargo.toml`, `build.rs`, `src/lib.rs` | ✅      |
| LICENSEs             | `LICENSE-MIT`, `LICENSE-APACHE`        | ✅      |
| Public entry         | `lib.slint`                            | ✅      |
| Enums                | `enums.slint` (11 enums)               | ✅ adds `IconButtonSize` |
| Globals              | 10 files under `globals/`              | ✅ iOS-tuned; `depth.slint` added, `Animation.pulse` renamed to `Animation.spinner-period` |
| Fonts                | `assets/phosphor.ttf`, `assets/lucide.ttf` | ✅ both bundled, switchable at runtime |
| Design docs          | `architecture/button.md`, `architecture/icon-button.md` | ✅ Button retroactive + IconButton forward |
| `Button` component   | `components/button.slint`              | ✅ extensively iterated, depth math now via `Depth` global, accessibility cascade wired |
| `Button` preview     | `previews/button.slint`                | ✅      |
| `IconButton` component | `components/icon-button.slint`       | ✅ shipped (19 properties, see API below) |
| `IconButton` preview | `previews/icon-button.slint`           | ✅      |
| `Toggle` component   | `components/toggle.slint`              | ✅ shipped (19 properties, see API below) — iOS pill switch, knob-only depth, `accessible-role: switch` |
| `Toggle` preview     | `previews/toggle.slint`                | ✅      |
| `Card` component     | `components/card.slint`                | ✅ shipped (16 properties, see API below) — opt-in interactive surface, single-layer in v1, conditional inner accessibility shim |
| `Card` preview       | `previews/card.slint`                  | ✅      |
| `KeyValueRow`        | —                                      | ❌ pending (next) |

### Playground (`abdu-slint-ui-playground/`)

| Layer                | File(s)                                | Status |
| -------------------- | -------------------------------------- | ------ |
| Crate scaffold       | `Cargo.toml`, `build.rs`, `src/main.rs` | ✅      |
| Window shell         | `ui/playground.slint`                  | ✅ sidebar locked at 240px, scrollable toolbar |
| Toolbar tokens       | button-shape, icon-btn-shape, card-shape, density, currency, icon-family, RTL, spinner-period | ✅ all wire to globals directly via `selected(v)` (sidebar uses `horizontal-stretch: 0`; toolbar wrapped in horizontal `ScrollView` for future-proofing) |
| Button section       | `ui/sections/button.slint`             | ✅ full property panel |
| IconButton section   | `ui/sections/icon-button.slint`        | ✅ full property panel, ~330 lines mirroring Button section's structure |
| Toggle section       | `ui/sections/toggle.slint`             | ✅ full property panel, ~290 lines, sidebar tile wired |
| Card section         | `ui/sections/card.slint`               | ✅ full property panel + density-demo content inside the demoed Card + dual-sentinel padding-override checkbox + sidebar tile wired |
| Other sections       | —                                      | ❌ pending per component (KeyValueRow) |
| Smoke test           | —                                      | ❌ pending |

---

## Design language: iOS / SwiftUI

The Phase 0 design contract was shadcn-inspired (flat, near-black primary, web aesthetic). After running the playground and reviewing visually, **we pivoted to iOS / SwiftUI**. Reasoning: this library targets POS terminals (touch-first, tablet-class hardware, used all day), not web SaaS. Shadcn sizing was too small (40px buttons → too tight for touch), shadcn palette too somber for POS.

What that means concretely:

- **Color palette:** iOS system colors. `Theme.primary = #007AFF` (systemBlue), `destructive = #FF3B30` (systemRed), `success = #34C759` (systemGreen), `warning = #FF9500`, `info = #5AC8FA`. Neutrals from iOS systemGray hierarchy.
- **Corner radii:** `Radius.md = 10px` (was 8px), `lg = 14px`, `xl = 18px`. Closer to iOS rounded-rectangle convention.
- **Sizes:** Button heights `xs=32 / sm=38 / md=44 / lg=52 / xl=60 / xxl=72 / hero=88`. `md = 44px` is Apple HIG's minimum tap target. `xxl` and `hero` are for tablet POS primary actions (PAY, etc.).
- **Default `button-shape`:** changed from `"pill"` to `"rounded"` (SwiftUI default is rounded rectangle, not capsule).
- **Shadow profile:** softer, more blur, tuned for tablet viewing distance.

The pivot is enacted in `globals/theme.slint`, `globals/sizes.slint`, `globals/radius.slint`, and the new size enum variants in `enums.slint`. Reverting would just be restoring those four files.

---

## Button API (as built)

The Button has grown substantially beyond IMPL.md's original 15 properties. This is the current authoritative reference for the API surface; **`architecture/button.md`** is the authoritative reference for *why* the surface is shaped this way (depth/lighting decisions, variant × tone composition rules, escape-hatch rationale, Slint trapdoors specific to Button).

### Core properties

| Property               | Type            | Default            | Notes |
| ---------------------- | --------------- | ------------------ | ----- |
| `label`                | `string`        | `""`               | |
| `icon-leading`         | `string`        | `""`               | resolved via `IconFont` |
| `icon-trailing`        | `string`        | `""`               | |
| `variant`              | `ButtonVariant` | `default`          | `default / destructive / outline / secondary / ghost / link` |
| `size`                 | `ButtonSize`    | `md`               | `xs / sm / md / lg / xl / xxl / hero / icon` |
| `shape`                | `Shape`         | `default`          | sentinel — follows `Theme.button-shape` |
| `tone`                 | `Tone`          | `default`          | overrides variant's color family — `success` makes a green variant of any variant |
| `disabled`             | `bool`          | `false`            | |
| `loading`              | `bool`          | `false`            | loader glyph rotates at `Animation.spinner-period`; blocks click |
| `full-width`           | `bool`          | `false`            | uses `horizontal-stretch` |
| `checkable`            | `bool`          | `false`            | toggle-button behavior |
| `checked`              | `bool` (in-out) | `false`            | |
| `tooltip`              | `string`        | `""`               | renders above button, themed colors |
| `min-content-width`    | `length`        | `0px`              | renamed from `min-width` to avoid Rectangle property collision |
| `aria-label`           | `string`        | `""`               | |

### Depth / lighting properties

| Property               | Type            | Default            | Notes |
| ---------------------- | --------------- | ------------------ | ----- |
| `elevated`             | `bool`          | `true`             | master shadow on/off |
| `shadow-elevation`     | `Elevation`     | `sm`               | `none / sm / md / lg / xl`; hover bumps one step |
| `shadow-color`         | `color`         | `transparent`      | tint override; transparent = use Theme |
| `shadow-direction`     | `int`           | `0`                | degrees, 0..359; light angle (shadow falls opposite) |
| `thickness`            | `length`        | `0px`              | physical extrusion — two-layer base+face; >0 makes button visibly 3D |
| `press-animation`      | `bool`          | `true`             | face slides down into base on press |
| `bg-color`             | `color`         | `transparent`      | direct background override; bypasses variant |
| `height-override`      | `length`        | `0px`              | overrides the size preset; font/icon/padding scale proportionally |
| `debug-bounds`         | `bool`          | `false`            | magenta outline on the `surface` Rectangle for layout debugging |

### Internal visual structure

The Button is **NOT** a single Rectangle. It is:

```
Button (root, transparent, sizing + event scope only)
├── focus-ring Rectangle (outside surface)
├── surface Rectangle (the "base/skirt" — darker, full height, drop-shadow)
│   └── face Rectangle (the "top face" — gradient, shorter by `thickness`)
│       ├── highlight Rectangle (top-half glossy overlay)
│       └── content HorizontalLayout (label, icons)
├── link underline (if variant=link)
├── TouchArea
├── FocusScope
└── tooltip Rectangle (if applicable)
```

The two-layer surface/face structure is what gives `thickness` its real 3D look. The face slides down on press while the base stays put, creating the "button depressing" feel.

### Callbacks

`clicked()`, `pressed-changed(bool)`, `hover-changed(bool)`, `focus-changed(bool)`.

---

## IconButton API (as built)

Square, icon-only sibling of Button. Not derived from Button — both compose the `Depth` global for shadow math; each owns its own visual structure, sizing, and shape resolution. Default variant is `ghost` (icon buttons are typically low-emphasis controls). See `architecture/icon-button.md` for the design contract.

### Core properties

| Property         | Type             | Default   | Notes |
| ---------------- | ---------------- | --------- | ----- |
| `icon`           | `string`         | `""`      | Library-canonical icon name or raw codepoint / emoji. |
| `aria-label`     | `string`         | `""`      | Falls through cascade `aria-label → tooltip → icon → "Button"` — see accessibility section below. |
| `variant`        | `ButtonVariant`  | `ghost`   | Different default from Button. Same enum, all six values supported. |
| `size`           | `IconButtonSize` | `md`      | `xs / sm / md / lg / xl / xxl` (32 / 38 / 44 / 52 / 60 / 72 px square). Distinct from `ButtonSize` so the compiler rejects `hero` / `icon`. |
| `shape`          | `Shape`          | `default` | Sentinel — follows `Theme.icon-button-shape` (default `"circle"`). On a square button `pill` and `circle` produce the same radius (height/2). |
| `tone`           | `Tone`           | `default` | Same semantics as Button. |
| `disabled`       | `bool`           | `false`   | |
| `loading`        | `bool`           | `false`   | Loader glyph replaces icon, rotates at `Animation.spinner-period`; blocks click. |
| `tooltip`        | `string`         | `""`      | Critical for icon-only buttons — primary discoverability. Also feeds accessibility cascade. |
| `checkable`      | `bool`           | `false`   | Favorite / pin / mute / star use cases. |
| `checked`        | `bool` (in-out)  | `false`   | |

### Depth / lighting properties

Same six properties as Button, delegated to the `Depth` global from day one.

| Property            | Type         | Default       | Notes |
| ------------------- | ------------ | ------------- | ----- |
| `elevated`          | `bool`       | `true`        | |
| `shadow-elevation`  | `Elevation`  | `sm`          | Hover bumps one step via `Depth.bumped()`. |
| `shadow-color`      | `color`      | `transparent` | Transparent = use Theme token. |
| `shadow-direction`  | `int`        | `0`           | Degrees. |
| `thickness`         | `length`     | `0px`         | Two-layer surface/face extrusion (same as Button). |
| `press-animation`   | `bool`       | `true`        | Face slides down on press. |

### Escape & debug

| Property            | Type      | Default | Notes |
| ------------------- | --------- | ------- | ----- |
| `height-override`   | `length`  | `0px`   | `0` = use `size` preset. Positive = square of that side; icon glyph scales as `max(14px, side * 0.5)`. |
| `debug-bounds`      | `bool`    | `false` | Magenta border on surface + magenta corner dot when the accessibility cascade is genuinely falling through to `"Button"`. |

### Not present on IconButton (intentional divergence from Button)

`label`, `icon-leading`, `icon-trailing` (consolidated to `icon`); `full-width` (always square); `min-content-width` (size dictates width); `bg-color` (curated palette wins — use `tone` or `variant` instead).

### Callbacks

`clicked()`, `pressed-changed(bool)`, `hover-changed(bool)`, `focus-changed(bool)`.

### Internal visual structure

Same five-layer pattern as Button — transparent root, focus-ring, surface Rectangle (with the drop-shadow), face Rectangle (with the gradient + highlight on filled-prominent variants), single centered icon Text, TouchArea, FocusScope, tooltip. The two-layer surface/face structure carries over because `thickness` works the same way.

---

## Toggle API (as built)

iOS-style binary switch. The first interactive primitive whose state semantics are inherent to its visual — `on` is captured by the knob's *position*, not by a depressed look. Not derived from Button or IconButton; consumes the `Depth` global for knob shadow math and adopts the accessibility cascade pattern with the first non-`button` role in the library (`accessible-role: switch`). See `architecture/toggle.md` for the design contract.

### Core properties

| Property      | Type            | Default | Notes |
| ------------- | --------------- | ------- | ----- |
| `label`       | `string`        | `""`    | Primary text beside the switch. RTL-aware column position. |
| `description` | `string`        | `""`    | Caption below `label`, in `Theme.muted-foreground`. **Not** part of the accessibility cascade (captions aren't names). |
| `tooltip`     | `string`        | `""`    | Hover discoverability + feeds the cascade as fallback. |
| `aria-label`  | `string`        | `""`    | Explicit a11y name. Always wins the cascade. |
| `on`          | `bool` (in-out) | `false` | Controlled. Toggle flips it itself on user activation and then fires `toggled(on)`. |
| `disabled`    | `bool`          | `false` | Opacity dim; blocks click and keyboard. |
| `loading`     | `bool`          | `false` | Knob glyph swaps to the rotating loader at `Animation.spinner-period`; blocks toggle. `on` is preserved through loading. |
| `size`        | `ToggleSize`    | `md`    | `sm / md / lg` — `md` is the iOS-canonical 51×31 reference. |
| `tone`        | `Tone`          | `default` | `default` → `Theme.success` (iOS green). `destructive` → red. Same enum as Button/IconButton. |
| `on-icon`     | `string`        | `""`    | Rendered inside the knob when `on` (iOS pattern). |
| `off-icon`    | `string`        | `""`    | Inside the knob when off. Cross-fades with `on-icon` over `Animation.normal` (same duration as the slide). |

### Depth / lighting properties (knob, not track)

Same six properties as Button/IconButton. The knob carries depth; the track is flat with a subtle inner-edge border.

| Property            | Type        | Default       | Notes |
| ------------------- | ----------- | ------------- | ----- |
| `elevated`          | `bool`      | `true`        | Master shadow gate on the knob. |
| `shadow-elevation`  | `Elevation` | `sm`          | Hover bumps one step (via `Depth.bumped()`). |
| `shadow-color`      | `color`     | `transparent` | Transparent = Theme token for the level. |
| `shadow-direction`  | `int`       | `0`           | Degrees [0, 359]. |
| `thickness`         | `length`    | `0px`         | Knob extrusion. Two-layer surface/face — the skirt peeks under the face by `thickness` px on press. |
| `press-animation`   | `bool`      | `true`        | Knob face slides down by 70% of `thickness` on press. **Composes orthogonally with the x-slide** between off and on positions. |

### Escape & debug

| Property          | Type     | Default | Notes |
| ----------------- | -------- | ------- | ----- |
| `height-override` | `length` | `0px`   | Forces track height; knob diameter and track width scale at the iOS aspect ratio (51/31 ≈ 1.645). |
| `debug-bounds`    | `bool`   | `false` | Magenta border on the track + corner aria badge when the cascade falls through to `"Toggle"`. |

### Callbacks

| Callback                | Notes |
| ----------------------- | ----- |
| `toggled(bool)`         | Fires *after* `on` has been flipped by user activation. Argument is the new value. |
| `pressed-changed(bool)` | Physical press state transitions. |
| `hover-changed(bool)`   | Mouse enters / leaves the track or label. |
| `focus-changed(bool)`   | Keyboard focus gained / lost. |

### Not present on Toggle (intentional API divergence)

`variant` (Toggle has one canonical iOS visual — variant proliferation would dilute the look); `shape` (track is pill-locked); `bg-color` / `track-color-on` / `track-color-off` (curated `tone` palette wins, matches IconButton's first move away from Button's escape hatches); `checkable` / `checked` (Toggle is by definition checkable — `on` IS the checked state). No `icon-leading` / `icon-trailing` (icons go inside the knob, not beside the track).

### Sizing

| Size | Track W×H | Knob ⌀ | Gap |
| ---- | --------- | ------ | --- |
| `sm` | 44 × 26   | 22px   | 2px |
| `md` | 51 × 31   | 27px   | 2px (iOS reference) |
| `lg` | 60 × 36   | 32px   | 2px |

The `gap` is a fixed 2px inset between knob edge and track edge in both x and y, locked across sizes and `height-override`. The aspect ratio (51/31) is preserved when `height-override` is set, giving a smooth scale-up path for tablet hero toggles without adding an `xl` preset.

### Internal visual structure

```
Toggle (root, transparent, sizing dictated by inner HorizontalLayout)
├── HorizontalLayout (alignment: start, spacing: Spacing.md when column present)
│   ├── [if Locale.rtl && has-column] VerticalLayout — label + description (right-aligned text)
│   ├── track-container Rectangle (fixed width × height from size)
│   │   └── track Rectangle (pill, animated background between off and on colors)
│   │       └── knob-surface Rectangle (skirt — animated x, carries drop-shadow via Depth.*)
│   │           └── knob-face Rectangle (top face — gradient white→#f5f5f5, animated y for press-dip)
│   │               ├── highlight Rectangle (top-half glossy sheen)
│   │               ├── off-icon-text   (opacity ↔ !on, cross-fades over Animation.normal)
│   │               ├── on-icon-text    (opacity ↔ on,  cross-fades over Animation.normal)
│   │               └── loader-text     (opacity ↔ loading, rotation via animation-tick())
│   └── [if !Locale.rtl && has-column] VerticalLayout — label + description
├── focus-ring Rectangle (encompasses entire control)
├── debug aria badge (debug-bounds + cascade-falls-through)
├── TouchArea          (covers the whole control — tap on track OR label flips on)
├── FocusScope         (Enter/Space activates)
└── tooltip Rectangle  (anchored to track-container, not full control width)
```

**Knob carries depth, not track.** A track shadow would weld the pill to its substrate; iOS knobs float over a recessed channel. The track gets a flat fill + subtle inner-edge border (`Theme.border` darker by 5–10%); the knob carries the drop-shadow and optional extrusion.

---

## Card API (as built)

Surface container. The fourth interactive primitive after Button, IconButton, and Toggle, and the first with **opt-in interactivity** — Card is a static surface unless `interactive: true`. No `variant`, no `tone` (Card is a structural primitive, not a semantic one — coloring a whole surface to imply meaning is inaccessible; semantic content belongs inside the card). See `architecture/card.md` for the design contract.

### Core properties

| Property      | Type      | Default   | Notes |
| ------------- | --------- | --------- | ----- |
| `aria-label`  | `string`  | `""`      | Required when `interactive: true`. Cascade: `aria-label → tooltip → "Card"`. No `label` step (Card has no visible text of its own — that's SectionCard in Phase 2). |
| `tooltip`     | `string`  | `""`      | **Only rendered when `interactive: true`** — a tooltip on a static surface promises interaction that doesn't exist. Also feeds the cascade as fallback. |
| `shape`       | `Shape`   | `default` | `default → Theme.card-shape`. `rounded → Radius.lg` (14px, larger than buttons). `square → 0px`. `pill` and `circle` **fall back to `rounded`** with a doc-comment note — a 300px-wide pill card is a giant lozenge. |
| `bordered`    | `bool`    | `true`    | 1px `Theme.border`. Safe default — guarantees visibility even when `elevated: false` and `shadow-elevation: none`. |
| `padding-density` | `Density` | `default` | `compact → Spacing.md (12px)`, `default → Spacing.lg (16px)`, `comfortable → Spacing.xl (24px)`. **Named `padding-density` (not `padding`) because Slint reserves `padding` on components inheriting Rectangle.** |
| `interactive` | `bool`    | `false`   | Opt-in interactivity. When true: hover/press feedback, focus ring, `clicked()`, accessibility cascade, tooltip, keyboard activation. When false: pure surface, no events, no AT role, **not in the keyboard tab order**. |
| `disabled`    | `bool`    | `false`   | Only effective when `interactive: true`. Opacity dim to 0.5; blocks click/keyboard. |

### Layout & escape hatches

| Property              | Type     | Default | Notes |
| --------------------- | -------- | ------- | ----- |
| `max-content-width`   | `length` | `0px`   | `0px` = no cap. Renamed from IMPL spec's `max-width` to avoid Rectangle's reserved name. |
| `padding-override`    | `length` | `0px`   | **Dual-sentinel.** `0px` = use `padding-density` preset (the common case). `0.001px` = **explicit zero padding** for full-bleed image cards / edge-to-edge list rows. Any other positive length forces that padding on all four sides. The three-way resolution is documented inline with a "do not collapse this" maintainer note. |

### Depth properties

Same six properties as Button/IconButton/Toggle. **Two of them are inert in v1**:

| Property            | Type        | Default       | Notes |
| ------------------- | ----------- | ------------- | ----- |
| `elevated`          | `bool`      | `true`        | Master shadow gate. |
| `shadow-elevation`  | `Elevation` | `sm`          | Hover bumps one step **only when `interactive`**. Non-interactive cards never bump on mouse-over. |
| `shadow-color`      | `color`     | `transparent` | Transparent = Theme token. |
| `shadow-direction`  | `int`       | `0`           | Degrees. |
| `thickness`         | `length`    | `0px`         | **API-parity only in v1 — renders no visible effect.** The two-layer surface/face pattern that Button uses creates a circular size dependency on content-driven Cards (surface sizes to face which sizes to surface). Deferred to v1.1. |
| `press-animation`   | `bool`      | `true`        | **API-parity only in v1** (paired with `thickness`'s inert status). |

### Debug

| Property        | Type   | Default | Notes |
| --------------- | ------ | ------- | ----- |
| `debug-bounds`  | `bool` | `false` | Magenta border + corner aria badge when `interactive && aria-label == "" && tooltip == ""`. **Aria badge gated on `interactive`** — a non-interactive card with no aria-label is correct behavior, not a missing-name bug. |

### Callbacks

All four fire **only when `interactive: true`**. Connecting to `clicked()` while `interactive: false` is a no-op.

`clicked()`, `pressed-changed(bool)`, `hover-changed(bool)`, `focus-changed(bool)`.

### Internal visual structure

```
Card (root Rectangle, transparent)
├── preferred-width/height bound to content-layout.preferred-width/height
│   (manual propagation — Slint doesn't auto-propagate preferred-size
│   through nested Rectangles, see quirk #15 below)
├── max-width: max-content-width when set, else 99999px
│
├── [if interactive] focus-ring Rectangle  (wraps surface bounds, not shadow blur radius)
├── surface Rectangle
│   ├── full size of root
│   ├── background: base-bg-resolved (animates between rest / hover / press tints)
│   ├── border-radius: resolved-radius (Radius.lg or 0)
│   ├── border: bordered ? Sizes.border-thin Theme.border : 0px
│   ├── clip: true                  ← non-negotiable; clips full-bleed children to corners
│   ├── drop-shadow-* via Depth.*   ← shadow stays put on press (effective-hover gated on !pressed)
│   └── content-layout := VerticalLayout
│       ├── padding: resolved-padding (dual-sentinel three-way resolution)
│       └── @children
├── [if debug-bounds && interactive && all-names-empty] magenta corner aria badge
├── TouchArea          (enabled = interactive && !disabled)
├── FocusScope         (enabled = interactive && !disabled — non-interactive cards NOT in tab order)
├── [if interactive] accessibility shim Rectangle
│   └── transparent overlay carrying accessible-role: button + cascade label + action-default
│   (workaround for Slint requiring accessible-role to be a compile-time constant)
└── [if interactive && tooltip != "" && hovered && !disabled] tooltip Rectangle
```

### Press feedback semantics (v1)

Three subtle effects, no y-shift or extrusion:

1. Background tints from `Theme.surface` → `surface.darker(4%)` on press.
2. Opacity dims from 1.0 → 0.96.
3. **Shadow returns to its rest level on press.** `effective-hover` is gated on `!touch.pressed`, so the hover-bump stops during press. This mimics actual physical depression — a card pressed into its substrate has *less* shadow, not more.

Initial implementation tried a press y-shift (surface translates down by 50% of thickness) but the shadow continuing to grow on hover-during-press created a >10px visual "jump downward" feel. Removing the y-shift and gating effective-hover on `!pressed` produces a clean tap feedback.

### Accessibility cascade with the conditional-shim pattern

Slint 1.14 requires `accessible-role` to be a compile-time constant — `accessible-role: root.interactive ? AccessibleRole.button : AccessibleRole.none` does not compile. Card's workaround: a conditional inner `if root.interactive: Rectangle { accessible-role: button; ... }` shim that exists only when interactive. The root keeps `accessible-role: none` (Rectangle default).

Behavior is identical to a ternary'd role:
- `interactive: false` → shim doesn't exist → root is `role: none` → invisible to AT tree
- `interactive: true` → shim exists with `role: button` → AT tree sees a labeled, action-able button

The shim carries the full cascade (`aria-label → tooltip → "Card"`), `accessible-enabled: !disabled`, and `accessible-action-default => clicked()`.

**Pattern propagates** to any future component whose accessibility role depends on a runtime property (e.g. a Chip that becomes a "tab" when used inside a tab group). Same conditional-inner-element trick.

---

## Depth global (`globals/depth.slint`)

**Two animations compose orthogonally.** Knob `x` slides via `Animation.normal + ease-out` (200ms) between off and on. Knob-face `y` dips via `Animation.fast + ease-out` (120ms) on press. Both can fire simultaneously without interference. Cross-fade between on-icon and off-icon shares `Animation.normal` with the slide so they read as a single motion; the loader uses `Animation.fast` because the load-state transition is a distinct semantic event.

**RTL geometry rule.** The knob's off/on x-positions do **NOT** flip in RTL — off is always physical-left of the track, on is always physical-right. Matches iOS Arabic behavior. The label/description column position *does* flip via two `if Locale.rtl` branches around the always-rendered track-container (mirrors Button's `icon-leading` / `icon-trailing` duplication pattern).

**State vs. interaction.** `on` does **not** feed `visually-pressed` (deliberate divergence from Button's `checkable && checked` path). The toggle's state lives in the knob position; only live tactile press triggers the knob face's dip.

### Accessibility cascade

`accessible-role: switch` — first non-`button` role in the library, exercising Slint's broader AccessibleRole enum. Cascade: `aria-label → tooltip → label → "Toggle"`. `accessible-checkable: true` (always — switches are checkable by definition). `accessible-checked: root.on`. `accessible-action-default` flips `on` and fires `toggled(on)`.

The magenta corner aria badge renders when `debug-bounds && aria-label == "" && tooltip == "" && label == ""` (description is excluded — captions are not names).

---

## Depth global (`globals/depth.slint`)

Stateless math provider for drop-shadow computation. Each consuming component owns the six depth input properties (the public contract); this global owns the resolution.

```
Depth.bumped(level, hovered)           → Elevation   // hover bumps sm→md→lg→xl, xl saturates
Depth.applies(elevated, disabled, level) → bool      // master gate
Depth.blur(level)                      → length      // Theme.shadow-{level}-blur
Depth.magnitude(level)                 → length      // Theme.shadow-{level}-y
Depth.color-of(level, override-color)  → color       // alpha>0 wins, else Theme token
Depth.offset-x(direction-deg, mag)     → length      // -sin(deg) * mag
Depth.offset-y(direction-deg, mag)     → length      //  cos(deg) * mag
```

All `pure public function`. They read `Theme.shadow-*` tokens but take all variable inputs as parameters — true stateless math. No inheritance, no slot-based composition; components stay flat and declarative.

Button, IconButton, Toggle, and Card all consume it. Toggle threads it through its knob (not its track); Card threads it through its surface with an `effective-hover` indirection that gates the hover-bump on `interactive: true` (non-interactive cards don't ripple shadow on mouse-over). The global stays purely about math — Card-specific gating happens at the call site.

---

## Accessibility cascade pattern

All four shipped components wire Slint's `accessible-*` properties to the platform AT tree (Card only when `interactive: true`). A nameless interactive node is worse than a degraded name, so each resolves `accessible-label` via a cascading fallback:

- **Button:** `aria-label → tooltip → label → "Button"` (role: `button`)
- **IconButton:** `aria-label → tooltip → icon → "Button"` (role: `button`)
- **Toggle:** `aria-label → tooltip → label → "Toggle"` (role: `switch` — Slint's distinct AccessibleRole.Switch)
- **Card:** `aria-label → tooltip → "Card"` (role: `button` when `interactive: true`, otherwise no AT role at all). Three-segment cascade (no `label` step — Card has no visible text; that's SectionCard's job in Phase 2).

Plus the role-appropriate state properties: `accessible-checkable` (true on Toggle by definition; tied to `root.checkable` on Button/IconButton), `accessible-checked` (`root.on` on Toggle, `root.checked` elsewhere), `accessible-enabled: !disabled && !loading`, and `accessible-action-default` so AT-driven activations fire `clicked()` / `toggled(on)`.

Authoring-time debug: when `debug-bounds: true` AND all three cascade sources are empty (the accessibility name is genuinely falling through to the default), a 6×6px magenta dot renders at the top-right corner. Invisible in production builds; conspicuous in the playground. Note that Toggle's `description` is **not** part of its cascade or debug condition (captions aren't names).

The cascade pattern propagates to every future interactive component. Any Phase-2 component with a clickable surface should adopt the same shape; non-button roles (e.g. `accessible-role: slider`) are now an established option after Toggle exercised the first non-`button` role. Conditional roles (where the role itself depends on a runtime property) use Card's conditional-inner-shim pattern.

---

## Slint quirks learned (the trapdoors)

These are non-obvious. Document them so we don't relearn:

1. **`drop-shadow-*` on a component's root element is silently dropped.** Slint's `lower_shadows` compiler pass refuses to transform shadows on the root and only emits a warning, not an error. **Workaround:** put visual + shadow on an inner Rectangle (not the root). Source: `vendor/i-slint-compiler/passes/lower_shadows.rs:110-117`.

2. **`min-width` and `border-color` collide with Rectangle's built-ins.** Naming a custom property `min-width` on a Rectangle-inheriting component fails to compile. Use distinct names (`min-content-width`, `resolved-border-color`).

3. ~~**`Text` has no `transform-rotation` in 1.14.**~~ **CORRECTION:** Text *does* support rotation via `transform-rotation` (aliased to `rotation-angle` on the `SimpleText` base). For continuous rotation, bind to `mod(animation-tick(), 1s) / 1s * 360deg` — `animation-tick()` ticks every frame, no Timer needed. **Gotcha:** the default `transform-origin` is `(0, 0)` (top-left corner), so without setting it the glyph orbits its corner instead of spinning in place. Always set `transform-origin: { x: self.width / 2, y: self.height / 2 }` for spin-in-place rotation. The original opacity-pulse fallback shipped on Button v1 was unnecessary and replaced by the rotating-glyph approach.

4. **`parent.width / parent.height` in root property bindings is rejected.** `full-width` via `width: parent.width` fails on a component root. **Workaround:** `horizontal-stretch: 1.0` in an enclosing layout.

5. **Color literals can't be compared with `!=`.** `bg-color != transparent` fails. **Workaround:** check `bg-color.alpha > 0`.

6. **`transparent` inside a struct literal fails.** A `{ slug: "none", swatch: transparent }` struct literal won't parse. Use `#00000000` instead.

7. **`.darker(0.25)` shifts hue subtly, not strongly.** Don't rely on `.darker()` for "make this look like a different color"; for strong contrast (e.g. a side wall on a saturated bg) overlay a semi-transparent black instead (`#00000066`).

8. **`std-widgets` ComboBox + intermediate state property = stale-value race.** Binding `current-value: my-prop` and writing `my-prop = v` in `selected(v)` results in `changed my-prop` not firing when the picked value equals the intermediate's current value. **Workaround:** bind `current-index` (computed from the global) and write directly to the global in `selected(v)`. No intermediate property.

9. **`Elevation` enum values default to `Elevation.none` in some contexts.** When iterating elevation enum in computed properties, all branches must terminate (no implicit "fall through to default"). Always have a final fallback.

10. **`Math.sin/cos` accept Slint's `angle` type natively.** Multiply an integer by `1deg` to convert: `direction * 1deg`. No manual degree-to-radian conversion needed.

11. **Slint's `import "font.ttf";` at module level registers the font automatically.** No Rust-side `register_font_*` needed. (And the public Slint crate doesn't even expose font registration functions in 1.14 — must go through the compile-time mechanism.)

12. **Standalone workspace with vendored sources.** The parent repo vendors all crates and uses `.cargo/config.toml` source replacement. Library crate inside the parent tree must declare its own `[workspace]` to opt out of the parent workspace's member list. Slint version must match the vendored version (`1.14`), not the latest crates.io.

13. **`padding` is a reserved property name on Rectangle-inheriting components.** Declaring `in property <Density> padding: ...` fails to compile with "Cannot override property 'padding'". Mirrors the existing `min-width` / `border-color` collisions from quirk #2. **Workaround:** rename (e.g. Card uses `padding-density`). Likely related to Slint's layout system reserving `padding` on layout-capable elements regardless of whether the component is actually a layout.

14. **`accessible-role` requires a compile-time constant expression.** Trying `accessible-role: root.interactive ? AccessibleRole.button : AccessibleRole.none` fails with "The `accessible-role` property must be a constant expression." **Workaround:** keep the root's `accessible-role` at the Rectangle default (`none`), and place a conditional inner Rectangle inside an `if`-block with the desired role as a literal: `if root.interactive: Rectangle { accessible-role: button; accessible-label: ...; accessible-action-default => ... }`. When the condition is false, the inner element doesn't exist → no entry in the AT tree. When true, it carries the role as a literal. Card uses this pattern; propagates to any component whose AT role depends on a runtime property.

15. **Preferred-size doesn't auto-propagate through nested Rectangles for content-driven components.** A `Rectangle { VerticalLayout { ... } }` does NOT propagate the VerticalLayout's preferred-size up to the outer Rectangle by default — the outer Rectangle defaults to "expand to fill parent." When the parent itself sizes-to-content, the result is a 0×0 root. **Workaround:** bind the root's preferred-width / preferred-height (and min-width / min-height) explicitly to a named inner layout: `preferred-width: content-layout.preferred-width;`. Card discovered this and bound to a `content-layout := VerticalLayout`. Affects any component that's content-driven and wraps content in a styled Rectangle.

---

## How to resume

```sh
cd /home/abdu/Downloads/e2manage-pos-terminal/abdu-slint-ui-playground
cargo run
```

In the playground:
- Sidebar shows **Button**, **IconButton**, **Toggle**, and **Card**. Click any to mount its section.
- Toolbar across the top: theme-shape combos, density, currency, icon-family swap, RTL toggle, **spinner-period slider** (retune `Animation.spinner-period` live). Wrapped in a horizontal `ScrollView` so adding more tokens doesn't compress the sidebar.
- Right panel: every public property surfaced as an interactive control (LineEdits, ComboBoxes, swatch grids, sliders).
- Bottom of preview pane: live code snippet of the current configuration.

Verify the build:

```sh
cd abdu-slint-ui/         && cargo check
cd abdu-slint-ui-playground/ && cargo build
```

Both should build clean. Expect four harmless library-level warnings — one each for Button, IconButton, Toggle, and Card not inheriting Window (`No code will be generated for it` / `This is deprecated`) — same root cause, the components are mounted by `lib.slint` re-exports and used by other Slint files, not instantiated from Rust. Can't easily silence in Slint 1.14.

---

## What's next (Phase 1 remainder)

In priority order:

1. **`KeyValueRow`** — display-only, RTL-aware, the canonical demonstration of the single-script segmentation principle (see README → "Internationalization: single-script segmentation"). `label: string` (one script) + `value: string` (one script), each in its own `Text` element; the row's HorizontalLayout flips per `Locale.rtl`. Optional `value-icon`, `emphasis` enum, `value-tone`, `density`, `show-divider`. No depth, no accessibility wiring needed. Per the architecture-doc policy, KeyValueRow likely warrants a *short* design doc anyway because it's the first explicit incarnation of the segmentation principle — the design doc names the principle in context.

2. **Smoke test:** `examples/settings-display.slint` — rewrite the existing 700-line `ui/screens/settings/display.slint` using only Phase 1 primitives. Confirms the API survives contact with a real screen. Will be the first composed example using Button + IconButton + Toggle + Card + KeyValueRow together.

3. **Phase 1 docs polish (post-smoke-test):** update README's "Project status" table, mark Phase 1 as ✅, write the Phase 1 retrospective summarizing what shipped vs the original IMPL spec, prepare the Phase 1 → Phase 2 decision-gate doc.

### Open API questions for Phase 2 entry

- Should `bg-color` and `height-override` on Button move to a private extension or stay as escape hatches? They make the API less curated but real-life POS sometimes needs them. **IconButton, Toggle, and Card all intentionally omit `bg-color`** — the pattern of tightening the curated palette over time is established. Decision still deferred for Button itself; revisit at v2.0.
- ~~Should the depth system be factored into a shared mixin?~~ **RESOLVED.** Extracted as the `Depth` global. Four shipped components consume it (Button, IconButton, Toggle, Card).
- Should variant/tone color resolution also become a global (parallel to `Depth`)? Button, IconButton, and Toggle duplicate the `tone-color` resolution. **Card did NOT add a new call site** — it ships without `variant` or `tone` (structural primitive, not semantic). The case stays at three call sites; next real test is Chip / OptionTile in Phase 2. Worth waiting for the fourth real consumer before extracting.
- **`Tone.muted` on Toggle.** Identified during Toggle design as a contrast risk: `Theme.muted-foreground` (#8e8e93) as a track-on color is close to the off color (#c6c6c8), so a `muted`-toned toggle reads as "off-ish" even when on. Currently shipping; revisit if real consumers find it confusing. May drop from Toggle's supported tone set in v1.1 if the Variant global doesn't fix it first.
- The `tone` enum override is implemented but not heavily exercised. Worth a playground tour to make sure every `variant × tone` combo looks reasonable before more components rely on the same pattern.
- The icon-only `loader` glyph quality depends on the icon font (Phosphor's looks like a tasteful spinner; Lucide's similar). A future Phase 2 task could lift Slint's own `SpinnerBase` Path math into a private `Spinner` micro-component so the loading visual is identical across icon-family swaps — explored once, rolled back as "doesn't look right yet"; revisit after design-direction stabilizes.
- **Toggle knob-drag interaction.** v1 is tap-only (tap track or label flips `on`). iOS supports drag-the-knob. Listed in `architecture/toggle.md` for v1.1 — needs careful `TouchArea` event-model work to distinguish a press-with-no-movement from a press-with-drag.
- **Card `thickness` extrusion.** v1 ships `thickness` and `press-animation` as API-parity properties only — they accept values but render no visible effect. Card's content-driven sizing creates a circular size dependency with Button's two-layer surface/face approach (surface sizes to face which sizes to surface's content). v1.1 candidate: introduce an alternative extrusion mechanism (e.g. an absolutely-positioned skirt sibling below the surface that doesn't participate in size negotiation) if hero cards demand real depth.
- **Patterns worth borrowing from SurrealismUI / Slint Material 3** (researched May 2026): `states [...]` blocks for variant resolution (both libraries use this — strong convergent signal), hand-tuned `-pressed` / `-disabled` palette shades on `Theme` (avoid runtime `.darker()` at call sites), `forward_focus: base` for future composite components (SectionCard). All slotted for Phase 1.5 between smoke-test and Phase 2 entry.

---

## Don't touch (Phases 1–3)

The POS itself stays untouched until Phase 4:

- `e2manage-pos-terminal/ui/` (except the existing `ui/spike/`)
- `e2manage-pos-terminal/src/`
- `e2manage-pos-terminal/crates/`
- `e2manage-pos-terminal/Cargo.toml` (workspace)

The library evolves in isolation. POS integration is its own phase with its own plan.

---

## Commit history

Read `git log --oneline` for the full chronology. The most recent commits — the Card vertical slice plus the segmentation-principle docs pass — are:

| Commit | Summary |
| -- | -- |
| `506bf3e feat(abdu-slint-ui): Card component + preview + playground section` | The full Card slice. Adds `components/card.slint` (~360 lines), preview (~280 lines, including density comparison, full-bleed clipping test, oversized-child safety, RTL), playground section (~390 lines with dual-sentinel padding-override checkbox + density-demo embedded content), sidebar tile + section routing. Single-layer surface in v1 (two-layer creates a circular size dependency on content-driven Cards). Press feedback: background tint + opacity dim + shadow returns to rest. Conditional-inner accessibility shim works around Slint's compile-time-constant `accessible-role` requirement. Three new Slint trapdoors discovered (quirks #13–15). |
| `a74bb63 docs(abdu-slint-ui): Card design contract` | Forward design doc at `architecture/card.md`. Settles 16 properties (no variant, no tone), opt-in interactivity, dual-sentinel padding-override, `pill` / `circle` shape fallback to `rounded`, `bordered: true` safe default. Card explicitly does NOT close the Variant-global case — that waits for Chip / OptionTile. |
| `39eb66d feat(abdu-slint-ui): Toggle component + preview + playground section` | The full Toggle slice. Adds `components/toggle.slint` (~370 lines), preview, playground section (~290 lines), sidebar tile + section routing. Knob carries depth via `Depth` global; track is flat with subtle inner border. Cross-fade between on-icon and off-icon shares the slide's `Animation.normal` timing. First library component with `accessible-role: switch`. |
| `e4d6f72 docs(abdu-slint-ui): Toggle design contract` | Forward design doc at `architecture/toggle.md`. Settles 19 properties (4 identity + 3 state + 2 visual + 2 knob-icon + 6 depth + 2 escape/debug) and four callbacks. Key load-bearing decisions captured: depth-on-knob-not-track, orthogonal x-slide / y-press-dip composition, locale-independent knob geometry, `accessible-role: switch` cascade. |
| `91e8fae feat(abdu-slint-ui): IconButton component + preview + playground section` | The full IconButton slice. Adds `IconButtonSize` enum, new `components/icon-button.slint` (~340 lines), preview, playground section (~330 lines), sidebar tile + section routing. Also adds the toolbar spinner-period slider and sidebar lock. |
| `56c93c9 fix(abdu-slint-ui): rotating loading spinner with global period` | Replaces Button's opacity-pulse fallback with real rotation via `transform-rotation` + `animation-tick()`. Renames the unused `Animation.pulse` (1500ms) to `Animation.spinner-period` (1200ms, in-out for runtime tuning). Corrects HANDOVER quirk #3. |
| `3c67439 feat(abdu-slint-ui): accessible-* wiring on Button with aria cascade` | Wires `accessible-role/label/checkable/checked/enabled/action-default` on Button. Cascade `aria-label → tooltip → label → "Button"`. Adds the debug-bounds magenta corner dot for missing accessibility metadata. |
| `cb78876 docs(abdu-slint-ui): IconButton design + Depth/accessibility plan` | Per-component design doc at `architecture/icon-button.md`. Documents the API surface, the Depth global signature, the accessibility cascade pattern, and the three-commit build order. |
| `1dd82cc refactor(abdu-slint-ui): extract shadow math into Depth global` | Pulls Button's ~50 lines of shadow-resolution state into `globals/depth.slint` as seven pure functions. Button keeps its public depth API unchanged, math is now consolidated for IconButton/Toggle/Card to share. |

Prior commits (Foundation + Button iteration, captured in commit `70a6f0f docs(abdu-slint-ui): capture mid-Phase 1 state in HANDOVER`):

| Commit prefix | Summary |
| -- | -- |
| `feat(abdu-slint-ui): scaffold Phase 1 foundation` | crate, enums, globals, fonts |
| `feat(abdu-slint-ui-playground): scaffold playground shell` | window + toolbar |
| `feat(abdu-slint-ui): Button component and preview` | first slice |
| `feat(abdu-slint-ui-playground): Button section with property controls` | live property panel |
| `fix(abdu-slint-ui): wire toolbar, tone, bg-color, shadows, tooltip` | feedback after first playground run |
| `feat(abdu-slint-ui): color palette swatch grid and height-override` | 18-color visual palette |
| `feat(abdu-slint-ui): switch design language from shadcn to iOS / SwiftUI` | the pivot |
| `feat(abdu-slint-ui): real depth on filled buttons — gradient + highlight + visible shadow` | first attempt at depth |
| `fix(abdu-slint-ui): drop-shadow on Button now actually renders` | the compiler-pass discovery |
| `feat(abdu-slint-ui): shadow-elevation and shadow-color properties on Button` | tunable shadow |
| `feat(abdu-slint-ui): shadow-direction property for simulated light angle` | directional shadow |
| `feat(abdu-slint-ui): thickness property — visible 3D depth on Button` | first stripe attempt |
| `fix(abdu-slint-ui): real extrusion for thickness instead of bottom stripe` | two-layer approach |
| `feat(abdu-slint-ui): expose press-animation as a Button property` | press machinery |
