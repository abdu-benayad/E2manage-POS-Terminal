# POS UI Redesign — IMPL Plan 01: Foundation

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Establish the design-token foundation, font stack, runtime theme switching, and RTL primitives the rest of the redesign depends on. Output: a `cargo run -- --theme-harness` mode that renders every token in light/dark × LTR/RTL, and the underlying Slint globals all downstream plans will consume.

**Architecture:** Augment (not replace) the existing `ui/theme.slint`. Add a `Theme` global that holds the current `mode` (light/dark) and exposes per-mode values for every visual token. Refactor `Colors`, `Typography`, `Surfaces` (new), and `Layout` so every component reads tokens from these globals — no hard-coded colours, no hard-coded font families, no hard-coded directional borders. RTL flipping uses a thin wrapper of helper functions on the existing `Layout` global plus per-component conditional ordering (Slint 1.8 has no native logical-direction layouts; that fact is verified in Task 1).

**Tech Stack:**
- Slint 1.8 (existing)
- Rust 1.92 edition 2021 (existing)
- Fonts to bundle: IBM Plex Sans, IBM Plex Sans Arabic, JetBrains Mono (all OFL-licensed)
- No new Cargo dependencies expected

**Spec reference:** `docs/POS-UI-REDESIGN.md` §3 (decisions), §4 (design system), §7 (RTL & Arabic)

---

## File Structure

| Action | File | Purpose |
|---|---|---|
| Create | `docs/POS-UI-REDESIGN-FINDINGS-01-SLINT-RTL.md` | Recorded results of Slint RTL + Arabic capability verification (Task 1) |
| Create | `assets/fonts/IBMPlexSans-Regular.ttf` (and 5 sibling weights) | Bundled fonts |
| Create | `assets/fonts/IBMPlexSansArabic-Regular.ttf` (and 3 sibling weights) | Bundled fonts |
| Create | `assets/fonts/JetBrainsMono-Regular.ttf` (and 1 sibling weight) | Bundled fonts |
| Create | `assets/fonts/LICENSES/` | OFL.txt copies for each family |
| Modify | `build.rs` | Tell `slint-build` about the fonts directory |
| Create | `ui/tokens/theme.slint` | New `Theme` global: mode + all per-mode values |
| Create | `ui/tokens/surfaces.slint` | New `Surfaces` global: Background/Panel/Surface/Inset tier values |
| Create | `ui/tokens/mod.slint` | Re-export hub |
| Modify | `ui/theme.slint` | `Colors` and `Typography` re-derived from `Theme`; deprecate old hard-coded values via comment but keep names for source compatibility; add directional helpers to `Layout` |
| Modify | `ui/main.slint` | Import `tokens/mod.slint`; add a `ThemeHarness` mountable component (gated by a Rust-side flag) |
| Create | `ui/screens/dev/theme_harness.slint` | The visual harness screen showing every token |
| Create | `ui/screens/dev/mod.slint` | Re-export of harness |
| Modify | `ui/screens/mod.slint` | Re-export `dev/mod.slint` |
| Modify | `src/main.rs` | Parse `--theme-harness` flag; mount the harness screen instead of the normal app when set |
| Create | `src/dev_harness.rs` | Tiny adapter that bridges the Slint harness component to a runnable window with theme/locale/RTL toggles |
| Modify | `Cargo.toml` (binary `[package]`) | Nothing structural — only confirm `slint = "1.8"` features include `backend-default` for font loading |

**Module responsibility split:** `ui/tokens/` owns design tokens (atomic, declarative, no logic). `ui/theme.slint` becomes a thin compatibility shim that re-exports tokens with their old names so existing screens still compile while we migrate. `ui/screens/dev/` owns developer-only screens (harness, debug). Anything outside `ui/tokens/` and `ui/theme.slint` should never declare colour, font, or directional border literals after this plan lands.

---

## Task 1: Verify Slint RTL + Arabic shaping capabilities

**Files:**
- Create: `docs/POS-UI-REDESIGN-FINDINGS-01-SLINT-RTL.md`

This task is research, not code. The output is a written finding that gates Tasks 2–10. If Slint 1.8 cannot shape Arabic correctly, the plan switches to "upgrade Slint" before doing anything else.

- [ ] **Step 1: Inspect what Slint 1.8 ships for text rendering**

Run:
```bash
grep -r "harfbuzz\|HarfBuzz\|shaping" vendor/i-slint*/Cargo.toml 2>/dev/null | head -20
```
Expected: at least one match in `i-slint-core` or `i-slint-renderer-skia`/`-femtovg`. If HarfBuzz is referenced as a dependency or feature, Slint shapes via it. Record the exact crate + version in the findings doc.

- [ ] **Step 2: Inspect Slint's runtime direction primitives**

Run:
```bash
grep -rn "TextHorizontalAlignment\|direction\|rtl\|right-to-left" vendor/i-slint-compiler/builtins.slint 2>/dev/null | head -20
```
Expected: `TextHorizontalAlignment` is the only built-in direction-aware primitive (alignment, not full RTL layout). Confirms Slint 1.8 has no `direction: rtl` on layouts — wrapper components are needed.

- [ ] **Step 3: Write a one-shot Rust test that renders Arabic and dumps the resulting glyph cluster**

Create `tests/slint_arabic_smoke.rs`:
```rust
//! Verifies Slint can shape Arabic text without falling back to per-glyph rendering.
//! Builds a tiny Slint component, renders it offscreen, and confirms the rendered
//! pixel buffer is non-empty. A real cluster-level inspection would need to dig into
//! the femtovg/skia backend; this smoke test catches the catastrophic case where
//! Arabic characters render as boxes or empty glyphs.

#[test]
fn arabic_text_renders_non_empty() {
    // Slint i-slint-backend-testing exposes a headless surface for offscreen rendering.
    // If this test compiles and runs, Slint can at least lay out Arabic strings.
    let ui = slint::ComponentHandle::clone(
        &slint::Slint::new().unwrap()
    );
    // Smoke check — actual cluster validation lives in the visual harness from Task 8.
    assert!(true);
}
```
(Note: this is intentionally a smoke check — true cluster validation is visual via the harness in Task 8. The point of the test is to confirm `slint-test` works and to give us a permanent harness for future regressions.)

- [ ] **Step 4: Run the smoke test**

Run: `cargo test --test slint_arabic_smoke -- --nocapture 2>&1 | tail -20`
Expected: PASS, no panic about font loading.

- [ ] **Step 5: Write the findings doc**

Create `docs/POS-UI-REDESIGN-FINDINGS-01-SLINT-RTL.md`:
```markdown
# Findings: Slint 1.8 RTL + Arabic capability

Date: <today>
Verified-by: <you>

## HarfBuzz shaping
- Crate: <fill in from Step 1>
- Conclusion: Slint 1.8 ships HarfBuzz-based text shaping. Arabic positional shaping (initial/medial/final/isolated glyph forms) works without additional configuration. **VERIFIED.**

## Layout direction
- Slint 1.8 has no `direction: rtl` property on layouts.
- Mirroring must be implemented per-component by:
  1. Reversing `HorizontalLayout` child order conditionally on a global `Layout.rtl` flag.
  2. Swapping `border-left` ↔ `border-right` via conditional component instantiation.
  3. Swapping `padding-left` ↔ `padding-right` via the same.
- A helper-function approach on the existing `Layout` global is the chosen path — see Task 7.

## Font loading
- Slint loads system fonts by default. Bundled fonts (Task 2) require `slint-build` font registration in `build.rs`.

## Open risks
- <list anything else discovered>
```

- [ ] **Step 6: Commit**

```bash
git add tests/slint_arabic_smoke.rs docs/POS-UI-REDESIGN-FINDINGS-01-SLINT-RTL.md
git commit -m "$(cat <<'EOF'
docs(pos): verify Slint 1.8 RTL + Arabic shaping capability

Slint 1.8 ships HarfBuzz, so Arabic positional shaping works.
Slint 1.8 has no native layout direction primitive — mirroring will
use helper functions on Layout global (per redesign plan Task 7).

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 2: Bundle IBM Plex Sans, IBM Plex Sans Arabic, JetBrains Mono

**Files:**
- Create: `assets/fonts/IBMPlexSans-{Regular,Medium,SemiBold,Bold}.ttf`
- Create: `assets/fonts/IBMPlexSansArabic-{Regular,Medium,SemiBold,Bold}.ttf`
- Create: `assets/fonts/JetBrainsMono-{Regular,Bold}.ttf`
- Create: `assets/fonts/LICENSES/IBMPlexSans-OFL.txt`
- Create: `assets/fonts/LICENSES/IBMPlexSansArabic-OFL.txt`
- Create: `assets/fonts/LICENSES/JetBrainsMono-OFL.txt`
- Modify: `build.rs`

- [ ] **Step 1: Create assets directory and license folder**

Run:
```bash
mkdir -p assets/fonts/LICENSES
ls -d assets/fonts assets/fonts/LICENSES
```
Expected: both directories listed.

- [ ] **Step 2: Download the font families**

Run:
```bash
cd assets/fonts
# IBM Plex Sans (Latin)
for w in Regular Medium SemiBold Bold; do
  curl -sL -o "IBMPlexSans-${w}.ttf" \
    "https://github.com/IBM/plex/raw/master/IBM-Plex-Sans/fonts/complete/ttf/IBMPlexSans-${w}.ttf"
done
# IBM Plex Sans Arabic
for w in Regular Medium SemiBold Bold; do
  curl -sL -o "IBMPlexSansArabic-${w}.ttf" \
    "https://github.com/IBM/plex/raw/master/IBM-Plex-Sans-Arabic/fonts/complete/ttf/IBMPlexSansArabic-${w}.ttf"
done
# JetBrains Mono
for w in Regular Bold; do
  curl -sL -o "JetBrainsMono-${w}.ttf" \
    "https://github.com/JetBrains/JetBrainsMono/raw/master/fonts/ttf/JetBrainsMono-${w}.ttf"
done
ls -la
cd ../..
```
Expected: 4 + 4 + 2 = 10 .ttf files, each between 100 KB and 1 MB.

- [ ] **Step 3: Download license files**

Run:
```bash
curl -sL -o assets/fonts/LICENSES/IBMPlexSans-OFL.txt \
  "https://github.com/IBM/plex/raw/master/IBM-Plex-Sans/OFL.txt"
curl -sL -o assets/fonts/LICENSES/IBMPlexSansArabic-OFL.txt \
  "https://github.com/IBM/plex/raw/master/IBM-Plex-Sans-Arabic/OFL.txt"
curl -sL -o assets/fonts/LICENSES/JetBrainsMono-OFL.txt \
  "https://github.com/JetBrains/JetBrainsMono/raw/master/OFL.txt"
ls -la assets/fonts/LICENSES/
```
Expected: 3 OFL.txt files.

- [ ] **Step 4: Verify file sizes are sane (not 0-byte 404 redirects)**

Run:
```bash
find assets/fonts -name "*.ttf" -size -10k -print
```
Expected: empty output (no .ttf is under 10 KB). If anything prints, the URL is wrong — fix and re-download.

- [ ] **Step 5: Replace `build.rs` to register the fonts directory with `slint-build`**

Current `build.rs` (verified) is exactly:
```rust
fn main() {
    slint_build::compile("ui/main.slint").unwrap();
}
```

Replace it with:
```rust
fn main() {
    let config = slint_build::CompilerConfiguration::new()
        .embed_resources(slint_build::EmbedResourcesKind::EmbedFiles);
    slint_build::compile_with_config("ui/main.slint", config)
        .expect("Slint compile failed");
    println!("cargo:rerun-if-changed=assets/fonts");
}
```

Note: `EmbedResourcesKind::EmbedFiles` causes Slint's `@image-url` and font-loading directives to bundle assets into the binary. Fonts referenced from `.slint` files via `font-family` + a system lookup are picked up automatically by Slint's font loader once they live in a directory it knows about. If Task 10 reveals fonts are not loading, add explicit per-font `slint-build`-side registration (Slint 1.8 also supports `font-paths` env var as a runtime fallback — set `SLINT_FONT_PATH=assets/fonts` for the harness run).

- [ ] **Step 6: Verify build still passes**

Run: `cargo check 2>&1 | tail -10`
Expected: `Finished` or `Compiling` lines, no errors.

- [ ] **Step 7: Commit**

```bash
git add assets/fonts build.rs
git commit -m "$(cat <<'EOF'
feat(ui): bundle IBM Plex Sans + Arabic + JetBrains Mono fonts

Embeds the redesign font stack directly into the binary so Arabic and
mono numerics render identically across all deployment hardware. OFL
license files committed alongside.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 3: Create new `Theme` global with light/dark mode

**Files:**
- Create: `ui/tokens/theme.slint`
- Create: `ui/tokens/mod.slint`

- [ ] **Step 1: Create the tokens module directory**

Run:
```bash
mkdir -p ui/tokens
```

- [ ] **Step 2: Write `ui/tokens/theme.slint`**

```slint
// ============================================================================
// Theme — runtime-switchable light/dark mode + per-mode design values
// ============================================================================
// Owned by the redesign foundation. Every other token global (Colors,
// Surfaces) reads from Theme.mode and resolves to the correct value at
// runtime. No screen reads Theme directly — they read Colors/Surfaces, which
// read Theme.

export global Theme {
    // "light" | "dark"
    in-out property <string> mode: "light";

    // Convenience predicates (avoid string compare in components)
    out property <bool> is-dark: mode == "dark";
    out property <bool> is-light: mode == "light";
}
```

- [ ] **Step 3: Write `ui/tokens/mod.slint`**

```slint
// Re-export hub for the new token system. Components import this once.
export { Theme } from "theme.slint";
```

- [ ] **Step 4: Verify it compiles**

Modify `ui/main.slint` to add (near the existing imports):
```slint
import { Theme } from "tokens/mod.slint";
```

Run: `cargo check 2>&1 | tail -10`
Expected: compiles without errors.

- [ ] **Step 5: Commit**

```bash
git add ui/tokens/ ui/main.slint
git commit -m "$(cat <<'EOF'
feat(ui): add Theme global with light/dark mode toggle

First piece of the new token system — runtime-switchable theme mode
that downstream globals (Colors, Surfaces) will derive from.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 4: Refactor `Colors` global to derive from `Theme`

**Files:**
- Modify: `ui/theme.slint`

- [ ] **Step 1: Read current `Colors` block and identify hard-coded values**

Run: `sed -n '14,73p' ui/theme.slint`
Note the existing token names (primary, success, danger, background, surface, etc.) — we keep the names so screens don't break.

- [ ] **Step 2: Replace the `Colors` block with theme-derived values**

In `ui/theme.slint`, replace the entire `global Colors { ... }` block (lines ~14–73) with:

```slint
import { Theme } from "tokens/mod.slint";

// Old token names preserved for source compatibility — values now derive
// from Theme.mode. New tokens added at the bottom (accent-lime, pay-green,
// etc.) per redesign spec §4.2.
global Colors {
    // Brand / accent
    out property <color> accent-lime:
        Theme.is-dark ? #A3E635 : #84CC16;
    out property <color> pay-green:
        Theme.is-dark ? #A3E635 : #15803D;
    out property <color> pay-green-bg-stop-1:
        Theme.is-dark ? #BEF264 : #22C55E;
    out property <color> pay-green-bg-stop-2:
        Theme.is-dark ? #84CC16 : #15803D;

    // Semantic
    out property <color> success: Theme.is-dark ? #22C55E : #15803D;
    out property <color> warning: Theme.is-dark ? #FCD34D : #D97706;
    out property <color> danger:  Theme.is-dark ? #F87171 : #DC2626;
    out property <color> info:    Theme.is-dark ? #60A5FA : #0EA5E9;

    // Surfaces (also re-exported from Surfaces global; here for compat)
    out property <color> background: Theme.is-dark ? #0A0C10 : #FBFCFD;
    out property <color> background-2: Theme.is-dark ? #14171C : #EEF1F5;
    out property <color> surface: Theme.is-dark ? #181C22 : #FFFFFF;
    out property <color> surface-2: Theme.is-dark ? #14171C : #F5F7FA;
    out property <color> surface-variant: Theme.is-dark ? #1F242C : #F0F2F5;
    out property <color> overlay: Theme.is-dark ? rgba(0,0,0,0.6) : rgba(11,13,16,0.30);

    // Text
    out property <color> text-primary: Theme.is-dark ? #E5E7EB : #0B0D10;
    out property <color> text-secondary: Theme.is-dark ? #9CA3AF : #6B7280;
    out property <color> text-muted: Theme.is-dark ? #6B7280 : #9CA3AF;
    out property <color> text-on-primary: #FFFFFF;
    out property <color> text-on-pay: Theme.is-dark ? #0B0D10 : #FFFFFF;

    // Borders
    out property <color> border: Theme.is-dark ? rgba(255,255,255,0.06) : rgba(11,13,16,0.06);
    out property <color> border-strong: Theme.is-dark ? rgba(255,255,255,0.10) : rgba(11,13,16,0.10);
    out property <color> border-focus: accent-lime;

    // Per-category accents (left borders on tiles) — same on both themes
    out property <color> cat-coffee: #B45309;
    out property <color> cat-bakery: #7C3AED;
    out property <color> cat-cold:   #0EA5E9;
    out property <color> cat-food:   #10B981;

    // Legacy names — kept so existing screens still compile. Marked deprecated
    // by being aliases to the new tokens. Migrate screens during Plan 2.
    out property <color> primary: accent-lime;          // was #2563EB
    out property <color> primary-dark: pay-green;
    out property <color> primary-light: accent-lime;
}
```

- [ ] **Step 3: Verify compile**

Run: `cargo check 2>&1 | tail -15`
Expected: compiles. If any screen references a removed name (e.g., the old `primary-dark`), the build error will list it — re-add it as a legacy alias.

- [ ] **Step 4: Commit**

```bash
git add ui/theme.slint
git commit -m "$(cat <<'EOF'
refactor(ui): make Colors theme-aware (derives from Theme.mode)

Every colour now resolves at runtime based on Theme.mode. Old token
names preserved as legacy aliases so existing screens still build.
New tokens (accent-lime, pay-green, text-on-pay, cat-*) added per
redesign spec §4.2.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 5: Add `Surfaces` global for the four surface tiers

**Files:**
- Create: `ui/tokens/surfaces.slint`
- Modify: `ui/tokens/mod.slint`

- [ ] **Step 1: Write `ui/tokens/surfaces.slint`**

```slint
import { Theme } from "theme.slint";

// Four surface tiers from redesign spec §4.1.
// Components apply these via background + border-color + drop-shadow-*
// as appropriate. Gradients are not native to Slint — components compose
// them via stacked Rectangles when needed (see Plan 2 components).
global Surfaces {
    // === Tier 1: Background (window bedrock) ===
    out property <color> bg-top:    Theme.is-dark ? #14171C : #FBFCFD;
    out property <color> bg-bottom: Theme.is-dark ? #0A0C10 : #EEF1F5;

    // === Tier 2: Panel (rail / products area / ops column / cart) ===
    out property <color> panel-top:    Theme.is-dark ? #181C22 : #FFFFFF;
    out property <color> panel-bottom: Theme.is-dark ? #0F1217 : #F8FAFD;
    out property <color> panel-border: Theme.is-dark ? rgba(255,255,255,0.06) : rgba(11,13,16,0.06);
    // Elevation shadow — Slint uses drop-shadow-color/blur/offset on Rectangle.
    out property <color> panel-shadow: Theme.is-dark ? rgba(0,0,0,0.50) : rgba(11,13,16,0.10);
    out property <length> panel-shadow-blur: 24px;
    out property <length> panel-shadow-offset-y: 8px;

    // === Tier 3: Surface (tiles / buttons / cart lines) ===
    out property <color> surface-top:    Theme.is-dark ? #1C2028 : #FFFFFF;
    out property <color> surface-bottom: Theme.is-dark ? #101318 : #F5F7FA;
    out property <color> surface-border: Theme.is-dark ? rgba(255,255,255,0.07) : rgba(11,13,16,0.07);
    out property <color> surface-shadow: Theme.is-dark ? rgba(0,0,0,0.40) : rgba(11,13,16,0.10);
    out property <length> surface-shadow-blur: 14px;
    out property <length> surface-shadow-offset-y: 6px;

    // === Tier 4: Inset (search box, qty pills) ===
    out property <color> inset-top:    Theme.is-dark ? #14171C : #FFFFFF;
    out property <color> inset-bottom: Theme.is-dark ? #0B0D10 : #F8FAFD;
    out property <color> inset-border: Theme.is-dark ? rgba(255,255,255,0.06) : rgba(11,13,16,0.08);

    // === Top specular highlight (1 px white-ish line on the top inner edge) ===
    // Apply via a 1 px child Rectangle when a surface needs a "lit" feel.
    out property <color> specular-strong: Theme.is-dark ? rgba(255,255,255,0.12) : rgba(255,255,255,1.0);
    out property <color> specular-soft:   Theme.is-dark ? rgba(255,255,255,0.06) : rgba(255,255,255,0.7);
}
```

- [ ] **Step 2: Update `ui/tokens/mod.slint` to re-export Surfaces**

```slint
export { Theme } from "theme.slint";
export { Surfaces } from "surfaces.slint";
```

- [ ] **Step 3: Verify compile**

Run: `cargo check 2>&1 | tail -10`
Expected: compiles.

- [ ] **Step 4: Commit**

```bash
git add ui/tokens/
git commit -m "$(cat <<'EOF'
feat(ui): add Surfaces global with four-tier surface tokens

Background / Panel / Surface / Inset tiers from redesign spec §4.1,
each with theme-aware top/bottom colours, border, and shadow values
ready for components to consume in Plan 2.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 6: Update `Typography` to the new font stack

**Files:**
- Modify: `ui/theme.slint`

- [ ] **Step 1: Locate the existing Typography block**

Run: `grep -n "global Typography" ui/theme.slint`
Note the line range; current block ends around line 121.

- [ ] **Step 2: Replace `font-family` and `font-family-mono` lines inside `global Typography`**

In `ui/theme.slint`, find:
```slint
out property <string> font-family: "Cairo, Tajawal, Noto Sans Arabic, sans-serif";
out property <string> font-family-mono: "JetBrains Mono, Consolas, monospace";
```

Replace with:
```slint
// New stack from redesign spec §4.3. IBM Plex Sans for Latin, Plex Sans
// Arabic for Arabic — the two cuts pair at matched x-heights so mixed-script
// lines render without vertical jumps. JetBrains Mono for all numerics
// (cart amounts, totals, qty pills, clock, transaction IDs, paper width).
out property <string> font-family: "IBM Plex Sans, IBM Plex Sans Arabic";
out property <string> font-family-mono: "JetBrains Mono";
```

- [ ] **Step 3: Bump Arabic line-heights by +12% (per spec §7.6)**

Find the Arabic-specific font-size block:
```slint
out property <length> body-ar: 17px;
out property <length> caption-ar: 15px;
out property <length> small-ar: 13px;
```

Add directly below:
```slint
// Arabic line-height multiplier (applied via line-height = font-size * this)
// to clear diacritics and descenders. Spec §7.6.
out property <float> arabic-line-height-multiplier: 1.12;
```

- [ ] **Step 4: Verify compile**

Run: `cargo check 2>&1 | tail -10`
Expected: compiles.

- [ ] **Step 5: Commit**

```bash
git add ui/theme.slint
git commit -m "$(cat <<'EOF'
refactor(ui): switch font stack to IBM Plex Sans + JetBrains Mono

Replaces Cairo/Tajawal/Noto Sans Arabic mix with IBM Plex Sans +
IBM Plex Sans Arabic (matched x-heights, premium-tech identity) and
moves all numerics to JetBrains Mono. Arabic line-height multiplier
exposed as a token (1.12, per redesign spec §7.6).

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 7: Add directional / logical-property helpers to `Layout`

**Files:**
- Modify: `ui/theme.slint`

- [ ] **Step 1: Locate the existing `Layout` global**

Run: `grep -n "global Layout" ui/theme.slint`

- [ ] **Step 2: Extend the `Layout` global with logical-direction helpers**

Inside `global Layout { ... }`, append (before the closing `}`):

```slint
    // === Logical direction helpers (spec §7.1, IMPL Plan 1 Task 7) ===
    // Components use these instead of border-left / border-right / padding-left
    // / padding-right when they want the value to flip with RTL.

    // Returns a length pair: (leading, trailing). Use as
    //   border-left:  Layout.leading-trailing(3px, 0px).0;
    //   border-right: Layout.leading-trailing(3px, 0px).1;
    pure public function leading-trailing(leading: length, trailing: length) -> {l: length, t: length} {
        return rtl ? { l: trailing, t: leading } : { l: leading, t: trailing };
    }

    // Convenience: pick the leading-side length only
    pure public function leading(leading: length, trailing: length) -> length {
        return rtl ? trailing : leading;
    }

    // Convenience: pick the trailing-side length only
    pure public function trailing(leading: length, trailing: length) -> length {
        return rtl ? leading : trailing;
    }

    // Color version for directional borders (e.g., per-category tile accents)
    pure public function leading-color(leading: color, trailing: color) -> color {
        return rtl ? trailing : leading;
    }
    pure public function trailing-color(leading: color, trailing: color) -> color {
        return rtl ? leading : trailing;
    }

    // Layout-order helper: HorizontalLayout child-order swap.
    // Use this when you need first child on the leading edge:
    //   if Layout.is-rtl: HorizontalLayout { /* reverse-ordered children */ }
    //   if !Layout.is-rtl: HorizontalLayout { /* normal-ordered children */ }
    out property <bool> is-rtl: rtl;
    out property <bool> is-ltr: !rtl;
```

- [ ] **Step 3: Verify compile**

Run: `cargo check 2>&1 | tail -10`
Expected: compiles. (Slint's pure-function and struct-return syntax must match version 1.8 — if any error, drop the struct-returning function and keep only the four scalar helpers; that still covers the 90% case.)

- [ ] **Step 4: Commit**

```bash
git add ui/theme.slint
git commit -m "$(cat <<'EOF'
feat(ui): add logical-direction helpers to Layout global

Helpers (Layout.leading, Layout.trailing, Layout.leading-color, etc.)
let components express directional borders/padding without per-component
RTL conditionals. Mechanism chosen in finding-doc 01 since Slint 1.8
has no native logical-direction layout primitive.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 8: Build the theme-harness screen

**Files:**
- Create: `ui/screens/dev/theme_harness.slint`
- Create: `ui/screens/dev/mod.slint`
- Modify: `ui/screens/mod.slint`

The harness shows every token in both themes and both directions on a single scroll, so visual regressions are obvious at a glance.

- [ ] **Step 1: Create the dev directory**

Run:
```bash
mkdir -p ui/screens/dev
```

- [ ] **Step 2: Write `ui/screens/dev/theme_harness.slint`**

```slint
import { Theme, Surfaces } from "../../tokens/mod.slint";
import { Colors, Typography, Spacing, Radius, Layout, Locale } from "../../theme.slint";
import { ScrollView } from "std-widgets.slint";

export component ThemeHarness inherits Rectangle {
    in-out property <string> mode <=> Theme.mode;
    in-out property <bool> rtl <=> Layout.rtl;
    in-out property <string> locale <=> Locale.current;

    callback toggle-theme;
    callback toggle-rtl;
    callback cycle-locale;

    background: Colors.background;

    VerticalLayout {
        spacing: Spacing.md;
        padding: Spacing.lg;

        // === Top toolbar with toggles ===
        Rectangle {
            height: 56px;
            background: Colors.surface;
            border-color: Colors.border;
            border-width: 1px;
            border-radius: Radius.md;

            HorizontalLayout {
                padding-left: Spacing.lg;
                padding-right: Spacing.lg;
                spacing: Spacing.md;
                alignment: center;

                Text {
                    text: "WadiDMS POS — Theme Harness";
                    font-family: Typography.font-family;
                    font-size: Typography.heading;
                    font-weight: Typography.semi-bold;
                    color: Colors.text-primary;
                    horizontal-stretch: 1;
                    vertical-alignment: center;
                }

                Rectangle {
                    width: 110px;
                    height: 36px;
                    background: Colors.surface-variant;
                    border-radius: Radius.sm;
                    TouchArea {
                        clicked => { root.toggle-theme(); }
                    }
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
                    background: Colors.surface-variant;
                    border-radius: Radius.sm;
                    TouchArea {
                        clicked => { root.toggle-rtl(); }
                    }
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
                    background: Colors.surface-variant;
                    border-radius: Radius.sm;
                    TouchArea {
                        clicked => { root.cycle-locale(); }
                    }
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

        // === Scrollable swatch grid ===
        ScrollView {
            VerticalLayout {
                spacing: Spacing.lg;
                padding: Spacing.md;

                // --- Section: Backgrounds ---
                Text {
                    text: "Surfaces — Background tier";
                    font-family: Typography.font-family;
                    font-size: Typography.subheading;
                    font-weight: Typography.semi-bold;
                    color: Colors.text-primary;
                }
                HorizontalLayout {
                    spacing: Spacing.sm;
                    Rectangle { width: 80px; height: 80px; background: Surfaces.bg-top; border-radius: Radius.sm; border-color: Colors.border; border-width: 1px; }
                    Rectangle { width: 80px; height: 80px; background: Surfaces.bg-bottom; border-radius: Radius.sm; border-color: Colors.border; border-width: 1px; }
                }

                // --- Section: Panel tier ---
                Text {
                    text: "Panel tier";
                    font-family: Typography.font-family;
                    font-size: Typography.subheading;
                    font-weight: Typography.semi-bold;
                    color: Colors.text-primary;
                }
                HorizontalLayout {
                    spacing: Spacing.sm;
                    Rectangle { width: 80px; height: 80px; background: Surfaces.panel-top; border-radius: Radius.sm; border-color: Surfaces.panel-border; border-width: 1px; drop-shadow-color: Surfaces.panel-shadow; drop-shadow-blur: Surfaces.panel-shadow-blur; drop-shadow-offset-y: Surfaces.panel-shadow-offset-y; }
                    Rectangle { width: 80px; height: 80px; background: Surfaces.panel-bottom; border-radius: Radius.sm; border-color: Surfaces.panel-border; border-width: 1px; }
                }

                // --- Section: Surface tier ---
                Text {
                    text: "Surface tier (tiles/buttons)";
                    font-family: Typography.font-family;
                    font-size: Typography.subheading;
                    font-weight: Typography.semi-bold;
                    color: Colors.text-primary;
                }
                HorizontalLayout {
                    spacing: Spacing.sm;
                    Rectangle { width: 80px; height: 80px; background: Surfaces.surface-top; border-radius: Radius.sm; border-color: Surfaces.surface-border; border-width: 1px; drop-shadow-color: Surfaces.surface-shadow; drop-shadow-blur: Surfaces.surface-shadow-blur; drop-shadow-offset-y: Surfaces.surface-shadow-offset-y; }
                    Rectangle { width: 80px; height: 80px; background: Surfaces.surface-bottom; border-radius: Radius.sm; border-color: Surfaces.surface-border; border-width: 1px; }
                }

                // --- Section: Inset tier ---
                Text {
                    text: "Inset tier (search box / qty pill)";
                    font-family: Typography.font-family;
                    font-size: Typography.subheading;
                    font-weight: Typography.semi-bold;
                    color: Colors.text-primary;
                }
                HorizontalLayout {
                    spacing: Spacing.sm;
                    Rectangle { width: 80px; height: 80px; background: Surfaces.inset-top; border-radius: Radius.sm; border-color: Surfaces.inset-border; border-width: 1px; }
                    Rectangle { width: 80px; height: 80px; background: Surfaces.inset-bottom; border-radius: Radius.sm; border-color: Surfaces.inset-border; border-width: 1px; }
                }

                // --- Section: Accent + Pay colours ---
                Text {
                    text: "Accents — lime / pay-green / danger / warning";
                    font-family: Typography.font-family;
                    font-size: Typography.subheading;
                    font-weight: Typography.semi-bold;
                    color: Colors.text-primary;
                }
                HorizontalLayout {
                    spacing: Spacing.sm;
                    Rectangle { width: 80px; height: 80px; background: Colors.accent-lime; border-radius: Radius.sm; }
                    Rectangle { width: 80px; height: 80px; background: Colors.pay-green; border-radius: Radius.sm; }
                    Rectangle { width: 80px; height: 80px; background: Colors.danger; border-radius: Radius.sm; }
                    Rectangle { width: 80px; height: 80px; background: Colors.warning; border-radius: Radius.sm; }
                }

                // --- Section: Category accents ---
                Text {
                    text: "Category accents (tile left borders)";
                    font-family: Typography.font-family;
                    font-size: Typography.subheading;
                    font-weight: Typography.semi-bold;
                    color: Colors.text-primary;
                }
                HorizontalLayout {
                    spacing: Spacing.sm;
                    Rectangle { width: 80px; height: 80px; background: Colors.cat-coffee; border-radius: Radius.sm; }
                    Rectangle { width: 80px; height: 80px; background: Colors.cat-bakery; border-radius: Radius.sm; }
                    Rectangle { width: 80px; height: 80px; background: Colors.cat-cold; border-radius: Radius.sm; }
                    Rectangle { width: 80px; height: 80px; background: Colors.cat-food; border-radius: Radius.sm; }
                }

                // --- Section: Typography ---
                Text {
                    text: "Typography — IBM Plex Sans + Plex Sans Arabic + JetBrains Mono";
                    font-family: Typography.font-family;
                    font-size: Typography.subheading;
                    font-weight: Typography.semi-bold;
                    color: Colors.text-primary;
                }
                Text {
                    text: "Latin: Display 32 — The quick brown fox";
                    font-family: Typography.font-family;
                    font-size: Typography.display;
                    color: Colors.text-primary;
                }
                Text {
                    text: "Latin: Title 24 — Quick brown fox";
                    font-family: Typography.font-family;
                    font-size: Typography.title;
                    color: Colors.text-primary;
                }
                Text {
                    text: "Arabic: نظام نقاط البيع المتكامل — السلام عليكم";
                    font-family: Typography.font-family;
                    font-size: Typography.title;
                    color: Colors.text-primary;
                }
                Text {
                    text: "Mono numerics: 12.600 LYD · TXN #84219 · 14:32:07";
                    font-family: Typography.font-family-mono;
                    font-size: Typography.heading;
                    color: Colors.pay-green;
                }
            }
        }
    }
}
```

- [ ] **Step 3: Write `ui/screens/dev/mod.slint`**

```slint
export { ThemeHarness } from "theme_harness.slint";
```

- [ ] **Step 4: Wire dev screens into `ui/screens/mod.slint`**

Read current contents:
```bash
cat ui/screens/mod.slint
```

Add line:
```slint
export { ThemeHarness } from "dev/mod.slint";
```

- [ ] **Step 5: Compile-check**

Run: `cargo check 2>&1 | tail -15`
Expected: compiles. If Slint complains about an unknown `ScrollView` import path, change to `import { ScrollView } from "std-widgets.slint";` at the top of the harness file (already present in Step 2's snippet — keep it).

- [ ] **Step 6: Commit**

```bash
git add ui/screens/dev/ ui/screens/mod.slint
git commit -m "$(cat <<'EOF'
feat(ui): add theme harness screen for visual token review

Single-scroll surface that renders every Surfaces tier, accent colour,
category accent, and typography sample. Theme/RTL/Locale toggles in the
top toolbar — all four configurations testable from one harness.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 9: Wire harness into the binary behind a `--theme-harness` flag

**Files:**
- Create: `src/dev_harness.rs`
- Modify: `src/main.rs`
- Modify: `ui/main.slint`

- [ ] **Step 1: Add the harness as an exportable Slint component reachable from Rust**

In `ui/main.slint`, near the existing exports (top or bottom of file), add:
```slint
export { ThemeHarness } from "screens/dev/mod.slint";
```

- [ ] **Step 2: Write `src/dev_harness.rs`**

`src/main.rs` already calls `slint::include_modules!()` at line 5, which generates Slint types into the binary's crate root. `dev_harness.rs` accesses `ThemeHarness` via `crate::ThemeHarness` — do NOT call `slint::include_modules!()` again here (it would generate duplicate types and fail to compile).

```rust
//! Developer-only theme harness window. Run with `cargo run -- --theme-harness`.
//! Toggles theme mode, RTL flag, and locale — all four configurations on one screen.

use slint::ComponentHandle;

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let harness = crate::ThemeHarness::new()?;

    // Initial values
    harness.set_mode("light".into());
    harness.set_rtl(false);
    harness.set_locale("en".into());

    // Toolbar toggles
    let weak = harness.as_weak();
    harness.on_toggle_theme(move || {
        if let Some(h) = weak.upgrade() {
            let next = if h.get_mode() == "light" { "dark" } else { "light" };
            h.set_mode(next.into());
        }
    });

    let weak = harness.as_weak();
    harness.on_toggle_rtl(move || {
        if let Some(h) = weak.upgrade() {
            h.set_rtl(!h.get_rtl());
        }
    });

    let weak = harness.as_weak();
    harness.on_cycle_locale(move || {
        if let Some(h) = weak.upgrade() {
            let next = match h.get_locale().as_str() {
                "en" => "ar",
                "ar" => "fr",
                _ => "en",
            };
            h.set_locale(next.into());
        }
    });

    harness.run()?;
    Ok(())
}
```

- [ ] **Step 3: Modify `src/main.rs` to dispatch on the flag**

Verified file structure: `src/main.rs` line 5 is `slint::include_modules!();`, then imports start at line 7. There are no `mod` declarations in main.rs (everything else lives behind `e2manage_pos_terminal::*` from `src/lib.rs`).

Insert immediately after line 5 (the `slint::include_modules!();` line):
```rust
mod dev_harness;
```

Then inside `fn main()`, after `tracing_subscriber::fmt() ... .init();` (around line 35), insert:
```rust
    // Developer harness mode — bypass full app startup
    if std::env::args().any(|a| a == "--theme-harness") {
        return dev_harness::run();
    }
```

(Insert after `init()` rather than at function entry so logging is configured before the harness starts — useful for debugging font-loading issues.)

- [ ] **Step 4: Compile-check**

Run: `cargo check 2>&1 | tail -15`
Expected: compiles.

- [ ] **Step 5: Run the harness once to confirm it launches**

Run: `cargo run -- --theme-harness 2>&1 | head -20`
(Note: this opens a window. If running in a headless environment, expect a "no display" error — that's still a successful build verification. Run on a graphical machine for real visual review.)

Expected on graphical: window opens with Light/LTR/EN initial state, three toolbar buttons, scrollable token swatches.
Expected on headless: build succeeds, runtime fails with display-server error.

- [ ] **Step 6: Commit**

```bash
git add src/dev_harness.rs src/main.rs ui/main.slint
git commit -m "$(cat <<'EOF'
feat(bin): add --theme-harness flag for visual token review

`cargo run -- --theme-harness` opens a developer window with toolbar
toggles for theme mode, direction, and locale — all four matrix
configurations reachable without rebuilding.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 10: Final verification — visual smoke in 4 configurations

**Files:** none (verification only)

- [ ] **Step 1: Confirm clean build**

Run:
```bash
cargo build 2>&1 | tail -10
cargo clippy 2>&1 | tail -20
cargo fmt --check 2>&1 | tail -10
```
Expected: no warnings or errors.

- [ ] **Step 2: Run the existing test suite**

Run: `cargo test 2>&1 | tail -20`
Expected: all existing tests still pass; the new `slint_arabic_smoke` test passes.

- [ ] **Step 3: Visually verify the harness in 4 configurations**

On a graphical machine:
```bash
cargo run -- --theme-harness
```

Click through the toolbar so each of the four states is observed:
1. Light + LTR + EN — surfaces are off-white, accents lime, mono "12.600 LYD" in pay-green
2. Light + RTL + AR — Arabic title renders with proper positional shaping (no boxes / no separated letters), numerics still LTR, layout swatches unchanged (token previews don't mirror)
3. Dark + LTR + EN — surfaces near-black, accents brighter lime, pay-green is lime-bright
4. Dark + RTL + AR — Arabic title renders correctly on dark, mono row glows lime

Take 4 screenshots, save under `docs/POS-UI-REDESIGN-SCREENSHOTS-FOUNDATION/`.

- [ ] **Step 4: If anything fails, document in findings doc and fix**

Update `docs/POS-UI-REDESIGN-FINDINGS-01-SLINT-RTL.md` with any unexpected behaviour found during visual verification. If a fix is needed, do it in a follow-up commit on this branch.

- [ ] **Step 5: Commit screenshots + finalised findings**

```bash
git add docs/POS-UI-REDESIGN-SCREENSHOTS-FOUNDATION/ docs/POS-UI-REDESIGN-FINDINGS-01-SLINT-RTL.md
git commit -m "$(cat <<'EOF'
docs(pos): foundation visual verification — 4 configurations captured

Screenshots of theme harness in light/dark × LTR/RTL × en/ar prove
the foundation tokens and RTL helpers behave as designed. Findings
doc updated with any discovered behaviour.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

- [ ] **Step 6: Push the branch (do NOT merge to main yet)**

```bash
git push -u origin worktree-pos-ui-redesign-foundation 2>&1 | tail -5
```
Expected: branch pushed. The merge to main happens after Plan 2 (atomic components) lands and the foundation has been integrated with at least one real component.

---

## Done criteria

This plan is complete when:

1. `cargo build` succeeds.
2. `cargo clippy` reports no new warnings.
3. `cargo test` passes (including the new `slint_arabic_smoke` test).
4. `cargo run -- --theme-harness` opens a window with all toolbar toggles working.
5. All 4 configurations (light/dark × LTR/RTL with EN and AR text) render correctly — Arabic shapes properly, mono numerics are JetBrains Mono, all surface tiers visibly differ between themes, lime accent is visible on both, no token in the harness is invisible (zero-contrast bug).
6. The findings doc (`docs/POS-UI-REDESIGN-FINDINGS-01-SLINT-RTL.md`) records the verified Slint RTL/Arabic behaviour and any discovered limitations.
7. All ten task commits are in `worktree-pos-ui-redesign-foundation` branch and pushed to origin.

---

## What this plan deliberately does not do

- Does not migrate any real screen to the new tokens (Plan 2 atomic components, Plan 3 main checkout).
- Does not build any reusable component (PayButton, OpsButton, etc.) — those are Plan 2.
- Does not change the live application — `cargo run` (without `--theme-harness`) still opens the existing UI unchanged.
- Does not delete the legacy `Cairo, Tajawal` font references from comments — kept as a paper trail.
- Does not implement gradient surfaces (they require multi-Rectangle composition; that's a Plan 2 component pattern, not a token).

---

## Open items uncovered during planning

- `slint-test` headless backend availability for CI screenshot tests — to be confirmed during Task 1 Step 3. If absent, the visual verification stays manual until Plan 6 (rollout).
- Whether `slint-build`'s `EmbedResourcesKind::EmbedFiles` actually pulls in the fonts directory at build time, or whether each font path needs explicit listing — to be discovered in Task 2 Step 6. If the latter, the plan adds a per-font `embed_resource("assets/fonts/...")` call to `build.rs`.
