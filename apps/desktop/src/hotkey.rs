//! Global hotkey registration. Ctrl+Alt+<key> combos are the defaults: rarely
//! bound by Windows or common apps (unlike Ctrl+Alt+Arrow/Del or Win+letter
//! combos). Windows' own Win+Plus / Win+Minus magnifier shortcuts are not touched.

use assistant_config::HotkeyConfig;
use assistant_core::{Command, Feature};
use global_hotkey::hotkey::HotKey;
use global_hotkey::GlobalHotKeyManager;
use std::cell::RefCell;
use std::str::FromStr;

/// Parses the configured string; invalid input falls back to `default`.
pub fn parse(s: &str, default: &str) -> HotKey {
    HotKey::from_str(s).unwrap_or_else(|e| {
        eprintln!("pcassist: invalid hotkey {s:?} ({e}), using {default}");
        HotKey::from_str(default).expect("default hotkey is valid")
    })
}

/// True if `s` is a hotkey string the manager can parse, e.g. "ctrl+alt+p".
pub fn is_valid(s: &str) -> bool {
    HotKey::from_str(s).is_ok()
}

/// Registered hotkeys. The manager must stay alive for them to remain active.
pub struct Hotkeys {
    manager: Option<GlobalHotKeyManager>,
    bindings: Vec<(HotKey, Command)>,
}

impl Hotkeys {
    /// Command bound to the hotkey with this id, if any.
    pub fn command_for(&self, id: u32) -> Option<Command> {
        self.bindings
            .iter()
            .find_map(|&(k, c)| (k.id() == id).then_some(c))
    }

    /// Replaces all registrations with the ones from `cfg` (after saving settings).
    pub fn reregister(&mut self, cfg: &HotkeyConfig) {
        let Some(manager) = &self.manager else { return };
        for (k, _) in self.bindings.drain(..) {
            let _ = manager.unregister(k);
        }
        self.bindings = bind(manager, cfg);
    }
}

thread_local! {
    static HOTKEYS: RefCell<Option<Hotkeys>> = const { RefCell::new(None) };
}

/// Keeps the registered hotkeys alive (main thread) and makes them reachable
/// for `command_for` and `reregister_all`.
pub fn install(h: Hotkeys) {
    HOTKEYS.with_borrow_mut(|slot| *slot = Some(h));
}

/// Command for a pressed hotkey id.
pub fn command_for(id: u32) -> Option<Command> {
    HOTKEYS.with_borrow(|h| h.as_ref().and_then(|h| h.command_for(id)))
}

/// Re-registers all hotkeys from the config.
pub fn reregister_all(cfg: &HotkeyConfig) {
    HOTKEYS.with_borrow_mut(|h| {
        if let Some(h) = h {
            h.reregister(cfg);
        }
    });
}

/// Registers all configured hotkeys. A failing hotkey only logs a warning
/// (e.g. already taken); the others stay active.
/// Must be called on the main thread after the event loop backend exists.
pub fn register(cfg: &HotkeyConfig) -> Hotkeys {
    let manager = match GlobalHotKeyManager::new() {
        Ok(m) => m,
        Err(e) => {
            eprintln!("pcassist: global hotkey unavailable: {e}");
            return Hotkeys {
                manager: None,
                bindings: Vec::new(),
            };
        }
    };
    let bindings = bind(&manager, cfg);
    Hotkeys {
        manager: Some(manager),
        bindings,
    }
}

fn bind(manager: &GlobalHotKeyManager, cfg: &HotkeyConfig) -> Vec<(HotKey, Command)> {
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
        (
            &cfg.toggle_mouse,
            &defaults.toggle_mouse,
            Command::ToggleFeature(Feature::MouseHighlight),
        ),
    ];
    let mut bindings = Vec::new();
    for (s, default, cmd) in wanted {
        let hotkey = parse(s, default);
        match manager.register(hotkey) {
            Ok(()) => bindings.push((hotkey, cmd)),
            Err(e) => eprintln!("pcassist: could not register hotkey {s:?}: {e}"),
        }
    }
    bindings
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
    fn validity() {
        assert!(is_valid("ctrl+alt+p"));
        assert!(is_valid("ctrl+alt+Equal"));
        assert!(!is_valid("nonsense+++"));
        assert!(!is_valid(""));
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
            &d.toggle_mouse,
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
