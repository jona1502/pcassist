//! Persistent settings, stored as TOML in %APPDATA%\pcassist\config.toml.

use serde::{Deserialize, Serialize};
use std::io;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub panel: PanelConfig,
    pub hotkeys: HotkeyConfig,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct HotkeyConfig {
    /// Global hotkey toggling the panel, e.g. "ctrl+alt+p".
    pub toggle_panel: String,
}

impl Default for HotkeyConfig {
    fn default() -> Self {
        Self {
            toggle_panel: "ctrl+alt+p".into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct PanelConfig {
    pub width: u32,
    pub always_on_top: bool,
    pub preferred_monitor: Option<String>,
}

impl Default for PanelConfig {
    fn default() -> Self {
        Self {
            width: 360,
            always_on_top: true,
            preferred_monitor: None,
        }
    }
}

impl Config {
    /// %APPDATA%\pcassist\config.toml, falls back to the current dir.
    pub fn path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_default()
            .join("pcassist")
            .join("config.toml")
    }

    /// Missing or corrupt file yields defaults; corrupt file is kept as `.bak`.
    pub fn load() -> Config {
        Self::load_from(&Self::path())
    }

    pub fn save(&self) -> io::Result<()> {
        self.save_to(&Self::path())
    }

    pub fn load_from(path: &Path) -> Config {
        let Ok(text) = std::fs::read_to_string(path) else {
            return Config::default();
        };
        match toml::from_str(&text) {
            Ok(cfg) => cfg,
            Err(_) => {
                let _ = std::fs::copy(path, path.with_extension("toml.bak"));
                Config::default()
            }
        }
    }

    pub fn save_to(&self, path: &Path) -> io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let text = toml::to_string_pretty(self).map_err(io::Error::other)?;
        let tmp = path.with_extension("toml.tmp");
        std::fs::write(&tmp, text)?;
        std::fs::rename(&tmp, path).inspect_err(|_| {
            let _ = std::fs::remove_file(&tmp);
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_file_gives_defaults() {
        let d = tempfile::tempdir().unwrap();
        let c = Config::load_from(&d.path().join("nope.toml"));
        assert_eq!(c.panel.width, 360);
        assert!(c.panel.always_on_top);
        assert_eq!(c.panel.preferred_monitor, None);
    }

    #[test]
    fn roundtrip() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("sub").join("config.toml");
        let mut c = Config::default();
        c.panel.width = 500;
        c.panel.always_on_top = false;
        c.panel.preferred_monitor = Some("DP-1".into());
        c.save_to(&p).unwrap();
        assert_eq!(Config::load_from(&p), c);
    }

    #[test]
    fn partial_file() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("c.toml");
        std::fs::write(&p, "[panel]\nwidth = 420\n").unwrap();
        let c = Config::load_from(&p);
        assert_eq!(c.panel.width, 420);
        assert!(c.panel.always_on_top);
        std::fs::write(&p, "").unwrap();
        assert_eq!(Config::load_from(&p), Config::default());
    }

    #[test]
    fn old_file_without_hotkeys_loads() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("c.toml");
        std::fs::write(&p, "[panel]
width = 420
").unwrap();
        let c = Config::load_from(&p);
        assert_eq!(c.hotkeys.toggle_panel, "ctrl+alt+p");
        assert_eq!(c.panel.width, 420);
    }

    #[test]
    fn corrupt_file_gives_defaults_and_backup() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("c.toml");
        std::fs::write(&p, "[panel\nwidth = ???").unwrap();
        assert_eq!(Config::load_from(&p), Config::default());
        assert!(d.path().join("c.toml.bak").exists());
    }
}
