# Session handover — KeyValueRow + dark mode + Material parity plan

> Read this first. Fold into `HANDOVER.md` once you're caught up.
> Companion docs: `HANDOVER.md` (pre-session snapshot), `architecture/key-value-row.md` (KeyValueRow design contract), `architecture/key-value-row-material-parity.md` (forward plan for the next slices), `architecture/Dark-Mode.md` (dark-mode rollout plan).

---

## TL;DR

- **Phase 1 nearly closed.** Dark mode landed. KeyValueRow shipped with the `numeric → unit-position` semantic rewrite, locale-stable row-height lock, and `wrap` support.
- **Material parity plan written** (`architecture/key-value-row-material-parity.md`) — four ordered slices to expand KeyValueRow to match Material `ListTile` and SwiftUI `LabeledContent`. No code written for these yet.
- **Versioning is OFF.** Project is `v0.0.1` and stays there. No `v1.1` / `v2.0` references — use "later", "deferred", "next slice" instead.
- **One unresolved staleness:** `architecture/key-value-row.md` and `abdu-slint-ui/CLAUDE.md` still describe `numeric` as a property and "LTR-atomic numeric rendering" as a principle. Both need a doc-sync pass — see "Stale docs" below.

---

## What changed in this session

### Dark mode landed (committed `d043329`)

- New `ThemeMode { light, dark }` enum.
- `Theme.mode: in-out` property selects between parallel `light-X` / `dark-X` sub-tokens. Every public color token (`Theme.primary`, `Theme.surface`, etc.) is now a derived property that flips with mode.
- Three new color-derivation helpers on `Theme`:
  - `Theme.hover-tint(base)` — mode-asymmetric subtle shift for hover (darker in light mode, brighter in dark).
  - `Theme.press-tint(base)` — deeper variant for press feedback.
  - `Theme.skirt-tint(base)` — for two-layer Button/IconButton surface side-walls.
- Card, Button, IconButton, Toggle migrated from inline `.darker(N%)` to these helpers.
- Playground toolbar gained a `Mode: [light | dark]` ComboBox.
- Button section gained a "Mode reference" strip (`default | outline | secondary` fixed-variant Buttons) so the iOS HIG brand-color preservation (`Theme.primary` stays nearly identical across modes by design — `#007aff → #0a84ff`, only ~10 RGB units off) doesn't read as a wiring bug on the configurable Button.

### Typography defaults swapped to single names

`globals/typography.slint`:
- `font-family: "Inter"` (was `"Inter, system-ui, -apple-system, sans-serif"`).
- `font-family-ar: "Noto Sans Arabic"` (was `"Cairo, Tajawal, Noto Sans Arabic, sans-serif"`).

**Reason:** Slint's `font-family` takes a single name, NOT a CSS-style fallback list. The old comma-separated values resolved to a literal lookup, found no match, and Slint silently fell back to its default font — which usually has no Arabic glyphs, so RTL text rendered as tofu boxes. Doc-comment now explains this trapdoor.

For broad cross-system availability the library should eventually bundle a Cairo or Tajawal `.ttf` under `assets/` and import it. Deferred.

### Playground gained a `Locale: [en | ar]` combobox

The toolbar now exposes `Locale.current` (was previously only `Locale.rtl`). Lets you switch Arabic font + reading direction independently.

### KeyValueRow scaffolding committed (in `d043329`)

`components/key-value-row.slint`, `previews/key-value-row.slint`, `abdu-slint-ui-playground/ui/sections/key-value-row.slint` — all 480+ lines each, working state.

### KeyValueRow: `numeric` → `unit-position` rename (uncommitted)

This was a semantic rewrite, not just a rename. Captured in detail below.

**Old `numeric: bool`**:
- `numeric: true` → cluster stays LTR-atomic in RTL (`[icon, value, unit]` regardless of locale)
- `numeric: false` (default) → cluster mirrors with locale (`[unit, value, icon]` in RTL)
- Effect in LTR: none (cluster always `[icon, value, unit]`)

**New `unit-position: UnitPosition` (enum `{ trailing, leading }`)**:
- `trailing` (default) = standard behavior, cluster mirrors with locale so reader always reads value→unit
- `leading` = currency-prefix style, unit reads BEFORE value in both locales
- **XOR semantic**: `mirror = Locale.rtl XOR (unit-position == leading)` — works symmetrically in BOTH locales. Setting `leading` flips the cluster regardless of locale.

The component code uses `Locale.rtl == leading-flip` (XNOR — produces the right truth table; Slint supports `==` between booleans).

Files changed (all uncommitted):
- `enums.slint` — added `UnitPosition` enum
- `lib.slint` — exports `UnitPosition`
- `components/key-value-row.slint` — property renamed, TrailingCluster sub-component updated
- `previews/key-value-row.slint` — stripped `numeric: true` from routine rows; rewrote the "Numeric × RTL" demo section as "Unit-position × Locale" showing both `trailing` and `leading` rows
- `abdu-slint-ui-playground/ui/sections/key-value-row.slint` — CheckBox replaced with ComboBox over `[trailing, leading]`; caption rewritten; code-snippet emits `unit-position: UnitPosition.X;`

### KeyValueRow: locale-stable row-height lock (uncommitted)

**Problem found:** when toggling Locale en↔ar, KeyValueRow grew vertically because Noto Sans Arabic has a taller natural line-height than the Latin fallback font. The containing Card grew with it.

**Fix:** lock row height to `2 * padding-y + value-font-size * 1.6`, independent of Text natural line-height. Implemented as `preferred-height = min-height = max-height = row-total-height`.

**Side effect:** glyph clipping in extreme line-height fonts is acceptable trade — Latin/Arabic UI fonts (Inter, Noto Sans Arabic, Cairo) all fit comfortably at 1.6× font-size.

### KeyValueRow: `wrap: bool` property (uncommitted)

After user pushback on "caging the component", wrap is now first-class. No deferral.

**API:**
- `wrap: false` (default) → label and value use `wrap: no-wrap; overflow: elide;`. Long text truncates with `…`. Row stays locked-height.
- `wrap: true` → label and value use `wrap: word-wrap`. Row's height lock relaxes (`max-height` removed) so the row grows vertically. The locked height becomes a floor (single-line content still hits min-height).

**Layout change:**
- The previous LTR/RTL outer-layout pattern was `[LeadingCluster][Spacer][TrailingCluster]` with a horizontal-stretch spacer.
- New pattern: `[LeadingCluster (stretch:1)][TrailingCluster]` — LeadingCluster directly absorbs the gap via its own `horizontal-stretch: 1.0`. Spacer removed.
- LeadingCluster's inner HorizontalLayout sets `alignment: Locale.rtl ? LayoutAlignment.end : LayoutAlignment.start` so the label anchors to the reading-leading edge of the cluster.
- This is what makes elision work — without LeadingCluster stretching, long labels grew the row width instead of eliding.

### Material parity plan written (uncommitted)

Saved at `architecture/key-value-row-material-parity.md`. Plans four slices to close the gap with Material `ListTile` and SwiftUI `LabeledContent`. All architectural decisions resolved. No code written.

Plan summary (build order — smallest surgery first):

1. **`description: string`** — secondary text below label (~30 lines)
2. **`@children` slot** — trailing-widget slot for Toggle/IconButton/custom badge composition (~10 lines)
3. **`clicked() + interactive + disabled + aria-label`** — direct row interactivity stack (~80 lines, touches accessibility/tooltip/focus)
4. **`avatar-image: image` + companions** — image-based leading slot, circular, complements `label-icon: string` (~40 lines, lowest priority — defer if not POS-critical)

After all four: KeyValueRow goes from 13 properties + 0 callbacks → 18 properties + 4 callbacks + 1 children slot.

---

## Where the repo stands

### Committed (most recent first)

```
d043329 feat(abdu-slint-ui): dark mode support + KeyValueRow scaffolding
2722bf8 docs(abdu-slint-ui): KeyValueRow design contract
44a3bdc docs(abdu-slint-ui): single-script segmentation principle + post-Card status
506bf3e feat(abdu-slint-ui): Card component + preview + playground section
…
```

### Uncommitted (deltas from this session)

```
M abdu-slint-ui/components/key-value-row.slint   ← unit-position rename + height-lock + wrap
M abdu-slint-ui/enums.slint                       ← UnitPosition enum added
M abdu-slint-ui/lib.slint                         ← UnitPosition export
M abdu-slint-ui/globals/typography.slint          ← single-name font defaults
M abdu-slint-ui/previews/key-value-row.slint      ← Unit-position × Locale demo rewrite
M abdu-slint-ui-playground/ui/playground.slint    ← Locale combobox
M abdu-slint-ui-playground/ui/sections/key-value-row.slint  ← unit-position ComboBox + wrap CheckBox

??  abdu-slint-ui/architecture/key-value-row-material-parity.md  ← forward plan
??  abdu-slint-ui/HANDOVER-SESSION-NOTES.md  ← this doc
```

Plus parent-repo noise (`../.gitignore`, `../Cargo.toml`, `../README.md`, `../build.rs`, `../src/`, `../ui/`, `../tests/`, etc.) — these are NOT this library's concern; leave them out of any KeyValueRow / playground commits.

### Build status

Library `cargo check` → clean (4 expected harmless "doesn't inherit Window" warnings).
Playground `cargo check` → clean (same 4 warnings, plus 1 deprecated for KeyValueRow).
Preview file parses cleanly (verified via `timeout 3 slint-viewer …` — exit 143 = SIGTERM, no parse errors).

---

## What's next, in order

1. **Commit the uncommitted KeyValueRow expansion + dark-mode polish.** Suggested split:
   - One commit: `feat(abdu-slint-ui): unit-position semantic + locale-stable row height + wrap` (covers the rename, height-lock, wrap, layout simplification).
   - One commit: `feat(abdu-slint-ui-playground): Locale combobox + font-family single-name defaults` (covers playground.slint + typography.slint).
   - One commit: `docs(abdu-slint-ui): Material parity plan + session handover` (covers the two new architecture docs).
2. **Doc-sync pass** — `architecture/key-value-row.md` and `abdu-slint-ui/CLAUDE.md` still describe `numeric` and "LTR-atomic numeric rendering" as the principle. Rewrite both to match the new `unit-position` semantic. (See "Stale docs" below.)
3. **Begin Material parity slice 1 (`description`).** Smallest surgery, biggest UX win. Plan is in `architecture/key-value-row-material-parity.md` — all decisions resolved, just translate to code.
4. **Slices 2, 3, 4 in order** per the plan.
5. **Phase 1 smoke test** — `examples/settings-display.slint` rewriting a real POS settings screen against the five Phase-1 primitives. Tracked in `ROADMAP.md` / `IMPL.md`.

---

## Stale docs (need follow-up rewrites)

These describe `numeric` / LTR-atomic-as-default-principle and need to flip to the new framing:

- **`architecture/key-value-row.md`** — "Numeric mode and the segmentation principle" section is the load-bearing rewrite. Frame `unit-position` as a symmetric flip; demote LTR-atomic from "the principle" to "what you opt into via `unit-position: leading`". Update API table.
- **`abdu-slint-ui/CLAUDE.md`** — "LTR-atomic numeric rendering" was listed as a load-bearing principle. KeyValueRow no longer adheres to it by default. Reframe: the segmentation principle (label + value as separate Text elements) still holds; LTR-atomic is now consumer-opt-in for currency-prefix style.
- **`architecture/Dark-Mode.md`** — written as a forward plan; the work is now done. Either annotate as "implemented in commit `d043329`" or fold key learnings (e.g., iOS HIG brand-color preservation across modes is intentional, not a wiring bug) into the existing per-component design docs and delete.

---

## Non-obvious context worth preserving

1. **iOS HIG brand-color preservation.** `Theme.primary` is `#007aff` in light, `#0a84ff` in dark — visually nearly identical. This is INTENTIONAL per Apple HIG (brand colors stay constant across modes; only neutrals invert). When a user toggles Mode on a `variant: default` Button and sees "no change", that IS correct behavior. The Mode-reference strip in the Button section (`default | outline | secondary`) demonstrates this by showing neutrals flipping dramatically while the brand-blue stays put.

2. **Slint `font-family` is single-name only.** Comma-separated CSS-style fallbacks (`"Cairo, Tajawal, sans-serif"`) resolve as a literal lookup and fail silently. Documented as a trapdoor in `globals/typography.slint`.

3. **Slint's `accessible-role` is compile-time constant.** Cannot bind to a runtime property. Card and (future) interactive KeyValueRow both use a "conditional inner shim" pattern: `if interactive: Rectangle { accessible-role: button; … }`. When the condition is false, the shim doesn't exist → no entry in the AT tree. HANDOVER quirk #14.

4. **Slint's preferred-size doesn't auto-propagate through nested Rectangles.** Content-driven components must bind `root.preferred-width: inner-layout.preferred-width;` explicitly. HANDOVER quirk #15. KeyValueRow does this.

5. **Row-height lock trade-off.** KeyValueRow's `value-font-size * 1.6` multiplier is tight — works for Inter, Noto Sans Arabic, Cairo, Tajawal at body sizes. Extreme descender-heavy display fonts may clip. If you discover clipping on a target font, bump the multiplier in `row-content-height`.

6. **TrailingCluster's `ltr-order` is XNOR, not XOR.** The variable is named `ltr-order` (preserves naming continuity with the old `numeric` code) but its formula is `Locale.rtl == leading-flip`. Comment in the code calls this out.

7. **The user is the project owner, not a consumer.** Abdu makes design calls; we implement. Architecture documents are evaluated for plain-English clarity (he reads them to evaluate, not just to learn Rust syntax). Push back on bad architectural ideas with evidence, hold positions unless new evidence appears.

---

## Don't touch (still applies)

- `e2manage-pos-terminal/ui/` (the POS UI proper) — untouched until much later
- `e2manage-pos-terminal/src/` — same
- `e2manage-pos-terminal/crates/` — same
- `e2manage-pos-terminal/Cargo.toml` (workspace) — same

The library evolves in isolation. POS integration is a much later phase with its own plan.

---

## How to resume

```sh
# Verify build
cd /home/abdu/Downloads/e2manage-pos-terminal/abdu-slint-ui-playground
cargo check
cargo run    # opens the playground — KeyValueRow tile in sidebar

# Validate preview matrix
slint-viewer /home/abdu/Downloads/e2manage-pos-terminal/abdu-slint-ui/previews/key-value-row.slint

# Read in this order
abdu-slint-ui/HANDOVER-SESSION-NOTES.md   # this doc
abdu-slint-ui/HANDOVER.md                  # full project state
abdu-slint-ui/architecture/key-value-row.md  # KeyValueRow design contract (note: stale on numeric → unit-position rename)
abdu-slint-ui/architecture/key-value-row-material-parity.md  # forward plan for next slices
```

Memory check: `/home/abdu/.claude/projects/-home-abdu-Downloads-e2manage-pos-terminal/memory/` is currently empty. No persistent memory to consult.
