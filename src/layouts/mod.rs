//! Deterministic, platform-independent layout algorithms.
//!
//! Each algorithm maps an ordered collection of window identifiers and a usable
//! screen area (AX coordinates: top-left origin, Y down) to target rectangles.
//! The engine performs no native calls and never modifies real windows.

use std::collections::HashSet;

use crate::core::geometry::Rect;
use crate::core::layout::{LayoutConfig, LayoutError, LayoutResult, LayoutType};
use crate::core::window::WindowId;

mod columns;
mod master_stack;
mod monocle;
mod rows;

/// Calculates target rectangles for `windows` inside `area`.
///
/// `windows` keeps its input order, which the result preserves. When a
/// collection is empty, an empty [`LayoutResult`] is returned.
pub fn calculate(
    windows: &[WindowId],
    area: Rect,
    config: LayoutConfig,
) -> Result<LayoutResult, LayoutError> {
    config.validate()?;
    if !area.is_valid() {
        return Err(LayoutError::InvalidArea(
            "area must be finite with positive dimensions".to_string(),
        ));
    }

    let mut seen = HashSet::with_capacity(windows.len());
    for window in windows {
        if !seen.insert(*window) {
            return Err(LayoutError::DuplicateWindowId(*window));
        }
    }

    if windows.is_empty() {
        return Ok(LayoutResult {
            layout: config.layout,
            windows: Vec::new(),
        });
    }

    let placed = match config.layout {
        LayoutType::MasterStack => master_stack::arrange(windows, area, config),
        LayoutType::Columns => columns::arrange(windows, area, config),
        LayoutType::Rows => rows::arrange(windows, area, config),
        LayoutType::Monocle => monocle::arrange(windows, area, config),
    }?;

    for placement in &placed {
        if !placement.rect.is_valid() {
            return Err(LayoutError::InvalidResult(
                "layout produced an invalid rectangle".to_string(),
            ));
        }
    }

    Ok(LayoutResult {
        layout: config.layout,
        windows: placed,
    })
}

/// The area left after applying `gap` around every edge.
pub(super) fn content_area(area: Rect, gap: f64) -> Result<Rect, LayoutError> {
    let width = area.size.width - 2.0 * gap;
    let height = area.size.height - 2.0 * gap;
    if width <= 0.0 || height <= 0.0 {
        return Err(LayoutError::InsufficientSpace {
            available: area.size.width.min(area.size.height),
            required: 2.0 * gap,
        });
    }
    Ok(Rect::new(
        area.origin.x + gap,
        area.origin.y + gap,
        width,
        height,
    ))
}

/// Splits an axis of `total` length into `count` segments separated by `inner`.
///
/// The last segment absorbs the fractional remainder so the final edge lands
/// exactly on `origin + total`, avoiding accumulated floating-point drift.
/// Each returned pair is `(start, length)`.
pub(super) fn partition_axis(
    origin: f64,
    total: f64,
    count: usize,
    inner: f64,
) -> Result<Vec<(f64, f64)>, LayoutError> {
    debug_assert!(count > 0);
    let required = (count - 1) as f64 * inner;
    if required >= total {
        return Err(LayoutError::InsufficientSpace {
            available: total,
            required: required + inner,
        });
    }
    let base = (total - required) / count as f64;
    let mut parts = Vec::with_capacity(count);
    for index in 0..count {
        let start = origin + index as f64 * (base + inner);
        let length = if index == count - 1 {
            origin + total - start
        } else {
            base
        };
        parts.push((start, length));
    }
    Ok(parts)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn windows(count: usize) -> Vec<WindowId> {
        (0..count).map(|index| WindowId(index as u64)).collect()
    }

    fn area() -> Rect {
        Rect::new(0.0, 25.0, 1440.0, 875.0)
    }

    fn negative_area() -> Rect {
        Rect::new(-1920.0, -180.0, 1920.0, 1080.0)
    }

    const EPS: f64 = 1e-9;

    fn assert_inside(rect: Rect, area: Rect) {
        assert!(rect.origin.x >= area.origin.x - EPS);
        assert!(rect.origin.y >= area.origin.y - EPS);
        assert!(rect.origin.x + rect.size.width <= area.origin.x + area.size.width + EPS);
        assert!(rect.origin.y + rect.size.height <= area.origin.y + area.size.height + EPS);
    }

    fn assert_no_overlap(a: Rect, b: Rect) {
        let x_separated = a.origin.x + a.size.width <= b.origin.x + EPS
            || b.origin.x + b.size.width <= a.origin.x + EPS;
        let y_separated = a.origin.y + a.size.height <= b.origin.y + EPS
            || b.origin.y + b.size.height <= a.origin.y + EPS;
        assert!(
            x_separated || y_separated,
            "rectangles overlap: {a:?} vs {b:?}"
        );
    }

    #[test]
    fn empty_window_collection_returns_empty_layout() {
        let result = calculate(&[], area(), LayoutConfig::default()).expect("empty layout");
        assert!(result.windows.is_empty());
        assert_eq!(result.layout, LayoutType::MasterStack);
    }

    #[test]
    fn duplicate_window_id_is_rejected() {
        let input = [WindowId(1), WindowId(1)];
        assert_eq!(
            calculate(&input, area(), LayoutConfig::default()),
            Err(LayoutError::DuplicateWindowId(WindowId(1)))
        );
    }

    #[test]
    fn invalid_area_is_rejected() {
        let bad = Rect::new(f64::NAN, 0.0, 100.0, 100.0);
        assert!(matches!(
            calculate(&[WindowId(1)], bad, LayoutConfig::default()),
            Err(LayoutError::InvalidArea(_))
        ));
        let zero = Rect::new(0.0, 0.0, 0.0, 100.0);
        assert!(matches!(
            calculate(&[WindowId(1)], zero, LayoutConfig::default()),
            Err(LayoutError::InvalidArea(_))
        ));
    }

    #[test]
    fn invalid_config_is_rejected() {
        let config = LayoutConfig {
            master_ratio: 1.5,
            ..LayoutConfig::default()
        };
        assert!(matches!(
            calculate(&[WindowId(1)], area(), config),
            Err(LayoutError::InvalidConfig(_))
        ));
    }

    #[test]
    fn gaps_that_consume_everything_report_insufficient_space() {
        let tiny = Rect::new(0.0, 0.0, 10.0, 10.0);
        assert!(matches!(
            calculate(&[WindowId(1)], tiny, LayoutConfig::default()),
            Err(LayoutError::InsufficientSpace { .. })
        ));
    }

    #[test]
    fn deterministic_inputs_produce_identical_results() {
        let input = windows(3);
        let config = LayoutConfig::default();
        let first = calculate(&input, area(), config).expect("first");
        let second = calculate(&input, area(), config).expect("second");
        assert_eq!(first, second);
    }

    #[test]
    fn output_preserves_window_order_and_count() {
        let input = windows(5);
        let result = calculate(&input, area(), LayoutConfig::default()).expect("layout");
        assert_eq!(result.windows.len(), input.len());
        for (expected, placement) in input.iter().zip(&result.windows) {
            assert_eq!(placement.window, *expected);
        }
    }

    #[test]
    fn negative_origin_layout_stays_inside_area() {
        let input = windows(2);
        let config = LayoutConfig {
            layout: LayoutType::Columns,
            outer_gap: 0.0,
            inner_gap: 8.0,
            ..LayoutConfig::default()
        };
        let result = calculate(&input, negative_area(), config).expect("layout");
        for placement in &result.windows {
            assert_inside(placement.rect, negative_area());
            assert!(placement.rect.is_valid());
        }
        assert_no_overlap(result.windows[0].rect, result.windows[1].rect);
    }

    #[test]
    fn layout_grid_preserves_invariants() {
        let configs = [
            LayoutConfig::default(),
            LayoutConfig {
                outer_gap: 0.0,
                inner_gap: 0.0,
                ..LayoutConfig::default()
            },
            LayoutConfig {
                outer_gap: 20.0,
                inner_gap: 12.0,
                ..LayoutConfig::default()
            },
        ];
        let types = [
            LayoutType::MasterStack,
            LayoutType::Columns,
            LayoutType::Rows,
            LayoutType::Monocle,
        ];

        let areas = [
            area(),
            negative_area(),
            Rect::new(0.0, 0.0, 800.0, 1600.0),
            Rect::new(0.0, 0.0, 120.0, 80.0),
            Rect::new(0.0, 0.0, 14.0, 14.0),
        ];

        for layout in types {
            for count in 0..=8 {
                for config in &configs {
                    for area in &areas {
                        let input = windows(count);
                        let config = LayoutConfig { layout, ..*config };
                        let result = calculate(&input, *area, config);
                        match result {
                            Ok(result) => {
                                assert_eq!(result.windows.len(), count);
                                assert_eq!(result.layout, layout);
                                for (expected, placement) in input.iter().zip(&result.windows) {
                                    assert_eq!(placement.window, *expected);
                                    assert!(placement.rect.is_valid());
                                    assert_inside(placement.rect, *area);
                                }
                                if layout != LayoutType::Monocle {
                                    for a in 0..result.windows.len() {
                                        for b in a + 1..result.windows.len() {
                                            assert_no_overlap(
                                                result.windows[a].rect,
                                                result.windows[b].rect,
                                            );
                                        }
                                    }
                                } else if count > 0 {
                                    let reference = result.windows[0].rect;
                                    for placement in &result.windows {
                                        assert_eq!(placement.rect, reference);
                                    }
                                }
                            }
                            Err(LayoutError::InsufficientSpace { .. }) => {}
                            Err(other) => panic!("unexpected error: {other:?}"),
                        }
                    }
                }
            }
        }
    }
}
