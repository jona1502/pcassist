use crate::PanelWindow;
use assistant_core::Command;
use slint::{ComponentHandle, PhysicalPosition, PhysicalSize};

const PANEL_WIDTH: f32 = 360.0;

/// Panel an rechten Rand des Arbeitsbereichs (Monitor unter Cursor) docken.
fn dock_right(panel: &PanelWindow) {
    let Some((x, y, w, h, scale)) = crate::platform::work_area_for_cursor_monitor() else {
        return;
    };
    let width = ((PANEL_WIDTH * scale).round() as i32).min(w).max(1);
    let window = panel.window();
    window.set_size(PhysicalSize::new(width as u32, h.max(1) as u32));
    window.set_position(PhysicalPosition::new(x + w - width, y));
}

pub fn handle(cmd: Command, panel: &PanelWindow) {
    match cmd {
        Command::OpenPanel => {
            dock_right(panel);
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
            handle(next, panel);
        }
        Command::OpenSettings => {
            // TODO: Einstellungen-Fenster
        }
        Command::Quit => {
            let _ = slint::quit_event_loop();
        }
    }
}
