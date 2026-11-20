//! Global hotkey registration. Ctrl+Alt+P is the default: rarely bound by
//! Windows or common apps (unlike Ctrl+Alt+Arrow/Del or Win+letter combos).

use global_hotkey::hotkey::HotKey;
use global_hotkey::GlobalHotKeyManager;
use std::str::FromStr;

const DEFAULT: &str = "ctrl+alt+p";

/// Parses the configured string; invalid input falls back to the default.
pub fn parse(s: &str) -> HotKey {
    HotKey::from_str(s).unwrap_or_else(|e| {
        eprintln!("pcassist: invalid hotkey {s:?} ({e}), using {DEFAULT}");
        HotKey::from_str(DEFAULT).expect("default hotkey is valid")
    })
}

/// Registers the hotkey. Returns the manager (must be kept alive) and the
/// hotkey id, or None if registration failed (e.g. already taken).
/// Must be called on the main thread after the event loop backend exists.
pub fn register(s: &str) -> Option<(GlobalHotKeyManager, u32)> {
    let hotkey = parse(s);
    let manager = match GlobalHotKeyManager::new() {
        Ok(m) => m,
        Err(e) => {
            eprintln!("pcassist: global hotkey unavailable: {e}");
            return None;
        }
    };
    if let Err(e) = manager.register(hotkey) {
        eprintln!("pcassist: could not register hotkey {s:?}: {e}");
        return None;
    }
    Some((manager, hotkey.id()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_falls_back() {
        assert_eq!(parse("nonsense+++"), parse(DEFAULT));
        assert_ne!(parse("ctrl+alt+o"), parse(DEFAULT));
    }
}
