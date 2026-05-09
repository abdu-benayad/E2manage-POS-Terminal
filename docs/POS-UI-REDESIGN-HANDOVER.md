# POS UI Redesign — Handover (after Plan 1 / Foundation)

**Status as of 2026-05-09:** Plan 1 (Foundation) complete on branch
`worktree-pos-ui-redesign-foundation`. Ready to start Plan 2 (Atomic Components).

---

## Where everything lives

| File | Purpose |
|---|---|
| `docs/POS-UI-REDESIGN.md` | **Design spec.** Identity, layout, palette, RTL strategy, payment-tile system, mandatory-receipt flow. The "what and why." |
| `docs/POS-UI-REDESIGN-IMPL-01-FOUNDATION.md` | Plan 1 — completed. Token system, font bundle, RTL helpers, harness. |
| `docs/POS-UI-REDESIGN-FINDINGS-01-SLINT-RTL.md` | Slint capability findings + Task 10 verification + Plan 2 carry-forwards. |
| `docs/POS-UI-REDESIGN-HANDOVER.md` | **This file.** Read first when picking up the work. |

Plan 2–6 documents do **not** exist yet — they need to be written when the
foundation has been visually verified and the work continues.

---

## What Plan 1 delivered (the foundation Plan 2 builds on)

### New globals (under `ui/tokens/`)
- **`Theme`** — `mode: "light"|"dark"`, `is-dark`, `is-light` (predicates).
- **`Surfaces`** — Background / Panel / Surface / Inset four-tier tokens, each
  with top + bottom + border + shadow values, all derived from `Theme.is-dark`.
  Specular highlights for "glassy edge" treatment.
- **`Fonts`** — Canonical family-name strings (`sans`, `sans-arabic`, `mono`)
  backed by the bundled `.ttf` imports in `fonts.slint`.

### Refactored globals (in `ui/theme.slint`)
- **`Colors`** — Every value now derives from `Theme.is-dark`. All 38 original
  tokens preserved as legacy aliases so existing screens compile unchanged.
  12 new tokens added (`accent-lime`, `pay-green`, `pay-green-bg-stop-1/2`,
  `text-on-pay`, `border-strong`, `cat-coffee/bakery/cold/food`, `background-2`,
  `surface-2`). `Colors.background` and `background-2` aliased to
  `Surfaces.bg-bottom` / `bg-top` (single source of truth).
- **`Typography`** — `font-family` now `Fonts.sans + ", " + Fonts.sans-arabic`,
  `font-family-mono` is `Fonts.mono`. Added `arabic-line-height-multiplier: 1.12`.
- **`Layout`** — Added 4 logical-direction helper functions
  (`leading`, `trailing`, `leading-color`, `trailing-color`) plus 2 predicates
  (`is-rtl`, `is-ltr`). Use these instead of `border-left` / `border-right`
  literals so values flip with `Layout.rtl`.

### Bundled fonts
- IBM Plex Sans (4 weights), IBM Plex Sans Arabic (4 weights), JetBrains Mono
  (2 weights) under `assets/fonts/`. Embedded into the binary via bare
  `import "...ttf";` lines in `ui/tokens/fonts.slint` (Slint's compile-time
  font-embed mechanism). Verified by inspecting generated Rust.

### Theme harness
- `ui/screens/dev/theme_harness.slint` — renders every token in one scrollable
  view. Wrapped by `theme_harness_window.slint` (Window inheritor — required
  for Slint to generate Rust bindings).
- `cargo run -- --theme-harness` opens the window. Three toolbar buttons
  toggle `Theme.mode`, `Layout.rtl`, and `Locale.current`. Implementation in
  `src/dev_harness.rs`; CLI dispatch in `src/main.rs`.

### Other
- `tests/slint_arabic_smoke.rs` — infrastructure anchor (`assert!(true)`).
  Proves the Slint test target builds.
- `build.rs` — switched to
  `slint_build::CompilerConfiguration::new().embed_resources(EmbedFiles)`
  so the font-import pipeline triggers.

---

## Critical findings from Plan 1 (don't re-discover them)

1. **Vendored Slint is 1.14.1, not 1.8.** Cargo.toml says `slint = "1.8"` but
   the vendor directory resolves to 1.14.1 (semver-compatible upgrade). All
   APIs in this codebase are 1.14.
2. **Slint uses `parley` + `swash` for text shaping**, not C HarfBuzz directly.
   Pure-Rust OpenType shaper. Arabic positional shaping works. Obscure edge
   cases may differ from upstream HarfBuzz — visual verification is the gate.
3. **`slint::register_font_from_memory` does NOT exist as a public API.** It's
   internal to the Renderer trait. The canonical way to bundle a font is bare
   `import "x.ttf";` in a `.slint` file. Slint embeds it at compile time.
4. **Slint 1.14 has no native logical-direction layout primitive.** RTL
   mirroring is handled per-component via `Layout.leading() / .trailing()`
   helpers and `Layout.is-rtl` conditionals on `HorizontalLayout` child order.
5. **Slint only generates Rust bindings for components inheriting Window.**
   The `ThemeHarness` (Rectangle) is wrapped by `ThemeHarnessWindow` (Window)
   so Rust can construct it — pattern Plan 2 components must follow if any
   need direct Rust access.

---

## Plan 2 carry-forwards (must address before/during Plan 2)

1. **Visual verification gate (operator-only).** Run
   `cargo run -- --theme-harness` on a workstation with a display and capture
   four screenshots into `docs/POS-UI-REDESIGN-SCREENSHOTS-FOUNDATION/`:
   - light + LTR + en
   - light + RTL + ar
   - dark + LTR + en
   - dark + RTL + ar
   Confirm Arabic shapes correctly (no boxes, no separated letters), mono
   numerics render in JetBrains Mono, all four `Surfaces` tiers visibly
   differ between themes. Append a "Visual verification" subsection to
   `docs/POS-UI-REDESIGN-FINDINGS-01-SLINT-RTL.md` with the result.

2. **`Layout.rtl` defaults to `true`.** Plan 2's first task that touches the
   normal startup path in `src/main.rs` must wire locale detection to
   `Layout::get_global::<Layout>().set_rtl(...)` so non-Arabic developers see
   LTR layout on `cargo run`.

3. **Repo-wide rustfmt drift.** ~60 pre-existing files fail
   `cargo fmt --check`. Plan 1 explicitly did not fix them (out of scope).
   Suggested: one-shot `cargo fmt` + add `cargo fmt --check` and
   `cargo clippy -- -D warnings` to CI as the first commit of Plan 2.

4. **Two pre-existing clippy warnings in `src/main.rs`** — `run_startup_sequence`
   has too many args; one `else if let Err` is collapsible. Address
   opportunistically.

---

## What Plan 2 should be (Atomic Components)

Per the design spec, Plan 2 builds the reusable component library that the
new main checkout screen (Plan 3) needs. Roughly: Panel, Button, SearchInput,
ProductTile, CartLine, OpsButton, StatusLED, PayButton. Each:
- Built against the new tokens (Theme, Surfaces, Colors, Fonts, Typography).
- Uses `Layout.leading() / .trailing()` for any directional border / padding.
- Wraps a Window-inheriting variant if Rust must construct it directly.
- Has an isolated harness slot in the existing `ThemeHarness` (or its own
  `screens/dev/component_gallery.slint`).
- Renders correctly in light/dark × LTR/RTL × en/ar.

**Plan 2 doesn't yet have an IMPL doc.** Writing it is the first step of the
next session (after the visual-verification gate). Use
`superpowers:writing-plans` to draft it; the design spec
(`docs/POS-UI-REDESIGN.md`) and this handover are the inputs.

---

## Branch state

- Branch: `worktree-pos-ui-redesign-foundation`
- Worktree path: `.claude/worktrees/pos-ui-redesign-foundation` (harness-managed)
- 15 commits ahead of `main`
- Last commit: `458fb9e fix(ui): align Colors.background with Surfaces; fix invisible status-bar-border`
- Not pushed to origin
- Not merged to main

To resume:
```bash
cd /home/admin/projects/WadiDMS/e2manage-pos-terminal/.claude/worktrees/pos-ui-redesign-foundation
git log --oneline -5
```

If the worktree was cleaned up between sessions, recreate it:
```bash
cd /home/admin/projects/WadiDMS/e2manage-pos-terminal
git worktree add .claude/worktrees/pos-ui-redesign-foundation worktree-pos-ui-redesign-foundation
```

---

## Conventions established by Plan 1

- **Tokens live in `ui/tokens/`** (one global per file, re-exported from `mod.slint`).
- **Legacy globals stay in `ui/theme.slint`** until Plan 2/3 migrate consumers.
- **Developer screens live in `ui/screens/dev/`** (gated behind CLI flags, never reachable from production navigation).
- **Window-wrapped variants** for components Rust constructs directly (Slint binding requirement).
- **Bundled fonts via `import "...ttf";`** in a `.slint` file (no Rust-side `register_font_from_memory`).
- **Commits**: conventional format, why-focused body, `Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>` trailer.
