//! TOML settings file (SPEC-011 FR-001, FR-002).
//!
//! Pure parsing and loading live here; the module has no AppKit types, so all
//! of its behavior is unit-testable without macOS permissions.

use std::fmt;
use std::path::{Path, PathBuf};

use serde::Deserialize;

/// Default flat background color used when no wallpaper is available
/// (SPEC-011 FR-002), as a strict `#RRGGBB` string.
pub const DEFAULT_BACKGROUND_COLOR: &str = "#1E1E1E";

/// The user configuration. Every key is optional; unknown keys are ignored so
/// older binaries tolerate newer config files.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Folder scanned (non-recursively) for wallpaper images.
    pub wallpaper_folder: Option<PathBuf>,
    /// Flat background color as `#RRGGBB`, used when no image applies.
    pub background_color: Option<String>,
}

/// Why a configuration file could not be used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigError {
    /// The file does not exist.
    NotFound(PathBuf),
    /// The file exists but is not valid TOML or does not fit the schema.
    InvalidToml(String),
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigError::NotFound(path) => {
                write!(f, "config file not found: {}", path.display())
            }
            ConfigError::InvalidToml(message) => {
                write!(f, "invalid config file: {message}")
            }
        }
    }
}

impl std::error::Error for ConfigError {}

impl Settings {
    /// Parses TOML contents. Unknown keys and an empty document are valid and
    /// yield [`Settings::default`].
    pub fn parse(contents: &str) -> Result<Self, ConfigError> {
        toml::from_str(contents).map_err(|error| ConfigError::InvalidToml(error.to_string()))
    }

    /// Loads the file at `path`, mapping a missing file to
    /// [`ConfigError::NotFound`].
    pub fn load_from(path: &Path) -> Result<Self, ConfigError> {
        let contents = std::fs::read_to_string(path).map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                ConfigError::NotFound(path.to_path_buf())
            } else {
                ConfigError::InvalidToml(format!("{}: {error}", path.display()))
            }
        })?;
        Self::parse(&contents)
    }

    /// Loads the configuration for the current process.
    ///
    /// A missing file applies defaults silently; an invalid file reports a
    /// diagnostic and applies defaults. Loading never fails.
    pub fn load() -> Self {
        load_or_default(default_path().as_deref())
    }
}

/// Resolves the configuration path: `$XDG_CONFIG_HOME/tiling_window/config.toml`
/// with a `~/.config/tiling_window/config.toml` fallback (SPEC-011 §15.3).
pub fn default_path() -> Option<PathBuf> {
    if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
    {
        return Some(xdg.join("tiling_window").join("config.toml"));
    }
    std::env::var_os("HOME")
        .filter(|value| !value.is_empty())
        .map(|home| {
            PathBuf::from(home)
                .join(".config")
                .join("tiling_window")
                .join("config.toml")
        })
}

/// Loads `path` (or defaults when `path` is `None`), reporting diagnostics for
/// present-but-invalid files.
fn load_or_default(path: Option<&Path>) -> Settings {
    let Some(path) = path else {
        return Settings::default();
    };
    match Settings::load_from(path) {
        Ok(settings) => settings,
        Err(ConfigError::NotFound(_)) => Settings::default(),
        Err(error) => {
            eprintln!("config: {error}; using defaults");
            Settings::default()
        }
    }
}

/// Parses a strict `#RRGGBB` color into its red, green and blue components.
///
/// Returns `None` for anything else (short forms, `0x` prefixes, names,
/// trailing characters, or non-hex digits).
pub fn parse_hex_color(value: &str) -> Option<[u8; 3]> {
    let digits = value.strip_prefix('#')?;
    if digits.len() != 6 || !digits.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    let component = |range: std::ops::Range<usize>| u8::from_str_radix(&digits[range], 16).ok();
    Some([component(0..2)?, component(2..4)?, component(4..6)?])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Writes `contents` to a unique temp file, runs `test`, and removes it.
    fn with_temp_config(contents: &str, test: impl FnOnce(&Path)) {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "tiling_window_config_test_{}_{}.toml",
            std::process::id(),
            contents.len()
        ));
        std::fs::write(&path, contents).expect("write temp config");
        test(&path);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn missing_file_is_reported_as_not_found() {
        let path = std::env::temp_dir().join("tiling_window_config_does_not_exist.toml");
        let _ = std::fs::remove_file(&path);
        assert_eq!(Settings::load_from(&path), Err(ConfigError::NotFound(path)));
    }

    #[test]
    fn valid_file_loads_both_keys() {
        let contents = r##"
            wallpaper_folder = "/Users/me/Pictures/Wallpapers"
            background_color = "#102030"
        "##;
        with_temp_config(contents, |path| {
            let settings = Settings::load_from(path).expect("valid config");
            assert_eq!(
                settings.wallpaper_folder,
                Some(PathBuf::from("/Users/me/Pictures/Wallpapers"))
            );
            assert_eq!(settings.background_color.as_deref(), Some("#102030"));
        });
    }

    #[test]
    fn unknown_keys_are_ignored() {
        let contents = r#"
            wallpaper_folder = "/w"
            future_option = 42
            another = { nested = true }
        "#;
        let settings = Settings::parse(contents).expect("unknown keys tolerated");
        assert_eq!(settings.wallpaper_folder, Some(PathBuf::from("/w")));
        assert_eq!(settings.background_color, None);
    }

    #[test]
    fn empty_file_yields_defaults() {
        assert_eq!(Settings::parse("").expect("empty"), Settings::default());
    }

    #[test]
    fn folder_only_key_is_valid() {
        let settings = Settings::parse(r#"wallpaper_folder = "/w""#).expect("folder only");
        assert_eq!(settings.wallpaper_folder, Some(PathBuf::from("/w")));
        assert_eq!(settings.background_color, None);
    }

    #[test]
    fn invalid_toml_is_rejected() {
        assert!(matches!(
            Settings::parse("wallpaper_folder = "),
            Err(ConfigError::InvalidToml(_))
        ));
        assert!(matches!(
            Settings::parse("wallpaper_folder = 42"),
            Err(ConfigError::InvalidToml(_))
        ));
    }

    #[test]
    fn load_or_default_covers_missing_path_and_invalid_file() {
        assert_eq!(load_or_default(None), Settings::default());

        let missing = std::env::temp_dir().join("tiling_window_config_absent.toml");
        let _ = std::fs::remove_file(&missing);
        assert_eq!(load_or_default(Some(&missing)), Settings::default());

        with_temp_config("not = valid = toml", |path| {
            assert_eq!(load_or_default(Some(path)), Settings::default());
        });
    }

    #[test]
    fn hex_colors_parse_strictly() {
        assert_eq!(parse_hex_color("#1E1E1E"), Some([0x1E, 0x1E, 0x1E]));
        assert_eq!(parse_hex_color("#000000"), Some([0, 0, 0]));
        assert_eq!(parse_hex_color("#abcdef"), Some([0xAB, 0xCD, 0xEF]));
        assert_eq!(parse_hex_color("#ABCDEF"), Some([0xAB, 0xCD, 0xEF]));

        assert_eq!(parse_hex_color("1E1E1E"), None);
        assert_eq!(parse_hex_color("#1E1E1"), None);
        assert_eq!(parse_hex_color("#1E1E1E0"), None);
        assert_eq!(parse_hex_color("#1E1E1G"), None);
        assert_eq!(parse_hex_color("# 1E1E1E"), None);
        assert_eq!(parse_hex_color(""), None);
    }

    #[test]
    fn default_color_constant_is_valid() {
        assert!(parse_hex_color(DEFAULT_BACKGROUND_COLOR).is_some());
    }
}
