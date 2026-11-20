//! Plattformspezifische Helfer.

/// Arbeitsbereich (ohne Taskleiste) des Monitors unter dem Mauszeiger.
/// Rueckgabe: (x, y, breite, hoehe, skalierung) in physischen Pixeln.
#[cfg(windows)]
pub fn work_area_for_cursor_monitor() -> Option<(i32, i32, i32, i32, f32)> {
    use windows_sys::Win32::Foundation::POINT;
    use windows_sys::Win32::Graphics::Gdi::{
        GetMonitorInfoW, MonitorFromPoint, MONITORINFO, MONITOR_DEFAULTTONEAREST,
    };
    use windows_sys::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
    use windows_sys::Win32::UI::WindowsAndMessaging::GetCursorPos;

    unsafe {
        let mut pt = POINT { x: 0, y: 0 };
        if GetCursorPos(&mut pt) == 0 {
            return None;
        }
        let mon = MonitorFromPoint(pt, MONITOR_DEFAULTTONEAREST);
        if mon.is_null() {
            return None;
        }
        let mut info: MONITORINFO = std::mem::zeroed();
        info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        if GetMonitorInfoW(mon, &mut info) == 0 {
            return None;
        }
        let (mut dx, mut dy) = (0u32, 0u32);
        let scale = if GetDpiForMonitor(mon, MDT_EFFECTIVE_DPI, &mut dx, &mut dy) == 0 && dx > 0 {
            dx as f32 / 96.0
        } else {
            1.0
        };
        let r = info.rcWork;
        Some((r.left, r.top, r.right - r.left, r.bottom - r.top, scale))
    }
}

#[cfg(not(windows))]
pub fn work_area_for_cursor_monitor() -> Option<(i32, i32, i32, i32, f32)> {
    None
}
