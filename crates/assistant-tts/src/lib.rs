//! Text-to-speech via Windows SAPI.
//!
//! Backend: SAPI `ISpVoice` through the `windows` crate. The `tts` crate wraps
//! WinRT SpeechSynthesizer on Windows, which has no reliable pause/rate control;
//! SAPI offers Pause/Resume/SetRate directly. COM objects live on one worker
//! thread (MTA) fed by an mpsc channel, so callers never block.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::Arc;
use std::thread::JoinHandle;

pub const MIN_RATE: i32 = -10;
pub const MAX_RATE: i32 = 10;
pub const MAX_TEXT_CHARS: usize = 20_000;

/// Clamps a speech rate into SAPI's -10..=10 range.
pub fn clamp_rate(rate: i32) -> i32 {
    rate.clamp(MIN_RATE, MAX_RATE)
}

/// True if a SAPI output description and a device name denote the same device.
/// Case-insensitive; legacy MME names are cut to 31 characters, so a prefix
/// of at least 8 characters on either side counts too.
pub fn device_matches(description: &str, wanted: &str) -> bool {
    let (d, w) = (
        description.trim().to_lowercase(),
        wanted.trim().to_lowercase(),
    );
    if d.is_empty() || w.is_empty() {
        return false;
    }
    d == w || (d.len().min(w.len()) >= 8 && (d.starts_with(&w) || w.starts_with(&d)))
}

/// Trims, collapses all whitespace runs to one space, caps at `MAX_TEXT_CHARS`.
pub fn clean_text(s: &str) -> String {
    let mut out = String::new();
    for word in s.split_whitespace() {
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(word);
        if out.chars().count() >= MAX_TEXT_CHARS {
            break;
        }
    }
    if out.chars().count() > MAX_TEXT_CHARS {
        out = out.chars().take(MAX_TEXT_CHARS).collect();
    }
    out
}

enum Msg {
    Speak(String),
    Pause,
    Resume,
    Stop,
    Rate(i32),
    /// Output device name; `None` = system default.
    Output(Option<String>),
    Quit,
}

/// Non-blocking speech handle. Dropping it stops speech and joins the worker.
pub struct Tts {
    tx: Sender<Msg>,
    speaking: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl Tts {
    /// Starts the worker thread. Fails if the SAPI voice cannot be created.
    pub fn new() -> Result<Tts, String> {
        let (tx, rx) = mpsc::channel();
        let (ready_tx, ready_rx) = mpsc::channel();
        let speaking = Arc::new(AtomicBool::new(false));
        let flag = speaking.clone();
        let worker = std::thread::Builder::new()
            .name("tts".into())
            .spawn(move || backend::run(rx, ready_tx, flag))
            .map_err(|e| e.to_string())?;
        match ready_rx.recv() {
            Ok(Ok(())) => Ok(Tts {
                tx,
                speaking,
                worker: Some(worker),
            }),
            Ok(Err(e)) => {
                let _ = worker.join();
                Err(e)
            }
            Err(_) => Err("tts worker died".into()),
        }
    }

    /// Speaks `text` asynchronously, purging earlier speech. Empty text only stops.
    pub fn speak(&self, text: &str) {
        let t = clean_text(text);
        if t.is_empty() {
            self.stop();
        } else {
            // Optimistic so callers see "speaking" before the worker polls.
            self.speaking.store(true, Ordering::SeqCst);
            let _ = self.tx.send(Msg::Speak(t));
        }
    }

    pub fn pause(&self) {
        let _ = self.tx.send(Msg::Pause);
    }

    pub fn resume(&self) {
        let _ = self.tx.send(Msg::Resume);
    }

    pub fn stop(&self) {
        self.speaking.store(false, Ordering::SeqCst);
        let _ = self.tx.send(Msg::Stop);
    }

    /// Rate -10..=10 (clamped); 0 is normal.
    pub fn set_rate(&self, rate: i32) {
        let _ = self.tx.send(Msg::Rate(clamp_rate(rate)));
    }

    /// Routes speech to the output device with this name (as listed by
    /// `assistant-audio`); `None` = system default. Best effort: an unknown
    /// device or a SAPI error silently keeps the current output.
    pub fn set_output_device(&self, name: Option<&str>) {
        let _ = self.tx.send(Msg::Output(name.map(str::to_owned)));
    }

    /// True while speech is running or paused.
    pub fn is_speaking(&self) -> bool {
        self.speaking.load(Ordering::SeqCst)
    }
}

impl Drop for Tts {
    fn drop(&mut self) {
        let _ = self.tx.send(Msg::Quit);
        if let Some(w) = self.worker.take() {
            let _ = w.join();
        }
    }
}

#[cfg(windows)]
mod output;

#[cfg(windows)]
mod backend {
    use super::Msg;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
    use std::sync::Arc;
    use std::time::Duration;
    use windows::core::{IUnknown, Interface, PCWSTR};
    use windows::Win32::Media::Speech::{ISpVoice, SpVoice, SPVOICESTATUS};
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_MULTITHREADED,
    };

    const SPF_ASYNC: u32 = 1;
    const SPF_PURGEBEFORESPEAK: u32 = 2;
    const SPF_IS_NOT_XML: u32 = 16;
    /// SPRS_DONE = 1: speech finished.
    const SPRS_DONE: u32 = 1;

    /// Best effort: on any failure the voice keeps its current output.
    unsafe fn set_output(voice: &ISpVoice, name: Option<&str>) {
        let token = match name {
            None => None,
            Some(n) => match crate::output::find_token(n) {
                Some(t) => Some(t),
                None => return,
            },
        };
        let unk = token.and_then(|t| t.cast::<IUnknown>().ok());
        // None resets to the default output.
        let _ = voice.SetOutput(unk.as_ref(), true);
    }

    pub fn run(rx: Receiver<Msg>, ready: Sender<Result<(), String>>, speaking: Arc<AtomicBool>) {
        // SAFETY: COM is initialised and uninitialised on this thread only; the
        // voice is dropped before CoUninitialize.
        unsafe {
            if let Err(e) = CoInitializeEx(None, COINIT_MULTITHREADED).ok() {
                let _ = ready.send(Err(format!("CoInitializeEx: {e}")));
                return;
            }
            let voice: ISpVoice = match CoCreateInstance(&SpVoice, None, CLSCTX_ALL) {
                Ok(v) => v,
                Err(e) => {
                    let _ = ready.send(Err(format!("SpVoice: {e}")));
                    CoUninitialize();
                    return;
                }
            };
            let _ = ready.send(Ok(()));
            let mut paused = false;
            let mut just_spoke;
            loop {
                just_spoke = false;
                match rx.recv_timeout(Duration::from_millis(100)) {
                    Ok(Msg::Speak(text)) => {
                        let _ = voice.Resume();
                        paused = false;
                        just_spoke = true;
                        let wide: Vec<u16> = text.encode_utf16().chain(Some(0)).collect();
                        let r = voice.Speak(
                            PCWSTR(wide.as_ptr()),
                            SPF_ASYNC | SPF_PURGEBEFORESPEAK | SPF_IS_NOT_XML,
                            None,
                        );
                        if r.is_err() {
                            speaking.store(false, Ordering::SeqCst);
                        }
                    }
                    Ok(Msg::Pause) => {
                        if voice.Pause().is_ok() {
                            paused = true;
                        }
                    }
                    Ok(Msg::Resume) => {
                        let _ = voice.Resume();
                        paused = false;
                    }
                    Ok(Msg::Stop) => {
                        let _ = voice.Resume();
                        paused = false;
                        let _ = voice.Speak(PCWSTR::null(), SPF_PURGEBEFORESPEAK, None);
                    }
                    Ok(Msg::Rate(r)) => {
                        let _ = voice.SetRate(r);
                    }
                    Ok(Msg::Output(name)) => {
                        let _ = voice.Resume();
                        paused = false;
                        set_output(&voice, name.as_deref());
                    }
                    Ok(Msg::Quit) | Err(RecvTimeoutError::Disconnected) => {
                        let _ = voice.Resume();
                        let _ = voice.Speak(PCWSTR::null(), SPF_PURGEBEFORESPEAK, None);
                        break;
                    }
                    Err(RecvTimeoutError::Timeout) => {}
                }
                // Async Speak may still report DONE for a moment; skip one poll.
                if paused || just_spoke {
                    continue;
                }
                // Messages are handled before polling, so a just-queued Speak is
                // already submitted when the state is read.
                let mut st = SPVOICESTATUS::default();
                if voice.GetStatus(&mut st, std::ptr::null_mut()).is_ok()
                    && st.dwRunningState == SPRS_DONE
                {
                    speaking.store(false, Ordering::SeqCst);
                }
            }
            drop(voice);
            CoUninitialize();
        }
    }
}

#[cfg(not(windows))]
mod backend {
    use super::Msg;
    use std::sync::atomic::AtomicBool;
    use std::sync::mpsc::{Receiver, Sender};
    use std::sync::Arc;

    pub fn run(_rx: Receiver<Msg>, ready: Sender<Result<(), String>>, _s: Arc<AtomicBool>) {
        let _ = ready.send(Err("TTS needs Windows".into()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_name_matching() {
        assert!(device_matches(
            "Lautsprecher (Realtek)",
            "lautsprecher (realtek)"
        ));
        assert!(device_matches(
            "MONITOR L/R (Volt 1",
            "MONITOR L/R (Volt 1)"
        ));
        assert!(!device_matches("Lautsprecher (Realtek)", "Kopfhörer (USB)"));
        assert!(!device_matches("", ""));
        assert!(!device_matches("abc", "abcdef"));
    }

    #[cfg(windows)]
    #[test]
    fn lists_sapi_outputs_without_panic() {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            unsafe {
                use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};
                let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            }
            tx.send(output::descriptions()).unwrap();
        });
        let n = rx.recv().unwrap();
        eprintln!("sapi output tokens: {n:?}");
    }

    #[test]
    fn rate_clamps() {
        assert_eq!(clamp_rate(-99), -10);
        assert_eq!(clamp_rate(99), 10);
        assert_eq!(clamp_rate(3), 3);
    }

    #[test]
    fn clean_collapses_and_trims() {
        assert_eq!(clean_text("  a \n\t b   c  "), "a b c");
        assert_eq!(clean_text(" \n "), "");
    }

    #[test]
    fn clean_caps_length() {
        let long = "ä".repeat(MAX_TEXT_CHARS + 500);
        assert_eq!(clean_text(&long).chars().count(), MAX_TEXT_CHARS);
        let words = "wort ".repeat(10_000);
        assert!(clean_text(&words).chars().count() <= MAX_TEXT_CHARS);
    }
}
