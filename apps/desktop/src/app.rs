use crate::PanelWindow;
use assistant_config::Config;
use assistant_core::{Command, Feature};
use assistant_zoom::Zoom;
use slint::{ComponentHandle, PhysicalPosition, PhysicalSize};
use std::cell::{Cell, RefCell};

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

thread_local! {
    static FEATURES: Cell<[bool; 4]> = const { Cell::new([false; 4]) };
    /// Lazily created on first zoom use; dropped in `shutdown`.
    static ZOOM: RefCell<Option<Zoom>> = const { RefCell::new(None) };
}

const ZOOM_UNAVAILABLE: &str = "Zoom nicht verfügbar";
const ZOOM_ON_LEVEL: f32 = 2.0;

fn zoom_status(level: f32) -> String {
    format!("Zoom: {} %", (level * 100.0).round() as i32)
}

/// Runs `f` on the lazily created `Zoom`; returns the status text.
fn with_zoom(f: impl FnOnce(&mut Zoom) -> Result<(), assistant_zoom::ZoomError>) -> String {
    ZOOM.with_borrow_mut(|slot| {
        if slot.is_none() {
            match Zoom::new() {
                Ok(z) => *slot = Some(z),
                Err(e) => {
                    eprintln!("pcassist: zoom init failed: {e}");
                    return ZOOM_UNAVAILABLE.to_string();
                }
            }
        }
        let zoom = slot.as_mut().expect("zoom initialized above");
        match f(zoom) {
            Ok(()) => zoom_status(zoom.level()),
            Err(e) => {
                eprintln!("pcassist: zoom failed: {e}");
                ZOOM_UNAVAILABLE.to_string()
            }
        }
    })
}

fn toggle_zoom() -> String {
    with_zoom(|z| {
        if z.level() > assistant_zoom::MIN_LEVEL {
            z.reset()
        } else {
            z.set_level(ZOOM_ON_LEVEL)
        }
    })
}

/// Called from the 50 ms timer: keeps the zoomed view on the cursor.
pub fn tick() {
    ZOOM.with_borrow_mut(|slot| {
        if let Some(z) = slot
            .as_mut()
            .filter(|z| z.level() > assistant_zoom::MIN_LEVEL)
        {
            if let Err(e) = z.follow_cursor() {
                eprintln!("pcassist: zoom follow failed: {e}");
            }
        }
    });
}

/// Resets the screen to 100 % (drops `Zoom`). Call before exit.
pub fn shutdown() {
    ZOOM.with_borrow_mut(|slot| *slot = None);
}

/// Platzhalter: schaltet Feature-Flag um, liefert Statustext.
fn toggle_feature(f: Feature) -> String {
    let (idx, name) = match f {
        // Zoom is real (see `toggle_zoom`); never reaches the placeholder.
        Feature::Zoom => (0, "Zoom"),
        Feature::Tts => (1, "Vorlesen"),
        Feature::Mic => (2, "Mikrofon"),
        Feature::Contrast => (3, "Kontrast"),
    };
    let mut state = FEATURES.get();
    state[idx] = !state[idx];
    FEATURES.set(state);
    let on = if state[idx] { "an" } else { "aus" };
    format!("{name}: {on} (Platzhalter)")
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
        Command::ToggleFeature(Feature::Zoom) => {
            panel.set_status_text(toggle_zoom().into());
        }
        Command::ToggleFeature(f) => {
            panel.set_status_text(toggle_feature(f).into());
        }
        Command::ZoomIn => panel.set_status_text(with_zoom(Zoom::zoom_in).into()),
        Command::ZoomOut => panel.set_status_text(with_zoom(Zoom::zoom_out).into()),
        Command::ZoomReset => panel.set_status_text(with_zoom(Zoom::reset).into()),
        Command::Quit => {
            shutdown();
            if let Err(e) = config.save() {
                eprintln!("config save failed: {e}");
            }
            let _ = slint::quit_event_loop();
        }
    }
}
