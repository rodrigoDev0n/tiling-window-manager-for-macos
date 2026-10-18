//! Platform-independent layout primitives.
//!
//! [`Shortcut`] helpers turn a screen work area and the current window
//! rectangle into a single target rectangle. The [`calculate`] entry point
//! dispatches full-layout computation to the pure algorithms in `layouts`.
//! These helpers perform no native calls and are fully testable.

use std::fmt;

use crate::core::geometry::{Rect, Size};
use crate::core::window::WindowId;

/// Re-exports the layout engine entry point (implemented in `crate::layouts`).
pub use crate::layouts::calculate;

/// A predefined window arrangement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shortcut {
    /// Left half of the work area.
    Left,
    /// Right half of the work area.
    Right,
    /// The entire work area.
    Maximize,
    /// The current size, clamped to and centered within the work area.
    Center,
}

impl Shortcut {
    /// Parses a CLI shortcut name.
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "left" => Some(Shortcut::Left),
            "right" => Some(Shortcut::Right),
            "maximize" => Some(Shortcut::Maximize),
            "center" => Some(Shortcut::Center),
            _ => None,
        }
    }
}

/// Computes the target rectangle for `shortcut`.
///
/// Returns `None` when `work_area` is invalid or, for [`Shortcut::Center`],
/// when the resulting rectangle would be invalid.
pub fn shortcut_rect(shortcut: Shortcut, work_area: Rect, current: Rect) -> Option<Rect> {
    if !work_area.is_valid() {
        return None;
    }

    let half_width = work_area.size.width / 2.0;
    let rect = match shortcut {
        Shortcut::Left => Rect::new(
            work_area.origin.x,
            work_area.origin.y,
            half_width,
            work_area.size.height,
        ),
        Shortcut::Right => Rect::new(
            work_area.origin.x + half_width,
            work_area.origin.y,
            half_width,
            work_area.size.height,
        ),
        Shortcut::Maximize => work_area,
        Shortcut::Center => {
            let size = centered_size(work_area.size, current.size);
            let x = work_area.origin.x + (work_area.size.width - size.width) / 2.0;
            let y = work_area.origin.y + (work_area.size.height - size.height) / 2.0;
            Rect::new(x, y, size.width, size.height)
        }
    };

    rect.is_valid().then_some(rect)
}

fn centered_size(work: Size, current: Size) -> Size {
    if current.is_valid() {
        Size::new(
            current.width.min(work.width),
            current.height.min(work.height),
        )
    } else {
        work
    }
}

/// A tiling layout algorithm.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutType {
    /// A left-hand master column plus a right-hand stack column.
    MasterStack,
    /// Equal-width vertical columns.
    Columns,
    /// Equal-height horizontal rows.
    Rows,
    /// All windows share the full usable area.
    Monocle,
    /// Two columns with the windows stacked inside each column.
    Quad,
}

impl LayoutType {
    /// Parses a CLI layout name.
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "masterstack" | "master_stack" | "master-stack" => Some(LayoutType::MasterStack),
            "columns" | "column" => Some(LayoutType::Columns),
            "rows" | "row" => Some(LayoutType::Rows),
            "monocle" => Some(LayoutType::Monocle),
            "quad" | "grid" => Some(LayoutType::Quad),
            _ => None,
        }
    }
}

impl fmt::Display for LayoutType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            LayoutType::MasterStack => "masterstack",
            LayoutType::Columns => "columns",
            LayoutType::Rows => "rows",
            LayoutType::Monocle => "monocle",
            LayoutType::Quad => "quad",
        };
        write!(f, "{name}")
    }
}

/// Configuration for a single layout calculation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LayoutConfig {
    /// The selected layout algorithm.
    pub layout: LayoutType,
    /// Gap, in points, around the complete layout.
    pub outer_gap: f64,
    /// Gap, in points, between adjacent tiled rectangles.
    pub inner_gap: f64,
    /// Proportion of horizontal space assigned to the master area.
    pub master_ratio: f64,
    /// Number of leading windows placed in the master group.
    pub master_count: usize,
}

impl Default for LayoutConfig {
    fn default() -> Self {
        Self {
            layout: LayoutType::MasterStack,
            outer_gap: 8.0,
            inner_gap: 8.0,
            master_ratio: 0.55,
            master_count: 1,
        }
    }
}

impl LayoutConfig {
    /// Validates values that do not depend on the available geometry.
    pub fn validate(&self) -> Result<(), LayoutError> {
        if !self.outer_gap.is_finite() || !self.inner_gap.is_finite() {
            return Err(LayoutError::InvalidConfig(
                "gaps must be finite".to_string(),
            ));
        }
        if self.outer_gap < 0.0 || self.inner_gap < 0.0 {
            return Err(LayoutError::InvalidConfig(
                "gaps must be non-negative".to_string(),
            ));
        }
        if !self.master_ratio.is_finite() || self.master_ratio <= 0.0 || self.master_ratio >= 1.0 {
            return Err(LayoutError::InvalidConfig(
                "master ratio must be strictly between 0 and 1".to_string(),
            ));
        }
        if self.master_count == 0 {
            return Err(LayoutError::InvalidConfig(
                "master count must be at least 1".to_string(),
            ));
        }
        Ok(())
    }
}

/// One computed window placement, preserving the input order.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlacedWindow {
    pub window: WindowId,
    pub rect: Rect,
}

/// The deterministic output of a layout calculation.
#[derive(Debug, Clone, PartialEq)]
pub struct LayoutResult {
    pub layout: LayoutType,
    pub windows: Vec<PlacedWindow>,
}

/// Errors produced while computing a layout.
#[derive(Debug, Clone, PartialEq)]
pub enum LayoutError {
    /// The available area has non-finite or non-positive dimensions.
    InvalidArea(String),
    /// Configuration values violate [`LayoutConfig::validate`].
    InvalidConfig(String),
    /// The window collection contains repeated identifiers.
    DuplicateWindowId(WindowId),
    /// Gaps and/or the master ratio consume all available space.
    InsufficientSpace { available: f64, required: f64 },
    /// A calculation produced an invalid rectangle (NaN, infinity, zero size).
    InvalidResult(String),
}

impl fmt::Display for LayoutError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LayoutError::InvalidArea(message) => write!(f, "invalid layout area: {message}"),
            LayoutError::InvalidConfig(message) => {
                write!(f, "invalid layout configuration: {message}")
            }
            LayoutError::DuplicateWindowId(window) => {
                write!(f, "duplicate window identifier: {}", window.0)
            }
            LayoutError::InsufficientSpace {
                available,
                required,
            } => write!(
                f,
                "insufficient space: {available:.0} points available, {required:.0} required"
            ),
            LayoutError::InvalidResult(message) => write!(f, "invalid layout result: {message}"),
        }
    }
}

impl std::error::Error for LayoutError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn work_area() -> Rect {
        Rect::new(0.0, 25.0, 1440.0, 800.0)
    }

    #[test]
    fn left_uses_left_half() {
        let rect = shortcut_rect(Shortcut::Left, work_area(), work_area());
        assert_eq!(rect, Some(Rect::new(0.0, 25.0, 720.0, 800.0)));
    }

    #[test]
    fn right_uses_right_half() {
        let rect = shortcut_rect(Shortcut::Right, work_area(), work_area());
        assert_eq!(rect, Some(Rect::new(720.0, 25.0, 720.0, 800.0)));
    }

    #[test]
    fn maximize_fills_work_area() {
        let rect = shortcut_rect(Shortcut::Maximize, work_area(), work_area());
        assert_eq!(rect, Some(work_area()));
    }

    #[test]
    fn center_keeps_clamped_size_and_centers() {
        let current = Rect::new(100.0, 100.0, 2000.0, 400.0);
        let rect = shortcut_rect(Shortcut::Center, work_area(), current);
        // Width clamped to 1440, height kept at 400, centered in the work area.
        assert_eq!(rect, Some(Rect::new(0.0, 225.0, 1440.0, 400.0)));
    }

    #[test]
    fn center_with_invalid_current_uses_work_area_size() {
        let current = Rect::new(0.0, 0.0, 0.0, 0.0);
        let rect = shortcut_rect(Shortcut::Center, work_area(), current);
        assert_eq!(rect, Some(work_area()));
    }

    #[test]
    fn invalid_work_area_is_rejected() {
        let bad = Rect::new(0.0, 0.0, 0.0, 0.0);
        assert_eq!(shortcut_rect(Shortcut::Maximize, bad, work_area()), None);
    }

    #[test]
    fn parse_accepts_known_names() {
        assert_eq!(Shortcut::parse("left"), Some(Shortcut::Left));
        assert_eq!(Shortcut::parse("right"), Some(Shortcut::Right));
        assert_eq!(Shortcut::parse("maximize"), Some(Shortcut::Maximize));
        assert_eq!(Shortcut::parse("center"), Some(Shortcut::Center));
        assert_eq!(Shortcut::parse("nope"), None);
    }

    #[test]
    fn layout_type_parse_accepts_lowercase_names() {
        assert_eq!(
            LayoutType::parse("masterstack"),
            Some(LayoutType::MasterStack)
        );
        assert_eq!(
            LayoutType::parse("master_stack"),
            Some(LayoutType::MasterStack)
        );
        assert_eq!(LayoutType::parse("columns"), Some(LayoutType::Columns));
        assert_eq!(LayoutType::parse("rows"), Some(LayoutType::Rows));
        assert_eq!(LayoutType::parse("monocle"), Some(LayoutType::Monocle));
        assert_eq!(LayoutType::parse("quad"), Some(LayoutType::Quad));
        assert_eq!(LayoutType::parse("grid"), Some(LayoutType::Quad));
        assert_eq!(LayoutType::parse("bsp"), None);
    }

    #[test]
    fn layout_type_displays_lowercase_name() {
        assert_eq!(LayoutType::MasterStack.to_string(), "masterstack");
        assert_eq!(LayoutType::Monocle.to_string(), "monocle");
        assert_eq!(LayoutType::Quad.to_string(), "quad");
    }

    #[test]
    fn default_config_matches_spec_004() {
        let config = LayoutConfig::default();
        assert_eq!(config.layout, LayoutType::MasterStack);
        assert_eq!(config.outer_gap, 8.0);
        assert_eq!(config.inner_gap, 8.0);
        assert_eq!(config.master_ratio, 0.55);
        assert_eq!(config.master_count, 1);
    }

    #[test]
    fn config_rejects_negative_or_non_finite_gaps() {
        let bad = LayoutConfig {
            outer_gap: -1.0,
            ..LayoutConfig::default()
        };
        assert!(matches!(bad.validate(), Err(LayoutError::InvalidConfig(_))));
        let bad = LayoutConfig {
            inner_gap: f64::NAN,
            ..LayoutConfig::default()
        };
        assert!(matches!(bad.validate(), Err(LayoutError::InvalidConfig(_))));
    }

    #[test]
    fn config_rejects_ratio_outside_open_unit_interval() {
        for ratio in [0.0, 1.0, f64::NAN, f64::INFINITY] {
            let bad = LayoutConfig {
                master_ratio: ratio,
                ..LayoutConfig::default()
            };
            assert!(matches!(bad.validate(), Err(LayoutError::InvalidConfig(_))));
        }
    }

    #[test]
    fn config_rejects_zero_master_count() {
        let bad = LayoutConfig {
            master_count: 0,
            ..LayoutConfig::default()
        };
        assert!(matches!(bad.validate(), Err(LayoutError::InvalidConfig(_))));
    }

    #[test]
    fn valid_config_accepts_zero_gaps() {
        let zero = LayoutConfig {
            outer_gap: 0.0,
            inner_gap: 0.0,
            ..LayoutConfig::default()
        };
        assert_eq!(zero.validate(), Ok(()));
    }
}
