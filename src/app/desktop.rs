//! Desktop background model (SPEC-011).
//!
//! Everything here is platform independent: folder scanning, wallpaper
//! selection, the background choice, and the desktop-visibility state are pure
//! and unit-testable. Native window work lives in `platform::macos::desktop`.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::config::{DEFAULT_BACKGROUND_COLOR, Settings, parse_hex_color};

/// Image extensions treated as wallpapers (SPEC-011 FR-003).
const SUPPORTED_EXTENSIONS: [&str; 8] = ["png", "jpg", "jpeg", "heic", "tiff", "tif", "bmp", "gif"];

/// A flat background color with sRGB components in `0..=255`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FlatColor {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
}

impl Default for FlatColor {
    /// The default flat color (`#1E1E1E`, SPEC-011 FR-002).
    fn default() -> Self {
        parse_hex_color(DEFAULT_BACKGROUND_COLOR)
            .map(|[red, green, blue]| Self { red, green, blue })
            .unwrap_or(Self {
                red: 0x1E,
                green: 0x1E,
                blue: 0x1E,
            })
    }
}

/// What the manager draws as the desktop background (SPEC-011 §5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Background {
    /// An image file from the configured folder; scaled to fill each display.
    Image(PathBuf),
    /// A minimalist flat color, used when no image applies.
    Color(FlatColor),
}

/// Visibility of the Finder desktop icons over the manager's background
/// (SPEC-011 FR-007). The desktop starts **hidden**.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DesktopState {
    visible: bool,
}

impl DesktopState {
    /// Flips the visibility and returns the new value: `true` means the
    /// Finder desktop icons are shown.
    pub fn toggle(&mut self) -> bool {
        self.visible = !self.visible;
        self.visible
    }

    /// Whether the Finder desktop icons are currently shown.
    #[allow(dead_code)]
    pub const fn is_visible(&self) -> bool {
        self.visible
    }
}

/// Collects supported image files directly inside `folder` (SPEC-011 FR-003),
/// sorted by file name so selection is independent of directory order.
///
/// Subdirectories and unsupported extensions are skipped. I/O errors while
/// reading the directory or a single entry are propagated to the caller.
pub fn collect_candidates(folder: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut candidates = Vec::new();
    for entry in std::fs::read_dir(folder)? {
        let path = entry?.path();
        if path.is_file() && has_supported_extension(&path) {
            candidates.push(path);
        }
    }
    candidates.sort_by(|left, right| left.file_name().cmp(&right.file_name()));
    Ok(candidates)
}

fn has_supported_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            SUPPORTED_EXTENSIONS
                .iter()
                .any(|supported| extension.eq_ignore_ascii_case(supported))
        })
}

/// Picks the wallpaper of this session (SPEC-011 FR-004).
///
/// Returns `None` when there is no candidate; otherwise the candidate at
/// `random_u64 % candidates.len()`, which makes the choice deterministic for a
/// given random value and therefore unit-testable.
pub fn select_wallpaper(candidates: &[PathBuf], random_u64: u64) -> Option<&Path> {
    if candidates.is_empty() {
        return None;
    }
    let index = (random_u64 % candidates.len() as u64) as usize;
    candidates.get(index).map(PathBuf::as_path)
}

/// One value of session entropy for the per-session wallpaper choice
/// (SPEC-011 FR-004). Derived from the wall clock and the process id; no
/// dependency or thread is introduced for randomness.
pub fn session_random() -> u64 {
    let elapsed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO);
    (elapsed.as_nanos() as u64) ^ (u64::from(std::process::id()) << 32)
}

/// Resolves the configured settings into a concrete background
/// (SPEC-011 §5 behavior matrix) plus every diagnostic found on the way.
///
/// - No folder, an unreadable folder, or a folder without supported images
///   yields the flat color (configured `background_color`, or `#1E1E1E`).
/// - An invalid `background_color` yields a diagnostic and the default color.
pub fn resolve_background(settings: &Settings, random_u64: u64) -> (Background, Vec<String>) {
    let mut diagnostics = Vec::new();

    let color = match settings.background_color.as_deref() {
        Some(value) => match parse_hex_color(value) {
            Some([red, green, blue]) => FlatColor { red, green, blue },
            None => {
                diagnostics.push(format!(
                    "invalid background_color {value:?} (expected {DEFAULT_BACKGROUND_COLOR} format); using {DEFAULT_BACKGROUND_COLOR}"
                ));
                FlatColor::default()
            }
        },
        None => FlatColor::default(),
    };

    let Some(folder) = settings.wallpaper_folder.clone() else {
        return (Background::Color(color), diagnostics);
    };

    let candidates = match collect_candidates(&folder) {
        Ok(candidates) => candidates,
        Err(error) => {
            diagnostics.push(format!(
                "wallpaper folder {} is unreadable: {error}; using flat color",
                folder.display()
            ));
            return (Background::Color(color), diagnostics);
        }
    };

    match select_wallpaper(&candidates, random_u64) {
        Some(path) => (Background::Image(path.to_path_buf()), diagnostics),
        None => {
            diagnostics.push(format!(
                "wallpaper folder {} contains no supported images; using flat color",
                folder.display()
            ));
            (Background::Color(color), diagnostics)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static UNIQUE: AtomicU64 = AtomicU64::new(0);

    /// Creates a unique empty temp directory for one test.
    fn temp_dir() -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "tiling_window_desktop_test_{}_{}",
            std::process::id(),
            UNIQUE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&path).expect("create temp dir");
        path
    }

    #[test]
    fn desktop_starts_hidden_and_toggles() {
        let mut state = DesktopState::default();
        assert!(!state.is_visible());
        assert!(state.toggle());
        assert!(state.is_visible());
        assert!(!state.toggle());
        assert!(!state.is_visible());
    }

    #[test]
    fn default_flat_color_matches_the_config_default() {
        assert_eq!(
            FlatColor::default(),
            FlatColor {
                red: 0x1E,
                green: 0x1E,
                blue: 0x1E
            }
        );
    }

    #[test]
    fn select_wallpaper_returns_none_for_empty_candidates() {
        assert_eq!(select_wallpaper(&[], 0), None);
        assert_eq!(select_wallpaper(&[], u64::MAX), None);
    }

    #[test]
    fn select_wallpaper_picks_the_index_modulo_length() {
        let candidates = vec![
            PathBuf::from("/w/a.png"),
            PathBuf::from("/w/b.png"),
            PathBuf::from("/w/c.png"),
        ];
        assert_eq!(
            select_wallpaper(&candidates, 0),
            Some(Path::new("/w/a.png"))
        );
        assert_eq!(
            select_wallpaper(&candidates, 1),
            Some(Path::new("/w/b.png"))
        );
        assert_eq!(
            select_wallpaper(&candidates, 2),
            Some(Path::new("/w/c.png"))
        );
        assert_eq!(
            select_wallpaper(&candidates, 3),
            Some(Path::new("/w/a.png"))
        );
        // 2^64 ≡ 1 (mod 3), so u64::MAX = 2^64 − 1 ≡ 0 (mod 3).
        assert_eq!(
            select_wallpaper(&candidates, u64::MAX),
            Some(Path::new("/w/a.png"))
        );
    }

    #[test]
    fn collect_candidates_filters_and_sorts() {
        let folder = temp_dir();
        std::fs::write(folder.join("c.png"), b"x").expect("file");
        std::fs::write(folder.join("a.JPG"), b"x").expect("file");
        std::fs::write(folder.join("b.txt"), b"x").expect("file");
        std::fs::write(folder.join("no_extension"), b"x").expect("file");
        std::fs::create_dir(folder.join("sub.png")).expect("dir");

        let candidates = collect_candidates(&folder).expect("read dir");
        let names: Vec<String> = candidates
            .iter()
            .map(|path| {
                path.file_name()
                    .expect("file name")
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        assert_eq!(names, vec!["a.JPG".to_string(), "c.png".to_string()]);

        std::fs::remove_dir_all(&folder).expect("cleanup");
    }

    #[test]
    fn collect_candidates_missing_folder_is_an_error() {
        let folder = std::env::temp_dir().join("tiling_window_desktop_missing_folder");
        let _ = std::fs::remove_dir(&folder);
        assert!(collect_candidates(&folder).is_err());
    }

    #[test]
    fn resolve_without_folder_uses_default_color() {
        let (background, diagnostics) = resolve_background(&Settings::default(), 7);
        assert_eq!(background, Background::Color(FlatColor::default()));
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn resolve_uses_configured_color() {
        let settings = Settings {
            wallpaper_folder: None,
            background_color: Some("#102030".to_string()),
        };
        let (background, diagnostics) = resolve_background(&settings, 7);
        assert_eq!(
            background,
            Background::Color(FlatColor {
                red: 0x10,
                green: 0x20,
                blue: 0x30
            })
        );
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn resolve_invalid_color_reports_and_falls_back() {
        let settings = Settings {
            wallpaper_folder: None,
            background_color: Some("blue".to_string()),
        };
        let (background, diagnostics) = resolve_background(&settings, 7);
        assert_eq!(background, Background::Color(FlatColor::default()));
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].contains("background_color"));
    }

    #[test]
    fn resolve_missing_folder_reports_and_uses_color() {
        let settings = Settings {
            wallpaper_folder: Some(
                std::env::temp_dir().join("tiling_window_wallpaper_missing_folder"),
            ),
            background_color: None,
        };
        let (background, diagnostics) = resolve_background(&settings, 0);
        assert_eq!(background, Background::Color(FlatColor::default()));
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].contains("unreadable"));
    }

    #[test]
    fn resolve_empty_folder_reports_and_uses_color() {
        let folder = temp_dir();
        let settings = Settings {
            wallpaper_folder: Some(folder.clone()),
            background_color: None,
        };
        let (background, diagnostics) = resolve_background(&settings, 0);
        assert_eq!(background, Background::Color(FlatColor::default()));
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].contains("no supported images"));

        std::fs::remove_dir_all(&folder).expect("cleanup");
    }

    #[test]
    fn resolve_folder_with_image_yields_image() {
        let folder = temp_dir();
        std::fs::write(folder.join("wall.png"), b"x").expect("file");
        let settings = Settings {
            wallpaper_folder: Some(folder.clone()),
            background_color: Some("bad".to_string()),
        };
        let (background, diagnostics) = resolve_background(&settings, 0);
        assert_eq!(background, Background::Image(folder.join("wall.png")));
        // Only the color diagnostic; the image applies regardless.
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].contains("background_color"));

        std::fs::remove_dir_all(&folder).expect("cleanup");
    }

    #[test]
    fn session_random_is_generated() {
        // Two calls in the same process may collide only with astronomically
        // low probability; assert only that the value is produced as non-zero
        // entropy input is not guaranteed, so accept any value but call it
        // twice to exercise the path.
        let _ = session_random();
        let _ = session_random();
    }
}
