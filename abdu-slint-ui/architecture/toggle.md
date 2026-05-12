# Toggle — Design

> Per-component design doc. Sibling docs live under `abdu-slint-ui/architecture/`.
> Role: the *what and why* for Toggle. Implementation steps live in `IMPL.md`
> (Component 3 — superseded by the API surface below in the same way Button's and IconButton's specs were).

---

## Purpose

A binary on/off control with a sliding knob. The third interactive primitive after Button and IconButton, and the first whose value semantics are inherent to its visual — a Toggle's *position* is its state, not a transient interaction outcome.

Toggle shares the depth, focus, and accessibility machinery established by Button and IconButton:

- Six knob-shadow properties resolved through the `Depth` global (no duplicated math).
- Accessibility cascade (`aria-label → tooltip → label → "Toggle"`) wired through Slint's `accessible-*` properties — but with `accessible-role: switch`, not `button`.
- Two-layer surface/face structure on the knob: `thickness` and `press-animation` work the same way they do on Button's face, just on a smaller circular element. The press *depresses* the knob's face into its base; the slide is independent.

Toggle is **not** derived from Button or IconButton. It is a sibling primitive with its own visual structure, its own state machine, and its own size enum. The reusable parts (depth math, accessibility cascade pattern) live in shared globals or are duplicated where duplication is cheaper than the abstraction.

---

## Scope

**In scope (v1):**
- iOS-style pill switch: rounded track with a circular knob sliding between two extremes.
- Three sizes (`sm / md / lg`) mapping to fixed track + knob dimensions tuned for POS-tablet touch.
- Tone override (`Tone.default → Theme.success`, plus `destructive / warning / info / primary / muted` for non-default on-color toggles).
- Optional `label` and `description` rendered beside the track, RTL-aware.
- Optional `on-icon` and `off-icon` rendered *inside* the knob (iOS pattern). Single icon at a time — whichever matches `on`.
- Loading state — knob glyph rotates at `Animation.spinner-period`, interaction blocked.
- Disabled state — opacity dim, click and keyboard activation blocked.
- Tooltip with internal hover delay (Slint's `has-hover` covers the timing).
- Full depth/lighting on the knob: `elevated`, `shadow-elevation`, `shadow-color`, `shadow-direction`, `thickness`, `press-animation`.
- `height-override` escape hatch — proportionally scales the entire track + knob.
- `debug-bounds` instrumentation: magenta border on the track Rectangle + magenta corner dot when the accessibility cascade is genuinely falling through to `"Toggle"`.
- Accessibility wiring (`accessible-role: switch`, cascade label, `accessible-checked`, `accessible-action-default`).
- Tap-anywhere-on-track or tap-on-label activation.

**Explicitly out of scope:**
- `variant` — Toggle has one canonical visual (iOS pill switch). Variant proliferation would dilute the look; consumers needing different track shapes write a different component.
- `shape` — iOS toggles are pill-locked. The track radius is always `track-height / 2`. No `Shape` property.
- `bg-color` / `track-color-on` / `track-color-off` direct overrides — the curated `tone` palette is the public way to recolor a toggle. Matches IconButton's tightening from Button.
- **Drag-the-knob interaction** — v1 is click/tap-only. Tapping the track or label flips `on`. Drag-to-toggle (knob tracks pointer x) is Phase 2+.
- Tri-state / indeterminate — `bool` only. If a tri-state checkbox is needed, that's a different component (`Checkbox` in Phase 2 with explicit `Checked / Unchecked / Indeterminate` enum).
- Multi-label captions, on-text/off-text inside the track ("ON" / "OFF" Material-style) — out. Icons inside the knob cover the same affordance more economically.

---

## Public API

### Properties (19 total)

Sized between IconButton (19) and Button (25). The breakdown:

| Group           | Count |
|-----------------|-------|
| Identity & a11y | 4     |
| State           | 3     |
| Visual          | 2     |
| Knob content    | 2     |
| Depth           | 6     |
| Escape & debug  | 2     |

**Identity & accessibility**

| Property      | Type     | Default | Notes                                              |
|---------------|----------|---------|----------------------------------------------------|
| `label`       | `string` | `""`    | Visible primary text beside the switch. RTL-aware position. |
| `description` | `string` | `""`    | Caption below `label`, rendered in `Theme.muted-foreground`. |
| `tooltip`     | `string` | `""`    | Hover discoverability + feeds the accessibility cascade. |
| `aria-label`  | `string` | `""`    | Explicit a11y name. Always wins the cascade.       |

**State**

| Property   | Type            | Default | Notes |
|------------|-----------------|---------|-------|
| `on`       | `bool` (in-out) | `false` | Controlled state. Toggle flips it on user activation and then fires `toggled(on)`. Matches Button's `checkable` flow. |
| `disabled` | `bool`          | `false` | Opacity dim, blocks click and keyboard activation. |
| `loading`  | `bool`          | `false` | Knob glyph swaps to the rotating loader; blocks `toggled`. Justified: toggles routinely gate async confirmation (cloud-sync, persisted settings). |

**Visual**

| Property | Type         | Default   | Notes                                                                 |
|----------|--------------|-----------|-----------------------------------------------------------------------|
| `size`   | `ToggleSize` | `md`      | `sm / md / lg`. Concrete dims in [Sizing rules](#sizing-rules) below. |
| `tone`   | `Tone`       | `default` | `default` → `Theme.success` (iOS green). `destructive` → red (for "permanently delete" toggles). Same enum as Button/IconButton. |

**Knob content**

| Property   | Type     | Default | Notes |
|------------|----------|---------|-------|
| `on-icon`  | `string` | `""`    | Library-canonical name (resolved via `IconFont`) or raw glyph. Rendered inside the knob when `on`. |
| `off-icon` | `string` | `""`    | Inside the knob when off. |

**Depth (delegated to `Depth` global; applies to the knob, not the track)**

| Property            | Type        | Default       | Notes |
|---------------------|-------------|---------------|-------|
| `elevated`          | `bool`      | `true`        | Master shadow gate on the knob.                |
| `shadow-elevation`  | `Elevation` | `sm`          | Hover bumps one step via `Depth.bumped()`.     |
| `shadow-color`      | `color`     | `transparent` | Transparent = Theme token for the level.       |
| `shadow-direction`  | `int`       | `0`           | Degrees.                                       |
| `thickness`         | `length`    | `0px`         | Knob extrusion depth — two-layer surface/face. |
| `press-animation`   | `bool`      | `true`        | Knob face slides down by 70% of `thickness` on press. Composes with the x-slide. |

**Escape & debug**

| Property          | Type     | Default | Notes |
|-------------------|----------|---------|-------|
| `height-override` | `length` | `0px`   | `0` = use `size` preset. Positive forces track height; knob diameter and track width scale proportionally (aspect ratio = 51/31 = iOS-canonical). |
| `debug-bounds`    | `bool`   | `false` | Magenta border on the track Rectangle + magenta corner dot when the accessibility cascade is falling through to `"Toggle"`. |

### Callbacks

| Callback                | Fires when                                                                                  |
|-------------------------|---------------------------------------------------------------------------------------------|
| `toggled(bool)`         | After Toggle flips `on`. Argument is the *new* value. Fires on tap, Enter/Space, AT default action. |
| `pressed-changed(bool)` | Physical press state transitions.                                                           |
| `hover-changed(bool)`   | Mouse enters / leaves the track or label.                                                   |
| `focus-changed(bool)`   | Keyboard focus gained / lost.                                                               |

### What is **not** here

`variant`, `shape`, `bg-color`, `track-color-on`, `track-color-off`, `min-content-width`, `full-width`, `icon-leading`, `icon-trailing`. See [Scope → out of scope](#scope) for rationale.

---

## New enum

None. `ToggleSize { sm, md, lg }` already exists in `enums.slint` (added during Phase 1 foundation work).

The size map reads from a Toggle-specific dimension table (below), **not** from `Sizes.button-*` — toggle proportions don't track button heights. Track width is dictated by knob diameter and slide distance, not by tap-target size.

---

## Sizing rules

### Preset path

iOS's canonical toggle is 51×31 with a 27px knob; we keep that as `md` and scale conservatively in both directions.

| `size` | Track W | Track H | Knob ⌀ | Gap | Aspect (W/H) |
|--------|---------|---------|--------|-----|--------------|
| `sm`   | 44px    | 26px    | 22px   | 2px | 1.69         |
| `md`   | 51px    | 31px    | 27px   | 2px | 1.65 (iOS)   |
| `lg`   | 60px    | 36px    | 32px   | 2px | 1.67         |

The gap stays fixed at 2px across sizes; it's the inset between the knob edge and the track edge in both x and y. Knob diameter = track height − 2 × gap.

### Override path

When `height-override > 0`:
- `track-height = height-override`
- `gap = 2px` (fixed across sizes and overrides)
- `knob-diameter = track-height - 2 * gap`
- `track-width = track-height * 1.645` — iOS aspect ratio (51/31) preserved

This gives a smooth scale-up path for tablet hero toggles without introducing a fourth `xl` preset.

### Knob x position

- Off:  `x = gap`
- On:   `x = track-width - knob-diameter - gap`

Animated with `Animation.normal` (200ms) + `ease-out`. The press-animation dip on the knob's `face` is on the y-axis and composes independently — pressing while sliding produces a knob that dips and slides simultaneously.

### Label / description column

When `label != ""` or `description != ""`, a vertical column renders alongside the track:

- LTR: track on the left, column on the right.
- RTL: column on the left, track on the right.

Column spacing from track: `Spacing.md` (gap inside the parent `HorizontalLayout`).

Column internal layout:
- `label` Text — `Typography.text-base`, `Theme.foreground`.
- `description` Text — `Typography.text-sm`, `Theme.muted-foreground`, `wrap: word-wrap`, `vertical-spacing: 2px` from the label.

When `label == "" && description == ""`, the column is omitted entirely — Toggle is a bare track.

---

## Internal visual structure

```
Toggle (root, transparent, sizing + events only)
├── root: drop-shadow on the root is silently dropped (Slint quirk #1) — the
│   knob's shadow lives on the inner knob-surface Rectangle. The root carries
│   sizing, focus-ring positioning, opacity dim, and TouchArea/FocusScope only.
│
├── focus-ring Rectangle  (outside the track, drawn when focus-scope.has-focus)
│
├── HorizontalLayout  (alignment + spacing depend on Locale.rtl)
│   │
│   ├── track Rectangle  ← always-present pill
│   │   ├── width = resolved-track-width
│   │   ├── height = resolved-track-height
│   │   ├── border-radius = height / 2   (pill, locked — no `shape`)
│   │   ├── background = animated between track-off-bg and track-on-bg
│   │   ├── border-width = Sizes.border-thin
│   │   ├── border-color = subtle inner-edge tone (track-on-bg.darker(0.1) when on,
│   │   │                 Theme.border.darker(0.05) when off) — provides edge
│   │   │                 definition without faking inset shadow
│   │   │
│   │   └── knob-surface Rectangle  ← "base/skirt" of the knob
│   │       ├── width = knob-diameter
│   │       ├── height = knob-diameter
│   │       ├── x = animated between off-x and on-x  (Animation.normal, ease-out)
│   │       ├── y = gap
│   │       ├── border-radius = knob-diameter / 2   (always a circle)
│   │       ├── background = #f0f0f0  (knob skirt — the visible side wall when thickness > 0)
│   │       ├── drop-shadow-* via Depth.*  (knob's drop-shadow lives here)
│   │       │
│   │       └── knob-face Rectangle  ← "top face" of the knob
│   │           ├── y = (visually-pressed && press-animation) ? thickness * 0.7 : 0
│   │           ├── height = parent.height - thickness
│   │           ├── width = parent.width
│   │           ├── background = @linear-gradient(180deg, #ffffff 0%, #f5f5f5 100%)
│   │           ├── border-radius = (parent.height - thickness) / 2
│   │           ├── animated y, height, background
│   │           │
│   │           ├── highlight Rectangle  (top half, glossy sheen)
│   │           │   white-to-transparent vertical gradient, 50% height
│   │           │
│   │           └── knob-icon Text
│   │               ├── text = loading ? IconFont.resolve("loader")
│   │               │       : root.on ? IconFont.resolve(root.on-icon)
│   │               │       : IconFont.resolve(root.off-icon)
│   │               ├── font-family = IconFont.font-family-name()
│   │               ├── font-size = knob-diameter * 0.5
│   │               ├── color = Theme.muted-foreground  (icons inside the white knob)
│   │               ├── rotation-angle when loading: mod(animation-tick(), Animation.spinner-period)
│   │               │                              / Animation.spinner-period * 360deg
│   │               ├── transform-origin: { x: self.width / 2, y: self.height / 2 }
│   │               └── horizontal+vertical alignment center
│   │
│   └── VerticalLayout  (omitted when label == "" && description == "")
│       ├── label Text         (Typography.text-base, Theme.foreground)
│       └── description Text   (Typography.text-sm,  Theme.muted-foreground, word-wrap)
│
├── debug aria badge  (debug-bounds + cascade-falls-through)
├── TouchArea          (covers root — tap on track OR label flips `on`)
├── FocusScope         (Enter/Space activates)
└── tooltip Rectangle  (when tooltip != "" && hovered && !disabled)
```

### Why the knob carries depth, not the track

A drop-shadow on the track would attach the entire pill to the surface beneath it — visually disconnected from the iOS aesthetic where the knob *floats* slightly above the track. The track wants to read as a recessed channel; the knob as a tactile, liftable disc. Putting depth on the knob mirrors iOS's actual visual model:

- Knob: outset drop-shadow (Depth global) + gradient face + highlight + optional `thickness` extrusion.
- Track: flat fill, subtle inner-edge border, **no shadow**. The off-track color (`Theme.border`) is one shade darker than the off-track background would be — that's the recessed-channel cue. Faking a true inner shadow would require an inset sublayer; deferred until v2 or until visual review demands it.

### Why two layers on the knob, not one

Same rationale as Button: the two-layer `surface + face` structure is what makes `thickness` read as physical depth. A single-disc knob with `thickness` would have nowhere to depress *into*. With the skirt + face split, pressing slides the face down into the skirt by 70% of thickness. The skirt's `#f0f0f0` becomes the visible side wall during press, and at rest (thickness > 0) it provides the bottom-edge depth cue.

When `thickness == 0`, the face fully covers the skirt — visually a single white disc. No cost.

### Track color resolution

```
property <color> track-on-bg:
      root.tone == Tone.default     ? Theme.success
    : root.tone == Tone.primary     ? Theme.primary
    : root.tone == Tone.success     ? Theme.success
    : root.tone == Tone.warning     ? Theme.warning
    : root.tone == Tone.destructive ? Theme.destructive
    : root.tone == Tone.info        ? Theme.info
    : root.tone == Tone.muted       ? Theme.muted-foreground
    : Theme.success;

property <color> track-off-bg: Theme.border;   // iOS separator gray (#c6c6c8)

property <color> track-bg: root.on ? root.track-on-bg : root.track-off-bg;
```

Animated:
```slint
animate background { duration: Animation.fast; easing: ease-out; }
```

### Press / hover / disabled

```
property <bool> visually-pressed: !root.loading && touch.pressed;

opacity:
      root.disabled         ? 0.5
    : root.visually-pressed ? 0.92   // subtle — slide is the dominant feedback
    : 1.0;
```

Note that `on` does **not** feed `visually-pressed` here (unlike Button's `checkable && checked` path). The toggle's state is captured by the knob's *position*, not by a depressed look. The face's press-dip applies only to live tactile press, not to the on/off resting state.

### Loading

- `knob-icon` swaps to `IconFont.resolve("loader")`.
- Rotation via `transform-rotation: mod(animation-tick(), Animation.spinner-period) / Animation.spinner-period * 360deg`. `transform-origin` is set to the glyph's center (HANDOVER quirk #3).
- `TouchArea.enabled = false`. The keyboard `FocusScope`'s key-pressed handler also gates on `!loading`.
- `on` is **not** changed by entering loading. The consumer is responsible for flipping `on` once their async work completes; until then, the toggle stays at its pre-loading value.
- `accessible-enabled = !disabled && !loading` so AT clients see the toggle as temporarily unavailable.

### Tooltip

Same render as Button/IconButton — Rectangle above the track, `Theme.tooltip-*` colors, soft drop shadow. Always above the track (never below). Tooltip x is centered on the track, not on the entire control width (the label column doesn't move the tooltip).

### Accessibility

```
accessible-role: switch;        // Slint's distinct AccessibleRole.Switch
accessible-label:
      root.aria-label != "" ? root.aria-label
    : root.tooltip    != "" ? root.tooltip
    : root.label      != "" ? root.label
    : "Toggle";
accessible-checkable: true;     // a switch is by definition checkable
accessible-checked:   root.on;
accessible-enabled:   !root.disabled && !root.loading;
accessible-action-default => {
    root.on = !root.on;
    root.toggled(root.on);
}
```

**Cascade rationale.** Same shape as Button/IconButton with `label` replacing IconButton's `icon` as the next-to-last fallback. Toggle's typical configuration has a visible `label`, so the cascade reaches `label` quickly. When `label == ""` (a bare toggle inside a row that has its own caption), the consumer is expected to set `aria-label` or `tooltip`.

**Debug surfacing.** When `debug-bounds: true` AND all three of `aria-label`, `tooltip`, `label` are empty, a 6×6px magenta dot renders at the top-right corner of the track. Same condition as Button — `description` is **not** part of the cascade or the debug condition (it's a caption, not a name; speaking it as the accessibility name would be misleading).

---

## Depth integration

Identical caller pattern to IconButton's, applied to the knob:

```slint
property <Elevation> eff-level:     Depth.bumped(root.shadow-elevation, touch.has-hover);
property <bool>      apply-shadow:  Depth.applies(root.elevated, root.disabled, root.shadow-elevation);
property <length>    eff-magnitude: Depth.magnitude(root.eff-level);

knob-surface := Rectangle {
    drop-shadow-blur:     root.apply-shadow ? Depth.blur(root.eff-level) : 0px;
    drop-shadow-offset-x: root.apply-shadow ? Depth.offset-x(root.shadow-direction, root.eff-magnitude) : 0px;
    drop-shadow-offset-y: root.apply-shadow ? Depth.offset-y(root.shadow-direction, root.eff-magnitude) : 0px;
    drop-shadow-color:    root.apply-shadow ? Depth.color-of(root.eff-level, root.shadow-color) : #00000000;
    ...
}
```

The hover-elevation bump (`Depth.bumped`) applies because `touch.has-hover` reads true anywhere on the track or label — both are inside the TouchArea. This means hovering the label nudges the knob's shadow up a step, providing a subtle "this whole control is reactive" cue.

`thickness` and `press-animation` are visual-structure properties on the knob-face Rectangle (its `y` and `height` bindings), not shadow properties. They don't pass through `Depth`.

---

## Globals consumed

`Theme`, `Typography`, `Sizes`, `Radius` (transitively via `Spacing.md` for column gap; not directly), `Spacing`, `Animation`, `Locale`, `IconFont`, `Depth`.

Not consumed: `CurrencyFormat` (not numeric).

---

## Acceptance criteria (visual validation gate)

Toggle is done when **every** cell of the matrix below renders correctly in `previews/toggle.slint` and in the playground section:

- **States × sizes:** off and on, across `sm / md / lg` — knob positions visibly correct, animation smooth.
- **Tones (7):** `default → success`, `primary`, `success`, `warning`, `destructive`, `info`, `muted` — track-on color visibly distinct in each.
- **Interaction states:** rest, hover (knob shadow bumps, track tint deepens slightly via tone), pressed (knob face dips when thickness > 0, opacity 0.92), focus (focus-ring renders around the entire control, label + track), disabled (opacity 0.5, no hover/press response), loading (loader spins inside the knob, taps blocked).
- **Knob icons:** `on-icon: "check"`, `off-icon: "x"` — correct icon visible per state, cross-fade absent (instant swap is acceptable since the slide masks it).
- **Label + description layout:** LTR — column right of track; RTL — column left of track. Long descriptions wrap. No label/description → bare track.
- **Depth:** at least one preview row varies `shadow-elevation` from `none` through `xl`, and one varies `thickness` from `0` through `6px`. Press-animation visibly depresses the knob face when thickness > 0.
- **`height-override`:** at least one preview with `height-override: 48px` showing the proportional scale-up (track becomes 79px wide, knob 44px diameter).
- **RTL:** Locale.rtl: true flips the column to the opposite side of the track. The knob's on/off positions do **not** flip — "on" is always physical-right of the track regardless of locale (a switch's geometry is independent of reading direction; this matches iOS behavior in Arabic locales).
- **Accessibility:** debug-bounds toggle + label/aria-label/tooltip all-empty shows the magenta corner dot.

---

## Open questions deferred to Phase 1.5 / Phase 2

1. **Variant resolution as a global.** Toggle is the third call site duplicating the `tone-color` → resolved-bg derivation. After Card lands (the fourth interactive primitive needing tones), a `Variant.bg(tone, state) → color` global is the obvious extraction. Decision deferred until four call sites exist.
2. **Drag-the-knob interaction.** iOS supports tap *or* drag; Material supports drag. v1 is tap-only because Slint's `TouchArea` event model needs care to distinguish a press-with-no-movement from a press-with-drag-along-x, and getting that wrong feels worse than not supporting drag at all. Add in v1.1 once the tap path is solid.
3. **Inset shadow on the track.** True iOS toggles have a subtle inner shadow on the track that conveys "recessed channel." Slint's `drop-shadow-*` is outset-only; emulating inset costs a sublayer (a slightly larger Rectangle behind the track with a darker fill, clipped). Not worth the visual cost in v1. Revisit when Card considers inset shadows or after a Slint version with inset support lands.
4. **`on-text` / `off-text` inside the knob or on the track.** Material's "ON" / "OFF" pill is one option; iOS doesn't do text-on-track. Skipped in v1 to keep the icon-only knob clean. If POS demands localized text affordances, the design conversation reopens.
5. **Animated knob squish on press.** The current model dips the face down by 70% of thickness. An additional micro-squish (knob width grows by 1–2px during press) is an iOS detail worth considering. Costs a few derived properties and an `animate width` block. Defer until visual review on real hardware.
6. **`size: xl` for tablet hero toggles.** `lg` at 60×36 is currently the largest preset. If hero toggles become common (e.g. "TERMINAL ARMED" master switch on the main POS screen), an `xl` preset around 80×48 is the natural addition. For now, `height-override` covers this. Decide once a real consumer asks.

---

## Build order

Two commits, separated to keep the design doc reviewable in isolation. Matches the IconButton template:

### Commit 1 — `docs(abdu-slint-ui): Toggle design contract`

1. Add this file (`architecture/toggle.md`).
2. Update `HANDOVER.md` if scope decisions invalidate any prior assertions about Toggle.
3. No code changes. User reviews the doc.

### Commit 2 — `feat(abdu-slint-ui): Toggle component + preview + playground section`

1. Write `components/toggle.slint` (~350 lines expected, comparable to IconButton).
2. Re-export from `lib.slint`.
3. Write `previews/toggle.slint` covering the state × size × tone × interaction × depth matrix.
4. Write `abdu-slint-ui-playground/src/state/toggle.rs` mirroring `icon_button.rs`.
5. Register the module in `abdu-slint-ui-playground/src/main.rs`.
6. Write `abdu-slint-ui-playground/ui/sections/toggle.slint` exposing every public property as a control.
7. Wire the section into the sidebar tile list in `ui/playground.slint`.
8. `cargo check` (library) + `cargo build` (playground) clean.
9. User runs the playground, exercises the matrix, confirms visual quality.

---

## Risks

- **Knob x-slide animation interacts with `height-override` resolution.** Both `track-width` and `knob-diameter` are derived from `track-height`; the knob's `on-x` depends on `track-width - knob-diameter - gap`. If any of these are evaluated in the wrong dependency order, the animation snaps instead of slides. Mitigation: all four derived properties (`resolved-track-height`, `resolved-track-width`, `resolved-knob-diameter`, `resolved-on-x`) are pure declarative properties read from the same source — Slint's binding system handles the order. Verify in preview with `height-override` toggled live.
- **Focus-ring positioning around an asymmetric control.** The focus-ring should encompass the track (and optionally the label column). The simplest path is a single Rectangle outside the entire `HorizontalLayout`. Risk: if the layout's intrinsic size changes (e.g. long description wraps to two lines), the ring needs to follow. Mitigation: focus-ring is positioned via `width: parent.width + 2 * Sizes.focus-ring-offset`, `height: parent.height + 2 * Sizes.focus-ring-offset` — declarative, follows automatically.
- **Loading spinner inside the knob is small.** At `sm` size the knob is 22px and the spinner glyph would render at ~11px (knob-diameter × 0.5). Borderline legible. Mitigation: accept for v1 — toggles in the `sm` size are typically inline-in-rows where the spinner's smallness is contextual. Revisit in v1.1 if POS feedback flags it.
- **Tone × on-track contrast on `muted`.** `Theme.muted-foreground` (#8e8e93) as a track-on color is low-contrast against the off color (#c6c6c8). A `muted`-toned toggle would look "off-ish" even when on. Mitigation: document `Tone.muted` on Toggle as "soft on-state — use for toggles where on/off distinction is intentionally subdued (e.g. cosmetic preferences)." If real consumers find it confusing, drop `muted` from Toggle's supported tone set in v1.1.
- **AT-SPI mapping of `accessible-role: switch`.** Slint exposes the role to the platform, but Linux AT-SPI's `switch` semantics aren't universally read by every screen reader the same way as `button[checkable]`. Mitigation: also set `accessible-checkable: true` (we already do), which AT clients use as a secondary signal. Cross-platform AT testing is out of scope for v1; flag for the Phase 3 accessibility audit.
