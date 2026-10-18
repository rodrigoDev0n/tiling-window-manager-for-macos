//! Directional zone movement with magnetic repulsion.
//!
//! Pure geometry: given the work area, the focused rectangle and the other
//! window rectangles, a direction moves the focused window into the matching
//! half of the work area and repels every window already occupying that half
//! into the opposite one. No native calls are performed.

use crate::core::action::Direction;
use crate::core::geometry::Rect;

/// The result of a zone move.
#[derive(Debug, Clone, PartialEq)]
pub struct MoveOutcome {
    /// New rectangle of the focused window.
    pub focused: Rect,
    /// Displaced windows: an index into the `others` slice and its target
    /// rectangle, in input order.
    pub displaced: Vec<(usize, Rect)>,
}

/// Computes the outcome of moving `focused` towards `direction`.
///
/// The focused window fills the half of `work_area` indicated by `direction`.
/// Every other rectangle that overlaps that half is repelled into the opposite
/// half: a single displaced window keeps its size and is mirrored across the
/// work-area centre; when several overlap the half they are stacked, in input
/// order, to fill the opposite one.
///
/// Returns `None` when the work area or the focused rectangle is invalid, or
/// when the move would leave every rectangle unchanged.
pub fn compute_move(
    work_area: Rect,
    focused: Rect,
    direction: Direction,
    others: &[Rect],
) -> Option<MoveOutcome> {
    if !work_area.is_valid() || !focused.is_valid() {
        return None;
    }

    let target = zone(work_area, direction);
    let opposite = zone(work_area, direction.opposite());

    let mut overlapping = Vec::new();
    for (index, rect) in others.iter().enumerate() {
        if rect.is_valid() && overlaps(*rect, target) {
            overlapping.push(index);
        }
    }

    if overlapping.is_empty() && focused == target {
        return None;
    }

    let displaced = if overlapping.len() <= 1 {
        overlapping
            .into_iter()
            .map(|index| (index, repel(others[index], work_area, opposite, direction)))
            .collect()
    } else {
        stack(&overlapping, opposite, direction)
    };

    Some(MoveOutcome {
        focused: target,
        displaced,
    })
}

/// The half of `work_area` that `direction` points to.
fn zone(work_area: Rect, direction: Direction) -> Rect {
    let half_width = work_area.size.width / 2.0;
    let half_height = work_area.size.height / 2.0;
    match direction {
        Direction::Left => Rect::new(
            work_area.origin.x,
            work_area.origin.y,
            half_width,
            work_area.size.height,
        ),
        Direction::Right => Rect::new(
            work_area.origin.x + half_width,
            work_area.origin.y,
            work_area.size.width - half_width,
            work_area.size.height,
        ),
        Direction::Up => Rect::new(
            work_area.origin.x,
            work_area.origin.y,
            work_area.size.width,
            half_height,
        ),
        Direction::Down => Rect::new(
            work_area.origin.x,
            work_area.origin.y + half_height,
            work_area.size.width,
            work_area.size.height - half_height,
        ),
    }
}

/// Mirrors `rect` across the work-area centre along the movement axis and
/// clamps it into the opposite half, shrinking it when it cannot fit.
fn repel(rect: Rect, work_area: Rect, opposite: Rect, direction: Direction) -> Rect {
    let width = rect.size.width.min(opposite.size.width);
    let height = rect.size.height.min(opposite.size.height);

    let (x, y) = match direction {
        Direction::Left | Direction::Right => (
            work_area.origin.x + work_area.size.width - (rect.origin.x + rect.size.width),
            rect.origin.y,
        ),
        Direction::Up | Direction::Down => (
            rect.origin.x,
            work_area.origin.y + work_area.size.height - (rect.origin.y + rect.size.height),
        ),
    };

    Rect::new(
        x.clamp(
            opposite.origin.x,
            opposite.origin.x + opposite.size.width - width,
        ),
        y.clamp(
            opposite.origin.y,
            opposite.origin.y + opposite.size.height - height,
        ),
        width,
        height,
    )
}

/// Fills the opposite half with the displaced windows, stacked in input order.
fn stack(indices: &[usize], opposite: Rect, direction: Direction) -> Vec<(usize, Rect)> {
    let count = indices.len() as f64;
    let mut placed = Vec::with_capacity(indices.len());

    match direction {
        Direction::Left | Direction::Right => {
            let base = opposite.size.height / count;
            for (position, &index) in indices.iter().enumerate() {
                let y = opposite.origin.y + position as f64 * base;
                let height = if position + 1 == indices.len() {
                    opposite.origin.y + opposite.size.height - y
                } else {
                    base
                };
                placed.push((
                    index,
                    Rect::new(opposite.origin.x, y, opposite.size.width, height),
                ));
            }
        }
        Direction::Up | Direction::Down => {
            let base = opposite.size.width / count;
            for (position, &index) in indices.iter().enumerate() {
                let x = opposite.origin.x + position as f64 * base;
                let width = if position + 1 == indices.len() {
                    opposite.origin.x + opposite.size.width - x
                } else {
                    base
                };
                placed.push((
                    index,
                    Rect::new(x, opposite.origin.y, width, opposite.size.height),
                ));
            }
        }
    }
    placed
}

/// Whether two rectangles share a strictly positive area (touching edges do not
/// count as an overlap).
fn overlaps(a: Rect, b: Rect) -> bool {
    a.origin.x < b.origin.x + b.size.width
        && b.origin.x < a.origin.x + a.size.width
        && a.origin.y < b.origin.y + b.size.height
        && b.origin.y < a.origin.y + a.size.height
}

#[cfg(test)]
mod tests {
    use super::*;

    const EPS: f64 = 1e-9;

    fn area() -> Rect {
        Rect::new(0.0, 25.0, 1440.0, 875.0)
    }

    fn assert_rect_eq(actual: Rect, expected: Rect) {
        assert!(
            (actual.origin.x - expected.origin.x).abs() < EPS,
            "{actual:?}"
        );
        assert!(
            (actual.origin.y - expected.origin.y).abs() < EPS,
            "{actual:?}"
        );
        assert!(
            (actual.size.width - expected.size.width).abs() < EPS,
            "{actual:?}"
        );
        assert!(
            (actual.size.height - expected.size.height).abs() < EPS,
            "{actual:?}"
        );
    }

    #[test]
    fn move_fills_the_target_half() {
        let outcome = compute_move(
            area(),
            Rect::new(8.0, 33.0, 400.0, 300.0),
            Direction::Left,
            &[],
        )
        .expect("move");
        assert_rect_eq(outcome.focused, Rect::new(0.0, 25.0, 720.0, 875.0));
        assert!(outcome.displaced.is_empty());
    }

    #[test]
    fn right_and_down_halves_are_the_complement_of_left_and_up() {
        let left = compute_move(
            area(),
            Rect::new(0.0, 0.0, 10.0, 10.0),
            Direction::Left,
            &[],
        );
        let right = compute_move(
            area(),
            Rect::new(0.0, 0.0, 10.0, 10.0),
            Direction::Right,
            &[],
        );
        let (left, right) = (left.expect("left"), right.expect("right"));
        assert_rect_eq(left.focused, Rect::new(0.0, 25.0, 720.0, 875.0));
        assert_rect_eq(right.focused, Rect::new(720.0, 25.0, 720.0, 875.0));
    }

    #[test]
    fn single_overlapping_window_is_repelled_to_the_opposite_half() {
        // Focused sits in the left half; another window owns the right half.
        let focused = Rect::new(8.0, 33.0, 708.0, 859.0);
        let other = Rect::new(724.0, 33.0, 708.0, 859.0);
        let outcome = compute_move(area(), focused, Direction::Right, &[other]).expect("move");

        assert_rect_eq(outcome.focused, Rect::new(720.0, 25.0, 720.0, 875.0));
        assert_eq!(outcome.displaced.len(), 1);
        let (index, displaced) = outcome.displaced[0];
        assert_eq!(index, 0);
        // Mirrored across the vertical centre of the work area.
        assert_rect_eq(displaced, Rect::new(8.0, 33.0, 708.0, 859.0));
    }

    #[test]
    fn repelled_window_keeps_its_size_when_smaller_than_the_half() {
        let small = Rect::new(1000.0, 100.0, 300.0, 200.0);
        let outcome = compute_move(
            area(),
            Rect::new(8.0, 33.0, 500.0, 400.0),
            Direction::Right,
            &[small],
        )
        .expect("move");

        let (_, displaced) = outcome.displaced[0];
        assert!((displaced.size.width - 300.0).abs() < EPS);
        assert!((displaced.size.height - 200.0).abs() < EPS);
        assert!((displaced.origin.x - 140.0).abs() < EPS);
        assert!(displaced.origin.x + displaced.size.width <= outcome.focused.origin.x + EPS);
    }

    #[test]
    fn several_overlapping_windows_fill_the_opposite_half_in_order() {
        let first = Rect::new(720.0, 33.0, 712.0, 400.0);
        let second = Rect::new(720.0, 440.0, 712.0, 452.0);
        let outcome = compute_move(
            area(),
            Rect::new(8.0, 33.0, 700.0, 859.0),
            Direction::Right,
            &[first, second],
        )
        .expect("move");

        assert_eq!(outcome.displaced.len(), 2);
        let (index_a, a) = outcome.displaced[0];
        let (index_b, b) = outcome.displaced[1];
        assert_eq!((index_a, index_b), (0, 1));
        assert_rect_eq(a, Rect::new(0.0, 25.0, 720.0, 437.5));
        assert_rect_eq(b, Rect::new(0.0, 462.5, 720.0, 437.5));
        assert!((a.origin.y + a.size.height - b.origin.y).abs() < EPS);
    }

    #[test]
    fn vertical_moves_mirror_across_the_horizontal_centre() {
        let top = Rect::new(8.0, 33.0, 708.0, 420.0);
        let outcome = compute_move(
            area(),
            Rect::new(8.0, 470.0, 708.0, 420.0),
            Direction::Up,
            &[top],
        )
        .expect("move");

        assert_rect_eq(outcome.focused, Rect::new(0.0, 25.0, 1440.0, 437.5));
        let (_, displaced) = outcome.displaced[0];
        assert!(displaced.origin.y >= outcome.focused.origin.y + outcome.focused.size.height - EPS);
    }

    #[test]
    fn a_move_that_changes_nothing_returns_none() {
        let focused = Rect::new(0.0, 25.0, 720.0, 875.0);
        let other = Rect::new(724.0, 33.0, 708.0, 859.0);
        assert_eq!(
            compute_move(area(), focused, Direction::Left, &[other]),
            None
        );
    }

    #[test]
    fn invalid_inputs_are_rejected() {
        let bad = Rect::new(f64::NAN, 0.0, 100.0, 100.0);
        assert_eq!(compute_move(bad, bad, Direction::Left, &[]), None);
        assert_eq!(compute_move(area(), bad, Direction::Left, &[]), None);
        // An invalid neighbour is ignored instead of displacing anything.
        let outcome = compute_move(
            area(),
            Rect::new(8.0, 33.0, 708.0, 859.0),
            Direction::Left,
            &[bad],
        )
        .expect("move");
        assert!(outcome.displaced.is_empty());
    }

    #[test]
    fn touching_edges_are_not_displaced() {
        let touching = Rect::new(720.0, 33.0, 716.0, 859.0);
        let outcome = compute_move(
            area(),
            Rect::new(8.0, 33.0, 708.0, 859.0),
            Direction::Left,
            &[touching],
        )
        .expect("move");
        assert!(outcome.displaced.is_empty());
    }

    #[test]
    fn negative_origin_areas_are_handled() {
        let work = Rect::new(-1920.0, -180.0, 1920.0, 1080.0);
        let other = Rect::new(-960.0, -180.0, 960.0, 1080.0);
        let outcome = compute_move(
            work,
            Rect::new(-1920.0, -180.0, 960.0, 1080.0),
            Direction::Right,
            &[other],
        )
        .expect("move");

        assert_rect_eq(outcome.focused, Rect::new(-960.0, -180.0, 960.0, 1080.0));
        let (_, displaced) = outcome.displaced[0];
        assert_rect_eq(displaced, Rect::new(-1920.0, -180.0, 960.0, 1080.0));
    }
}
