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
            let next = if g.get_mode() == "light" {
                "dark"
            } else {
                "light"
            };
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
