//! Monocle layout: every window receives the same full-content rectangle.

use crate::core::geometry::Rect;
use crate::core::layout::{LayoutConfig, LayoutError, PlacedWindow};
use crate::core::window::WindowId;

use super::content_area;

/// Assigns the full usable content area to every window, ignoring inner gaps.
pub(super) fn arrange(
    windows: &[WindowId],
    area: Rect,
    config: LayoutConfig,
) -> Result<Vec<PlacedWindow>, LayoutError> {
    let content = content_area(area, config.outer_gap)?;
    Ok(windows
        .iter()
        .map(|window| PlacedWindow {
            window: *window,
            rect: content,
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::layout::{LayoutType, calculate};

    #[test]
    fn every_window_receives_an_identical_rectangle() {
        let area = Rect::new(0.0, 25.0, 1440.0, 875.0);
        let config = LayoutConfig {
            inner_gap: 33.0,
            layout: LayoutType::Monocle,
            ..LayoutConfig::default()
        };
        let result =
            calculate(&[WindowId(1), WindowId(2), WindowId(3)], area, config).expect("layout");
        let reference = result.windows[0].rect;
        for placement in &result.windows {
            assert_eq!(placement.rect, reference);
        }
        // Inner gaps are ignored; only outer gaps apply.
        assert_eq!(reference.origin.x, 8.0);
        assert_eq!(reference.origin.y, 33.0);
        assert_eq!(reference.size.width, 1424.0);
        assert_eq!(reference.size.height, 859.0);
    }
}
