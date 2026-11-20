//! Settings window: created lazily, filled from the current config on every
//! open, applied and saved on "Speichern". Nothing sensitive is logged.

use crate::{PanelWindow, SettingsWindow};
use assistant_config::{Config, HotkeyConfig};
use global_hotkey::hotkey::HotKey;
use slint::{
    CloseRequestResponse, ComponentHandle, ModelRc, PhysicalPosition, PhysicalSize, SharedString,
    VecModel,
};
use std::cell::RefCell;
use std::str::FromStr;

/// First entry of every device list; stored as `None` in the config.
pub const DEFAULT_DEVICE: &str = "Standardgerät";
/// Window size in logical pixels.
const WIDTH: f32 = 720.0;
const HEIGHT: f32 = 560.0;

const OUTPUT_NOTE: &str =
    "Gilt für das Vorlesen. Wird das Gerät nicht gefunden, nutzt Windows das Standardgerät.";

thread_local! {
    /// Last saved (or loaded) config; source of truth for the whole app.
    static CURRENT: RefCell<Config> = RefCell::new(Config::default());
    static WINDOW: RefCell<Option<SettingsWindow>> = const { RefCell::new(None) };
}

/// Remembers the loaded config. Call once at startup.
pub fn init(config: Config) {
    set_current(config);
}

pub fn current() -> Config {
    CURRENT.with_borrow(Config::clone)
}

pub fn set_current(config: Config) {
    CURRENT.with_borrow_mut(|c| *c = config);
}

/// Device list for a combo box: default entry first; a configured device that
/// is currently not present stays selectable so saving does not drop it.
fn device_choices(devices: Vec<String>, selected: Option<&str>) -> Vec<String> {
    let mut out = vec![DEFAULT_DEVICE.to_string()];
    out.extend(devices.into_iter().filter(|d| d != DEFAULT_DEVICE));
    if let Some(s) = selected {
        if !out.iter().any(|d| d == s) {
            out.push(s.to_string());
        }
    }
    out
}

fn choice_to_option(choice: &str) -> Option<String> {
    let c = choice.trim();
    (!c.is_empty() && c != DEFAULT_DEVICE).then(|| c.to_string())
}

fn choice_from_option(o: Option<&str>) -> String {
    o.unwrap_or(DEFAULT_DEVICE).to_string()
}

/// German names of the hotkey fields, in `hotkey_fields` order.
const HOTKEY_LABELS: [&str; 7] = [
    "Panel ein/aus",
    "Zoom vergrößern",
    "Zoom verkleinern",
    "Zoom zurücksetzen",
    "Vorlesen starten",
    "Vorlesen stoppen",
    "Mausmarker umschalten",
];

fn hotkey_fields(h: &HotkeyConfig) -> [String; 7] {
    [
        h.toggle_panel.clone(),
        h.zoom_in.clone(),
        h.zoom_out.clone(),
        h.zoom_reset.clone(),
        h.speak_selection.clone(),
        h.speak_stop.clone(),
        h.toggle_mouse.clone(),
    ]
}

fn hotkey_from_fields(f: [String; 7]) -> HotkeyConfig {
    let [toggle_panel, zoom_in, zoom_out, zoom_reset, speak_selection, speak_stop, toggle_mouse] =
        f;
    HotkeyConfig {
        toggle_panel,
        zoom_in,
        zoom_out,
        zoom_reset,
        speak_selection,
        speak_stop,
        toggle_mouse,
    }
}

/// Takes the edited hotkeys over; invalid or duplicate ones keep the old value.
/// Returns the result and the labels of the rejected fields.
fn merge_hotkeys(old: &HotkeyConfig, edited: [String; 7]) -> (HotkeyConfig, Vec<&'static str>) {
    let old_fields = hotkey_fields(old);
    let mut result: Vec<String> = Vec::new();
    let mut ids: Vec<u32> = Vec::new();
    let mut rejected = Vec::new();
    for (i, text) in edited.iter().enumerate() {
        let text = text.trim();
        let parsed = HotKey::from_str(text).ok().map(|k| k.id());
        match parsed {
            Some(id) if !ids.contains(&id) => {
                ids.push(id);
                result.push(text.to_string());
            }
            _ => {
                rejected.push(HOTKEY_LABELS[i]);
                if let Ok(k) = HotKey::from_str(&old_fields[i]) {
                    ids.push(k.id());
                }
                result.push(old_fields[i].clone());
            }
        }
    }
    let fields: [String; 7] = result.try_into().expect("seven hotkey fields");
    (hotkey_from_fields(fields), rejected)
}

fn models(list: Vec<String>) -> ModelRc<SharedString> {
    ModelRc::new(VecModel::from(
        list.into_iter().map(SharedString::from).collect::<Vec<_>>(),
    ))
}

/// Shows the settings window (created on first use), centered, with current values.
pub fn open(panel: &PanelWindow) {
    // The strong handle stays in WINDOW (keeps the window alive); work on a
    // weak upgrade so no borrow is held while Slint runs.
    let existing = WINDOW.with_borrow(|w| w.as_ref().map(SettingsWindow::as_weak));
    let win = match existing.and_then(|w| w.upgrade()) {
        Some(w) => w,
        None => match build(panel) {
            Ok(w) => {
                let weak = w.as_weak();
                WINDOW.with_borrow_mut(|slot| *slot = Some(w));
                match weak.upgrade() {
                    Some(w) => w,
                    None => return,
                }
            }
            Err(e) => {
                eprintln!("pcassist: settings window failed: {e}");
                return;
            }
        },
    };
    populate(&win, &current());
    // Re-showing a visible window raises it reliably only after a hide.
    if win.window().is_visible() {
        let _ = win.hide();
    }
    center(&win);
    if let Err(e) = win.show() {
        eprintln!("pcassist: settings window show failed: {e}");
        return;
    }
    win.invoke_focus_window();
}

fn center(win: &SettingsWindow) {
    let Some((x, y, w, h, scale)) = crate::platform::work_area_for_cursor_monitor() else {
        return;
    };
    let width = ((WIDTH * scale).round() as i32).min(w).max(1);
    let height = ((HEIGHT * scale).round() as i32).min(h).max(1);
    let window = win.window();
    window.set_size(PhysicalSize::new(width as u32, height as u32));
    window.set_position(PhysicalPosition::new(
        x + (w - width) / 2,
        y + (h - height) / 2,
    ));
}

fn populate(win: &SettingsWindow, cfg: &Config) {
    win.set_zoom_step(cfg.zoom.step as i32);
    win.set_tts_rate(cfg.tts.rate as f32);
    let input = cfg.audio.input_device.as_deref();
    let output = cfg.audio.output_device.as_deref();
    win.set_input_devices(models(device_choices(
        assistant_audio::list_input_devices(),
        input,
    )));
    win.set_output_devices(models(device_choices(
        assistant_audio::list_output_devices(),
        output,
    )));
    win.set_input_device(choice_from_option(input).into());
    win.set_output_device(choice_from_option(output).into());
    win.set_output_note(OUTPUT_NOTE.into());
    let [toggle, zin, zout, zreset, speak, stop, mouse] = hotkey_fields(&cfg.hotkeys);
    win.set_hk_toggle(toggle.into());
    win.set_hk_zoom_in(zin.into());
    win.set_hk_zoom_out(zout.into());
    win.set_hk_zoom_reset(zreset.into());
    win.set_hk_speak(speak.into());
    win.set_hk_stop(stop.into());
    win.set_hk_mouse(mouse.into());
    win.set_always_on_top_setting(cfg.panel.always_on_top);
    win.set_status_text("".into());
}

fn build(panel: &PanelWindow) -> Result<SettingsWindow, slint::PlatformError> {
    let win = SettingsWindow::new()?;
    win.window().on_close_requested(|| {
        restore_tts();
        CloseRequestResponse::HideWindow
    });
    win.on_validate_hotkey(|s| crate::hotkey::is_valid(s.trim()));

    let weak = win.as_weak();
    win.on_close_requested(move || {
        restore_tts();
        if let Some(w) = weak.upgrade() {
            let _ = w.hide();
        }
    });

    let weak = win.as_weak();
    win.on_test_speak(move || {
        let Some(w) = weak.upgrade() else { return };
        let ok = crate::app::speak_test(
            w.get_tts_rate().round() as i32,
            choice_to_option(w.get_output_device().as_str()).as_deref(),
        );
        w.set_status_text(if ok { "" } else { "Vorlesen nicht verfügbar" }.into());
    });

    let weak = win.as_weak();
    let weak_panel = panel.as_weak();
    win.on_save(move || {
        if let (Some(w), Some(p)) = (weak.upgrade(), weak_panel.upgrade()) {
            save(&w, &p);
        }
    });
    Ok(win)
}

/// Unsaved test rate and device must not outlive the window.
fn restore_tts() {
    let cfg = current();
    crate::app::set_tts_rate(cfg.tts.rate);
    crate::app::set_tts_output(cfg.audio.output_device.as_deref());
}

fn save(win: &SettingsWindow, panel: &PanelWindow) {
    let mut cfg = current();
    let step = u32::try_from(win.get_zoom_step()).unwrap_or(0);
    if assistant_zoom::ZoomStep::from_percent(step).is_some() {
        cfg.zoom.step = step;
    }
    cfg.tts.rate = assistant_tts::clamp_rate(win.get_tts_rate().round() as i32);
    cfg.audio.input_device = choice_to_option(win.get_input_device().as_str());
    cfg.audio.output_device = choice_to_option(win.get_output_device().as_str());
    cfg.panel.always_on_top = win.get_always_on_top_setting();
    let (hotkeys, rejected) = merge_hotkeys(
        &cfg.hotkeys,
        [
            win.get_hk_toggle().to_string(),
            win.get_hk_zoom_in().to_string(),
            win.get_hk_zoom_out().to_string(),
            win.get_hk_zoom_reset().to_string(),
            win.get_hk_speak().to_string(),
            win.get_hk_stop().to_string(),
            win.get_hk_mouse().to_string(),
        ],
    );
    cfg.hotkeys = hotkeys;

    if let Err(e) = cfg.save() {
        eprintln!("config save failed: {e}");
        win.set_status_text("Speichern fehlgeschlagen".into());
        return;
    }
    set_current(cfg.clone());
    crate::app::apply_config(&cfg, panel);
    crate::hotkey::reregister_all(&cfg.hotkeys);
    win.set_status_text(
        if rejected.is_empty() {
            "Gespeichert".to_string()
        } else {
            format!(
                "Gespeichert, aber ungültige oder doppelte Tastenkürzel nicht übernommen: {}",
                rejected.join(", ")
            )
        }
        .into(),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn choices_start_with_default_and_keep_missing_selection() {
        let v = device_choices(vec!["A".into(), "B".into()], Some("Weg"));
        assert_eq!(v, ["Standardgerät", "A", "B", "Weg"]);
        let v = device_choices(vec!["A".into()], Some("A"));
        assert_eq!(v, ["Standardgerät", "A"]);
        assert_eq!(device_choices(vec![], None), ["Standardgerät"]);
    }

    #[test]
    fn choice_mapping() {
        assert_eq!(choice_to_option("Standardgerät"), None);
        assert_eq!(choice_to_option(""), None);
        assert_eq!(choice_to_option(" Mic "), Some("Mic".into()));
        assert_eq!(choice_from_option(None), "Standardgerät");
        assert_eq!(choice_from_option(Some("Mic")), "Mic");
    }

    #[test]
    fn valid_hotkeys_are_taken() {
        let old = HotkeyConfig::default();
        let mut f = hotkey_fields(&old);
        f[0] = " ctrl+alt+o ".into();
        let (h, rej) = merge_hotkeys(&old, f);
        assert_eq!(h.toggle_panel, "ctrl+alt+o");
        assert!(rej.is_empty());
    }

    #[test]
    fn invalid_hotkey_keeps_old_value() {
        let old = HotkeyConfig::default();
        let mut f = hotkey_fields(&old);
        f[1] = "nonsense+++".into();
        let (h, rej) = merge_hotkeys(&old, f);
        assert_eq!(h.zoom_in, old.zoom_in);
        assert_eq!(rej, ["Zoom vergrößern"]);
    }

    #[test]
    fn duplicate_hotkey_is_rejected() {
        let old = HotkeyConfig::default();
        let mut f = hotkey_fields(&old);
        f[1] = old.toggle_panel.clone();
        let (h, rej) = merge_hotkeys(&old, f);
        assert_eq!(h.zoom_in, old.zoom_in);
        assert_eq!(rej, ["Zoom vergrößern"]);
    }
}
