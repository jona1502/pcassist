mod app;

use slint::ComponentHandle;

slint::include_modules!();

fn main() -> Result<(), slint::PlatformError> {
    let panel = PanelWindow::new()?;
    // Schliessen versteckt nur das Fenster; Beenden nur ueber das Tray-Menue.
    panel
        .window()
        .on_close_requested(|| slint::CloseRequestResponse::HideWindow);
    slint::run_event_loop_until_quit()
}
