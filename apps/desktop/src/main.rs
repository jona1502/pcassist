slint::include_modules!();

fn main() -> Result<(), slint::PlatformError> {
    let panel = PanelWindow::new()?;
    panel.show()?;
    slint::run_event_loop()
}
