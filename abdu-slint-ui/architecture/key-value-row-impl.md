# KeyValueRow — IMPL

> Working document. Time-bounded — deleted or archived after the rewrite ships and the PR merges. Same lifecycle as `segment-pattern-consultation.md`.
>
> Audience: the agent executing the rewrite. The design doc ([`key-value-row.md`](./key-value-row.md)) is the *what and why*. This doc is the *how, in what order, and how to verify each step*.

---

## Scope

Implement `KeyValueRow` from the design doc, composed on the segment-as-cell pattern's foundation primitives (`Segment`, `SegmentColumn`, `Badge` — all already shipped). Restore the playground section. Restore the `lib.slint` export. Land as a single PR.

**Pre-shipped foundation (no work in this IMPL):**

- `components/_segment.slint` ✓
- `components/_segment-column.slint` ✓
- `components/_badge.slint` ✓
- `Sizes.icon-xs` global token ✓ (was already present, confirmed)
- `Typography.font-family-monospace` global token ✓ (added during pre-IMPL cleanup)

**To be added by this IMPL:**

- `DisclosureIndicator` enum in `enums.slint`, re-exported via `lib.slint`.
- `components/key-value-row.slint` — the public component.
- `previews/key-value-row.slint` — the regression preview.
- `abdu-slint-ui-playground/ui/sections/key-value-row.slint` — the interactive playground section (restoration; deleted in task 5).
- `lib.slint` export restoration.
- `abdu-slint-ui-playground/ui/playground.slint` restoration (sidebar tile + section mount + fallback-message tweak).

---

## Branching and commit convention

**Branch:** `feature/keyvaluerow-segment-rewrite`. All 8 phases land on this branch. Single PR to `main` at the end.

**Commit message format:** `feat(abdu-slint-ui): KeyValueRow IMPL Phase N — <one-line summary>`.

Why branch-per-phase + single PR:

- `main` stays compilable at every revision. Intermediate phases (e.g., Phase 2's skeleton with no logic) would compile fine but produce a non-functional component if landed on `main`.
- The PR's commit list tells the phase-by-phase story for review.
- Rollback: `git revert HEAD` (on the branch) reverts one phase; the branch itself can be abandoned without touching `main`.

**Start of work:**

```bash
git checkout -b feature/keyvaluerow-segment-rewrite
```

**End of each phase that passes verification:**

```bash
git add -A
git commit -m "feat(abdu-slint-ui): KeyValueRow IMPL Phase N — <thing>"
```

**End of work (after Phase 8 passes):**

Open PR from `feature/keyvaluerow-segment-rewrite` to `main`. PR description summarizes the 8 phases and links to `architecture/key-value-row.md` + `architecture/segment-pattern.md`.

---

## Verification discipline

Each phase ends with a **verification gate**. Do not proceed to the next phase until the current phase's gate passes. If verification fails:

1. Stop. Do not write more code in the next phase.
2. Identify whether the failure is in the phase's own work or in the assumption it inherited from a prior phase / the design doc.
3. If in the phase's own work: revert the phase's WIP changes (do not commit them), fix, re-verify.
4. If in an inherited assumption: stop and surface the issue. The design doc may need an update; this is not a "patch around it locally" situation.

The verification gates are stricter for KeyValueRow than they were for Segment/SegmentColumn/Badge, because KeyValueRow stacks on those primitives. A regression in cell behavior surfaces here, but the fix belongs upstream — not as a workaround in `key-value-row.slint`.

**Capture a screenshot at each preview-verification gate.** Drop them in `architecture/screenshots/key-value-row-phase-N.png` so the PR reviewer can walk the phase progression visually. After the rewrite merges, screenshots get consolidated into a single `_key-value-row-preview.png` and the phase screenshots get cleaned up.

**Throwaway preview discipline.** Phases 4, 5, and 6 each create a temporary preview file (`previews/_kvr-phase-N.slint`) to verify the phase's work, then delete it after capturing the phase screenshot. The temporary files have no long-term value; their job is to keep the verification surface minimal and avoid stale preview files accumulating in `previews/`. Phase 7 creates the long-lived `previews/key-value-row.slint` — that one stays.

---

## Phase 1 — Types and tokens

**Goal:** add the one type that doesn't yet exist (`DisclosureIndicator` enum). All other tokens were added during pre-IMPL cleanup.

### Steps

1. Open `enums.slint`. Add the `DisclosureIndicator` enum after `UnitPosition` (alphabetical/grouped ordering — both are KeyValueRow-specific enums; keep them adjacent).

   ```slint
   export enum DisclosureIndicator {
       none,        // no indicator
       chevron,     // `›` LTR, `‹` RTL
       external,    // `↗` LTR, `↖` RTL
   }
   ```

2. Open `lib.slint`. Add `DisclosureIndicator` to the existing enum re-export list (alongside `UnitPosition`).

### Verification gate

- `cargo check` succeeds with no new warnings.
- No new compile errors.

### Commit

`feat(abdu-slint-ui): KeyValueRow IMPL Phase 1 — DisclosureIndicator enum`

---

## Phase 2 — Row file skeleton (public API surface only)

**Goal:** create `components/key-value-row.slint` with the full 18-property public API surface declared, but with no derived state, no layout, no rendering logic. The component compiles as an exported `inherits Rectangle` shell.

### Steps

1. Create `components/key-value-row.slint`.

2. Add the file header comment (purpose, segmentation principle reference, link to `architecture/key-value-row.md` and `architecture/segment-pattern.md`).

3. Imports — pull in everything the row will need (enums, all globals, the three foundation primitives `Segment`, `SegmentColumn`, `Badge`).

   ```slint
   import { Emphasis, Tone, Density, UnitPosition, DisclosureIndicator } from "../enums.slint";
   import { Theme } from "../globals/theme.slint";
   import { Typography } from "../globals/typography.slint";
   import { Sizes } from "../globals/sizes.slint";
   import { Radius } from "../globals/radius.slint";
   import { Spacing } from "../globals/spacing.slint";
   import { Animation } from "../globals/animation.slint";
   import { Locale } from "../globals/locale.slint";
   import { IconFont } from "../globals/icon-font.slint";

   import { Segment } from "./_segment.slint";
   import { SegmentColumn } from "./_segment-column.slint";
   import { Badge } from "./_badge.slint";
   ```

4. Declare the component shell:

   ```slint
   export component KeyValueRow inherits Rectangle {
       // ===== Public API — 18 properties =====
       // Content (6)
       in property <string> label: "";
       in property <string> label-icon: "";
       in property <string> description: "";
       in property <string> value: "";
       in property <string> value-unit: "";
       in property <string> value-icon: "";

       // Typography & tone (3)
       in property <Emphasis> emphasis: Emphasis.normal;
       in property <Tone> value-tone: Tone.default;
       in property <bool> value-monospace: false;

       // Affordances (3)
       in property <bool> show-status: false;
       in property <Tone> status-tone: Tone.muted;
       in property <DisclosureIndicator> disclosure: DisclosureIndicator.none;

       // Layout & behaviour (5)
       in property <Density> density: Density.default;
       in property <UnitPosition> unit-position: UnitPosition.trailing;
       in property <bool> wrap: false;
       in property <bool> show-divider: false;
       in property <string> tooltip: "";

       // Debug (1)
       in property <bool> debug-bounds: false;

       // No derived state, no layout — Phase 3 and Phase 4 add those.
       background: transparent;
   }
   ```

5. Restore `lib.slint`'s `KeyValueRow` export. Replace the commented-out line with the live export.

### Verification gate

- `cargo check` succeeds with no new errors.
- `lib.slint` re-exports `KeyValueRow`.
- Doc-comments on every public property (skipped above for brevity; the actual file should follow the same `///` doc-comment style as Button/Card/Toggle).

### Commit

`feat(abdu-slint-ui): KeyValueRow IMPL Phase 2 — public API skeleton (18 properties)`

---

## Phase 3 — Row-level derived state

**Goal:** add every row-level derived property from `key-value-row.md → Row-level derived state` to the component. No layout, no cells — just the derived bindings the layout will reference in Phase 4.

### Steps

1. Below the public API block, add the derived-state section in the order from `key-value-row.md`:

   - Font & icon-font selection (`row-font`)
   - Density → vertical padding (`padding-y`)
   - Emphasis → typography sizes & weights (label/value/description font-sizes and weights, `icon-size`)
   - Color resolution (`label-color`, `value-default-color`, `value-color`)
   - Status indicator state (`status-glyph`, `status-color`, `status-size`)
   - Disclosure glyph (locale-aware)
   - Label-icon glyph
   - Value-cluster slot resolution (`leading-flip`, `icon-leads-value-cluster`, `value-side-a-text/font/size`, `value-side-b-text/font/size`)
   - Row sizing (`row-content-height`, `row-total-height`)

2. Match the design doc's bindings exactly. No improvisation — every binding has been spec'd.

3. Each property in its own `property <type> name: ...;` declaration. Inline comments for non-obvious computations (the XNOR slot resolution, the disclosure glyph table).

### Verification gate

- `cargo check` succeeds.
- No new warnings.
- The properties don't yet render anything (no layout), but every binding must compile against the types it references (`Density.compact`, `Theme.muted-foreground`, `IconFont.resolve(...)`, etc.). A typo in a token name surfaces here.

### Commit

`feat(abdu-slint-ui): KeyValueRow IMPL Phase 3 — row-level derived state`

---

## Phase 4 — LTR and RTL branches with the full outer layout

**Goal:** add the outer `HorizontalLayout` with both LTR and RTL branches as ordered cell sequences. Each cell — status dot, label-icon, label SegmentColumn (label + description), slack Rectangle, value-side-A, value, value-side-B, disclosure — declared twice (once per branch) per the segment pattern's row composition contract.

**Why both branches in one phase:** verification requires toggling `Locale.rtl` to confirm the flip is correct. A single-branch phase has no verifiable verification gate (per consultation with the expert).

### Steps

1. Below the derived-state block, add the row's outer layout:

   ```slint
   layout := HorizontalLayout {
       spacing: 0;                  // cells own gaps via padding-h
       padding-top:    root.padding-y;
       padding-bottom: root.padding-y;

       // ===== LTR branch — label-side leads, value-side trails =====
       if !Locale.rtl: Segment       { /* status dot */ ... }
       if !Locale.rtl: Segment       { /* label-icon */ ... }
       if !Locale.rtl: SegmentColumn { /* label + description */ ... }
       if !Locale.rtl: Rectangle     { horizontal-stretch: 1; }  // slack
       if !Locale.rtl: Segment       { /* value-side-A */ ... }
       if !Locale.rtl: Segment       { /* value */ ... }
       if !Locale.rtl: Segment       { /* value-side-B */ ... }
       if !Locale.rtl: Segment       { /* disclosure */ ... }

       // ===== RTL branch — mirror sequence =====
       if  Locale.rtl: Segment       { /* disclosure */ ... }
       if  Locale.rtl: Segment       { /* value-side-A */ ... }
       if  Locale.rtl: Segment       { /* value */ ... }
       if  Locale.rtl: Segment       { /* value-side-B */ ... }
       if  Locale.rtl: Rectangle     { horizontal-stretch: 1; }  // slack
       if  Locale.rtl: SegmentColumn { /* label + description */ ... }
       if  Locale.rtl: Segment       { /* label-icon */ ... }
       if  Locale.rtl: Segment       { /* status dot */ ... }
   }
   ```

2. Fill in each cell's property bindings using the row-level derived state from Phase 3. The cell template is `key-value-row.md`'s composition table — for example:

   - Status dot Segment: `text: status-glyph; font-family: row-font; font-size: status-size; text-color: status-color; align-h: center; padding-h: Spacing.xs;`
   - Label SegmentColumn: contains two Segments (primary `text: root.label`, secondary `text: root.description`), each with locale-appropriate `align-h` per branch (`left` in LTR, `right` in RTL).
   - The value cell: `text: root.value; font-family: root.value-monospace ? Typography.font-family-monospace : row-font; font-size: value-font-size; ...`

3. Per-branch differences boil down to `align-h` (locale-dependent text) and the position in the cell sequence. Properties travel with the cell across branches; only `align-h` differs at the call site.

### Verification gate

- `cargo check` succeeds.
- Write a temporary preview file `previews/_kvr-phase4.slint` with a single KeyValueRow instance and a button that toggles `Locale.rtl`. Run `slint-viewer previews/_kvr-phase4.slint`. Verify:
  - **LTR mode:** label and label-icon on the physical-left, value cluster (icon + value + unit) on the physical-right, all cells visible and correctly typographed.
  - **RTL mode:** same cells, mirrored — label and label-icon on the physical-right, value cluster on the physical-left. The cluster's internal order flips per `unit-position` (test both `trailing` and `leading`).
  - **No regressions:** value text does not collapse to zero (the bug class from the original cluster pattern). Both branches show all cells.
- Capture screenshots: `architecture/screenshots/key-value-row-phase-4-ltr.png` and `_phase-4-rtl.png`.
- Delete `previews/_kvr-phase4.slint` — the real preview file comes in Phase 6's auxiliary section or as a separate step before Phase 8. Do not let throwaway preview files accumulate.

### Commit

`feat(abdu-slint-ui): KeyValueRow IMPL Phase 4 — LTR/RTL branches with cell composition`

---

## Phase 5 — Locked height, density, divider

**Goal:** apply the sizing rules from `key-value-row.md → Locked row height` and add the `show-divider` Rectangle. The row now sizes correctly per `density` and stays height-stable across locales.

### Steps

1. Bind the root Rectangle's sizing properties:

   ```slint
   horizontal-stretch: 1.0;       // row stretches to fill container width
   preferred-width:  layout.preferred-width;
   min-width:        layout.min-width;
   preferred-height: root.wrap || root.description != "" ? layout.preferred-height : root.row-total-height;
   min-height:       root.row-total-height;
   max-height:       root.wrap || root.description != "" ? 99999px               : root.row-total-height;
   ```

2. Add the `show-divider` Rectangle (1px Theme.border at the row's bottom edge):

   ```slint
   if root.show-divider: Rectangle {
       x: 0;
       y: parent.height - 1px;
       width: parent.width;
       height: 1px;
       background: Theme.border;
   }
   ```

### Verification gate

- `cargo check` succeeds.
- Temporary preview verifying:
  - **Density variants** (compact / default / comfortable) — row height matches `2 × padding-y + value-font-size × 1.6`.
  - **Locale-stable height** — toggle `Locale.current` between `"en"` and `"ar"`. Row height does NOT change (the Inter→Noto Sans Arabic font swap doesn't resize the row).
  - **wrap: true** — row grows vertically with multi-line content; `min-height` floor still holds.
  - **description != ""** — same as `wrap: true` — row grows vertically; SegmentColumn shows two lines.
  - **show-divider** — 1px line at the row's bottom edge in Theme.border.
- Screenshot: `architecture/screenshots/key-value-row-phase-5.png`.

### Commit

`feat(abdu-slint-ui): KeyValueRow IMPL Phase 5 — sizing, density, divider`

---

## Phase 6 — Tooltip and debug-bounds

**Goal:** add the auxiliary affordances. Tooltip (gated on `tooltip != ""`) renders a TouchArea-driven hover popup. debug-bounds is **row-only** in v1.0 (per design doc decision — per-cell outlines deferred).

### Steps

1. **debug-bounds on the row root only:**

   ```slint
   border-width: root.debug-bounds ? 2px : 0px;
   border-color: #ff00ff;
   ```

   That's the entire debug-bounds implementation for v1.0. No per-cell wrapping. The design doc's [Debug bounds](./key-value-row.md#debug-bounds) section documents the deferral and the escape hatch (extend `Segment` with `debug-outline: bool` if a real need surfaces). For cell-level debugging during composition work, use `previews/_segment.slint`'s `Bound` helper, which already visualizes cell boundaries.

2. **Tooltip block** — gated on `tooltip != ""`, identical to the previous KeyValueRow's tooltip mechanism (per design doc → Tooltip TouchArea section):

   ```slint
   if root.tooltip != "": Rectangle {
       x: 0; y: 0;
       width: parent.width;
       height: parent.height;
       background: transparent;

       tooltip-area := TouchArea {}

       if tooltip-area.has-hover: Rectangle {
           // popup positioning, drop shadow, tooltip text — per design doc
       }
   }
   ```

   Reference the Card's tooltip implementation for the exact popup shape; it's the same pattern at a different scale.

### Verification gate

- `cargo check` succeeds.
- Temporary preview verifying:
  - **tooltip** — hover the row, popup appears above with the tooltip text.
  - **debug-bounds** — magenta 2px border on the row root. No per-cell outlines (v1.0 deferral).
- Screenshot: `architecture/screenshots/key-value-row-phase-6.png`.

### Commit

`feat(abdu-slint-ui): KeyValueRow IMPL Phase 6 — tooltip + debug-bounds`

---

## Phase 7 — Regression preview

**Goal:** create the public regression preview at `previews/key-value-row.slint`. This is the long-lived preview, not a throwaway. Same shape as `previews/_segment.slint` etc. — sections demonstrating every variant × size × state × locale combination.

### Steps

1. Create `previews/key-value-row.slint`. Use the `Section` + `Bound` helper-component pattern from the foundation previews (inherit VerticalLayout, hold Text children — don't inherit Text directly; per the pattern doc's Implementation idiom).

2. Sections (one per acceptance criterion from the design doc):

   1. **Emphasis matrix** — all 4 levels rendered in a single Card.
   2. **Value-tone matrix** — all 7 tones.
   3. **Density variants** — three side-by-side Cards, one per density.
   4. **Unit-position × Locale** — the 4-cell XNOR truth table. Include a Locale-toggle button.
   5. **Description (two-line content)** — rows with and without description; verify the row grows correctly.
   6. **Disclosure variants** — none / chevron / external in both LTR and RTL.
   7. **Status dot variants** — show-status + each status-tone color.
   8. **value-monospace** — column of numeric values, with and without monospace; verify decimal alignment.
   9. **wrap: true** — long-content rows that wrap.
   10. **show-divider** — list with dividers between rows.
   11. **Tooltip** — one row with tooltip set.
   12. **Debug bounds** — one row with debug-bounds: true.
   13. **Dark mode** — Theme.mode = ThemeMode.dark over a representative set of rows.

3. Per-section caption explaining what to verify visually.

4. Apply the Slint 1.14 idioms learned during foundation work: header Texts use `wrap: no-wrap` (or live inside a width-bounded parent), no `alignment: start` if it interacts badly, etc. The Card preview is the canonical template.

### Verification gate

- `cargo check` succeeds.
- `slint-viewer previews/key-value-row.slint` opens a non-runaway window (preferred-height matches `preferred-height: ...` set in the Window declaration; not 17000+px).
- Every section renders the variants described in its caption.
- Locale toggle in Section 4 flips the truth table correctly.
- Dark-mode toggle in Section 13 flips colors correctly.
- Screenshot: `architecture/screenshots/key-value-row-phase-7.png`.

### Commit

`feat(abdu-slint-ui): KeyValueRow IMPL Phase 7 — regression preview`

---

## Phase 8 — Playground section restoration

**Goal:** restore the playground section file (deleted in task 5). This is a **recreation from scratch**, not an edit of an existing file. Follow the restoration-site comments left in `abdu-slint-ui-playground/ui/playground.slint`.

The previous playground section (the version that lived at this path before task 5) was built for the old cluster-pattern KeyValueRow with 13 properties. The new one is built for the segment-pattern KeyValueRow with 18 properties. Some controls migrate (label, value, unit, density, emphasis, value-tone, unit-position, show-divider, wrap, tooltip, debug-bounds); five are new (description, value-monospace, show-status, status-tone, disclosure).

### Steps

1. **Create `abdu-slint-ui-playground/ui/sections/key-value-row.slint`** from scratch. Match the shape of `sections/card.slint` and `sections/button.slint` — left preview pane with live `KeyValueRow` instance + a code-snippet panel, right property-control panel with one input per public property.

2. **Restore the playground.slint integration sites** (three sites, all marked with restoration comments):

   - **Import** (around line 20): replace the `// KeyValueRowSection removed during rewrite ...` comment with `import { KeyValueRowSection } from "sections/key-value-row.slint";`
   - **Sidebar tile** (around line 166): replace the `// KeyValueRow sidebar tile removed ...` block with the actual `key-value-row-tile := Rectangle { ... }` block matching the other tiles' shape.
   - **Section mount + fallback message** (around line 318): replace the `// KeyValueRow section removed during rewrite ...` comment with `if root.selected-section == "key-value-row": KeyValueRowSection { }`, and update the fallback-message conjunction to include `&& root.selected-section != "key-value-row"`, and add "KeyValueRow" back to the fallback message's list of available sections.

3. **Property controls in the section** — one for each of the 18 properties. Use the existing patterns:

   - Strings → `LineEdit`
   - Bools → `CheckBox`
   - Enums → `ComboBox` listing every variant
   - Reference `sections/card.slint` for the layout.

4. **Live code-snippet panel** — same approach as the previous section had, but updated for the new property set. Show only properties that differ from their defaults (cleaner snippet).

### Verification gate

- `cargo check` succeeds for `abdu-slint-ui-playground`.
- `cargo run` opens the playground. Click the "KeyValueRow" sidebar tile. The section mounts.
- Every control on the right panel mutates the preview correctly.
- Toolbar's RTL toggle flips the KeyValueRow's layout.
- Toolbar's mode toggle (light/dark) re-themes the row.
- Toolbar's locale combobox (en/ar) re-fonts the row.
- Screenshot: `architecture/screenshots/key-value-row-phase-8.png`.

### Commit

`feat(abdu-slint-ui): KeyValueRow IMPL Phase 8 — playground section restoration`

---

## After Phase 8 — open the PR

1. Push the branch: `git push -u origin feature/keyvaluerow-segment-rewrite`
2. Open the PR with a description that:
   - Summarizes the rewrite (KeyValueRow re-implemented on the segment-as-cell pattern).
   - Links to `architecture/key-value-row.md` (design) and `architecture/segment-pattern.md` (foundation).
   - Lists the 18 properties.
   - Inlines or references the 8 phase screenshots.
   - Calls out the 4 new properties (description, disclosure, status-tone+show-status, value-monospace) as additive vs the v0 cluster-pattern API.
   - Notes that `lib.slint`'s KeyValueRow export has been restored.
3. PR review walks phase-by-phase via the commit list.
4. After merge: delete this IMPL doc (`architecture/key-value-row-impl.md`) — its job is done. Consolidate the phase screenshots into a single `_key-value-row-preview.png` (the final phase 7 screenshot is a reasonable canonical one) and delete the phase-N screenshots.

---

## Acceptance criteria checklist

(Mirrors `key-value-row.md → Acceptance criteria`. Verify each before opening the PR.)

- [ ] All 4 `emphasis` values render with the right typography.
- [ ] All 7 `value-tone` values color the value cluster correctly; label stays muted-foreground.
- [ ] All 3 `density` values map to the documented `padding-y`.
- [ ] `unit-position` XNOR truth table verifies in both locales (4 cells).
- [ ] `description` renders correctly; collapses cleanly when empty.
- [ ] `show-status` + `status-tone` renders status dot in the right color at the row's leading edge in both locales.
- [ ] `disclosure: chevron` and `disclosure: external` auto-flip glyphs with locale.
- [ ] `value-monospace` produces tabular alignment when columns of values stack vertically.
- [ ] `wrap: true` grows the row vertically; ellipsis on overflow when `wrap: false`.
- [ ] `show-divider` renders 1px Theme.border at the bottom.
- [ ] `tooltip` appears on hover.
- [ ] `debug-bounds` outlines every cell.
- [ ] Locale toggle (en ↔ ar) preserves row height.
- [ ] Dark mode toggle flips Theme colors correctly across all variants.
- [ ] Playground section exposes all 18 properties as interactive controls.
- [ ] Playground code-snippet panel reflects the live configuration.

---

## Known risks for the rewrite

1. **Tooltip TouchArea blocking Card clicks** — by design, a tooltip-bearing row consumes hover events, so a wrapping interactive `Card` won't receive clicks through the row. Documented in `key-value-row.md`; the playground's Card-wrapper preview (if any) needs to either set tooltip OR be interactive, not both. Phase 8 verification.

2. **Status dot color with `status-tone: muted`** — `Theme.muted-foreground` is a low-contrast color. A dot in muted-foreground may be visually invisible. Verify Phase 4 visually; if dots are too faint, the design doc may need to specify a minimum-contrast fallback.

3. **value-monospace font fallback** — `Typography.font-family-monospace` defaults to `"DejaVu Sans Mono"`. On systems without it installed, Slint falls back to its own default. Phase 7 verification: check that monospace alignment actually achieves tabular figures with the default font on the dev machine. If not, consider bundling a monospaced TTF.

4. **The 18-cell-declaration LOC count** — the row's source will be visibly longer than the previous cluster-pattern version. Tolerable per the pattern's design (see `segment-pattern.md → "The duplication question"`); the cost is bounded and the per-cell triviality is the payoff.
