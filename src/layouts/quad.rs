//! Quad layout: two columns with the windows stacked inside each column.

use crate::core::geometry::Rect;
use crate::core::layout::{LayoutConfig, LayoutError, PlacedWindow};
use crate::core::window::WindowId;

use super::{content_area, partition_axis};

/// Arranges the windows into two equal-width columns, splitting the collection
/// into a leading half and a trailing half so the leading windows land on the
/// left column and the rest on the right one.
///
/// With four windows this yields two windows stacked on the left and two
/// stacked on the right. A single window fills the whole content area.
pub(super) fn arrange(
    windows: &[WindowId],
    area: Rect,
    config: LayoutConfig,
) -> Result<Vec<PlacedWindow>, LayoutError> {
    let content = content_area(area, config.outer_gap)?;
    let count = windows.len();

    if count == 1 {
        return Ok(vec![PlacedWindow {
            window: windows[0],
            rect: content,
        }]);
    }

    let columns = partition_axis(content.origin.x, content.size.width, 2, config.inner_gap)?;
    let left_count = count.div_ceil(2);
    let group_sizes = [left_count, count - left_count];

    let mut placed = Vec::with_capacity(count);
    let mut start = 0;
    for (index, size) in group_sizes.into_iter().enumerate() {
        if size == 0 {
            continue;
        }
        let (x, width) = columns[index];
        let rows = partition_axis(
            content.origin.y,
            content.size.height,
            size,
            config.inner_gap,
        )?;
        for (window, (y, height)) in windows[start..start + size].iter().zip(rows) {
            placed.push(PlacedWindow {
                window: *window,
                rect: Rect::new(x, y, width, height),
            });
        }
        start += size;
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

    fn area() -> Rect {
        Rect::new(0.0, 25.0, 1440.0, 875.0)
    }

    fn quad_config() -> LayoutConfig {
        LayoutConfig {
            layout: LayoutType::Quad,
            ..LayoutConfig::default()
        }
    }

    #[test]
    fn single_window_fills_content_area() {
        let result = calculate(&ids(1), area(), quad_config()).expect("layout");
        let rect = result.windows[0].rect;
        assert!((rect.origin.x - 8.0).abs() < EPS);
        assert!((rect.origin.y - 33.0).abs() < EPS);
        assert!((rect.size.width - 1424.0).abs() < EPS);
        assert!((rect.size.height - 859.0).abs() < EPS);
    }

    #[test]
    fn four_windows_split_two_left_two_right() {
        let config = quad_config();
        let result = calculate(&ids(4), area(), config).expect("layout");
        let rects: Vec<Rect> = result
            .windows
            .iter()
            .map(|placement| placement.rect)
            .collect();

        assert!((rects[0].origin.x - rects[1].origin.x).abs() < EPS);
        assert!((rects[2].origin.x - rects[3].origin.x).abs() < EPS);
        assert!(rects[0].origin.x < rects[2].origin.x);
        assert!((rects[0].size.width - rects[2].size.width).abs() < EPS);

        // The left column stacks its two windows with the inner gap.
        assert!(
            (rects[1].origin.y - (rects[0].origin.y + rects[0].size.height + config.inner_gap))
                .abs()
                < EPS
        );
        // The right column does the same for its two windows.
        assert!(
            (rects[3].origin.y - (rects[2].origin.y + rects[2].size.height + config.inner_gap))
                .abs()
                < EPS
        );

        // Both columns end exactly on the content right edge.
        assert!((rects[3].origin.x + rects[3].size.width - 1432.0).abs() < EPS);
        assert!((rects[1].origin.y + rects[1].size.height - 892.0).abs() < EPS);
    }

    #[test]
    fn three_windows_fill_the_left_column_first() {
        let result = calculate(&ids(3), area(), quad_config()).expect("layout");
        let left: Vec<Rect> = result.windows[..2].iter().map(|p| p.rect).collect();
        let right = result.windows[2].rect;

        assert!((left[0].origin.x - left[1].origin.x).abs() < EPS);
        assert!(left[0].origin.x < right.origin.x);
        assert!((right.size.height - 859.0).abs() < EPS);
        assert!((left[0].size.width - right.size.width).abs() < EPS);
    }

    #[test]
    fn two_windows_split_into_columns() {
        let result = calculate(&ids(2), area(), quad_config()).expect("layout");
        let first = result.windows[0].rect;
        let second = result.windows[1].rect;
        assert!((first.size.width - second.size.width).abs() < EPS);
        assert!((first.size.height - second.size.height).abs() < EPS);
        assert!((second.origin.x - (first.origin.x + first.size.width + 8.0)).abs() < EPS);
    }

    #[test]
    fn zero_gaps_tile_without_residual_space() {
        let config = LayoutConfig {
            layout: LayoutType::Quad,
            outer_gap: 0.0,
            inner_gap: 0.0,
            ..LayoutConfig::default()
        };
        let result = calculate(&ids(4), Rect::new(0.0, 0.0, 100.0, 100.0), config).expect("layout");
        let top_left = result.windows[0].rect;
        let bottom_left = result.windows[1].rect;
        let top_right = result.windows[2].rect;

        assert_eq!(top_left.origin.x, 0.0);
        assert_eq!(top_left.origin.y, 0.0);
        assert_eq!(
            bottom_left.origin.y,
            top_left.origin.y + top_left.size.height
        );
        assert_eq!(top_right.origin.x, top_left.origin.x + top_left.size.width);
        assert_eq!(top_right.origin.x + top_right.size.width, 100.0);
        let bottom_right = result.windows[3].rect;
        assert_eq!(bottom_right.origin.y + bottom_right.size.height, 100.0);
    }
}
