# POS Terminal UI Redesign — Design Doc

**Status:** Design proposed, awaiting approval before IMPL doc
**Date:** 2026-05-09
**Owner:** Abdu
**Supersedes (in part):** `docs/DESIGN-SYSTEM.md`

---

## 1. Problem

The current POS terminal UI works but feels generic, busy, and "developer-built." Three specific failures:

- **Visual identity is anonymous** — uses default Slint patterns, system blues, mixed typography (Cairo + Tajawal + Inter Arabic fallbacks), no consistent personality.
- **Density is uncontrolled** — four kinds of stacked banners (price-check mode, last-scanned, voided-item, offline) layer on top of products; a five-button quick-action bar competes with the cart's Pay button; the category strip has its own overflow chevron. Nothing clearly dominates because everything is fighting for attention.
- **No defensible MENA position** — payment tiles say "Card / QR / Other"; mada, STC Pay, Apple Pay are not visible as brands. Foodics owns this trust signal locally. Generic tiles read as "imported software."

The redesign is a visual + interaction overhaul, not a reshuffle of features. Workflow logic stays largely intact; what changes is the surface.

---

## 2. Goals and non-goals

### Goals
- Establish a visual identity distinct from any existing MENA POS, with both light and dark themes shipping from day one.
- Cut on-screen density without removing capabilities — promote contextual surfaces over permanent chrome.
- Surface offline-first and bilingual capability as features, not as degraded modes.
- Replace ad-hoc cart-line buttons with a fixed-position operations column for muscle memory.
- Replace generic payment tender tiles with a config-driven, branded set keyed to the tenant's enabled methods.
- Make every state (empty, loading, success, offline, error) explicitly designed in the same language as the happy path.

### Non-goals (this round)
- Rewriting the cart, transaction, or payment service code paths.
- Redesigning admin / reports / settings deeply (only the chrome is updated, layouts remain).
- Adding new payment integrations.
- Replacing Slint as the rendering layer.
- Touching the printer/scanner/scale hardware abstraction.

---

## 3. Locked design decisions

These were validated in brainstorming. Everything downstream (the IMPL doc, the token rewrite, the per-screen work) follows from these:

1. **Identity:** "Operator" — sharp, Linear/Bloomberg-energy, mono numerics, single accent colour applied with restraint. Both light and dark variants share DNA.
2. **Layout:** four logical zones, left to right in LTR (mirrored in RTL):
   - **Categories rail** (icons only, ~52 dp wide)
   - **Products area** (search, action bar, product grid)
   - **Operations column** (~96 dp wide; six fixed buttons targeting the selected cart line)
   - **Cart panel** (~250 dp wide; line items, totals, single Pay action)
3. **Operations column (the new pattern):** `+1 / −1 / ×n / % / ✎ / ⌫`. Targets the selected cart line. Most-recently-added line is auto-selected so the common "scan + ×N" path stays one tap.
4. **Pay button:** single-row strip (label and total side by side), not a tall stacked block. Lit in dark (lime gradient + halo); deep green in light (#15803D, AAA-contrast).
5. **Payment tender tiles:** branded and config-driven (the "C" approach). Backend `payment_methods` config per tenant decides which tiles render; the POS only knows how to render branded tiles for known providers (mada, STC Pay, Apple Pay, Visa, Mastercard, cash, QR, Sadad, etc.).
6. **Receipt is mandatory.** Every successful transaction auto-prints. The success modal confirms `Receipt printed` with paper size and timestamp; it offers REPRINT and EMAIL COPY but never an opt-out from the original print. This is a fiscal/compliance rule (ZATCA in KSA, equivalent elsewhere).
7. **Glassy edge treatment** with restraint — specular highlights, gradient surfaces, lit Pay, glowing selected states. No backdrop blur on bulk content (only on header/footer/overlays/sheets) because in-store lighting kills it.
8. **Stability rule:** the rail icon order, the operations column order, and the Pay button position are sacred. They never move without explicit cashier-driven configuration. Muscle memory is a feature.

---

## 4. The design system (what becomes `theme.slint`)

### 4.1 Surfaces and depth

There are four surface tiers; everything renders into one of them:

| Tier | What lives here | Light | Dark |
|---|---|---|---|
| **Background** | Window bedrock | gradient `#FBFCFD → #EEF1F5` | gradient `#14171C → #0A0C10` |
| **Panel** | Rail / products area / ops column / cart | white-ish gradient + 1 px border + 8 dp elevation shadow | dark gradient + 1 px white-alpha border + 8 dp shadow |
| **Surface** | Tiles, buttons, cart lines | flatter gradient + 1 px border + 6 dp shadow | deeper gradient + same | 
| **Inset** | Search box, qty pills | recessed gradient + inner shadow + 1 px border | same, dark |

Every surface gets a top inset highlight (`box-shadow: 0 1px 0 rgba(255,255,255,0.95) inset` in light, lower opacity in dark). This is the "glassy edge" — done once globally, no further work per component.

### 4.2 Colour

There is exactly one accent — a controlled lime — and exactly one CTA colour — green. Everything else is neutral.

```
Lime accent (selected, active, +1):  #84CC16 (light), #A3E635 (dark)
Pay green (Pay button BG):           #15803D (light), lime gradient (dark)
Danger:                              #DC2626 (light), #F87171 (dark)
Warning (offline LED):               #D97706 (light), #FCD34D (dark)
Tile category accents (LEFT BORDERS, configurable per category):
  coffee #B45309, bakery #7C3AED, cold #0EA5E9, food #10B981
```

Per-category accent borders are the only place colour repeats. No saturated washes, no decorative gradients in colour.

### 4.3 Typography

**Single family across both scripts:** IBM Plex Sans + IBM Plex Sans Arabic. This replaces the current Cairo + Tajawal + Inter mix. IBM Plex pairs Arabic and Latin at matched x-heights out of the box and reads as "premium tech."

**Numerics:** JetBrains Mono everywhere. Cart amounts, totals, qty pills, clock, transaction IDs, paper width. The fact that numbers are mono is the strongest tell that this is the same product across every screen.

**Sizes:** display 32 / title 24 / heading 20 / body 16 / caption 14 / small 12 / tiny 11. Arabic gets +12% line-height at every size to clear diacritics — non-negotiable.

**Weights:** Regular 400, Medium 500, Semibold 600, Bold 700. Two weights per screen max.

### 4.4 Spacing

4 dp grid, no exceptions. Tokens: `xxs 2 / xs 4 / sm 8 / md 12 / lg 16 / xl 24 / xxl 32 / xxxl 48`. If something doesn't fit one of these, the layout is wrong.

### 4.5 Motion

Informational only. Tile press: 80 ms scale 0.97 → 1.0 ease-out. Selected-line glow: 150 ms fade-in. Modal in: 200 ms slide up + fade. Spinner: only on operations that genuinely cannot be made optimistic (e.g., card-terminal handshake). Never on cart updates.

### 4.6 Sound

Three-note family, all togglable but on by default:
- Scan / add (short high tick)
- Error (lower two-tone)
- Payment success (rising chord)

This is the existing audio strategy continued; no change to assets needed if current sounds are acceptable.

---

## 5. Layout: data flow in plain English

### 5.1 The main checkout screen

A request to start a sale arrives (cashier opens a shift, or finalizes the previous sale). The screen renders four zones from the rail outward:

1. The **categories rail** queries the catalog for the tenant's top-level categories and shows them as 36 dp icon buttons. The first icon is "All" / "⌗", followed by 4–8 categories, then a "⋯" overflow button that opens a sheet with the rest. At the bottom of the rail, two non-category icons — a back/exit chevron and a settings cog — are pinned.
2. The **products area** has a search row at top (search input plus PLU and Price-check buttons) and the product grid below. The grid is the largest single zone on screen — it gets four columns by default at 1024 px width, scales to five at 1280, six at 1600. Tiles all render at the same aspect ratio with a left-border colour matching the category accent.
3. The **operations column** is six buttons stacked vertically, always in the same order: `+1 / −1 / ×n / % / ✎ / ⌫`. The label "ON SELECTED" is small and centered above. The most-recently-added cart line is auto-selected; the cashier can tap any line to change selection. The `+1` button is the primary (lime-tinted) variant by default; `⌫` is the danger (red-tinted) variant. Less-frequent operations (HOLD, DRAFT, CUSTOMER, VOID ALL) live in the footer "MORE" menu, not the column.
4. The **cart panel** shows the line items (with their qty pill on the leading edge), a totals block (subtotal, tax breakdown), and the Pay button as a single horizontal strip at the bottom. The selected cart line glows lime — there is no other colour state on a cart line.

The header and footer are full-width above and below all four zones. The header carries the brand name, terminal ID, current operator, online/offline LED, and clock. The footer carries offline-queue status and the "MORE" overflow.

### 5.2 The payment screen

Tapping Pay swaps the centre area (products + ops + cart) for the payment selection. Header and footer stay. Layout: a total-banner across the top showing "Total to collect 12.600 LYD" with the amount in mono green; below, a grid of branded payment tiles drawn from the tenant's enabled methods. Tiles render their actual brand mark (mada, STC Pay logo, Apple Pay logotype, Visa wordmark, etc.). Cancel is a footer button on the trailing edge. Tapping a tile launches the appropriate flow (cash counter, card terminal, QR display).

### 5.3 The success modal

Payment confirmed → success modal overlays the payment screen (which blurs and dims). Modal contains: a haloed lime check, the headline "Payment received", the total in mono green, the payment-method line (`mada · ending 4221 · TXN #84219`), a receipt-status strip (`↦ Receipt printed · 80mm · 14:34:02`), and three actions: REPRINT, EMAIL COPY, NEW SALE (primary). Tapping NEW SALE returns to the empty cart state.

---

## 6. State design (the "feels premium" tell)

Every non-happy state is designed in the same language as the happy path. None of them is a default Slint dialog.

### 6.1 Empty cart
The cashier sees this 200+ times a day. A glowing lime "∅" mark inside an 80 dp soft tile, headline "Ready for a sale", calm sub-copy, and three keyboard-shortcut hint chips (`F1 PLU`, `F2 Hold`, `/ Search`). Renders inside the cart panel, full-height. Not a blank rectangle. Not a stock illustration.

### 6.2 Loading product grid (first-time hydration)
Skeleton tiles in the same shape and colour as real tiles, with a slow shimmer. Categories rail loads first, search bar is interactive immediately. Never a spinner over the whole screen.

### 6.3 Cart action loading
Optimistic by default. If the backend is doing something the user can't observe (sync, payment confirm), a thin top-progress bar in lime (the existing `progress_bar.slint` style updated). Never a spinner that blocks the cart.

### 6.4 Offline
- Header LED switches to amber.
- Persistent banner under the header: amber tile + headline "Working offline — sales continue" + meta line `3 transactions queued · last sync 14:18 · will retry every 30s` + manual RETRY NOW button.
- Cashier keeps selling. Transactions queue locally. This is treated as a feature, not a degraded state.

### 6.5 Payment success
See §5.3.

### 6.6 Card declined
Inline error strip above the payment grid, NOT a blocking dialog: red glyph + plain headline ("Card declined") + plain body with parenthetical bank code (`The bank refused this transaction (code 51 — insufficient funds). Try another card, switch to cash, or split the payment.`) + two next-action buttons (SPLIT, TRY AGAIN). Background payment grid stays visible. No raw API error strings, ever.

### 6.7 Other errors (sync failed, printer offline, scale unstable)
Same inline-strip pattern, colour and tone matched to severity (warning amber for sync failed, danger red for printer offline mid-sale). Specific copy lives in the IMPL doc.

---

## 7. RTL and Arabic — designed first, not bolted on

This was a question during brainstorming. The answer is yes, every decision above has an explicit RTL/Arabic specification. The summary:

### 7.1 Layout mirroring
The four-zone layout mirrors cleanly in RTL: cart panel becomes the leading (right) edge, products area sits centre, operations column is between products and cart in both directions, categories rail is the trailing (left in RTL) edge.

**Mechanism is unresolved at the design-doc level** because Slint 1.8 does not have CSS-style logical properties (`border-inline-start`, etc.) as a native concept. The IMPL doc must pick one of:
1. A small wrapper-component layer that exposes `border-leading` / `border-trailing` props and resolves them against `Layout.rtl` from `theme.slint`.
2. Per-component `if Layout.rtl { ... } else { ... }` branches inside every directional component.
3. A horizontal-flex container that swaps child order when `Layout.rtl` is true, plus per-tile mirroring of accent borders.

Option 1 is the only one that scales — the others repeat the same conditional in dozens of places. This is a hard call to make in the IMPL doc, not here, but flagging it now because if Slint cannot express any of the three cleanly, the RTL story collapses and the redesign needs a different runtime answer. **Verify Slint capability before committing to the IMPL plan.**

### 7.2 Numbers stay LTR
Every numeric — totals, qty pills, transaction IDs, clock, paper width, bank decline codes — renders left-to-right even inside Arabic UI. Unicode bidi handles this automatically as long as the runtime doesn't force a direction override on the digit string.

### 7.3 Eastern vs Western digits
Per-tenant config: default is Western (0–9) for parity with Foodics and younger consumers. Tenants can opt into Eastern Arabic digits (٠–٩) for traditional receipts; the same JetBrains Mono cell rules apply.

### 7.4 Currency placement
Per-tenant config: default is amount-then-symbol (`12.600 LYD`) for parity with the existing UI; tenants can opt into symbol-first (`LYD 12.600`) or Arabic abbreviation (`12.600 د.ل`).

### 7.5 Glyph mirroring
- Mirror in RTL: `→` arrows, `‹/›` chevrons, `↦` receipt arrow, `⌫` backspace.
- Do NOT mirror: payment-method logos (brand assets), category icons, status LED, the `∅` empty mark, math symbols (`+`, `−`, `×`, `%`, `✎`).

### 7.6 Typography
IBM Plex Sans Arabic for the Arabic cut, IBM Plex Sans for Latin and numerics. The two cuts pair at matched x-heights, so the same line of mixed-script text renders without visible jumps. Arabic line-height is +12% globally vs Latin.

### 7.7 Slint runtime requirement
HarfBuzz-shaped text rendering. This must be verified before shipping — Arabic positional shaping (initial / medial / final / isolated glyphs) is the #1 silent failure mode in non-MENA-built POS systems. If the current Slint build does not use HarfBuzz, that is a blocker for the redesign.

### 7.8 Bilingual receipt
Two-column thermal layout (AR primary right column RTL, EN secondary left column LTR), or stacked (AR primary above, EN smaller below). Tax line, ZATCA QR (KSA tenants), and totals must render correctly in both. Receipt template work is its own implementation step; the POS UI just confirms the print succeeded.

---

## 8. Payment tile system (the C approach)

The POS does not hard-code which tender tiles render. Instead:

1. The backend exposes a `tenant.payment_methods` array per tenant: an ordered list of method codes (`cash`, `mada`, `visa`, `mastercard`, `stc_pay`, `apple_pay`, `qr`, `sadad`, ...) plus per-method display config (label, sub-label, brand asset reference).
2. The POS pulls this on first sync and caches locally.
3. The payment screen renders one tile per enabled method, in the order the backend specifies.
4. The POS knows how to render branded tiles for a fixed catalog of providers — the brand asset, the colour treatment in light vs dark, the standard label. New providers require both a backend entry AND a POS asset bundle update.

**Open for IMPL doc:** the exact backend schema, asset packaging (bundled vs lazy-loaded), and the fallback when an unknown method code arrives (recommend: render as generic "Other" tile with the method code as label, and log a warning).

**Out of scope for this redesign:** adding new payment integrations. The redesign accepts whatever the backend enables. The market-scope decision (Libya only vs MENA-wide default tile set) belongs to the backend seed-data work, not this UI doc.

---

## 9. Responsive behavior

Two layouts:

- **Counter (≥ 1024 px wide):** four-zone layout described in §5.1.
- **Compact (< 1024 px):** stacks vertically — products area on top, cart at the bottom, operations column collapses into the cart's top edge as a horizontal strip, categories rail collapses into a horizontal scroll above the search input. The Pay button stays sticky at the bottom of the cart.

The compact mode is for small POS hardware (10" tablets) and emergency fallback. It is not the primary target.

The header and footer chrome compress vertically (header 38 → 32 dp, footer 22 → 18 dp).

---

## 10. What this doc deliberately does not specify

The IMPL doc will resolve:

- The exact Slint component file layout (which existing components stay, which get rewritten, which split).
- The migration from `theme.slint` → new token system (likely additive: new tokens added, old tokens deprecated, screens migrated incrementally).
- The HarfBuzz verification step.
- The asset bundle format for payment-method brand marks.
- The shimmer-skeleton loading components (new).
- The exact copy strings for every error state (in three locales).
- The keyboard shortcut map.
- The unit + screenshot test strategy for theme switching and RTL.
- The exact rollout plan (feature-flagged screen-by-screen vs all-at-once).

---

## 11. Verification plan (at the design level)

Before this design is considered shipped:

1. **Side-by-side with current UI:** the same transaction (3 items, mixed cart, mada payment, receipt printed) demoed on both UIs in light, dark, LTR, and RTL. Eight screenshots total. The redesign should be unambiguously better on all eight.
2. **In-store lighting check:** the dark theme rendered on the actual target hardware under the actual fluorescent / window-light conditions of a real customer site. The glassy edges must survive — if they do not, fall back to the flatter dark variant (kept on file).
3. **Cashier dry-run:** one trained cashier completes 20 transactions on the redesign without prior briefing; we measure tap count and time-per-sale against the current UI baseline. Target: ≤ same time, fewer taps on the hot path.
4. **Bilingual proofread:** an Arabic-native speaker reviews every Arabic string for tone, glyph correctness, and typography. Specifically look for any English bleed-through inside Arabic strings (the most common error).
5. **WCAG AA on light, AA-contrast on dark:** automated check on text-on-surface ratios for every tier × every state.

---

## 12. Open questions for the user

These belong with you, not the IMPL doc:

1. **Market scope for default payment tiles.** Libya only, MENA-wide, KSA-first? This decides the seed data for `tenant.payment_methods`.
2. **Brand colour overlay.** The redesign uses a controlled lime/green palette. Does WadiDMS have an existing brand colour that needs to coexist (e.g., a specific blue used elsewhere in the platform)? If yes, we need to decide whether to fold it in or keep the POS visually distinct from the web platform.
3. **Audio assets.** Keep the existing scan/error/success sounds, or commission a new three-note family that matches the visual identity?
4. **Customer-facing display.** Does any tenant deploy a second screen facing the customer? If yes, that's a separate UI surface that needs its own (much simpler) treatment.
