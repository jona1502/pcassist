mod app;
mod hotkey;
mod platform;
mod selection;
mod single_instance;
mod tray;

use assistant_config::Config;
use assistant_core::{Command, Feature};
use global_hotkey::{GlobalHotKeyEvent, HotKeyState};
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

    let weak_action = panel.as_weak();
    let action_config = config.clone();
    panel.on_action_triggered(move |id| {
        let cmd = match id.as_str() {
            "zoom" => Command::ToggleFeature(Feature::Zoom),
            "zoom-in" => Command::ZoomIn,
            "zoom-out" => Command::ZoomOut,
            "zoom-reset" => Command::ZoomReset,
            "speak-pause" => Command::SpeakPause,
            "speak-stop" => Command::SpeakStop,
            "tts" => Command::ToggleFeature(Feature::Tts),
            "mic" => Command::ToggleFeature(Feature::Mic),
            "contrast" => Command::ToggleFeature(Feature::Contrast),
            "settings" => Command::OpenSettings,
            _ => return,
        };
        if let Some(p) = weak_action.upgrade() {
            app::handle(cmd, &p, &action_config);
        }
    });

    // Tray nach Backend-Init (PanelWindow) erstellen und am Leben halten.
    let tray = tray::create()?;

    // Manager must stay alive for the hotkey to remain registered.
    let hotkey = hotkey::register(&config.hotkeys);

    let weak = panel.as_weak();
    let timer = slint::Timer::default();
    timer.start(
        slint::TimerMode::Repeated,
        std::time::Duration::from_millis(50),
        move || {
            let Some(panel) = weak.upgrade() else { return };
            app::tick(&panel, &config);
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
            while let Ok(ev) = GlobalHotKeyEvent::receiver().try_recv() {
                if ev.state != HotKeyState::Pressed {
                    continue;
                }
                if let Some(cmd) = hotkey.command_for(ev.id) {
                    app::handle(cmd, &panel, &config);
                }
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
    app::shutdown();
    Ok(())
}
