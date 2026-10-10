//! Platform-independent layout primitives.
//!
//! These helpers turn a screen work area and the current window rectangle into
//! a target rectangle. They perform no native calls and are fully testable.

use crate::core::geometry::{Rect, Size};

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
}
