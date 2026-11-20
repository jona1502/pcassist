use crate::PanelWindow;
use assistant_config::Config;
use assistant_core::{Command, Feature};
use assistant_tts::Tts;
use assistant_zoom::Zoom;
use slint::{ComponentHandle, PhysicalPosition, PhysicalSize};
use std::cell::{Cell, RefCell};
use std::sync::mpsc::{Receiver, TryRecvError};

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
    static ZOOM_STEP: Cell<assistant_zoom::ZoomStep> = const { Cell::new(assistant_zoom::ZoomStep::Coarse) };
    static ZOOM: RefCell<Option<Zoom>> = const { RefCell::new(None) };
    /// Lazily created on first speech; dropped in `shutdown`.
    static TTS: RefCell<Option<Tts>> = const { RefCell::new(None) };
    /// Running selection capture (worker thread result).
    static CAPTURE: RefCell<Option<Receiver<Option<String>>>> = const { RefCell::new(None) };
    static PAUSED: Cell<bool> = const { Cell::new(false) };
    /// Foreground window (HWND) of another process before the panel opened.
    static LAST_EXTERNAL: Cell<Option<isize>> = const { Cell::new(None) };
}

const ZOOM_UNAVAILABLE: &str = "Zoom nicht verfügbar";
const ZOOM_ON_LEVEL: f32 = 2.0;

fn zoom_step() -> assistant_zoom::ZoomStep {
    ZOOM_STEP.get()
}

/// Applies the configured zoom step (invalid values fall back to 100 %).
pub fn init_zoom_step(config: &Config) {
    let step = assistant_zoom::ZoomStep::from_percent(config.zoom.step).unwrap_or_default();
    ZOOM_STEP.set(step);
}

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

const TTS_UNAVAILABLE: &str = "Vorlesen nicht verfügbar";
const TTS_NO_SELECTION: &str = "Kein Text markiert oder App unterstützt Kopieren nicht";
const TTS_RUNNING: &str = "Vorlesen läuft";

/// Runs `f` on the lazily created `Tts`; `None` if unavailable.
fn with_tts<R>(rate: i32, f: impl FnOnce(&Tts) -> R) -> Option<R> {
    TTS.with_borrow_mut(|slot| {
        if slot.is_none() {
            match Tts::new() {
                Ok(t) => {
                    t.set_rate(rate);
                    *slot = Some(t);
                }
                Err(e) => {
                    eprintln!("pcassist: tts init failed: {e}");
                    return None;
                }
            }
        }
        slot.as_ref().map(f)
    })
}

fn is_speaking() -> bool {
    TTS.with_borrow(|t| t.as_ref().is_some_and(Tts::is_speaking))
}

/// Starts a background capture; returns the status text, or `None` if one is running.
fn speak_selection() -> Option<String> {
    if CAPTURE.with_borrow(Option::is_some) {
        return None;
    }
    // Panel button: our panel is foreground, so Ctrl+C must go to the window
    // that was focused before the panel opened. Hotkey: external app already is.
    let restore = if crate::selection::own_window_is_foreground() {
        LAST_EXTERNAL.get()
    } else {
        None
    };
    CAPTURE.with_borrow_mut(|c| *c = Some(crate::selection::capture_async(restore)));
    Some("Text wird gelesen…".into())
}

fn speak_pause(rate: i32) -> Option<&'static str> {
    if !is_speaking() {
        return None;
    }
    let paused = !PAUSED.get();
    if with_tts(rate, |t| if paused { t.pause() } else { t.resume() }).is_none() {
        return Some(TTS_UNAVAILABLE);
    }
    PAUSED.set(paused);
    Some(if paused {
        "Vorlesen pausiert"
    } else {
        TTS_RUNNING
    })
}

fn speak_stop() -> &'static str {
    PAUSED.set(false);
    TTS.with_borrow(|t| {
        if let Some(t) = t {
            t.stop();
        }
    });
    "Vorlesen gestoppt"
}

/// Called from the 50 ms timer: finished selection capture, zoom follow.
pub fn tick(panel: &PanelWindow, config: &Config) {
    let done = CAPTURE.with_borrow_mut(|slot| {
        let res = match slot.as_ref()?.try_recv() {
            Ok(text) => text,
            Err(TryRecvError::Disconnected) => None,
            Err(TryRecvError::Empty) => return None,
        };
        *slot = None;
        Some(res)
    });
    if let Some(text) = done {
        let msg = match text {
            Some(t) => {
                PAUSED.set(false);
                match with_tts(config.tts.rate, |tts| tts.speak(&t)) {
                    Some(()) => TTS_RUNNING,
                    None => TTS_UNAVAILABLE,
                }
            }
            None => TTS_NO_SELECTION,
        };
        panel.set_status_text(msg.into());
    }
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
    // Drop stops speech and joins the worker thread.
    TTS.with_borrow_mut(|slot| *slot = None);
}

/// Platzhalter: schaltet Feature-Flag um, liefert Statustext.
fn toggle_feature(f: Feature) -> String {
    let (idx, name) = match f {
        // Zoom is real (see `toggle_zoom`); never reaches the placeholder.
        Feature::Zoom => (0, "Zoom"),
        // Tts is real (see `handle`); never reaches the placeholder.
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
            if let Some(h) = crate::selection::foreground_external() {
                LAST_EXTERNAL.set(Some(h));
            }
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
        Command::ToggleFeature(Feature::Tts) => {
            let next = if is_speaking() {
                Command::SpeakStop
            } else {
                Command::SpeakSelection
            };
            handle(next, panel, config);
        }
        Command::SpeakSelection => {
            if let Some(msg) = speak_selection() {
                panel.set_status_text(msg.into());
            }
        }
        Command::SpeakPause => {
            if let Some(msg) = speak_pause(config.tts.rate) {
                panel.set_status_text(msg.into());
            }
        }
        Command::SpeakStop => panel.set_status_text(speak_stop().into()),
        Command::ToggleFeature(f) => {
            panel.set_status_text(toggle_feature(f).into());
        }
        Command::ZoomIn => panel.set_status_text(with_zoom(|z| z.zoom_in(zoom_step())).into()),
        Command::ZoomOut => panel.set_status_text(with_zoom(|z| z.zoom_out(zoom_step())).into()),
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
