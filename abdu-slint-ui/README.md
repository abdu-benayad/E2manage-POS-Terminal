# abdu-slint-ui

Modern, environment-aware Slint UI primitives for Rust applications. Touch-first, tablet-class hardware in mind; iOS / SwiftUI–inspired design language.

> **Status — Phase 1 complete.** Foundation shipped (10 globals, dual icon font, design-token system). All five Phase 1 primitives built: `Button`, `IconButton`, `Toggle`, `Card`, and `KeyValueRow`, each with a preview and an interactive playground section. A smoke-test screen (`examples/settings-display.slint`) is the remaining Phase-1 deliverable. See [HANDOVER.md](./HANDOVER.md) for current state and [IMPL.md](./IMPL.md) for the implementation playbook.

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
| `Theme`           | Mode-aware color tokens (light + dark palettes selected by `Theme.mode`), shape tokens, shadow scale, mode-asymmetric tint helpers (`hover-tint`, `press-tint`, `skirt-tint`) |
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

### Dark mode

The library ships parallel light and dark color palettes calibrated to iOS system colors. Switch modes by setting `Theme.mode`:

```rust
let theme = window.global::<Theme>();
theme.set_mode(ThemeMode::Dark);
```

Every component re-renders automatically. Slint's reactive property bindings handle the transition — there are zero per-component code paths that branch on mode. Component code reads tokens like `Theme.surface` and `Theme.foreground`; the active value is derived from `Theme.mode` inside the global.

**System theme detection is the consumer's responsibility.** The library has no platform code. A Rust crate like `dark-light` detects the OS preference; the consumer calls `set_mode` accordingly. This stays out of the library because POS use cases (kiosk-style hardware with manual operator override, time-of-day forced themes for outdoor terminals) often want detection logic that isn't "follow the OS."

**Mode-asymmetric color shifts are centralized in `Theme` helpers.** Components that need a hover or press tint on a surface call `Theme.hover-tint(c)` / `Theme.press-tint(c)` / `Theme.skirt-tint(c)`. The helpers darken in light mode and brighten in dark mode (subject to the rule that *saturated* tones — primary, destructive, success — darken on press in both modes, the way iOS dark mode actually behaves). Components never inline `.darker()` for press feedback on neutral surfaces; that's a footgun in dark mode and the helpers exist to make the right pattern the path of least resistance.

**Tooltips intentionally invert.** A near-black tooltip on a light page in light mode becomes a near-white tooltip on a dark page in dark mode. The tooltip is meant to *pop against* the surrounding UI, not match it.

**Card's `bordered: true` default becomes load-bearing in dark mode.** Dark-mode shadows have boosted opacity (60–90% vs 12–35% in light) to remain visible against near-black surfaces, but even at 90% they don't fully define elevation. The 1px border carries the rest of the separation. The default was already safety-first; dark mode validates it.

**Brand customization requires overriding both palettes.** Setting `Theme.primary` at startup overrides only the *active* derived value; if the consumer wants brand colors that work in both modes, they must also set `Theme.dark-primary`, `Theme.primary-hover`, and `Theme.dark-primary-hover`. The library does not auto-derive hover or pressed shades from a single base color — the hand-tuned light/dark pairs matter visually. Treat the brand palette as a complete bundle of overrides, not a single token.

### Internationalization: single-script segmentation

This library is the first Slint component library with first-class RTL and Arabic support. The design contract that makes this work is **single-script segmentation**: no library component renders mixed-script content inside a single `Text` element. Multi-content components split their content into separately-anchored `Text` elements, each containing one script direction. Layout positions the segments according to `Locale.rtl`; the *contents* of each segment stay in their native direction.

This is not a stylistic choice. Slint's bidirectional text support is incomplete in v1.14 — issue [#2294](https://github.com/slint-ui/slint/issues/2294) (RTL layouts, open RFC since 2023) and [#7267](https://github.com/slint-ui/slint/issues/7267) (Persian text bugs in LineEdit, open) both mean that any `Text` or `TextInput` holding bidi-mixed content has rendering and selection bugs. Segmentation sidesteps every Slint bidi bug **by construction**: if our API forces consumers to split before they hand us strings, we never feed Slint bidi content.

#### Where this applies (component-by-component)

| Component | Segmentation pattern |
|---|---|
| **Button** | `label` (one script) + `icon-leading` / `icon-trailing` (codepoints, scriptless). RTL flips the icon's physical side; the label is single-script per consumer. |
| **IconButton** | `icon` (codepoint). No mixed text by construction. |
| **Toggle** | `label` (one script) + `description` (one script, separate `Text`). Knob position is geometric, locale-independent. |
| **Card** | No text of its own. Children inside are the consumer's responsibility — they have segmented primitives to compose with. |
| **KeyValueRow** | Every textual property is its own cell — `label`, `description`, `value`, `value-unit` (and icon glyphs for `label-icon`, `value-icon`, `status`, `disclosure`). The dominant POS case (Arabic label + Latin numeric value) renders as two `Text` elements, never one bidi-mixed string. Layout flips per `Locale.rtl`; the cluster's internal order is governed by `unit-position` XNOR `Locale.rtl`. |
| **Money** (Phase 2) | `amount: string` (LTR-atomic numeric) + currency symbol/code (its own native direction). The pair renders as **two `Text` elements**, not one. `"12.50 ر.س"` is `Text { "12.50" }` next to `Text { "ر.س" }`. |
| **Quantity** (Phase 2) | Same pattern as Money — `value` Text + `unit` Text, separately. `100 سم` is two segments, never `"100سم"` in one Text. |
| **SectionCard** (Phase 2) | `title: string` (one script) + `icon` (codepoint) + content slot (consumer-composed). |
| **FormRow** (Phase 2) | `label` (one script) + `helper` / `error` (one script). Control sits in its own slot. |

#### LTR-atomic numeric content (a specific case of the principle)

For numeric content (currency, measurement, percentage, code, timestamp), segmentation also enforces an **LTR-atomic ordering rule**: the pair renders number-then-unit (or unit-then-number for leading-symbol currencies like `$`) regardless of `Locale.rtl`. The pair's *position* in surrounding layout respects locale; the pair's *internal order* does not.

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

Within the pair, glyphs use their native script direction (Arabic letters in `كغ` render RTL among themselves; Latin letters in `kg` render LTR). Because each script lives in its own `Text` element, Slint's BiDi algorithm only sees single-direction content per segment — no bugs trigger.

#### Where segmentation breaks: free text input

The principle holds for everything except inputs accepting arbitrary user text. If a cashier types `"إجمالي 12.50"` into a free-form `Input { kind: text }`, that single string lives in one `TextInput`'s buffer — Slint's bidi rendering kicks in, and the bugs from #7267 apply (cursor position anomalies, word reordering on selection).

Mitigations in the Phase 2 input components:

- **`MoneyInput`** restricts to numeric-only characters → LTR by construction; bidi bugs cannot trigger.
- **`Input`** ships a `text-direction: TextDirection { auto, ltr, rtl }` property. Default `auto` follows `Locale.rtl` and accepts only the dominant direction. Consumers needing free bidi input opt in explicitly with a documented warning.
- Text fields displaying *pre-formatted* strings (read-only labels, captions) are always safe because the formatting happened on the Rust side; the `Text` element receives single-direction content.

#### Consumer guidance

- **Pre-localize content at the boundary.** Format strings in Rust before handing them to the library. Don't construct `"price: " + amount + " " + currency` in one string and pass it as a label.
- **Use segmented components for label+value patterns.** Reach for `KeyValueRow`, not a Button with a long bilingual label.
- **For numeric+unit content, use `Money` / `Quantity`** — they enforce the LTR-atomic rule internally so you don't have to.
- **Treat free text input as opt-in bidi.** Set `text-direction` explicitly when you mean it; default to single-direction.

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

### Toggle — shipped

iOS-style pill switch. Knob carries depth (track is flat with a subtle inner-edge border — a track shadow would weld the pill to its substrate). Slide animation on the x-axis composes orthogonally with the press-dip on the knob's face y-axis. Three sizes (`ToggleSize.sm | md | lg`, with `md` at the iOS-canonical 51×31). First library component with `accessible-role: switch`. **19 public properties total** — see [HANDOVER.md → "Toggle API (as built)"](./HANDOVER.md) and [`architecture/toggle.md`](./architecture/toggle.md) for the design contract.

| Property      | Type            | Default   | Description                                                                                  |
| ------------- | --------------- | --------- | -------------------------------------------------------------------------------------------- |
| `on`          | `bool` (in-out) | `false`   | Controlled state. Toggle flips this on user activation and then fires `toggled(on)`.         |
| `size`        | `ToggleSize`    | `md`      | `sm \| md \| lg` (44×26 / 51×31 / 60×36; knob 22 / 27 / 32 px)                               |
| `label`       | `string`        | `""`      | Optional primary text beside the switch. RTL-aware column position.                          |
| `description` | `string`        | `""`      | Optional caption below `label`. **Not** part of the accessibility cascade (captions aren't names). |
| `tone`        | `Tone`          | `default` | `default` → `Theme.success` (iOS green). Same enum as Button/IconButton for on-track color.  |
| `on-icon`     | `string`        | `""`      | Rendered inside the knob when on. Cross-fades with `off-icon` over the slide duration.       |
| `off-icon`    | `string`        | `""`      | Inside the knob when off.                                                                    |
| `loading`     | `bool`          | `false`   | Knob glyph swaps to rotating loader; blocks toggle. `on` is preserved through loading.       |
| `disabled`    | `bool`          | `false`   |                                                                                              |
| `tooltip`     | `string`        | `""`      | Hover discoverability + feeds the accessibility cascade as fallback.                         |
| `aria-label`  | `string`        | `""`      | Explicit a11y name. Cascade: `aria-label → tooltip → label → "Toggle"`.                      |

Plus the depth set on the knob (`elevated`, `shadow-elevation`, `shadow-color`, `shadow-direction`, `thickness`, `press-animation`), `height-override` (preserves the iOS 51/31 aspect ratio), and `debug-bounds`.

**Excluded vs Button/IconButton** (intentional API divergence): `variant` (one canonical iOS visual); `shape` (track is pill-locked); `bg-color` / `track-color-on` / `track-color-off` (curated `tone` wins); `checkable` / `checked` (Toggle is by definition checkable — `on` IS the checked state).

**Callbacks:** `toggled(bool)`, `pressed-changed(bool)`, `hover-changed(bool)`, `focus-changed(bool)`

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

### Card — shipped

Structural surface container with **opt-in interactivity**. Pure surface unless `interactive: true`; no `variant`, no `tone` (color-coding a card to imply semantics is inaccessible — semantic content belongs inside, not painted onto the surface). When interactive, adopts the accessibility cascade via a conditional inner shim (Slint requires `accessible-role` to be compile-time constant, so the role lives on a conditional inner Rectangle when `interactive: true`). **16 public properties total** — see [HANDOVER.md → "Card API (as built)"](./HANDOVER.md) and [`architecture/card.md`](./architecture/card.md).

| Property            | Type       | Default   | Description                                                                                  |
| ------------------- | ---------- | --------- | -------------------------------------------------------------------------------------------- |
| `interactive`       | `bool`     | `false`   | Opt-in interactivity. When true: hover/press feedback, focus ring, `clicked()`, AT role, tooltip, keyboard activation. When false: pure surface, **not in keyboard tab order**. |
| `disabled`          | `bool`     | `false`   | Only effective when `interactive: true`.                                                     |
| `shape`             | `Shape`    | `default` | Follows `Theme.card-shape`. `rounded → Radius.lg` (14px, larger than buttons). `pill` and `circle` fall back to `rounded`. |
| `bordered`          | `bool`     | `true`    | 1px `Theme.border`. Safe default — guarantees visibility even with `elevated: false`.        |
| `padding-density`   | `Density`  | `default` | `compact / default / comfortable`. Renamed from `padding` (Slint reserves the bare name).    |
| `padding-override`  | `length`   | `0px`     | **Dual-sentinel.** `0px` = use preset. `0.001px` = explicit zero (full-bleed images). Any other positive value forces that padding. |
| `max-content-width` | `length`   | `0px`     | `0px` = no cap. Renamed from `max-width` (Rectangle's reserved property).                    |
| `aria-label`        | `string`   | `""`      | Required when `interactive`. Cascade: `aria-label → tooltip → "Card"`.                       |
| `tooltip`           | `string`   | `""`      | Only rendered when `interactive: true`.                                                      |

Plus the full depth set (`elevated`, `shadow-elevation`, `shadow-color`, `shadow-direction`) and `debug-bounds`. **`thickness` and `press-animation` are accepted but inert in v1** — they're API parity placeholders; Card's content-driven sizing creates a circular dependency with Button's two-layer surface/face structure. Phase 1.1 may revisit.

**Press feedback (v1):** background tint darkens by 4% + opacity dims to 0.96 + shadow returns to rest (effective-hover gated on `!touch.pressed`). No y-shift.

**Children:** wrapped in a `VerticalLayout` with `resolved-padding`. Consumers needing horizontal arrangement nest their own `HorizontalLayout` inside.

**Excluded vs Button/IconButton** (intentional API divergence): `variant`, `tone` (Card is structural, not semantic), `bg-color`, `loading`, `checkable`/`checked`, `label`/`title` (that's SectionCard in Phase 2), `icon-leading`/`icon-trailing`.

**Callbacks:** `clicked()`, `pressed-changed(bool)`, `hover-changed(bool)`, `focus-changed(bool)` — all fire only when `interactive: true`.

**Reads from environment:** `Theme`, `Radius`, `Spacing`, `Sizes`, `Animation`, `Depth`, `Typography` (tooltip only)

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

### KeyValueRow — shipped (Phase 1)

Label on the start side, value on the end side. RTL-aware. The building block for breakdowns, totals, summary rows, settings lists. **The canonical incarnation of the [single-script segmentation principle](#internationalization-single-script-segmentation):** every textual property is its own cell (`label`, `description`, `value`, `value-unit`), so the dominant POS case — Arabic label + Latin numeric value — renders as separate `Text` elements that never mix scripts in one bidi run.

Composed on the segment-as-cell pattern's foundation primitives (`Segment` plus the bare slack `Rectangle`). The row's body is a primary `HorizontalLayout` holding every single-line cell plus a conditional description row below, indented to align under the label. Design rationale in [`architecture/key-value-row.md`](architecture/key-value-row.md); pattern reference in [`architecture/segment-pattern.md`](architecture/segment-pattern.md).

#### Content (6)

| Property      | Type     | Default | Description |
| ------------- | -------- | ------- | ----------- |
| `label`       | `string` | `""`    | Leading-side primary text. Single-script content. |
| `label-icon`  | `string` | `""`    | Optional `IconFont` name (e.g. `"receipt"`). Position flips with `Locale.rtl`. |
| `description` | `string` | `""`    | Optional secondary text rendered below `label`, indented to align under it. When empty, the row collapses to single-line. |
| `value`       | `string` | `""`    | Trailing-side primary text. Single-script content. |
| `value-unit`  | `string` | `""`    | Optional unit/suffix adjacent to `value` (`"SAR"`, `"%"`, `"kg"`). Reading position controlled by `unit-position`. |
| `value-icon`  | `string` | `""`    | Optional `IconFont` name rendered in the value cluster (trend arrows, status glyphs). |

#### Typography & tone (3)

| Property          | Type       | Default     | Description |
| ----------------- | ---------- | ----------- | ----------- |
| `emphasis`        | `Emphasis` | `normal`    | `subtle \| normal \| strong \| total` — drives label/value/unit font-size and weight. |
| `value-tone`      | `Tone`     | `default`   | Colors the value cluster (value text + unit + icon). Label is never toned. |
| `value-monospace` | `bool`     | `false`     | When `true`, the value cell uses `Typography.font-family-monospace` for tabular-figures alignment across stacked numeric rows. |

#### Affordances (3)

| Property      | Type                  | Default      | Description |
| ------------- | --------------------- | ------------ | ----------- |
| `show-status` | `bool`                | `false`      | Renders a colored `●` dot at the row's leading edge (`Sizes.icon-xs`, 12px). Dot, not pill. |
| `status-tone` | `Tone`                | `Tone.muted` | Color of the status dot. Resolves via the same `Tone` mapping as `value-tone`. |
| `disclosure`  | `DisclosureIndicator` | `none`       | `none \| chevron \| external`. Glyph auto-flips with locale (`›/‹`, `↗/↖`). |

#### Layout & behaviour (5)

| Property        | Type           | Default    | Description |
| --------------- | -------------- | ---------- | ----------- |
| `density`       | `Density`      | `default`  | `compact` → `padding-y: 8px`, `default` → `12px`, `comfortable` → `16px`. Horizontal padding is always `0` — KeyValueRow expects to live inside a padded surface (Card / SectionCard). |
| `unit-position` | `UnitPosition` | `trailing` | Reading position of `value-unit` relative to `value`. `trailing` (default) = value→unit; `leading` = unit→value (currency-prefix style). Symmetric across locales. |
| `wrap`          | `bool`         | `false`    | Applies to `label` and `description` only. When `true`, they wrap and the row grows vertically. `value`, `value-unit`, `value-icon` always stay single-line (numeric values must not character-wrap). |
| `show-divider` | `bool`         | `false`    | Renders 1px `Theme.border` at the row's bottom edge. |
| `tooltip`       | `string`       | `""`       | Hover popup. Cursor-anchored with 500ms emerge delay and click-to-dismiss. A non-empty value installs a row-spanning `TouchArea` that blocks click propagation to a wrapping interactive Card — choose tooltip OR clickable-row-via-Card, not both. |

#### Debug (1)

| Property       | Type   | Default | Description |
| -------------- | ------ | ------- | ----------- |
| `debug-bounds` | `bool` | `false` | 2px magenta border on the row root. Row-only in v1.0; per-cell outlines deferred (escape hatch: extend `Segment` with `debug-outline: bool` if a real need surfaces). |

**Reads from environment:** `Theme`, `Typography`, `Sizes`, `Spacing`, `Locale`, `IconFont`. Animation is implicit (the tooltip delay reads `Animation.slower`).

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
- ✅ **Focus-ring rendering** — implemented on Button, IconButton, and Toggle via a dedicated focus-ring Rectangle drawn outside the surface (or outside the entire control on Toggle), driven by an internal `FocusScope`. Pattern propagates to remaining interactive primitives.
- ✅ **Depth physics architecture** — extracted as the stateless `Depth` global (seven pure functions). Components declare their own six depth-input properties; the global owns the resolution math. Decouples shadow tuning from any single component. Toggle proved the pattern works on a non-rectangular target (the circular knob).
- ✅ **Accessibility wiring** — every interactive component sets Slint's `accessible-role / label / checkable / checked / enabled / action-default`, with a name cascade so `aria-label` omissions still produce a usable screen-reader name. Toggle exercised the first non-`button` role (`switch`) — non-button AccessibleRole values are now an established option for Phase 2 components.
- ✅ **Loading spinner** — rotating glyph driven by `animation-tick()`, with rotation period read from the global `Animation.spinner-period`. Tunable per app at startup. Toggle places the spinner inside the knob; same mechanism, smaller real estate.
- ✅ **Two-layer surface/face depth on non-rectangular elements** — Toggle proved the Button-style skirt + face structure works on a circular knob. `thickness` and `press-animation` compose orthogonally with the knob's x-slide.

Still open, deferred to later Phase 1 work or Phase 2:

- **Compact mode.** `Theme.density` global exists with `"compact" | "default" | "comfortable"`. Specifics — including whether `Density` is per-component overridable — settled when the first density-aware component lands.
- **Variant resolution global.** Button, IconButton, and Toggle all duplicate the tone→color resolution (Toggle's is smaller — only needs the track-on color — but the pattern is identical). Card will be the fourth call site; by then the case for a `Variant` global parallel to `Depth` should be conclusive.
- **`Tone.muted` on Toggle.** Identified during Toggle implementation as a contrast risk: `Theme.muted-foreground` (#8e8e93) as an on-track color is close to the off track (#c6c6c8). Currently shipping; may drop from Toggle's supported tones in v1.1 if the Variant global doesn't fix it first.
- **Toggle knob-drag interaction.** v1 is tap-only. iOS supports drag-the-knob. Listed in `architecture/toggle.md` for v1.1 — needs `TouchArea` event-model work to distinguish a press-with-no-movement from a press-with-drag.
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
| Per-component design docs (`architecture/`)        | `button.md` + `icon-button.md` + `toggle.md` + `card.md` + `key-value-row.md` + `segment-pattern.md`. Policy: non-trivial components get one |
| Tokens + globals                                   | done (10 globals: `Theme`, `Typography`, `Spacing`, `Radius`, `Sizes`, `Animation`, `Depth`, `Locale`, `CurrencyFormat`, `IconFont`) |
| Design language                                    | iOS / SwiftUI-derived; documented in HANDOVER      |
| Single-script segmentation principle               | codified in README + CLAUDE.md; first explicit incarnation shipped in KeyValueRow |
| Segment-as-cell pattern                            | shipped — foundation primitives `Segment`, `SegmentColumn`, `Badge` (private, `components/_*.slint`); documented in `architecture/segment-pattern.md` |
| `Button`                                           | shipped (25 properties, full depth + accessibility) |
| `IconButton`                                       | shipped (19 properties)                            |
| `Toggle`                                           | shipped (19 properties, knob-only depth, `accessible-role: switch`) |
| `Card`                                             | shipped (16 properties, opt-in interactivity, conditional accessibility shim) |
| `KeyValueRow`                                      | shipped (18 properties, segment-as-cell composition, cursor-anchored tooltip, locale-stable height) |
| Phase 1 smoke test (`examples/settings-display.slint`) | pending                                        |
| Playground app (`abdu-slint-ui-playground`)        | shipped (sidebar + scrollable toolbar + Button & IconButton & Toggle & Card & KeyValueRow sections + spinner-period live tuning) |
| Phase 2 primitives (10 more)                       | pending                                            |
| Workspace integration into `e2manage-pos-terminal`  | pending (Phase 4)                                 |
