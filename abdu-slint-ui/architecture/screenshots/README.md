# Pattern verification screenshots

Visual evidence for load-bearing claims in `../segment-pattern.md` and `../segment-pattern-consultation.md`. The underscore-prefixed filenames mirror the source files in `components/_*.slint`.

Captured on Slint 1.14.1 (the version pinned by `Cargo.lock`).

## Files

| File | What it shows | Doc reference |
|---|---|---|
| `_segment-preview.png` | Segment regression suite — 7 verification scenarios rendered side-by-side: empty-cell zero-width (invariant 5), padding-h variations, align-h with cell wider than content, elide on constrained width, wrap on constrained width, font-family variations (Latin / Arabic / icon), typography variations. | `segment-pattern.md` → Segment, Appendix B. |
| `_segment-column-preview.png` | SegmentColumn regression suite — 5 scenarios: two-line composition (canonical use case), empty-secondary-collapses, vstack-spacing variations, SegmentColumn as one cell in a horizontal row, show:bool collapses the column to zero. | `segment-pattern.md` → SegmentColumn. |
| `_segment-column-vstack-verification.png` | 4-case vertical-stacking test confirming the horizontal-stretch-mediated bug class doesn't apply to vertically-stacked Segments. The diagnostic case is row 3: primary elide=true + secondary elide=false at 200px width — primary elides normally, does NOT collapse to zero. | `segment-pattern-consultation.md` → Q4 (empirical result), `segment-pattern.md` → SegmentColumn (justification). |
| `_badge-empty-wart-and-show-fix.png` | Early ad-hoc verification (from `/tmp/badge-empty-test.slint`): Badge wrapping an empty Segment **without** `show:bool` gating renders a 16px gold chrome strip (the wart); Badge with `show:false` renders zero-width invisible (the fix). | `segment-pattern.md` → Badge, "The show: bool convention". Superseded as a regression artifact by `_badge-preview.png` (which integrates the same scenario into the production library preview); kept here as the original empirical evidence. |
| `_badge-preview.png` | Badge regression suite — 4 scenarios. Scenario 1 (load-bearing): three-pill comparison showing the wart (`show=true` + empty content = visible chrome strip) next to the fix (`show=false` = zero contribution) next to a normal chip. Plus chrome variations, Badge as one inelastic cell in a horizontal row with a separate slack Rectangle (Invariant 7), and Badge wrapping a SegmentColumn for two-line chips. | `segment-pattern.md` → Badge. |

## Re-running the verifications

The first two screenshots came from preview files inside the library:
- `_segment-preview.png` → `slint-viewer abdu-slint-ui/previews/_segment.slint`
- `_segment-column-vstack-verification.png` → the verification source is in `/tmp/vstack-elide-test.slint` (not yet ported into the library; will land as `previews/_segment-column.slint` when SegmentColumn ships).

The badge test was an ad-hoc script (`/tmp/badge-empty-test.slint`) that will be folded into `previews/_badge.slint` when Badge ships. Until then, the screenshot is the canonical artifact for the wart-vs-fix claim.
