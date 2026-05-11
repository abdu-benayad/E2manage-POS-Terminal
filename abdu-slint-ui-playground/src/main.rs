// abdu-slint-ui playground binary.
// Mounts the PlaygroundWindow defined in ui/playground.slint and seeds
// the abdu-slint-ui global tokens from playground defaults.

slint::include_modules!();

fn main() -> Result<(), slint::PlatformError> {
    let window = PlaygroundWindow::new()?;
    window.run()
}
