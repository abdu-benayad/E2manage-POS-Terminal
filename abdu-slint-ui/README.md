# abdu-slint-ui

Modern, environment-aware Slint UI primitives for Rust applications. Touch-first, tablet-class hardware in mind; iOS / SwiftUI–inspired design language.

> **Status — Phase 1 in progress.** Foundation shipped (10 globals, dual icon font, design-token system). Two of five Phase 1 primitives built: `Button` and `IconButton`, each with a preview and an interactive playground section. `Toggle`, `Card`, `KeyValueRow`, and a smoke-test screen still to land before Phase 1 closes. See [HANDOVER.md](./HANDOVER.md) for current state and [IMPL.md](./IMPL.md) for the implementation playbook.

---

## What this is

A library of *living* UI components for Slint apps. Each component:

- Reads its environment — theme, locale, currency format, depth physics — from globals the consumer populates once at startup
- Exposes a rich public API (interactive components ship 15–25 properties — the textbook range for design-system primitives)
- Owns its internal state (hover, focus, press, animation, state machines, accessibility-name resolution) so callers don't have to manage it
- Composes into screens with structural code, not inline `Rectangle + TouchArea + Text` rebuilds

If your screens are restating border-radius, drop-shadow, locale ternaries, currency concatenations, accessibility wiring, and toggle-knob layouts inline, you're missing this layer.

## What this is not

- **Not a fork of `std-widgets.slint`.** It does not wrap or extend Slint's stock components.
- **Not coupled to any domain.** Zero references to `Cart`, `Product`, `Operator`, `Transaction`, or any app type. The library could be dropped into a logistics kiosk, a clinic intake form, or a warehouse scanner unchanged.
- **Not a translation library.** Components take pre-localized strings as input. The library provides RTL-aware layout, never copy.
- **Not theme-agnostic.** One opinionated theme — iOS / SwiftUI–derived: systemBlue primary, system color palette, soft drop shadows tuned for tablet viewing distance, Apple-HIG-compliant tap targets (44px minimum). Consumers customize via token overrides, not by forking components.
- **Not yet stable.** Until v1, expect API churn. Consumers should pin a specific commit.

## License & distribution

Licensed under **MIT OR Apache-2.0** (Rust ecosystem convention). Consumers may choose either license. Two `LICENSE-*` files ship in the crate root.

The library bundles **both** Phosphor and Lucide icon fonts (each open-source under MIT / SIL-OFL), runtime-switchable via `IconFont.family`. Components accept a canonical icon name; the global resolves to the codepoint for the active family. No runtime Rust setup required for icons.

## Design philosophy

1. **Rich, curated APIs.** Interactive primitives ship 15–25 properties — the textbook range for design-system components (Material UI, Mantine, Ant). Discoverability and call-site ergonomics matter for a library; the narrow-API rule belongs in *application* code where each component is tied to one use site.
2. **Environment over plumbing.** Components read shared context — theme, typography, depth physics, locale, currency, animation pacing — from globals; callers don't pass `currency`, `locale`, `theme-color`, or shadow-elevation to every instance.
3. **Common patterns are first-class properties, not wrappers.** A button with a loading spinner is `Button { loading: true }`, not `LoadingWrapper { Button {…} }`. Built-in conveniences for any interactive primitive: `loading`, `icon-leading` / `icon-trailing`, `disabled`, `tooltip`, `checkable`, `aria-label`. Composition still applies for uncommon combinations.
4. **No domain knowledge.** Library code references only its own primitives and the Slint standard library.
5. **No user-facing copy.** All visible text comes in as a property. The library does not own a translation table.
6. **Internal state, narrow events.** Hover, focus, press, animation, transitions, accessibility-name resolution all live inside. Callers receive typed callbacks (`clicked()`, `value-changed(string)`), never raw `pointer-event`s.
7. **Pre-formatted strings cross the boundary.** Slint never computes domain values. Rust formats money, dates, percentages; the lib displays and lays them out.
8. **Accessibility is the component's job, not the caller's.** Every interactive primitive wires Slint's `accessible-*` properties to the platform AT tree. A name cascade (`aria-label → tooltip → label/icon → fallback`) ensures consumers who forget the explicit `aria-label` still get a usable screen-reader name.

---

## Environment globals

The library defines these globals. The consumer populates them once at startup (typically from Rust) and the library reads from them everywhere.

| Global            | Purpose                                                                 |
| ----------------- | ----------------------------------------------------------------------- |
| `Theme`           | Semantic color tokens (primary, foreground, accent, destructive, ring, semantic palettes), shape tokens, shadow scale |
| `Typography`      | Font family (incl. Arabic fallback), size scale, weight scale           |
| `Spacing`         | Spacing scale (`xs` through `4xl`)                                      |
| `Radius`          | Border-radius scale (`sm`, `md`, `lg`, `xl`, `full`)                    |
| `Sizes`           | Standard heights (button range `xs`..`hero`, input, icon, focus-ring)   |
| `Animation`       | Duration scale (`instant`, `fast`, `normal`, `slow`, `slower`) + `spinner-period` (revolution duration for indeterminate loading) |
| `Depth`           | Stateless shadow math: `bumped(level, hovered)`, `blur(level)`, `magnitude(level)`, `offset-x/y(direction, mag)`, `color-of(level, override)`. Components read it from their visual layer |
| `Locale`          | Current locale code, RTL boolean, directional glyph helpers             |
| `CurrencyFormat`  | Currency code, decimal places, grouping separator, symbol position      |
| `IconFont`        | Dual-family icon resolution (Phosphor or Lucide), runtime-switchable    |

**Stability contract:** adding fields to a global is non-breaking. Renaming or removing a field is a breaking change requiring a version bump. Globals' shapes are part of the public API surface.

**Default values:** every global has sensible defaults so the library can render in `slint-viewer` standalone without any Rust wiring. The consumer overrides what it needs.

### Shape tokens

The `Theme` global exposes three shape tokens controlling the silhouette of interactive surfaces. Changing one token at the app level restyles every screen.

| Token               | Type     | Default     | Affects                                                                |
| ------------------- | -------- | ----------- | ---------------------------------------------------------------------- |
| `button-shape`      | `string` | `"rounded"` | `Button`, `Chip`, `OptionTile`, `MoneyInput`                           |
| `card-shape`        | `string` | `"rounded"` | `Card`, `SectionCard`                                                  |
| `icon-button-shape` | `string` | `"circle"`  | `IconButton`, `BackButton`                                             |

Accepted values:

- **`rounded`** — moderate radius (`Radius.md`, ~10px). SwiftUI-default rounded rectangle, the iOS / Apple HIG mainstream for primary actions.
- **`pill`** — capsule shape (`radius = height / 2`). Common for navigation chips, secondary actions, and contexts where extra softness is wanted.
- **`square`** — zero radius. Brutalist / Notion-style. Use sparingly.
- **`circle`** — only valid for `icon-button-shape`. Forces 1:1 aspect ratio plus full radius.

**Defaults are deliberate.** The library ships with `rounded` + `circle` to match SwiftUI's primary-action convention on touch hardware. To switch to a capsule look (mid-2020s web aesthetic), set `Theme.button-shape = "pill"` at app startup. To switch to a flat/brutalist look, `"square"`.

**Per-instance override.** Every shape-bearing component accepts a `shape` property. The sentinel value `Shape.default` means "follow the theme token." Explicit values override.

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

15 primitives. Each one ~50–340 lines of Slint source.

Naming convention: PascalCase, single-noun where possible, no `Abdu*` or library prefix on components (the import path provides the namespace).

**Property types use Slint enums, not strings.** The tables below highlight the most-used properties of each component; the *complete* property surface (with all defaults, depth/lighting properties, debug knobs, accessibility wiring) lives in [HANDOVER.md](./HANDOVER.md) for shipped components and [IMPL.md](./IMPL.md) for pending ones. The catalog below is the *shape contract*; HANDOVER + IMPL together are the *precision contract*.

**Icons** are referenced by name (e.g., `"chevron-right"`, `"trash"`, `"check"`) resolved via `IconFont`. Raw codepoint characters and emoji are accepted as fallback. Both Phosphor and Lucide ship bundled, runtime-switchable via `IconFont.family`.

**Depth / lighting.** Every elevated component (Button, IconButton, eventually Card/Toggle) exposes a six-property depth API — `elevated`, `shadow-elevation`, `shadow-color`, `shadow-direction`, `thickness`, `press-animation` — backed by the stateless `Depth` global for shadow math. Tunable per instance; defaults read from the Theme.

**Accessibility.** Every interactive component wires Slint's `accessible-*` properties. `aria-label` is the explicit caller intent; an internal cascade (`aria-label → tooltip → label/icon → fallback`) ensures the platform AT tree always gets a usable name. Activation, checked state, and disabled state propagate automatically.

### Button — shipped

Foundation interactive primitive. Six variants, eight sizes (xs..hero plus icon-square), the full depth/lighting set, accessibility cascade, optional tooltip, optional loading spinner, optional `checkable` toggle behaviour. **25 public properties total** — see [HANDOVER.md → "Button API (as built)"](./HANDOVER.md) for the complete property table and [`architecture/button.md`](./architecture/button.md) for the design rationale (depth/lighting decisions, variant × tone composition, internal visual structure, Slint trapdoors).

| Property        | Type             | Default            | Description                                                                                  |
| --------------- | ---------------- | ------------------ | -------------------------------------------------------------------------------------------- |
| `label`         | `string`         | `""`               | Visible text. Empty renders an icon-only button (prefer `IconButton` for that case).         |
| `icon-leading`  | `string`         | `""`               | Icon name on reading-start side. RTL-aware.                                                  |
| `icon-trailing` | `string`         | `""`               | Icon name on reading-end side.                                                               |
| `variant`       | `ButtonVariant`  | `default`          | `default \| destructive \| outline \| secondary \| ghost \| link`                            |
| `size`          | `ButtonSize`     | `md`               | `xs \| sm \| md \| lg \| xl \| xxl \| hero \| icon` (32 / 38 / 44 / 52 / 60 / 72 / 88 / 44px) |
| `shape`         | `Shape`          | `default`          | Follows `Theme.button-shape` unless overridden.                                              |
| `tone`          | `Tone`           | `default`          | `default \| primary \| success \| warning \| destructive \| info \| muted` — overrides the variant's color family. |
| `loading`       | `bool`           | `false`            | Replaces content with a rotating spinner; blocks click. Period read from `Animation.spinner-period`. |
| `disabled`      | `bool`           | `false`            | Visually muted; blocks click and keyboard activation.                                        |
| `full-width`    | `bool`           | `false`            | Stretches to parent's available width.                                                       |
| `checkable`     | `bool`           | `false`            | When true, the button behaves as a toggle and `checked` drives the pressed visual.           |
| `checked`       | `bool` (in-out)  | `false`            | Controlled checked state.                                                                    |
| `tooltip`       | `string`         | `""`               | Hover text. Also feeds the accessibility cascade if `aria-label` is empty.                   |
| `aria-label`    | `string`         | `""`               | Required when `label` is empty; primary accessibility name.                                  |

Plus the depth set (`elevated`, `shadow-elevation`, `shadow-color`, `shadow-direction`, `thickness`, `press-animation`), the escape hatches (`bg-color`, `height-override`, `min-content-width`), and `debug-bounds`. See HANDOVER for full details.

**Callbacks:** `clicked()`, `pressed-changed(bool)`, `hover-changed(bool)`, `focus-changed(bool)`

**Reads from environment:** `Theme`, `Typography`, `Sizes`, `Radius`, `Spacing`, `Animation`, `Locale`, `IconFont`, `Depth`

---

### IconButton — shipped

Square, icon-only sibling of Button. Same six variants (default is `ghost`), distinct size enum (`IconButtonSize.xs..xxl`, no `hero`), default shape resolves against `Theme.icon-button-shape` (`"circle"`). **19 public properties total** — see [HANDOVER.md → "IconButton API (as built)"](./HANDOVER.md) and [`architecture/icon-button.md`](./architecture/icon-button.md) for the design contract.

| Property      | Type              | Default   | Description                                                                                   |
| ------------- | ----------------- | --------- | --------------------------------------------------------------------------------------------- |
| `icon`        | `string`          | `""`      | The glyph to render. Library-canonical name or raw codepoint / emoji.                         |
| `aria-label`  | `string`          | `""`      | Accessibility name. Cascade: `aria-label → tooltip → icon → "Button"`.                        |
| `variant`     | `ButtonVariant`   | `ghost`   | Different default from Button. Same six values.                                               |
| `size`        | `IconButtonSize`  | `md`      | `xs \| sm \| md \| lg \| xl \| xxl` (32 / 38 / 44 / 52 / 60 / 72 px square)                   |
| `shape`       | `Shape`           | `default` | Follows `Theme.icon-button-shape`. On a square button, `pill` and `circle` look identical.    |
| `tone`        | `Tone`            | `default` | Same enum as Button.                                                                          |
| `loading`     | `bool`            | `false`   | Replaces icon with a rotating spinner; period read from `Animation.spinner-period`.           |
| `disabled`    | `bool`            | `false`   |                                                                                               |
| `tooltip`     | `string`          | `""`      | Critical for icon-only — primary discoverability. Also feeds the accessibility cascade.       |
| `checkable`   | `bool`            | `false`   | Favorite / pin / mute / star use cases.                                                       |
| `checked`     | `bool` (in-out)   | `false`   | Controlled checked state.                                                                     |

Plus the full depth set (same six properties as Button), `height-override`, and `debug-bounds`.

**Excluded vs Button** (intentional API divergence): `label`, `icon-leading`, `icon-trailing` (consolidated to `icon`); `full-width` (always square); `min-content-width` (size dictates width); `bg-color` (curated palette wins — use `tone` or `variant`).

**Callbacks:** `clicked()`, `pressed-changed(bool)`, `hover-changed(bool)`, `focus-changed(bool)`

**Reads from environment:** `Theme`, `Typography`, `Sizes`, `Radius`, `Animation`, `IconFont`, `Depth`

---

### BackButton — pending (Phase 2)

Locale-aware navigation back control. Picks the correct chevron glyph from `Locale.rtl`. Will compose `IconButton` internally rather than re-implementing the depth + accessibility wiring.

| Property | Type           | Default     | Description |
| -------- | -------------- | ----------- | ----------- |
| `tone`   | `TonalSurface` | `on-surface` | `on-surface \| on-primary \| on-dark \| on-light` — picks contrast for the header background |

**Callbacks:** `clicked()`

**Reads from environment:** `Theme`, `Locale`, `IconFont`

---

### Toggle — pending (next)

iOS-style switch. Single internal state machine: knob slide animation, track color transition between off and on, optional knob icon. Three sizes (`ToggleSize.sm | md | lg`). Will adopt the accessibility cascade with `accessible-role: switch`.

| Property      | Type         | Default | Description                                                  |
| ------------- | ------------ | ------- | ------------------------------------------------------------ |
| `on`          | `bool`       | `false` | Current state (controlled).                                  |
| `size`        | `ToggleSize` | `md`    | `sm \| md \| lg`                                             |
| `label`       | `string`     | `""`    | Optional label rendered next to the toggle.                  |
| `description` | `string`     | `""`    | Optional caption below the label.                            |
| `on-icon`     | `string`     | `""`    | Optional icon inside the knob when on (e.g. `"check"`).      |
| `off-icon`    | `string`     | `""`    | Optional icon inside the knob when off.                      |
| `disabled`    | `bool`       | `false` |                                                              |
| `aria-label`  | `string`     | `""`    | Required when `label` is empty.                              |

**Callbacks:** `toggled(bool)`, `focus-changed(bool)`

**Reads from environment:** `Theme`, `Typography`, `Sizes`, `Radius`, `Spacing`, `Animation`, `Locale`, `IconFont`, `Depth`

---

### OptionTile — pending (Phase 2)

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

### Chip — pending (Phase 2)

Compact label or count badge. Static; no interaction.

| Property | Type     | Default     | Description                                          |
| -------- | -------- | ----------- | ---------------------------------------------------- |
| `label`  | `string` | `""`        | The chip's text.                                     |
| `icon`   | `string` | `""`        | Optional leading glyph.                              |
| `tone`   | `string` | `"neutral"` | `neutral \| info \| success \| warning \| destructive` |

**Reads from environment:** `Theme`, `Typography`, `Radius`

---

### StatusPill — pending (Phase 2)

State-aware pill. Like `Chip` but with an optional pulse animation when state changes — communicates "this just updated."

| Property      | Type     | Default     | Description                                                |
| ------------- | -------- | ----------- | ---------------------------------------------------------- |
| `label`       | `string` | `""`        | The displayed text.                                        |
| `state`       | `string` | `"idle"`    | `idle \| active \| success \| warning \| error`           |
| `pulse`       | `bool`   | `false`     | When true, pulses gently. Use for "live" indicators.       |

**Reads from environment:** `Theme`, `Animation`, `Radius`

---

### Card — pending (Phase 1)

Surface container with shadow and radius. No header, no padding-on-content opinions — pure surface.

| Property      | Type     | Default     | Description                                            |
| ------------- | -------- | ----------- | ------------------------------------------------------ |
| `elevation`   | `string` | `"sm"`      | `none \| sm \| md \| lg` — drop-shadow intensity      |
| `interactive` | `bool`   | `false`     | If true, shows hover/press feedback and emits `clicked()`. |

**Callbacks:** `clicked()` (only when `interactive: true`)

**Reads from environment:** `Theme`, `Radius`, `Animation`

---

### SectionCard — pending (Phase 2)

Card with a built-in header (icon + title) and content slot. The dominant pattern in settings/report screens.

| Property | Type     | Default | Description                            |
| -------- | -------- | ------- | -------------------------------------- |
| `title`  | `string` | `""`    | Section heading text.                  |
| `icon`   | `string` | `""`    | Optional leading icon for the header.  |

**Slot:** child elements become the body content.

**Reads from environment:** `Theme`, `Typography`, `Spacing`, `Radius`

---

### KeyValueRow — pending (Phase 1)

Label on the start side, value on the end side. RTL-aware. The building block for breakdowns, totals, summaries.

| Property        | Type     | Default     | Description                                                       |
| --------------- | -------- | ----------- | ----------------------------------------------------------------- |
| `label`         | `string` | `""`        | Left side in LTR, right side in RTL.                              |
| `value`         | `string` | `""`        | The opposite side.                                                |
| `emphasis`      | `string` | `"normal"`  | `normal \| strong \| total` — weight and color treatment of value |
| `value-tone`    | `string` | `"default"` | `default \| positive \| negative \| muted`                        |

**Reads from environment:** `Theme`, `Typography`, `Locale`

---

### FormRow — pending (Phase 2)

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

### Money — pending (Phase 2)

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

### Quantity — pending (Phase 2)

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

### MoneyInput — pending (Phase 2)

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

### Input — pending (Phase 2)

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

> Shape below is the intended consumer experience. The library is mid–Phase 1 and not yet integrated into a real consumer app; the import path and global-population pattern are settled.

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
- ✅ **Icon system** — both Phosphor and Lucide bundled, runtime-switchable via `IconFont.family`. Components accept canonical icon names, resolved through `IconFont`; raw codepoints/emoji accepted as fallback.

Resolved during Phase 1:

- ✅ **Phosphor vs Lucide** — ship both; let consumers pick at runtime via `IconFont.family`.
- ✅ **Focus-ring rendering** — implemented on Button and IconButton via a dedicated focus-ring Rectangle drawn outside the surface, driven by an internal `FocusScope`. Pattern propagates to remaining interactive primitives.
- ✅ **Depth physics architecture** — extracted as the stateless `Depth` global (seven pure functions). Components declare their own six depth-input properties; the global owns the resolution math. Decouples shadow tuning from any single component.
- ✅ **Accessibility wiring** — every interactive component sets Slint's `accessible-role / label / checkable / checked / enabled / action-default`, with a name cascade so `aria-label` omissions still produce a usable screen-reader name. Pattern propagates to remaining interactive primitives.
- ✅ **Loading spinner** — rotating glyph driven by `animation-tick()`, with rotation period read from the global `Animation.spinner-period`. Tunable per app at startup.

Still open, deferred to later Phase 1 work or Phase 2:

- **Compact mode.** `Theme.density` global exists with `"compact" | "default" | "comfortable"`. Specifics — including whether `Density` is per-component overridable — settled when the first density-aware component lands.
- **Variant resolution global.** Button and IconButton currently duplicate `variant-base-bg / variant-hover-bg / variant-foreground / tone-color / tone-foreground / base-bg / hover-bg / foreground-color / resolved-border-color`. After Card lands (third consumer), revisit whether a `Variant` global parallel to `Depth` cleanly factors the resolution math.
- **Spinner visual identity.** Currently uses the icon-font `loader` codepoint with rotation. A Phase-2 task could replace it with a `SpinnerBase`-style Path-based arc spinner so the loading visual is identical across icon-family swaps.

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

| Phase                                              | State                                              |
| -------------------------------------------------- | -------------------------------------------------- |
| Design contract (this doc)                         | done (Phase 1 revision applied)                    |
| Construction discipline (`CLAUDE.md`)              | done                                               |
| Roadmap (`ROADMAP.md`)                             | done                                               |
| Implementation playbook (`IMPL.md`)                | done for Phase 1; later phases sketched            |
| Per-component design docs (`architecture/`)        | `button.md` + `icon-button.md`. Policy: non-trivial components get one |
| Tokens + globals                                   | done (10 globals: `Theme`, `Typography`, `Spacing`, `Radius`, `Sizes`, `Animation`, `Depth`, `Locale`, `CurrencyFormat`, `IconFont`) |
| Design language                                    | iOS / SwiftUI-derived; documented in HANDOVER      |
| `Button`                                           | shipped (25 properties, full depth + accessibility) |
| `IconButton`                                       | shipped (19 properties)                            |
| `Toggle`                                           | pending — next Phase 1 component                   |
| `Card`                                             | pending                                            |
| `KeyValueRow`                                      | pending                                            |
| Phase 1 smoke test (`examples/settings-display.slint`) | pending                                        |
| Playground app (`abdu-slint-ui-playground`)        | shipped (sidebar + scrollable toolbar + Button & IconButton sections + spinner-period live tuning) |
| Phase 2 primitives (10 more)                       | pending                                            |
| Workspace integration into `e2manage-pos-terminal`  | pending (Phase 4)                                 |
