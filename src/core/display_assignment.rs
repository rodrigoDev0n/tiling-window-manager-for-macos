//! Pure window-to-display assignment.
//!
//! Uses the SPEC-005 default strategy: assign each window to the display with
//! the largest intersection area between the window rectangle and the display's
//! full bounds. Ties prefer the display containing the window center, falling
//! back to the display with the smallest runtime identifier.

use crate::core::display::{Display, DisplayId};
use crate::core::geometry::{Point, Rect};

/// Relative tolerance used when comparing intersection areas.
const AREA_EPSILON: f64 = 1e-9;

/// Assigns `window` to a display using the SPEC-005 default strategy.
///
/// Returns `None` when the window has invalid geometry, no display intersects
/// it, or there are no active displays.
pub fn assign(displays: &[Display], window: Rect) -> Option<DisplayId> {
    if !window.is_valid() {
        return None;
    }

    let largest = displays
        .iter()
        .map(|display| intersection_area(window, display.bounds))
        .max_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))?;

    let tied: Vec<&Display> = displays
        .iter()
        .filter(|display| {
            let area = intersection_area(window, display.bounds);
            area > 0.0 && (area - largest).abs() <= AREA_EPSILON
        })
        .collect();

    if tied.is_empty() {
        return None;
    }

    let centre = Point::new(
        window.origin.x + window.size.width / 2.0,
        window.origin.y + window.size.height / 2.0,
    );
    let containing_centre: Vec<&Display> = tied
        .iter()
        .copied()
        .filter(|display| contains_point(display.bounds, centre))
        .collect();

    // Prefer a single display that contains the window center; otherwise fall
    // back to the smallest display identifier for a deterministic result.
    if containing_centre.len() == 1 {
        return Some(containing_centre[0].id);
    }
    tied.into_iter().map(|display| display.id).min()
}

/// Area of the intersection between two axis-aligned rectangles.
pub fn intersection_area(a: Rect, b: Rect) -> f64 {
    let x_overlap =
        (a.origin.x + a.size.width).min(b.origin.x + b.size.width) - a.origin.x.max(b.origin.x);
    let y_overlap =
        (a.origin.y + a.size.height).min(b.origin.y + b.size.height) - a.origin.y.max(b.origin.y);
    if x_overlap <= 0.0 || y_overlap <= 0.0 {
        0.0
    } else {
        x_overlap * y_overlap
    }
}

fn contains_point(rect: Rect, point: Point) -> bool {
    point.x >= rect.origin.x
        && point.x < rect.origin.x + rect.size.width
        && point.y >= rect.origin.y
        && point.y < rect.origin.y + rect.size.height
}

#[cfg(test)]
mod tests {
    use super::*;

    fn display(id: u32, bounds: Rect) -> Display {
        Display {
            id: DisplayId(id),
            name: None,
            main: id == 1,
            scale: 2.0,
            bounds,
            usable: bounds,
        }
    }

    #[test]
    fn assigns_window_fully_inside_single_display() {
        let displays = vec![display(1, Rect::new(0.0, 0.0, 100.0, 100.0))];
        let window = Rect::new(10.0, 10.0, 50.0, 50.0);
        assert_eq!(assign(&displays, window), Some(DisplayId(1)));
    }

    #[test]
    fn assigns_window_wholly_on_second_display() {
        let displays = vec![
            display(1, Rect::new(0.0, 0.0, 100.0, 100.0)),
            display(2, Rect::new(100.0, 0.0, 100.0, 100.0)),
        ];
        let window = Rect::new(120.0, 10.0, 50.0, 50.0);
        assert_eq!(assign(&displays, window), Some(DisplayId(2)));
    }

    #[test]
    fn tie_uses_display_containing_window_center() {
        // Window spans both displays equally; its center falls on display 2.
        let displays = vec![
            display(1, Rect::new(0.0, 0.0, 100.0, 100.0)),
            display(2, Rect::new(100.0, 0.0, 100.0, 100.0)),
        ];
        let window = Rect::new(50.0, 0.0, 100.0, 100.0);
        assert_eq!(assign(&displays, window), Some(DisplayId(2)));
    }

    #[test]
    fn unresolved_tie_falls_back_to_smallest_display_id() {
        // Overlapping displays make both ties and center containment ambiguous.
        let displays = vec![
            display(2, Rect::new(0.0, 0.0, 100.0, 100.0)),
            display(5, Rect::new(0.0, 0.0, 100.0, 100.0)),
        ];
        let window = Rect::new(10.0, 10.0, 50.0, 50.0);
        assert_eq!(assign(&displays, window), Some(DisplayId(2)));
    }

    #[test]
    fn supports_negative_coordinates() {
        let displays = vec![display(1, Rect::new(-1920.0, 0.0, 1920.0, 1080.0))];
        let window = Rect::new(-1900.0, 100.0, 500.0, 500.0);
        assert_eq!(assign(&displays, window), Some(DisplayId(1)));
    }

    #[test]
    fn no_intersection_excludes_window() {
        let displays = vec![display(1, Rect::new(0.0, 0.0, 100.0, 100.0))];
        let window = Rect::new(500.0, 500.0, 50.0, 50.0);
        assert_eq!(assign(&displays, window), None);
    }

    #[test]
    fn edge_touching_window_has_zero_area_and_is_excluded() {
        let displays = vec![display(1, Rect::new(0.0, 0.0, 100.0, 100.0))];
        // Shares only the x = 100 edge with the display.
        let window = Rect::new(100.0, 0.0, 50.0, 50.0);
        assert_eq!(assign(&displays, window), None);
    }

    #[test]
    fn empty_display_collection_returns_none() {
        assert_eq!(assign(&[], Rect::new(0.0, 0.0, 50.0, 50.0)), None);
    }

    #[test]
    fn invalid_window_returns_none() {
        let displays = vec![display(1, Rect::new(0.0, 0.0, 100.0, 100.0))];
        assert_eq!(assign(&displays, Rect::new(0.0, 0.0, 0.0, 0.0)), None);
        assert_eq!(
            assign(&displays, Rect::new(f64::NAN, 0.0, 10.0, 10.0)),
            None
        );
    }

    #[test]
    fn intersection_area_is_normalized() {
        let a = Rect::new(0.0, 0.0, 100.0, 100.0);
        let b = Rect::new(50.0, 50.0, 100.0, 100.0);
        assert!((intersection_area(a, b) - 2500.0).abs() < AREA_EPSILON);
        let disjoint = Rect::new(200.0, 200.0, 10.0, 10.0);
        assert_eq!(intersection_area(a, disjoint), 0.0);
    }
}
