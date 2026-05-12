# abdu-slint-ui — Session Handover

> Snapshot mid-Phase 1. Read this first when starting a fresh session.

---

## TL;DR

- **Project:** `abdu-slint-ui` — a Slint UI component library + companion playground app, built to replace the inline Slint patterns currently bloating `e2manage-pos-terminal` and provide a reusable, well-styled foundation.
- **Phase 0 status:** ✅ complete (design docs).
- **Phase 1 status:** 🔄 in progress. **Foundation + Button + IconButton fully built**. Depth global extracted, accessibility cascade pattern established, rotating loading spinner with global period token. **3 components + smoke test remain (Toggle → Card → KeyValueRow → settings-display.slint)**.
- **Design language pivoted:** shadcn-inspired → **iOS / SwiftUI**. Larger sizes for POS tablets, glossy gradients, soft drop shadows, system color palette.

---

## Where things live

| Path                                                          | What                              |
| ------------------------------------------------------------- | --------------------------------- |
| `e2manage-pos-terminal/abdu-slint-ui/`                        | **This library** (compiling, Button + IconButton shipped) |
| `e2manage-pos-terminal/abdu-slint-ui/architecture/`           | Per-component design docs. Entries: `button.md` (retroactive), `icon-button.md` (forward). |
| `e2manage-pos-terminal/abdu-slint-ui-playground/`             | **Playground app** (compiling, Button + IconButton sections live) |
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
| `Toggle`             | —                                      | ❌ pending (next) |
| `Card`               | —                                      | ❌ pending |
| `KeyValueRow`        | —                                      | ❌ pending |

### Playground (`abdu-slint-ui-playground/`)

| Layer                | File(s)                                | Status |
| -------------------- | -------------------------------------- | ------ |
| Crate scaffold       | `Cargo.toml`, `build.rs`, `src/main.rs` | ✅      |
| Window shell         | `ui/playground.slint`                  | ✅ sidebar locked at 240px, scrollable toolbar |
| Toolbar tokens       | button-shape, icon-btn-shape, card-shape, density, currency, icon-family, RTL, spinner-period | ✅ all wire to globals directly via `selected(v)` (sidebar uses `horizontal-stretch: 0`; toolbar wrapped in horizontal `ScrollView` for future-proofing) |
| Button section       | `ui/sections/button.slint`             | ✅ full property panel |
| IconButton section   | `ui/sections/icon-button.slint`        | ✅ full property panel, ~330 lines mirroring Button section's structure |
| Other sections       | —                                      | ❌ pending per component (Toggle, Card, KeyValueRow) |
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

Button currently consumes it; IconButton consumes it. Toggle and Card will compose the same global when they land.

---

## Accessibility cascade pattern

Both Button and IconButton wire Slint's `accessible-*` properties to the platform AT tree. A nameless interactive node is worse than a degraded name, so each component resolves `accessible-label` via a cascading fallback:

- **Button:** `aria-label → tooltip → label → "Button"`
- **IconButton:** `aria-label → tooltip → icon → "Button"`

Plus `accessible-role: button`, `accessible-checkable: root.checkable`, `accessible-checked: root.checked`, `accessible-enabled: !disabled && !loading`, and `accessible-action-default` so AT-driven activations fire `clicked()` (and toggle `checked` when `checkable`).

Authoring-time debug: when `debug-bounds: true` AND all three cascade sources are empty (the accessibility name is genuinely falling through to `"Button"`), a 6×6px magenta dot renders at the top-right corner of the button. Invisible in production builds; conspicuous in the playground.

The cascade pattern propagates to every future interactive component. Toggle, Card-when-interactive, and any Phase-2 component with a clickable surface should adopt the same shape.

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

---

## How to resume

```sh
cd /home/abdu/Downloads/e2manage-pos-terminal/abdu-slint-ui-playground
cargo run
```

In the playground:
- Sidebar shows **Button** and **IconButton**. Click either to mount its section.
- Toolbar across the top: theme-shape combos, density, currency, icon-family swap, RTL toggle, **spinner-period slider** (retune `Animation.spinner-period` live). Wrapped in a horizontal `ScrollView` so adding more tokens doesn't compress the sidebar.
- Right panel: every public property surfaced as an interactive control (LineEdits, ComboBoxes, swatch grids, sliders).
- Bottom of preview pane: live code snippet of the current configuration.

Verify the build:

```sh
cd abdu-slint-ui/         && cargo check
cd abdu-slint-ui-playground/ && cargo build
```

Both should build clean. Expect two harmless library-level warnings about Button and IconButton not inheriting Window (`No code will be generated for it` / `This is deprecated`) — same root cause, the components are mounted by `lib.slint` re-exports and used by other Slint files, not instantiated from Rust. Can't easily silence in Slint 1.14.

---

## What's next (Phase 1 remainder)

In priority order:

1. **`Toggle`** — iOS-style switch with the rounded knob. iOS has a specific look (green-when-on, gradient knob, subtle inner shadow). The internal-state machine for the slide animation is the trickier part. Should consume `Depth` for the knob shadow and adopt the accessibility cascade pattern (`accessible-role: switch` or `Switch` per Slint's AccessibleRole enum).

2. **`Card`** — surface container. Reads `Theme.card-shape`, exposes `elevation`, `padding`, `interactive`. The depth properties (shadow-elevation/color/direction) plug directly into `Depth`. When `interactive: true`, add the accessibility cascade and emit `clicked()` like Button.

3. **`KeyValueRow`** — display-only, RTL-aware. Trivial relative to the above. No depth, no accessibility wiring needed (`accessible-role: none` since it's text).

4. **Smoke test:** `examples/settings-display.slint` — rewrite the existing 700-line `ui/screens/settings/display.slint` using only Phase 1 primitives. Confirms the API survives contact with a real screen.

### Open API questions for Phase 2 entry

- Should `bg-color` and `height-override` on Button move to a private extension or stay as escape hatches? They make the API less curated but real-life POS sometimes needs them. **IconButton intentionally omitted `bg-color`** as the first move in this direction. Decision still deferred for Button itself.
- ~~Should the depth system be factored into a shared mixin?~~ **RESOLVED.** Extracted as the `Depth` global (stateless math provider). Each component declares its own 6 depth input properties; the global owns the resolution math. See section above.
- Should variant/tone color resolution also become a global (parallel to `Depth`)? Button and IconButton now duplicate `variant-base-bg / variant-hover-bg / variant-foreground / tone-color / tone-foreground / base-bg / hover-bg / foreground-color / resolved-border-color`. Likely yes, once Card lands and confirms three real call sites. Deferred to Phase 1.5 / Phase 2.
- The `tone` enum override is implemented but not heavily exercised. Worth a playground tour to make sure every `variant × tone` combo looks reasonable before more components rely on the same pattern.
- The icon-only `loader` glyph quality depends on the icon font (Phosphor's looks like a tasteful spinner; Lucide's similar). A future Phase 2 task could lift Slint's own `SpinnerBase` Path math into a private `Spinner` micro-component so the loading visual is identical across icon-family swaps — explored once, rolled back as "doesn't look right yet"; revisit after design-direction stabilizes.

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

Read `git log --oneline` for the full chronology. The most recent five — IconButton vertical slice plus the foundation that supports it — are:

| Commit | Summary |
| -- | -- |
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
