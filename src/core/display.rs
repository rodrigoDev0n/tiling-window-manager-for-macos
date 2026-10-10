//! Platform-independent display domain model.
//!
//! All geometry uses the canonical layout convention: the origin is the
//! top-left corner of the main display, X grows to the right, Y grows
//! downward, and all values are measured in points.

use std::fmt;
use std::str::FromStr;

use crate::core::geometry::Rect;

/// Tolerance (in points) allowed when checking containment of usable bounds.
const CONTAINMENT_EPSILON: f64 = 0.5;

/// Runtime display identifier, wrapping a Core Graphics `CGDirectDisplayID`.
///
/// This identifier is stable only for the current display configuration. It may
/// change after reconnecting, reordering, sleeping, or rebooting displays; no
/// public API provides a persistent hardware identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DisplayId(pub u32);

impl DisplayId {
    /// Parses a decimal or `0x`-prefixed hexadecimal identifier.
    pub fn parse(value: &str) -> Result<Self, DisplayIdError> {
        let trimmed = value.trim();
        let parsed = if let Some(hex) = trimmed
            .strip_prefix("0x")
            .or_else(|| trimmed.strip_prefix("0X"))
        {
            u32::from_str_radix(hex, 16)
        } else {
            trimmed.parse::<u32>()
        };
        parsed
            .map(DisplayId)
            .map_err(|_| DisplayIdError(value.to_string()))
    }
}

impl fmt::Display for DisplayId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for DisplayId {
    type Err = DisplayIdError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse(value)
    }
}

/// Error returned when a display identifier cannot be parsed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DisplayIdError(pub String);

impl fmt::Display for DisplayIdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid display identifier: {}", self.0)
    }
}

impl std::error::Error for DisplayIdError {}

/// A discovered display and its geometry.
#[derive(Debug, Clone, PartialEq)]
pub struct Display {
    /// Runtime identifier.
    pub id: DisplayId,
    /// Localized display name when available.
    pub name: Option<String>,
    /// Whether this is the main (menu-bar) display.
    pub main: bool,
    /// Backing scale factor (for example `2.0` on Retina displays).
    pub scale: f64,
    /// Full display bounds in canonical coordinates.
    pub bounds: Rect,
    /// Usable area available to ordinary windows in canonical coordinates.
    pub usable: Rect,
}

impl Display {
    /// Returns whether the geometry is valid and the usable area lies within
    /// the full bounds (allowing for floating-point rounding).
    pub fn is_consistent(&self) -> bool {
        self.bounds.is_valid() && self.usable.is_valid() && contains_rect(self.bounds, self.usable)
    }
}

/// Returns the display matching `id`, if present.
pub fn find(displays: &[Display], id: DisplayId) -> Option<&Display> {
    displays.iter().find(|display| display.id == id)
}

/// Returns the main display, if any is flagged.
pub fn main_display(displays: &[Display]) -> Option<&Display> {
    displays.iter().find(|display| display.main)
}

fn contains_rect(outer: Rect, inner: Rect) -> bool {
    inner.origin.x >= outer.origin.x - CONTAINMENT_EPSILON
        && inner.origin.y >= outer.origin.y - CONTAINMENT_EPSILON
        && inner.origin.x + inner.size.width
            <= outer.origin.x + outer.size.width + CONTAINMENT_EPSILON
        && inner.origin.y + inner.size.height
            <= outer.origin.y + outer.size.height + CONTAINMENT_EPSILON
}

#[cfg(test)]
mod tests {
    use super::*;

    fn display(id: u32, main: bool, bounds: Rect, usable: Rect) -> Display {
        Display {
            id: DisplayId(id),
            name: Some(format!("Display {id}")),
            main,
            scale: 2.0,
            bounds,
            usable,
        }
    }

    #[test]
    fn parses_decimal_and_hex_ids() {
        assert_eq!(DisplayId::parse("69733248"), Ok(DisplayId(69733248)));
        assert_eq!(DisplayId::parse("0x10"), Ok(DisplayId(16)));
        assert_eq!(DisplayId::parse("0X10"), Ok(DisplayId(16)));
        assert!(DisplayId::parse("nope").is_err());
        assert!(DisplayId::parse("-1").is_err());
    }

    #[test]
    fn finds_display_by_id() {
        let displays = vec![
            display(
                1,
                true,
                Rect::new(0.0, 0.0, 1440.0, 900.0),
                Rect::new(0.0, 25.0, 1440.0, 875.0),
            ),
            display(
                2,
                false,
                Rect::new(1440.0, 0.0, 1920.0, 1080.0),
                Rect::new(1440.0, 0.0, 1920.0, 1080.0),
            ),
        ];
        assert_eq!(
            find(&displays, DisplayId(2)).map(|d| d.id),
            Some(DisplayId(2))
        );
        assert!(find(&displays, DisplayId(99)).is_none());
    }

    #[test]
    fn selects_main_display() {
        let displays = vec![
            display(
                2,
                false,
                Rect::new(-1920.0, 0.0, 1920.0, 1080.0),
                Rect::new(-1920.0, 0.0, 1920.0, 1080.0),
            ),
            display(
                1,
                true,
                Rect::new(0.0, 0.0, 1440.0, 900.0),
                Rect::new(0.0, 25.0, 1440.0, 875.0),
            ),
        ];
        assert_eq!(main_display(&displays).map(|d| d.id), Some(DisplayId(1)));
    }

    #[test]
    fn consistency_requires_usable_within_bounds() {
        // Menu bar reservation at the top.
        let good = display(
            1,
            true,
            Rect::new(0.0, 0.0, 1440.0, 900.0),
            Rect::new(0.0, 25.0, 1440.0, 875.0),
        );
        assert!(good.is_consistent());

        let overflowing = display(
            1,
            true,
            Rect::new(0.0, 0.0, 1440.0, 900.0),
            Rect::new(0.0, -10.0, 1440.0, 920.0),
        );
        assert!(!overflowing.is_consistent());
    }

    #[test]
    fn invalid_geometry_is_inconsistent() {
        let invalid = display(
            1,
            true,
            Rect::new(0.0, 0.0, 0.0, 900.0),
            Rect::new(0.0, 0.0, 1440.0, 875.0),
        );
        assert!(!invalid.is_consistent());
    }
}
