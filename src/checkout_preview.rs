//! Developer-only checkout preview. Run with
//! `cargo run -- --checkout-preview`. Renders the Plan 3 skeleton of the
//! main checkout screen with hardcoded mock data, in light/dark × LTR/RTL
//! × en/ar. Not part of the cashier flow.

use slint::ComponentHandle;

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let preview = crate::CheckoutPreviewWindow::new()?;

    // Initial state mirrors detected locale (consistent with the live app).
    let (locale_code, rtl) = crate::locale_detect::detect_locale();
    preview.set_mode("light".into());
    preview.set_rtl(rtl);
    preview.set_locale(locale_code.into());

    let weak = preview.as_weak();
    preview.on_toggle_theme(move || {
        if let Some(p) = weak.upgrade() {
            let next = if p.get_mode() == "light" {
                "dark"
            } else {
                "light"
            };
            p.set_mode(next.into());
        }
    });

    let weak = preview.as_weak();
    preview.on_toggle_rtl(move || {
        if let Some(p) = weak.upgrade() {
            p.set_rtl(!p.get_rtl());
        }
    });

    let weak = preview.as_weak();
    preview.on_cycle_locale(move || {
        if let Some(p) = weak.upgrade() {
            let next = match p.get_locale().as_str() {
                "en" => "ar",
                "ar" => "fr",
                _ => "en",
            };
            p.set_locale(next.into());
        }
    });

    preview.run()?;
    Ok(())
}
