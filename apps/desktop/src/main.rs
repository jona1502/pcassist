mod app;
mod tray;

use assistant_core::Command;
use slint::ComponentHandle;
use tray_icon::menu::MenuEvent;
use tray_icon::{MouseButton, MouseButtonState, TrayIconEvent};

slint::include_modules!();

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let panel = PanelWindow::new()?;
    // Schliessen versteckt nur das Fenster; Beenden nur ueber das Tray-Menue.
    panel
        .window()
        .on_close_requested(|| slint::CloseRequestResponse::HideWindow);

    // Tray nach Backend-Init (PanelWindow) erstellen und am Leben halten.
    let tray = tray::create()?;

    let weak = panel.as_weak();
    let timer = slint::Timer::default();
    timer.start(
        slint::TimerMode::Repeated,
        std::time::Duration::from_millis(50),
        move || {
            let Some(panel) = weak.upgrade() else { return };
            while let Ok(ev) = MenuEvent::receiver().try_recv() {
                let cmd = if ev.id == tray.open_id {
                    Command::OpenPanel
                } else if ev.id == tray.settings_id {
                    Command::OpenSettings
                } else if ev.id == tray.quit_id {
                    Command::Quit
                } else {
                    continue;
                };
                app::handle(cmd, &panel);
            }
            while let Ok(ev) = TrayIconEvent::receiver().try_recv() {
                if let TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                } = ev
                {
                    app::handle(Command::TogglePanel, &panel);
                }
            }
        },
    );

    slint::run_event_loop_until_quit()?;
    drop(timer);
    Ok(())
}
