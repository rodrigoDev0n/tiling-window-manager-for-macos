//! Equal-width column layout.

use crate::core::geometry::Rect;
use crate::core::layout::{LayoutConfig, LayoutError, PlacedWindow};
use crate::core::window::WindowId;

use super::{content_area, partition_axis};

/// Arranges every window into an equal-width column separated by inner gaps.
pub(super) fn arrange(
    windows: &[WindowId],
    area: Rect,
    config: LayoutConfig,
) -> Result<Vec<PlacedWindow>, LayoutError> {
    let content = content_area(area, config.outer_gap)?;
    let columns = partition_axis(
        content.origin.x,
        content.size.width,
        windows.len(),
        config.inner_gap,
    )?;

    let mut placed = Vec::with_capacity(windows.len());
    for (window, (x, width)) in windows.iter().zip(columns) {
        placed.push(PlacedWindow {
            window: *window,
            rect: Rect::new(x, content.origin.y, width, content.size.height),
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
    fn single_window_fills_content_area() {
        let area = Rect::new(0.0, 25.0, 1440.0, 875.0);
        let config = LayoutConfig {
            layout: LayoutType::Columns,
            ..LayoutConfig::default()
        };
        let result = calculate(&ids(1), area, config).expect("layout");
        let rect = result.windows[0].rect;
        assert!((rect.origin.x - 8.0).abs() < EPS);
        assert!((rect.origin.y - 33.0).abs() < EPS);
        assert!((rect.size.width - 1424.0).abs() < EPS);
        assert!((rect.size.height - 859.0).abs() < EPS);
    }

    #[test]
    fn two_columns_are_equal_and_gapped() {
        let area = Rect::new(0.0, 25.0, 1440.0, 875.0);
        let config = LayoutConfig {
            layout: LayoutType::Columns,
            ..LayoutConfig::default()
        };
        let result = calculate(&ids(2), area, config).expect("layout");
        let first = result.windows[0].rect;
        let second = result.windows[1].rect;

        assert!((first.size.width - second.size.width).abs() < EPS);
        assert!((first.size.height - second.size.height).abs() < EPS);
        assert_eq!(first.origin.y, second.origin.y);
        assert!((second.origin.x - (first.origin.x + first.size.width + 8.0)).abs() < EPS);
        // The last column lands exactly on the right edge of the content area.
        assert!(
            (second.origin.x + second.size.width - 1432.0).abs() < EPS,
            "right edge {:.3}",
            second.origin.x + second.size.width
        );
    }

    #[test]
    fn three_columns_split_evenly_with_remainder_in_last() {
        let area = Rect::new(0.0, 25.0, 1440.0, 875.0);
        let config = LayoutConfig {
            layout: LayoutType::Columns,
            ..LayoutConfig::default()
        };
        let result = calculate(&ids(3), area, config).expect("layout");
        let widths: Vec<f64> = result.windows.iter().map(|w| w.rect.size.width).collect();
        assert!((widths[0] - widths[1]).abs() < EPS);
        assert!((widths[1] - widths[2]).abs() < EPS);
        let last = result.windows[2].rect;
        assert!(
            (last.origin.x + last.size.width - 1432.0).abs() < EPS,
            "right edge {:.3}",
            last.origin.x + last.size.width
        );
    }

    #[test]
    fn zero_gaps_reuse_every_pixel() {
        let area = Rect::new(0.0, 25.0, 100.0, 100.0);
        let config = LayoutConfig {
            outer_gap: 0.0,
            inner_gap: 0.0,
            layout: LayoutType::Columns,
            ..LayoutConfig::default()
        };
        let result = calculate(&ids(2), area, config).expect("layout");
        let first = result.windows[0].rect;
        let second = result.windows[1].rect;
        let third_expected = first.origin.x + first.size.width;
        assert_eq!(second.origin.x, third_expected);
        assert_eq!(second.origin.x + second.size.width, 100.0);
    }
}
