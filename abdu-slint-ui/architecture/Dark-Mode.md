Dark mode implementation brief

  Goal: add light / dark mode support to Theme global, validated against every shipped component in the playground. Land before the KeyValueRow slice commits so KeyValueRow ships
  dark-mode-aware from day one (no retrofit later, and KeyValueRow's preview can include dark-mode rows).

  Why dark mode now, not Phase 2: Phase 1 is closing. Adding dark mode now means the smoke test (examples/settings-display.slint) and every Phase 2 component lands knowing the mode-switching
   pattern. Adding it later means refactoring every Theme call site, plus retrofitting every shipped preview.

  Why no component code changes: Slint's reactive bindings handle this. Components already read Theme.background, Theme.primary, Theme.surface, etc. We refactor Theme to make those tokens
  derived properties that select from a light or dark sub-palette based on Theme.mode. Setting Theme.mode = ThemeMode.dark flips every derived property simultaneously; every component
  re-renders. Zero changes in components/*.slint.

  Sequencing

  Pause KeyValueRow's implementation slice commit. The fresh session has implementation in flight (lib.slint already exports KeyValueRow, playground has the tile). Pause that commit. Land
  dark mode first, then resume KeyValueRow's preview + playground section to include dark-mode validation rows, then commit KeyValueRow as a single slice.

  Commit order:

  1. feat(abdu-slint-ui): dark mode support in Theme global — the refactor + playground toolbar combobox. No component code changes. Validation: every shipped component (Button, IconButton,
  Toggle, Card) renders correctly in both modes via the playground.
  2. feat(abdu-slint-ui): KeyValueRow component + preview + playground section — the deferred slice, now with dark-mode preview rows included.

  Step-by-step

  Step 1 — Add ThemeMode enum

  abdu-slint-ui/enums.slint — append:

  export enum ThemeMode {
      light,
      dark,
  }

  Re-export from lib.slint.

  Step 2 — Refactor globals/theme.slint

  Pattern: for each existing token, split into a light-* and dark-* value and turn the existing token into a derived property selecting between them.

  Mode property:

  in-out property <ThemeMode> mode: ThemeMode.light;

  Each existing token becomes three properties. Example for background:

  // Light palette
  out property <color> light-background: #ffffff;

  // Dark palette
  out property <color> dark-background: #000000;

  // Active (derived — what components read)
  out property <color> background: mode == ThemeMode.dark ? dark-background : light-background;

  Do this for every color token currently in Theme. The file roughly doubles. The public surface (token names like Theme.background, Theme.primary, etc.) does not change — components keep
  reading the same names.

  Step 3 — iOS dark mode palette values

  These match iOS's published system color values (Apple HIG / SF Symbols documentation). Use these verbatim:

  ┌────────────────────────┬─────────────────┬─────────────────┐
  │         Token          │ Light (current) │ Dark (proposed) │
  ├────────────────────────┼─────────────────┼─────────────────┤
  │ background             │ #ffffff         │ #000000         │
  ├────────────────────────┼─────────────────┼─────────────────┤
  │ foreground             │ #1c1c1e         │ #ffffff         │
  ├────────────────────────┼─────────────────┼─────────────────┤
  │ surface                │ #ffffff         │ #1c1c1e         │
  ├────────────────────────┼─────────────────┼─────────────────┤
  │ surface-muted          │ #f2f2f7         │ #2c2c2e         │
  ├────────────────────────┼─────────────────┼─────────────────┤
  │ primary                │ #007aff         │ #0a84ff         │
  ├────────────────────────┼─────────────────┼─────────────────┤
  │ primary-hover          │ #0066d6         │ #409cff                           │
  ├────────────────────────┼─────────────────┼───────────────────────────────────┤
  │ primary-foreground     │ #ffffff         │ #ffffff                                                            │
  ├────────────────────────┼─────────────────┼────────────────────────────────────────────────────────────────────┤
  │ destructive            │ #ff3b30         │ #ff453a                                                            │
  ├────────────────────────┼─────────────────┼────────────────────────────────────────────────────────────────────┤
  │ destructive-hover      │ #d70015         │ #ff6961                                                            │
  ├────────────────────────┼─────────────────┼────────────────────────────────────────────────────────────────────┤
  │ destructive-foreground │ #ffffff         │ #ffffff                                                            │
  ├────────────────────────┼─────────────────┼────────────────────────────────────────────────────────────────────┤
  │ secondary              │ #f2f2f7         │ #2c2c2e                                                            │
  ├────────────────────────┼─────────────────┼────────────────────────────────────────────────────────────────────┤
  │ secondary-hover        │ #e5e5ea         │ #3a3a3c                                                            │
  ├────────────────────────┼─────────────────┼────────────────────────────────────────────────────────────────────┤
  │ secondary-foreground   │ #1c1c1e         │ #ffffff                                                            │
  ├────────────────────────┼─────────────────┼────────────────────────────────────────────────────────────────────┤
  │ accent                 │ #f2f2f7         │ #2c2c2e                                                            │
  ├────────────────────────┼─────────────────┼────────────────────────────────────────────────────────────────────┤
  │ accent-foreground      │ #007aff         │ #0a84ff                                                            │
  ├────────────────────────┼─────────────────┼────────────────────────────────────────────────────────────────────┤
  │ success                │ #34c759         │ #30d158                                                            │
  ├────────────────────────┼─────────────────┼────────────────────────────────────────────────────────────────────┤
  │ success-foreground     │ #ffffff         │ #000000                                                            │
  ├────────────────────────┼─────────────────┼────────────────────────────────────────────────────────────────────┤
  │ warning                │ #ff9500         │ #ff9f0a                                                            │
  ├────────────────────────┼─────────────────┼────────────────────────────────────────────────────────────────────┤
  │ warning-foreground     │ #ffffff         │ #000000                                                            │
  ├────────────────────────┼─────────────────┼────────────────────────────────────────────────────────────────────┤
  │ info                   │ #5ac8fa         │ #64d2ff                                                            │
  ├────────────────────────┼─────────────────┼────────────────────────────────────────────────────────────────────┤
  │ info-foreground        │ #ffffff         │ #000000                                                            │
  ├────────────────────────┼─────────────────┼────────────────────────────────────────────────────────────────────┤
  │ border                 │ #c6c6c8         │ #38383a                                                            │
  ├────────────────────────┼─────────────────┼────────────────────────────────────────────────────────────────────┤
  │ ring                   │ #007aff         │ #0a84ff                                                            │
  ├────────────────────────┼─────────────────┼────────────────────────────────────────────────────────────────────┤
  │ muted-foreground       │ #8e8e93         │ #8e8e93 (intentionally unchanged)                                  │
  ├────────────────────────┼─────────────────┼────────────────────────────────────────────────────────────────────┤
  │ tooltip-background     │ #1c1c1e         │ #f2f2f7 (intentionally inverted — tooltip pops against background) │
  ├────────────────────────┼─────────────────┼────────────────────────────────────────────────────────────────────┤
  │ tooltip-foreground     │ #ffffff         │ #1c1c1e                                                            │
  └────────────────────────┴─────────────────┴────────────────────────────────────────────────────────────────────┘

  Notable decisions baked into the table:

  - Semantic foregrounds (success-foreground, warning-foreground, info-foreground) flip black-on-dark-mode. Bright iOS greens / yellows / cyans read better with a dark foreground in dark
  mode, where the saturated color is light enough to need dark text on it.
  - muted-foreground stays #8e8e93 in both modes. iOS systemGray works on both light and dark backgrounds.
  - Tooltip inverts. Dark-mode tooltips are light surfaces with dark text — the tooltip pops against the dark UI behind it, same way the light-mode dark tooltip pops against light UI.
  - Surface vs background. In dark mode, background is pure black (#000000, iOS systemBackground dark) and surface is the next-step-up gray (#1c1c1e, iOS secondarySystemBackground dark).
  Cards visually float above the background.

  Step 4 — Shadow palette in dark mode

  Dark-mode shadows are visually weak (black-on-black) but they still help define elevation in conjunction with borders. Increase opacity significantly:

  ┌─────────────────┬─────────────────┬─────────────────┐
  │      Token      │ Light (current) │ Dark (proposed) │
  ├─────────────────┼─────────────────┼─────────────────┤
  │ shadow-sm-color │ #0000001f (12%) │ #00000099 (60%) │
  ├─────────────────┼─────────────────┼─────────────────┤
  │ shadow-md-color │ #00000038 (22%) │ #000000b3 (70%) │
  ├─────────────────┼─────────────────┼─────────────────┤
  │ shadow-lg-color │ #00000047 (28%) │ #000000cc (80%) │
  ├─────────────────┼─────────────────┼─────────────────┤
  │ shadow-xl-color │ #00000059 (35%) │ #000000e6 (90%) │
  └─────────────────┴─────────────────┴─────────────────┘

  Blur and y-offset values stay the same in both modes — only opacity changes.

  Note: Card's bordered: true default becomes load-bearing in dark mode. Even with the boosted shadow opacity, cards in dark mode rely more on the border for separation than on the shadow.
  The default was already safety-first; dark mode just makes that safety more visible.

  Step 5 — Optional: theme transition animation

  Existing component-level animate background { duration: Animation.fast; ... } blocks make mode flips animate at 120ms — usable but feels rushed for a deliberate UI mode change. Two
  options:

  - A. Ship as-is. 120ms is snappy. Users barely notice the transition. Reduces scope.
  - B. Add Animation.theme-transition: 300ms to the Animation global. Don't change component-level animate blocks now; document the token as "use this duration on theme-driven palette
  changes if you want a more deliberate fade." Phase 1.5 can swap in the longer duration after testing.

  Recommend B — adds one token, zero behavior change, gives future-us a knob to turn.

  Step 6 — Playground toolbar combobox

  Add a "Theme" combobox to the playground's toolbar (in abdu-slint-ui-playground/ui/playground.slint), mirroring the pattern of the existing density / icon-family combobox:

  HorizontalBox {
      spacing: 6px;
      Text { text: "Mode:"; vertical-alignment: center; font-size: 12px; }
      ComboBox {
          model: ["light", "dark"];
          current-index: Theme.mode == ThemeMode.dark ? 1 : 0;
          selected(v) => {
              Theme.mode = v == "dark" ? ThemeMode.dark : ThemeMode.light;
          }
      }
  }

  Wire-up: add ThemeMode to the existing import { Theme, ... } from "@abdu-slint-ui" line at top of playground.slint.

  Position the combobox in the toolbar's HorizontalBox between existing controls — probably between "Density" and "Currency" since theme-mode and density are both visual-style controls.

  Step 7 — Visual validation matrix

  Run cargo run and visually validate each shipped component in both modes. Toggle the playground toolbar's "Mode" combobox; the entire window's color scheme should swap live (with the
  existing 120ms animations).

  Validation grid (every cell must look right):

  ┌────────────────────────────────────┬───────┬──────┬─────────────────────────────────────────────────────────────────────────────────────────────────────────┐
  │             Component              │ Light │ Dark │                                                  Notes                                                  │
  ├────────────────────────────────────┼───────┼──────┼─────────────────────────────────────────────────────────────────────────────────────────────────────────┤
  │ Button (default, all sizes)        │ ✓     │ ✓    │ Primary blue should stay readable.                                                                      │
  ├────────────────────────────────────┼───────┼──────┼─────────────────────────────────────────────────────────────────────────────────────────────────────────┤
  │ Button (outline)                   │ ✓     │ ✓    │ Border color comes from Theme.border — dark mode darker-gray.                                           │
  ├────────────────────────────────────┼───────┼──────┼─────────────────────────────────────────────────────────────────────────────────────────────────────────┤
  │ Button (ghost)                     │ ✓     │ ✓    │ Transparent background in both; foreground inverts.                                                     │
  ├────────────────────────────────────┼───────┼──────┼─────────────────────────────────────────────────────────────────────────────────────────────────────────┤
  │ Button (link)                      │ ✓     │ ✓    │ Underline color uses primary; both modes blue.                                                          │
  ├────────────────────────────────────┼───────┼──────┼─────────────────────────────────────────────────────────────────────────────────────────────────────────┤
  │ Button (destructive)               │ ✓     │ ✓    │ Red shifts slightly brighter in dark.                                                                   │
  ├────────────────────────────────────┼───────┼──────┼─────────────────────────────────────────────────────────────────────────────────────────────────────────┤
  │ IconButton (all variants)          │ ✓     │ ✓    │ Same checks as Button.                                                                                  │
  ├────────────────────────────────────┼───────┼──────┼─────────────────────────────────────────────────────────────────────────────────────────────────────────┤
  │ Toggle (off)                       │ ✓     │ ✓    │ Off-track uses Theme.border; dark mode darker.                                                          │
  ├────────────────────────────────────┼───────┼──────┼─────────────────────────────────────────────────────────────────────────────────────────────────────────┤
  │ Toggle (on)                        │ ✓     │ ✓    │ Success green shifts. Knob (white surface) stays white in both modes.                                   │
  ├────────────────────────────────────┼───────┼──────┼─────────────────────────────────────────────────────────────────────────────────────────────────────────┤
  │ Card (bordered, all elevations)    │ ✓     │ ✓    │ Border is critical for definition in dark mode.                                                         │
  ├────────────────────────────────────┼───────┼──────┼─────────────────────────────────────────────────────────────────────────────────────────────────────────┤
  │ Card (borderless + shadow)         │ ✓     │ ✓    │ Shadow opacity boost makes this still readable in dark.                                                 │
  ├────────────────────────────────────┼───────┼──────┼─────────────────────────────────────────────────────────────────────────────────────────────────────────┤
  │ Card (interactive — hover + press) │ ✓     │ ✓    │ Background tint darker-still on press. Confirm .darker(4%) doesn't go below surface contrast threshold. │
  ├────────────────────────────────────┼───────┼──────┼─────────────────────────────────────────────────────────────────────────────────────────────────────────┤
  │ Tooltips                           │ ✓     │ ✓    │ Should INVERT — light tooltip in dark mode, dark tooltip in light mode.                                 │
  ├────────────────────────────────────┼───────┼──────┼─────────────────────────────────────────────────────────────────────────────────────────────────────────┤
  │ Focus rings                        │ ✓     │ ✓    │ Theme.ring follows primary in both modes.                                                               │
  ├────────────────────────────────────┼───────┼──────┼─────────────────────────────────────────────────────────────────────────────────────────────────────────┤
  │ KeyValueRow (after this lands)     │ ✓     │ ✓    │ Will validate as part of KeyValueRow's slice.                                                           │
  └────────────────────────────────────┴───────┴──────┴─────────────────────────────────────────────────────────────────────────────────────────────────────────┘

  If any cell looks wrong, fix the relevant hex value in the dark palette, recompile, re-test. Document any cell that needed tuning.

  Step 8 — README documentation

  Add a "Dark mode" subsection to README, before "Component catalog":

  ▎ Dark mode
  ▎
  ▎ The library ships with parallel light and dark color palettes. Switch modes by setting Theme.mode:
  ▎
  ▎ let theme = MainWindow::get_theme(&window);
  ▎ theme.set_mode(ThemeMode::Dark);
  ▎
  ▎ Every component re-renders automatically. No per-component code changes needed.
  ▎
  ▎ System theme detection is the consumer's responsibility. Use a Rust crate like dark-light to detect the OS preference and call set_mode accordingly. The library has no platform-specific
  ▎ code.
  ▎
  ▎ Brand color customization is independent of mode. If you override Theme.primary, you must also override Theme.dark-primary (and the matching -hover variants for both modes) — the library
  ▎  doesn't auto-derive hover/pressed shades. Document your brand palette as a complete set of overrides.

  Step 9 — Commit message

  Use a single commit:

  feat(abdu-slint-ui): dark mode support in Theme global

  Adds ThemeMode { light, dark } enum and parallel dark color palette.
  Every public Theme token becomes a derived property selecting between
  light-* and dark-* sub-tokens based on Theme.mode. Zero changes to
  shipped component code — components already bind to public Theme
  tokens, and Slint's reactive property bindings handle the runtime
  switch automatically.

  Palette decisions follow iOS system colors (light + dark variants from
  Apple HIG / SF Symbols documentation). Notable choices baked into the
  mapping:

  - Semantic foregrounds (success/warning/info) flip black-on-dark-mode
    because saturated iOS colors are light enough to need dark text on
    them in dark mode.
  - muted-foreground (#8e8e93) is intentionally unchanged across modes —
    iOS systemGray works on both light and dark backgrounds.
  - Tooltip background/foreground invert (light tooltip in dark mode,
    dark tooltip in light) so the tooltip always pops against the
    surrounding UI.
  - Shadow opacity boosts significantly in dark mode (12-35% range
    becomes 60-90% range) because black-on-near-black shadows are
    invisible. Blur and y-offset stay the same; only alpha changes.
  - Card's bordered:true default becomes load-bearing in dark mode where
    shadows alone provide less separation. The default was already
    safety-first; dark mode validates the choice.

  Adds Animation.theme-transition: 300ms token for future use on mode
  flips that want a more deliberate fade. Current component-level
  animate blocks stay at Animation.fast (120ms) — usable for v1; the
  new token is available to swap in if visual review prefers slower
  transitions.

  Adds "Mode" combobox to the playground toolbar between density and
  currency controls. Toggling switches all four shipped components
  live; visual validation per the matrix in the commit description
  holds across Button, IconButton, Toggle, Card.

  README gains a "Dark mode" subsection documenting the consumer API
  (set Theme.mode from Rust), system theme detection responsibility,
  and the brand-customization convention (overriding Theme.primary
  requires also overriding dark-primary and the hover variants).

  What NOT to do

  - Don't add per-component theme-mode override properties. Per-section theming (dark sidebar + light content) is out of scope for v1. If the use case emerges, it gets its own design pass.
  - Don't add system-theme auto-detection inside the library. The consumer's Rust code calls set_mode based on whatever signals matter to them (OS preference, time of day, manual toggle).
  Library stays platform-agnostic.
  - Don't try to auto-derive hover/pressed from base colors. The hand-tuned primary-hover quality matters. Consumers customizing brand colors override the full set; the library doesn't
  compute .darker(8%) for them.
  - Don't refactor component code. If any component "needs changes" to support dark mode, that's a bug in the component — it's bypassing Theme.* somehow. Find and fix the call site (probably
   a hardcoded color literal), don't add a mode-aware branch in the component.

  Risks / non-trivial details

  1. Card's Theme.surface.darker(4%) on press. When Theme.surface is #000000 (dark mode), .darker(4%) produces a barely-different black. The press feedback may be invisible. Check this; if
  invisible, swap to Theme.surface.brighter(4%) in dark mode. The cleanest fix is computing this AT the component level, where it can branch on Theme.mode. Card's base-bg-resolved already
  takes hover/press as inputs; adding Theme.mode == ThemeMode.dark to the branching logic is local to Card and doesn't touch Theme.
  2. Theme.shadow-*-color consumers. The Depth global reads these. Depth doesn't need changes (it forwards whatever color it gets). But confirm that the boosted dark-mode opacities actually
  render visibly in the preview — black shadows on #1c1c1e cards on #000000 background are subtle even at 90% alpha.
  3. Window background: Theme.background in the playground. The playground window currently sets background: Theme.background — once dark mode lands, the whole window goes dark when mode
  flips. Check that the sidebar's hardcoded #f8fafc and section-routing fallback Rectangle (Theme.surface-muted) follow correctly. Sidebar is hardcoded so it'll stay light — this is
  intentional (the playground chrome stays consistent), document it.
  4. Cargo build warning count stays at 4 — the four "doesn't inherit Window" warnings (Button, IconButton, Toggle, Card). Dark mode doesn't change the warning surface.

  Validation gate

  The dark-mode commit is "done" when:

  - Every cell of the validation matrix above renders correctly in both modes
  - Toggling the playground "Mode" combobox feels responsive (the 120ms animations are snappy, not jarring)
  - cargo check clean (library) + cargo build clean (playground), four expected warnings only
  - README has the new "Dark mode" subsection
  - No component file under components/ was modified in this commit (zero behavior changes — the entire effect comes from Theme refactoring)

  After dark mode lands, resume KeyValueRow:

  - KeyValueRow's preview gains dark-mode validation rows (toggle the preview's Locale.rtl style — add a similar dark-mode-demo: bool property and conditionally instantiate sample rows in
  both modes, OR rely on the playground's mode combobox for validation and don't multiply preview cases)
  - KeyValueRow's playground section needs no changes — toolbar combobox flips the section live, just like every other section
  - KeyValueRow's commit message notes that dark mode shipped first and KeyValueRow was validated in both modes