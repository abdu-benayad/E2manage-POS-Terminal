# abdu-slint-ui — Project Instructions

> This file applies to work inside `abdu-slint-ui/`. It supplements `~/.claude/CLAUDE.md` and the parent project's `CLAUDE.md`, and **explicitly overrides** them where stated below.

Read this *before* editing anything in this directory. The general principles from the user-wide CLAUDE.md still apply except where this file overrides them.

The companion document [README.md](./README.md) is the **design contract** — what to build. This file is the **construction discipline** — how to build it.

---

## Project context

This is a UI component library for Slint applications. It exists to provide *living, environment-aware* components that consumer apps compose against, eliminating inline styling, locale ternaries, currency string-concatenation, and toggle-knob rebuilding from screen code.

The work mode here is fundamentally different from general application development:

- **The library *is* an abstraction.** "Avoid premature abstraction" — the abstraction *is* the deliverable.
- **The public API is the product.** Property names, types, defaults, callback signatures are user-facing and stability-sensitive.
- **Stability matters more than expressiveness.** A clever API that changes every release is worse than a verbose one that doesn't.
- **Visual quality is the primary correctness criterion.** A component that compiles, passes type checks, and emits the right callbacks but *looks dated* has failed.

---

## Overrides to `~/.claude/CLAUDE.md`

The following general rules are **suspended or inverted** for work inside this directory.

### 1. "Narrow APIs (~8 properties max)" — SUSPENDED

UI component libraries ship rich APIs. Material UI Button: 18 properties. Mantine: 25. Ant Design: 22. **Target 15–25 properties per interactive component**, 5–10 for display-only.

This is not bloat. It is discoverability and convenience. The 8-property heuristic is for *application code* where each component is tied to one use site. Library components serve unknown future use sites and need ergonomic breadth.

If you're considering a 26th property, ask: "is this a new component in disguise?" Sometimes yes; usually no.

### 2. "Composition over kitchen sinks" — PARTIALLY SUSPENDED

The general rule says compose a Button + LoadingState wrapper for a loading button. **For a library, no.** Common patterns become first-class properties:

```
Button { loading: true; }                  ← yes
LoadingWrapper { Button { ... } }          ← no
```

Built-in conveniences for any interactive primitive:

- `loading: bool` with built-in spinner
- `icon-leading`, `icon-trailing`
- `full-width`, `disabled`, `tooltip`
- `checkable`, `checked` (toggle-style)
- `aria-label` (icon-only accessibility)

Composition still applies for *uncommon* combinations or domain-specific wrappers (e.g., `MoneyButton` composing `Button + Money` in v1.1). For common patterns, ship them built in.

### 3. "No backwards-compat hacks" — INVERTED

A library's public API is a contract with consumers. From v1.0 onward:

- **Adding a property:** non-breaking. Minor version bump.
- **Renaming a property:** breaking. Major version only.
- **Removing a property:** breaking. Major version only, after a deprecation period.
- **Changing a property's default value:** breaking (alters behavior for every consumer not setting it explicitly). Major version only.

Backwards-compat *is* the design discipline. Plan for additive evolution from day one. Don't be cute with names; you'll live with them.

### 4. "Working tests > implementation completeness" — ADJUSTED

UI components are validated **visually**, not via unit tests. The primary verification loop is:

1. Write the component
2. Write its preview file in `previews/{component}.slint`
3. Run `slint-viewer previews/{component}.slint`
4. Inspect every variant, size, state, locale visually
5. Component is "done" when every cell of that matrix is correct

Unit tests are limited to:
- **Layout invariants** (e.g., button height matches its `size` enum value)
- **Behavior** (e.g., disabled buttons do not fire `clicked`)
- **State machines** (e.g., Toggle animation reaches the expected final state)

**Do not write tests for visual properties** — colors, radii, shadows, spacing. Visual review catches those; tests cannot meaningfully verify them.

### 5. "Type-driven development" — REFINED

Still applies. The refinement specific to Slint:

**Variants and discrete value sets use Slint enums, not strings.**

```slint
// NO:
in property <string> variant: "default";  // tolerates "destrukktive" silently

// YES:
enum ButtonVariant { default, destructive, outline, secondary, ghost, link }
in property <ButtonVariant> variant: default;  // compiler-checked
```

The README still shows `variant: string` in some component tables — that's documentation shorthand. The implementation uses real enums.

Strings are only acceptable when:
- The value set is genuinely open (free-form labels, custom text)
- The set is too large for a useful enum (ISO currency codes, locale codes)
- Native script content (Arabic unit names, etc.)

---

## Component design principles

### Sensible defaults — common case is zero-config

A component renders usefully with zero properties set:

```slint
Button { }   // renders an empty pill button with default styling
```

Every property has a default. The default produces the most common case.

### Doc-commented properties (the discoverability story)

Every public property gets a Slint doc comment:

```slint
/// The visible button text. Empty renders an icon-only button.
in property <string> label;

/// Visual variant. `default` is the primary action style.
in property <ButtonVariant> variant: default;

/// When true, hides the label and shows a spinner; blocks `clicked`.
in property <bool> loading: false;
```

The LSP surfaces these on hover. This is the equivalent of Delphi's Object Inspector for our purposes. Treat it as user-facing documentation, not a code comment.

### Internal state, narrow events

Components own their internal state machines:
- Hover, focus, press, animation progress, transition timing
- Loading spinner state
- Toggle/check transitions

Components emit **semantic callbacks**, not raw input events:

```slint
// YES
callback clicked();
callback value-changed(string);
callback submitted();

// NO
callback mouse-down(PointerEvent);   // wrap a TouchArea if you need this
```

### Living component pattern

Every component reads its environment from globals. **The consumer never passes theme, locale, or currency on every component instance.**

Required environment globals (defined in `globals/`):
- `Theme` — semantic colors, shadows
- `Typography` — fonts, sizes, weights
- `Spacing`, `Radius`, `Sizes` — dimensional tokens
- `Animation` — durations and easings
- `Locale` — RTL boolean, locale code, directional helpers
- `CurrencyFormat` — currency symbol, decimals, position

Consumers populate these once from Rust at app startup. Components read from them everywhere.

---

## Slint conventions for this library

### Naming

| Kind            | Convention            | Example                               |
| --------------- | --------------------- | ------------------------------------- |
| Components      | `PascalCase`          | `Button`, `SectionCard`, `MoneyInput` |
| Properties      | `kebab-case`          | `icon-leading`, `full-width`          |
| Callbacks       | `kebab-case` verb-event | `clicked`, `value-changed`, `focus-lost` |
| Globals         | `PascalCase`          | `Theme`, `Locale`, `CurrencyFormat`   |
| Enum types      | `PascalCase`          | `ButtonVariant`, `ChipTone`           |
| Enum values     | `kebab-case` or lowercase | `default`, `destructive`, `on-surface` |

### File layout

Two sibling directories — the library (pure Slint) and the playground app (Rust binary that consumes the library):

```
abdu-slint-ui/                  # the library
├── README.md                   # Design contract (what)
├── CLAUDE.md                   # Project discipline (how) — this file
├── ROADMAP.md                  # Build phases (when, in what order)
├── IMPL.md                     # Per-phase implementation steps
├── LICENSE-MIT
├── LICENSE-APACHE
├── lib.slint                   # Single import entry point — re-exports public API
├── globals/
│   ├── theme.slint
│   ├── typography.slint
│   ├── spacing.slint
│   ├── radius.slint
│   ├── sizes.slint
│   ├── animation.slint
│   ├── locale.slint
│   ├── currency-format.slint
│   └── icon-font.slint         # icon name → codepoint lookup
├── assets/
│   └── icons.ttf               # bundled icon font (Phosphor or Lucide)
├── components/
│   ├── button.slint
│   ├── icon-button.slint
│   └── ... (one file per primitive)
├── previews/                   # fast per-component dev iteration via slint-viewer
│   ├── button.slint            # variant × size × state matrix for Button
│   └── ... (one preview per primitive)
└── examples/                   # full-screen smoke tests
    └── settings-display.slint

abdu-slint-ui-playground/        # the playground app (Rust + Slint, depends on abdu-slint-ui)
├── README.md
├── Cargo.toml                  # depends on abdu-slint-ui, slint, slint-build
├── src/
│   ├── main.rs                 # entry point, populates globals, mounts window
│   ├── state/                  # per-component Rust state structs (live property values)
│   │   ├── button.rs
│   │   └── ...
│   └── lib.rs
├── ui/
│   ├── playground.slint        # the window — sidebar + preview pane + controls
│   ├── sections/               # one section file per library component
│   │   ├── button.slint
│   │   └── ...
│   └── chrome/                 # playground's own sidebar/topbar/controls (uses std-widgets)
└── build.rs
```

**One component per library file.** One preview per component. One playground section per component. Components import only from `globals/`. Inter-component imports inside `components/` are disallowed unless explicitly justified (e.g., `SectionCard` uses `Card`).

### Imports — single entry point for consumers

Consumer code imports from `lib.slint` only:

```slint
import { Button, Toggle, SectionCard, Money } from "@abdu-slint-ui/lib.slint";
```

`lib.slint` re-exports every public component, global, and enum type. Internal helpers stay unexported.

### Animation

Use Slint's `animate` blocks for property transitions. Read durations and easings from the `Animation` global:

```slint
animate background { duration: Animation.fast; easing: ease-out; }
```

**Never hardcode `150ms` or `ease-out`** in component code. The whole point of the global is that motion is consistent and tunable from one place.

---

## Playground discipline

The sibling crate `abdu-slint-ui-playground` is **not optional documentation** — it is the user-facing discoverability surface for this library, the Object Inspector equivalent. Every component shipped in v1.0 must have a playground section.

### Section requirements

For each component, the playground section must:

1. **Mount the component** in a preview pane that resizes to fit
2. **Expose every public property** as an interactive control:
   - `bool` → toggle
   - `int` / `length` → numeric input or slider with sensible bounds
   - `string` → text input with `placeholder` showing the default
   - enum types → combo box or segmented control listing every variant
   - colors → predefined-palette dropdown for v1 (color picker is out of scope)
3. **Show a live code snippet** of the current configuration in Slint source form, copy-pasteable
4. **Inherit global controls** from the toolbar (theme shape, locale RTL/LTR, currency) — these are app-level, never per-section

### What the playground uses, what it doesn't

- The **preview pane** (where the demoed component appears) renders the actual `abdu-slint-ui` component. This is the consumer-of-library role.
- The **chrome** (sidebar, control panels, toolbar) uses `std-widgets.slint` for inputs, sliders, comboboxes. **The playground does not eat its own dog food** for its UI shell — if the library has a bug, the playground still works to debug it.

### Adding a new component

Workflow when building a new primitive:

1. Build the component (`abdu-slint-ui/components/{name}.slint`)
2. Build the preview file (`abdu-slint-ui/previews/{name}.slint`) — fast dev iteration
3. Build the playground section (`abdu-slint-ui-playground/ui/sections/{name}.slint` + `abdu-slint-ui-playground/src/state/{name}.rs`)
4. Register the section in the playground sidebar
5. Mark the component done only after all three exist and visually pass

This adds work per component, but it pays back permanently: the playground is what consumers see, evaluate against, and copy-paste from.

---

## Internationalization

### Single-script segmentation — the core principle

**No library component renders mixed-script content inside a single `Text` element.** Every multi-content component splits its content into separately-anchored `Text` elements, each holding one script direction. Layout positions the segments per `Locale.rtl`; the *contents* of each segment stay in their native direction.

This isn't a stylistic preference — it's a structural workaround for Slint 1.14's incomplete bidi support. Slint issues [#2294](https://github.com/slint-ui/slint/issues/2294) (RTL layouts RFC, open since 2023) and [#7267](https://github.com/slint-ui/slint/issues/7267) (Persian text bugs in LineEdit, open) mean any `Text` or `TextInput` holding bidi-mixed content has rendering or selection bugs. Segmentation sidesteps the framework limitation by construction: each `Text` only ever sees one direction, so Slint's bidi algorithm never has anything to get wrong.

**When designing a new component:** if it accepts more than one piece of textual content, those pieces are SEPARATE properties handled by SEPARATE `Text` elements. Never combine into one string. Examples:

- ✅ `KeyValueRow { label: string, value: string }` — two properties, two Text elements
- ❌ `KeyValueRow { content: string }` where consumers pass `"إجمالي: 12.50"` — single Text holding bidi content, breaks at runtime
- ✅ `Money { amount: string }` + currency rendered from `CurrencyFormat` global → two Text elements
- ❌ `Money { value: "12.50 ر.س" }` as a single pre-formatted string handed to one Text

This rule supersedes the "LTR-atomic numeric rendering" rule documented in earlier drafts; LTR-atomic is now a *specific case* of segmentation applied to numeric+unit pairs.

### RTL handling

- `Locale.rtl: bool` is the single source of truth for directional layout.
- Use `HorizontalLayout` with `if Locale.rtl` branches to flip layout direction at composition points (mirror the Button `icon-leading` / `icon-trailing` pattern, the Toggle column pattern, the Card column pattern).
- **Consumers never reason about left/right.** They reason about leading/trailing.
- **Geometric metaphors do not flip in RTL.** A switch's on-position is always physical-right regardless of locale (matches iOS Arabic behavior). A slider's "more" direction is always physical-right. Document the rule on any component with a physical metaphor.

### LTR-atomic numeric rendering (a specific case of segmentation)

Components displaying numbers + units (`Money`, `Quantity`, `MoneyInput`, any future `Code` / `Timestamp` / `Percent`) render the pair as an LTR sub-flow regardless of `Locale.rtl`. The pair's *position* in surrounding layout respects locale; the pair's *internal order* does not.

Implementation: number and unit are SEPARATE Text elements (per the segmentation principle), arranged in a fixed `HorizontalLayout` whose direction is locked LTR. The whole pair as a unit is positioned by `Locale.rtl` in its parent.

**This is non-negotiable.** Any new numeric-displaying primitive must follow this rule. If you add such a component, document the LTR-atom behavior in its description.

### Free text input is the exception

The principle holds for everything except inputs accepting arbitrary user text. If a cashier types `"إجمالي 12.50"` into a free-form `Input { kind: text }`, that single string lives in one `TextInput`'s buffer — Slint's bidi rendering kicks in, and the bugs from #7267 apply.

Phase 2 input components inherit this limitation; mitigation strategies (numeric-only restriction, `text-direction: TextDirection` opt-in, splitting the visible value across two fields) live in each component's design doc. For now: document the limitation prominently in the component's doc-comment so consumers don't expect bidi to "just work" inside a single input.

### No user-facing copy

The library contains **zero translated strings**. All user-visible text is a property the consumer provides pre-localized.

Exceptions: emoji/codepoint icons used as visual symbols (these are universal). No actual words in any language.

---

## API stability

### Versioning

- **v0.x** — design settling. Breaking changes expected per release. Consumers pin a specific commit.
- **v1.0** — public API frozen.
- **v1.x** — additive only. Adding properties is OK; renaming/removing is not.
- **v2.0** — bumped only when accumulated breaking changes justify the migration cost.

Slint compatibility is pinned per minor version of the library. We track one specific Slint version at a time.

### Deprecation path

When a property becomes unwise to use but cannot be removed (pre-major):

```slint
/// DEPRECATED in v1.3: use `tone` instead. Will be removed in v2.0.
in property <ChipColor> color: neutral;
```

Leave it functioning identically until the major version removes it.

### Default changes are breaking

Changing a default value is a behavior change for every consumer who didn't set the property explicitly. **Treat as breaking.** Major version only.

---

## Accessibility

Slint's accessibility support is partial and evolving. The library should:

- Use `FocusScope` for any keyboard-interactive component
- Support keyboard activation (`Enter`, `Space`) on every clickable component
- Render a visible focus indicator (focus ring) when focused via keyboard
- Set Slint's `accessible-role` and related properties where they apply
- Provide `aria-label`-equivalent properties for icon-only components (e.g., `IconButton.label` exists for the screen reader, not for visible rendering)

**Do not ship a component that fails keyboard navigation.** Do not ship a focus-trapping component without explicit focus management.

---

## Working stance

### Design-first applies fully

The general rule "no substantial code before architecture is established" is in force:

1. **README.md** (design contract) → done
2. **CLAUDE.md** (this file) → done
3. **IMPL.md** (build order, validation, definition of done) → next
4. **Code** → only after the above are settled and approved

### One component at a time

Build each component to **completion** — component file + preview file + doc comments + visual validation in every variant × size × state × locale — before starting the next. Do not half-write five components in parallel.

### Visual validation gate

A component is "done" only when:

- Every variant renders correctly in `slint-viewer`
- Every size renders correctly
- Every state (default, hover, pressed, focused, disabled, loading) renders correctly
- LTR and RTL both render correctly (toggle `Locale.rtl` in the preview file)
- Doc comments are written on every public property
- The preview file shows all of the above in one window
- **A corresponding section exists in `abdu-slint-ui-playground/`** with every public property exposed as an interactive control and the code-snippet panel showing the current configuration

If any of these fails, the component isn't done. No exceptions.

### Smoke test after the first batch

After 4–6 components are built (Button, IconButton, Toggle, Card, KeyValueRow at minimum), refactor **one real POS screen** against them as `examples/settings-display.slint`. This is a smoke test, not integration into the POS. If the API survives that contact, continue with confidence. If not, revise the offending components *before* building the remaining primitives.

This catches API errors while the surface area is small. Don't build all 14 components against theory.

---

## What this library does NOT do

Build constraints, restated from README:

- **No domain knowledge.** No references to `Cart`, `Product`, `Operator`, `Transaction`, `Shift`. Ever.
- **No platform-specific code.** No `cfg(windows)`, no path handling, no environment variables.
- **No I/O.** No filesystem, network, database, or backend integration. The library renders what it's told to render.
- **No translation tables.** All strings come in as properties.
- **No application state management** beyond visual component state.
- **No re-exports of `std-widgets.slint`.** We build our own equivalents to control the design.
- **No Rust glue inside this library.** Rust code that populates the globals lives in the consumer. The library is pure Slint.

---

## Communication style

Anti-sycophancy from `~/.claude/CLAUDE.md` applies fully. Lead with problems, hold positions unless new evidence appears, push back on bad architectural ideas. The library's API will outlive its first consumer; design decisions deserve scrutiny.

---

## When CLAUDE.md files conflict

Resolution order for any rule that's mentioned in multiple files:

1. **This file** (`abdu-slint-ui/CLAUDE.md`) — highest priority for work inside this directory
2. Parent project CLAUDE.md (`e2manage-pos-terminal/CLAUDE.md`)
3. User-wide CLAUDE.md (`~/.claude/CLAUDE.md`)
4. Default Claude behavior

If a rule is mentioned only at level 2 or 3, it applies unchanged. If it's mentioned in this file, this file wins.
