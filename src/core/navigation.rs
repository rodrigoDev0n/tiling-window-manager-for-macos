//! Pure directional window navigation (SPEC-008 FR-006).
//!
//! Given the current geometry of every window on a display and the index of the
//! focused window, this module selects the neighbouring window in a requested
//! direction. It performs no native calls and is fully testable.

use crate::core::action::Direction;
use crate::core::geometry::{Point, Rect};

/// Selects the index (into `rects`) of the window nearest to `rects[current]`
/// in `direction`.
///
/// Candidates must lie strictly in the requested direction, measured from the
/// centre of the reference rectangle along the primary axis. The closest
/// candidate by primary-axis distance wins; ties are broken by the smallest
/// perpendicular offset, then by the smallest index (which is guaranteed by
/// iterating in order and only replacing strictly better candidates).
///
/// Returns `None` when `current` is out of bounds, the reference rectangle is
/// invalid, or no candidate lies in the requested direction.
pub fn directional_neighbor(rects: &[Rect], current: usize, direction: Direction) -> Option<usize> {
    let reference = *rects.get(current)?;
    if !reference.is_valid() {
        return None;
    }

    let current_centre = centre(reference);
    let mut best: Option<(f64, f64, usize)> = None;

    for (index, rect) in rects.iter().enumerate() {
        if index == current || !rect.is_valid() {
            continue;
        }
        let candidate_centre = centre(*rect);
        let (primary, secondary) = match direction {
            Direction::Left => (
                current_centre.x - candidate_centre.x,
                (candidate_centre.y - current_centre.y).abs(),
            ),
            Direction::Right => (
                candidate_centre.x - current_centre.x,
                (candidate_centre.y - current_centre.y).abs(),
            ),
            Direction::Up => (
                current_centre.y - candidate_centre.y,
                (candidate_centre.x - current_centre.x).abs(),
            ),
            Direction::Down => (
                candidate_centre.y - current_centre.y,
                (candidate_centre.x - current_centre.x).abs(),
            ),
        };

        if primary <= 0.0 {
            continue;
        }

        let is_better = best.is_none_or(|(best_primary, best_secondary, _)| {
            (primary, secondary) < (best_primary, best_secondary)
        });
        if is_better {
            best = Some((primary, secondary, index));
        }
    }

    best.map(|(_, _, index)| index)
}

fn centre(rect: Rect) -> Point {
    Point::new(
        rect.origin.x + rect.size.width / 2.0,
        rect.origin.y + rect.size.height / 2.0,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tile(x: f64, y: f64, width: f64, height: f64) -> Rect {
        Rect::new(x, y, width, height)
    }

    #[test]
    fn left_selects_the_nearest_window_to_the_left() {
        let rects = [
            tile(0.0, 25.0, 400.0, 875.0),   // far left
            tile(400.0, 25.0, 400.0, 875.0), // near left (focused)
            tile(800.0, 25.0, 400.0, 875.0), // right
            tile(200.0, 25.0, 150.0, 200.0), // also to the left, closer?
        ];
        let index = directional_neighbor(&rects, 1, Direction::Left).expect("neighbor");
        assert_eq!(index, 3);
    }

    #[test]
    fn right_selects_the_nearest_window_to_the_right() {
        let rects = [
            tile(0.0, 25.0, 400.0, 875.0),
            tile(400.0, 25.0, 400.0, 875.0), // focused
            tile(820.0, 25.0, 400.0, 875.0),
            tile(1800.0, 25.0, 400.0, 875.0),
        ];
        assert_eq!(directional_neighbor(&rects, 1, Direction::Right), Some(2));
    }

    #[test]
    fn up_and_down_select_by_vertical_axis() {
        let rects = [
            tile(0.0, 0.0, 400.0, 250.0),
            tile(0.0, 250.0, 400.0, 250.0), // focused
            tile(0.0, 500.0, 400.0, 250.0),
        ];
        assert_eq!(directional_neighbor(&rects, 1, Direction::Up), Some(0));
        assert_eq!(directional_neighbor(&rects, 1, Direction::Down), Some(2));
    }

    #[test]
    fn no_candidate_in_direction_returns_none() {
        let rects = [tile(0.0, 25.0, 400.0, 875.0), tile(0.0, 25.0, 400.0, 875.0)];
        assert_eq!(directional_neighbor(&rects, 1, Direction::Right), None);
        assert_eq!(directional_neighbor(&rects, 0, Direction::Left), None);
    }

    #[test]
    fn overlapping_windows_pick_primary_axis_first() {
        // Primary-axis distance dominates the perpendicular offset: a window
        // far off the vertical axis wins when it is much closer horizontally.
        let rects = [
            tile(0.0, 300.0, 100.0, 275.0),   // on the same y line as focus
            tile(300.0, 0.0, 200.0, 875.0),   // focused (centre x = 400)
            tile(130.0, 8995.0, 100.0, 10.0), // much closer, far above
        ];
        assert_eq!(directional_neighbor(&rects, 1, Direction::Left), Some(2));
    }

    #[test]
    fn ties_prefer_the_smaller_index() {
        let rects = [
            tile(0.0, 0.0, 400.0, 875.0),
            tile(600.0, 25.0, 400.0, 875.0), // focused
            tile(400.0, 25.0, 400.0, 875.0), // equidistant on the left
            tile(320.0, 0.0, 400.0, 875.0),  // the other equidistant candidate
        ];
        // Both index 2 and 3 sit on the same y centre, left at the same distance.
        assert_eq!(directional_neighbor(&rects, 1, Direction::Left), Some(2));
    }

    #[test]
    fn invalid_or_out_of_bounds_returns_none() {
        let rects = [tile(0.0, 25.0, 400.0, 875.0)];
        assert_eq!(directional_neighbor(&rects, 5, Direction::Left), None);
        let invalid = [tile(0.0, 25.0, 0.0, 0.0), tile(100.0, 25.0, 400.0, 875.0)];
        assert_eq!(directional_neighbor(&invalid, 0, Direction::Right), None);
    }

    #[test]
    fn empty_collection_returns_none() {
        assert_eq!(directional_neighbor(&[], 0, Direction::Left), None);
    }
}
