//! Master-stack layout: a left-hand master column plus a right-hand stack.

use crate::core::geometry::Rect;
use crate::core::layout::{LayoutConfig, LayoutError, PlacedWindow};
use crate::core::window::WindowId;

use super::{content_area, partition_axis};

/// Arranges the first `master_count` windows into a left master column and the
/// remaining windows into a right stack column.
///
/// When every window fits into the master group, the whole content area is
/// used for the master column.
pub(super) fn arrange(
    windows: &[WindowId],
    area: Rect,
    config: LayoutConfig,
) -> Result<Vec<PlacedWindow>, LayoutError> {
    let content = content_area(area, config.outer_gap)?;
    let count = windows.len();
    let masters = config.master_count.min(count);

    if masters == count {
        let rows = partition_axis(
            content.origin.y,
            content.size.height,
            count,
            config.inner_gap,
        )?;
        return Ok(windows
            .iter()
            .zip(rows)
            .map(|(window, (y, height))| PlacedWindow {
                window: *window,
                rect: Rect::new(content.origin.x, y, content.size.width, height),
            })
            .collect());
    }

    let master_width = content.size.width * config.master_ratio;
    let stack_width = content.size.width - master_width - config.inner_gap;
    if stack_width <= 0.0 {
        return Err(LayoutError::InsufficientSpace {
            available: content.size.width,
            required: master_width + config.inner_gap,
        });
    }

    let master_rows = partition_axis(
        content.origin.y,
        content.size.height,
        masters,
        config.inner_gap,
    )?;
    let stack_rows = partition_axis(
        content.origin.y,
        content.size.height,
        count - masters,
        config.inner_gap,
    )?;

    let stack_x = content.origin.x + master_width + config.inner_gap;
    let mut placed = Vec::with_capacity(count);

    for (window, (y, height)) in windows[..masters].iter().zip(master_rows) {
        placed.push(PlacedWindow {
            window: *window,
            rect: Rect::new(content.origin.x, y, master_width, height),
        });
    }
    for (window, (y, height)) in windows[masters..].iter().zip(stack_rows) {
        placed.push(PlacedWindow {
            window: *window,
            rect: Rect::new(stack_x, y, stack_width, height),
        });
    }
    Ok(placed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::layout::calculate;

    fn ids(count: usize) -> Vec<WindowId> {
        (0..count).map(|index| WindowId(index as u64)).collect()
    }

    const EPS: f64 = 1e-9;

    fn area() -> Rect {
        Rect::new(0.0, 25.0, 1440.0, 875.0)
    }

    #[test]
    fn single_window_fills_content_area() {
        let result = calculate(&ids(1), area(), LayoutConfig::default()).expect("layout");
        let rect = result.windows[0].rect;
        assert!((rect.origin.x - 8.0).abs() < EPS);
        assert!((rect.size.width - 1424.0).abs() < EPS);
    }

    #[test]
    fn master_count_greater_than_windows_uses_full_width() {
        let config = LayoutConfig {
            master_count: 5,
            ..LayoutConfig::default()
        };
        let result = calculate(&ids(3), area(), config).expect("layout");
        for placement in &result.windows {
            assert!((placement.rect.size.width - 1424.0).abs() < EPS);
        }
    }

    #[test]
    fn master_and_stack_split_horizontally() {
        let config = LayoutConfig::default();
        let result = calculate(&ids(3), area(), config).expect("layout");

        let master = result.windows[0].rect;
        let stack = result.windows[1].rect;
        let expected_master_width = 1424.0 * config.master_ratio;
        assert!((master.size.width - expected_master_width).abs() < EPS);
        assert!(
            (stack.origin.x - (master.origin.x + master.size.width + config.inner_gap)).abs() < EPS
        );
        // The stack column ends exactly on the content right edge.
        assert!((stack.origin.x + stack.size.width - 1432.0).abs() < EPS);
    }

    #[test]
    fn master_windows_stack_vertically() {
        let config = LayoutConfig {
            master_count: 2,
            ..LayoutConfig::default()
        };
        let result = calculate(&ids(3), area(), config).expect("layout");
        let first = result.windows[0].rect;
        let second = result.windows[1].rect;
        assert!(
            (second.origin.y - (first.origin.y + first.size.height + config.inner_gap)).abs() < EPS
        );
        // The final stack window occupies the whole content height.
        let stack = result.windows[2].rect;
        assert!((stack.size.width - (1424.0 - 1424.0 * 0.55 - 8.0)).abs() < EPS);
        assert!((stack.size.height - 859.0).abs() < EPS);
    }

    #[test]
    fn insufficient_width_for_stack_is_reported() {
        let narrow = Rect::new(0.0, 0.0, 20.0, 100.0);
        let config = LayoutConfig {
            outer_gap: 0.0,
            inner_gap: 8.0,
            master_ratio: 0.95,
            ..LayoutConfig::default()
        };
        assert!(matches!(
            calculate(&ids(2), narrow, config),
            Err(LayoutError::InsufficientSpace { .. })
        ));
    }

    #[test]
    fn zero_gaps_tile_without_residual_space() {
        let config = LayoutConfig {
            outer_gap: 0.0,
            inner_gap: 0.0,
            ..LayoutConfig::default()
        };
        let result = calculate(&ids(3), Rect::new(0.0, 0.0, 100.0, 100.0), config).expect("layout");
        let master = result.windows[0].rect;
        let stack = result.windows[1].rect;
        assert_eq!(master.origin.x + master.size.width, stack.origin.x);
    }
}
