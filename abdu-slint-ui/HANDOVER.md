# abdu-slint-ui — Session Handover

> Snapshot at end of **Phase 0 (design)**. Read this first when starting a fresh session.

---

## TL;DR

- **Project:** `abdu-slint-ui` — a Slint UI component library + companion playground app, built to replace the inline Slint patterns currently bloating `e2manage-pos-terminal` and provide a reusable, well-styled foundation.
- **Phase 0 status:** ✅ complete. All design decisions made, all four design docs written, one Slint spike produced.
- **Phase 1 status:** ⏳ not started. Next action: create `Cargo.toml` and begin library implementation per `IMPL.md §1.0`.
- **No code in the library yet.** One out-of-tree spike at `../ui/spike/shadcn_button.slint` demonstrates the shadcn-derived token system.

---

## Where things live

| Path                                                          | What                              |
| ------------------------------------------------------------- | --------------------------------- |
| `e2manage-pos-terminal/abdu-slint-ui/`                        | **This library** (Slint, no code yet) |
| `e2manage-pos-terminal/abdu-slint-ui-playground/`             | **Playground app** (Rust+Slint, not yet created) |
| `e2manage-pos-terminal/ui/spike/shadcn_button.slint`          | Out-of-tree spike from Phase 0    |
| `e2manage-pos-terminal/ui/`                                   | The existing POS UI (untouched in Phases 1–3) |
| `e2manage-pos-terminal/crates/`                               | POS workspace crates (untouched)  |

---

## Document set — read in this order

1. **`HANDOVER.md`** (this file) — where you are
2. **`README.md`** — design contract: what the library is, what it's not, philosophy, component catalog, environment globals, shape tokens, numeric-content rendering rule, license, distribution, project status
3. **`CLAUDE.md`** — construction discipline that **overrides `~/.claude/CLAUDE.md`** inside this directory (rich APIs allowed, composition partially suspended, additive API stability is mandatory, visual validation primary over unit tests, typed enums not strings, playground discipline)
4. **`ROADMAP.md`** — phase plan with decision gates: Phase 0 → 1 → 2 → 3 → 4 → 5+, effort estimates, validation gates
5. **`IMPL.md`** — Phase 1 precise spec at file-creation granularity: Cargo.toml, all 8 globals (every property, type, default), 9 Slint enums, all 5 Phase 1 components (every property, callback, state machine, acceptance criteria), preview file matrices, playground crate setup, playground sections, smoke-test plan, Phase 1 definition of done

---

## Decisions made in Phase 0

| Decision           | Choice                                           |
| ------------------ | ------------------------------------------------ |
| Crate location     | Sibling directory at repo root (current location)|
| License            | MIT OR Apache-2.0 (dual, Rust convention)        |
| Input component    | Yes — primitive #15 in v1                        |
| Icon system        | Bundled icon font (Phosphor or Lucide — final choice in Phase 1); components accept icon names |

## Decisions deferred to Phase 1

| Open item               | Plan                                                    |
| ----------------------- | ------------------------------------------------------- |
| Phosphor vs Lucide      | Side-by-side comparison in playground, then commit       |
| Focus-ring rendering    | Attempt on interactive primitives in v1; document final state in IMPL.md |
| Density / compact mode  | Global token `Theme.density`; per-component override TBD |

---

## What this library is and is not

**Is:** a library of living, environment-aware UI components for Slint apps. Reads theme/locale/currency from globals. Narrow event callbacks. Visual validation primary. Rich textbook-style APIs (15–25 props on interactive components).

**Is not:** a fork of `std-widgets.slint`, coupled to any domain (no `Cart`, `Product`, etc.), a translation library, theme-agnostic, currently stable.

See `README.md` for the precise contract.

---

## Phase 1 — what's next

Per `ROADMAP.md §Phase 1` and `IMPL.md §1.0–§1.7`:

### Build order

1. **Crate setup** (`§1.0`) — `Cargo.toml`, `build.rs`, `src/lib.rs`, `lib.slint`, `LICENSE-MIT`, `LICENSE-APACHE`
2. **Enums** (`§1.1`) — `enums.slint` with 9 enums (ButtonVariant, ButtonSize, Shape, Tone, Elevation, Density, Emphasis, ToggleSize, TonalSurface)
3. **Globals** (`§1.2`) — 8 files under `globals/`, each with concrete defaults per IMPL.md
4. **Components** (`§1.3`) — 5 files under `components/`, in this order: Button → IconButton → Toggle → Card → KeyValueRow
5. **Previews** (`§1.4`) — 5 files under `previews/`, viewable with `slint-viewer`
6. **Playground crate** (`§1.5`) — sibling `abdu-slint-ui-playground/` with the shell layout
7. **Playground sections** (`§1.6`) — 5 sections, one per component
8. **Smoke test** (`§1.7`) — `examples/settings-display.slint`, rewrite of `e2manage-pos-terminal/ui/screens/settings/display.slint` using only Phase 1 primitives

### Phase 1 definition of done

See `IMPL.md` final section. 12-item checklist covering library build, all components, all previews, playground build, all playground sections, smoke test, and code cleanliness.

### Phase 1 decision gate

After completion: did the API survive contact with the smoke-test screen? If yes → Phase 2. If no → revise specific components before continuing.

---

## What this session produced

| Artifact                                              | Status     |
| ----------------------------------------------------- | ---------- |
| `abdu-slint-ui/README.md`                             | ✅ written |
| `abdu-slint-ui/CLAUDE.md`                             | ✅ written |
| `abdu-slint-ui/ROADMAP.md`                            | ✅ written |
| `abdu-slint-ui/IMPL.md`                               | ✅ written |
| `abdu-slint-ui/HANDOVER.md`                           | ✅ this file |
| `ui/spike/shadcn_button.slint`                        | ✅ written (out-of-tree spike) |
| Library code                                          | ❌ none yet |
| Playground crate                                      | ❌ not created |

---

## Verifying state when resuming

```sh
cd /home/abdu/Downloads/e2manage-pos-terminal/

# Doc set should be five files
ls abdu-slint-ui/
# Expected: CLAUDE.md  HANDOVER.md  IMPL.md  README.md  ROADMAP.md

# Spike file present
ls ui/spike/
# Expected: shadcn_button.slint

# Last commit references Phase 0
git log --oneline -1
# Expected: <hash> docs(abdu-slint-ui): add Phase 0 design docs
```

---

## Discipline reminders (full version in CLAUDE.md)

- **CLAUDE.md overrides `~/.claude/CLAUDE.md`** inside this directory. Don't apply the user-wide "8-prop narrow APIs" rule — textbook 15–25 prop surfaces are correct here.
- **Visual validation is the primary correctness criterion.** A component renders → it works. Unit tests are limited to layout invariants, behavior, and state machines — not visuals.
- **Every component closes only when its preview file + playground section + doc comments are complete.** No half-done components.
- **One component at a time.** Don't half-write five in parallel.
- **Anti-sycophancy from `~/.claude/CLAUDE.md` applies fully.** Push back on bad ideas; lead with problems; don't fold under restated conviction.

---

## Don't touch (Phases 1–3)

The POS itself stays untouched until Phase 4:

- `e2manage-pos-terminal/ui/` (except the existing `ui/spike/`)
- `e2manage-pos-terminal/src/`
- `e2manage-pos-terminal/crates/`
- `e2manage-pos-terminal/Cargo.toml` (workspace)

The library evolves in isolation. POS integration is its own phase with its own plan.
