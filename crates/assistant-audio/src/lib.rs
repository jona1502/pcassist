//! Audio device listing via cpal (default host, WASAPI on Windows).
//! Only device names are exposed; errors yield an empty list, never a panic.

use cpal::traits::{DeviceTrait, HostTrait};

/// Names of all input devices (microphones). Empty on error or none present.
pub fn list_input_devices() -> Vec<String> {
    collect(|h| h.input_devices().map(|d| names(d.collect())))
}

/// Names of all output devices (speakers). Empty on error or none present.
pub fn list_output_devices() -> Vec<String> {
    collect(|h| h.output_devices().map(|d| names(d.collect())))
}

fn collect<E>(
    f: impl FnOnce(&cpal::Host) -> Result<Vec<String>, E> + std::panic::UnwindSafe,
) -> Vec<String> {
    // Audio drivers occasionally misbehave; a listing failure must never take
    // the app down.
    std::panic::catch_unwind(|| f(&cpal::default_host()))
        .ok()
        .and_then(Result::ok)
        .map(dedup)
        .unwrap_or_default()
}

fn names(devices: Vec<cpal::Device>) -> Vec<String> {
    devices.iter().filter_map(|d| d.name().ok()).collect()
}

/// Removes empty and duplicate names, keeps order.
fn dedup(list: Vec<String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for n in list {
        let n = n.trim().to_string();
        if !n.is_empty() && !out.contains(&n) {
            out.push(n);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn listing_does_not_panic() {
        // May be empty on CI without audio hardware.
        let _ = list_input_devices();
        let _ = list_output_devices();
    }

    #[test]
    fn dedup_drops_empty_and_duplicates() {
        let v = vec!["A".into(), " ".into(), "B".into(), "A".into(), " B ".into()];
        assert_eq!(dedup(v), vec!["A".to_string(), "B".to_string()]);
    }

    #[test]
    fn failing_host_gives_empty() {
        assert!(collect(|_| Err::<Vec<String>, ()>(())).is_empty());
    }
}
