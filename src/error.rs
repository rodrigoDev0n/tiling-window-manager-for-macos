use std::fmt;

use crate::core::selector::WindowCandidate;

/// Errors surfaced by the application boundary.
#[derive(Debug)]
pub enum AppError {
    /// Accessibility permissions are required but not granted.
    PermissionDenied,
    /// The requested command is not recognized.
    UnknownCommand(String),
    /// The command arguments are missing or contradictory.
    InvalidArgument(String),
    /// No window matched the provided selector.
    WindowNotFound,
    /// More than one window matched and the selector was not specific enough.
    AmbiguousSelection(Vec<WindowCandidate>),
    /// No focused window could be resolved.
    NoFocusedWindow,
    /// The selected window does not support position or size changes.
    UnsupportedWindow,
    /// A native window operation failed for another reason.
    ControlFailed(String),
    /// The requested display identifier is not among the active displays.
    DisplayNotFound(u32),
    /// The overlay window could not be created or managed.
    OverlayFailed(String),
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AppError::PermissionDenied => {
                write!(f, "Accessibility permission is not granted")
            }
            AppError::UnknownCommand(command) => {
                write!(f, "unknown command: {command}")
            }
            AppError::InvalidArgument(message) => write!(f, "{message}"),
            AppError::WindowNotFound => write!(f, "no window matched the selector"),
            AppError::AmbiguousSelection(candidates) => {
                writeln!(f, "selector matched {} windows:", candidates.len())?;
                for candidate in candidates {
                    let title = candidate.title.as_deref().unwrap_or("(untitled)");
                    writeln!(
                        f,
                        "  PID {}  {}  \"{}\"",
                        candidate.pid.0, candidate.application, title
                    )?;
                }
                write!(f, "use --index to disambiguate")
            }
            AppError::NoFocusedWindow => write!(f, "no focused window is available"),
            AppError::UnsupportedWindow => {
                write!(
                    f,
                    "the selected window does not support position or size changes"
                )
            }
            AppError::ControlFailed(message) => write!(f, "window control failed: {message}"),
            AppError::DisplayNotFound(id) => {
                write!(f, "no active display with identifier {id}")
            }
            AppError::OverlayFailed(message) => write!(f, "overlay failed: {message}"),
        }
    }
}

impl std::error::Error for AppError {}
