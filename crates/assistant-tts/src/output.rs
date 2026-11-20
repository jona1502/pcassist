//! Maps an output device name (as listed by `assistant-audio`) to a SAPI audio
//! output token (SPCAT_AUDIOOUT, machine-wide category; the per-user category
//! enumerates nothing). Best effort: no match or any COM error means "use default".

use windows::core::{w, PCWSTR};
use windows::Win32::Media::Speech::{
    IEnumSpObjectTokens, ISpObjectToken, ISpObjectTokenCategory, SpObjectTokenCategory,
};
use windows::Win32::System::Com::{CoCreateInstance, CoTaskMemFree, CLSCTX_ALL};

/// Must run on a COM-initialised thread. Returns the token whose description
/// matches `wanted` (see `device_matches`).
pub fn find_token(wanted: &str) -> Option<ISpObjectToken> {
    tokens()?
        .into_iter()
        .find(|(desc, _)| crate::device_matches(desc, wanted))
        .map(|(_, t)| t)
}

/// Descriptions of all SAPI audio output tokens (for diagnostics and tests).
#[cfg(test)]
pub fn descriptions() -> Vec<String> {
    tokens()
        .map(|v| v.into_iter().map(|(d, _)| d).collect())
        .unwrap_or_default()
}

fn tokens() -> Option<Vec<(String, ISpObjectToken)>> {
    // SAFETY: plain COM calls on interfaces we own; the string returned by
    // GetStringValue is CoTaskMem memory and freed right after copying.
    unsafe {
        let cat: ISpObjectTokenCategory =
            CoCreateInstance(&SpObjectTokenCategory, None, CLSCTX_ALL).ok()?;
        cat.SetId(
            w!(r"HKEY_LOCAL_MACHINE\SOFTWARE\Microsoft\Speech\AudioOutput"),
            false,
        )
        .ok()?;
        let en: IEnumSpObjectTokens = cat.EnumTokens(PCWSTR::null(), PCWSTR::null()).ok()?;
        let mut out = Vec::new();
        loop {
            let mut tok: Option<ISpObjectToken> = None;
            let mut fetched = 0u32;
            if en.Next(1, &mut tok, Some(&mut fetched)).is_err() || fetched == 0 {
                break;
            }
            let Some(tok) = tok else { break };
            if let Ok(p) = tok.GetStringValue(PCWSTR::null()) {
                if !p.is_null() {
                    let desc = p.to_string().unwrap_or_default();
                    CoTaskMemFree(Some(p.0 as *const _));
                    out.push((desc, tok));
                }
            }
        }
        Some(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::core::{IUnknown, Interface};
    use windows::Win32::Media::Speech::{ISpVoice, SpVoice};
    use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_MULTITHREADED};

    /// Routes a voice to the first listed output and reads the token back.
    /// Skips silently on machines without SAPI outputs.
    #[test]
    fn set_output_roundtrip() {
        std::thread::spawn(|| unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            let names = descriptions();
            if let Some(name) = names.first() {
                let token = find_token(name).expect("listed token is found");
                let voice: ISpVoice = CoCreateInstance(&SpVoice, None, CLSCTX_ALL).unwrap();
                let unk = token.cast::<IUnknown>().unwrap();
                voice.SetOutput(&unk, true).expect("SetOutput");
                let got = voice.GetOutputObjectToken().expect("output token");
                let p = got.GetStringValue(PCWSTR::null()).unwrap();
                assert_eq!(&p.to_string().unwrap(), name);
                CoTaskMemFree(Some(p.0 as *const _));
                voice.SetOutput(None::<&IUnknown>, true).expect("reset");
            }
            CoUninitialize();
        })
        .join()
        .unwrap();
    }
}
