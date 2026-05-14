# Segment-as-Cell Pattern — Consultation

> **Companion** to `segment-pattern.md`. Time-bounded. Contains the questions we want a reviewer to weigh in on before we commit to the pattern for new row primitives beyond `KeyValueRow`. This file's lifecycle ends when consultation concludes; after that it's archived or its findings folded back into the canonical doc.

---

## Read first

This document presumes the reader has read [`segment-pattern.md`](./segment-pattern.md). That doc states the pattern as canonical reference — the rules, the seven invariants, the primitive family, the verified composition mechanism, the deferred decorators.

This doc asks: **is the pattern right? Are there sharp edges we haven't surfaced? What should we adjust before adopting it as the foundation for future row primitives?**

---

## Decision request

We propose adopting the segment-as-cell pattern as the structural foundation for all direction-aware row primitives in `abdu-slint-ui`, starting with the rewrite of `KeyValueRow`. The pattern supersedes the cluster architecture that the previous `KeyValueRow` implementation used.

The alternative is to repair the cluster pattern (remove `overflow: elide` from the value Text, restore an explicit slack Rectangle, audit every conditional predicate for the elide-collapse bug class), which would fix the immediate regression but leave the structural fragility in place for future row primitives.

We're asking for review before committing.

---

## Specific questions for the reviewer

**1. Is segment-as-cell the right structural pattern for direction-aware row primitives in Slint 1.14?**

Or is there a Slint-idiomatic approach we're missing? Specifically:
- Is there a way to make `HorizontalLayout` reorder children at runtime that we don't know about?
- Is there a Slint feature for "set FlowDirection at the root, cascade to layouts" that we missed?
- Does Slint 1.14 expose a child-introspection mechanism (parent reads `@children`'s preferred-width / min-width) that would let decorators auto-zero on empty content without the `show: bool` convention?

If any of these exist, the pattern in `segment-pattern.md` is over-engineered and we should simplify.

**2. Is the `show: bool` convention the right answer to the empty-decorator problem?**

Verified empirically: a Badge wrapping an empty Segment renders as a `2 * padding-h` chrome strip. The `show: bool` convention closes this by gating preferred-width / min-width / visible on a call-site-provided predicate. The cost: visibility coordination lives at the call site; the row author threads the predicate through both the decorator's `show` and the wrapped Segment's content.

Alternative considered: have the decorator auto-detect empty content via `layout.preferred-width <= 2 * padding-h`. This works for single-Segment Badges but breaks if a Badge wraps multiple cells where some are empty and others aren't. We rejected it on generality grounds. Is there a better option?

**3. Should `Segment` be allowed to grow beyond typography-and-intra-cell-alignment?**

The pattern draws a category line: Segment may accept new properties only if they belong to the typography-and-intra-cell-alignment surface (e.g., `letter-spacing`, `line-height`). Visual chrome, padding-v, stretch, interactivity, multi-line vertical content — all explicitly forbidden, all routed to composition primitives instead.

Specifically: the modification exercise (adding `background-color` to Segment) showed three options for the "what happens when text='' and background-color is set" question, none of which is clean. The right answer was to introduce `Badge` instead. But the category line is harder to enforce than a count-based "no more than 10 properties" rule. Is there a sharper way to state it, or is the category itself the right abstraction?

**4. Vertical stacking — adopted as `SegmentColumn`, justified by multiple downstream consumers.**

A `label` with a `description` underneath is two pieces of content with their own typography. The pattern's atomic cell (`Segment`) is single-Text by design; vertical stacking inside a cell is solved by introducing a second cell primitive — `SegmentColumn` — that wraps N Segments via `@children` in a `VerticalLayout`. The column is one cell from the row's HorizontalLayout perspective; from its own perspective, it's a vertical stack.

We initially considered deferring vertical stacking to a future `ListTile` primitive and keeping `KeyValueRow` single-line (the YAGNI option). Counting actual downstream consumers reversed that decision:

- **KeyValueRow.description** — settings row with primary label + secondary text below
- **RadioGroup item** — option name + description (e.g., payment method + card details)
- **CheckboxGroup item** — setting name + explanation (e.g., feature toggle + what it does)
- **DataTable cell** — primary value + supporting detail (timestamp, delta, sub-label)
- **ListTile** — Material's `text` + `supporting_text` pattern

Five consumers exceeds the rule-of-three threshold for extracting a shared abstraction. SegmentColumn earns its keep.

**Empirical result:** verified on Slint 1.14 with a 4-case test (`/tmp/vstack-elide-test.slint`). A column containing two Segments stacked vertically, with the parent forced to 200px (well below the natural width of either child), exhibits the following behavior:

- Both cells with `elide: true` → both elide normally with `…` at their respective trailing edges.
- Primary `elide: true`, secondary `elide: false` (the asymmetric case that triggered the cluster bug horizontally) → primary elides normally, secondary clips at the cell boundary. **The primary does NOT collapse to zero.** The horizontal-stretch-mediated bug class is genuinely horizontal-specific; vertical siblings don't compete for shared width because each row receives the full parent width independently.
- Symmetric reverse (primary no-elide, secondary elide) → same correct behavior.

The bug class is structural to horizontal width pressure, not vertical layouts. SegmentColumn inherits the cell-isolation guarantees of the wider pattern.

**Design choice — `@children` slot vs parameterized prefixed properties.** A parameterized SegmentColumn with `primary-text`, `primary-font-family`, ..., `secondary-text`, `secondary-font-family`, ... would expose ~20 properties (each Segment property doubled) and lock the column to exactly two children. The `@children` slot mechanism keeps the column thin (one real property, `vstack-spacing`), reuses Segment's typography contract for every stacked child, and generalizes to N ≥ 2 lines (DataTable cells may want value + delta + timestamp). Composition over parameterization, same principle as Badge.

**Question for the reviewer:** is anything wrong with this approach? Specifically:

- Does Slint 1.14 reliably handle `@children` inside a VerticalLayout for `preferred-width = max` and `preferred-height = sum` calculation? Card and Badge use `@children` inside other layouts and work; we expect the same to hold here, but haven't tested SegmentColumn under width pressure in production-sized lists yet.
- Is the choice to make SegmentColumn always-present in the row (with the secondary Segment self-zeroing when description is empty) preferable to gating the row's label cell with a 4-way branch (`if !Locale.rtl && !has-description`, `if !Locale.rtl && has-description`, etc.)? The always-present approach keeps row branching to 2 (LTR/RTL only) at the cost of one extra Rectangle per row even when there's no description. The trade-off seems clearly in favor of structural simplicity, but a reviewer with Slint perf intuition may disagree.

**5. Grid-like alignment across rows.**

Settings sections where the value column is aligned across all rows (so `42.00`, `6.30`, `2.50`, `50.80` decimal-align) require a parent grid context that's row-external. The pattern as described is row-local — each row sizes its cells independently, so adjacent rows can have differently-positioned value columns even with identical content shapes.

Is this a missed opportunity? Should the pattern address column alignment via a parent layout convention (`KeyValueGroup`?) that constrains its child rows to share column widths? Or is column alignment a separate Phase 2 concern (likely a `DataTable` primitive) that has nothing to do with this pattern?

---

## Risks-still-worth-flagging

These aren't blocking, but a reviewer should know they exist and may want to weigh in.

### Slint version portability

The pattern depends on three Slint 1.14 behaviors documented in `segment-pattern.md` § "Slint version portability": explicit intrinsic-size propagation through nested Rectangles, Rectangle's `horizontal-stretch: 1` default, `@children` slot composition inside HorizontalLayout. Future Slint versions may change any of these. The pattern's tests would need to re-run on a Slint upgrade.

### Per-frame cost of 16+ children in one HorizontalLayout

A populated KeyValueRow renders ~9 cells. With the LTR/RTL duplication, the row's HorizontalLayout has up to 18 declared children (9 per branch, half gated off by `if`). Slint compiles to native code; per-frame layout cost should be linear in *rendered* children. The concern is more about compile-time IR than runtime, but a settings list of ~50 rows on a low-end POS terminal is worth measuring before declaring the pattern performant.

We haven't measured. A reviewer with deeper Slint performance knowledge may know whether this is a non-issue or worth attention.

### Source-level duplication

Each cell is declared twice (LTR branch + RTL branch). For a 9-cell row, that's ~18 declarations. Per-branch property bindings reference row-level derived properties, so each declaration is short (~6-8 lines) and reading the row source top-to-bottom remains legible. But the LOC count of a complete `KeyValueRow` source under this pattern will be visibly larger than the cluster pattern's source was.

Is this acceptable? Or is there a Slint mechanism we haven't considered (macros, template generation in build.rs, code-generation) that could collapse the duplication without losing the per-cell-independence guarantee?

### Tooltip and Pressable design

Named as members of the decorator family but their specific bindings are deferred (`segment-pattern.md` § "Decorator status" lists the hard sub-problems concretely). The composition mechanism is verified for Badge; both Tooltip and Pressable inherit that mechanism's shape but have additional sub-problems specific to their interaction surfaces.

We're confident the composition pattern is sound for them. We're not yet confident about the specific bindings (PopupWindow positioning, TouchArea event ordering, hover-state scope). A reviewer with experience shipping Slint tooltips/clickable cells inside complex layouts may have practical guidance we should fold in before designing those primitives.

---

## What success looks like for this review

If the reviewer concludes:

- **Pattern is sound, adopt as-is**: we commit to the pattern, archive this consultation file, and proceed to writing the implementation plan for the `KeyValueRow` rewrite.
- **Pattern is sound but needs adjustment** (e.g., the category line for Segment's surface needs a sharper rule, the `show: bool` convention can be replaced with something better, vertical stacking has a cleaner answer): we fold the adjustments into the canonical doc and then proceed.
- **Pattern has a deeper structural problem we haven't surfaced**: we revisit before writing any code. Better to discover this now than after the rewrite.

We're open to all three outcomes.
