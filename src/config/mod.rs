//! User configuration (SPEC-011).
//!
//! The manager reads an optional TOML file at startup. A missing file is not
//! an error: defaults apply. An invalid file produces a diagnostic and falls
//! back to defaults; it never aborts the process.

pub mod settings;

pub use settings::{DEFAULT_BACKGROUND_COLOR, Settings, parse_hex_color};
