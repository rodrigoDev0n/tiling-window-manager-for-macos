//! Platform-independent actions triggered by global shortcuts (SPEC-008).
//!
//! The action model contains no native types, so the run loop, the binding
//! table, and the overlay reference can share it without touching macOS APIs.

use crate::core::layout::LayoutType;

/// An operation a global shortcut can request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Show or hide the shortcut overlay.
    ToggleOverlay,
    /// Apply a tiling layout to the focused display.
    SetLayout(LayoutType),
    /// Move focus to the directional neighbour of the focused window.
    Focus(Direction),
    /// Swap the focused window's frame with its directional neighbour.
    MoveWindow(Direction),
    /// Enter or leave the interactive move mode.
    ToggleMoveMode,
    /// Toggle native fullscreen on the focused window.
    ToggleFullscreen,
}

/// A screen direction used by focus and move-window actions.
///
/// Coordinates follow the macOS convention: the origin is the top-left corner,
/// the X axis grows rightward, and the Y axis grows downward.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Left,
    Right,
    Up,
    Down,
}

impl Direction {
    /// The direction opposite to `self`.
    pub const fn opposite(self) -> Self {
        match self {
            Direction::Left => Direction::Right,
            Direction::Right => Direction::Left,
            Direction::Up => Direction::Down,
            Direction::Down => Direction::Up,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn actions_are_copyable_and_equatable() {
        let a = Action::SetLayout(LayoutType::Columns);
        let b = a;
        assert_eq!(a, b);
        assert_ne!(a, Action::Focus(Direction::Left));
    }

    #[test]
    fn directions_are_distinct_values() {
        assert_ne!(Direction::Left, Direction::Right);
        assert_ne!(Direction::Up, Direction::Down);
    }

    #[test]
    fn opposite_directions_are_symmetric() {
        for direction in [
            Direction::Left,
            Direction::Right,
            Direction::Up,
            Direction::Down,
        ] {
            assert_eq!(direction.opposite().opposite(), direction);
        }
        assert_ne!(Direction::Left.opposite(), Direction::Left);
    }
}
