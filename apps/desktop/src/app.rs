use crate::PanelWindow;
use assistant_config::Config;
use assistant_core::Command;
use slint::{ComponentHandle, PhysicalPosition, PhysicalSize};

/// Panel an rechten Rand des Arbeitsbereichs (Monitor unter Cursor) docken.
fn dock_right(panel: &PanelWindow, panel_width: u32) {
    let Some((x, y, w, h, scale)) = crate::platform::work_area_for_cursor_monitor() else {
        return;
    };
    let width = ((panel_width as f32 * scale).round() as i32).min(w).max(1);
    let window = panel.window();
    window.set_size(PhysicalSize::new(width as u32, h.max(1) as u32));
    window.set_position(PhysicalPosition::new(x + w - width, y));
}

pub fn handle(cmd: Command, panel: &PanelWindow, config: &Config) {
    match cmd {
        Command::OpenPanel => {
            dock_right(panel, config.panel.width);
            let _ = panel.show();
            panel.invoke_focus_panel();
        }
        Command::ClosePanel => {
            let _ = panel.hide();
        }
        Command::TogglePanel => {
            let next = if panel.window().is_visible() {
                Command::ClosePanel
            } else {
                Command::OpenPanel
            };
            handle(next, panel, config);
        }
        Command::OpenSettings => {
            // TODO: Einstellungen-Fenster
        }
        Command::Quit => {
            if let Err(e) = config.save() {
                eprintln!("config save failed: {e}");
            }
            let _ = slint::quit_event_loop();
        }
    }
}
