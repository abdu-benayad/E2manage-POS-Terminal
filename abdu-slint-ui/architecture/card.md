# Card — Design

> Per-component design doc. Sibling docs live under `abdu-slint-ui/architecture/`.
> Role: the *what and why* for Card. Implementation steps live in `IMPL.md`
> (Component 4 — superseded by the API surface below in the same way Button's,
> IconButton's, and Toggle's specs were).

---

## Purpose

A surface container. The first non-interactive primitive in the library (interactive only when opted in via `interactive: true`), and the first to accept a `@children` slot rather than rendering its own content from properties.

Card is a *structural* primitive, not a *semantic* one. It has no `variant`, no `tone`, no domain-color identity. Cards are surfaces on which other content is placed; coloring the whole surface to imply meaning (a destructive-red Card containing destructive content) is the kind of styling that reads as "this is dangerous to my eye" while being inaccessible to screen-reader users who don't see the color at all. Semantic meaning belongs in the content; Card just holds it up.

Card shares with Button/IconButton/Toggle:

- The six depth properties resolved via the `Depth` global.
- The accessibility cascade pattern, **only activated when `interactive: true`** — non-interactive cards don't appear in the AT tree at all (`accessible-role: none`).
- The two-layer surface/face structure when `thickness > 0` (rare but supported for parity).
- `debug-bounds` instrumentation, with the magenta corner aria-badge gated by `interactive` (a non-interactive card with no aria-label is correct behavior, not a missing-name bug).

Card is **not** derived from Button/IconButton/Toggle. It is the fourth sibling primitive — none of them inherit from each other.

---

## Scope

**In scope (v1):**

- Pure surface container — receives a `@children` slot, renders the surface around it.
- `bordered: true` default — 1px `Theme.border`. Provides definition independent of shadow.
- `padding: Density { compact | default | comfortable }` mapping to `Spacing.md / lg / xl`.
- `padding-override` escape hatch with a dual-sentinel: `0px = use Density preset`, `0.001px = explicit zero padding` (full-bleed image cards, edge-to-edge list rows).
- Shape resolution: `default → Theme.card-shape`, plus explicit `rounded` (large radius — `Radius.lg / 14px`) and `square`. `pill` and `circle` are accepted but fall back to `rounded` with a doc-comment note (cards are wide rectangles; pill / circle shapes don't compose meaningfully).
- `max-content-width` cap (renamed from IMPL spec's `max-width` to avoid Rectangle's reserved property).
- Full depth set on the surface: `elevated`, `shadow-elevation`, `shadow-color`, `shadow-direction`, `thickness`, `press-animation`.
- Opt-in interactivity via `interactive: true`: hover/press feedback, focus ring, `clicked()` + the three lifecycle callbacks, accessibility cascade, tooltip, keyboard activation (Enter/Space).
- `disabled` (only effective when `interactive: true`).
- `debug-bounds` instrumentation, with the aria badge gated on `interactive`.

**Explicitly out of scope:**

- `variant` and `tone` — Card is a surface, not a semantic primitive. The case for the `Variant` global stays unsettled by Card; Chip/OptionTile in Phase 2 will be the fourth call site that closes it.
- `label` / `title` / `header` — that's `SectionCard` in Phase 2 (a composition of Card + header row, the dominant pattern in settings screens).
- `loading` — cards don't spin. Async-gated content renders a skeleton inside the card; the card itself stays visually constant.
- `checkable` / `checked` — card-as-toggle is an unusual pattern. If a screen needs it, the consumer manages selection externally and renders a different style. Out of scope for v1.
- Drag interaction, resize, drop targets — pure structural primitive, no DnD machinery.
- Built-in header / footer slots — `SectionCard` covers the header case; footer is rare enough to be a consumer responsibility (just add content at the bottom of `@children`).

---

## Public API

### Properties (16 total)

Smaller than Button (25), IconButton (19), and Toggle (19). Cards have less interaction surface area; the property count reflects that.

**Identity & accessibility**

| Property      | Type     | Default | Notes                                                                              |
|---------------|----------|---------|------------------------------------------------------------------------------------|
| `aria-label`  | `string` | `""`    | Explicit a11y name. Required when `interactive: true`. Cascade: `aria-label → tooltip → "Card"`. No `label` step — Card has no visible text. |
| `tooltip`     | `string` | `""`    | Hover discoverability. **Only rendered when `interactive: true`** — a tooltip on a static surface promises interaction the user can't have. |

**Visual**

| Property   | Type      | Default   | Notes |
|------------|-----------|-----------|-------|
| `shape`    | `Shape`   | `default` | `default → Theme.card-shape`. `rounded → Radius.lg (14px)`, `square → 0px`. `pill` and `circle` fall back to `rounded` (documented in the property's Slint doc-comment). |
| `bordered` | `bool`    | `true`    | 1px `Theme.border` border. Safe default — guarantees visibility even with `elevated: false` or `shadow-elevation: none`. Set `false` for pure-shadow cards. |
| `padding`  | `Density` | `default` | `compact → Spacing.md (12px)`, `default → Spacing.lg (16px)`, `comfortable → Spacing.xl (24px)`. |

**State**

| Property      | Type   | Default | Notes |
|---------------|--------|---------|-------|
| `interactive` | `bool` | `false` | Opt-in interactivity. Activates hover/press feedback, the accessibility cascade, the focus ring + FocusScope, the tooltip, the `clicked()` callback, and keyboard activation. When `false`, the Card is a pure non-clickable surface and **is not in the keyboard tab order**. |
| `disabled`    | `bool` | `false` | Only effective when `interactive: true`. Opacity dim (0.5), blocks click and keyboard activation, removes the card from the focus chain. |

**Layout**

| Property             | Type     | Default | Notes |
|----------------------|----------|---------|-------|
| `max-content-width`  | `length` | `0px`   | `0px` = no cap. Renamed from IMPL spec's `max-width` (HANDOVER quirk #2: Rectangle reserves `max-width`). |
| `padding-override`   | `length` | `0px`   | **Dual-sentinel.** `0px` = use the `padding` Density preset (the common case). `0.001px` = explicit zero padding (full-bleed image cards, edge-to-edge list rows). Any other positive value forces that padding on all four sides. See [Padding sentinel](#padding-sentinel) below for the rationale. |

**Depth (delegated to `Depth` global; applies to the surface, not the children area)**

| Property            | Type        | Default       | Notes                                                                  |
|---------------------|-------------|---------------|------------------------------------------------------------------------|
| `elevated`          | `bool`      | `true`        | Master shadow gate.                                                    |
| `shadow-elevation`  | `Elevation` | `sm`          | Hover bumps one step **only when `interactive: true`**. Non-interactive cards don't react to mouse. |
| `shadow-color`      | `color`     | `transparent` | Transparent = Theme token for the level.                               |
| `shadow-direction`  | `int`       | `0`           | Degrees [0, 359].                                                      |
| `thickness`         | `length`    | `0px`         | Two-layer surface/face extrusion. Rare on cards but supported for parity; useful for hero-class interactive cards (e.g. a "PAY" surface on a checkout screen). |
| `press-animation`   | `bool`      | `true`        | Only effective when `interactive && thickness > 0`. Face dips down by 70% of `thickness` on press. |

**Debug**

| Property        | Type   | Default | Notes |
|-----------------|--------|---------|-------|
| `debug-bounds`  | `bool` | `false` | Magenta border on the surface + magenta corner dot when `interactive && aria-label == "" && tooltip == ""`. Non-interactive cards never trigger the aria badge (no a11y role to be missing a name for). |

### Callbacks

All four fire **only when `interactive: true`**. Setting `interactive: false` while a consumer has connected to `clicked()` is a no-op (callback never fires).

| Callback                | Notes                                                       |
|-------------------------|-------------------------------------------------------------|
| `clicked()`             | Tap, click, or Enter/Space while focused.                   |
| `pressed-changed(bool)` | Physical press state transitions.                           |
| `hover-changed(bool)`   | Mouse enters / leaves the surface.                          |
| `focus-changed(bool)`   | Keyboard focus gained / lost (only reachable when interactive). |

### What is **not** here

`variant`, `tone`, `label`, `title`, `header`, `bg-color`, `track-color-on` / `track-color-off`, `loading`, `checkable`, `checked`, `icon-leading`, `icon-trailing`, `full-width`, `height-override`. See [Scope → out of scope](#scope) for rationale.

---

## New enum

None. Card uses existing `Shape`, `Density`, `Elevation`.

---

## Sizing rules

Card has no preset height or width — it sizes to its `@children` content. The library *does* impose:

- A **minimum content padding** governed by the `padding` Density preset (or `padding-override` when set).
- An **optional maximum width** via `max-content-width`. When `0px` (default), the Card grows to fill its available width up to the children's natural preferred-width.
- **Width stretching:** Card has `horizontal-stretch: 1.0` by default — it expands within its parent's horizontal layout. Consumers wanting a fixed-width card set `max-content-width` or wrap the Card in a sized parent.

There is no `height-override` because there's no preset height to override — Card always sizes to children + padding.

### Padding sentinel

Distinguishing "use the Density preset" from "explicit zero padding" with a single `length` property requires a sentinel. Options considered:

1. **Negative-length sentinel** (`padding-override: -1px = use preset`): rejected. Negative lengths are visually meaningless and surprising in autocomplete.
2. **Separate `padding-mode: enum { preset, explicit }` property**: rejected. Adds a 17th property for a corner case; couples two values to express one logical choice.
3. **Dual sentinel** (`0px = preset`, `0.001px = explicit zero`): **chosen**. `0px` follows the established library pattern (`height-override: 0px = use size preset`); `0.001px` is the explicit-zero escape. Documented in the property's Slint doc-comment.

The resolution logic:

```slint
property <length> resolved-padding:
      root.padding-override == 0px        ? root.density-padding         // preset (most common)
    : root.padding-override <= 0.001px    ? 0px                          // explicit zero
    : root.padding-override;                                             // explicit positive
```

**Note for maintainers:** the `0.001px` magic number is intentional. Do not "clean up" by collapsing the three-way conditional into `padding-override > 0px ? padding-override : density-padding` — that would silently break the full-bleed-image use case (`padding-override: 0.001px` would treat as "use preset", which is the opposite of what the consumer intended).

### Shape resolution

```slint
property <string> resolved-shape:
      root.shape == Shape.default ? Theme.card-shape
    : root.shape == Shape.rounded ? "rounded"
    : root.shape == Shape.square  ? "square"
    : "rounded";                              // pill / circle fall back

property <length> resolved-radius:
      root.resolved-shape == "square" ? 0px
    : Radius.lg;                              // 14px — larger than buttons' Radius.md
```

**Why `Radius.lg` (14px) and not `Radius.md` (10px) like buttons.** Cards are physically larger surfaces; a 10px radius on a 400×300 card reads as "almost square." 14px is the iOS Card convention (`UIVisualEffectView` and similar) and visually matches the larger surface. `Radius.xl` (18px) is reserved for sheets/modals — too soft for inline cards.

**Why `pill` and `circle` fall back to `rounded`.** A 300px-wide pill-shaped card is a giant lozenge — the radius equals half the height, ~150px, swallowing the corners and making content placement awkward. `circle` requires 1:1 aspect ratio that cards rarely have. Rather than refuse with a compile error (no good Slint mechanism for that) or render visually broken (capsule of width), we fall back to the sensible default and document the behavior. The `Shape` enum stays a single shared type across components; per-component validity is handled by resolution.

---

## Internal visual structure

```
Card (root Rectangle — transparent, sizing dictated by inner surface + content)
├── focus-ring Rectangle  (only renders when interactive && focus-scope.has-focus)
│   ├── positioned to wrap the SURFACE bounds (not the shadow's blur radius)
│   └── radius = resolved-radius + focus-ring-offset
│
├── surface Rectangle  ← the visible card
│   ├── width = root.width, capped by max-content-width when > 0
│   ├── background = base-bg-resolved (interactive hover/press tints applied)
│   ├── border-radius = resolved-radius
│   ├── border-width = debug-bounds ? 2px : (bordered ? Sizes.border-thin : 0px)
│   ├── border-color = debug-bounds ? #ff00ff : Theme.border
│   ├── drop-shadow-* via Depth.*
│   ├── clip: true        ← children clipped to the rounded corners
│   │
│   └── face Rectangle  ← "top face" when thickness > 0 (two-layer)
│       ├── y = (interactive && visually-pressed && press-animation) ? thickness * 0.7 : 0px
│       ├── height = parent.height - thickness
│       ├── width = parent.width
│       ├── background = face-bg-resolved (transparent gradient or flat fill)
│       ├── border-radius = parent.border-radius
│       ├── clip: true    ← children clipped to face's rounded corners
│       │
│       └── content VerticalLayout
│           ├── padding = resolved-padding (uniform on all four sides)
│           └── @children
│
├── debug aria badge  (only when interactive && debug-bounds && cascade falls through)
├── TouchArea         (enabled = interactive && !disabled)
├── FocusScope        (only present when interactive — kept out of tab order entirely otherwise)
└── tooltip Rectangle (only when interactive && tooltip != "" && hovered && !disabled)
```

### Why focus-ring wraps the surface, not the shadow

The focus ring is a 2px outline around the *clickable region*. The clickable region is the surface — pixels outside the surface (inside the shadow's blur radius) aren't part of the card visually or interactively. A focus ring extending into the shadow would:

- Float in apparent emptiness for users with light themes (the shadow is partially transparent; the ring would partly overlap shadow, partly overlap background).
- Misalign with the user's mental model ("the card is THIS thing; the ring should hug THIS thing").
- Grow as `shadow-elevation` increases, making the focus ring's size depend on shadow magnitude — a semantic coupling between visual depth and a11y indicator.

The ring is positioned at `surface.x - focus-ring-offset, surface.y - focus-ring-offset` with width/height matching the surface plus 2 × offset. Same pattern as Button/IconButton/Toggle.

### Clipping discipline

`surface.clip = true` is non-negotiable. Without it:
- A full-bleed image inside the Card (set via `padding-override: 0.001px`) renders as a square block hanging over the rounded corners.
- A child with explicit absolute positioning could escape the surface boundary entirely.

`face.clip = true` is also enforced. The two `clip: true` declarations ensure that no matter which layer the consumer's content lives on (face or surface when `thickness == 0`), corners stay clean.

**Trade-off:** clipping prevents drop-shadows on child elements from escaping the card. If a consumer wants a Card containing a Button whose drop-shadow extends past the Card's edge, this won't work. In practice, nested shadows on cards are rare and usually visually muddy anyway; the clipping discipline is the right default.

### State semantics (interactive)

```
property <bool> visually-pressed: root.interactive && !root.disabled && touch.pressed;
```

- **Hover** (interactive only): background tints to `base-bg.darker(2%)`; shadow bumps one step via `Depth.bumped()`.
- **Pressed** (interactive only): background tints to `base-bg.darker(4%)`; opacity 0.96; face dips when `thickness > 0`.
- **Focused** (interactive only): focus ring renders around the surface.
- **Disabled** (interactive only): opacity 0.5; TouchArea disabled; FocusScope disabled; no hover/press response.
- **Non-interactive** (`interactive: false`): no hover, no press, no focus, no opacity change, no tooltip. The card is inert.

### Focusability rule

`FocusScope.enabled = interactive && !disabled`. Concretely:
- `interactive: false` → no focus scope active. The card does not appear in the keyboard tab order, Enter/Space at the page level doesn't reach it, and `focus-changed` never fires.
- `interactive: true, disabled: false` → in tab order; Enter/Space fires `clicked()`.
- `interactive: true, disabled: true` → removed from tab order (disabled focus scope). Behaves like a non-interactive card for keyboard users.

This matches WCAG 2.4.3 (Focus Order): only operable elements participate in the focus sequence. A non-interactive card is not operable.

### Accessibility cascade (interactive only)

```
// Only set when interactive:
accessible-role: button;
accessible-label:
      root.aria-label != "" ? root.aria-label
    : root.tooltip    != "" ? root.tooltip
    : "Card";
accessible-enabled: !root.disabled;
accessible-action-default => { root.clicked(); }

// When interactive: false, accessible-role is left as the Rectangle default (none).
```

**Why `accessible-role: button` and not a Card-specific role.** Slint's AccessibleRole enum has no `Card` value (cards aren't a platform-recognized AT primitive). An interactive card behaves identically to a button from an AT perspective — it's a region the user can activate to do something. `button` is the correct mapping.

**`label` is absent from the cascade** because Card has no visible-text equivalent. SectionCard (Phase 2) *will* have a `title` property and its cascade will be `aria-label → tooltip → title → "Section"`.

### Debug aria badge

Renders when **all four** conditions hold: `interactive: true && debug-bounds: true && aria-label == "" && tooltip == ""`. The `interactive` gate is critical — a non-interactive card with no aria-label is *correct* (it has no a11y role), not a missing-name bug. Flagging it would be noise.

### Hover-bump gating

`Depth.bumped()` is called with `(shadow-elevation, touch.has-hover && interactive)`. Non-interactive cards pass `false` for the hover argument, so they never bump. This preserves a visually-static look for surface cards in long settings lists (where lots of subtle hover shadows would be visually noisy).

---

## Depth integration

Identical caller pattern to Button/IconButton/Toggle:

```slint
property <bool>      effective-hover: touch.has-hover && root.interactive;
property <Elevation> eff-level:     Depth.bumped(root.shadow-elevation, effective-hover);
property <bool>      apply-shadow:  Depth.applies(root.elevated, root.disabled, root.shadow-elevation);
property <length>    eff-magnitude: Depth.magnitude(root.eff-level);

surface := Rectangle {
    drop-shadow-blur:     root.apply-shadow ? Depth.blur(root.eff-level) : 0px;
    drop-shadow-offset-x: root.apply-shadow ? Depth.offset-x(root.shadow-direction, root.eff-magnitude) : 0px;
    drop-shadow-offset-y: root.apply-shadow ? Depth.offset-y(root.shadow-direction, root.eff-magnitude) : 0px;
    drop-shadow-color:    root.apply-shadow ? Depth.color-of(root.eff-level, root.shadow-color) : #00000000;
    ...
}
```

The only Card-specific wrinkle is the `effective-hover` indirection — the global's `bumped(level, hovered)` signature accepts a `bool`, and we gate it on `interactive` before passing it in. This is cleaner than adding a third parameter to `Depth.bumped` (`interactive: bool`), which would couple a generic math global to a Card-specific concept.

---

## Globals consumed

`Theme` (surface, border, ring, shadow-*, tooltip-*), `Radius` (`lg`, `sm` for tooltip), `Spacing` (Density preset mappings), `Sizes` (`border-thin`, `focus-ring`, `focus-ring-offset`), `Animation` (`fast` for hover/press transitions), `Depth`, and `Typography` (tooltip text).

Not consumed: `Locale` (Card has no RTL-asymmetric content — the surface is symmetric; tooltip text inherits the locale font via Typography), `CurrencyFormat`, `IconFont` (no icons in v1; SectionCard will consume IconFont in Phase 2).

---

## Acceptance criteria (visual validation gate)

Card is done when **every** cell of the matrix below renders correctly in `previews/card.slint` and in the playground section:

- **Shape (3):** default (→ rounded), rounded, square. `pill` and `circle` fall back to rounded with no visual artifact.
- **Padding (3 × 1 override):** `compact / default / comfortable` density presets, plus one preview row each at `padding-override: 0.001px` (explicit zero — full-bleed) and `padding-override: 40px` (oversized).
- **`bordered` × `shadow-elevation`:** `bordered: true / false` × `shadow-elevation: none / sm / md / lg / xl` — 10 combinations. Bordered + none should render a visible flat card. Borderless + none should render an *invisible* card (this is the safety check that motivates the `bordered: true` default).
- **`interactive: false`:** rest, hover (no change), pressed (no change), focused (cannot be — verify by tab-cycling in the preview). Card stays visually static, never appears in tab order.
- **`interactive: true`:** rest, hover (subtle tint + shadow bump), pressed (deeper tint + opacity dim + face dip if `thickness > 0`), focused (focus ring around surface bounds, not shadow), disabled (opacity 0.5, no hover/press).
- **`thickness × press-animation`:** at least one preview row with `interactive: true, thickness: 4px, press-animation: true` — click the card, confirm the face dips while the surface stays put.
- **`max-content-width`:** at least one preview row with `max-content-width: 320px` showing the cap; sibling row without cap fills the available width.
- **Clipping:** at least one preview row with `padding-override: 0.001px` and an explicit child Rectangle that would extend beyond the radius corners — confirm clipping works (corners stay clean).
- **Accessibility:** debug-bounds toggle + `interactive: true` + all naming sources empty → magenta corner badge. Same toggle with `interactive: false` → no badge (correctness, not a bug).
- **RTL:** Locale.rtl: true — the surface itself renders identically (symmetric); children inside flip per their own RTL behavior (Card adds no RTL machinery of its own).

---

## Open questions deferred to Phase 1.5 / Phase 2

1. **Variant resolution as a global.** Card was the long-imagined fourth call site that would close the case for extracting a `Variant` global parallel to `Depth`. With Card shipping without `variant` or `tone`, the case stays at 3 (Button, IconButton, Toggle's `tone-color` resolution). The next real test will be **Chip** or **OptionTile** in Phase 2 — by then we'll have either a clearer pattern to extract or evidence that the duplication is per-component-tuned enough that a global wouldn't help.
2. **`SectionCard` as a composition of Card + header row.** Phase 2. Adds a `title` property, an `icon` property, a `header-trailing-slot` for action buttons. Will compose Card internally rather than reimplementing the depth/border/padding logic.
3. **`elevation` shortcut sugar.** Some design systems offer a single `elevation: int` property that maps to a curated shadow + thickness + border combo (Material's elevation 0–24). Card's current API exposes the components separately. If the playground reveals consumers always set `shadow-elevation` and `thickness` together in the same combinations, a v1.1 `elevation` sugar property might be worth adding.
4. **Skeleton state.** A `Card { skeleton: true }` rendering a pulsing placeholder is a common pattern. Currently consumers render skeletons inside the card's children. If skeleton screens become common in POS use, a built-in skeleton mode is a Phase 2 candidate.
5. **Border tone.** Currently `Theme.border` (iOS separator gray). Some design systems use `border-foreground` when interactive and `border-muted` otherwise. v1 keeps it static; if visual review of interactive vs non-interactive cards in a settings screen feels off, this gets revisited.
6. **Animated entrance.** Cards appearing in lists (e.g. transaction history loading) sometimes fade-in with a slight y-translate. This is a consumer concern (the consumer animates the card's position), not a Card-internal feature. Leaving here for completeness — Card v1 has no entrance animation.

---

## Build order

Two commits, matching the IconButton/Toggle template:

### Commit 1 — `docs(abdu-slint-ui): Card design contract`

1. Add this file (`architecture/card.md`).
2. Update HANDOVER.md only if a scope decision invalidates a prior assertion (no expected updates beyond the eventual post-slice docs pass).
3. No code changes. User reviews the doc.

### Commit 2 — `feat(abdu-slint-ui): Card component + preview + playground section`

1. Write `components/card.slint` (~280 lines expected — smaller than Toggle/IconButton because no variant/tone resolution and no rotating spinner).
2. Re-export from `lib.slint`.
3. Write `previews/card.slint` covering the shape × padding × bordered × elevation × interactive × thickness matrix.
4. Write `abdu-slint-ui-playground/ui/sections/card.slint` exposing every public property as a control. Note: Card is the first section whose preview needs *interactive children* — the preview should embed a Button or Text inside the previewed Card so consumers can see real composition.
5. Wire the section into the sidebar tile list in `ui/playground.slint`.
6. `cargo check` (library) + `cargo build` (playground) clean.
7. User runs the playground, exercises the matrix, confirms visual quality (especially clipping and the focus-ring-on-surface positioning).

---

## Risks

- **Slint `@children` placement inside a clipped Rectangle.** The `clip: true` on `surface` (and `face` when `thickness > 0`) should clip any children to the rounded corners. Verify in preview — if Slint's clip doesn't apply to `@children` correctly, the workaround is wrapping `@children` in an inner Rectangle with explicit radius + clip. Add to HANDOVER's quirks list if it bites.
- **The `padding-override: 0.001px` sentinel.** A small but real risk that a future maintainer "cleans up" the three-way conditional into a two-way one, silently breaking explicit-zero padding. Mitigation: prominent comment in `components/card.slint` referencing this design doc, plus a dedicated playground preview row showing a full-bleed card so the use case is visible and testable.
- **Focus ring on rapidly-resizing cards.** If a Card's children size changes mid-interaction (e.g., text in a child input grows), the focus ring follows the surface bounds — which follow the children. The ring will reposition, which could be visually jarring. Mitigation: `animate width / height` on the focus ring with `Animation.fast`. Add only if review surfaces a problem.
- **`max-content-width` interaction with `horizontal-stretch`.** Card has `horizontal-stretch: 1.0` but is capped by `max-content-width`. Slint's layout system should handle this correctly (stretching up to but not beyond the max), but the interaction is worth verifying in preview.
- **Non-interactive cards with hover effects via consumer overrides.** A consumer could compose a Card with `interactive: false` but wrap it in their own TouchArea + hover state. The Card itself stays visually static (correct), but the consumer's TouchArea would still fire. This isn't a risk to Card's API — it's a consumer choice — but worth documenting that "interactive: false" means "Card itself has no interactivity," not "this region is inert" at the wider layout level.
- **Clipping on cards with nested drop-shadowed children.** A Button inside a Card whose drop-shadow extends past the Card's edge will have its shadow clipped. Documented trade-off; if it bites real screens, the workaround is `bordered: false; shadow-elevation: none` on the Card (turning the Card into a layout container with no clipping concerns), or accepting the clipped shadow.
