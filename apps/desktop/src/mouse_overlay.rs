//! Mausmarker: ein eigenes, klick-durchlaessiges Overlay-Fenster, das einen
//! Ring/eine Scheibe/ein Fadenkreuz um den Mauszeiger zeichnet.
//!
//! Das Fenster bekommt per Win32 `WS_EX_LAYERED | WS_EX_TRANSPARENT |
//! WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW`: Mausklicks gehen durch das Fenster
//! hindurch, es nimmt nie den Fokus und taucht weder in Taskleiste noch in
//! Alt+Tab auf. Solange der Stil nicht gesetzt ist, bleibt das Fenster ausserhalb
//! des sichtbaren Bereichs, damit es nie Klicks abfangen kann.
//!
//! Nur solange der Marker an ist, existieren Fenster und 16-ms-Timer; aus heisst:
//! Fenster zerstoert, kein Timer. Mauspositionen werden nie geloggt.

use crate::MouseOverlay;
use assistant_config::{parse_hex_color, MouseConfig};
use slint::{Color, ComponentHandle, PhysicalPosition, PhysicalSize, Timer, TimerMode};
use std::cell::RefCell;
use std::time::{Duration, Instant};

const FRAME: Duration = Duration::from_millis(16);
/// Dauer des Klick-Impulses.
const PULSE: Duration = Duration::from_millis(250);
/// Rand um den Durchmesser (logische Pixel), damit nichts abgeschnitten wird.
const MARGIN: f32 = 8.0;
/// Parkposition, solange das Fenster noch nicht klick-durchlaessig ist.
const OFFSCREEN: i32 = -32000;
/// Die gefuellte Scheibe ist immer etwas durchsichtiger als Ring/Fadenkreuz,
/// damit der Inhalt darunter lesbar bleibt.
const FILLED_ALPHA_FACTOR: f32 = 0.6;

struct Running {
    win: MouseOverlay,
    /// Haelt den 16-ms-Takt; wird mit dem Fenster zusammen gestoppt.
    _timer: Timer,
    styled: bool,
    /// Vordergrundfenster vor dem Anzeigen, falls die Aktivierung beim Erzeugen
    /// des Fensters den Fokus gestohlen hat.
    prev_foreground: isize,
    last_pos: Option<(i32, i32)>,
    last_size: (u32, u32),
    pulse_start: Option<Instant>,
    buttons_down: bool,
}

#[derive(Default)]
struct State {
    cfg: MouseConfig,
    running: Option<Running>,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::default());
}

/// Uebernimmt eine (neue) Konfiguration und wendet sie sofort an: Marker an/aus,
/// Form, Farbe, Groesse usw. Fuer Start und fuer die Einstellungen-Oberflaeche.
/// Ungueltige Werte werden korrigiert (`MouseConfig::sanitized`).
pub fn apply_config(cfg: &MouseConfig) {
    let cfg = cfg.sanitized();
    STATE.with_borrow_mut(|s| {
        s.cfg = cfg;
        if !s.cfg.enabled {
            s.running = None; // zerstoert Fenster, stoppt Timer
            return;
        }
        match &mut s.running {
            Some(r) => push_props(&r.win, &s.cfg),
            None => s.running = start(&s.cfg),
        }
    });
}

/// Aktuelle (bereinigte) Konfiguration, inkl. an/aus-Zustand. Zum Speichern.
pub fn current_config() -> MouseConfig {
    STATE.with_borrow(|s| s.cfg.clone())
}

/// Marker umschalten; liefert den neuen Zustand.
pub fn toggle() -> bool {
    let mut cfg = current_config();
    cfg.enabled = !cfg.enabled;
    apply_config(&cfg);
    cfg.enabled
}

/// Marker ausblenden und Fenster zerstoeren. Vor dem Beenden aufrufen.
pub fn shutdown() {
    STATE.with_borrow_mut(|s| s.running = None);
}

fn start(cfg: &MouseConfig) -> Option<Running> {
    let win = match MouseOverlay::new() {
        Ok(w) => w,
        Err(e) => {
            eprintln!("pcassist: mouse overlay failed: {e}");
            return None;
        }
    };
    push_props(&win, cfg);
    let size = overlay_size(cfg, 1.0);
    win.window().set_size(PhysicalSize::new(size.0, size.1));
    win.window()
        .set_position(PhysicalPosition::new(OFFSCREEN, OFFSCREEN));
    let prev_foreground = platform::foreground();
    if let Err(e) = win.show() {
        eprintln!("pcassist: mouse overlay failed: {e}");
        return None;
    }
    let timer = Timer::default();
    timer.start(TimerMode::Repeated, FRAME, || {
        STATE.with_borrow_mut(|s| {
            if let Some(r) = &mut s.running {
                r.tick(&s.cfg);
            }
        });
    });
    let mut running = Running {
        win,
        _timer: timer,
        styled: false,
        prev_foreground,
        last_pos: None,
        last_size: size,
        pulse_start: None,
        buttons_down: false,
    };
    // Das Fenster kann hier schon existieren; sonst versucht es `tick` erneut.
    running.try_style();
    Some(running)
}

impl Drop for Running {
    /// Ein angezeigtes Slint-Fenster lebt weiter, bis es versteckt wird; ein
    /// blosses Droppen der Komponente wuerde den Marker sichtbar lassen.
    fn drop(&mut self) {
        let _ = self.win.hide();
    }
}

impl Running {
    fn try_style(&mut self) {
        if !self.styled && platform::make_click_through(&self.win, self.prev_foreground) {
            self.styled = true;
        }
    }

    fn tick(&mut self, cfg: &MouseConfig) {
        self.try_style();
        let Some((x, y, scale)) = platform::cursor_and_scale() else {
            return;
        };
        let size = overlay_size(cfg, scale);
        if size != self.last_size {
            self.win
                .window()
                .set_size(PhysicalSize::new(size.0, size.1));
            self.last_size = size;
        }
        if self.styled {
            let pos = (x - size.0 as i32 / 2, y - size.1 as i32 / 2);
            if self.last_pos != Some(pos) {
                self.win
                    .window()
                    .set_position(PhysicalPosition::new(pos.0, pos.1));
                self.last_pos = Some(pos);
            }
        }
        self.update_pulse(cfg);
    }

    /// Klick-Impuls: Start bei Tastendruck (Flanke), Fortschritt 0..1 in 250 ms.
    fn update_pulse(&mut self, cfg: &MouseConfig) {
        if !cfg.click_highlight {
            self.buttons_down = false;
            if self.pulse_start.take().is_some() {
                self.win.set_pulse(1.0);
            }
            return;
        }
        let down = platform::any_mouse_button_down();
        if down && !self.buttons_down {
            self.pulse_start = Some(Instant::now());
        }
        self.buttons_down = down;
        let Some(start) = self.pulse_start else {
            return;
        };
        let p = start.elapsed().as_secs_f32() / PULSE.as_secs_f32();
        if p >= 1.0 {
            self.pulse_start = None;
            self.win.set_pulse(1.0);
        } else {
            self.win.set_pulse(p);
        }
    }
}

/// Fenstergroesse in physischen Pixeln: Durchmesser plus Rand, mal Skalierung.
fn overlay_size(cfg: &MouseConfig, scale: f32) -> (u32, u32) {
    let px = ((cfg.size as f32 + 2.0 * MARGIN) * scale).ceil().max(1.0) as u32;
    (px, px)
}

fn color(hex: &str, alpha: f32) -> Color {
    let (r, g, b) = parse_hex_color(hex).unwrap_or((255, 210, 63));
    Color::from_argb_u8((alpha.clamp(0.0, 1.0) * 255.0).round() as u8, r, g, b)
}

fn push_props(win: &MouseOverlay, cfg: &MouseConfig) {
    let (shape, factor) = match cfg.shape.as_str() {
        "filled" => (1, FILLED_ALPHA_FACTOR),
        "crosshair" => (2, 1.0),
        _ => (0, 1.0),
    };
    win.set_shape(shape);
    win.set_mark_color(color(&cfg.color, cfg.opacity as f32 / 100.0 * factor));
    win.set_diameter(cfg.size as f32);
    win.set_thickness(cfg.thickness as f32);
    win.set_click_color(color(&cfg.click_color, 1.0));
}

#[cfg(windows)]
mod platform {
    use crate::MouseOverlay;
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use slint::ComponentHandle;
    use windows_sys::Win32::Foundation::{HWND, POINT};
    use windows_sys::Win32::Graphics::Gdi::{MonitorFromPoint, MONITOR_DEFAULTTONEAREST};
    use windows_sys::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        GetAsyncKeyState, VK_LBUTTON, VK_RBUTTON,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetCursorPos, GetForegroundWindow, GetWindowLongPtrW, SetForegroundWindow,
        SetLayeredWindowAttributes, SetWindowLongPtrW, ShowWindow, GWL_EXSTYLE, LWA_ALPHA, SW_HIDE,
        SW_SHOWNOACTIVATE, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TRANSPARENT,
    };

    pub fn foreground() -> isize {
        unsafe { GetForegroundWindow() as isize }
    }

    /// Cursorposition (physische Pixel) und Skalierung des Monitors darunter.
    pub fn cursor_and_scale() -> Option<(i32, i32, f32)> {
        unsafe {
            let mut pt = POINT { x: 0, y: 0 };
            if GetCursorPos(&mut pt) == 0 {
                return None;
            }
            let mon = MonitorFromPoint(pt, MONITOR_DEFAULTTONEAREST);
            let (mut dx, mut dy) = (0u32, 0u32);
            let scale = if !mon.is_null()
                && GetDpiForMonitor(mon, MDT_EFFECTIVE_DPI, &mut dx, &mut dy) == 0
                && dx > 0
            {
                dx as f32 / 96.0
            } else {
                1.0
            };
            Some((pt.x, pt.y, scale))
        }
    }

    pub fn any_mouse_button_down() -> bool {
        // Hoechstes Bit gesetzt (negativer Wert) = Taste gerade gedrueckt.
        unsafe {
            GetAsyncKeyState(VK_LBUTTON as i32) < 0 || GetAsyncKeyState(VK_RBUTTON as i32) < 0
        }
    }

    /// Macht das Fenster klick-durchlaessig, nicht aktivierbar und aus
    /// Taskleiste/Alt+Tab verschwunden. `false`, wenn es noch kein HWND gibt.
    pub fn make_click_through(win: &MouseOverlay, prev_foreground: isize) -> bool {
        let handle = win.window().window_handle();
        let Ok(handle) = handle.window_handle() else {
            return false;
        };
        let RawWindowHandle::Win32(h) = handle.as_raw() else {
            return false;
        };
        let hwnd = h.hwnd.get() as HWND;
        const WANTED: u32 = WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW;
        unsafe {
            let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
            if ex & WANTED != WANTED {
                SetWindowLongPtrW(hwnd, GWL_EXSTYLE, (ex | WANTED) as isize);
                // Ein geschichtetes Fenster zeigt erst nach diesem Aufruf etwas an.
                SetLayeredWindowAttributes(hwnd, 0, 255, LWA_ALPHA);
                // TOOLWINDOW wirkt auf die Taskleiste erst nach Aus-/Einblenden.
                ShowWindow(hwnd, SW_HIDE);
                ShowWindow(hwnd, SW_SHOWNOACTIVATE);
            }
            // Falls das Erzeugen des Fensters es aktiviert hat: Fokus zurueck.
            if GetForegroundWindow() == hwnd && prev_foreground != 0 {
                SetForegroundWindow(prev_foreground as HWND);
            }
        }
        true
    }
}

#[cfg(not(windows))]
mod platform {
    use crate::MouseOverlay;

    pub fn foreground() -> isize {
        0
    }

    pub fn cursor_and_scale() -> Option<(i32, i32, f32)> {
        None
    }

    pub fn any_mouse_button_down() -> bool {
        false
    }

    pub fn make_click_through(_win: &MouseOverlay, _prev_foreground: isize) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlay_size_adds_margin_and_scales() {
        let cfg = MouseConfig::default(); // 64 px
        assert_eq!(overlay_size(&cfg, 1.0), (80, 80));
        assert_eq!(overlay_size(&cfg, 1.5), (120, 120));
    }

    #[test]
    fn invalid_hex_falls_back_to_yellow() {
        assert_eq!(color("nope", 1.0), Color::from_rgb_u8(255, 210, 63));
        assert_eq!(color("#000000", 0.5).alpha(), 128);
    }
}
