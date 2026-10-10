use std::fmt;

/// Errors surfaced by the application boundary.
#[derive(Debug)]
pub enum AppError {
    /// Accessibility permissions are required but not granted.
    PermissionDenied,
    /// The requested command is not recognized.
    UnknownCommand(String),
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
        }
    }
}

impl std::error::Error for AppError {}
