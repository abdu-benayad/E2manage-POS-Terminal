# abdu-slint-ui — Session Handover

> Snapshot mid-Phase 1. Read this first when starting a fresh session.

---

## TL;DR

- **Project:** `abdu-slint-ui` — a Slint UI component library + companion playground app, built to replace the inline Slint patterns currently bloating `e2manage-pos-terminal` and provide a reusable, well-styled foundation.
- **Phase 0 status:** ✅ complete (design docs).
- **Phase 1 status:** 🔄 in progress. **Foundation complete + Button fully built**. 4 components + smoke test remain.
- **Design language pivoted:** shadcn-inspired → **iOS / SwiftUI**. Larger sizes for POS tablets, glossy gradients, soft drop shadows, system color palette.

---

## Where things live

| Path                                                          | What                              |
| ------------------------------------------------------------- | --------------------------------- |
| `e2manage-pos-terminal/abdu-slint-ui/`                        | **This library** (compiling, Button shipped) |
| `e2manage-pos-terminal/abdu-slint-ui-playground/`             | **Playground app** (compiling, Button section live) |
| `e2manage-pos-terminal/ui/spike/shadcn_button.slint`          | Original Phase 0 spike (kept for reference) |
| `e2manage-pos-terminal/ui/`                                   | Existing POS UI (untouched, Phase 4 target) |

---

## Document set

1. **`HANDOVER.md`** (this file) — current state
2. **`README.md`** — Phase 0 design contract. **Partially stale** — written with shadcn assumptions before the iOS pivot. The high-level philosophy still applies; the specific styling assertions (button heights, palette, "narrow APIs") have evolved. Revisit before Phase 2.
3. **`CLAUDE.md`** — construction discipline that overrides `~/.claude/CLAUDE.md` inside this directory.
4. **`ROADMAP.md`** — phase plan with decision gates.
5. **`IMPL.md`** — Phase 1 file-creation spec. The Button section is **superseded by the actual implementation** (see `Button API` below); the other 4 components still match the spec.

---

## What's built

### Library (`abdu-slint-ui/`)

| Layer                | File(s)                                | Status |
| -------------------- | -------------------------------------- | ------ |
| Crate scaffold       | `Cargo.toml`, `build.rs`, `src/lib.rs` | ✅      |
| LICENSEs             | `LICENSE-MIT`, `LICENSE-APACHE`        | ✅      |
| Public entry         | `lib.slint`                            | ✅      |
| Enums                | `enums.slint` (10 enums)               | ✅      |
| Globals              | 9 files under `globals/`               | ✅ iOS-tuned |
| Fonts                | `assets/phosphor.ttf`, `assets/lucide.ttf` | ✅ both bundled, switchable at runtime |
| `Button` component   | `components/button.slint`              | ✅ extensively iterated, see API below |
| `Button` preview     | `previews/button.slint`                | ✅      |
| `IconButton`         | —                                      | ❌ pending |
| `Toggle`             | —                                      | ❌ pending |
| `Card`               | —                                      | ❌ pending |
| `KeyValueRow`        | —                                      | ❌ pending |

### Playground (`abdu-slint-ui-playground/`)

| Layer                | File(s)                                | Status |
| -------------------- | -------------------------------------- | ------ |
| Crate scaffold       | `Cargo.toml`, `build.rs`, `src/main.rs` | ✅      |
| Window shell         | `ui/playground.slint`                  | ✅ sidebar + toolbar |
| Toolbar tokens       | button-shape, icon-btn-shape, card-shape, density, currency, icon-family, RTL | ✅ all wire to globals directly via `selected(v)` callback (not via intermediate state) |
| Button section       | `ui/sections/button.slint`             | ✅ full property panel |
| Other sections       | —                                      | ❌ pending per component |
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

The Button has grown substantially beyond IMPL.md's original 15 properties. This is the current authoritative reference.

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
| `loading`              | `bool`          | `false`            | spinner pulses opacity; blocks click |
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
- Sidebar shows **Button**. Click to mount its section.
- Toolbar across the top: theme-shape combos, density, currency, icon-family swap, RTL toggle.
- Right panel: every Button property surfaced as an interactive control (LineEdits, ComboBoxes, swatch grids, sliders).
- Bottom of preview pane: live code snippet of the current configuration.

Verify the build:

```sh
cd abdu-slint-ui/         && cargo check
cd abdu-slint-ui-playground/ && cargo build
```

Both should build clean (one harmless deprecation warning about Button's component not inheriting Window — known, library-level, can't easily silence in Slint 1.14).

---

## What's next (Phase 1 remainder)

In priority order:

1. **`IconButton`** — adjacent to Button; should be straightforward. The depth properties (shadow-*, thickness, press-animation) should carry over with minimal changes — possibly extract them into a shared mixin or a private base component.

2. **`Toggle`** — iOS-style switch with the rounded knob. iOS has a specific look (green-when-on, gradient knob, subtle inner shadow). The internal-state machine for the slide animation is the trickier part.

3. **`Card`** — surface container. Reads `Theme.card-shape`, exposes `elevation`, `padding`, `interactive`. The depth pattern from Button (shadow-elevation, shadow-color, shadow-direction) should likely apply here too.

4. **`KeyValueRow`** — display-only, RTL-aware. Trivial relative to the above.

5. **Smoke test:** `examples/settings-display.slint` — rewrite the existing 700-line `ui/screens/settings/display.slint` using only Phase 1 primitives. Confirms the API survives contact with a real screen.

### Open API questions for Phase 2 entry

- Should `bg-color` and `height-override` on Button move to a private extension or stay as escape hatches? They make the API less curated but real-life POS sometimes needs them. Decision deferred.
- Should the depth system (shadow-elevation, shadow-color, shadow-direction, thickness, press-animation) be factored out into a shared mixin so IconButton, Toggle, and Card get it without re-declaring 6 properties each? Probably yes; design TBD.
- The `tone` enum override is implemented but not heavily exercised. Worth a playground tour to make sure every `variant × tone` combo looks reasonable before more components rely on the same pattern.

---

## Don't touch (Phases 1–3)

The POS itself stays untouched until Phase 4:

- `e2manage-pos-terminal/ui/` (except the existing `ui/spike/`)
- `e2manage-pos-terminal/src/`
- `e2manage-pos-terminal/crates/`
- `e2manage-pos-terminal/Cargo.toml` (workspace)

The library evolves in isolation. POS integration is its own phase with its own plan.

---

## Commit history (this Phase 1 session)

Selected commits in order — read `git log --oneline` for the full chronology:

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
| `feat(abdu-slint-ui): expose press-animation as a Button property` | latest |
