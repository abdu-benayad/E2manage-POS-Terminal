# abdu-slint-ui

Modern, environment-aware Slint UI primitives for Rust applications.

> **Status — design draft (v0).** No code yet. This document is the contract. Read it, push back, iterate. Components land only after the architecture is settled.

---

## What this is

A library of *living* UI components for Slint apps. Each component:

- Reads its environment — theme, locale, currency format — from globals the consumer populates once at startup
- Exposes a narrow public API, usually 3–6 properties per component
- Owns its internal state (hover, focus, press, animation, state machines) so callers don't have to manage it
- Composes into screens with structural code, not inline `Rectangle + TouchArea + Text` rebuilds

If your screens are restating border-radius, drop-shadow, locale ternaries, currency concatenations, and toggle-knob layouts inline, you're missing this layer.

## What this is not

- **Not a fork of `std-widgets.slint`.** It does not wrap or extend Slint's stock components.
- **Not coupled to any domain.** Zero references to `Cart`, `Product`, `Operator`, `Transaction`, or any app type. The library could be dropped into a logistics kiosk, a clinic intake form, or a warehouse scanner unchanged.
- **Not a translation library.** Components take pre-localized strings as input. The library provides RTL-aware layout, never copy.
- **Not theme-agnostic.** One opinionated theme (shadcn-derived). Consumers customize via token overrides, not by forking components.
- **Not yet stable.** Until v1, expect API churn. Consumers should pin a specific commit.

## License & distribution

Licensed under **MIT OR Apache-2.0** (Rust ecosystem convention). Consumers may choose either license. Two `LICENSE-*` files ship in the crate root.

The library bundles an icon font (Phosphor or Lucide — final choice in Phase 1) for use across all icon-bearing components. The font is open-source (MIT or SIL-OFL) and embedded via Slint's `@import` mechanism. No runtime Rust setup required for icons.

## Design philosophy

1. **Narrow APIs.** If a component has more than ~8 properties, it's two components.
2. **Environment over plumbing.** Components read shared context from globals; callers don't pass `currency`, `locale`, `theme-color` to every instance.
3. **Composition over kitchen sinks.** A button with a loading spinner is `Button` wrapped in `LoadingState`, not a `Button` with `Loading`/`LoadingText`/`LoadingColor`/`LoadingPosition` properties.
4. **No domain knowledge.** Library code references only its own primitives and the Slint standard library.
5. **No user-facing copy.** All visible text comes in as a property. The library does not own a translation table.
6. **Internal state, narrow events.** Hover, focus, press, animation, transitions all live inside. Callers receive typed callbacks (`clicked()`, `value-changed(string)`), never raw `pointer-event`s.
7. **Pre-formatted strings cross the boundary.** Slint never computes domain values. Rust formats money, dates, percentages; the lib displays and lays them out.

---

## Environment globals

The library defines these globals. The consumer populates them once at startup (typically from Rust) and the library reads from them everywhere.

| Global            | Purpose                                                                 |
| ----------------- | ----------------------------------------------------------------------- |
| `Theme`           | Semantic color tokens (primary, foreground, accent, destructive, ring) |
| `Typography`      | Font family, size scale, weight scale                                   |
| `Spacing`         | Spacing scale (`xs`, `sm`, `md`, `lg`, `xl`, `xxl`)                    |
| `Radius`          | Border-radius scale (`sm`, `md`, `lg`, `full`)                          |
| `Sizes`           | Standard heights (button, input, list-row, touch-target)                |
| `Animation`       | Duration scale (`instant`, `fast`, `normal`, `slow`) and easings        |
| `Locale`          | Current locale code, RTL boolean                                        |
| `CurrencyFormat`  | Currency code, decimal places, grouping separator, symbol position      |
| `IconFont`        | Icon font name, named-icon lookup (codepoints for common names)         |

**Stability contract:** adding fields to a global is non-breaking. Renaming or removing a field is a breaking change requiring a version bump. Globals' shapes are part of the public API surface.

**Default values:** every global has sensible defaults so the library can render in `slint-viewer` standalone without any Rust wiring. The consumer overrides what it needs.

### Shape tokens

The `Theme` global exposes three shape tokens controlling the silhouette of interactive surfaces. Changing one token at the app level restyles every screen.

| Token               | Type     | Default     | Affects                                                                |
| ------------------- | -------- | ----------- | ---------------------------------------------------------------------- |
| `button-shape`      | `string` | `"pill"`    | `Button`, `Chip`, `OptionTile`, `MoneyInput`                           |
| `card-shape`        | `string` | `"rounded"` | `Card`, `SectionCard`                                                  |
| `icon-button-shape` | `string` | `"circle"`  | `IconButton`, `BackButton`                                             |

Accepted values:

- **`rounded`** — moderate radius (`Radius.md`, ~8px). Shadcn / Linear / Vercel aesthetic.
- **`pill`** — capsule shape (`radius = height / 2`). Apple / Google / YouTube / X / Spotify / Material 3 Expressive — the 2024–25 mainstream.
- **`square`** — zero radius. Brutalist / Notion-style. Use sparingly.
- **`circle`** — only valid for `icon-button-shape`. Forces 1:1 aspect ratio plus full radius.

**Defaults are deliberate.** The library ships with `pill` + `circle` because that matches the dominant contemporary aesthetic. To switch to a shadcn/Linear look, set `Theme.button-shape = "rounded"` at app startup.

**Per-instance override.** Every shape-bearing component accepts a `shape` property. The sentinel value `"default"` (the property's default value) means "follow the theme token." Explicit values override.

### Numeric content rendering

Any number-with-unit pair — currency, measurement, percentage, code — is an **LTR atomic sub-flow** in this library, regardless of `Locale.rtl`. The pair always renders number-then-unit (or unit-then-number for leading-symbol currencies like `$`), with the unit on the physical right of the number in the standard case.

| Content         | LTR context  | RTL context (visual) |
| --------------- | ------------ | -------------------- |
| Weight          | `12 kg`      | `12 kg`              |
| Weight (Arabic) | `12 كغ`      | `12 كغ`              |
| Price (SAR)     | `12.500 SAR` | `12.500 SAR`         |
| Price (Arabic)  | `12.500 ر.س` | `12.500 ر.س`         |
| Price (USD)     | `$12.50`     | `$12.50`             |
| Dimension       | `100 cm`     | `100 cm`             |
| Dimension (Ar)  | `100 سم`     | `100 سم`             |
| Percentage      | `15%`        | `15%`                |

The pair's *position within its surrounding layout* still flips with `Locale.rtl` — a price in a row's trailing slot moves to the row's physical left in RTL. But the *contents* of the pair never flip. Within the pair, glyphs use their native script direction (Arabic letters in `كغ` render RTL among themselves; Latin letters in `kg` render LTR).

This is the standard Unicode BiDi behavior for numeric content embedded in mixed-direction text. The library enforces it inside `Money`, `Quantity`, `MoneyInput`, and any future primitive that displays a value with a unit. Consumers never set bidi controls manually.

---

## Component catalog (v1)

15 primitives. Each one ~50–200 lines of Slint source. Total expected library size ~1800–2200 lines.

Naming convention: PascalCase, single-noun where possible, no `Abdu*` or library prefix on components (the import path provides the namespace).

**Property types use Slint enums, not strings.** The tables below show enum values inline (e.g., `variant: ButtonVariant`) but the precise enum definitions, defaults, and every public property live in [IMPL.md](./IMPL.md). The catalog below is the *shape contract*; IMPL is the *precision contract*. Where a table lists 5–8 properties, the actual component will ship with 10–20 (per the construction discipline in [CLAUDE.md](./CLAUDE.md)) — the additional ones are convenience features (loading state, full-width, tooltip, etc.) standard to textbook component libraries.

**Icons** are referenced by name (e.g., `"chevron-right"`, `"trash"`, `"check"`) resolved via the bundled icon font through the `IconFont` global. Raw codepoint characters and emoji are also accepted as fallback. Final icon-font choice (Phosphor vs Lucide) lands in Phase 1.

### Button

The foundation. Variants and sizes mirror shadcn/ui.

| Property   | Type      | Default     | Description                                                   |
| ---------- | --------- | ----------- | ------------------------------------------------------------- |
| `label`    | `string`  | `""`        | Visible text. Empty for icon-only buttons.                    |
| `icon`     | `string`  | `""`        | Leading icon (emoji, glyph, or icon-font codepoint).          |
| `variant`  | `string`  | `"default"` | `default \| destructive \| outline \| secondary \| ghost`     |
| `size`     | `string`  | `"default"` | `default \| sm \| lg \| icon`                                 |
| `disabled` | `bool`    | `false`     | Visually muted, non-interactive.                              |

**Callbacks:** `clicked()`

**Reads from environment:** `Theme`, `Typography`, `Radius`, `Animation`

---

### IconButton

Square click target. Convenience over `Button { size: "icon"; }` for the common case.

| Property   | Type      | Default | Description                                       |
| ---------- | --------- | ------- | ------------------------------------------------- |
| `icon`     | `string`  | `""`    | Required. The glyph or symbol to render.          |
| `tone`     | `string`  | `"default"` | `default \| muted \| destructive`             |
| `size-px`  | `length`  | `40px`  | Side length. Touch-target compliant by default.   |
| `disabled` | `bool`    | `false` |                                                   |

**Callbacks:** `clicked()`

**Reads from environment:** `Theme`, `Radius`, `Animation`

---

### BackButton

Locale-aware navigation back control. Picks the correct arrow glyph from `Locale.rtl`.

| Property | Type   | Default | Description |
| -------- | ------ | ------- | ----------- |
| `tone`   | `string` | `"on-surface"` | `on-surface \| on-primary` — picks contrast for header background |

**Callbacks:** `clicked()`

**Reads from environment:** `Theme`, `Locale`

---

### Toggle

A switch. Single internal animation (knob slide), no caller plumbing.

| Property   | Type   | Default | Description                              |
| ---------- | ------ | ------- | ---------------------------------------- |
| `on`       | `bool` | `false` | Current state.                           |
| `disabled` | `bool` | `false` |                                          |

**Callbacks:** `toggled(bool)` — fires with the new state after the user activates the toggle.

**Reads from environment:** `Theme`, `Animation`

---

### OptionTile

Selectable tile in a radio group. The parent owns the "which is selected" state; the tile just renders based on `selected`.

| Property    | Type     | Default | Description                                           |
| ----------- | -------- | ------- | ----------------------------------------------------- |
| `selected`  | `bool`   | `false` | Whether this tile is the active choice.               |
| `label`     | `string` | `""`    | Primary text.                                         |
| `sublabel`  | `string` | `""`    | Optional secondary text below the label.              |
| `icon`      | `string` | `""`    | Optional leading icon/glyph.                          |
| `disabled`  | `bool`   | `false` |                                                       |

**Callbacks:** `chosen()`

**Reads from environment:** `Theme`, `Radius`, `Animation`, `Spacing`

---

### Chip

Compact label or count badge. Static; no interaction.

| Property | Type     | Default     | Description                                          |
| -------- | -------- | ----------- | ---------------------------------------------------- |
| `label`  | `string` | `""`        | The chip's text.                                     |
| `icon`   | `string` | `""`        | Optional leading glyph.                              |
| `tone`   | `string` | `"neutral"` | `neutral \| info \| success \| warning \| destructive` |

**Reads from environment:** `Theme`, `Typography`, `Radius`

---

### StatusPill

State-aware pill. Like `Chip` but with an optional pulse animation when state changes — communicates "this just updated."

| Property      | Type     | Default     | Description                                                |
| ------------- | -------- | ----------- | ---------------------------------------------------------- |
| `label`       | `string` | `""`        | The displayed text.                                        |
| `state`       | `string` | `"idle"`    | `idle \| active \| success \| warning \| error`           |
| `pulse`       | `bool`   | `false`     | When true, pulses gently. Use for "live" indicators.       |

**Reads from environment:** `Theme`, `Animation`, `Radius`

---

### Card

Surface container with shadow and radius. No header, no padding-on-content opinions — pure surface.

| Property      | Type     | Default     | Description                                            |
| ------------- | -------- | ----------- | ------------------------------------------------------ |
| `elevation`   | `string` | `"sm"`      | `none \| sm \| md \| lg` — drop-shadow intensity      |
| `interactive` | `bool`   | `false`     | If true, shows hover/press feedback and emits `clicked()`. |

**Callbacks:** `clicked()` (only when `interactive: true`)

**Reads from environment:** `Theme`, `Radius`, `Animation`

---

### SectionCard

Card with a built-in header (icon + title) and content slot. The dominant pattern in settings/report screens.

| Property | Type     | Default | Description                            |
| -------- | -------- | ------- | -------------------------------------- |
| `title`  | `string` | `""`    | Section heading text.                  |
| `icon`   | `string` | `""`    | Optional leading icon for the header.  |

**Slot:** child elements become the body content.

**Reads from environment:** `Theme`, `Typography`, `Spacing`, `Radius`

---

### KeyValueRow

Label on the start side, value on the end side. RTL-aware. The building block for breakdowns, totals, summaries.

| Property        | Type     | Default     | Description                                                       |
| --------------- | -------- | ----------- | ----------------------------------------------------------------- |
| `label`         | `string` | `""`        | Left side in LTR, right side in RTL.                              |
| `value`         | `string` | `""`        | The opposite side.                                                |
| `emphasis`      | `string` | `"normal"`  | `normal \| strong \| total` — weight and color treatment of value |
| `value-tone`    | `string` | `"default"` | `default \| positive \| negative \| muted`                        |

**Reads from environment:** `Theme`, `Typography`, `Locale`

---

### FormRow

Label + control + helper text + optional error. The form-screen building block.

| Property        | Type     | Default | Description                                                     |
| --------------- | -------- | ------- | --------------------------------------------------------------- |
| `label`         | `string` | `""`    | The field label, shown above the control.                       |
| `helper`        | `string` | `""`    | Optional caption below the control.                             |
| `error`         | `string` | `""`    | When non-empty, replaces `helper`, styled as error.             |
| `required`      | `bool`   | `false` | Renders a required marker on the label.                         |

**Slot:** child element becomes the control (typically `Input`, `Toggle`, or `OptionTile`).

**Reads from environment:** `Theme`, `Typography`, `Spacing`

---

### Money

Currency-aware display. Renders amount and currency as an **LTR-atomic pair** (see [Numeric content rendering](#numeric-content-rendering)) — the currency sits on the physical right of the number for trailing-symbol currencies (`SAR`, `ر.س`, `kr`, `¥` in some locales), or on the physical left for leading-symbol currencies (`$`, `£`, `€`). The pair's internal ordering does not flip with `Locale.rtl`; only the pair's position within surrounding layout does.

| Property   | Type     | Default     | Description                                                       |
| ---------- | -------- | ----------- | ----------------------------------------------------------------- |
| `amount`   | `string` | `"0"`       | Pre-formatted numeric portion (e.g. `"12.500"`, `"-450"`).        |
| `tone`     | `string` | `"default"` | `default \| positive \| negative \| muted`                        |
| `size`     | `string` | `"body"`    | `caption \| body \| heading \| display`                           |
| `flash`    | `bool`   | `true`      | Brief background highlight when `amount` changes.                 |

**Reads from environment:** `Theme`, `Typography`, `Locale`, `CurrencyFormat`, `Animation`

Currency code/symbol and its position (leading vs trailing the number) come from `CurrencyFormat`. Negative values get the destructive tone unless explicitly overridden.

---

### Quantity

Generic unit-bearing value (weight, length, capacity, frequency, percentage). Same LTR-atomic rendering rule as `Money` — the unit sits on the physical right of the number unless `unit-position` overrides.

| Property        | Type     | Default      | Description                                                              |
| --------------- | -------- | ------------ | ------------------------------------------------------------------------ |
| `value`         | `string` | `"0"`        | Pre-formatted numeric portion.                                           |
| `unit`          | `string` | `""`         | Unit symbol or word (`"kg"`, `"كغ"`, `"cm"`, `"سم"`, `"GB"`, `"%"`).      |
| `unit-position` | `string` | `"trailing"` | `trailing` (right of number) or `leading` (left of number).              |
| `tone`          | `string` | `"default"`  | `default \| positive \| negative \| muted`                               |
| `size`          | `string` | `"body"`     | `caption \| body \| heading \| display`                                  |

**Reads from environment:** `Theme`, `Typography`, `Locale`

The pair always renders LTR internally; only the unit's own glyphs follow their script's native direction. Validation and unit conversion are the caller's responsibility — `Quantity` only displays.

---

### MoneyInput

Numeric input with the current currency rendered inline. Handles decimal-place enforcement, max-digit constraints, and locale-aware alignment.

| Property        | Type     | Default | Description                                                     |
| --------------- | -------- | ------- | --------------------------------------------------------------- |
| `value`         | `string` | `""`    | Current value as a pre-formatted string.                        |
| `placeholder`   | `string` | `""`    | Shown when value is empty.                                      |
| `max-digits`    | `int`    | `9`     | Total digits including decimals; library blocks input beyond.   |
| `disabled`      | `bool`   | `false` |                                                                 |
| `error`         | `bool`   | `false` | Visual error state (red border).                                |

**Callbacks:** `value-changed(string)`, `submitted()`

**Reads from environment:** `Theme`, `Typography`, `Locale`, `CurrencyFormat`, `Radius`, `Animation`

Validation logic (range, business rules) is the caller's responsibility. The input only enforces format constraints.

---

### Input

General-purpose text input supporting multiple input kinds. The form-building workhorse.

| Property        | Type        | Default  | Description                                                              |
| --------------- | ----------- | -------- | ------------------------------------------------------------------------ |
| `value`         | `string`    | `""`     | Current text content.                                                    |
| `placeholder`   | `string`    | `""`     | Shown when value is empty.                                               |
| `kind`          | `InputKind` | `text`   | `text \| password \| search \| numeric \| email \| url \| tel \| multi-line` |
| `size`          | `InputSize` | `md`     | `sm \| md \| lg`                                                         |
| `shape`         | `Shape`     | `default`| Inherits from `Theme.button-shape` unless overridden.                    |
| `disabled`      | `bool`      | `false`  |                                                                          |
| `read-only`     | `bool`      | `false`  |                                                                          |
| `error`         | `bool`      | `false`  | Visual error state.                                                      |
| `icon-leading`  | `string`    | `""`     | Icon name shown inside the input on the leading side.                    |
| `icon-trailing` | `string`    | `""`     | Icon name on the trailing side (e.g., clear-button, search-glyph).       |
| `max-length`    | `int`       | `0`      | 0 means unlimited.                                                       |

**Additional v1 properties** (see IMPL.md): `autocomplete`, `autofocus`, `pattern` (regex hint, not enforced), `rows` (multi-line), `show-character-count`, `aria-label`.

**Callbacks:** `value-changed(string)`, `submitted()`, `focus-changed(bool)`, `icon-trailing-clicked()`

**Reads from environment:** `Theme`, `Typography`, `Locale`, `Radius`, `Animation`, `IconFont`

Validation logic (regex enforcement, range checks, business rules) is the caller's responsibility. The input only enforces visible format constraints (`max-length`, allowed character class per `kind`). Password masking, password-show-toggle, and search-clear-button are built in per `kind`.

---

## What's explicitly *not* in v1

These belong in the consumer app, not the library — they reference domain concepts:

- `CartItem`, `ProductTile`, `OperatorBadge`, `TransactionRow`, `ShiftCard` — domain components
- `StatusBar` — references app-specific state (sync count, current operator)
- `SyncIndicator` — references sync state owned by the app

The primitives the lib *does* provide (`Card`, `KeyValueRow`, `Chip`, `StatusPill`, `Money`) are what those domain components should be built from.

Also deferred to a later version:

- `Dialog` / `Modal` — focus traps and overlay management need a real design pass
- `Dropdown` / `Combobox` — popover positioning is non-trivial
- `Tabs` — multiple sub-patterns, needs scoping
- `DatePicker` — depends on date library choices, locale calendar systems
- `Slider`, `ProgressBar`, `Skeleton` — likely v1.1
- `Code`, `Timestamp`, `Percent` — additional LTR-atomic primitives for IDs, codes, time displays, and percentages; same rendering rule as `Money` and `Quantity`. Likely v1.1
- `Toast`, `Tooltip`, `Popover` — overlay subsystem in v2

---

## Using the library

> Not yet implemented. The shape below is the intended consumer experience.

### 1. Depend on the crate

```toml
# Cargo.toml (consumer)
[dependencies]
abdu-slint-ui = { path = "../abdu-slint-ui" }   # or git, or version
```

### 2. Populate the environment at startup

From Rust, once at app boot, set the globals' fields from your app config. The library never reads from disk, talks to a backend, or guesses — it only renders what the consumer has told it.

### 3. Import and compose

```slint
import { Button, FormRow, Toggle, SectionCard, Money } from "@abdu-slint-ui/lib.slint";

SectionCard {
    title: "Display";
    icon: "🖥";

    FormRow {
        label: "Sound";
        Toggle {
            on: settings.sound-enabled;
            toggled(v) => { settings.sound-enabled = v; }
        }
    }
}
```

No locale ternaries. No drop-shadow restating. No string concatenation with currencies. The screen becomes structural composition.

---

## Versioning

- **v0.x** — design and shape settling. Breaking changes expected on every release.
- **v1.0** — public API frozen. Property additions are non-breaking; renames/removals require a major bump.
- **v1.x** — additive only.
- Slint compatibility is pinned per release. The library tracks one specific Slint minor version at a time.

---

## Compatibility (intended)

- **Slint**: 1.8+
- **Rust**: 1.75+ (workspace MSRV)
- **Platforms**: any Slint-supported platform. The library does no platform-specific work.
- **Renderers**: tested against `femtovg` and `skia`. No assumed renderer features beyond drop-shadow, gradients, and animation.

---

## Open design questions

Resolved in Phase 0:

- ✅ **Crate location** — sibling directory at repo root (`abdu-slint-ui/` and `abdu-slint-ui-playground/` next to `crates/`, `src/`, `ui/`).
- ✅ **License** — MIT OR Apache-2.0 (dual). Both `LICENSE-MIT` and `LICENSE-APACHE` ship with the crate.
- ✅ **Input component** — added as primitive #15 in v1.
- ✅ **Icon system** — library bundles an icon font (Phosphor or Lucide; choice in Phase 1) loaded via Slint `@import`; components accept icon *names* resolved through the `IconFont` global, with raw codepoints/emoji as a fallback.

Still open, deferred to Phase 1 implementation:

- **Focus-ring rendering.** Slint's focus model is limited. We'll attempt keyboard-focus visuals on `Button`, `IconButton`, `Input`, `MoneyInput`, `Toggle`, and `OptionTile` in v1; other components may or may not, depending on Slint capability. Document final state in IMPL.md.
- **Compact mode.** Decision: read a global density token (`Theme.density`?) rather than per-component compact flags. Specifics — including whether `Density` is per-component overridable — settled when the first density-aware component lands.
- **Phosphor vs Lucide.** Both are excellent. Phosphor has more variants (regular, thin, light, bold, fill); Lucide is simpler and lighter (~250 KB vs ~600 KB). Final choice in Phase 1, possibly after a side-by-side font weight comparison in the playground.

---

## Exploring the library

The canonical way to explore `abdu-slint-ui` is the **playground app** — a sibling crate (`abdu-slint-ui-playground`) that renders every component live, with interactive controls for every property, theme/locale switchers in the toolbar, and a copy-pasteable code snippet for the current configuration.

Three artifacts, three jobs:

| Artifact                            | Job                                                                       |
| ----------------------------------- | ------------------------------------------------------------------------- |
| `previews/{component}.slint`        | Fast per-component iteration during development. `slint-viewer`, no Rust rebuild. |
| `abdu-slint-ui-playground`          | Cumulative interactive catalog for exploration, design review, and consumer demos. The Object Inspector equivalent. |
| `examples/`                         | Full-screen smoke tests proving the library composes into realistic screens. |

Read source for *how a component is built*. Run the playground for *how to use it*.

## Project status

| Phase                                              | State   |
| -------------------------------------------------- | ------- |
| Design contract (this doc)                         | draft   |
| Construction discipline (`CLAUDE.md`)              | draft   |
| Roadmap (`ROADMAP.md`)                             | draft   |
| Token / global design                              | pending |
| Component IMPL doc                                 | pending |
| Reference theme retune                             | pending |
| First primitive (`Button`)                         | spike done — see `ui/spike/shadcn_button.slint` |
| Playground app (`abdu-slint-ui-playground`)        | pending |
| Workspace integration                              | pending |
| Consumed by `e2manage-pos-terminal`                | pending |
