//! Equal-height row layout.

use crate::core::geometry::Rect;
use crate::core::layout::{LayoutConfig, LayoutError, PlacedWindow};
use crate::core::window::WindowId;

use super::{content_area, partition_axis};

/// Arranges every window into an equal-height row separated by inner gaps.
pub(super) fn arrange(
    windows: &[WindowId],
    area: Rect,
    config: LayoutConfig,
) -> Result<Vec<PlacedWindow>, LayoutError> {
    let content = content_area(area, config.outer_gap)?;
    let rows = partition_axis(
        content.origin.y,
        content.size.height,
        windows.len(),
        config.inner_gap,
    )?;

    let mut placed = Vec::with_capacity(windows.len());
    for (window, (y, height)) in windows.iter().zip(rows) {
        placed.push(PlacedWindow {
            window: *window,
            rect: Rect::new(content.origin.x, y, content.size.width, height),
        });
    }
    Ok(placed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::layout::{LayoutType, calculate};

    fn ids(count: usize) -> Vec<WindowId> {
        (0..count).map(|index| WindowId(index as u64)).collect()
    }

    const EPS: f64 = 1e-9;

    #[test]
    fn two_rows_are_equal_and_gapped() {
        let area = Rect::new(0.0, 25.0, 1440.0, 875.0);
        let config = LayoutConfig {
            layout: LayoutType::Rows,
            ..LayoutConfig::default()
        };
        let result = calculate(&ids(2), area, config).expect("layout");
        let first = result.windows[0].rect;
        let second = result.windows[1].rect;

        assert!((first.size.height - second.size.height).abs() < EPS);
        assert_eq!(first.origin.x, second.origin.x);
        assert!((second.origin.y - (first.origin.y + first.size.height + 8.0)).abs() < EPS);
        // The last row lands exactly on the bottom edge of the content area.
        assert!(
            (second.origin.y + second.size.height - 892.0).abs() < EPS,
            "bottom edge {:.3}",
            second.origin.y + second.size.height
        );
    }

    #[test]
    fn rows_preserve_window_order_top_to_bottom() {
        let area = Rect::new(0.0, 25.0, 1440.0, 875.0);
        let config = LayoutConfig {
            layout: LayoutType::Rows,
            ..LayoutConfig::default()
        };
        let result = calculate(&ids(3), area, config).expect("layout");
        for pair in result.windows.windows(2) {
            assert!(pair[0].rect.origin.y < pair[1].rect.origin.y);
        }
    }
}
