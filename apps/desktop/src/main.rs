mod app;
mod platform;
mod single_instance;
mod tray;

use assistant_config::Config;
use assistant_core::Command;
use slint::ComponentHandle;
use tray_icon::menu::MenuEvent;
use tray_icon::{MouseButton, MouseButtonState, TrayIconEvent};

slint::include_modules!();

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let single_rx = match single_instance::acquire() {
        single_instance::Role::Primary(rx) => rx,
        single_instance::Role::Secondary => return Ok(()),
    };

    let config = Config::load();
    let panel = PanelWindow::new()?;
    // Schliessen versteckt nur das Fenster; Beenden nur ueber das Tray-Menue.
    panel
        .window()
        .on_close_requested(|| slint::CloseRequestResponse::HideWindow);

    let weak_close = panel.as_weak();
    let close_config = config.clone();
    panel.on_close_requested(move || {
        if let Some(p) = weak_close.upgrade() {
            app::handle(Command::ClosePanel, &p, &close_config);
        }
    });

    // Tray nach Backend-Init (PanelWindow) erstellen und am Leben halten.
    let tray = tray::create()?;

    let weak = panel.as_weak();
    let timer = slint::Timer::default();
    timer.start(
        slint::TimerMode::Repeated,
        std::time::Duration::from_millis(50),
        move || {
            let Some(panel) = weak.upgrade() else { return };
            while let Ok(cmd) = single_rx.try_recv() {
                app::handle(cmd, &panel, &config);
            }
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
                app::handle(cmd, &panel, &config);
            }
            while let Ok(ev) = TrayIconEvent::receiver().try_recv() {
                if let TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                } = ev
                {
                    app::handle(Command::TogglePanel, &panel, &config);
                }
            }
        },
    );

    slint::run_event_loop_until_quit()?;
    drop(timer);
    Ok(())
}
