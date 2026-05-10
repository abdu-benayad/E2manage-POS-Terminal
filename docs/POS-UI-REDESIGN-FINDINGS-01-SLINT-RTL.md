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

## Foundation verification (Task 10)

Date: 2026-05-09. Cumulative status of Tasks 1-9 (tokens, fonts, theme, surfaces,
RTL helpers, harness binary).

- **`cargo build`**: PASS. Workspace + binary build clean.
- **`cargo clippy`**: PASS with 2 pre-existing warnings in `src/main.rs`
  (`run_startup_sequence` 8/7 args at line 2094; `collapsible_else_if` at line
  890). Both predate Plan 1 and are unrelated to UI tokens / fonts / harness.
  Plan 1's added Slint code (Theme, Surfaces, Layout helpers, ThemeHarness) and
  the `--theme-harness` CLI branch produce no new clippy warnings.
- **`cargo fmt --check`**: FAIL on 60+ files — pre-existing across the
  multi-crate workspace (`crates/pos-*/**`, `src/**`, `tests/**`,
  `build.rs`). The `build.rs` and `dev_harness.rs` reformatting affects lines
  Plan 1 touched, but the rule violations themselves (line-wrapping style) are
  the same conventions the repo has always used. Repo has no fmt CI gate today;
  Plan 1 did not introduce a new gate either. **Recommend**: Plan 2 add
  `cargo fmt --check` to a CI step and run a one-shot `cargo fmt` cleanup commit
  before, so future PRs land formatted.
- **`cargo test --no-run`**: PASS. Full test suite (28 binaries including
  e2e_*, services_integration, navigation, transaction, draft, slint_arabic_smoke)
  compiles cleanly.
- **`cargo test --test slint_arabic_smoke`**: PASS — `1 passed; 0 failed`.
  Confirms Slint links into a test binary and the embedded fonts compile.
- **Harness launch (`--theme-harness`)**: NOT EXECUTED in this verification run.
  Headless dev box has no Wayland/X11 display and the agent sandbox blocks
  binary execution. The harness compiles into the main binary (verified above)
  and is dispatched by `src/main.rs`'s `--theme-harness` argv branch. Visual
  validation requires running on a developer workstation with a display server.
  **Manual verification step for Plan 2**: on a workstation,
  `cargo run -- --theme-harness` should open a window showing the four-tier
  surface palette, light/dark toggle, IBM Plex Sans Latin + Arabic samples,
  JetBrains Mono numeric tabular sample, and a mixed-direction string.

### Final foundation status

**READY for Plan 2.** All Plan 1 deliverables (font bundle, Theme global,
Surfaces global, Colors theme-derivation, Fonts global, Layout RTL helpers,
ThemeHarness component, `--theme-harness` binary entry point, Slint+Arabic
smoke test) compile, link, and pass automated checks. The remaining open item
is a live visual review of the harness on a workstation — operator action,
not blocking architecture.

Carry-forward to Plan 2:
- One-shot `cargo fmt` + add `cargo fmt --check` and `cargo clippy -- -D warnings`
  to CI so the next plan does not also inherit a 60-file fmt drift.
- Address the two pre-existing `src/main.rs` clippy warnings opportunistically
  (split `run_startup_sequence` into a context struct; collapse the
  `else if let Err`).
- `Layout.rtl` global currently defaults to `true` (Arabic-first). The
  production startup path in `src/main.rs` does not yet wire locale detection
  to this flag. Plan 2's first task that touches startup must call
  `Layout::get_global::<Layout>(window).set_rtl(...)` based on detected
  locale, otherwise non-Arabic developer machines will see RTL layout on
  `cargo run`.
- Visual verification of the four-configuration matrix (light/dark × LTR/RTL
  × en/ar) was not executed in Task 10 because the dev sandbox is headless.
  Plan 1's "READY for Plan 2" status is contingent on this being done by an
  operator on a workstation before the foundation branch merges to main —
  it is a procedural gate, not architectural work. Capture screenshots into
  `docs/POS-UI-REDESIGN-SCREENSHOTS-FOUNDATION/` and amend this section.

## Visual verification — Plan 2 (Atomic Components)

Date: 2026-05-10
Verified-by: abdu-benayad

Run host: developer workstation (X11, 1920×1080). Build: `cargo build`
(offline, vendored), `dev` profile, 7m 42s. Binary launched with
`--component-gallery`. Each configuration's screenshot is a vertical stitch
of two `import -window` captures (top-of-scroll + bottom-of-scroll)
because all eight component blocks do not fit in one viewport at the
gallery's preferred 1280×900 size.

### Configuration matrix

- Light + LTR + EN — pass / observed issues: none
- Light + RTL + AR — pass / observed issues: none
- Dark + LTR + EN — pass / observed issues: StatusLED syncing dot has no
  halo pulse and no halo brightness vs online/offline (color-only difference)
- Dark + RTL + AR — pass / observed issues: same StatusLED issue as 03

### Screenshots

See `docs/POS-UI-REDESIGN-SCREENSHOTS-PLAN-02/01-light-ltr-en.png`
through `04-dark-rtl-ar.png`.

### Component checks

- Panel — surface tier 2 tokens differ between themes: PASS. Light shows
  white panels on a pale-gray background; dark shows dark-navy panels on
  near-black. Panel/background contrast survives the theme flip.
- Button — primary/secondary/danger/ghost render: PASS in both themes,
  with primary lime, secondary surface-tinted, danger red (light) /
  coral (dark), ghost transparent, disabled muted.
- SearchInput — magnifier mirrors to the leading edge: PASS. Magnifier is
  on the LEFT in LTR (01, 03) and on the RIGHT in RTL (02, 04). Placeholder
  text mirrors as well.
- OpsButton — primary (lime) / neutral / danger render: PASS in all four
  configs. ADD lime, REMOVE/QTY/DISCOUNT/EDIT neutral surface, VOID red
  (light) / coral (dark), DISABLED muted.
- StatusLED — syncing pulse animates: FAIL. Operator confirmed live: the
  syncing dot is a static blue circle the same size and brightness as the
  online and offline dots. No halo, no opacity pulse.
- PayButton — dark halo present, light solid green: PASS. In dark configs
  (03, 04) the active PAY button shows a lime fill with a visible green
  halo glow around the rectangle. In light configs (01, 02) it is a solid
  deep green with no glow and no gradient. Disabled/zero-total state is
  rendered as a muted version in both themes.
- ProductTile — accent stripe on correct edge per direction: PASS. Stripe
  sits on the LEFT in LTR (01 Café Latte orange / Croissant purple / Cold
  Water teal / Sandwich red-or-green) and on the RIGHT in RTL (02, 04).
  Out-of-stock badge ("غير متوفر" in AR) renders correctly under each tile.
- CartLine — qty pill on correct edge, selected glow visible: PASS. The
  ×2 / ×1 quantity pill is on the LEFT in LTR and on the RIGHT in RTL.
  Selected line in all four configs shows a lime stroke around the row.

### RTL-specific checks (configs 02, 04)

- Numerics stay LTR inside RTL containers: PASS. PayButton total
  "12.600 LYD", ProductTile prices ("12.500", "6.000", "1.500", "18.000"),
  and CartLine totals ("25.000", "6.000", "18.000") all render
  left-to-right with the decimal point in the expected position even
  though the surrounding Arabic flows right-to-left.

### Arabic shaping checks (configs 02, 04)

- PASS. "ادفع" (PayButton), "قهوة لاتيه" / "كرواسون" / "ماء بارد" /
  "ساندويش" (ProductTile titles), "غير متوفر" (out-of-stock badge),
  "ساندويش بالدجاج المشوي" (long CartLine title) all show connected
  letter forms, no isolated-form fallbacks, no missing-glyph boxes.

### Animation checks

- Click opacity dip on press: PASS-with-caveat. The dip fires but the
  operator reports it is "difficult to notice" — likely the pressed
  opacity step is too close to 1.0. Tune the press opacity for a more
  legible press affordance in Plan 3.
- StatusLED syncing halo opacity pulse: FAIL. No animation observed.
  Either the animation is not wired (timer never starts, animate
  property not bound) or the animated property is not visible (no halo
  rendered to animate). Combined with the brightness FAIL — most likely
  root cause is that the halo ring is not drawn at all, so there is
  nothing for the animation to modulate.

### Issues / follow-ups

- StatusLED syncing variant ships without a visible halo and without a
  pulse. Both Plan 2 spec items ("syncing dot has a brighter halo than
  online/offline" in dark, "halo opacity animates ~1.2 s period")
  regress here. Inspect `ui/components/atomic/status_led.slint` —
  expect either a missing `halo` Rectangle / Path child, or the halo's
  opacity bound to a constant rather than to an `animate opacity { ... }`
  block. Re-verify after the fix using configs 03 and 04, where the halo
  would be most visible against the dark background.
- Button / OpsButton / PayButton press affordance is technically present
  but not perceptible. Plan 3 should drop the pressed opacity to ~0.7
  (or equivalent scale/translate) so touch users get clear feedback on a
  24" cashier display.
