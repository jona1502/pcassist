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
    pub mouse: MouseConfig,
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

/// High-contrast colour presets for the mouse highlight: (name, "#RRGGBB").
pub const MOUSE_COLOR_PRESETS: [(&str, &str); 7] = [
    ("Gelb", "#FFD23F"),
    ("Rot", "#FF4D4D"),
    ("Cyan", "#00E5FF"),
    ("Grün", "#3DFF6B"),
    ("Weiß", "#FFFFFF"),
    ("Schwarz", "#000000"),
    ("Magenta", "#FF3DF2"),
];

pub const MOUSE_SHAPES: [&str; 3] = ["ring", "filled", "crosshair"];
pub const MOUSE_SIZE_RANGE: (u32, u32) = (40, 200);
pub const MOUSE_THICKNESS_RANGE: (u32, u32) = (2, 12);
pub const MOUSE_OPACITY_RANGE: (u32, u32) = (20, 100);

/// Parses "#RRGGBB" (leading `#` required, hex digits any case) into (r, g, b).
pub fn parse_hex_color(s: &str) -> Option<(u8, u8, u8)> {
    let hex = s.strip_prefix('#')?;
    if hex.len() != 6 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let v = u32::from_str_radix(hex, 16).ok()?;
    Some(((v >> 16) as u8, (v >> 8) as u8, v as u8))
}

/// Mouse pointer highlight for low-vision users.
///
/// Every field is validated on load: numbers are clamped to their range,
/// anything unusable (wrong type, unknown shape, bad colour) becomes the default.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct MouseConfig {
    #[serde(deserialize_with = "de_enabled")]
    pub enabled: bool,
    /// "ring", "filled" or "crosshair".
    #[serde(deserialize_with = "de_shape")]
    pub shape: String,
    /// Highlight colour, "#RRGGBB".
    #[serde(deserialize_with = "de_color")]
    pub color: String,
    /// Diameter in logical px, 40..=200.
    #[serde(deserialize_with = "de_size")]
    pub size: u32,
    /// Line thickness in logical px, 2..=12.
    #[serde(deserialize_with = "de_thickness")]
    pub thickness: u32,
    /// Opacity in percent, 20..=100.
    #[serde(deserialize_with = "de_opacity")]
    pub opacity: u32,
    /// Show a short pulse on mouse button press.
    #[serde(deserialize_with = "de_click_highlight")]
    pub click_highlight: bool,
    /// Pulse colour, "#RRGGBB".
    #[serde(deserialize_with = "de_click_color")]
    pub click_color: String,
}

impl Default for MouseConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            shape: "ring".into(),
            color: "#FFD23F".into(),
            size: 64,
            thickness: 4,
            opacity: 80,
            click_highlight: true,
            click_color: "#FF4D4D".into(),
        }
    }
}

impl MouseConfig {
    /// Same rules as on load, for values set at runtime (e.g. by a settings UI).
    pub fn sanitized(&self) -> MouseConfig {
        let d = MouseConfig::default();
        MouseConfig {
            enabled: self.enabled,
            shape: if MOUSE_SHAPES.contains(&self.shape.as_str()) {
                self.shape.clone()
            } else {
                d.shape
            },
            color: valid_color(&self.color, &d.color),
            size: self.size.clamp(MOUSE_SIZE_RANGE.0, MOUSE_SIZE_RANGE.1),
            thickness: self
                .thickness
                .clamp(MOUSE_THICKNESS_RANGE.0, MOUSE_THICKNESS_RANGE.1),
            opacity: self
                .opacity
                .clamp(MOUSE_OPACITY_RANGE.0, MOUSE_OPACITY_RANGE.1),
            click_highlight: self.click_highlight,
            click_color: valid_color(&self.click_color, &d.click_color),
        }
    }
}

/// Normalised upper-case "#RRGGBB", or `default` if `s` is not a valid colour.
fn valid_color(s: &str, default: &str) -> String {
    match parse_hex_color(s) {
        Some((r, g, b)) => format!("#{r:02X}{g:02X}{b:02X}"),
        None => default.to_string(),
    }
}

fn lenient_bool<'de, D: serde::Deserializer<'de>>(d: D, default: bool) -> Result<bool, D::Error> {
    Ok(match toml::Value::deserialize(d)? {
        toml::Value::Boolean(b) => b,
        _ => default,
    })
}

fn lenient_u32<'de, D: serde::Deserializer<'de>>(
    d: D,
    (lo, hi): (u32, u32),
    default: u32,
) -> Result<u32, D::Error> {
    Ok(match toml::Value::deserialize(d)? {
        toml::Value::Integer(i) => i.clamp(lo as i64, hi as i64) as u32,
        _ => default,
    })
}

fn lenient_color<'de, D: serde::Deserializer<'de>>(
    d: D,
    default: &str,
) -> Result<String, D::Error> {
    Ok(match toml::Value::deserialize(d)? {
        toml::Value::String(s) => valid_color(&s, default),
        _ => default.to_string(),
    })
}

fn de_enabled<'de, D: serde::Deserializer<'de>>(d: D) -> Result<bool, D::Error> {
    lenient_bool(d, false)
}

fn de_click_highlight<'de, D: serde::Deserializer<'de>>(d: D) -> Result<bool, D::Error> {
    lenient_bool(d, true)
}

fn de_shape<'de, D: serde::Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    Ok(match toml::Value::deserialize(d)? {
        toml::Value::String(s) if MOUSE_SHAPES.contains(&s.as_str()) => s,
        _ => "ring".to_string(),
    })
}

fn de_color<'de, D: serde::Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    lenient_color(d, "#FFD23F")
}

fn de_click_color<'de, D: serde::Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    lenient_color(d, "#FF4D4D")
}

fn de_size<'de, D: serde::Deserializer<'de>>(d: D) -> Result<u32, D::Error> {
    lenient_u32(d, MOUSE_SIZE_RANGE, 64)
}

fn de_thickness<'de, D: serde::Deserializer<'de>>(d: D) -> Result<u32, D::Error> {
    lenient_u32(d, MOUSE_THICKNESS_RANGE, 4)
}

fn de_opacity<'de, D: serde::Deserializer<'de>>(d: D) -> Result<u32, D::Error> {
    lenient_u32(d, MOUSE_OPACITY_RANGE, 80)
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
    /// Toggle the mouse pointer highlight, e.g. "ctrl+alt+m".
    pub toggle_mouse: String,
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
            toggle_mouse: "ctrl+alt+m".into(),
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

    fn load_str(text: &str) -> Config {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("c.toml");
        std::fs::write(&p, text).unwrap();
        Config::load_from(&p)
    }

    #[test]
    fn old_file_without_mouse_loads() {
        let c = load_str("[panel]\nwidth = 420\n[hotkeys]\ntoggle_panel = \"ctrl+alt+x\"\n");
        assert_eq!(c.mouse, MouseConfig::default());
        assert!(!c.mouse.enabled);
        assert_eq!(c.hotkeys.toggle_mouse, "ctrl+alt+m");
        assert_eq!(c.hotkeys.toggle_panel, "ctrl+alt+x");
    }

    #[test]
    fn mouse_values_load_and_roundtrip() {
        let c = load_str(
            "[mouse]\nenabled = true\nshape = \"crosshair\"\ncolor = \"#00e5ff\"\nsize = 120\nthickness = 8\nopacity = 50\nclick_highlight = false\nclick_color = \"#3DFF6B\"\n",
        );
        assert!(c.mouse.enabled);
        assert_eq!(c.mouse.shape, "crosshair");
        assert_eq!(c.mouse.color, "#00E5FF");
        assert_eq!(
            (c.mouse.size, c.mouse.thickness, c.mouse.opacity),
            (120, 8, 50)
        );
        assert!(!c.mouse.click_highlight);
        assert_eq!(c.mouse.click_color, "#3DFF6B");
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("c.toml");
        c.save_to(&p).unwrap();
        assert_eq!(Config::load_from(&p), c);
    }

    #[test]
    fn mouse_numbers_are_clamped() {
        let c = load_str("[mouse]\nsize = 5\nthickness = 99\nopacity = 0\n");
        assert_eq!(
            (c.mouse.size, c.mouse.thickness, c.mouse.opacity),
            (40, 12, 20)
        );
        let c = load_str("[mouse]\nsize = 9999\nthickness = -3\nopacity = 500\n");
        assert_eq!(
            (c.mouse.size, c.mouse.thickness, c.mouse.opacity),
            (200, 2, 100)
        );
    }

    #[test]
    fn mouse_invalid_values_fall_back_without_losing_the_file() {
        let c = load_str(
            "[panel]\nwidth = 420\n[mouse]\nenabled = \"yes\"\nshape = \"star\"\ncolor = \"red\"\nsize = \"big\"\nthickness = 2.5\nclick_color = \"#12\"\nclick_highlight = 3\n",
        );
        assert_eq!(c.panel.width, 420);
        assert_eq!(c.mouse, MouseConfig::default());
    }

    #[test]
    fn sanitized_fixes_runtime_values() {
        let m = MouseConfig {
            enabled: true,
            shape: "x".into(),
            color: "#abc".into(),
            size: 1,
            thickness: 100,
            opacity: 0,
            click_highlight: false,
            click_color: "#ff4d4d".into(),
        }
        .sanitized();
        assert!(m.enabled && !m.click_highlight);
        assert_eq!(m.shape, "ring");
        assert_eq!(m.color, "#FFD23F");
        assert_eq!((m.size, m.thickness, m.opacity), (40, 12, 20));
        assert_eq!(m.click_color, "#FF4D4D");
        assert_eq!(MouseConfig::default().sanitized(), MouseConfig::default());
    }

    #[test]
    fn hex_parsing() {
        assert_eq!(parse_hex_color("#FFD23F"), Some((255, 210, 63)));
        assert_eq!(parse_hex_color("#00e5ff"), Some((0, 229, 255)));
        assert_eq!(parse_hex_color("#000000"), Some((0, 0, 0)));
        for bad in [
            "", "FFD23F", "#FFD23", "#FFD23F0", "#GGGGGG", "# FFD23", "#+FD23F",
        ] {
            assert_eq!(parse_hex_color(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn presets_are_valid_and_distinct() {
        for (i, (_, hex)) in MOUSE_COLOR_PRESETS.iter().enumerate() {
            assert!(parse_hex_color(hex).is_some(), "{hex}");
            for (_, other) in &MOUSE_COLOR_PRESETS[i + 1..] {
                assert_ne!(hex, other);
            }
        }
        assert_eq!(MOUSE_COLOR_PRESETS[0].1, MouseConfig::default().color);
    }
}
