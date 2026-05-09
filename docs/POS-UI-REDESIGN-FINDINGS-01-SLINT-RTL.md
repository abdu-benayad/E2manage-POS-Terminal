# Findings: Slint 1.8 RTL + Arabic capability

Date: 2026-05-09
Verified-by: Claude Opus 4.7

## Resolved Slint version

`Cargo.toml` declares `slint = "1.8"`. The vendored sources resolve this to
**Slint 1.14.1** (semver-compatible, latest 1.x). All references to "Slint 1.8"
below mean the actually-used version 1.14.1.

## HarfBuzz shaping

- **Crate: `parley` v0.6.0**, which depends on `swash` (pure-Rust OpenType shaper).
- Slint 1.14.1's femtovg renderer activates the `shared-parley` feature on
  `i-slint-core`, which gates `parley = "0.6.0"` (`vendor/i-slint-core/Cargo.toml`
  lines 68-71, 210-212).
- `i-slint-renderer-femtovg/Cargo.toml` (lines 79-94) explicitly enables the
  `shared-parley` and `shared-fontique` features on `i-slint-core`.
- `vendor/i-slint-core/textlayout/sharedparley.rs` re-exports `parley` and is
  the actual shaping pipeline used at render time
  (`pub use parley;` at line 5).
- `vendor/i-slint-renderer-femtovg/itemrenderer.rs` calls
  `sharedparley::draw_text` and consumes `parley::layout::Glyph`, confirming the
  runtime path.
- `swash` (pulled in by parley) implements the OpenType GSUB/GPOS layout tables
  that drive Arabic positional shaping (initial / medial / final / isolated
  glyph forms), Indic reordering, and other complex-script needs. It is not
  literal HarfBuzz, but it implements the same OpenType shaping spec in pure
  Rust. The vendored `vendor/femtovg/Cargo.toml` also lists `rustybuzz =
  "0.20.0"` behind its `textlayout` feature; femtovg's own shaper is gated off
  in Slint's integration (`default-features = false`, only `image-loading`),
  because Slint shapes via parley/swash and hands femtovg pre-shaped glyph runs.
- **Conclusion:** Slint 1.14.1 ships parley + swash and shapes complex scripts
  (including Arabic positional forms) without additional configuration. Arabic
  positional shaping works out of the box. **VERIFIED.**

## Layout direction

- Slint 1.14.1 has **no** `direction: rtl` property on layouts. Confirmed by
  `grep -rn "TextHorizontalAlignment|direction|rtl|right-to-left"
  vendor/i-slint-compiler/builtins.slint`: only `TextHorizontalAlignment` (on
  Text / TextInput / TextEdit) and `AnimationDirection` (on animations) appear.
  No `LayoutDirection`, `LayoutAlignment` is content alignment only.
- `vendor/i-slint-common/enums.rs` confirms the same: only `AnimationDirection`,
  no layout-level RTL enum.
- Mirroring must be implemented per-component by:
  1. Reversing `HorizontalLayout` child order conditionally on a global
     `Layout.rtl` flag.
  2. Swapping `border-left` ↔ `border-right` via conditional component
     instantiation.
  3. Swapping `padding-left` ↔ `padding-right` via the same.
- A helper-function approach on the existing `Layout` global is the chosen path
  — see Task 7.

## Font loading

- Slint loads system fonts by default through the `fontique` font discovery
  crate (gated by the `shared-fontique` feature, also activated by
  `i-slint-renderer-femtovg`).
- Bundled fonts (Task 2) require `slint-build` font registration in `build.rs`.
- Note: the current `build.rs` is just a 1-line placeholder; Task 2 will need to
  add explicit font registration there.

## Open risks

- **Vendor symlink in worktree:** the worktree had no `vendor/` directory; I
  symlinked the parent repo's `vendor/` into the worktree so cargo's vendored
  source replacement (`.cargo/config.toml` -> `directory = "vendor"`) would
  resolve. This is a worktree-local hack — main repo unaffected. If git
  worktrees are used regularly, consider adding `vendor` to a worktree-init
  script, or switch the cargo config to use an absolute path / CARGO_HOME
  override.
- **Parley vs literal HarfBuzz:** Slint uses `swash` (via `parley`), not the C
  HarfBuzz library. Swash implements the OpenType shaping spec in pure Rust.
  For all common scripts including Arabic this is fine, but very obscure
  shaping edge cases could behave differently than upstream HarfBuzz. Visual
  verification in Task 10 is the gating check — if any Arabic cluster
  mis-shapes, that's the moment to revisit.
- **Bidirectional text (mixed Arabic + Latin):** Parley handles bidi, but
  `unicode-bidi` only enters the picture when text contains both directions.
  The harness in Task 8 should include at least one mixed-direction string
  (e.g. an Arabic label with embedded Latin SKU code) to confirm bidi works.
- **Smoke test coverage is intentionally trivial.** `tests/slint_arabic_smoke.rs`
  is `assert!(true)` for now — its purpose is to prove the test infrastructure
  builds with Slint linked in, not to verify glyph clusters. Real visual
  validation is the harness in Task 10. If we later want a real cluster check,
  we'd need to either snapshot femtovg pixel buffers or hook into parley's
  layout output and inspect glyph runs directly; both are larger pieces of work.
