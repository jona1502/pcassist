use crate::PanelWindow;
use assistant_core::Command;
use slint::ComponentHandle;

pub fn handle(cmd: Command, panel: &PanelWindow) {
    match cmd {
        Command::OpenPanel => {
            let _ = panel.show();
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
