# abdu-slint-ui — Implementation Plan

> The file-level implementation playbook. Phase 1 is spec'd in full below. Later phases are sketched and filled in when their predecessor closes.
>
> **Document role:** [README.md](./README.md) = *what to build* (design contract). [CLAUDE.md](./CLAUDE.md) = *how to build* (construction discipline). [ROADMAP.md](./ROADMAP.md) = *when, in what order* (phases). **This doc = *precise spec for what each file contains*.**

---

## Phase 1 status (as of latest commit)

- ✅ §1.0 — Library crate skeleton, lib.slint entry point.
- ✅ §1.1 — Enums (now 11 entries; original spec listed 10, `IconButtonSize` was added when IconButton landed).
- ✅ §1.2 — Globals. Original spec listed 8; actual count is **10** (added `icon-font.slint` during font work, then `depth.slint` for the shadow-math extraction). `Animation.pulse` renamed to `Animation.spinner-period`.
- 🔄 §1.3 — Components. **Button** and **IconButton** shipped; their property tables below are **SUPERSEDED** by the actual implementations documented in `HANDOVER.md`. **Toggle / Card / KeyValueRow** specs below are still authoritative.
- 🔄 §1.4 — Preview files. Button and IconButton shipped; others pending.
- ✅ §1.5 — Playground crate skeleton.
- 🔄 §1.6 — Playground sections. Button and IconButton shipped; others pending. **Note:** the playground sections turned out to be pure-Slint with `in-out` state on the section component, not Rust state structs as originally spec'd. The Rust-side state plan in §1.6 below is superseded — actual sections store state directly in Slint properties.
- ❌ §1.7 — Smoke test pending.

### Net additions beyond the original spec

- `globals/depth.slint` (the Depth global — stateless shadow math, mentioned in HANDOVER's Depth section).
- `architecture/icon-button.md` — first per-component design doc. New convention: each non-trivial component gets a design doc here.
- Accessibility cascade pattern wired on Button and IconButton (was not in the original spec).
- `globals/icon-font.slint` (dual-font with Phosphor + Lucide, runtime-switchable; original spec assumed a single font).

---

## Phase 1 — Foundation

**Goal recap (from ROADMAP):** 8 globals + 5 components (Button, IconButton, Toggle, Card, KeyValueRow) + playground app shell with 5 sections + one smoke-test screen.

Build order:

1. Library crate skeleton (§1.0)
2. Enum definitions (§1.1) — must land first, components depend on them
3. Globals (§1.2) — must land before components
4. Components 1–5 (§1.3) — in the listed order; later components in this batch depend on earlier
5. Preview files (§1.4) — one per component, parallel with component work
6. Playground crate skeleton (§1.5)
7. Playground sections (§1.6) — one per component
8. Smoke-test example (§1.7)

---

## §1.0 — Library crate setup

### `abdu-slint-ui/Cargo.toml`

```toml
[package]
name = "abdu-slint-ui"
version = "0.1.0"
edition = "2021"
license = "MIT OR Apache-2.0"
description = "Modern, environment-aware Slint UI primitives for Rust applications"
authors = ["Abdu Benayad <abdu.benayad@gmail.com>"]
keywords = ["slint", "ui", "components", "design-system", "rtl"]
categories = ["gui"]

[lib]
path = "src/lib.rs"

[dependencies]
slint = { version = "1.8", default-features = false, features = ["compat-1-2"] }

[build-dependencies]
slint-build = "1.8"
```

### `abdu-slint-ui/build.rs`

```rust
fn main() {
    slint_build::compile("lib.slint").unwrap();
}
```

### `abdu-slint-ui/src/lib.rs`

```rust
slint::include_modules!();
```

### `abdu-slint-ui/lib.slint`

Single import entry point. Re-exports every public component, global, and enum:

```slint
// Globals
export * from "globals/theme.slint";
export * from "globals/typography.slint";
export * from "globals/spacing.slint";
export * from "globals/radius.slint";
export * from "globals/sizes.slint";
export * from "globals/animation.slint";
export * from "globals/locale.slint";
export * from "globals/currency-format.slint";
export * from "globals/icon-font.slint";

// Enums
export * from "enums.slint";

// Components (added as each lands)
export * from "components/button.slint";
export * from "components/icon-button.slint";
export * from "components/toggle.slint";
export * from "components/card.slint";
export * from "components/key-value-row.slint";
```

### `LICENSE-MIT`, `LICENSE-APACHE`

Standard text from https://opensource.org/licenses (verbatim, no modifications).

---

## §1.1 — Slint enum definitions

### `abdu-slint-ui/enums.slint`

All Phase 1 enums in one file. Components import what they need.

```slint
export enum ButtonVariant {
    default,        // primary action — solid near-black
    destructive,    // red, for destructive actions
    outline,        // bordered, transparent background
    secondary,      // muted background, dark foreground
    ghost,          // transparent until hover
    link,           // text-only, underline on hover
}

export enum ButtonSize {
    xs,    // h-7, text-xs
    sm,    // h-9, text-sm
    md,    // h-10, text-sm (default)
    lg,    // h-11, text-base
    xl,    // h-12, text-base
    icon,  // square (h-10 w-10), icon-only
}

export enum Shape {
    default,   // sentinel — follows theme token
    rounded,   // Radius.md (~8px)
    pill,      // height/2 — capsule
    square,    // 0px
    circle,    // for icon variants only — 1:1 + pill
}

export enum Tone {
    default,
    primary,
    success,
    warning,
    destructive,
    info,
    muted,
}

export enum Elevation {
    none,
    sm,    // subtle, e.g. cards at rest
    md,    // hovered cards, sticky surfaces
    lg,    // dialogs, popovers
    xl,    // toasts, top-level overlays
}

export enum Density {
    compact,    // tighter padding/heights
    default,    // standard
    comfortable, // looser padding/heights
}

export enum Emphasis {
    subtle,
    normal,
    strong,
    total,    // for grand totals / final values
}

export enum ToggleSize {
    sm, md, lg,
}

export enum TonalSurface {
    on-surface,
    on-primary,
    on-dark,
    on-light,
}
```

---

## §1.2 — Globals (8 files)

Each global lives in its own file under `globals/`. Properties marked `out` are read-only from the library's perspective (consumer sees defaults); `in-out` properties are populated by the consumer at startup.

### `globals/theme.slint`

| Property                | Direction | Type     | Default     | Notes                              |
| ----------------------- | --------- | -------- | ----------- | ---------------------------------- |
| `background`            | `out`     | `color`  | `#ffffff`   | Page-level background              |
| `foreground`            | `out`     | `color`  | `#020817`   | Primary text on background         |
| `surface`               | `out`     | `color`  | `#ffffff`   | Card/dialog surface                |
| `surface-muted`         | `out`     | `color`  | `#f1f5f9`   | Subtle surface variant             |
| `primary`               | `out`     | `color`  | `#0f172a`   | Near-black, shadcn signature       |
| `primary-hover`         | `out`     | `color`  | `#1e293b`   |                                    |
| `primary-foreground`    | `out`     | `color`  | `#f8fafc`   | Text on primary surfaces           |
| `destructive`           | `out`     | `color`  | `#ef4444`   |                                    |
| `destructive-hover`     | `out`     | `color`  | `#dc2626`   |                                    |
| `destructive-foreground`| `out`     | `color`  | `#f8fafc`   |                                    |
| `secondary`             | `out`     | `color`  | `#f1f5f9`   |                                    |
| `secondary-hover`       | `out`     | `color`  | `#e2e8f0`   |                                    |
| `secondary-foreground`  | `out`     | `color`  | `#0f172a`   |                                    |
| `accent`                | `out`     | `color`  | `#f1f5f9`   | Hover bg for ghost/outline         |
| `accent-foreground`     | `out`     | `color`  | `#0f172a`   |                                    |
| `success`               | `out`     | `color`  | `#10b981`   |                                    |
| `success-foreground`    | `out`     | `color`  | `#f0fdf4`   |                                    |
| `warning`               | `out`     | `color`  | `#f59e0b`   |                                    |
| `warning-foreground`    | `out`     | `color`  | `#fffbeb`   |                                    |
| `info`                  | `out`     | `color`  | `#3b82f6`   |                                    |
| `info-foreground`       | `out`     | `color`  | `#eff6ff`   |                                    |
| `border`                | `out`     | `color`  | `#e2e8f0`   |                                    |
| `ring`                  | `out`     | `color`  | `#94a3b8`   | Focus ring color                   |
| `muted-foreground`      | `out`     | `color`  | `#64748b`   | Secondary text                     |
| `button-shape`          | `in-out`  | `string` | `"pill"`    | One of `rounded \| pill \| square` |
| `card-shape`            | `in-out`  | `string` | `"rounded"` |                                    |
| `icon-button-shape`     | `in-out`  | `string` | `"circle"`  | One of `rounded \| pill \| circle \| square` |
| `density`               | `in-out`  | `string` | `"default"` | `compact \| default \| comfortable` |
| `shadow-sm-blur`        | `out`     | `length` | `2px`       |                                    |
| `shadow-sm-y`           | `out`     | `length` | `1px`       |                                    |
| `shadow-sm-color`       | `out`     | `color`  | `rgba(0,0,0,0.05)` |                            |
| `shadow-md-blur`        | `out`     | `length` | `6px`       |                                    |
| `shadow-md-y`           | `out`     | `length` | `2px`       |                                    |
| `shadow-md-color`       | `out`     | `color`  | `rgba(0,0,0,0.08)` |                            |
| `shadow-lg-blur`        | `out`     | `length` | `16px`      |                                    |
| `shadow-lg-y`           | `out`     | `length` | `6px`       |                                    |
| `shadow-lg-color`       | `out`     | `color`  | `rgba(0,0,0,0.12)` |                            |

### `globals/typography.slint`

| Property        | Direction | Type     | Default                                          |
| --------------- | --------- | -------- | ------------------------------------------------ |
| `font-family`   | `in-out`  | `string` | `"Inter, system-ui, -apple-system, sans-serif"`  |
| `font-family-ar`| `in-out`  | `string` | `"Cairo, Tajawal, Noto Sans Arabic, sans-serif"` |
| `text-xs`       | `out`     | `length` | `12px`                                           |
| `text-sm`       | `out`     | `length` | `14px`                                           |
| `text-base`     | `out`     | `length` | `16px`                                           |
| `text-lg`       | `out`     | `length` | `18px`                                           |
| `text-xl`       | `out`     | `length` | `20px`                                           |
| `text-2xl`      | `out`     | `length` | `24px`                                           |
| `text-3xl`      | `out`     | `length` | `32px`                                           |
| `text-display`  | `out`     | `length` | `48px`                                           |
| `weight-regular`| `out`     | `int`    | `400`                                            |
| `weight-medium` | `out`     | `int`    | `500`                                            |
| `weight-semibold`| `out`    | `int`    | `600`                                            |
| `weight-bold`   | `out`     | `int`    | `700`                                            |

### `globals/spacing.slint`

| Property | Direction | Type     | Default |
| -------- | --------- | -------- | ------- |
| `0`      | `out`     | `length` | `0px`   |
| `xs`     | `out`     | `length` | `4px`   |
| `sm`     | `out`     | `length` | `8px`   |
| `md`     | `out`     | `length` | `12px`  |
| `lg`     | `out`     | `length` | `16px`  |
| `xl`     | `out`     | `length` | `24px`  |
| `2xl`    | `out`     | `length` | `32px`  |
| `3xl`    | `out`     | `length` | `48px`  |
| `4xl`    | `out`     | `length` | `64px`  |

### `globals/radius.slint`

| Property | Direction | Type     | Default  |
| -------- | --------- | -------- | -------- |
| `none`   | `out`     | `length` | `0px`    |
| `sm`     | `out`     | `length` | `4px`    |
| `md`     | `out`     | `length` | `8px`    |
| `lg`     | `out`     | `length` | `12px`   |
| `xl`     | `out`     | `length` | `16px`   |
| `2xl`    | `out`     | `length` | `24px`   |
| `full`   | `out`     | `length` | `9999px` |

### `globals/sizes.slint`

| Property                  | Direction | Type     | Default |
| ------------------------- | --------- | -------- | ------- |
| `touch-target`            | `out`     | `length` | `48px`  |
| `button-xs`               | `out`     | `length` | `28px`  |
| `button-sm`               | `out`     | `length` | `36px`  |
| `button-md`               | `out`     | `length` | `40px`  |
| `button-lg`               | `out`     | `length` | `44px`  |
| `button-xl`               | `out`     | `length` | `48px`  |
| `icon-button-square`      | `out`     | `length` | `40px`  |
| `input-sm`                | `out`     | `length` | `36px`  |
| `input-md`                | `out`     | `length` | `40px`  |
| `input-lg`                | `out`     | `length` | `48px`  |
| `icon-xs`                 | `out`     | `length` | `12px`  |
| `icon-sm`                 | `out`     | `length` | `16px`  |
| `icon-md`                 | `out`     | `length` | `20px`  |
| `icon-lg`                 | `out`     | `length` | `24px`  |
| `icon-xl`                 | `out`     | `length` | `32px`  |
| `border-thin`             | `out`     | `length` | `1px`   |
| `border-medium`           | `out`     | `length` | `2px`   |
| `focus-ring`              | `out`     | `length` | `2px`   |
| `focus-ring-offset`       | `out`     | `length` | `2px`   |

### `globals/animation.slint`

| Property      | Direction | Type        | Default |
| ------------- | --------- | ----------- | ------- |
| `instant`     | `out`     | `duration`  | `0ms`   |
| `fast`        | `out`     | `duration`  | `120ms` |
| `normal`      | `out`     | `duration`  | `200ms` |
| `slow`        | `out`     | `duration`  | `300ms` |
| `slower`      | `out`     | `duration`  | `500ms` |
| `pulse`       | `out`     | `duration`  | `1500ms`|

Easings are Slint built-in (`ease-out`, `ease-in`, `ease-in-out`, `ease`). The library uses `ease-out` for state changes and `ease-in-out` for continuous animations (pulse). No custom cubic-beziers in v1.

### `globals/locale.slint`

| Property        | Direction | Type     | Default | Notes                                    |
| --------------- | --------- | -------- | ------- | ---------------------------------------- |
| `current`       | `in-out`  | `string` | `"en"`  | ISO 639-1 code                           |
| `rtl`           | `in-out`  | `bool`   | `false` | Single source of truth for layout direction |

Plus helper functions (computed properties):

| Function         | Returns | LTR     | RTL     |
| ---------------- | ------- | ------- | ------- |
| `arrow-start`    | string  | `"←"`   | `"→"`   |
| `arrow-end`      | string  | `"→"`   | `"←"`   |
| `chevron-start`  | string  | `"‹"`   | `"›"`   |
| `chevron-end`    | string  | `"›"`   | `"‹"`   |

### `globals/currency-format.slint`

| Property         | Direction | Type     | Default | Notes                                      |
| ---------------- | --------- | -------- | ------- | ------------------------------------------ |
| `currency`       | `in-out`  | `string` | `"USD"` | ISO 4217 or symbol (e.g. `"SAR"`, `"ر.س"`) |
| `symbol-position`| `in-out`  | `string` | `"leading"` | `leading \| trailing`                  |
| `decimals`       | `in-out`  | `int`    | `2`     |                                            |
| `grouping`       | `in-out`  | `string` | `","`   | thousands separator                        |
| `decimal-mark`   | `in-out`  | `string` | `"."`   |                                            |

### `globals/icon-font.slint`

```slint
export global IconFont {
    out property <string> family: "Phosphor";  // or "Lucide" — chosen in Phase 1
    
    // Most-common icon names mapped to their codepoints.
    // Full lookup table generated from the bundled font.
    // Consumers pass icon names ("check", "chevron-right", etc.); components
    // resolve to the codepoint character here.
    
    pure public function resolve(name: string) -> string {
        // Slint doesn't have hashmaps; this is a ternary chain over the
        // ~150 most-used icons. If `name` doesn't match, return it as-is
        // (so consumers can pass raw codepoints or emoji as fallback).
        return
            name == "check"          ? "\u{e182}"
          : name == "x"              ? "\u{e1d6}"
          : name == "chevron-left"   ? "\u{e198}"
          : name == "chevron-right"  ? "\u{e19a}"
          // ... ~150 entries total
          : name;
    }
}
```

The ternary chain is generated from the icon font's manifest. The list of named icons supported in v1 lives in `abdu-slint-ui/icons-supported.md` (also a Phase 1 deliverable — a one-page table mapping the ~150 named icons to their codepoints, so consumers know what's available).

Bundle the font via Slint's font import in `lib.slint`:

```slint
@image-url("assets/icons.ttf");
```

(Exact mechanism depends on Slint version; Phase 1 implementation verifies the correct directive.)

---

## §1.3 — Phase 1 components

For each component: file path, property table (every public property), callback list, internal state machine description, and acceptance criteria. Property tables here **supersede** the README's tables — the README is overview, this is precise spec.

### Component 1: `components/button.slint`

The foundation. Variants and sizes mirror shadcn/ui; convenience features mirror Material UI / Mantine.

| Property        | Type            | Default           | Description                                                        |
| --------------- | --------------- | ----------------- | ------------------------------------------------------------------ |
| `label`         | `string`        | `""`              | Visible text. Empty for icon-only buttons (use IconButton instead).|
| `icon-leading`  | `string`        | `""`              | Icon name (or codepoint/emoji fallback). On reading-start side.    |
| `icon-trailing` | `string`        | `""`              | Icon name. On reading-end side.                                    |
| `variant`       | `ButtonVariant` | `default`         |                                                                    |
| `size`          | `ButtonSize`    | `md`              |                                                                    |
| `shape`         | `Shape`         | `default`         | Sentinel — follows `Theme.button-shape`.                           |
| `tone`          | `Tone`          | `default`         | Overrides variant color tone (e.g. success-toned outline).          |
| `disabled`      | `bool`          | `false`           | Visually muted, blocks click and keyboard activation.              |
| `loading`       | `bool`          | `false`           | Replaces label with a spinner; keeps width; blocks `clicked`.       |
| `full-width`    | `bool`          | `false`           | Stretches to parent's available width.                              |
| `checkable`     | `bool`          | `false`           | When true, button acts as a toggle button.                          |
| `checked`       | `bool`          | `false`           | Controlled state for toggle buttons.                                |
| `tooltip`       | `string`        | `""`              | Hover text. Empty disables tooltip.                                 |
| `min-width`     | `length`        | `0px`             | Lower bound on button width (`0` = hugs content).                   |
| `aria-label`    | `string`        | `""`              | Required when `label` is empty.                                     |

**Callbacks:**

| Callback                  | Fires when                                          |
| ------------------------- | --------------------------------------------------- |
| `clicked()`               | Tap, click, or Enter/Space when focused             |
| `pressed-changed(bool)`   | Physical press state transitions                    |
| `hover-changed(bool)`     | Mouse enters / leaves                               |
| `focus-changed(bool)`     | Keyboard focus gained / lost                        |

**Internal state machine:** a 6-state visual machine (`rest`, `hover`, `pressed`, `focus`, `disabled`, `loading`). State transitions animated via `animate background` and `animate opacity` using `Animation.fast` and `ease-out`.

**Reads from environment:** `Theme`, `Typography`, `Sizes`, `Radius`, `Spacing`, `Animation`, `Locale`, `IconFont`.

**Acceptance criteria for §1.7 visual validation gate:**

- All 6 variants render correctly (default, destructive, outline, secondary, ghost, link)
- All 6 sizes render correctly (xs through xl + icon)
- Shape `pill`, `rounded`, `square` each render correctly
- All states: rest, hover, pressed, focus (keyboard tab to focus), disabled, loading
- Icon-leading and icon-trailing render with correct gap and position in both LTR and RTL
- Tooltip appears on hover after ~500ms
- `checkable + checked` shows pressed visual permanently

### Component 2: `components/icon-button.slint`

> **SUPERSEDED — see `HANDOVER.md` → "IconButton API (as built)" and `architecture/icon-button.md` for the authoritative spec.** The actual implementation has 19 properties (not 9), uses a new `IconButtonSize` enum, ships the full depth set via the `Depth` global, and wires the accessibility cascade. The original spec below is left for historical reference.

Square click target. Convenience over `Button { size: icon; label: ""; icon-leading: ... }` for the common icon-only case.

| Property      | Type            | Default     | Description                                              |
| ------------- | --------------- | ----------- | -------------------------------------------------------- |
| `icon`        | `string`        | `""`        | **Required.** Icon name or codepoint.                    |
| `variant`     | `ButtonVariant` | `ghost`     | Subset typically: `default`, `outline`, `ghost`.         |
| `size`        | `ButtonSize`    | `md`        | `xs \| sm \| md \| lg \| xl` — square aspect derived.    |
| `shape`       | `Shape`         | `default`   | Sentinel — follows `Theme.icon-button-shape` (default `circle`). |
| `tone`        | `Tone`          | `default`   |                                                          |
| `disabled`    | `bool`          | `false`     |                                                          |
| `loading`     | `bool`          | `false`     | Spinner replaces icon.                                   |
| `tooltip`     | `string`        | `""`        |                                                          |
| `aria-label`  | `string`        | `""`        | **Required** (screen reader; never visible).             |

**Callbacks:** `clicked()`, `pressed-changed(bool)`, `hover-changed(bool)`, `focus-changed(bool)`

**State machine:** identical to Button.

**Reads from environment:** `Theme`, `Sizes`, `Radius`, `Animation`, `IconFont`.

**Acceptance criteria:**

- Square aspect at all sizes
- All 3 typical variants render correctly
- Circle, pill, rounded shapes each render correctly (circle is the default)
- All states + tooltip behave as Button

### Component 3: `components/toggle.slint`

A switch with smooth knob slide.

| Property      | Type         | Default | Description                                                    |
| ------------- | ------------ | ------- | -------------------------------------------------------------- |
| `on`          | `bool`       | `false` | Current state (controlled).                                    |
| `size`        | `ToggleSize` | `md`    |                                                                |
| `label`       | `string`     | `""`    | Optional label rendered next to the toggle.                    |
| `description` | `string`     | `""`    | Optional caption below the label.                              |
| `on-icon`     | `string`     | `""`    | Optional icon shown in the knob when `on` (e.g. `"check"`).    |
| `off-icon`    | `string`     | `""`    | Optional icon shown in the knob when off.                      |
| `disabled`    | `bool`       | `false` |                                                                |
| `aria-label`  | `string`     | `""`    | Required when `label` is empty.                                |

**Callbacks:**

| Callback           | Fires when                                |
| ------------------ | ----------------------------------------- |
| `toggled(bool)`    | User activates (click, tap, Space, Enter) |
| `focus-changed(bool)` | Keyboard focus gained / lost           |

**Internal state machine:** 2 states (`off`, `on`). Knob `x` animates between the two extremes using `Animation.fast` + `ease-out`. Track color transitions between `Theme.border` (off) and `Theme.primary` (on).

**Reads from environment:** `Theme`, `Typography`, `Sizes`, `Radius`, `Spacing`, `Animation`, `Locale`, `IconFont`.

**Acceptance criteria:**

- 3 sizes render correctly with proportional knob and track
- Toggle animates smoothly (no jump)
- Label + description layout correct in LTR and RTL
- Disabled state visibly muted, ignores clicks and keyboard input
- Focus ring renders when focused via keyboard

### Component 4: `components/card.slint`

Surface container with shadow and radius.

| Property      | Type        | Default     | Description                                                    |
| ------------- | ----------- | ----------- | -------------------------------------------------------------- |
| `elevation`   | `Elevation` | `sm`        |                                                                |
| `interactive` | `bool`      | `false`     | When true: hover/press feedback, emits `clicked()`.            |
| `padding`     | `Density`   | `default`   | `compact \| default \| comfortable` — maps to `Spacing.md/lg/xl`. |
| `shape`       | `Shape`     | `default`   | Sentinel — follows `Theme.card-shape` (default `rounded`).     |
| `bordered`    | `bool`      | `true`      | When `elevation: none`, border provides definition.            |
| `max-width`   | `length`    | `0px`       | `0` = no cap.                                                   |

**Slot:** children become card body content.

**Callbacks:** `clicked()` — only emitted when `interactive: true`.

**Internal state machine:** when `interactive`: 4 states (`rest`, `hover`, `pressed`, `focus`). Hover increases elevation by one step (`sm → md`, `md → lg`).

**Reads from environment:** `Theme`, `Radius`, `Spacing`, `Animation`.

**Acceptance criteria:**

- All 5 elevations render visibly distinct shadows
- All 3 paddings produce correct internal spacing
- Interactive card has visible hover/press feedback
- Non-interactive card has zero hover/press feedback
- `max-width` constrains correctly when set

### Component 5: `components/key-value-row.slint`

Label-value pair, RTL-aware. The building block for breakdowns, totals, summaries.

| Property        | Type        | Default     | Description                                                       |
| --------------- | ----------- | ----------- | ----------------------------------------------------------------- |
| `label`         | `string`    | `""`        | Leading side (LTR: left, RTL: right).                             |
| `value`         | `string`    | `""`        | Trailing side.                                                    |
| `value-icon`    | `string`    | `""`        | Icon shown next to the value (e.g., trend arrow).                 |
| `emphasis`      | `Emphasis`  | `normal`    | Visual weight: `subtle \| normal \| strong \| total`.             |
| `value-tone`    | `Tone`      | `default`   |                                                                   |
| `density`       | `Density`   | `default`   | Affects vertical padding.                                         |
| `show-divider`  | `bool`      | `false`     | Bottom border between rows.                                       |

**Callbacks:** none — display-only.

**Internal state machine:** none.

**Reads from environment:** `Theme`, `Typography`, `Spacing`, `Locale`, `IconFont`.

**Acceptance criteria:**

- Label appears on physical left in LTR, right in RTL
- Value appears on the opposite side
- All 4 emphasis levels render with distinct visual weight
- All 5 tones color the value distinctly
- Divider renders correctly when enabled
- Density affects only padding; text size unchanged

---

## §1.4 — Preview files

One per component, lives in `abdu-slint-ui/previews/{name}.slint`. Each preview file is a `Window` showing every variant × size × state × locale matrix. Run with `slint-viewer previews/{name}.slint`.

### `previews/button.slint`

Structure:

```
Window
├── Title: "Button"
├── Description
├── Section "Variants" — row of 6 (default, destructive, outline, secondary, ghost, link)
├── Section "Sizes" — row of 6 (xs through xl + icon)
├── Section "Shapes" — row of 3 (rounded, pill, square)
├── Section "With icon" — row of 4 (leading, trailing, both, icon-only)
├── Section "States" — row of 5 (rest, hover-via-mouseover, focus-via-Tab, disabled, loading)
├── Section "Checkable" — row of 2 (unchecked, checked)
├── Section "Full-width" — single button
├── Section "RTL" — toggle bound to button that flips Locale.rtl, then all sections re-render
```

### `previews/icon-button.slint`

```
Window
├── Section "Variants" — row of 3 (default, outline, ghost)
├── Section "Sizes" — row of 5 (xs through xl)
├── Section "Shapes" — row of 4 (default→circle, rounded, pill, square)
├── Section "States" — disabled, loading
├── Section "RTL" — toggle
```

### `previews/toggle.slint`

```
Window
├── Section "Sizes" — row of 3
├── Section "States" — off, on, disabled-off, disabled-on
├── Section "With label" — toggles with label + description
├── Section "With icons" — toggles using on-icon/off-icon
├── Section "RTL" — toggle
```

### `previews/card.slint`

```
Window
├── Section "Elevation" — 5 cards in a row, each at a different elevation
├── Section "Padding" — 3 cards with different padding
├── Section "Shapes" — 3 cards (rounded, pill, square — note: pill cards look odd; document)
├── Section "Interactive" — interactive vs non-interactive comparison
├── Section "Max-width" — wide and narrow
```

### `previews/key-value-row.slint`

```
Window
├── Section "Emphasis" — column of 4 rows (subtle, normal, strong, total)
├── Section "Tones" — column of 5 rows (default, positive, negative, muted, primary value-tone)
├── Section "Value-icon" — row with arrow-up, arrow-down icons
├── Section "Density" — 3 columns
├── Section "Divider" — column of 5 rows with show-divider: true
├── Section "RTL" — toggle, all sections re-render mirrored
```

Each preview file is ~150–300 lines of Slint.

---

## §1.5 — Playground crate setup

### `abdu-slint-ui-playground/Cargo.toml`

```toml
[package]
name = "abdu-slint-ui-playground"
version = "0.1.0"
edition = "2021"
license = "MIT OR Apache-2.0"
description = "Interactive catalog for abdu-slint-ui components"

[dependencies]
slint = "1.8"
abdu-slint-ui = { path = "../abdu-slint-ui" }

[build-dependencies]
slint-build = "1.8"
```

### File layout

```
abdu-slint-ui-playground/
├── Cargo.toml
├── build.rs                   # compiles ui/playground.slint
├── src/
│   ├── main.rs                # entry: populates abdu-slint-ui globals from Rust, mounts window
│   ├── lib.rs                 # exports state modules
│   └── state/
│       ├── mod.rs
│       ├── button.rs          # ButtonState struct, fields = every public Button property
│       ├── icon_button.rs
│       ├── toggle.rs
│       ├── card.rs
│       └── key_value_row.rs
└── ui/
    ├── playground.slint       # window root
    ├── chrome/
    │   ├── sidebar.slint      # uses std-widgets ListView
    │   ├── toolbar.slint      # global controls (theme shape, locale, currency)
    │   └── property-panel.slint  # property control templates
    └── sections/
        ├── button.slint
        ├── icon-button.slint
        ├── toggle.slint
        ├── card.slint
        └── key-value-row.slint
```

### `src/main.rs` shape

```rust
use slint::ComponentHandle;

fn main() -> Result<(), slint::PlatformError> {
    let window = PlaygroundWindow::new()?;
    // Populate abdu-slint-ui globals from playground defaults
    let theme = window.global::<abdu_slint_ui::Theme>();
    theme.set_button_shape("pill".into());
    // ... etc
    window.run()
}
```

### `ui/playground.slint` layout

```
PlaygroundWindow (Window, 1280×800 preferred)
├── HorizontalLayout
│   ├── Sidebar (240px fixed) — std-widgets ListView showing component names
│   ├── VerticalLayout (stretch)
│   │   ├── Toolbar (56px fixed)
│   │   │   ├── Theme shape combo (rounded/pill/square)
│   │   │   ├── Locale toggle (LTR/RTL)
│   │   │   ├── Currency combo (USD/SAR/EUR/...)
│   │   │   └── Density combo
│   │   └── HorizontalLayout (stretch)
│   │       ├── Preview pane (stretch, scrollable)
│   │       └── Property panel (320px fixed, scrollable)
│   │           ├── Property controls (varies per section)
│   │           └── Code snippet panel (bottom 200px)
```

Switching the sidebar selection swaps which section file is mounted in the preview pane and property panel.

---

## §1.6 — Playground sections

For each component, two artifacts: a Rust state struct and a Slint section file.

### Section 1: Button

#### `src/state/button.rs`

```rust
#[derive(Default, Clone)]
pub struct ButtonState {
    pub label: String,
    pub icon_leading: String,
    pub icon_trailing: String,
    pub variant: String,    // mapped to enum at the Slint boundary
    pub size: String,
    pub shape: String,
    pub tone: String,
    pub disabled: bool,
    pub loading: bool,
    pub full_width: bool,
    pub checkable: bool,
    pub checked: bool,
    pub tooltip: String,
    pub min_width_px: f32,
    pub aria_label: String,
}
```

#### `ui/sections/button.slint`

Controls panel (using std-widgets):

| Property      | Control type        |
| ------------- | ------------------- |
| `label`       | LineEdit            |
| `icon-leading`| LineEdit (with hint "icon name") |
| `icon-trailing`| LineEdit           |
| `variant`     | ComboBox            |
| `size`        | ComboBox            |
| `shape`       | ComboBox            |
| `tone`        | ComboBox            |
| `disabled`    | CheckBox            |
| `loading`     | CheckBox            |
| `full-width`  | CheckBox            |
| `checkable`   | CheckBox            |
| `checked`     | CheckBox (greyed when not `checkable`) |
| `tooltip`     | LineEdit            |
| `min-width`   | Slider 0–400        |
| `aria-label`  | LineEdit            |

Code snippet panel: live-generated string of the current configuration in Slint syntax (for copy-paste).

### Section 2: IconButton

State struct: 8 fields matching every IconButton property. Controls: same patterns — LineEdit for icon, ComboBoxes for enums, CheckBoxes for booleans.

### Section 3: Toggle

State struct: 8 fields. Controls: ComboBox for size, LineEdits for label/description/on-icon/off-icon, CheckBoxes for on/disabled.

### Section 4: Card

State struct: 6 fields. Controls: ComboBoxes for elevation/padding/shape, CheckBoxes for interactive/bordered, Slider for max-width.

A "demo content" rendered inside the card (some Text + a Button) so users see how the card looks with realistic content.

### Section 5: KeyValueRow

State struct: 7 fields. Controls: LineEdits for label/value/value-icon, ComboBoxes for emphasis/value-tone/density, CheckBox for show-divider.

Demo shows 5 rows in a column, each driven by independent state so users can see how multiple rows compose.

---

## §1.7 — Smoke-test example

### `abdu-slint-ui/examples/settings-display.slint`

Re-implementation of `e2manage-pos-terminal/ui/screens/settings/display.slint` (700 lines) using only Phase 1 primitives + globals.

**Target:** ~150–250 lines.

**Coverage of Phase 1 components:**

- Header with `BackButton` (deferred from Phase 1 — temporarily use `IconButton { icon: Locale.arrow-start }`; replace once BackButton lands in Phase 2)
- Language selection: `OptionTile` × 2 (deferred — temporarily render as `Card { interactive: true }` with label and selected indicator)
- Theme selection: similar deferred-OptionTile approach
- Font size selection: deferred-OptionTile approach
- Sound toggle: **`Toggle`** with label + description
- Screen timeout: deferred-OptionTile approach
- Preview section: **`Card`** containing demo content

**What the smoke test validates:**

1. The 5 Phase 1 components compose correctly into a real screen
2. The Card + Toggle + KeyValueRow combination is ergonomic enough for the dominant settings-screen pattern
3. The locale-ternary problem is solved (no `Locale.rtl ? "..." : "..."` anywhere in the example file — all strings are passed in as properties)
4. Line count drops from ~700 to ~150–250

**Limitations of the smoke test in Phase 1:**

Without `OptionTile`, `SectionCard`, `FormRow`, `Chip` (all Phase 2), the smoke test uses workarounds. That's expected and documented in the example file's header comment. The Phase 2 version of `settings-display.slint` (post-OptionTile/SectionCard/FormRow) will be cleaner and the line count will drop further.

The Phase 1 smoke test is a *capability* validation, not a *quality* validation. It proves the architecture works; Phase 2 proves it produces idiomatic results.

---

## Phase 2, 3, 4 — Sketched

Filled in when their predecessor closes. For now:

### Phase 2

- 10 remaining components: `BackButton`, `OptionTile`, `Chip`, `StatusPill`, `SectionCard`, `FormRow`, `Input`, `Money`, `Quantity`, `MoneyInput`
- Per-component property tables matching Phase 1's level of precision
- Playground sections for each
- Three smoke-test examples: `z-report.slint`, `payment-cash.slint`, `return-items.slint`
- Phase 2 IMPL.md section written at start of Phase 2

### Phase 3

- API freeze checklist
- Doc-comment audit
- Version tagging procedure
- License compliance verification
- Phase 3 IMPL.md section written when Phase 2 closes

### Phase 4

- POS integration plan
- Per-screen refactor order
- Old-code-deletion checklist
- Cross-screen visual-regression checklist
- Phase 4 IMPL.md section written when v1.0 ships

---

## Phase 1 definition of done

Phase 1 closes when all of the following are true:

- [ ] Library crate compiles (`cargo build` from `abdu-slint-ui/` succeeds)
- [ ] All 8 globals defined with every property and default value
- [ ] Slint enums file complete
- [ ] All 5 components defined with every property, callback, and state machine
- [ ] All 5 preview files render correctly via `slint-viewer`
- [ ] Every variant × size × state × LTR/RTL combination visually validated for each component
- [ ] Playground crate compiles and runs
- [ ] Playground shell renders with sidebar, toolbar, preview pane, property panel, code snippet panel
- [ ] All 5 playground sections present with every public property exposed as a control
- [ ] Global toolbar controls (theme shape, locale, currency, density) work live across all sections
- [ ] Smoke-test `examples/settings-display.slint` renders correctly and is ~150–250 lines
- [ ] No `TODO`, `FIXME`, `XXX`, hardcoded color/size literals in any committed file
- [ ] Phase 2 IMPL section drafted (next-phase prep)

Closing Phase 1 unlocks the Phase 1 decision point (per ROADMAP): did the API survive contact? If yes, proceed to Phase 2. If no, revise specific components before continuing.
