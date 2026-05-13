# KeyValueRow — Material parity expansion

> Forward design doc for closing the four gaps between our `KeyValueRow` and the
> equivalent rich-row APIs in Material (`ListTile`) and SwiftUI (`LabeledContent`).
> No code changes yet — confirms surface, surgery, and architectural decisions
> before implementation.
>
> Companion to `architecture/key-value-row.md` (the existing design contract).

---

## Motivation

The current KeyValueRow ships with 13 string-typed properties and zero callbacks.
That covers the dominant POS pattern (label + numeric value + unit), but it
restricts the consumer compared to peer libraries:

- **Material `ListTile`** ships `supporting_text`, `@children` slot for trailing
  widgets, `clicked()` + `enabled`, and an image-based `avatar`. Real consumers
  use these to build `CheckBoxTile`, `SwitchTile`, etc. via inheritance.
- **SwiftUI `LabeledContent`** ships `label` and `content` slots as arbitrary
  Views (not strings), plus form-aware styles, plus `labelsHidden()`. Apple
  composes per-control row primitives (Toggle, NavigationLink, Stepper) on
  top of this slot model rather than typing the value.
- **iOS 26 / Liquid Glass** doesn't change list-row anatomy — Liquid Glass is
  explicitly excluded from "the content layer (lists, tables, media)". So
  the Settings row pattern carrying over from iOS 16+ is the baseline.

Four real gaps follow from that comparison:

1. `description` — secondary line below the label
2. `@children` slot — trailing widget the consumer controls (Toggle, IconButton, custom badge)
3. `clicked()` + `interactive` + `disabled` — direct row interactivity
4. `avatar-image` — image-based leading slot (alongside the existing icon-font `label-icon`)

This doc plans all four. Implementation order: 1, 2, 3, 4 (smallest surgery first; avatar deferred if not POS-critical).

---

## Architectural framing — Path A vs Path B

Two viable paths for evolving the row primitive:

- **Path A (Material-style generic):** add `@children` to KeyValueRow. One
  component, infinitely composable. Risk: API becomes split-personality (string
  value AND @children both possible).
- **Path B (iOS-style specialization):** keep KeyValueRow narrow (label + value
  text). Add separate row primitives for non-text trailings (Toggle is already
  row-shaped; add `ChevronRow`, `ActionRow`, etc. later).

Our existing `Toggle` already row-shaped with its own `label` + `description` +
state. That's already partway down Path B. **This plan picks Path A for
KeyValueRow** (broader composability inside one component) **while keeping
the door open for Path B specializations** (Toggle stays as it is; future
specialized rows can be added without conflict).

The `@children` slot in KeyValueRow lands as the dominant composition primitive
for clicked rows with arbitrary trailing content. Specialized rows can still
exist when a pattern is common enough to warrant a dedicated component.

---

## Property specifications

### 1. `description: string`

```slint
/// Secondary text rendered below `label`. Empty disables.
/// Sized one Typography step below the label (text-xs for subtle/normal
/// emphasis, text-sm for total). Color: Theme.muted-foreground — stronger
/// hierarchy than Material's same-color-but-smaller pattern.
/// **Not part of the accessibility cascade** — captions aren't names.
in property <string> description: "";
```

**Implementation notes:**
- Refactor `LeadingCluster` from a single `HorizontalLayout { icon, label }` to
  a conditional:
  - When `description == ""`: current single-line layout.
  - When `description != ""`: `HorizontalLayout { icon, VerticalLayout { label, description } }`.
- Bump `row-content-height` when `description != ""`:
  `(label-font-size + description-font-size) * 1.4` instead of `value-font-size * 1.6`.
  Locale-stable height still holds.
- Pass `description`, `description-font-size`, `description-color` from
  KeyValueRow root down to `LeadingCluster`.

**Surgery scope:** ~30 lines. `LeadingCluster` body grows; KeyValueRow adds 1
property + 2 derived sizing properties.

**Risk:** Low. Mirrors `Toggle.description` which already works.

**Open decision (resolved):** Color is `Theme.muted-foreground`, matching our
Toggle convention. Material's "same color but smaller" is rejected — too subtle
on POS surfaces.

---

### 2. `@children` slot

No property declaration — `@children` is structural Slint syntax.

```slint
// Inside KeyValueRow body, in BOTH outer-layout branches (LTR + RTL),
// AFTER `TrailingCluster`:
@children
```

Doc-comment policy on KeyValueRow itself:

```slint
// Trailing-widget slot. Children passed to KeyValueRow render AFTER
// `value` / `value-unit` / `value-icon` (which can also be empty if
// you want children-only). Width: child-controlled; no stretch.
// Position flips with `Locale.rtl` alongside the TrailingCluster —
// children always sit on the row's trailing edge (physical-RIGHT in
// LTR, physical-LEFT in RTL).
//
// Common composition: KeyValueRow with a Toggle, IconButton, or
// custom badge in the trailing slot.
```

**Implementation notes:**
- Add `@children` placement in both LTR-block and RTL-block of the outer
  HorizontalLayout. The slot mirrors with locale automatically.
- Children sit immediately after `TrailingCluster` in both branches.
- No conflict with existing `value` / `value-unit` / `value-icon` — both render.
  If the consumer wants children-only, they leave `value` empty (zero-width
  cluster collapses).

**Surgery scope:** ~10 lines.

**Risk:** Low if coexist. Slint's `@children` has no detect-empty mechanism,
so we can't conditionally hide `TrailingCluster` when children are present.
Document the coexistence policy in the property doc-comment.

**Open decision (resolved):** Coexist. Simplest, no conditional logic.

---

### 3. `clicked()` + `interactive: bool` + `disabled: bool`

```slint
/// Opt-in interactivity. When true: row is keyboard-tabable, fires
/// `clicked` on tap or Enter/Space, renders hover/press feedback,
/// gets accessible-role: button. When false (default): pure display,
/// no feedback, not in tab order. Same gating pattern as Card.
in property <bool> interactive: false;

/// Only effective when `interactive: true`. Dims opacity to 0.5,
/// blocks click + keyboard.
in property <bool> disabled: false;

/// Accessibility label override. Cascade when interactive:
/// `aria-label → tooltip → label → "Row"`.
in property <string> aria-label: "";

/// Fires when `interactive && !disabled` and the row is activated
/// (tap or Enter/Space).
callback clicked();

/// State-transition callbacks; fire only when interactive.
callback hover-changed(bool);
callback pressed-changed(bool);
callback focus-changed(bool);
```

**Implementation notes:**

1. Add `TouchArea` at root. `enabled: interactive && !disabled`.
   `mouse-cursor: interactive ? pointer : default`.
2. Add `FocusScope` for keyboard activation. Space/Enter → `clicked()`.
3. Add focus-ring Rectangle (`Theme.ring`), visible on `focused`.
4. Hover/press background tint via `Theme.hover-tint(Theme.surface)` /
   `Theme.press-tint(Theme.surface)` — same helpers Card uses. Row's base
   background goes `transparent → hover-tint(surface)` on hover. Mirrors
   Card's pattern exactly.
5. `disabled`: opacity 0.5, TouchArea disabled.
6. Accessibility cascade (only when interactive): conditional inner shim
   `if interactive: Rectangle { accessible-role: button; accessible-label: <cascade>; accessible-action-default => clicked() }`.
   Reuses Card's conditional-inner-shim pattern (HANDOVER quirk #14 —
   `accessible-role` requires a compile-time-constant expression).

**TouchArea / tooltip integration:**

Before: tooltip support wrapped its own conditional `if tooltip != "": Rectangle { TouchArea }`.
Now: one TouchArea handles BOTH click capture (when interactive) AND tooltip
hover. The TouchArea is rendered whenever `interactive || tooltip != ""` (i.e.,
any reason exists to capture events). Tooltip uses `touch-area.has-hover`
regardless of interactive state.

**Card composition policy:** when KeyValueRow has its own TouchArea (interactive
or tooltip-bearing), it consumes pointer events. A wrapping
`Card { interactive: true; clicked => ... }` won't receive them. Consumer
picks one layer for interactivity. Documented in `interactive`'s doc-comment.

**Surgery scope:** ~80 lines. Biggest of the four.

**Risk:** Medium-high. Touches accessibility, tooltip, focus, hover. Three
subsystems need to stay consistent. The "disabled-only-effective-when-interactive"
rule is easy to break — guard with explicit checks in every relevant binding.

**Open decision (resolved):** `disabled` is opacity-dim only (matches Card).
No muted-text variant. Disabled rows stay readable.

---

### 4. `avatar-image: image` + companion props

```slint
/// Image rendered as a circular leading avatar. Sized to row content
/// height. When set, takes precedence over `label-icon` (which is the
/// icon-font-glyph path — consumer picks one paradigm).
in property <image> avatar-image;

/// Avatar background tint (visible when image has alpha or when used
/// with `avatar-text`). Default `Theme.surface-muted` for a subtle
/// circle that contains image overlap gracefully.
in property <color> avatar-background: Theme.surface-muted;

/// Text-based avatar fallback (initials like "AB"). Renders inside
/// the circle when `avatar-image` is unset.
in property <string> avatar-text: "";

/// Foreground for avatar-text (and image colorize). Default
/// `Theme.foreground`.
in property <color> avatar-foreground: Theme.foreground;
```

**Implementation notes:**
- New `Avatar` sub-component (Rectangle, circular, sized to row-content-height)
  at the start of `LeadingCluster`.
- Rendered when any of: `avatar-image.width > 0 && avatar-image.height > 0`,
  `avatar-text != ""`, OR `avatar-background != transparent`.
- Avatar-OR-label-icon: if both are set, avatar wins. Doc-noted.

**Surgery scope:** ~40 lines (new sub-component + integration).

**Risk:** Low-medium. Image rendering is well-supported in Slint. Sizing tied
to row content height keeps locale-stability.

**Open decision (resolved):** Avatar shape is always circular (Material + iOS
convention). Configurable shape is bloat for the row primitive.

**Priority:** Lowest of the four. Image avatars are not a POS-critical pattern
(cashier profile pictures might use them; nothing more). Implement if a real
use case appears, defer otherwise.

---

## Build order

Each slice ships as a standalone commit.

| Order | Slice | Surgery | Risk | Why this position |
|---|---|---|---|---|
| 1 | `description` | ~30 lines | Low | Smallest surgery; biggest immediate UX win; no architectural decisions left |
| 2 | `@children` slot | ~10 lines | Low | Small; opens up Toggle-in-trailing pattern; no conflict with existing props once coexist is settled |
| 3 | `clicked + interactive + disabled` | ~80 lines | Med-high | Biggest surgery; touches accessibility/tooltip/focus; mirrors Card's pattern so the cascade is established |
| 4 | `avatar-image` + companions | ~40 lines | Low-med | Lowest priority; defer if not POS-critical |

After all four: KeyValueRow's API surface goes from **13 properties + 0 callbacks** to **18 properties + 4 callbacks + 1 children slot**. Lands at the high end of the 15–25 target per CLAUDE.md, still inside the band.

---

## Cross-cutting decisions (recap)

| Decision | Chosen approach |
|---|---|
| `description` color | `Theme.muted-foreground` (stronger hierarchy than Material's same-color-but-smaller) |
| `@children` + value-cluster | Coexist (children renders after value cluster) |
| TouchArea ownership when both `interactive` and `tooltip` are set | Single TouchArea handles both — clicks (gated on `interactive && !disabled`), hover (tooltip) |
| `interactive` row vs wrapping interactive Card | Document as mutually exclusive — whichever has TouchArea consumes the clicks |
| Avatar paradigm | Image takes precedence over `label-icon` (doc-noted) |
| Avatar shape | Always circular (Material + iOS convention) |

---

## What this plan does NOT include

- **Polymorphic "leading slot"** that accepts an image OR an icon OR text
  uniformly (Material's `Avatar` does this). Considered; rejected. Type-driven
  separation (`label-icon: string` for glyph names, `avatar-image: image` for
  images) keeps each path's contract clear in the type system.
- **FormatStyle integration** (SwiftUI's `LabeledContent("Number", value: 100, format: .number)`).
  Slint has no FormatStyle protocol. Consumers pre-format value strings on the
  Rust side. Not a closable gap.
- **`labelsHidden()` modifier** equivalent. Niche; setting `label: ""` already
  works for the use case.
- **State layer / ripple feedback** on press (Material's M3 ripple). Our visual
  language is iOS-faithful — flat-tint hover/press without rippling. Drop.
- **Specialized row primitives** (`ChevronRow`, `ActionRow`, etc.). These are
  separate components, planned later as the codebase matures. Not part of
  KeyValueRow's surface.

---

## Estimated total cost

~160 lines of Slint across the four slices, plus playground section updates
(~50 lines), preview file updates (~40 lines), and architecture doc revisions
(this doc + updates to the original `architecture/key-value-row.md`).

Approximate total: 250–300 lines of code change + 100 lines of doc.

---

## Open questions for confirmation before any slice lands

None remaining; all six cross-cutting decisions resolved above. Begin with
`description` when approved.
