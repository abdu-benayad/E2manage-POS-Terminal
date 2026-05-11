# abdu-slint-ui — Roadmap

> The phased plan from "design draft" to "library shipped and integrated into the POS." Each phase has a concrete goal, the inputs it requires, the outputs it produces, the validation gate that closes it, and the decision point that follows.

---

## Where we are today

| Artifact                              | State                                                                |
| ------------------------------------- | -------------------------------------------------------------------- |
| `README.md` — design contract          | **Done** (v0). 14 primitives catalogued, 8 globals defined, philosophy stated, LTR-atomic rule documented. Needs one revision pass for typed-enum / richer-API alignment with CLAUDE.md. |
| `CLAUDE.md` — construction discipline  | **Done** (v0). Overrides to user-wide rules, Slint conventions, validation methodology, API stability rules. |
| `ROADMAP.md` — this document           | **In progress.** Establishes phase order and decision gates.        |
| `IMPL.md` — per-phase implementation   | **Not started.** Written after this roadmap is agreed.              |
| Code                                                       | **None.** One spike exists at `ui/spike/shadcn_button.slint` outside this library; it'll be re-implemented properly during Phase 1. |
| `abdu-slint-ui-playground/` — interactive catalog crate     | **Not yet created.** Sibling crate, developed in parallel with the library starting Phase 1. |
| License decision                                            | **Pending.**                                                          |
| Crate location decision                                     | **Pending.**                                                          |

The library has a design contract and construction rules. It has no decisions about where it physically lives, what license it ships under, and no code.

---

## Document order

Documents are written and reviewed in this order. Code follows the docs, never precedes them.

1. **`README.md`** — *what to build*. The design contract.
2. **`CLAUDE.md`** — *how to build it*. The construction discipline.
3. **`ROADMAP.md`** — *when to build what, in what order*. (You are here.)
4. **`IMPL.md`** — *the per-phase step-by-step plan*. Written after this roadmap is agreed.
5. **Code** — produced one phase at a time, with the IMPL doc as the playbook.

---

## Phases

```
Phase 0: Pre-flight             ─── settle decisions, revise README, write IMPL
Phase 1: Foundation             ─── globals + 5 foundation components + smoke test
   │
   ▼ (decision gate: API survives smoke test?)
Phase 2: Complete primitives    ─── remaining ~9 primitives, all of v0.1
   │
   ▼ (decision gate: API stable enough to freeze?)
Phase 3: v1.0 release            ─── API freeze, docs pass, version tag
   │
   ▼ (decision gate: ready to integrate?)
Phase 4: POS integration         ─── coordinated refactor of e2manage-pos-terminal
   │
   ▼
Phase 5+: Future work             ─── v1.x additive primitives, v2 overlay subsystem
```

---

## Phase 0 — Pre-flight

**Goal:** Settle every decision that blocks code, and produce the implementation playbook.

**Inputs:**
- README.md (design contract) — done
- CLAUDE.md (construction discipline) — done

**Outputs:**
1. **Decision: license.** MIT, Apache-2.0, or MIT-OR-Apache-2.0 dual. Recorded in README, applied to file headers later.
2. **Decision: crate location.** Workspace member (`crates/abdu-slint-ui/`) or sibling directory (`abdu-slint-ui/`). Determines `Cargo.toml` structure.
3. **README revision pass.** Update component tables to use **typed enums** instead of strings (per CLAUDE.md §5). Expand interactive-component property surfaces to the textbook 15–25 range. Resolve the README's currently-open questions (1–6 in "Open design questions" section).
4. **IMPL.md.** Per-phase step-by-step plan with file-level granularity. For Phase 1: which globals to write in what order, then which components, then how the smoke test is constructed.

**Validation gate:**
- All three pending decisions have explicit, recorded answers
- README's "Open design questions" section is empty or reduced to items genuinely deferred to v1.x
- IMPL.md describes Phase 1 to the file-creation level

**Decision point at the end of Phase 0:** Are we confident enough in the design to write code, or does another design iteration need to happen?

**Effort:** small (1 working session of design discussion + revision).

**Risks:**
- Revising the README may surface contradictions in the CLAUDE.md overrides. If so, revise CLAUDE.md too before continuing.

---

## Phase 1 — Foundation

**Goal:** Build the globals plus the smallest set of primitives that can render a realistic screen, and validate the API against one real POS screen before continuing.

**Inputs:**
- Phase 0 outputs (decisions made, README revised, IMPL.md written)

**Outputs:**

### 1.1 Globals (8 files)

All globals defined with sensible defaults so the library renders in `slint-viewer` standalone, before any Rust integration exists:

- `globals/theme.slint` — semantic colors, shadow scale, shape tokens (button-shape, card-shape, icon-button-shape)
- `globals/typography.slint` — font family, size scale, weight scale
- `globals/spacing.slint` — spacing scale
- `globals/radius.slint` — radius scale
- `globals/sizes.slint` — standard heights
- `globals/animation.slint` — durations, easings
- `globals/locale.slint` — RTL bool, locale code, directional helpers (`chevron-end`, `arrow-start`, etc.)
- `globals/currency-format.slint` — currency code, symbol, position, decimals

### 1.2 First-batch components (5 primitives)

Selected because (a) they have the simplest dependencies and (b) together they can compose a settings screen:

- `components/button.slint` — the foundation, all variants and sizes
- `components/icon-button.slint`
- `components/toggle.slint`
- `components/card.slint`
- `components/key-value-row.slint`

Each one ships with:
- The component file (`components/{name}.slint`)
- A preview file (`previews/{name}.slint`) showing every variant × size × state × locale for fast dev iteration
- Doc comments on every public property
- Visual validation passed in `slint-viewer`
- A corresponding section in the playground app (see 1.3)

### 1.3 Playground app — initial shell + 5 sections

The sibling crate `abdu-slint-ui-playground` comes online in Phase 1. Initial deliverables:

- Cargo crate skeleton (`Cargo.toml`, `src/main.rs`, `build.rs`)
- Main window: sidebar (component list) + preview pane + property-controls panel + code-snippet panel + global toolbar (theme shape, locale, currency)
- One section per Phase-1 component, with every public property exposed as an interactive control
- Playground builds, runs, and demos all 5 components interactively
- Global toolbar applies live to every section (theme/locale/currency switches re-render the active preview)

By the end of Phase 1, **the playground is the way to look at what's been built**.

### 1.4 Smoke test example

`examples/settings-display.slint` — `ui/screens/settings/display.slint` (700 lines, the worst boundary-violation offender) rewritten against the new primitives. Lives in this library's examples directory, **does not touch the POS source.** Its purpose: prove the API works for a real screen before more components get built on the same assumptions.

**Validation gate (decision point):**
- All 8 globals exist and render with defaults
- All 5 components render correctly in every state × locale via their preview files
- The playground app builds, runs, and demos all 5 components interactively, with global toolbar controls (theme/locale/currency) working live
- `examples/settings-display.slint` renders correctly and is meaningfully smaller than the original 700 lines (~150-250 lines expected)
- The smoke test surfaced no API design errors *that the team isn't prepared to live with*

**Decision point at end of Phase 1:**
- If the API survives the smoke test → proceed to Phase 2
- If specific components were awkward to compose with → revise those components and their previews before continuing
- If the architecture itself was wrong → return to Phase 0 (worst case)

**Effort:** medium (5–8 sessions, depending on how iterative the smoke test feedback gets).

**Risks:**
- The 8-prop limit removal might cause early components to under-specify their API; the smoke test should catch this and force expansion.
- Slint quirks not yet known: rendering edge cases, focus behavior, RTL behavior in `HorizontalLayout`. Surface them now while the surface is small.

---

## Phase 2 — Complete primitives

**Goal:** Build the remaining v0.1 components against the validated foundation.

**Inputs:**
- Phase 1 complete and smoke-test-passed
- Any component-design revisions from the Phase 1 decision point

**Outputs:**

Remaining 9 components per the README catalog (assuming order will be revised in Phase 0):

- `back-button` — locale-aware
- `option-tile` — selectable tile in radio groups
- `chip` — static label badge
- `status-pill` — state-aware with pulse animation
- `section-card` — header + slot
- `form-row` — label + control + helper/error slot
- `money` — LTR-atomic currency display
- `quantity` — LTR-atomic value + unit
- `money-input` — numeric input with currency

Each one ships with the same quad as Phase 1: **component + preview + playground section + doc comments + visual validation pass.** The playground catalog grows section-by-section as primitives land.

Additional smoke tests as appropriate:

- `examples/z-report.slint` — covers `Money`, `KeyValueRow`, `SectionCard`, `StatusPill` together
- `examples/payment-cash.slint` — covers `MoneyInput`, `Button` (loading state), `FormRow`
- `examples/return-items.slint` — covers `Quantity`, `Card`, `Chip`, `OptionTile`

These exercise different component subsets and surface different API gaps.

**Validation gate (decision point):**
- All 14 primitives exist and pass their preview matrix
- At least 3 example screens render correctly using only library primitives
- No component has a TODO comment, debug code, or hardcoded magic value (everything goes through globals)
- README is up to date with any properties added during implementation

**Decision point at end of Phase 2:**
- API surfaces stable across the 3+ example screens → freeze to v1.0
- Some component still feels under-specified after examples → one more pass
- Major shape of API needs to change → release as v0.x with a longer settling period

**Effort:** medium-large (8–14 sessions). Components 6–14 should go faster than 1–5 because the patterns are established.

**Risks:**
- Component fatigue: writing 9 primitives in a row is repetitive. Mitigate by interleaving example-screen work between batches of 3 components.
- The `MoneyInput` component touches Slint's focus and input model — likely the hardest to get right. Tackle it last so prior experience helps.

---

## Phase 3 — v1.0 release

**Goal:** Freeze the public API and ship.

**Inputs:**
- Phase 2 complete

**Outputs:**

1. **API freeze.** All property names, types, defaults, callback signatures locked. Any change after this requires a major version bump.
2. **Documentation pass.**
   - Every public property has a doc comment
   - Every component has a top-of-file description
   - README updated to v1.0 state (no "design draft" disclaimer)
   - ROADMAP updated to show Phase 3 closed
3. **Version tag.** `v1.0.0` recorded in `Cargo.toml`, git tagged.
4. **Playground v1.0 release.** `abdu-slint-ui-playground` version-bumped to match the library. Every public component has a polished playground section. The playground binary becomes the canonical exploration tool, shippable alongside the library.
5. **Compatibility statement.** Slint version pinned. Stated explicitly.
6. **License files** present in repo root (`LICENSE-MIT`, `LICENSE-APACHE`, or whichever combination is chosen in Phase 0).
7. **AboutSlint attribution** decision: the library itself doesn't display attribution (it's the consumer's job), but documented in README how consumers comply.

**Validation gate:**
- All v0.1 features still working after the doc-comment pass (no accidental edits)
- A grep for `TODO`, `FIXME`, `XXX`, `debug!`, hardcoded color/size literals returns clean
- README, CLAUDE.md, ROADMAP.md, IMPL.md all consistent with reality

**Decision point at end of Phase 3:**
- Integrate into POS now (Phase 4), or accumulate more components first?
- For our use case, the answer is "integrate now" — the POS refactor *is* the goal that motivated this library.

**Effort:** small (1–2 sessions). Documentation polish, no new functionality.

**Risks:**
- Discovering a v0.x bug during the doc pass that requires a behavior change. Decide whether to fix and ship v1.0, or ship v0.99 with the bug and fix in v1.0.1.

---

## Phase 4 — POS integration

**Goal:** Refactor the e2manage POS terminal to consume `abdu-slint-ui`. This is the actual product goal that this library exists to serve.

**Inputs:**
- v1.0 of `abdu-slint-ui` shipped
- A coordinated work plan for the POS refactor (separate from this library's roadmap — likely lives at `e2manage-pos-terminal/architecture/UI_REFACTOR.md`)

**Outputs:**

1. POS depends on `abdu-slint-ui` via path or git
2. POS's `Cargo.toml` gets the `AboutSlint` attribution requirement satisfied (or "Made with Slint" badge added)
3. POS's 8 environment globals are populated from Rust at startup
4. All 36 screens refactored against library primitives, in batches:
   - Auth screens (4) — likely first, simpler
   - Settings screens (12) — biggest win, smallest screen size
   - Payment screens (5)
   - Reports (2)
   - Returns (3)
   - Shift (2)
   - Checkout (2)
   - Receipt (1)
   - Conflicts (2)
   - Draft (2)
5. Old custom components in `ui/components/` deleted (replaced by library primitives)
6. Old `theme.slint` retired (replaced by library globals populated from Rust)
7. POS visual regression checked end-to-end

**Validation gate:**
- POS builds and runs
- Every screen renders correctly in Arabic and English
- No screen imports from the old `ui/components/` directory
- Estimated screen-code reduction: 33K lines → ~10K lines per earlier analysis

**Decision point at end of Phase 4:**
- Library complete and integrated. Library evolution moves to Phase 5 cadence (small, additive).

**Effort:** large (10–20 sessions for 36 screens, accelerated by the fact that most screens follow patterns established in the first 5).

**Risks:**
- Untested edge cases on specific screens surface late. Mitigate by doing the simplest screens first and the most-complex screens (checkout, z_report) only after the pattern is well-established.
- POS-specific domain components (`CartItem`, `ProductTile`, `OperatorBadge`) get refactored to compose library primitives — that's domain work outside the library scope but inside this phase's work.

---

## Phase 5+ — Future work

After v1.0 integration, the library evolves additively. Tentative roadmap:

### v1.1 — More numeric primitives

- `Code` — IDs, account numbers (LTR-atomic, monospace font)
- `Timestamp` — date+time display (LTR-atomic in RTL contexts)
- `Percent` — convenience wrapper for `Quantity { unit: "%" }`

### v1.2 — Form & input expansion

- `Input` — general text input (the open question from README #1)
- `Slider`
- `ProgressBar`
- `Skeleton` — loading placeholder

### v2.0 — Overlay subsystem

Requires a focus and z-index discipline that doesn't exist in v1. Likely a significant Slint upgrade or careful workarounds.

- `Dialog` / `Modal`
- `Dropdown` / `Combobox`
- `Toast`
- `Tooltip`
- `Popover`
- `DatePicker`
- `Tabs`

---

## Effort summary (rough orders of magnitude)

| Phase | Description                                                          | Effort         | Cumulative |
| ----- | -------------------------------------------------------------------- | -------------- | ---------- |
| 0     | Pre-flight (decisions, IMPL doc)                                      | small (1 sess) | 1          |
| 1     | Foundation + playground shell + 5 sections + smoke test               | medium (7–10)  | 8–11       |
| 2     | Remaining primitives + playground sections + more examples            | medium (10–16) | 18–27      |
| 3     | v1.0 + playground v1.0 release                                        | small (1–3)    | 19–30      |
| 4     | POS integration                                                       | large (10–20)  | 29–50      |

Total to a fully-integrated POS: roughly 29–50 focused work sessions, conservatively 70–120 hours of senior-developer time. This assumes one developer working with reasonable continuity, no major framework surprises, and no scope changes mid-flight.

Time estimates are rough orders of magnitude, not commitments. They exist to make trade-offs visible (e.g., "is this 60 hours of work justified vs. retuning theme.slint inside the existing POS?"). If the answer is no, the plan should change.

---

## Decision points to revisit

These are explicit re-evaluation moments built into the plan:

1. **End of Phase 0:** Is the design (README + CLAUDE.md) coherent enough to write code against? If not, iterate before Phase 1.
2. **End of Phase 1 (smoke test):** Did the API survive contact with a real screen? If not, revise specific components before Phase 2.
3. **End of Phase 2:** Is v0.1 stable across multiple example screens? If not, do another iteration before Phase 3.
4. **End of Phase 3:** Should we integrate into the POS now, or build more primitives first? For this project, "now" is the right answer.
5. **End of Phase 4:** Library is integrated. Time to move to Phase 5 cadence — small, additive, slower.

At any of these gates, the legitimate answer to "should we continue?" can be **no, stop and revise** — that's the point of having the gates.

---

## What this roadmap is not

- **Not a calendar.** No wall-clock dates. Effort estimates are work-time, not elapsed-time.
- **Not a commitment.** Phases can be paused, reordered, or cancelled if priorities shift.
- **Not exhaustive.** Phase 5 is sketched; details emerge after Phase 4.
- **Not a substitute for IMPL.md.** This document is about *phase ordering*; IMPL.md is about *file-level implementation steps within a phase*.
