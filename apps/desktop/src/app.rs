use crate::PanelWindow;
use assistant_config::Config;
use assistant_core::{Command, Feature};
use assistant_tts::Tts;
use assistant_zoom::Zoom;
use slint::{ComponentHandle, PhysicalPosition, PhysicalSize};
use std::cell::{Cell, RefCell};
use std::sync::mpsc::{Receiver, TryRecvError};

/// Abstand der Dock-Leiste zum Bildschirmrand (logische Pixel).
const DOCK_MARGIN: f32 = 8.0;

/// Panel als schmale Dock-Leiste an den rechten Rand des Arbeitsbereichs
/// (Monitor unter Cursor) setzen, vertikal zentriert.
/// Groesse kommt aus dem Slint-Layout (`content-width`/`content-height`);
/// `panel.width` aus der Config wird bewusst ignoriert (bleibt nur fuer
/// alte config.toml-Dateien erhalten).
fn dock_right(panel: &PanelWindow) {
    let Some((x, y, w, h, scale)) = crate::platform::work_area_for_cursor_monitor() else {
        return;
    };
    let margin = (DOCK_MARGIN * scale).round() as i32;
    let width = ((panel.get_content_width() * scale).round() as i32)
        .min(w - margin)
        .max(1);
    let height = ((panel.get_content_height() * scale).round() as i32)
        .min(h - 2 * margin)
        .max(1);
    let window = panel.window();
    window.set_size(PhysicalSize::new(width as u32, height as u32));
    window.set_position(PhysicalPosition::new(
        x + w - width - margin,
        y + (h - height) / 2,
    ));
}

thread_local! {
    static FEATURES: Cell<[bool; 5]> = const { Cell::new([false; 5]) };
    /// Lazily created on first zoom use; dropped in `shutdown`.
    static ZOOM_STEP: Cell<assistant_zoom::ZoomStep> = const { Cell::new(assistant_zoom::ZoomStep::Coarse) };
    static ZOOM: RefCell<Option<Zoom>> = const { RefCell::new(None) };
    /// Lazily created on first speech; dropped in `shutdown`.
    static TTS: RefCell<Option<Tts>> = const { RefCell::new(None) };
    /// Current speech rate (-10..=10); applied to a lazily created `Tts` and live.
    static TTS_RATE: Cell<i32> = const { Cell::new(0) };
    /// Wanted speech output device (`None` = system default).
    static TTS_OUTPUT: RefCell<Option<String>> = const { RefCell::new(None) };
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

/// Applies the configured zoom step (invalid values fall back to 100 %) and
/// the speech rate.
pub fn init_runtime(config: &Config) {
    let step = assistant_zoom::ZoomStep::from_percent(config.zoom.step).unwrap_or_default();
    ZOOM_STEP.set(step);
    TTS_RATE.set(assistant_tts::clamp_rate(config.tts.rate));
    TTS_OUTPUT.with_borrow_mut(|o| o.clone_from(&config.audio.output_device));
}

/// Applies a saved config to the running app: zoom step, speech rate, the
/// dock's always-on-top flag and the mouse highlight.
pub fn apply_config(config: &Config, panel: &PanelWindow) {
    init_runtime(config);
    sync_zoom(panel);
    panel.set_keep_on_top(config.panel.always_on_top);
    set_tts_rate(config.tts.rate);
    set_tts_output(config.audio.output_device.as_deref());
    crate::mouse_overlay::apply_config(&config.mouse);
}

/// Sets the speech rate, also on a running `Tts`.
pub fn set_tts_rate(rate: i32) {
    let rate = assistant_tts::clamp_rate(rate);
    TTS_RATE.set(rate);
    TTS.with_borrow(|t| {
        if let Some(t) = t {
            t.set_rate(rate);
        }
    });
}

/// Routes speech to the named output device (`None` = default), also on a
/// running `Tts`. Unknown devices silently fall back inside the TTS worker.
pub fn set_tts_output(name: Option<&str>) {
    let changed = TTS_OUTPUT.with_borrow_mut(|o| {
        let changed = o.as_deref() != name;
        *o = name.map(str::to_owned);
        changed
    });
    if changed {
        TTS.with_borrow(|t| {
            if let Some(t) = t {
                t.set_output_device(name);
            }
        });
    }
}

/// Speaks a short test sentence with `rate` on `output` (not persisted).
pub fn speak_test(rate: i32, output: Option<&str>) -> bool {
    let rate = assistant_tts::clamp_rate(rate);
    set_tts_output(output);
    with_tts(|t| {
        t.set_rate(rate);
        t.speak("Dies ist ein Test");
    })
    .is_some()
}

/// Pushes current zoom percent and step to the panel.
pub fn sync_zoom(panel: &PanelWindow) {
    let pct = ZOOM.with_borrow(|z| z.as_ref().map_or(100, Zoom::percent));
    panel.set_zoom_percent(pct as i32);
    panel.set_zoom_step(zoom_step().percent() as i32);
}

/// User picked a step size: remember, persist, update the panel.
pub fn select_zoom_step(percent: i32, panel: &PanelWindow) {
    let Some(step) = u32::try_from(percent)
        .ok()
        .and_then(assistant_zoom::ZoomStep::from_percent)
    else {
        return;
    };
    ZOOM_STEP.set(step);
    sync_zoom(panel);
    persist_current();
}

/// Saves the current config including the live zoom step.
fn persist_current() {
    let mut c = crate::settings::current();
    c.zoom.step = zoom_step().percent();
    // Only the on/off state follows the live overlay: other mouse values may be an
    // unsaved preview from the settings window and are saved by "Speichern" only.
    c.mouse.enabled = crate::mouse_overlay::current_config().enabled;
    crate::settings::set_current(c.clone());
    if let Err(e) = c.save() {
        eprintln!("config save failed: {e}");
    }
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
fn with_tts<R>(f: impl FnOnce(&Tts) -> R) -> Option<R> {
    TTS.with_borrow_mut(|slot| {
        if slot.is_none() {
            match Tts::new() {
                Ok(t) => {
                    t.set_rate(TTS_RATE.get());
                    TTS_OUTPUT.with_borrow(|o| {
                        if o.is_some() {
                            t.set_output_device(o.as_deref());
                        }
                    });
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

fn speak_pause() -> Option<&'static str> {
    if !is_speaking() {
        return None;
    }
    let paused = !PAUSED.get();
    if with_tts(|t| if paused { t.pause() } else { t.resume() }).is_none() {
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
pub fn tick(panel: &PanelWindow) {
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
                match with_tts(|tts| tts.speak(&t)) {
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
    crate::mouse_overlay::shutdown();
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
        // MouseHighlight is real (see `handle`); never reaches the placeholder.
        Feature::MouseHighlight => (4, "Mausmarker"),
    };
    let mut state = FEATURES.get();
    state[idx] = !state[idx];
    FEATURES.set(state);
    let on = if state[idx] { "an" } else { "aus" };
    format!("{name}: {on} (Platzhalter)")
}

pub fn handle(cmd: Command, panel: &PanelWindow) {
    match cmd {
        Command::OpenPanel => {
            if let Some(h) = crate::selection::foreground_external() {
                LAST_EXTERNAL.set(Some(h));
            }
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
        Command::OpenSettings => crate::settings::open(panel),
        Command::ToggleFeature(Feature::Zoom) => {
            panel.set_status_text(toggle_zoom().into());
            sync_zoom(panel);
        }
        Command::ToggleFeature(Feature::MouseHighlight) => {
            let on = crate::mouse_overlay::toggle();
            persist_current();
            crate::settings::sync_mouse_enabled();
            let msg = if on {
                "Mausmarker an"
            } else {
                "Mausmarker aus"
            };
            panel.set_status_text(msg.into());
        }
        Command::ToggleFeature(Feature::Tts) => {
            let next = if is_speaking() {
                Command::SpeakStop
            } else {
                Command::SpeakSelection
            };
            handle(next, panel);
        }
        Command::SpeakSelection => {
            if let Some(msg) = speak_selection() {
                panel.set_status_text(msg.into());
            }
        }
        Command::SpeakPause => {
            if let Some(msg) = speak_pause() {
                panel.set_status_text(msg.into());
            }
        }
        Command::SpeakStop => panel.set_status_text(speak_stop().into()),
        Command::ToggleFeature(f) => {
            panel.set_status_text(toggle_feature(f).into());
        }
        Command::ZoomIn => {
            panel.set_status_text(with_zoom(|z| z.zoom_in(zoom_step())).into());
            sync_zoom(panel);
        }
        Command::ZoomOut => {
            panel.set_status_text(with_zoom(|z| z.zoom_out(zoom_step())).into());
            sync_zoom(panel);
        }
        Command::ZoomReset => {
            panel.set_status_text(with_zoom(Zoom::reset).into());
            sync_zoom(panel);
        }
        Command::Quit => {
            shutdown();
            persist_current();
            let _ = slint::quit_event_loop();
        }
    }
}
