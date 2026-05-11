// abdu-slint-ui — Slint UI component library.
// All public components, globals, and enums are exported from lib.slint
// and surfaced to Rust consumers via the include_modules! macro below.
//
// The bundled Phosphor and Lucide icon fonts are embedded at compile time
// via `import "assets/*.ttf"` statements in lib.slint — no runtime
// registration is required.

slint::include_modules!();
