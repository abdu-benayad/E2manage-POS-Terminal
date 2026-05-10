//! Developer-only theme harness window. Run with `cargo run -- --theme-harness`.
//! Toggles theme mode, RTL flag, and locale — all four configurations on one screen.
//!
//! The harness component itself (`ThemeHarness` in
//! `ui/screens/dev/theme_harness.slint`) inherits `Rectangle`, so Slint does
//! not generate Rust bindings for it directly. We mount it inside
//! `ThemeHarnessWindow` (a thin Window wrapper) which forwards every property
//! and callback to the inner component.

use slint::ComponentHandle;

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let harness = crate::ThemeHarnessWindow::new()?;

    // Initial values — light/LTR/English is the canonical first configuration.
    let (locale_code, rtl) = crate::locale_detect::detect_locale();
    harness.set_mode("light".into());
    harness.set_rtl(rtl);
    harness.set_locale(locale_code.into());

    // Toolbar toggles
    let weak = harness.as_weak();
    harness.on_toggle_theme(move || {
        if let Some(h) = weak.upgrade() {
            let next = if h.get_mode().as_str() == "light" {
                "dark"
            } else {
                "light"
            };
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
