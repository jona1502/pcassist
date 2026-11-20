//! Fullscreen zoom via the Windows Magnification API (magnification.dll).
//!
//! Limitations:
//! - Multi-monitor: the fullscreen transform is driven with the *primary*
//!   monitor size (`GetSystemMetrics(SM_CXSCREEN/SM_CYSCREEN)`). Cursor
//!   following is therefore only correct on the primary monitor; on other
//!   monitors the view is clamped to the primary monitor's range.
//! - The process should be per-monitor DPI aware, otherwise cursor and screen
//!   coordinates are virtualized and offsets are off on scaled displays. The
//!   Slint/winit app is (thread DPI awareness context read at runtime: per
//!   monitor v2), so `GetCursorPos` and `GetSystemMetrics` are physical pixels.
//! - `GetCursorPos` under an active transform returns desktop coordinates, not
//!   magnified-screen coordinates (verified: equals the position set with
//!   `SetCursorPos`).
//!
//! Crash safety (best effort, honest summary):
//! - `Drop` resets to 1.0 and calls `MagUninitialize`.
//! - A chained panic hook and a console control handler
//!   (`SetConsoleCtrlHandler`: Ctrl+C, console close, logoff, shutdown) try to
//!   reset to 1.0 before the process dies. Both are best effort: the ctrl
//!   handler runs on a system-created thread, and whether Magnification calls
//!   succeed from a thread other than the initializing one is unverified.
//! - A hard kill (`taskkill /f`, `TerminateProcess`, power loss) cannot be
//!   intercepted. The Magnification fullscreen transform is, to our
//!   understanding, owned by the process that called `MagInitialize` and is
//!   dropped by the system when that process exits, so the screen should
//!   return to 100 %. This is NOT verified against Microsoft documentation
//!   and NOT tested here. If wrong, a stuck zoom can be cleared by starting
//!   the app again and pressing reset, or via the Windows magnifier
//!   (Win+Plus, then Win+Minus until 100 %, or Win+Esc to close it).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Once;

use windows_sys::Win32::Foundation::{BOOL, POINT};
use windows_sys::Win32::System::Console::SetConsoleCtrlHandler;
use windows_sys::Win32::UI::Magnification::{
    MagGetFullscreenTransform, MagInitialize, MagSetFullscreenTransform, MagUninitialize,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GetCursorPos, GetSystemMetrics, SM_CXSCREEN, SM_CYSCREEN,
};

pub const MIN_PERCENT: u32 = 100;
pub const MAX_PERCENT: u32 = 800;
pub const MIN_LEVEL: f32 = 1.0;
pub const MAX_LEVEL: f32 = 8.0;

/// Step size of `zoom_in` / `zoom_out`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ZoomStep {
    /// 100 %
    #[default]
    Coarse,
    /// 50 %
    Medium,
    /// 10 %
    Fine,
}

impl ZoomStep {
    pub fn percent(self) -> u32 {
        match self {
            ZoomStep::Coarse => 100,
            ZoomStep::Medium => 50,
            ZoomStep::Fine => 10,
        }
    }

    /// Only 100, 50 and 10 are valid.
    pub fn from_percent(p: u32) -> Option<ZoomStep> {
        match p {
            100 => Some(ZoomStep::Coarse),
            50 => Some(ZoomStep::Medium),
            10 => Some(ZoomStep::Fine),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ZoomError {
    /// `MagInitialize` failed (Win32 error code).
    Init(u32),
    /// `MagSetFullscreenTransform` failed (Win32 error code).
    Transform(u32),
}

impl std::fmt::Display for ZoomError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ZoomError::Init(c) => write!(f, "MagInitialize failed (error {c})"),
            ZoomError::Transform(c) => write!(f, "MagSetFullscreenTransform failed (error {c})"),
        }
    }
}

impl std::error::Error for ZoomError {}

// ---- pure helpers -------------------------------------------------------

/// Clamps to `MIN_LEVEL..=MAX_LEVEL`; NaN becomes `MIN_LEVEL`.
pub fn clamp_level(level: f32) -> f32 {
    if level.is_nan() {
        MIN_LEVEL
    } else {
        level.clamp(MIN_LEVEL, MAX_LEVEL)
    }
}

/// Zoom factor as integer percent (rounded, clamped to 100..=800).
pub fn percent_of(level: f32) -> u32 {
    (clamp_level(level) * 100.0).round() as u32
}

/// Next multiple of `step` strictly above `percent`, clamped to 100..=800.
pub fn step_up(percent: u32, step: ZoomStep) -> u32 {
    let s = step.percent();
    ((percent / s + 1) * s).clamp(MIN_PERCENT, MAX_PERCENT)
}

/// Previous multiple of `step` strictly below `percent`, clamped to 100..=800.
pub fn step_down(percent: u32, step: ZoomStep) -> u32 {
    let s = step.percent();
    let below = if percent.is_multiple_of(s) {
        percent.saturating_sub(s)
    } else {
        percent / s * s
    };
    below.clamp(MIN_PERCENT, MAX_PERCENT)
}

/// Offset on one axis so the view centers on `cursor`, clamped to
/// `0..=screen - screen/level`.
pub fn clamp_offset(cursor: i32, screen: i32, level: f32) -> i32 {
    let level = clamp_level(level);
    let view = screen as f32 / level;
    let max = (screen as f32 - view).floor().max(0.0) as i32;
    let want = (cursor as f32 - view / 2.0).round() as i32;
    want.clamp(0, max)
}

/// Offset on one axis that maps the pointer proportionally: pointer at the
/// first pixel of the screen gives offset 0, pointer at the last pixel gives
/// the maximum offset `screen - screen/level`, so every screen edge (taskbar,
/// dock) is reachable. In between the offset is linear in `cursor`.
///
/// The magnified pointer position `(cursor - offset) * level` then equals the
/// cursor position within `level` pixels, i.e. the pointer stays where the
/// mouse physically is and the dock under it stays under it. Centering the
/// view on the cursor instead made Windows deliver presses on the dock's
/// stepper tiles to the window below (measured, see `follow_cursor`).
pub fn proportional_offset(cursor: i32, screen: i32, level: f32) -> i32 {
    let level = clamp_level(level);
    let max = (screen as f32 - screen as f32 / level).floor().max(0.0);
    let span = (screen - 1).max(1) as f32;
    let t = (cursor as f32 / span).clamp(0.0, 1.0);
    ((t * max).round() as i32).clamp(0, max as i32)
}

// ---- crash safety --------------------------------------------------------

/// True between a successful `MagInitialize` and `MagUninitialize`.
static ACTIVE: AtomicBool = AtomicBool::new(false);
static SAFETY_NET: Once = Once::new();

/// Best-effort reset to 100 % without touching any `Zoom` value.
fn emergency_reset() {
    if ACTIVE.load(Ordering::SeqCst) {
        // SAFETY: plain FFI call with scalar arguments.
        unsafe {
            MagSetFullscreenTransform(1.0, 0, 0);
        }
    }
}

unsafe extern "system" fn ctrl_handler(_ctrl_type: u32) -> BOOL {
    emergency_reset();
    0 // not handled: default handling (process exit) continues
}

fn install_safety_net() {
    SAFETY_NET.call_once(|| {
        let prev = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            emergency_reset();
            prev(info);
        }));
        // SAFETY: handler is a valid `extern "system"` fn for the whole process lifetime.
        unsafe {
            SetConsoleCtrlHandler(Some(ctrl_handler), 1);
        }
    });
}

// ---- Zoom ---------------------------------------------------------------

/// RAII fullscreen zoom. Only one should exist at a time.
pub struct Zoom {
    level: f32,
    offset: (i32, i32),
}

fn last_error() -> u32 {
    std::io::Error::last_os_error().raw_os_error().unwrap_or(0) as u32
}

impl Zoom {
    pub fn new() -> Result<Zoom, ZoomError> {
        install_safety_net();
        // SAFETY: plain FFI calls.
        unsafe {
            if MagInitialize() == 0 {
                return Err(ZoomError::Init(last_error()));
            }
            ACTIVE.store(true, Ordering::SeqCst);
            let (mut lvl, mut x, mut y) = (1.0f32, 0i32, 0i32);
            if MagGetFullscreenTransform(&mut lvl, &mut x, &mut y) == 0 {
                lvl = 1.0;
                x = 0;
                y = 0;
            }
            Ok(Zoom {
                level: lvl,
                offset: (x, y),
            })
        }
    }

    pub fn level(&self) -> f32 {
        self.level
    }

    /// Sets the level (clamped to 1.0..=8.0). The view follows the cursor
    /// proportionally right away (no jump on the next tick); without a cursor
    /// position the view center is kept where it was.
    pub fn set_level(&mut self, level: f32) -> Result<(), ZoomError> {
        let level = clamp_level(level);
        let (sw, sh) = screen_size();
        let (x, y) = if level <= MIN_LEVEL {
            (0, 0)
        } else if let Some(pt) = cursor_pos() {
            (
                proportional_offset(pt.x, sw, level),
                proportional_offset(pt.y, sh, level),
            )
        } else {
            let cx = self.offset.0 + (sw as f32 / self.level / 2.0) as i32;
            let cy = self.offset.1 + (sh as f32 / self.level / 2.0) as i32;
            (clamp_offset(cx, sw, level), clamp_offset(cy, sh, level))
        };
        self.apply(level, x, y)
    }

    /// Current zoom in percent (100..=800).
    pub fn percent(&self) -> u32 {
        percent_of(self.level)
    }

    pub fn zoom_in(&mut self, step: ZoomStep) -> Result<(), ZoomError> {
        self.set_level(step_up(self.percent(), step) as f32 / 100.0)
    }

    pub fn zoom_out(&mut self, step: ZoomStep) -> Result<(), ZoomError> {
        self.set_level(step_down(self.percent(), step) as f32 / 100.0)
    }

    pub fn reset(&mut self) -> Result<(), ZoomError> {
        self.apply(1.0, 0, 0)
    }

    /// Moves the view proportionally to the cursor (see `proportional_offset`),
    /// so all screen edges stay reachable and clicks hit the window under the
    /// pointer. No-op at level 1.0 or when the offset would not change.
    ///
    /// Why not "center on the cursor": measured on Windows 11 (2560x1440,
    /// 100 % scaling) with injected mouse input, a press while zoomed at 2x was
    /// not delivered to the always-on-top dock for most view offsets (the
    /// window below got and activated it); with proportional offsets every
    /// press reached the dock. Microsoft documents neither behaviour, and real
    /// hardware input was not tested.
    pub fn follow_cursor(&mut self) -> Result<(), ZoomError> {
        if self.level <= MIN_LEVEL {
            return Ok(());
        }
        let Some(pt) = cursor_pos() else {
            return Ok(());
        };
        let (sw, sh) = screen_size();
        let x = proportional_offset(pt.x, sw, self.level);
        let y = proportional_offset(pt.y, sh, self.level);
        if (x, y) == self.offset {
            return Ok(());
        }
        self.apply(self.level, x, y)
    }

    fn apply(&mut self, level: f32, x: i32, y: i32) -> Result<(), ZoomError> {
        // SAFETY: plain FFI call.
        if unsafe { MagSetFullscreenTransform(level, x, y) } == 0 {
            return Err(ZoomError::Transform(last_error()));
        }
        self.level = level;
        self.offset = (x, y);
        Ok(())
    }
}

impl Drop for Zoom {
    fn drop(&mut self) {
        let _ = self.reset();
        ACTIVE.store(false, Ordering::SeqCst);
        // SAFETY: plain FFI call; paired with the MagInitialize in `new`.
        unsafe {
            MagUninitialize();
        }
    }
}

fn cursor_pos() -> Option<POINT> {
    let mut pt = POINT { x: 0, y: 0 };
    // SAFETY: `pt` is a valid out pointer.
    (unsafe { GetCursorPos(&mut pt) } != 0).then_some(pt)
}

/// Primary monitor size in pixels.
fn screen_size() -> (i32, i32) {
    // SAFETY: plain FFI calls.
    let (w, h) = unsafe { (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN)) };
    (w.max(1), h.max(1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamp_level_range() {
        assert_eq!(clamp_level(0.2), 1.0);
        assert_eq!(clamp_level(100.0), 8.0);
        assert_eq!(clamp_level(f32::NAN), 1.0);
        assert_eq!(clamp_level(2.5), 2.5);
    }

    #[test]
    fn step_sizes() {
        assert_eq!(ZoomStep::Coarse.percent(), 100);
        assert_eq!(ZoomStep::Medium.percent(), 50);
        assert_eq!(ZoomStep::Fine.percent(), 10);
        assert_eq!(ZoomStep::from_percent(50), Some(ZoomStep::Medium));
        assert_eq!(ZoomStep::from_percent(25), None);
    }

    #[test]
    fn stepping_coarse() {
        assert_eq!(step_up(100, ZoomStep::Coarse), 200);
        assert_eq!(step_up(200, ZoomStep::Coarse), 300);
        assert_eq!(step_up(250, ZoomStep::Coarse), 300);
        assert_eq!(step_down(300, ZoomStep::Coarse), 200);
        assert_eq!(step_down(250, ZoomStep::Coarse), 200);
    }

    #[test]
    fn stepping_medium_fine() {
        assert_eq!(step_up(100, ZoomStep::Medium), 150);
        assert_eq!(step_down(150, ZoomStep::Medium), 100);
        assert_eq!(step_up(100, ZoomStep::Fine), 110);
        assert_eq!(step_down(110, ZoomStep::Fine), 100);
        assert_eq!(step_up(105, ZoomStep::Fine), 110);
        assert_eq!(step_down(105, ZoomStep::Fine), 100);
    }

    #[test]
    fn stepping_clamps_at_edges() {
        for s in [ZoomStep::Coarse, ZoomStep::Medium, ZoomStep::Fine] {
            assert_eq!(step_down(100, s), 100);
            assert_eq!(step_up(800, s), 800);
            assert_eq!(step_up(795, s), 800);
            assert_eq!(step_down(50, s), 100);
            assert_eq!(step_up(0, s), 100);
        }
        assert_eq!(step_down(800, ZoomStep::Coarse), 700);
        assert_eq!(step_down(800, ZoomStep::Fine), 790);
    }

    #[test]
    fn full_walk_fine() {
        let mut p = 100;
        let mut n = 0;
        while p < MAX_PERCENT {
            p = step_up(p, ZoomStep::Fine);
            n += 1;
        }
        assert_eq!(n, 70);
        while p > MIN_PERCENT {
            p = step_down(p, ZoomStep::Fine);
        }
        assert_eq!(p, 100);
    }

    #[test]
    fn percent_conversion() {
        assert_eq!(percent_of(1.0), 100);
        assert_eq!(percent_of(1.5), 150);
        assert_eq!(percent_of(8.0), 800);
        assert_eq!(percent_of(f32::NAN), 100);
    }

    #[test]
    fn offset_centers_and_clamps() {
        // 1920 wide, 2x: view 960, max offset 960.
        assert_eq!(clamp_offset(960, 1920, 2.0), 480);
        assert_eq!(clamp_offset(0, 1920, 2.0), 0);
        assert_eq!(clamp_offset(100, 1920, 2.0), 0);
        assert_eq!(clamp_offset(1919, 1920, 2.0), 960);
        assert_eq!(clamp_offset(5000, 1920, 2.0), 960);
        assert_eq!(clamp_offset(-50, 1920, 2.0), 0);
    }

    #[test]
    fn offset_at_level_one_is_zero() {
        assert_eq!(clamp_offset(500, 1080, 1.0), 0);
        assert_eq!(clamp_offset(500, 1080, 0.1), 0);
    }

    #[test]
    fn proportional_reaches_every_edge() {
        // 2560x1440: pointer on the last row shows the taskbar, last column
        // the dock at the right edge, first row/column the top/left edge.
        for l in [1.1f32, 1.5, 2.0, 3.0, 8.0] {
            let (w, h) = (2560, 1440);
            let (mx, my) = (
                (w as f32 - w as f32 / l).floor() as i32,
                (h as f32 - h as f32 / l).floor() as i32,
            );
            assert_eq!(proportional_offset(0, w, l), 0);
            assert_eq!(proportional_offset(0, h, l), 0);
            assert_eq!(proportional_offset(w - 1, w, l), mx);
            // view bottom at the last offset reaches the screen bottom
            assert!(proportional_offset(h - 1, h, l) + (h as f32 / l).ceil() as i32 >= h);
            assert_eq!(proportional_offset(h - 1, h, l), my);
        }
    }

    #[test]
    fn proportional_is_monotonic_and_clamps() {
        let mut prev = 0;
        for c in -50..1500 {
            let o = proportional_offset(c, 1440, 3.0);
            assert!(o >= prev);
            assert!((0..=960).contains(&o));
            prev = o;
        }
        assert_eq!(proportional_offset(-10, 1440, 3.0), 0);
        assert_eq!(proportional_offset(9999, 1440, 3.0), 960);
        assert_eq!(proportional_offset(700, 1440, 1.0), 0);
    }

    #[test]
    fn proportional_keeps_pointer_on_screen_and_in_place() {
        // The magnified pointer position stays on screen for every cursor
        // position and within two desktop pixels (`2 * level` screen pixels)
        // of the real position.
        for l in [1.5f32, 2.0, 4.0, 8.0] {
            for c in 0..1440 {
                let o = proportional_offset(c, 1440, l);
                let shown = (c - o) as f32 * l;
                assert!((0.0..1440.0).contains(&shown), "l={l} c={c} shown={shown}");
                assert!(
                    (shown - c as f32).abs() <= 2.0 * l,
                    "l={l} c={c} shown={shown}"
                );
            }
        }
    }

    #[test]
    fn dock_row_stays_clickable_position() {
        // Dock stepper row at y=395 on 1440 px, 2x: the view offset must keep
        // the row inside the visible part and the shown position near 395.
        let o = proportional_offset(395, 1440, 2.0);
        let shown = (395 - o) as f32 * 2.0;
        assert!((shown - 395.0).abs() <= 4.0);
    }

    #[test]
    fn offset_never_exceeds_range() {
        for l in [1.0f32, 1.1, 1.5, 2.0, 3.0, 8.0] {
            for c in (-10..2000).step_by(37) {
                let o = clamp_offset(c, 1920, l);
                assert!(o >= 0 && o as f32 <= 1920.0 - 1920.0 / l + 0.001);
            }
        }
    }
}
