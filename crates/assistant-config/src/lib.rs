//! Persistent settings, stored as TOML in %APPDATA%\pcassist\config.toml.

use serde::{Deserialize, Serialize};
use std::io;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub panel: PanelConfig,
    pub hotkeys: HotkeyConfig,
    pub tts: TtsConfig,
    pub zoom: ZoomConfig,
    pub audio: AudioConfig,
}

/// Audio devices by name; `None` = system default.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct AudioConfig {
    /// Microphone, reserved for the future voice control.
    pub input_device: Option<String>,
    /// Speech output device.
    pub output_device: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ZoomConfig {
    /// Zoom step in percent: 100, 50 or 10. Anything else falls back to 100.
    #[serde(deserialize_with = "valid_step")]
    pub step: u32,
}

impl Default for ZoomConfig {
    fn default() -> Self {
        Self { step: 100 }
    }
}

fn valid_step<'de, D: serde::Deserializer<'de>>(d: D) -> Result<u32, D::Error> {
    let v = i64::deserialize(d)?;
    Ok(match v {
        100 | 50 | 10 => v as u32,
        _ => 100,
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct TtsConfig {
    /// Speech rate -10..=10, 0 = normal.
    pub rate: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct HotkeyConfig {
    /// Global hotkey toggling the panel, e.g. "ctrl+alt+p".
    pub toggle_panel: String,
    /// Zoom in one step, e.g. "ctrl+alt+Equal".
    pub zoom_in: String,
    /// Zoom out one step, e.g. "ctrl+alt+Minus".
    pub zoom_out: String,
    /// Reset zoom to 100 %, e.g. "ctrl+alt+0".
    pub zoom_reset: String,
    /// Read the selected text aloud, e.g. "ctrl+alt+r".
    pub speak_selection: String,
    /// Stop speech, e.g. "ctrl+alt+s".
    pub speak_stop: String,
}

impl Default for HotkeyConfig {
    fn default() -> Self {
        Self {
            toggle_panel: "ctrl+alt+p".into(),
            zoom_in: "ctrl+alt+Equal".into(),
            zoom_out: "ctrl+alt+Minus".into(),
            zoom_reset: "ctrl+alt+0".into(),
            speak_selection: "ctrl+alt+r".into(),
            speak_stop: "ctrl+alt+s".into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct PanelConfig {
    /// Ignoriert: das Panel ist eine schmale Dock-Leiste mit fester Breite.
    /// Feld bleibt, damit alte config.toml-Dateien weiter laden.
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
        std::fs::write(
            &p,
            "[panel]
width = 420
",
        )
        .unwrap();
        let c = Config::load_from(&p);
        assert_eq!(c.hotkeys.toggle_panel, "ctrl+alt+p");
        assert_eq!(c.hotkeys.zoom_in, "ctrl+alt+Equal");
        assert_eq!(c.hotkeys.zoom_out, "ctrl+alt+Minus");
        assert_eq!(c.hotkeys.zoom_reset, "ctrl+alt+0");
        assert_eq!(c.hotkeys.speak_selection, "ctrl+alt+r");
        assert_eq!(c.hotkeys.speak_stop, "ctrl+alt+s");
        assert_eq!(c.tts.rate, 0);
        assert_eq!(c.panel.width, 420);
    }

    #[test]
    fn tts_rate_loads() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("c.toml");
        std::fs::write(&p, "[tts]\nrate = 4\n").unwrap();
        assert_eq!(Config::load_from(&p).tts.rate, 4);
    }

    #[test]
    fn corrupt_file_gives_defaults_and_backup() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("c.toml");
        std::fs::write(&p, "[panel\nwidth = ???").unwrap();
        assert_eq!(Config::load_from(&p), Config::default());
        assert!(d.path().join("c.toml.bak").exists());
    }

    #[test]
    fn zoom_step_loads_and_falls_back() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("c.toml");
        std::fs::write(
            &p,
            "[panel]
width = 420
",
        )
        .unwrap();
        assert_eq!(Config::load_from(&p).zoom.step, 100);
        std::fs::write(
            &p,
            "[zoom]
step = 50
",
        )
        .unwrap();
        assert_eq!(Config::load_from(&p).zoom.step, 50);
        std::fs::write(
            &p,
            "[zoom]
step = 10
",
        )
        .unwrap();
        assert_eq!(Config::load_from(&p).zoom.step, 10);
        std::fs::write(
            &p,
            "[zoom]
step = 33
",
        )
        .unwrap();
        assert_eq!(Config::load_from(&p).zoom.step, 100);
        std::fs::write(
            &p,
            "[zoom]
step = -5
",
        )
        .unwrap();
        assert_eq!(Config::load_from(&p).zoom.step, 100);
    }

    #[test]
    fn old_file_without_audio_loads() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("c.toml");
        std::fs::write(
            &p,
            "[tts]
rate = 2
",
        )
        .unwrap();
        let c = Config::load_from(&p);
        assert_eq!(c.audio, AudioConfig::default());
        assert_eq!(c.audio.input_device, None);
        assert_eq!(c.audio.output_device, None);
    }

    #[test]
    fn audio_roundtrip() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("c.toml");
        let mut c = Config::default();
        c.audio.input_device = Some("Mikrofon (USB)".into());
        c.audio.output_device = Some("Lautsprecher".into());
        c.save_to(&p).unwrap();
        assert_eq!(Config::load_from(&p), c);
    }
}
