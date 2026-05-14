# Session Handoff — KeyValueRow Rewrite

> Written 2026-05-14 at the end of a 500k-token session. The fresh session picks up here. Delete this file after task 7 ships.

## Read in this order

1. **`CLAUDE.md`** (`abdu-slint-ui/CLAUDE.md`) — project discipline, overrides on the user-wide CLAUDE.md.
2. **`architecture/segment-pattern.md`** — canonical pattern reference. The seven invariants, the primitive family, the show:bool convention, three Slint 1.14 idioms banked through the prior session.
3. **`architecture/key-value-row.md`** — KeyValueRow design doc. 18 properties, row-level derived state, locked-height rule, status-dot-not-pill decision, debug-bounds-is-row-only-in-v1.0.
4. **`architecture/key-value-row-impl.md`** — the 8-phase plan with verification gates and commit conventions. **This is the doc to execute.**
5. **`architecture/screenshots/README.md`** — visual evidence index. Each screenshot ties to a doc section.

Skim only if needed:
- `architecture/segment-pattern-consultation.md` — open questions for the external reviewer; consultation hasn't been sent yet (parallel-track item, not blocking the rewrite).
- `architecture/key-value-row-material-parity.md` — defers slice 2 (`@children` trailing slot) and slice 3 (`interactive` + `clicked()`) to post-v1.0.

## Where we left off

**Status: All planning and foundation work done. Task 7 (execute the 8-phase IMPL) is unblocked and ready to start.**

Completed tasks (in order):

1. ✓ Banked the Slint 1.14 Text-inheritance gotcha in `segment-pattern.md`
2. ✓ Shipped `SegmentColumn` (private cell primitive + preview + screenshot)
3. ✓ Shipped `Badge` (private decorator + preview + screenshot)
4. ✓ Updated `key-value-row.md` (18 properties, segment-pattern composition, all 4 design-doc fixes from review)
5. ✓ Deleted broken cluster-pattern KeyValueRow code (`components/key-value-row.slint`, `previews/key-value-row.slint`, `abdu-slint-ui-playground/ui/sections/key-value-row.slint`). Both crates still compile.
6. ✓ Wrote `key-value-row-impl.md` (8-phase plan, branch-per-phase commits, single PR at end, debug-bounds-decision settled as row-only for v1.0)

Pending:

7. **Execute task 7** — run the 8-phase IMPL.

## What's in the repo right now

**Library (`abdu-slint-ui/`) — compiles cleanly:**

- `components/_segment.slint` ✓ shipped
- `components/_segment-column.slint` ✓ shipped
- `components/_badge.slint` ✓ shipped
- `components/key-value-row.slint` — **does not exist** (deleted in task 5; rewrite recreates it in Phase 2)
- `lib.slint` — KeyValueRow export is commented out with a restoration note; Phase 2 restores it
- `enums.slint` — has `UnitPosition`; **missing `DisclosureIndicator`** (Phase 1 adds it)
- `globals/typography.slint` — `font-family-monospace: "DejaVu Sans Mono"` already present (added during pre-IMPL cleanup)
- `globals/sizes.slint` — `icon-xs: 12px` already present (no change needed)

**Previews:**

- `previews/_segment.slint`, `previews/_segment-column.slint`, `previews/_badge.slint` ✓ shipped
- `previews/key-value-row.slint` — **does not exist** (Phase 7 creates it)

**Playground (`abdu-slint-ui-playground/`) — compiles cleanly:**

- `ui/playground.slint` — KeyValueRow sidebar tile, section mount, and import all replaced with restoration-marker comments. Phase 8 reverses these.
- `ui/sections/key-value-row.slint` — **does not exist** (Phase 8 recreates it)

**Architecture docs:**

- All canonical docs current (segment-pattern.md, key-value-row.md, key-value-row-impl.md).
- Screenshots in `architecture/screenshots/` with a README index.
- Material-parity scope doc (`key-value-row-material-parity.md`) covers deferred slices.

## Phase plan in one screen

| Phase | Goal | Key step | Verification gate | Commit |
|---|---|---|---|---|
| 1 | Types and tokens | Add `DisclosureIndicator` enum to `enums.slint`, re-export from `lib.slint`. | `cargo check` | `feat(abdu-slint-ui): KeyValueRow IMPL Phase 1 — DisclosureIndicator enum` |
| 2 | Public API skeleton | Create `components/key-value-row.slint` with 18 properties declared; restore export in `lib.slint`. | `cargo check` | Phase 2 — public API skeleton (18 properties) |
| 3 | Row-level derived state | Add every derived property from the design doc. | `cargo check` — every token must resolve | Phase 3 — row-level derived state |
| 4 | LTR + RTL branches | Outer `HorizontalLayout` with both branches; cells composed from Segment/SegmentColumn. | **Pause for review.** Toggle `Locale.rtl` in temporary preview; verify flip; screenshot. | Phase 4 — LTR/RTL branches with cell composition |
| 5 | Locked height, density, divider | Apply sizing rules + show-divider Rectangle. | Locale-stable height verified; density variants render. | Phase 5 — sizing, density, divider |
| 6 | Tooltip + debug-bounds | Tooltip popup gated on `tooltip != ""`; debug-bounds is **row-only** (2px magenta border on root). | **Pause for review.** Tooltip popup + debug-bounds work; screenshot. | Phase 6 — tooltip + debug-bounds |
| 7 | Regression preview | Create long-lived `previews/key-value-row.slint` with 13 sections (one per acceptance criterion). | Every section renders correctly; locale + dark-mode toggles work. | Phase 7 — regression preview |
| 8 | Playground section restoration | Recreate `ui/sections/key-value-row.slint`; restore three integration sites in `ui/playground.slint`. | Playground compiles; KeyValueRow section mounts; every property control works. | Phase 8 — playground section restoration |

**Pause points (send screenshot, await go-ahead before continuing):** after Phase 4 and after Phase 6.
**Other phases:** continuous; screenshots committed but no human-in-the-loop pause.

After Phase 8 passes acceptance criteria, open a single PR from `feature/keyvaluerow-segment-rewrite` to `main`.

## Important decisions made (don't relitigate)

These were settled in the prior session. The fresh session should **proceed on these decisions** unless the user explicitly reopens them.

1. **Structural pattern: segment-as-cell.** See `segment-pattern.md`. Empirically verified, not theoretical.
2. **Foundation primitives: Segment, SegmentColumn, Badge.** Already shipped and screenshotted.
3. **Status indicator: dot, not pill.** A `Segment` with `text: "●"`, no Badge wrapping. Status size = `Sizes.icon-xs` (12px).
4. **debug-bounds: row-only in v1.0.** Per-cell outlines deferred. Escape hatch (extend Segment with `debug-outline: bool`) reserved for when there's cause.
5. **wrap stays row-global** (one bool, not split into `label-wrap` + `value-wrap`).
6. **description relaxes the height lock** when non-empty (same effect as `wrap: true`).
7. **Label cell is always a `SegmentColumn`** (containing label primary + description secondary). When `description == ""` the secondary Segment self-zeros and the column visually collapses to single-line. No 4-way row branching.
8. **One outer `if Locale.rtl` branch** per Invariant 4. Cells declared twice (once per branch), each declaration carries its own `align-h`.
9. **Branch + commit-per-phase, single PR at end.** Branch name `feature/keyvaluerow-segment-rewrite`.
10. **Commit message format:** `feat(abdu-slint-ui): KeyValueRow IMPL Phase N — <one-line summary>`. Match the existing project commit-history style.

## Three Slint 1.14 idioms banked during the prior session

Surfaced through real iteration; documented in `segment-pattern.md`. Future code authors hit these or know to avoid them:

1. **Don't inherit Text directly** — produces blank renders. Inherit Rectangle and hold a Text child.
2. **`wrap: word-wrap` needs a width-bounded parent** — otherwise the layout pass iterates and preferred-height runs away to 17000+px. Preview-time hazard.
3. **Slint reserves `color` and `border-radius` on Rectangle** — rename to `text-color` and `corner-radius` on any `inherits Rectangle` primitive that needs these as `in property` declarations.

## Compile-time gotchas to avoid

(Things that would have caused compile errors mid-IMPL if not fixed pre-IMPL.)

- `Spacing.xxs` doesn't exist. Smallest defined: `Spacing.xs (4px)`. Already audited and fixed.
- `Sizes.radius-*` doesn't exist. Radius tokens live in `Radius`, not `Sizes`. Smallest defined: `Radius.sm (6px)`. Already audited and fixed.
- `TextOverflow.elide` / `TextOverflow.clip` must be fully qualified — not bare `elide` / `clip`. Already used correctly in Segment.

## How to run things

```bash
# From abdu-slint-ui/
cargo check                                  # quick verification
slint-viewer previews/_segment.slint        # foundation regression preview
slint-viewer previews/_segment-column.slint
slint-viewer previews/_badge.slint

# From abdu-slint-ui-playground/
cargo check
cargo run                                    # interactive playground
```

Screenshot capture pattern from the prior session:

```bash
slint-viewer previews/PATH.slint > /tmp/sv.log 2>&1 &
SPID=$!
sleep 5
WID=$(xwininfo -root -tree 2>/dev/null | grep '"slint-viewer"' | head -1 | awk '{print $1}')
import -window "$WID" /tmp/screenshot.png
kill $SPID 2>/dev/null; wait 2>/dev/null
```

## Open items not in the IMPL plan

These exist as parallel/separate workstreams; don't pull them into the KeyValueRow rewrite.

- **Consultation companion** (`segment-pattern-consultation.md`) hasn't been sent to the external reviewer yet. Whenever the user sends it, feedback may land during or after the rewrite. Not blocking.
- **Slint 1.16 upgrade.** Workspace is on 1.14.1 (vendored). 1.16 is available but requires re-vendoring the offline-build sources. Deferred — not relevant to the rewrite.
- **Material-parity slices 2 and 3** (`@children` trailing slot, row-level interactivity). Documented in `key-value-row-material-parity.md`, scheduled for post-v1.0.
- **`Tooltip` and `Pressable` decorator primitives.** Named in the segment pattern's family but design deferred (PopupWindow positioning + TouchArea event-ordering sub-problems). Required for the side-toolbar feature, NOT required for KeyValueRow.

## When the fresh session opens

1. Read this HANDOFF.md (you're already doing it).
2. Then read in the order at top of this doc.
3. Mark task 7 as in_progress and start Phase 1.
4. Pause after Phase 4 and after Phase 6; send screenshots to the user; await go-ahead.
5. After Phase 8 passes acceptance criteria, propose opening the PR.
6. After the PR merges: delete this HANDOFF.md and the IMPL doc — their job is done.
