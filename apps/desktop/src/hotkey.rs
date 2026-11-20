//! Global hotkey registration. Ctrl+Alt+<key> combos are the defaults: rarely
//! bound by Windows or common apps (unlike Ctrl+Alt+Arrow/Del or Win+letter
//! combos). Windows' own Win+Plus / Win+Minus magnifier shortcuts are not touched.

use assistant_config::HotkeyConfig;
use assistant_core::Command;
use global_hotkey::hotkey::HotKey;
use global_hotkey::GlobalHotKeyManager;
use std::str::FromStr;

/// Parses the configured string; invalid input falls back to `default`.
pub fn parse(s: &str, default: &str) -> HotKey {
    HotKey::from_str(s).unwrap_or_else(|e| {
        eprintln!("pcassist: invalid hotkey {s:?} ({e}), using {default}");
        HotKey::from_str(default).expect("default hotkey is valid")
    })
}

/// Registered hotkeys. The manager must stay alive for them to remain active.
pub struct Hotkeys {
    _manager: Option<GlobalHotKeyManager>,
    bindings: Vec<(u32, Command)>,
}

impl Hotkeys {
    /// Command bound to the hotkey with this id, if any.
    pub fn command_for(&self, id: u32) -> Option<Command> {
        self.bindings
            .iter()
            .find_map(|&(i, c)| (i == id).then_some(c))
    }
}

/// Registers all configured hotkeys. A failing hotkey only logs a warning
/// (e.g. already taken); the others stay active.
/// Must be called on the main thread after the event loop backend exists.
pub fn register(cfg: &HotkeyConfig) -> Hotkeys {
    let defaults = HotkeyConfig::default();
    let wanted = [
        (
            &cfg.toggle_panel,
            &defaults.toggle_panel,
            Command::TogglePanel,
        ),
        (&cfg.zoom_in, &defaults.zoom_in, Command::ZoomIn),
        (&cfg.zoom_out, &defaults.zoom_out, Command::ZoomOut),
        (&cfg.zoom_reset, &defaults.zoom_reset, Command::ZoomReset),
        (
            &cfg.speak_selection,
            &defaults.speak_selection,
            Command::SpeakSelection,
        ),
        (&cfg.speak_stop, &defaults.speak_stop, Command::SpeakStop),
    ];
    let manager = match GlobalHotKeyManager::new() {
        Ok(m) => m,
        Err(e) => {
            eprintln!("pcassist: global hotkey unavailable: {e}");
            return Hotkeys {
                _manager: None,
                bindings: Vec::new(),
            };
        }
    };
    let mut bindings = Vec::new();
    for (s, default, cmd) in wanted {
        let hotkey = parse(s, default);
        match manager.register(hotkey) {
            Ok(()) => bindings.push((hotkey.id(), cmd)),
            Err(e) => eprintln!("pcassist: could not register hotkey {s:?}: {e}"),
        }
    }
    Hotkeys {
        _manager: Some(manager),
        bindings,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DEFAULT: &str = "ctrl+alt+p";

    #[test]
    fn invalid_falls_back() {
        assert_eq!(parse("nonsense+++", DEFAULT), parse(DEFAULT, DEFAULT));
        assert_ne!(parse("ctrl+alt+o", DEFAULT), parse(DEFAULT, DEFAULT));
    }

    #[test]
    fn default_zoom_hotkeys_parse_and_differ() {
        let d = HotkeyConfig::default();
        let ids: Vec<u32> = [
            &d.toggle_panel,
            &d.zoom_in,
            &d.zoom_out,
            &d.zoom_reset,
            &d.speak_selection,
            &d.speak_stop,
        ]
        .iter()
        .map(|s| HotKey::from_str(s).expect("default parses").id())
        .collect();
        for (i, a) in ids.iter().enumerate() {
            for b in &ids[i + 1..] {
                assert_ne!(a, b);
            }
        }
    }
}
