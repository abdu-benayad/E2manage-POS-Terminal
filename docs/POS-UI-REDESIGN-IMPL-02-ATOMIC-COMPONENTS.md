# POS UI Redesign — IMPL Plan 02: Atomic Components

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the eight reusable atomic components (Panel, Button, SearchInput, OpsButton, StatusLED, PayButton, ProductTile, CartLine) that the new main checkout screen (Plan 3) will compose. Components consume Plan 1's tokens (`Theme`, `Surfaces`, `Colors`, `Fonts`, `Typography`), use `Layout.leading()`/`.trailing()` helpers for any directional border or padding, and live in a new `ui/components/atomic/` subdirectory so the legacy components stay untouched until Plan 3 migrates screens.

**Architecture:**

- New components are added under `ui/components/atomic/`. They never modify the existing `ui/components/*.slint` files, so screens that already compile keep working through Plan 2. Plan 3 is responsible for migrating screens off the legacy components.
- Verification is visual via a new `--component-gallery` mode (parallel to `--theme-harness`). The gallery instantiates every atomic component in light/dark × LTR/RTL × en/ar and lets the operator scroll a single page.
- Production startup path is touched once (Task 1) to wire detected locale into the `Layout.rtl` and `Locale.current` globals so non-Arabic developers see LTR on `cargo run`. After this task, the legacy default of RTL for Arabic-first deployments still applies when `LANG=ar*`.
- Repo-wide `cargo fmt` drift (~60 files, pre-existing) is cleared in Task 0 and a CI gate is added so it cannot recur.

**Tech Stack:**

- Slint 1.14.1 (vendored as `slint = "1.8"` in Cargo.toml — Plan 1 finding)
- Rust 1.92 edition 2021
- No new Cargo dependencies. Locale detection uses `std::env::var` against `LANG` / `LC_ALL`.

**Spec reference:** `docs/POS-UI-REDESIGN.md` §3 (decisions), §4 (design system), §5 (layout), §7 (RTL & Arabic). Foundation status: `docs/POS-UI-REDESIGN-IMPL-01-FOUNDATION.md`. Carry-forwards: `docs/POS-UI-REDESIGN-HANDOVER.md` §"Plan 2 carry-forwards".

---

## File Structure

| Action | File | Purpose |
|---|---|---|
| Create | `.github/workflows/ci.yml` | Fmt-check + clippy CI gate (Task 0) |
| Modify | (whole repo) | One-shot `cargo fmt` cleanup of pre-existing drift (Task 0) |
| Modify | `src/main.rs` lines 875–895 | Collapse pre-existing `else if let Err` clippy warning (Task 0) |
| Create | `src/locale_detect.rs` | `detect_locale() -> (locale: &'static str, rtl: bool)` from `LANG`/`LC_ALL` (Task 1) |
| Modify | `src/main.rs` line 5 area + after line 131 | `mod locale_detect;` + apply detection to `Layout` and `Locale` globals on `MainWindow` (Task 1) |
| Modify | `src/dev_harness.rs` | Use `locale_detect::detect_locale` for the harness's initial state instead of hard-coded `"light" / false / "en"` (Task 1) |
| Create | `tests/locale_detect_tests.rs` | Unit tests for `detect_locale` (Task 1) |
| Create | `ui/components/atomic/mod.slint` | Re-export hub for new components (Task 2) |
| Create | `ui/screens/dev/component_gallery.slint` | Gallery surface that hosts every atomic component (Task 2 + later tasks add slots) |
| Create | `ui/screens/dev/component_gallery_window.slint` | `Window`-inheriting wrapper Rust constructs (Task 2) |
| Modify | `ui/screens/dev/mod.slint` | Re-export `ComponentGalleryWindow` (Task 2) |
| Modify | `ui/main.slint` | Re-export `ComponentGalleryWindow` so `slint::include_modules!()` picks it up (Task 2) |
| Create | `src/component_gallery.rs` | Rust adapter — opens the gallery window with toolbar toggles (Task 2) |
| Modify | `src/main.rs` line 7 area + line 39 area | `mod component_gallery;` + `--component-gallery` CLI dispatch (Task 2) |
| Create | `ui/components/atomic/panel.slint` | `Panel` component (Task 3) |
| Create | `ui/components/atomic/button.slint` | New `AtomicButton` with primary/secondary/danger/ghost variants (Task 4) |
| Create | `ui/components/atomic/search_input.slint` | `SearchInput` with leading icon slot + clear affordance (Task 5) |
| Create | `ui/components/atomic/ops_button.slint` | `OpsButton` (icon + label, lime/danger/neutral variants) (Task 6) |
| Create | `ui/components/atomic/status_led.slint` | `StatusLED` (online/offline/syncing dot) (Task 7) |
| Create | `ui/components/atomic/pay_button.slint` | `PayButton` (label + total strip, lit gradient, halo) (Task 8) |
| Create | `ui/components/atomic/product_tile.slint` | `ProductTile` with category-accent leading border (Task 9) |
| Create | `ui/components/atomic/cart_line.slint` | `CartLine` with qty pill on the leading edge + selected glow (Task 10) |
| Modify | `docs/POS-UI-REDESIGN-FINDINGS-01-SLINT-RTL.md` | Append "Visual verification — Plan 2" subsection with operator screenshots (Task 11) |
| Create | `docs/POS-UI-REDESIGN-SCREENSHOTS-PLAN-02/` | Screenshot directory referenced from the findings doc (Task 11) |

**Module responsibility split:** `ui/components/atomic/` owns the new component layer. Each file declares **one** exported component, takes only properties (no global mutation), and reads tokens from `tokens/mod.slint` plus `theme.slint`'s `Layout`/`Locale`/`Typography` globals. Components do not read Slint state globals (`AppState`, `CartState`) — those couplings happen at screen level in Plan 3.

`ui/screens/dev/` owns developer screens (theme harness from Plan 1, component gallery from Plan 2). They are reachable only through CLI flags, never from production navigation.

`src/locale_detect.rs` is a pure function with no I/O beyond `std::env`. `src/component_gallery.rs` mirrors `src/dev_harness.rs` — a thin window adapter.

---

## Conventions for every component

These conventions apply to Tasks 3–10. Each task body assumes them.

1. **Token sources only.** Every colour comes from `Colors` or `Surfaces`. Every font family comes from `Typography.font-family` or `Typography.font-family-mono` (which already resolve to `Fonts.*` per Plan 1). No hex literals in component files, except per-category accent colours which are themselves tokens (`Colors.cat-coffee` etc.).
2. **Directional borders/padding via helpers.** `Layout.leading(a, b)` / `Layout.trailing(a, b)` for lengths; `Layout.leading-color(a, b)` / `Layout.trailing-color(a, b)` for colours. No literal `border-left` / `border-right` for values that must mirror.
3. **Public API is properties + callbacks only.** Components are stateless except where Slint forces a `states [ pressed when ... ]` block. Selection, loading, and disabled flags are `in` properties pushed from the parent.
4. **Single-responsibility components.** No component owns more than one logical concept. ProductTile shows one product; CartLine shows one line; PayButton has one click target.
5. **Press animation, no hover.** POS is touch-first. `states [ pressed when touch.pressed: { scale: 0.97 } ] animate scale { duration: 80ms; easing: ease-out; }`. Hover is fine to add but never required.
6. **Naming.** New components keep their plain name (`Panel`, `Button`, `SearchInput`, …) inside `atomic/`. Where a name collides with a legacy component (`Button`, `ProductTile`, `CartItem`/`CartLine`), the new export uses the same name — they are reachable through different module paths (`ui/components/atomic/mod.slint` vs `ui/components/mod.slint`), and only Plan 3 swaps the import sites.
7. **Gallery slot per component.** Every Task 3–10 also adds one `// === <Component> ===` block to `ui/screens/dev/component_gallery.slint`, instantiating the component with two or three representative prop sets so the operator can eyeball the variants.

## Slint binding-generation pattern (project-specific)

This convention covers any task that needs Rust to read or write a Slint global. It was discovered during Task 1 and applies project-wide.

- **`global Foo { }`** (no `export`) — visible only inside the same `.slint` file. Plan 1's `Layout` and `Locale` start out this way.
- **`export { Foo, Bar }`** at the bottom of a `.slint` file — Slint-side re-export so other `.slint` files can import `Foo` and use it in `<=>` bindings. **Generates zero Rust accessors.**
- **`export global Foo { ... }`** (inline form) — generates flattened `set_<prefix>_*` / `get_<prefix>_*` methods on every `Window`-inheriting component that imports the file (e.g. `set_app_company_name` from `AppState`). Does **not** generate a `pub struct Foo` and `slint::Global<MainWindow, Foo>` is **not** the working pattern in this project despite the upstream Slint docs.
- **Forwarding property with `<=>`** on a `Window`-inheriting component — the canonical project pattern. Declaring `in-out property <T> name <=> Global.field;` inside `MainWindow` (or `ThemeHarnessWindow`, etc.) produces a clean `window.set_name(...)` / `window.get_name()` Rust accessor that updates the global in place. Use this whenever Rust needs to set or read a global field that wasn't already exposed via the inline `export global` form.

For atomic components (Tasks 3–10), this convention is mostly informational — components don't access Slint state from Rust directly. It matters for: the harness adapters (`src/dev_harness.rs`, `src/component_gallery.rs`), and any Plan 3+ work where a screen component needs Rust to read/write its own state.

---

## Task 0: Clear cargo fmt drift, fix one pre-existing clippy warning, add CI gate

**Files:**
- Modify: (whole repo, mechanical via `cargo fmt`)
- Modify: `src/main.rs` (collapse one `else if let Err` block)
- Create: `.github/workflows/ci.yml`

The handover doc records ~60 pre-existing files that fail `cargo fmt --check`. Plan 1 deliberately did not touch them. Plan 2's first commit clears the drift so subsequent atomic-component commits don't get reviewed against a moving baseline, and adds a CI gate so the drift can't recur.

The `too_many_arguments` warning on `run_startup_sequence` is a real refactor (extract a context struct) — out of scope here, defer to a separate cleanup pass. The `collapsible_else_if` warning is a 3-line mechanical fix included.

- [ ] **Step 1: Confirm baseline drift count**

Run:
```bash
cargo fmt --check 2>&1 | grep -c "^Diff in" || true
```
Expected: a number ≥ 50 (per Plan 1 findings).

- [ ] **Step 2: Apply formatter repo-wide**

Run:
```bash
cargo fmt --all
cargo fmt --check 2>&1 | tail -5
```
Expected: second command exits 0 with no diff output.

- [ ] **Step 3: Locate the `collapsible_else_if` warning**

Run:
```bash
cargo clippy --message-format=short 2>&1 | grep -E "collapsible_else_if|too_many_arguments" | head -5
```
Expected output includes one line about `collapsible_else_if` near `src/main.rs:890` and one line about `too_many_arguments` near `src/main.rs:2094`. Note the exact line numbers — they may have shifted after Step 2's reformat. Use the line printed by clippy for Step 4.

- [ ] **Step 4: Read the surrounding code**

Read `src/main.rs` around the line clippy reported. The pattern looks like:

```rust
} else {
    if let Err(e) = some_call() {
        ...
    }
}
```

Edit it to:

```rust
} else if let Err(e) = some_call() {
    ...
}
```

If the body of the original `else { if ... { ... } }` has more than the single `if let Err`, **do not collapse** — clippy will allow it. Re-read the block before editing.

- [ ] **Step 5: Verify clippy is clean for that warning**

Run:
```bash
cargo clippy --message-format=short 2>&1 | grep -E "collapsible_else_if" | head -3
```
Expected: empty output.

- [ ] **Step 6: Add the CI workflow**

Create `.github/workflows/ci.yml`:

```yaml
name: CI

on:
  push:
    branches: [main]
  pull_request:

jobs:
  check:
    name: Lint & format
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt, clippy
      - name: cargo fmt
        run: cargo fmt --all --check
      - name: cargo clippy
        run: cargo clippy --workspace --all-targets -- -D warnings -A clippy::too_many_arguments
        # too_many_arguments is allow-listed: pre-existing run_startup_sequence
        # has 8 args. Removing the allow is its own follow-up.

  test:
    name: Test
    runs-on: ubuntu-latest
    needs: check
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - run: cargo test --workspace --no-run
      - run: cargo test --workspace -- --skip e2e_
```

Note: e2e tests are skipped because they require a backend; the existing `scripts/run-e2e-tests.sh` runs them under operator control.

- [ ] **Step 7: Verify the pipeline thinks the same locally**

Run:
```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings -A clippy::too_many_arguments 2>&1 | tail -10
```
Expected: both exit 0 with no warning output.

- [ ] **Step 8: Commit**

```bash
git add -A
git commit -m "$(cat <<'EOF'
chore(repo): one-shot cargo fmt + add fmt/clippy CI gate

Clears the ~60-file pre-existing fmt drift recorded in the Plan 1
foundation findings so Plan 2 component commits don't review against
a moving baseline, and adds a GitHub Actions workflow that enforces
fmt + clippy on every push/PR. Also collapses one pre-existing
collapsible_else_if warning in src/main.rs that was trivial to fix.

The too_many_arguments warning on run_startup_sequence is allow-listed
in CI for now — extracting a context struct is its own refactor.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 1: Wire locale detection into `Layout.rtl` and `Locale.current` on app startup

**Files:**
- Create: `src/locale_detect.rs`
- Create: `tests/locale_detect_tests.rs`
- Modify: `src/main.rs` (`mod locale_detect;` declaration + apply after `MainWindow::new()`)
- Modify: `src/dev_harness.rs` (use detection for initial state)
- Modify: `ui/main.slint` (add two `<=>` forwarding properties on `MainWindow` so `set_rtl` / `set_locale` exist on the Rust side — see Step 7a)

The `Layout.rtl` global currently defaults to `true` (Arabic-first). On `cargo run` from a non-Arabic developer machine, the entire UI renders RTL. This task adds a small pure detector that maps `LANG` / `LC_ALL` to a `(locale, rtl)` pair and applies it once after the main window is created.

Detection rules:
- `LC_ALL` takes precedence over `LANG`.
- If the value starts with `ar` (any case, with or without `_LY`, `_SA`, `.UTF-8` suffix), result is `("ar", true)`.
- If it starts with `fr`, result is `("fr", false)`.
- Anything else, including unset, is `("en", false)`.

This is intentionally minimal. Tenant-driven locale + per-operator overrides are Plan 3+ work.

- [ ] **Step 1: Write the failing tests**

Create `tests/locale_detect_tests.rs`:

```rust
//! Unit tests for src/locale_detect.rs. Pure-function tests — no env mutation
//! relied upon between cases (each case passes the env value explicitly).

use e2manage_pos_terminal::locale_detect::detect_from_env;

#[test]
fn arabic_libya_is_rtl() {
    assert_eq!(detect_from_env(Some("ar_LY.UTF-8"), None), ("ar", true));
}

#[test]
fn arabic_saudi_is_rtl() {
    assert_eq!(detect_from_env(Some("ar_SA.UTF-8"), None), ("ar", true));
}

#[test]
fn english_us_is_ltr() {
    assert_eq!(detect_from_env(Some("en_US.UTF-8"), None), ("en", false));
}

#[test]
fn french_is_ltr() {
    assert_eq!(detect_from_env(Some("fr_FR.UTF-8"), None), ("fr", false));
}

#[test]
fn unset_defaults_to_english_ltr() {
    assert_eq!(detect_from_env(None, None), ("en", false));
}

#[test]
fn lc_all_takes_precedence_over_lang() {
    assert_eq!(
        detect_from_env(Some("en_US.UTF-8"), Some("ar_LY.UTF-8")),
        ("ar", true)
    );
}

#[test]
fn empty_string_treated_as_unset() {
    assert_eq!(detect_from_env(Some(""), None), ("en", false));
}

#[test]
fn unknown_locale_falls_back_to_english() {
    assert_eq!(detect_from_env(Some("ja_JP.UTF-8"), None), ("en", false));
}
```

The test module imports from `e2manage_pos_terminal::locale_detect`. The crate root (`src/lib.rs`) re-exports modules under that name — confirm by reading `src/lib.rs` first; if `locale_detect` is not yet re-exported, Step 4 adds the re-export.

- [ ] **Step 2: Run the tests to confirm they fail**

Run:
```bash
cargo test --test locale_detect_tests 2>&1 | tail -20
```
Expected: build error — `unresolved import e2manage_pos_terminal::locale_detect`.

- [ ] **Step 3: Write the detector**

Create `src/locale_detect.rs`:

```rust
//! Detects user locale + RTL flag from LC_ALL / LANG environment variables.
//! Pure function; the env-var read happens at the call site so unit tests can
//! pass values directly without mutating process state.

/// Pure detector. Pass `(lang, lc_all)` from the caller. `LC_ALL` wins.
/// Returns `(locale_code, rtl)` where `locale_code` is one of `"ar"`,
/// `"en"`, `"fr"` (extend the match arm as new locales are supported).
pub fn detect_from_env(lang: Option<&str>, lc_all: Option<&str>) -> (&'static str, bool) {
    let raw = lc_all.filter(|s| !s.is_empty()).or(lang.filter(|s| !s.is_empty()));

    match raw {
        Some(s) if s.to_ascii_lowercase().starts_with("ar") => ("ar", true),
        Some(s) if s.to_ascii_lowercase().starts_with("fr") => ("fr", false),
        _ => ("en", false),
    }
}

/// Convenience entry point that reads the actual env vars. Used from `main.rs`.
pub fn detect_locale() -> (&'static str, bool) {
    let lc_all = std::env::var("LC_ALL").ok();
    let lang = std::env::var("LANG").ok();
    detect_from_env(lang.as_deref(), lc_all.as_deref())
}
```

- [ ] **Step 4: Re-export the module from `src/lib.rs`**

Read the existing `src/lib.rs` first:
```bash
grep -n "^pub mod\|^pub use" src/lib.rs | head -20
```

Add the line `pub mod locale_detect;` in alphabetical order with the other `pub mod` declarations. If `lib.rs` currently has no `pub mod` entries (it only re-exports from workspace crates), add the line near the top of the file.

- [ ] **Step 5: Wire `mod locale_detect;` into `src/main.rs`**

In `src/main.rs`, find the line `mod dev_harness;` (around line 7). Add directly below it:

```rust
mod locale_detect;
```

Note: this is a `mod` declaration (binary-local), and Step 4 also adds a `pub mod` in `src/lib.rs`. Both are needed because the binary and the library are compiled separately; the binary needs the local `mod` to use `crate::locale_detect`, and tests need the `lib.rs` re-export to use `e2manage_pos_terminal::locale_detect`.

- [ ] **Step 6: Run the detector tests — they should pass now**

Run:
```bash
cargo test --test locale_detect_tests 2>&1 | tail -20
```
Expected: 8 passed; 0 failed.

- [ ] **Step 7a: Add forwarding properties on `MainWindow` so Slint emits `set_rtl` / `set_locale` accessors**

`Layout` and `Locale` are declared as plain `global` (not `export global`) in `ui/theme.slint`, and the `export { Layout, Locale, ... }` list at the bottom of that file is a Slint-side re-export only — it does **not** generate Rust accessors. The canonical pattern in this project (already used by `ThemeHarnessWindow`) is to add forwarding properties on the `Window`-inheriting component so Slint generates `set_*` methods on the Rust side.

In `ui/main.slint`, find the `MainWindow` component body and the existing `// AppState bindings (for setting from Rust)` block. Insert directly above it:

```slint
    // Locale + RTL forwarding (Plan 2 Task 1) — Rust sets these once at
    // startup based on LC_ALL / LANG so the whole component tree picks up
    // the correct direction and locale before first paint.
    in-out property <bool> rtl <=> Layout.rtl;
    in-out property <string> locale <=> Locale.current;
```

These two `<=>` bindings forward to the existing `Layout.rtl` and `Locale.current` globals — Slint generates `MainWindow::set_rtl(...)` and `MainWindow::set_locale(...)` Rust methods that update the globals in place, no copy. Do **not** mark the globals `export global` — last-round investigation confirmed that with this Slint version, the `export global Foo { }` form generates flattened `set_<prefix>_*` methods on `MainWindow` (e.g. `set_app_company_name` for `AppState`) rather than the `slint::Global` trait, and the `window.global::<T>()` Rust API does not work in this project.

- [ ] **Step 7b: Apply detection to the live `MainWindow`**

In `src/main.rs`, find the line `let window = MainWindow::new()?;` (around line 131). Insert directly after it:

```rust
    // === Apply detected locale to UI globals (Plan 2 Task 1) ===========
    {
        let (locale_code, rtl) = locale_detect::detect_locale();
        window.set_rtl(rtl);
        window.set_locale(SharedString::from(locale_code));
        info!(
            locale = locale_code,
            rtl,
            "applied detected locale to UI globals"
        );
    }
```

`SharedString` is brought into scope by the existing `use slint::{ModelRc, SharedString, VecModel};` line near the top of `main.rs`; no extra `use` is needed.

- [ ] **Step 8: Update `src/dev_harness.rs` to use detection for the harness's initial state**

Read the current file:
```bash
grep -n "set_mode\|set_rtl\|set_locale" src/dev_harness.rs
```

In `src/dev_harness.rs`, replace the three hard-coded lines:

```rust
    harness.set_mode("light".into());
    harness.set_rtl(false);
    harness.set_locale("en".into());
```

with:

```rust
    let (locale_code, rtl) = crate::locale_detect::detect_locale();
    harness.set_mode("light".into());
    harness.set_rtl(rtl);
    harness.set_locale(locale_code.into());
```

The harness still defaults theme mode to `"light"` (operator can toggle). RTL and locale follow the developer's environment so the harness opens with the same orientation the production app would.

- [ ] **Step 9: Verify build and lints**

Run:
```bash
cargo build 2>&1 | tail -10
cargo clippy --workspace --all-targets -- -D warnings -A clippy::too_many_arguments 2>&1 | tail -10
```
Expected: both succeed.

- [ ] **Step 10: Verify the unit tests still pass**

Run:
```bash
cargo test --test locale_detect_tests 2>&1 | tail -10
```
Expected: 8 passed.

- [ ] **Step 11: Commit**

```bash
git add src/locale_detect.rs src/lib.rs src/main.rs src/dev_harness.rs tests/locale_detect_tests.rs
git commit -m "$(cat <<'EOF'
feat(ui): detect locale from env and apply to Layout/Locale globals

Layout.rtl previously defaulted to true (Arabic-first), which made
non-Arabic developer machines render the whole UI mirrored on
`cargo run`. This adds a tiny pure detector keyed off LC_ALL/LANG
and applies the result to the Slint Layout and Locale globals once
after MainWindow is created. The dev harness uses the same detection
for its initial state.

Tenant-driven locale + per-operator overrides remain a Plan 3+ concern;
this task only fixes the cargo-run developer surface.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 2: Component-gallery shell + `--component-gallery` CLI flag

**Files:**
- Create: `ui/components/atomic/mod.slint`
- Create: `ui/screens/dev/component_gallery.slint`
- Create: `ui/screens/dev/component_gallery_window.slint`
- Modify: `ui/screens/dev/mod.slint`
- Modify: `ui/main.slint`
- Create: `src/component_gallery.rs`
- Modify: `src/main.rs`

This task creates the empty gallery surface with the same toolbar as the theme harness (theme/RTL/locale toggles), an empty scroll body, and a `--component-gallery` flag in the binary. Tasks 3–10 each fill in one section of the gallery body.

- [ ] **Step 1: Create the `atomic/` directory and an empty re-export hub**

Run:
```bash
mkdir -p ui/components/atomic
```

Create `ui/components/atomic/mod.slint`:
```slint
// Atomic components (Plan 2). One component per file; this hub re-exports
// them. Tasks 3–10 each append one line below.

// (Task 3) export { Panel } from "panel.slint";
// (Task 4) export { Button } from "button.slint";
// (Task 5) export { SearchInput } from "search_input.slint";
// (Task 6) export { OpsButton } from "ops_button.slint";
// (Task 7) export { StatusLED } from "status_led.slint";
// (Task 8) export { PayButton } from "pay_button.slint";
// (Task 9) export { ProductTile } from "product_tile.slint";
// (Task 10) export { CartLine } from "cart_line.slint";
```

The placeholder comments are templates — Tasks 3–10 uncomment them in order.

- [ ] **Step 2: Create the gallery surface**

Create `ui/screens/dev/component_gallery.slint`:

```slint
import { Theme, Surfaces } from "../../tokens/mod.slint";
import { Colors, Typography, Spacing, Radius, Layout, Locale } from "../../theme.slint";
import { ScrollView } from "std-widgets.slint";

// Component gallery — Plan 2. Hosts every atomic component in light/dark
// × LTR/RTL × en/ar so visual regressions are obvious. Each Task 3–10
// adds one block below the divider comment.
export component ComponentGallery inherits Rectangle {
    in-out property <string> mode <=> Theme.mode;
    in-out property <bool> rtl <=> Layout.rtl;
    in-out property <string> locale <=> Locale.current;

    callback toggle-theme;
    callback toggle-rtl;
    callback cycle-locale;

    background: Surfaces.bg-bottom;

    VerticalLayout {
        spacing: Spacing.md;
        padding: Spacing.lg;

        // Toolbar (mirrors theme harness)
        Rectangle {
            height: 56px;
            background: Surfaces.panel-top;
            border-color: Surfaces.panel-border;
            border-width: 1px;
            border-radius: Radius.md;

            HorizontalLayout {
                padding-left: Spacing.lg;
                padding-right: Spacing.lg;
                spacing: Spacing.md;
                alignment: center;

                Text {
                    text: "POS — Component Gallery";
                    font-family: Typography.font-family;
                    font-size: Typography.heading;
                    font-weight: Typography.semi-bold;
                    color: Colors.text-primary;
                    horizontal-stretch: 1;
                    vertical-alignment: center;
                }

                toolbar-btn := Rectangle {
                    width: 110px;
                    height: 36px;
                    background: Surfaces.surface-top;
                    border-color: Surfaces.surface-border;
                    border-width: 1px;
                    border-radius: Radius.sm;
                    TouchArea { clicked => { root.toggle-theme(); } }
                    Text {
                        text: "Theme: " + Theme.mode;
                        font-family: Typography.font-family;
                        font-size: Typography.caption;
                        color: Colors.text-primary;
                        horizontal-alignment: center;
                        vertical-alignment: center;
                    }
                }

                Rectangle {
                    width: 110px;
                    height: 36px;
                    background: Surfaces.surface-top;
                    border-color: Surfaces.surface-border;
                    border-width: 1px;
                    border-radius: Radius.sm;
                    TouchArea { clicked => { root.toggle-rtl(); } }
                    Text {
                        text: Layout.is-rtl ? "Dir: RTL" : "Dir: LTR";
                        font-family: Typography.font-family;
                        font-size: Typography.caption;
                        color: Colors.text-primary;
                        horizontal-alignment: center;
                        vertical-alignment: center;
                    }
                }

                Rectangle {
                    width: 110px;
                    height: 36px;
                    background: Surfaces.surface-top;
                    border-color: Surfaces.surface-border;
                    border-width: 1px;
                    border-radius: Radius.sm;
                    TouchArea { clicked => { root.cycle-locale(); } }
                    Text {
                        text: "Lang: " + Locale.current;
                        font-family: Typography.font-family;
                        font-size: Typography.caption;
                        color: Colors.text-primary;
                        horizontal-alignment: center;
                        vertical-alignment: center;
                    }
                }
            }
        }

        // Scrollable component sections — Tasks 3–10 add blocks here.
        ScrollView {
            VerticalLayout {
                spacing: Spacing.xl;
                padding: Spacing.md;

                // === COMPONENT SECTIONS START — appended by Plan 2 Tasks 3–10 ===

                // (Task 3) Panel section appended here
                // (Task 4) Button section appended here
                // (Task 5) SearchInput section appended here
                // (Task 6) OpsButton section appended here
                // (Task 7) StatusLED section appended here
                // (Task 8) PayButton section appended here
                // (Task 9) ProductTile section appended here
                // (Task 10) CartLine section appended here

                // Filler so the gallery is non-empty before any sections land.
                Text {
                    text: "Component sections will appear here as Plan 2 Tasks 3–10 land.";
                    font-family: Typography.font-family;
                    font-size: Typography.caption;
                    color: Colors.text-secondary;
                    horizontal-alignment: center;
                }
            }
        }
    }
}
```

- [ ] **Step 3: Wrap it in a `Window`-inheriting variant for Rust**

Create `ui/screens/dev/component_gallery_window.slint`:

```slint
import { ComponentGallery } from "component_gallery.slint";

// Window wrapper so Rust (slint::ComponentHandle) can construct it directly.
// Slint only generates Rust bindings for components that inherit Window.
export component ComponentGalleryWindow inherits Window {
    title: "POS Component Gallery";
    preferred-width: 1280px;
    preferred-height: 900px;

    in-out property <string> mode <=> gallery.mode;
    in-out property <bool> rtl <=> gallery.rtl;
    in-out property <string> locale <=> gallery.locale;

    callback toggle-theme <=> gallery.toggle-theme;
    callback toggle-rtl <=> gallery.toggle-rtl;
    callback cycle-locale <=> gallery.cycle-locale;

    gallery := ComponentGallery {
        width: 100%;
        height: 100%;
    }
}
```

- [ ] **Step 4: Re-export from `ui/screens/dev/mod.slint`**

Read current contents:
```bash
cat ui/screens/dev/mod.slint
```

The existing file (from Plan 1) re-exports `ThemeHarness` and `ThemeHarnessWindow`. Add:
```slint
export { ComponentGallery } from "component_gallery.slint";
export { ComponentGalleryWindow } from "component_gallery_window.slint";
```

- [ ] **Step 5: Re-export from `ui/main.slint`**

Find the existing `export { ThemeHarnessWindow } from "screens/dev/mod.slint";` line near the top of `ui/main.slint`. Add directly below:
```slint
export { ComponentGalleryWindow } from "screens/dev/mod.slint";
```

This makes `crate::ComponentGalleryWindow` available in Rust through `slint::include_modules!()`.

- [ ] **Step 6: Compile-check the Slint side**

Run:
```bash
cargo check 2>&1 | tail -10
```
Expected: compiles. If a Slint error mentions an unresolved import, re-read Steps 2/3 — paths are relative to the importing file.

- [ ] **Step 7: Write the Rust adapter**

Create `src/component_gallery.rs`:

```rust
//! Developer-only component gallery window. Run with
//! `cargo run -- --component-gallery`. Lights up every atomic component
//! from `ui/components/atomic/` in light/dark × LTR/RTL × en/ar so visual
//! regressions surface in one scroll.

use slint::ComponentHandle;

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let gallery = crate::ComponentGalleryWindow::new()?;

    // Initial state mirrors detected locale (consistent with the live app).
    let (locale_code, rtl) = crate::locale_detect::detect_locale();
    gallery.set_mode("light".into());
    gallery.set_rtl(rtl);
    gallery.set_locale(locale_code.into());

    let weak = gallery.as_weak();
    gallery.on_toggle_theme(move || {
        if let Some(g) = weak.upgrade() {
            let next = if g.get_mode() == "light" { "dark" } else { "light" };
            g.set_mode(next.into());
        }
    });

    let weak = gallery.as_weak();
    gallery.on_toggle_rtl(move || {
        if let Some(g) = weak.upgrade() {
            g.set_rtl(!g.get_rtl());
        }
    });

    let weak = gallery.as_weak();
    gallery.on_cycle_locale(move || {
        if let Some(g) = weak.upgrade() {
            let next = match g.get_locale().as_str() {
                "en" => "ar",
                "ar" => "fr",
                _ => "en",
            };
            g.set_locale(next.into());
        }
    });

    gallery.run()?;
    Ok(())
}
```

- [ ] **Step 8: Wire `--component-gallery` into `src/main.rs`**

In `src/main.rs`, find:

```rust
mod dev_harness;
mod locale_detect;
```

Add directly below:
```rust
mod component_gallery;
```

Then find the existing `--theme-harness` dispatch (around line 39):

```rust
    if std::env::args().any(|a| a == "--theme-harness") {
        return dev_harness::run();
    }
```

Add directly below:
```rust
    if std::env::args().any(|a| a == "--component-gallery") {
        return component_gallery::run();
    }
```

- [ ] **Step 9: Verify build**

Run:
```bash
cargo build 2>&1 | tail -10
cargo clippy --workspace --all-targets -- -D warnings -A clippy::too_many_arguments 2>&1 | tail -10
```
Expected: both succeed.

- [ ] **Step 10: Try to run the gallery (will fail headlessly, succeed graphically)**

Run:
```bash
cargo run -- --component-gallery 2>&1 | tail -5
```
On a headless box: error like `Could not initialize backend.` — that's fine, it confirms the build dispatched correctly. On a graphical machine: window opens with the toolbar and the placeholder text.

- [ ] **Step 11: Commit**

```bash
git add ui/components/atomic/mod.slint ui/screens/dev/component_gallery.slint ui/screens/dev/component_gallery_window.slint ui/screens/dev/mod.slint ui/main.slint src/component_gallery.rs src/main.rs
git commit -m "$(cat <<'EOF'
feat(ui): add component gallery shell + --component-gallery flag

Empty gallery surface with the same theme/RTL/locale toolbar as the
theme harness. Tasks 3–10 each append one component section. Window
wrapper follows the Plan 1 pattern so Rust can construct it directly.
Initial state seeds from locale_detect (Task 1) so non-Arabic
developers see LTR by default.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 3: `Panel` component

**Files:**
- Create: `ui/components/atomic/panel.slint`
- Modify: `ui/components/atomic/mod.slint`
- Modify: `ui/screens/dev/component_gallery.slint`

`Panel` is the four-tier surface tier-2 building block (rail, products area, ops column, cart). It applies the `Surfaces.panel-*` tokens and exposes a single content slot.

- [ ] **Step 1: Write the component**

Create `ui/components/atomic/panel.slint`:

```slint
import { Surfaces } from "../../tokens/mod.slint";
import { Spacing, Radius } from "../../theme.slint";

// Panel — surface tier 2 (rail / products / ops / cart). Shadow + border +
// 1 px specular top edge applied via a child rectangle. Content goes inside
// the @children slot.
export component Panel inherits Rectangle {
    in property <length> content-padding: Spacing.lg;

    background: Surfaces.panel-top;
    border-color: Surfaces.panel-border;
    border-width: 1px;
    border-radius: Radius.md;
    drop-shadow-color: Surfaces.panel-shadow;
    drop-shadow-blur: Surfaces.panel-shadow-blur;
    drop-shadow-offset-y: Surfaces.panel-shadow-offset-y;

    // Specular highlight — a 1 px line on the top inner edge.
    Rectangle {
        x: 1px;
        y: 1px;
        width: parent.width - 2px;
        height: 1px;
        background: Surfaces.specular-strong;
        border-radius: parent.border-radius;
    }

    Rectangle {
        x: root.content-padding;
        y: root.content-padding;
        width: parent.width - 2 * root.content-padding;
        height: parent.height - 2 * root.content-padding;
        @children
    }
}
```

- [ ] **Step 2: Activate the export in `ui/components/atomic/mod.slint`**

Replace `// (Task 3) export { Panel } from "panel.slint";` with `export { Panel } from "panel.slint";`.

- [ ] **Step 3: Add a gallery section**

In `ui/screens/dev/component_gallery.slint`, replace `// (Task 3) Panel section appended here` with:

```slint
                // === Panel ===
                Text {
                    text: "Panel — surface tier 2";
                    font-family: Typography.font-family;
                    font-size: Typography.heading;
                    font-weight: Typography.semi-bold;
                    color: Colors.text-primary;
                }
                HorizontalLayout {
                    spacing: Spacing.md;
                    Panel {
                        width: 240px;
                        height: 120px;
                        Text {
                            text: "Default panel";
                            font-family: Typography.font-family;
                            font-size: Typography.body;
                            color: Colors.text-primary;
                            horizontal-alignment: center;
                            vertical-alignment: center;
                        }
                    }
                    Panel {
                        width: 240px;
                        height: 120px;
                        content-padding: Spacing.xl;
                        Text {
                            text: "Wide-padding panel";
                            font-family: Typography.font-family;
                            font-size: Typography.body;
                            color: Colors.text-primary;
                            horizontal-alignment: center;
                            vertical-alignment: center;
                        }
                    }
                }
```

Add the import at the top of `component_gallery.slint`. Find the existing imports block and add:
```slint
import { Panel } from "../../components/atomic/mod.slint";
```

- [ ] **Step 4: Compile-check**

Run:
```bash
cargo check 2>&1 | tail -10
```
Expected: compiles.

- [ ] **Step 5: Commit**

```bash
git add ui/components/atomic/panel.slint ui/components/atomic/mod.slint ui/screens/dev/component_gallery.slint
git commit -m "$(cat <<'EOF'
feat(ui): add atomic Panel component (surface tier 2)

Panel applies the Surfaces.panel-* tokens (background, border, shadow,
top specular highlight) and exposes one content slot. Used by Plan 3
to wrap the rail / products area / ops column / cart panel.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 4: `Button` component (atomic)

**Files:**
- Create: `ui/components/atomic/button.slint`
- Modify: `ui/components/atomic/mod.slint`
- Modify: `ui/screens/dev/component_gallery.slint`

The legacy `ui/components/button.slint` stays untouched; this component lives only in `atomic/`. Variants: `primary` (lime accent), `secondary` (surface tier 3 outlined), `danger` (red), `ghost` (transparent).

- [ ] **Step 1: Write the component**

Create `ui/components/atomic/button.slint`:

```slint
import { Surfaces } from "../../tokens/mod.slint";
import { Colors, Typography, Spacing, Radius, Animation } from "../../theme.slint";

export component Button inherits Rectangle {
    in property <string> label: "Button";
    // "primary" | "secondary" | "danger" | "ghost"
    in property <string> variant: "primary";
    in property <bool> disabled: false;

    callback clicked;

    height: 44px;
    min-width: 120px;
    border-radius: Radius.md;

    // Background per variant
    background: disabled ? Surfaces.surface-bottom :
        variant == "primary" ? Colors.accent-lime :
        variant == "secondary" ? Surfaces.surface-top :
        variant == "danger" ? Colors.danger :
        transparent;

    border-width: variant == "secondary" ? 1px : 0px;
    border-color: Surfaces.surface-border;

    states [
        pressed when touch.pressed && !disabled: {
            // Compose press feedback via opacity rather than colour math —
            // keeps the variant-specific tones intact.
            opacity: 0.85;
            inner-press-scale: 0.97;
        }
    ]

    in-out property <float> inner-press-scale: 1.0;
    animate inner-press-scale { duration: Animation.fast; easing: ease-out; }
    animate opacity { duration: Animation.fast; }

    HorizontalLayout {
        padding-left: Spacing.lg;
        padding-right: Spacing.lg;
        alignment: center;

        Text {
            text: root.label;
            font-family: Typography.font-family;
            font-size: Typography.body;
            font-weight: Typography.semi-bold;
            // Text colour per variant.
            color: root.disabled ? Colors.text-muted :
                root.variant == "primary" ? #0B0D10 :  // dark on lime always (lime stays bright in both themes)
                root.variant == "secondary" ? Colors.text-primary :
                root.variant == "danger" ? Colors.text-on-primary :
                Colors.text-primary;
            vertical-alignment: center;
        }
    }

    touch := TouchArea {
        enabled: !root.disabled;
        clicked => { root.clicked(); }
    }
}
```

Note: `inner-press-scale` is exposed as `in-out` so the gallery can read it for verification, but normal call sites ignore it. The only on-element scale Slint applies is via `Rectangle.scale` — we use `opacity` as the press tell instead because Slint Rectangles don't expose a transform-scale on themselves; a true scale-on-press would require a parent `Flickable` or layout wrapper. Opacity is fine as the press affordance for a touch-first UI.

- [ ] **Step 2: Activate the export**

In `ui/components/atomic/mod.slint`, replace `// (Task 4) export { Button } from "button.slint";` with `export { Button } from "button.slint";`.

- [ ] **Step 3: Add a gallery section**

Add to imports of `component_gallery.slint`:
```slint
import { Button } from "../../components/atomic/mod.slint";
```

Replace `// (Task 4) Button section appended here` with:

```slint
                // === Button ===
                Text {
                    text: "Button — variants";
                    font-family: Typography.font-family;
                    font-size: Typography.heading;
                    font-weight: Typography.semi-bold;
                    color: Colors.text-primary;
                }
                HorizontalLayout {
                    spacing: Spacing.md;
                    Button { label: "Primary"; variant: "primary"; }
                    Button { label: "Secondary"; variant: "secondary"; }
                    Button { label: "Danger"; variant: "danger"; }
                    Button { label: "Ghost"; variant: "ghost"; }
                    Button { label: "Disabled"; variant: "primary"; disabled: true; }
                }
```

- [ ] **Step 4: Compile-check**

Run:
```bash
cargo check 2>&1 | tail -10
```
Expected: compiles. If a `cannot find Button` error appears, the legacy `ui/components/button.slint` is still being imported elsewhere — confirm by `grep -rn 'components/button.slint' ui/`. The atomic Button lives at `components/atomic/button.slint`, the legacy at `components/button.slint`. They coexist.

- [ ] **Step 5: Commit**

```bash
git add ui/components/atomic/button.slint ui/components/atomic/mod.slint ui/screens/dev/component_gallery.slint
git commit -m "$(cat <<'EOF'
feat(ui): add atomic Button (primary/secondary/danger/ghost)

New touch-first button living in components/atomic/ — does not collide
with the legacy ui/components/button.slint. Press feedback via opacity
(Slint Rectangles don't expose self-scale; layout wrappers in Plan 3
add a true scale-on-press where it matters).

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 5: `SearchInput` component

**Files:**
- Create: `ui/components/atomic/search_input.slint`
- Modify: `ui/components/atomic/mod.slint`
- Modify: `ui/screens/dev/component_gallery.slint`

`SearchInput` is an inset-tier (tier 4) box with a leading magnifier glyph, the editable text, and a trailing clear `×` that appears once text is non-empty. Mirrors with `Layout.is-rtl`.

- [ ] **Step 1: Write the component**

Create `ui/components/atomic/search_input.slint`:

```slint
import { Surfaces } from "../../tokens/mod.slint";
import { Colors, Typography, Spacing, Radius, Layout } from "../../theme.slint";

// SearchInput — surface tier 4 (Inset). Leading glyph + editable text +
// trailing clear button. Mirrors with Layout.is-rtl.
export component SearchInput inherits Rectangle {
    in property <string> placeholder: "Search…";
    in-out property <string> value: "";
    callback changed(string);
    callback cleared;
    callback submitted(string);

    height: 44px;
    background: Surfaces.inset-top;
    border-color: Surfaces.inset-border;
    border-width: 1px;
    border-radius: Radius.md;

    HorizontalLayout {
        padding-left: Spacing.md;
        padding-right: Spacing.md;
        spacing: Spacing.sm;

        // Leading glyph (always on the leading edge — child order swaps with RTL)
        Text {
            text: "⌕";
            font-family: Typography.font-family;
            font-size: Typography.heading;
            color: Colors.text-secondary;
            vertical-alignment: center;
            // Hide and re-show via x to mirror — Slint has no `order:` property,
            // so we use `x` against parent.width when RTL.
            visible: !Layout.is-rtl;
        }

        TextInput {
            text <=> root.value;
            font-family: Typography.font-family;
            font-size: Typography.body;
            color: Colors.text-primary;
            single-line: true;
            horizontal-stretch: 1;
            vertical-alignment: center;
            horizontal-alignment: Layout.is-rtl ? right : left;
            edited => { root.changed(self.text); }
            accepted => { root.submitted(self.text); }
        }

        // Trailing clear button (only when value is non-empty).
        Rectangle {
            width: 28px;
            height: 28px;
            border-radius: 14px;
            background: Surfaces.surface-bottom;
            visible: root.value != "";
            TouchArea {
                clicked => {
                    root.value = "";
                    root.cleared();
                    root.changed("");
                }
            }
            Text {
                text: "×";
                font-family: Typography.font-family;
                font-size: Typography.body;
                color: Colors.text-secondary;
                horizontal-alignment: center;
                vertical-alignment: center;
            }
        }

        // RTL leading-glyph slot (renders on the right when RTL).
        Text {
            text: "⌕";
            font-family: Typography.font-family;
            font-size: Typography.heading;
            color: Colors.text-secondary;
            vertical-alignment: center;
            visible: Layout.is-rtl;
        }
    }

    // Placeholder (rendered as overlay when value is empty)
    Text {
        x: Layout.leading(40px, parent.width - self.width - 40px);
        y: 0;
        height: parent.height;
        text: root.placeholder;
        font-family: Typography.font-family;
        font-size: Typography.body;
        color: Colors.text-muted;
        vertical-alignment: center;
        visible: root.value == "";
    }
}
```

The "duplicated leading glyph + visibility flip" trick is necessary because Slint 1.14 does not have a `flex-direction` / `order` property on `HorizontalLayout`. The two glyph copies trade visibility on `Layout.is-rtl`; the always-rendered TextInput stays in the middle; the clear button is always on the trailing edge of the layout, which Slint draws in HorizontalLayout order — that order is the visual leading-to-trailing flow only in LTR. In RTL the visual rightmost is the layout-first child. We sidestep that by keeping the clear button in the middle slot's tail and letting the inactive glyph collapse to zero-width via `visible: false`. Slint's `visible: false` does not reserve space inside layouts.

- [ ] **Step 2: Activate the export**

In `ui/components/atomic/mod.slint`, replace `// (Task 5) export { SearchInput } from "search_input.slint";` with `export { SearchInput } from "search_input.slint";`.

- [ ] **Step 3: Add a gallery section**

Add to imports of `component_gallery.slint`:
```slint
import { SearchInput } from "../../components/atomic/mod.slint";
```

Replace `// (Task 5) SearchInput section appended here` with:

```slint
                // === SearchInput ===
                Text {
                    text: "SearchInput";
                    font-family: Typography.font-family;
                    font-size: Typography.heading;
                    font-weight: Typography.semi-bold;
                    color: Colors.text-primary;
                }
                VerticalLayout {
                    spacing: Spacing.sm;
                    SearchInput {
                        width: 360px;
                        placeholder: "Search products…";
                    }
                    SearchInput {
                        width: 360px;
                        placeholder: "Search products…";
                        value: "Latte";
                    }
                }
```

- [ ] **Step 4: Compile-check**

Run:
```bash
cargo check 2>&1 | tail -10
```
Expected: compiles. If Slint complains about `visible` reserving space, swap to a 0-width invisible Rectangle placeholder — but the docs (and current Slint vendor) confirm `visible: false` collapses inside layouts.

- [ ] **Step 5: Commit**

```bash
git add ui/components/atomic/search_input.slint ui/components/atomic/mod.slint ui/screens/dev/component_gallery.slint
git commit -m "$(cat <<'EOF'
feat(ui): add atomic SearchInput

Inset-tier search box with leading magnifier (mirrors via dual-slot
visibility because Slint 1.14 has no order/flex-direction property),
editable text, and trailing clear affordance that appears when the
value is non-empty. Submitted/changed/cleared callbacks for parents.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 6: `OpsButton` component

**Files:**
- Create: `ui/components/atomic/ops_button.slint`
- Modify: `ui/components/atomic/mod.slint`
- Modify: `ui/screens/dev/component_gallery.slint`

`OpsButton` is the operations-column button — a tall, fixed-width tile with a glyph and a small label below. Variants: `primary` (lime, used by `+1`), `neutral` (surface tier 3, used by `−1`/`×n`/`%`/`✎`), `danger` (red, used by `⌫`).

- [ ] **Step 1: Write the component**

Create `ui/components/atomic/ops_button.slint`:

```slint
import { Surfaces } from "../../tokens/mod.slint";
import { Colors, Typography, Spacing, Radius, Animation } from "../../theme.slint";

export component OpsButton inherits Rectangle {
    in property <string> glyph: "+1";
    in property <string> label: "ADD";
    // "primary" | "neutral" | "danger"
    in property <string> variant: "neutral";
    in property <bool> disabled: false;

    callback clicked;

    width: 88px;
    height: 88px;
    border-radius: Radius.md;
    border-width: 1px;

    background: disabled ? Surfaces.surface-bottom :
        variant == "primary" ? Colors.accent-lime :
        variant == "danger" ? Colors.danger :
        Surfaces.surface-top;
    border-color: variant == "neutral" ? Surfaces.surface-border : transparent;

    drop-shadow-color: Surfaces.surface-shadow;
    drop-shadow-blur: Surfaces.surface-shadow-blur;
    drop-shadow-offset-y: Surfaces.surface-shadow-offset-y;

    states [
        pressed when touch.pressed && !disabled: { opacity: 0.85; }
    ]
    animate opacity { duration: Animation.fast; }

    VerticalLayout {
        spacing: Spacing.xs;
        padding: Spacing.sm;
        alignment: center;

        Text {
            text: root.glyph;
            font-family: Typography.font-family-mono;
            font-size: Typography.title;
            font-weight: Typography.bold;
            color: root.disabled ? Colors.text-muted :
                root.variant == "primary" ? #0B0D10 :
                root.variant == "danger" ? Colors.text-on-primary :
                Colors.text-primary;
            horizontal-alignment: center;
        }
        Text {
            text: root.label;
            font-family: Typography.font-family;
            font-size: Typography.tiny;
            font-weight: Typography.semi-bold;
            color: root.disabled ? Colors.text-muted :
                root.variant == "primary" ? rgba(11, 13, 16, 0.7) :
                root.variant == "danger" ? rgba(255, 255, 255, 0.85) :
                Colors.text-secondary;
            horizontal-alignment: center;
        }
    }

    touch := TouchArea {
        enabled: !root.disabled;
        clicked => { root.clicked(); }
    }
}
```

- [ ] **Step 2: Activate the export**

In `ui/components/atomic/mod.slint`, replace `// (Task 6) export { OpsButton } from "ops_button.slint";` with `export { OpsButton } from "ops_button.slint";`.

- [ ] **Step 3: Add a gallery section**

Add to imports:
```slint
import { OpsButton } from "../../components/atomic/mod.slint";
```

Replace `// (Task 6) OpsButton section appended here` with:

```slint
                // === OpsButton ===
                Text {
                    text: "OpsButton — operations column";
                    font-family: Typography.font-family;
                    font-size: Typography.heading;
                    font-weight: Typography.semi-bold;
                    color: Colors.text-primary;
                }
                HorizontalLayout {
                    spacing: Spacing.sm;
                    OpsButton { glyph: "+1"; label: "ADD"; variant: "primary"; }
                    OpsButton { glyph: "−1"; label: "REMOVE"; }
                    OpsButton { glyph: "×n"; label: "QTY"; }
                    OpsButton { glyph: "%"; label: "DISCOUNT"; }
                    OpsButton { glyph: "✎"; label: "EDIT"; }
                    OpsButton { glyph: "⌫"; label: "VOID"; variant: "danger"; }
                    OpsButton { glyph: "+1"; label: "DISABLED"; variant: "primary"; disabled: true; }
                }
```

- [ ] **Step 4: Compile-check + commit**

Run:
```bash
cargo check 2>&1 | tail -10
```
Expected: compiles.

```bash
git add ui/components/atomic/ops_button.slint ui/components/atomic/mod.slint ui/screens/dev/component_gallery.slint
git commit -m "$(cat <<'EOF'
feat(ui): add atomic OpsButton

Tall 88x88 tile with mono glyph + small label for the operations
column (ON SELECTED). Variants: primary (lime, used by +1), neutral
(surface, default), danger (red, used by ⌫).

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 7: `StatusLED` component

**Files:**
- Create: `ui/components/atomic/status_led.slint`
- Modify: `ui/components/atomic/mod.slint`
- Modify: `ui/screens/dev/component_gallery.slint`

`StatusLED` is a 10 dp dot with a soft halo, used in the header for online/offline/syncing state. Three states: `online` (lime), `offline` (warning amber), `syncing` (info blue, pulses).

- [ ] **Step 1: Write the component**

Create `ui/components/atomic/status_led.slint`:

```slint
import { Colors, Animation } from "../../theme.slint";

export component StatusLED inherits Rectangle {
    // "online" | "offline" | "syncing"
    in property <string> state: "online";

    width: 14px;
    height: 14px;

    out property <color> dot-color:
        state == "offline" ? Colors.warning :
        state == "syncing" ? Colors.info :
        Colors.accent-lime;

    // Halo
    Rectangle {
        width: 14px;
        height: 14px;
        border-radius: 7px;
        background: root.dot-color;
        opacity: 0.25;
        animate opacity {
            duration: 1200ms;
            iteration-count: -1;
            easing: ease-in-out;
        }
        // Pulse only when syncing — pinned-opacity otherwise
        // (Slint can't disable an animation conditionally, so we drive the
        // value through a state.)
        states [
            pulsing when root.state == "syncing": { opacity: 0.55; }
            steady when root.state != "syncing": { opacity: 0.25; }
        ]
    }
    // Core dot
    Rectangle {
        x: 3px;
        y: 3px;
        width: 8px;
        height: 8px;
        border-radius: 4px;
        background: root.dot-color;
    }
}
```

- [ ] **Step 2: Activate the export**

In `ui/components/atomic/mod.slint`, replace `// (Task 7) export { StatusLED } from "status_led.slint";` with `export { StatusLED } from "status_led.slint";`.

- [ ] **Step 3: Add a gallery section**

Add to imports:
```slint
import { StatusLED } from "../../components/atomic/mod.slint";
```

Replace `// (Task 7) StatusLED section appended here` with:

```slint
                // === StatusLED ===
                Text {
                    text: "StatusLED";
                    font-family: Typography.font-family;
                    font-size: Typography.heading;
                    font-weight: Typography.semi-bold;
                    color: Colors.text-primary;
                }
                HorizontalLayout {
                    spacing: Spacing.lg;
                    alignment: start;
                    HorizontalLayout {
                        spacing: Spacing.sm;
                        alignment: center;
                        StatusLED { state: "online"; }
                        Text {
                            text: "Online";
                            font-family: Typography.font-family;
                            font-size: Typography.caption;
                            color: Colors.text-primary;
                            vertical-alignment: center;
                        }
                    }
                    HorizontalLayout {
                        spacing: Spacing.sm;
                        alignment: center;
                        StatusLED { state: "offline"; }
                        Text {
                            text: "Offline";
                            font-family: Typography.font-family;
                            font-size: Typography.caption;
                            color: Colors.text-primary;
                            vertical-alignment: center;
                        }
                    }
                    HorizontalLayout {
                        spacing: Spacing.sm;
                        alignment: center;
                        StatusLED { state: "syncing"; }
                        Text {
                            text: "Syncing";
                            font-family: Typography.font-family;
                            font-size: Typography.caption;
                            color: Colors.text-primary;
                            vertical-alignment: center;
                        }
                    }
                }
```

- [ ] **Step 4: Compile-check + commit**

Run:
```bash
cargo check 2>&1 | tail -10
```
Expected: compiles.

```bash
git add ui/components/atomic/status_led.slint ui/components/atomic/mod.slint ui/screens/dev/component_gallery.slint
git commit -m "$(cat <<'EOF'
feat(ui): add atomic StatusLED (online/offline/syncing)

14×14 dot with halo. Lime when online, warning amber when offline,
info blue with subtle pulse when syncing. Used by the header chrome.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 8: `PayButton` component

**Files:**
- Create: `ui/components/atomic/pay_button.slint`
- Modify: `ui/components/atomic/mod.slint`
- Modify: `ui/screens/dev/component_gallery.slint`

The single most-important button in the UI. Single horizontal strip — label on the leading edge, mono total on the trailing edge — with the lit lime gradient in dark and the deep `#15803D` solid in light. Halo glow in dark mode only.

- [ ] **Step 1: Write the component**

Create `ui/components/atomic/pay_button.slint`:

```slint
import { Theme, Surfaces } from "../../tokens/mod.slint";
import { Colors, Typography, Spacing, Radius, Animation, Layout } from "../../theme.slint";

export component PayButton inherits Rectangle {
    in property <string> label: "PAY";
    in property <string> total: "0.00";
    in property <string> currency: "LYD";
    in property <bool> disabled: false;

    callback clicked;

    height: 64px;
    border-radius: Radius.md;

    // Solid green in light, lime gradient in dark.
    background: Theme.is-dark
        ? @linear-gradient(180deg, Colors.pay-green-bg-stop-1 0%, Colors.pay-green-bg-stop-2 100%)
        : Colors.pay-green;
    opacity: root.disabled ? 0.5 : 1.0;

    // Halo glow (dark mode only). A larger semi-transparent lime rectangle
    // sitting behind the button, blurred via drop-shadow.
    drop-shadow-color: Theme.is-dark ? Colors.accent-lime : transparent;
    drop-shadow-blur: 24px;
    drop-shadow-offset-y: 0px;

    // Specular highlight on top edge.
    Rectangle {
        x: 1px; y: 1px;
        width: parent.width - 2px;
        height: 1px;
        background: Surfaces.specular-strong;
        opacity: 0.55;
        border-radius: parent.border-radius;
    }

    states [
        pressed when touch.pressed && !root.disabled: { opacity: 0.85; }
    ]
    animate opacity { duration: Animation.fast; }

    HorizontalLayout {
        padding-left: Spacing.lg;
        padding-right: Spacing.lg;
        spacing: Spacing.md;
        alignment: space-between;

        // Leading: label
        Text {
            text: root.label;
            font-family: Typography.font-family;
            font-size: Typography.title;
            font-weight: Typography.bold;
            color: Colors.text-on-pay;
            vertical-alignment: center;
            horizontal-alignment: Layout.is-rtl ? right : left;
        }

        // Trailing: mono total + currency
        HorizontalLayout {
            spacing: Spacing.xs;
            alignment: center;
            Text {
                text: root.total;
                font-family: Typography.font-family-mono;
                font-size: Typography.title;
                font-weight: Typography.bold;
                color: Colors.text-on-pay;
                vertical-alignment: center;
            }
            Text {
                text: root.currency;
                font-family: Typography.font-family;
                font-size: Typography.body;
                font-weight: Typography.semi-bold;
                color: Colors.text-on-pay;
                vertical-alignment: center;
                opacity: 0.85;
            }
        }
    }

    touch := TouchArea {
        enabled: !root.disabled;
        clicked => { root.clicked(); }
    }
}
```

Numbers stay LTR even in RTL (per spec §7.2) — Slint's bidi handler reads the `total` string as a numeric run and lays it out LTR automatically inside the trailing layout.

- [ ] **Step 2: Activate the export**

In `ui/components/atomic/mod.slint`, replace `// (Task 8) export { PayButton } from "pay_button.slint";` with `export { PayButton } from "pay_button.slint";`.

- [ ] **Step 3: Add a gallery section**

Add to imports:
```slint
import { PayButton } from "../../components/atomic/mod.slint";
```

Replace `// (Task 8) PayButton section appended here` with:

```slint
                // === PayButton ===
                Text {
                    text: "PayButton";
                    font-family: Typography.font-family;
                    font-size: Typography.heading;
                    font-weight: Typography.semi-bold;
                    color: Colors.text-primary;
                }
                VerticalLayout {
                    spacing: Spacing.md;
                    PayButton {
                        width: 360px;
                        label: Locale.current == "ar" ? "ادفع" : "PAY";
                        total: "12.600";
                        currency: "LYD";
                    }
                    PayButton {
                        width: 360px;
                        label: Locale.current == "ar" ? "ادفع" : "PAY";
                        total: "0.00";
                        currency: "LYD";
                        disabled: true;
                    }
                }
```

- [ ] **Step 4: Compile-check + commit**

Run:
```bash
cargo check 2>&1 | tail -10
```
Expected: compiles.

```bash
git add ui/components/atomic/pay_button.slint ui/components/atomic/mod.slint ui/screens/dev/component_gallery.slint
git commit -m "$(cat <<'EOF'
feat(ui): add atomic PayButton

Single horizontal strip — label on leading edge, mono total on trailing.
Lit lime gradient + halo in dark; deep #15803D solid in light. Numbers
stay LTR inside RTL via Slint's native bidi.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 9: `ProductTile` component

**Files:**
- Create: `ui/components/atomic/product_tile.slint`
- Modify: `ui/components/atomic/mod.slint`
- Modify: `ui/screens/dev/component_gallery.slint`

`ProductTile` is the largest single repeating element in the UI. Surface tier 3, fixed aspect ratio (set by parent), category-accent colour as the leading-edge border. Press feedback per the press convention. Selected state isn't a thing on tiles (selection happens on cart lines, not products).

- [ ] **Step 1: Write the component**

Create `ui/components/atomic/product_tile.slint`:

```slint
import { Surfaces } from "../../tokens/mod.slint";
import { Colors, Typography, Spacing, Radius, Animation, Layout, Locale } from "../../theme.slint";

export component ProductTile inherits Rectangle {
    in property <string> name: "Product";
    in property <string> price: "0.00";
    in property <string> currency: "LYD";
    // Accent colour for the leading-edge stripe. Pass any of Colors.cat-*
    // or any other colour to opt into a different category.
    in property <color> category-accent: Colors.cat-coffee;
    in property <bool> disabled: false;
    // Optional out-of-stock banner.
    in property <bool> out-of-stock: false;

    callback clicked;

    border-radius: Radius.md;
    border-width: 1px;
    border-color: Surfaces.surface-border;
    background: Surfaces.surface-top;
    drop-shadow-color: Surfaces.surface-shadow;
    drop-shadow-blur: Surfaces.surface-shadow-blur;
    drop-shadow-offset-y: Surfaces.surface-shadow-offset-y;

    // Leading-edge accent stripe (3 dp wide, full height).
    Rectangle {
        x: Layout.leading(0px, parent.width - 3px);
        y: 0;
        width: 3px;
        height: parent.height;
        background: root.disabled ? Colors.text-muted : root.category-accent;
        border-top-left-radius: Layout.is-rtl ? 0 : parent.border-radius;
        border-bottom-left-radius: Layout.is-rtl ? 0 : parent.border-radius;
        border-top-right-radius: Layout.is-rtl ? parent.border-radius : 0;
        border-bottom-right-radius: Layout.is-rtl ? parent.border-radius : 0;
    }

    states [
        pressed when touch.pressed && !root.disabled: { opacity: 0.88; }
    ]
    animate opacity { duration: Animation.fast; }

    VerticalLayout {
        padding-left: Layout.leading(Spacing.md + 3px, Spacing.md);
        padding-right: Layout.trailing(Spacing.md + 3px, Spacing.md);
        padding-top: Spacing.md;
        padding-bottom: Spacing.md;
        spacing: Spacing.xs;

        Text {
            text: root.name;
            font-family: Typography.font-family;
            font-size: Typography.body;
            font-weight: Typography.semi-bold;
            color: root.disabled ? Colors.text-muted : Colors.text-primary;
            horizontal-stretch: 1;
            wrap: word-wrap;
            horizontal-alignment: Layout.is-rtl ? right : left;
        }

        // Price row, mono numerics + caption-sized currency.
        HorizontalLayout {
            spacing: Spacing.xs;
            alignment: Layout.is-rtl ? end : start;
            Text {
                text: root.price;
                font-family: Typography.font-family-mono;
                font-size: Typography.heading;
                font-weight: Typography.bold;
                color: root.disabled ? Colors.text-muted : Colors.text-primary;
            }
            Text {
                text: root.currency;
                font-family: Typography.font-family;
                font-size: Typography.caption;
                color: Colors.text-secondary;
                vertical-alignment: bottom;
            }
        }

        // Out-of-stock pill (replaces the price row visually when set).
        Rectangle {
            visible: root.out-of-stock;
            height: 22px;
            border-radius: 11px;
            background: Colors.danger;
            HorizontalLayout {
                padding-left: Spacing.sm;
                padding-right: Spacing.sm;
                alignment: center;
                Text {
                    text: Locale.current == "ar" ? "غير متوفر" : "OUT OF STOCK";
                    font-family: Typography.font-family;
                    font-size: Typography.tiny;
                    font-weight: Typography.bold;
                    color: Colors.text-on-primary;
                    vertical-alignment: center;
                }
            }
        }
    }

    touch := TouchArea {
        enabled: !root.disabled;
        clicked => { root.clicked(); }
    }
}
```

The accent stripe rounds only its outside corners (the ones that touch the tile edge); the inside corners are square so it visually flushes against the tile body. `Locale` is in the import line for the out-of-stock string ("غير متوفر" / "OUT OF STOCK").

- [ ] **Step 2: Activate the export**

In `ui/components/atomic/mod.slint`, replace `// (Task 9) export { ProductTile } from "product_tile.slint";` with `export { ProductTile } from "product_tile.slint";`.

- [ ] **Step 3: Add a gallery section**

Add to imports of `component_gallery.slint`:
```slint
import { ProductTile } from "../../components/atomic/mod.slint";
```

Replace `// (Task 9) ProductTile section appended here` with:

```slint
                // === ProductTile ===
                Text {
                    text: "ProductTile — category accents";
                    font-family: Typography.font-family;
                    font-size: Typography.heading;
                    font-weight: Typography.semi-bold;
                    color: Colors.text-primary;
                }
                HorizontalLayout {
                    spacing: Spacing.md;
                    ProductTile {
                        width: 180px; height: 120px;
                        name: Locale.current == "ar" ? "قهوة لاتيه" : "Café Latte";
                        price: "12.500";
                        currency: "LYD";
                        category-accent: Colors.cat-coffee;
                    }
                    ProductTile {
                        width: 180px; height: 120px;
                        name: Locale.current == "ar" ? "كرواسون" : "Croissant";
                        price: "6.000";
                        currency: "LYD";
                        category-accent: Colors.cat-bakery;
                    }
                    ProductTile {
                        width: 180px; height: 120px;
                        name: Locale.current == "ar" ? "ماء بارد" : "Cold Water";
                        price: "1.500";
                        currency: "LYD";
                        category-accent: Colors.cat-cold;
                    }
                    ProductTile {
                        width: 180px; height: 120px;
                        name: Locale.current == "ar" ? "ساندويش" : "Sandwich";
                        price: "18.000";
                        currency: "LYD";
                        category-accent: Colors.cat-food;
                        out-of-stock: true;
                    }
                }
```

- [ ] **Step 4: Compile-check + commit**

Run:
```bash
cargo check 2>&1 | tail -10
```
Expected: compiles.

```bash
git add ui/components/atomic/product_tile.slint ui/components/atomic/mod.slint ui/screens/dev/component_gallery.slint
git commit -m "$(cat <<'EOF'
feat(ui): add atomic ProductTile

Surface tier 3 with a 3 dp leading-edge stripe in the per-category
accent colour (Layout helpers flip its side in RTL). Mono price +
caption currency; out-of-stock pill swap when stock=0. Press feedback
via opacity per the touch-first convention.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 10: `CartLine` component

**Files:**
- Create: `ui/components/atomic/cart_line.slint`
- Modify: `ui/components/atomic/mod.slint`
- Modify: `ui/screens/dev/component_gallery.slint`

`CartLine` is one row in the cart panel. Layout: qty pill on the leading edge, name + unit price stacked in the middle (stretch), line total on the trailing edge in mono. Selected state is the only non-default state — applies a lime glow border and slightly raised opacity.

- [ ] **Step 1: Write the component**

Create `ui/components/atomic/cart_line.slint`:

```slint
import { Surfaces } from "../../tokens/mod.slint";
import { Colors, Typography, Spacing, Radius, Animation, Layout } from "../../theme.slint";

export component CartLine inherits Rectangle {
    in property <string> name: "Item";
    in property <int> qty: 1;
    in property <string> unit-price: "0.00";
    in property <string> line-total: "0.00";
    in property <string> currency: "LYD";
    in property <bool> selected: false;

    callback clicked;

    height: 64px;
    border-radius: Radius.md;
    background: Surfaces.surface-top;
    border-width: 1px;
    border-color: root.selected ? Colors.accent-lime : Surfaces.surface-border;
    drop-shadow-color: root.selected ? Colors.accent-lime : Surfaces.surface-shadow;
    drop-shadow-blur: root.selected ? 20px : Surfaces.surface-shadow-blur;
    drop-shadow-offset-y: root.selected ? 0px : Surfaces.surface-shadow-offset-y;

    animate border-color { duration: Animation.normal; easing: ease-out; }

    HorizontalLayout {
        padding-left: Spacing.md;
        padding-right: Spacing.md;
        spacing: Spacing.md;

        // Qty pill on the leading edge. Dual-slot trick: render the pill
        // first in LTR, render an invisible spacer first in RTL so the pill
        // is the layout-tail child (which is visually leading in RTL).
        Rectangle {
            visible: !Layout.is-rtl;
            width: 44px;
            height: 32px;
            border-radius: 16px;
            background: Surfaces.inset-top;
            border-color: Surfaces.inset-border;
            border-width: 1px;
            y: (parent.height - self.height) / 2;
            Text {
                text: "×" + root.qty;
                font-family: Typography.font-family-mono;
                font-size: Typography.body;
                font-weight: Typography.semi-bold;
                color: Colors.text-primary;
                horizontal-alignment: center;
                vertical-alignment: center;
            }
        }

        // Centre stack: name + unit price line.
        VerticalLayout {
            horizontal-stretch: 1;
            alignment: center;
            spacing: Spacing.xxs;
            Text {
                text: root.name;
                font-family: Typography.font-family;
                font-size: Typography.body;
                font-weight: Typography.semi-bold;
                color: Colors.text-primary;
                horizontal-alignment: Layout.is-rtl ? right : left;
                wrap: no-wrap;
                overflow: elide;
            }
            HorizontalLayout {
                spacing: Spacing.xs;
                alignment: Layout.is-rtl ? end : start;
                Text {
                    text: "@";
                    font-family: Typography.font-family;
                    font-size: Typography.caption;
                    color: Colors.text-secondary;
                    vertical-alignment: center;
                }
                Text {
                    text: root.unit-price;
                    font-family: Typography.font-family-mono;
                    font-size: Typography.caption;
                    color: Colors.text-secondary;
                    vertical-alignment: center;
                }
            }
        }

        // Line total on the trailing edge.
        HorizontalLayout {
            spacing: Spacing.xs;
            alignment: center;
            Text {
                text: root.line-total;
                font-family: Typography.font-family-mono;
                font-size: Typography.body;
                font-weight: Typography.bold;
                color: Colors.text-primary;
                vertical-alignment: center;
            }
            Text {
                text: root.currency;
                font-family: Typography.font-family;
                font-size: Typography.tiny;
                color: Colors.text-secondary;
                vertical-alignment: bottom;
            }
        }

        // RTL qty pill slot — fires when Layout.is-rtl. Last child = visual
        // leading edge in RTL.
        Rectangle {
            visible: Layout.is-rtl;
            width: 44px;
            height: 32px;
            border-radius: 16px;
            background: Surfaces.inset-top;
            border-color: Surfaces.inset-border;
            border-width: 1px;
            y: (parent.height - self.height) / 2;
            Text {
                text: "×" + root.qty;
                font-family: Typography.font-family-mono;
                font-size: Typography.body;
                font-weight: Typography.semi-bold;
                color: Colors.text-primary;
                horizontal-alignment: center;
                vertical-alignment: center;
            }
        }
    }

    touch := TouchArea {
        clicked => { root.clicked(); }
    }
}
```

The dual-slot pill mirrors the dual-slot leading glyph in `SearchInput` — Slint 1.14 has no `flex-direction: row-reverse`, so we use `visible:` to pick which copy renders.

- [ ] **Step 2: Activate the export**

In `ui/components/atomic/mod.slint`, replace `// (Task 10) export { CartLine } from "cart_line.slint";` with `export { CartLine } from "cart_line.slint";`.

- [ ] **Step 3: Add a gallery section**

Add to imports of `component_gallery.slint`:
```slint
import { CartLine } from "../../components/atomic/mod.slint";
```

Replace `// (Task 10) CartLine section appended here` with:

```slint
                // === CartLine ===
                Text {
                    text: "CartLine — selected vs default";
                    font-family: Typography.font-family;
                    font-size: Typography.heading;
                    font-weight: Typography.semi-bold;
                    color: Colors.text-primary;
                }
                VerticalLayout {
                    spacing: Spacing.sm;
                    CartLine {
                        width: 360px;
                        name: Locale.current == "ar" ? "قهوة لاتيه" : "Café Latte";
                        qty: 2;
                        unit-price: "12.500";
                        line-total: "25.000";
                        currency: "LYD";
                        selected: true;
                    }
                    CartLine {
                        width: 360px;
                        name: Locale.current == "ar" ? "كرواسون" : "Croissant";
                        qty: 1;
                        unit-price: "6.000";
                        line-total: "6.000";
                        currency: "LYD";
                    }
                    CartLine {
                        width: 360px;
                        name: Locale.current == "ar" ? "ساندويش بالدجاج المشوي والخضار الطازجة" : "Grilled Chicken Sandwich w/ Fresh Veg";
                        qty: 1;
                        unit-price: "18.000";
                        line-total: "18.000";
                        currency: "LYD";
                    }
                }
```

The third instance has a deliberately long name to verify text elision.

- [ ] **Step 4: Compile-check + commit**

Run:
```bash
cargo check 2>&1 | tail -10
```
Expected: compiles.

```bash
git add ui/components/atomic/cart_line.slint ui/components/atomic/mod.slint ui/screens/dev/component_gallery.slint
git commit -m "$(cat <<'EOF'
feat(ui): add atomic CartLine

Qty pill (mirrored via dual-slot pattern) + name/unit-price stack +
mono line total. Selected state replaces the surface border with a
lime glow and bumps the drop-shadow up. Long names elide.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 11: Final verification + visual sign-off

**Files:**
- Modify: `docs/POS-UI-REDESIGN-FINDINGS-01-SLINT-RTL.md` (append "Visual verification — Plan 2" section)
- Create: `docs/POS-UI-REDESIGN-SCREENSHOTS-PLAN-02/` (operator screenshot directory)

This task is operator-driven. The agent verifies build, lints, and tests; the operator runs `cargo run -- --component-gallery` on a workstation and captures the four-config screenshot matrix.

- [ ] **Step 1: Confirm clean build, lint, and test**

Run:
```bash
cargo build 2>&1 | tail -5
cargo fmt --all --check 2>&1 | tail -5
cargo clippy --workspace --all-targets -- -D warnings -A clippy::too_many_arguments 2>&1 | tail -10
cargo test --workspace -- --skip e2e_ 2>&1 | tail -10
```
Expected: all four exit 0, no warnings, all tests pass (including the new `locale_detect_tests`).

- [ ] **Step 2: Operator runs the gallery in 4 configurations**

On a workstation:
```bash
LANG=en_US.UTF-8 cargo run -- --component-gallery   # opens light + LTR + en
```

Click toolbar to step through:
1. Light + LTR + EN
2. Light + RTL + AR
3. Dark + LTR + EN
4. Dark + RTL + AR

Take 4 screenshots. Save under `docs/POS-UI-REDESIGN-SCREENSHOTS-PLAN-02/`:
- `01-light-ltr-en.png`
- `02-light-rtl-ar.png`
- `03-dark-ltr-en.png`
- `04-dark-rtl-ar.png`

Visual checks per config:
- All eight components render and respond to press.
- Arabic shapes correctly (no boxes, no separated letters).
- Numeric strings (price, line-total) stay LTR even in RTL mode.
- ProductTile's leading stripe is on the **right** in RTL, **left** in LTR.
- CartLine's qty pill follows the same rule.
- PayButton lights up (lime gradient + halo) only in dark.
- StatusLED `syncing` pulse animates.
- Press states (opacity dip) work on every clickable component.

- [ ] **Step 3: Append findings**

Edit `docs/POS-UI-REDESIGN-FINDINGS-01-SLINT-RTL.md`. After the existing `## Foundation verification (Task 10)` section, append:

```markdown
## Visual verification — Plan 2 (Atomic Components)

Date: <YYYY-MM-DD>
Verified-by: <operator>

### Configuration matrix
- Light + LTR + EN — pass / observed issues: <none | list>
- Light + RTL + AR — pass / observed issues: <none | list>
- Dark + LTR + EN — pass / observed issues: <none | list>
- Dark + RTL + AR — pass / observed issues: <none | list>

### Screenshots
See `docs/POS-UI-REDESIGN-SCREENSHOTS-PLAN-02/01-light-ltr-en.png` through
`04-dark-rtl-ar.png`.

### Component checks
- Panel — surface tokens correctly differ between themes: PASS / FAIL
- Button — all four variants render: PASS / FAIL
- SearchInput — dual-slot magnifier mirrors correctly: PASS / FAIL
- OpsButton — primary/neutral/danger render: PASS / FAIL
- StatusLED — syncing pulse animates: PASS / FAIL
- PayButton — dark-mode halo present, light-mode solid green: PASS / FAIL
- ProductTile — accent stripe on correct edge per direction: PASS / FAIL
- CartLine — qty pill on correct edge, selected glow visible: PASS / FAIL

### Issues / follow-ups
<list — empty if all green>
```

- [ ] **Step 4: Commit screenshots + finalised findings**

```bash
git add docs/POS-UI-REDESIGN-SCREENSHOTS-PLAN-02/ docs/POS-UI-REDESIGN-FINDINGS-01-SLINT-RTL.md
git commit -m "$(cat <<'EOF'
docs(pos): plan 2 visual verification — 4 configurations captured

Component gallery screenshots in light/dark × LTR/RTL × en/ar prove
the atomic components honour the RTL helpers and theme-aware tokens.
Findings doc updated with per-component pass/fail.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

- [ ] **Step 5: Push the branch**

```bash
git push -u origin worktree-pos-ui-redesign-foundation 2>&1 | tail -5
```

Branch stays unmerged until Plan 3 (main checkout screen) lands and at least one real screen consumes the atomic components.

---

## Done criteria

This plan is complete when:

1. `cargo build` succeeds.
2. `cargo fmt --all --check` exits clean.
3. `cargo clippy --workspace --all-targets -- -D warnings -A clippy::too_many_arguments` exits clean.
4. `cargo test --workspace -- --skip e2e_` passes (including `locale_detect_tests`).
5. `cargo run -- --component-gallery` opens a window with toolbar toggles working and all eight component sections rendering.
6. All four configurations (light/dark × LTR/RTL × en/ar) render correctly per the Task 11 component-checks list.
7. `.github/workflows/ci.yml` exists and the same gates pass on push.
8. The findings doc carries a "Visual verification — Plan 2" section with the operator-completed matrix.
9. All eleven task commits are in `worktree-pos-ui-redesign-foundation` and pushed to origin.

---

## What this plan deliberately does not do

- Does not migrate any existing screen to use the atomic components — Plan 3 (main checkout) is the first consumer.
- Does not delete or modify any file under `ui/components/*.slint` outside `atomic/` — legacy components still drive the live screens until Plan 3.
- Does not refactor `run_startup_sequence` to fix the 8-arg clippy warning — `clippy::too_many_arguments` is allow-listed in the CI gate; the refactor is its own follow-up task.
- Does not implement the "MORE" overflow menu, header chrome, footer chrome, or payment screen tiles — those are Plan 3+ scope.
- Does not implement gradient tile variants beyond the PayButton lit gradient — Tier-3 surface tiles use solid colours per Plan 1's `Surfaces.surface-top`.
- Does not change tenant config or backend wiring — the locale detector is a developer-machine convenience only.

---

## Open items uncovered during planning

- **Slint 1.14 may collapse `visible: false` differently inside layouts.** The `SearchInput` and `CartLine` components rely on `visible: false` taking zero layout space. The Plan 1 theme harness already uses `visible:` patterns and works, so this is verified at the runtime level, but if any of these tiles look subtly off in the gallery, the fallback is a `Layout.is-rtl ? component-a : component-b` conditional component instantiation — costlier in source, identical at runtime.
- **`PayButton` halo may be invisible against a dark Surfaces.bg-bottom.** `drop-shadow-color: Colors.accent-lime` should produce a glow regardless of background, but if the visual is too subtle on the dark theme, Plan 3 wraps the PayButton in a small Rectangle with its own outer drop-shadow as a halo amplifier. Decide after operator screenshots in Task 11.
- **Press feedback uses `opacity` rather than a true scale.** The brainstorming spec mentioned an 80 ms scale 0.97→1.0 ease-out on tile press. Slint Rectangles don't expose a self-scale property; achieving true scale requires a parent layout wrapper with `width`/`height` animations. If the press affordance feels weak on real hardware (Task 11 operator review), a follow-up task adds a `PressableScaleWrapper` that the atomic components compose into.
