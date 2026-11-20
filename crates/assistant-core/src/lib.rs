//! Core types shared across the assistant.

/// Toggleable assistant features.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Feature {
    Zoom,
    Tts,
    Mic,
    Contrast,
}

/// Commands the assistant can execute.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    OpenPanel,
    ClosePanel,
    TogglePanel,
    OpenSettings,
    ToggleFeature(Feature),
    ZoomIn,
    ZoomOut,
    ZoomReset,
    SpeakSelection,
    /// Toggles pause/resume of the current speech.
    SpeakPause,
    SpeakStop,
    Quit,
}
