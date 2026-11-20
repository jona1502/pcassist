//! Captures the selected text of the foreground app by simulating Ctrl+C.
//! Privacy: clipboard contents are never logged. Blocking; call off the UI thread.

use std::time::{Duration, Instant};
use windows_sys::Win32::Foundation::{GlobalFree, HANDLE};
use windows_sys::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardData, GetClipboardSequenceNumber, OpenClipboard,
    SetClipboardData,
};
use windows_sys::Win32::System::Memory::{
    GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock, GMEM_MOVEABLE,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP,
    VK_CONTROL, VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT,
};

/// Clipboard format CF_UNICODETEXT.
const CF_UNICODETEXT: u32 = 13;
const VK_C: u16 = 0x43;

fn key_down(vk: u16) -> bool {
    // SAFETY: plain query call.
    unsafe { GetAsyncKeyState(vk as i32) < 0 }
}

/// Waits (max ~500 ms) until the user released Ctrl/Alt/Shift/Win from the hotkey.
fn wait_modifiers_released() {
    let deadline = Instant::now() + Duration::from_millis(500);
    let mods = [VK_CONTROL, VK_MENU, VK_SHIFT, VK_LWIN, VK_RWIN];
    while Instant::now() < deadline && mods.iter().any(|&m| key_down(m)) {
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn open_clipboard() -> bool {
    for _ in 0..10 {
        // SAFETY: null owner window associates the clipboard with this task.
        if unsafe { OpenClipboard(std::ptr::null_mut()) } != 0 {
            return true;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    false
}

/// Current clipboard text, if any.
fn read_text() -> Option<String> {
    if !open_clipboard() {
        return None;
    }
    // SAFETY: clipboard is open; the locked memory is read within its size and
    // unlocked before closing.
    let text = unsafe {
        let h = GetClipboardData(CF_UNICODETEXT);
        if h.is_null() {
            None
        } else {
            let p = GlobalLock(h) as *const u16;
            if p.is_null() {
                None
            } else {
                let max = GlobalSize(h) / 2;
                let mut len = 0;
                while len < max && *p.add(len) != 0 {
                    len += 1;
                }
                let s = String::from_utf16_lossy(std::slice::from_raw_parts(p, len));
                GlobalUnlock(h);
                Some(s)
            }
        }
    };
    unsafe { CloseClipboard() };
    text
}

fn write_text(text: &str) {
    let wide: Vec<u16> = text.encode_utf16().chain(Some(0)).collect();
    if !open_clipboard() {
        return;
    }
    // SAFETY: allocation is filled before ownership passes to the clipboard;
    // freed by us only if SetClipboardData fails.
    unsafe {
        EmptyClipboard();
        let h = GlobalAlloc(GMEM_MOVEABLE, wide.len() * 2);
        if !h.is_null() {
            let p = GlobalLock(h) as *mut u16;
            if p.is_null() {
                GlobalFree(h);
            } else {
                std::ptr::copy_nonoverlapping(wide.as_ptr(), p, wide.len());
                GlobalUnlock(h);
                if SetClipboardData(CF_UNICODETEXT, h as HANDLE).is_null() {
                    GlobalFree(h);
                }
            }
        }
        CloseClipboard();
    }
}

fn send_ctrl_c() {
    let key = |vk: u16, flags| INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                wScan: 0,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    };
    let inputs = [
        key(VK_CONTROL, 0),
        key(VK_C, 0),
        key(VK_C, KEYEVENTF_KEYUP),
        key(VK_CONTROL, KEYEVENTF_KEYUP),
    ];
    // SAFETY: array of fully initialised INPUTs with the correct size.
    unsafe {
        SendInput(
            inputs.len() as u32,
            inputs.as_ptr(),
            std::mem::size_of::<INPUT>() as i32,
        );
    }
}

/// Copies the foreground selection via Ctrl+C and returns it; restores the
/// previous clipboard text. `None` if nothing was copied.
pub fn capture_selection() -> Option<String> {
    let previous = read_text();
    wait_modifiers_released();
    // SAFETY: plain query call.
    let seq = unsafe { GetClipboardSequenceNumber() };
    send_ctrl_c();
    let deadline = Instant::now() + Duration::from_millis(300);
    let mut changed = false;
    while Instant::now() < deadline {
        if unsafe { GetClipboardSequenceNumber() } != seq {
            changed = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    if !changed {
        return None;
    }
    let copied = read_text();
    if let Some(prev) = previous {
        write_text(&prev);
    }
    copied.filter(|t| !t.trim().is_empty())
}

/// Runs `capture_selection` on a worker thread; the result arrives on the
/// returned channel (drained by the UI timer).
pub fn capture_async() -> std::sync::mpsc::Receiver<Option<String>> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(capture_selection());
    });
    rx
}
