//! Core types shared across the assistant.

/// Commands the assistant can execute.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    OpenPanel,
    ClosePanel,
    TogglePanel,
    OpenSettings,
    Quit,
}
