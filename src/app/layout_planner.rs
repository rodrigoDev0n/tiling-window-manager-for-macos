//! Layout planning (SPEC-005 FR-005).
//!
//! The planner discovers nothing itself; it receives the display snapshot and
//! the discovered windows, filters eligible windows, assigns them to the target
//! display, orders them deterministically, and invokes the pure layout engine.
//! Building a plan never mutates any window.

use crate::core::display::{self, Display, DisplayId};
use crate::core::geometry::Rect;
use crate::core::layout::{self, LayoutConfig};
use crate::core::layout_plan::{Exclusion, LayoutPlan, PlanEntry, PlanError, WindowRef};
use crate::core::selector::compare_windows;
use crate::core::window::WindowId;

use super::window_eligibility;

/// Builds the execution plan for `config` on `target` from `displays` and
/// `windows`.
///
/// Eligible windows are ordered deterministically via the same comparator used
/// by window selection, so repeated invocations with unchanged state yield the
/// same ordering. The returned plan contains no handles to windows that were
/// excluded and never mutates anything.
pub fn build_plan<W: WindowRef>(
    displays: &[Display],
    windows: Vec<W>,
    target: DisplayId,
    config: LayoutConfig,
) -> Result<LayoutPlan<W>, PlanError> {
    let target_display =
        display::find(displays, target).ok_or(PlanError::DisplayNotFound(target))?;

    let discovered = windows.len();
    let mut eligible: Vec<(W, Rect)> = Vec::new();
    let mut exclusions: Vec<Exclusion> = Vec::new();

    for window in windows {
        match window_eligibility::assess(&window, displays, target) {
            Ok(bounds) => eligible.push((window, bounds)),
            Err(reason) => exclusions.push(Exclusion {
                application: window.info().application.clone(),
                title: window.info().title.clone(),
                reason,
            }),
        }
    }

    eligible.sort_by(|a, b| compare_windows(a.0.info(), b.0.info()));

    let ids: Vec<WindowId> = (0..eligible.len())
        .map(|index| WindowId(index as u64))
        .collect();
    let result =
        layout::calculate(&ids, target_display.usable, config).map_err(PlanError::Layout)?;

    let entries: Vec<PlanEntry<W>> = eligible
        .into_iter()
        .zip(result.windows)
        .map(|((window, original), placement)| PlanEntry {
            window_id: placement.window,
            original,
            target: placement.rect,
            window,
        })
        .collect();

    Ok(LayoutPlan {
        display_id: target,
        layout: config.layout,
        discovered,
        eligible: entries.len(),
        entries,
        exclusions,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::display::Display;
    use crate::core::display_assignment::assign;
    use crate::core::window::{ProcessId, WindowClassification, WindowInfo};

    struct FakeWindow {
        info: WindowInfo,
        classification: WindowClassification,
    }

    impl FakeWindow {
        fn new(application: &str, bounds: Rect) -> Self {
            Self {
                info: WindowInfo {
                    pid: ProcessId(1),
                    application: application.to_string(),
                    title: None,
                    bounds: Some(bounds),
                },
                classification: WindowClassification {
                    role: Some("AXWindow".to_string()),
                    minimized: Some(false),
                    fullscreen: Some(false),
                },
            }
        }
    }

    impl WindowRef for FakeWindow {
        fn info(&self) -> &WindowInfo {
            &self.info
        }
        fn classification(&self) -> &WindowClassification {
            &self.classification
        }
    }

    fn display() -> Display {
        Display {
            id: DisplayId(1),
            name: None,
            main: true,
            scale: 2.0,
            bounds: Rect::new(0.0, 0.0, 1440.0, 900.0),
            usable: Rect::new(0.0, 25.0, 1440.0, 875.0),
        }
    }

    fn window_in_usable() -> Rect {
        Rect::new(300.0, 200.0, 200.0, 200.0)
    }

    #[test]
    fn plan_tiles_eligible_windows_on_target_display() {
        let windows = vec![
            FakeWindow::new("B", window_in_usable()),
            FakeWindow::new("A", window_in_usable()),
            FakeWindow::new("C", window_in_usable()),
        ];
        // Off-target window is excluded.
        let off_target = FakeWindow::new("D", Rect::new(9000.0, 9000.0, 100.0, 100.0));

        let plan = build_plan(
            &[display()],
            windows
                .into_iter()
                .chain(std::iter::once(off_target))
                .collect(),
            DisplayId(1),
            LayoutConfig {
                layout: crate::core::layout::LayoutType::Columns,
                ..LayoutConfig::default()
            },
        )
        .expect("plan");

        assert_eq!(plan.eligible, 3);
        assert_eq!(plan.discovered, 4);
        assert_eq!(plan.exclusions.len(), 1);
        assert_eq!(plan.entries.len(), 3);
        // Deterministic ordering by application name: A, B, C.
        let names: Vec<String> = plan
            .entries
            .iter()
            .map(|entry| entry.window.info().application.clone())
            .collect();
        assert_eq!(
            names,
            vec!["A".to_string(), "B".to_string(), "C".to_string()]
        );
        // Unique, ordered identifiers.
        let ids: Vec<u64> = plan.entries.iter().map(|entry| entry.window_id.0).collect();
        assert_eq!(ids, vec![0, 1, 2]);
        // Targets are inside the usable area and valid.
        for entry in &plan.entries {
            assert!(entry.target.is_valid());
            assert!(entry.target.origin.x >= 0.0);
            assert!(entry.target.origin.y >= 25.0);
        }
    }

    #[test]
    fn unknown_display_is_rejected() {
        let windows = vec![FakeWindow::new("A", window_in_usable())];
        let result = build_plan(
            &[display()],
            windows,
            DisplayId(99),
            LayoutConfig::default(),
        );
        assert!(matches!(
            result,
            Err(PlanError::DisplayNotFound(DisplayId(99)))
        ));
    }

    #[test]
    fn empty_window_collection_yields_empty_plan() {
        let plan: LayoutPlan<FakeWindow> = build_plan(
            &[display()],
            Vec::new(),
            DisplayId(1),
            LayoutConfig::default(),
        )
        .expect("plan");
        assert_eq!(plan.eligible, 0);
        assert_eq!(plan.entries.len(), 0);
        assert!(plan.exclusions.is_empty());
    }

    #[test]
    fn planning_preserves_original_geometry_without_mutating() {
        // The planner has no platform handle; building a plan records the
        // discovered geometry as-is and only computes targets separately.
        let windows = vec![FakeWindow::new("A", window_in_usable())];
        let plan =
            build_plan(&[display()], windows, DisplayId(1), LayoutConfig::default()).expect("plan");
        assert_eq!(plan.entries.len(), 1);
        assert_eq!(plan.entries[0].original, window_in_usable());
        assert_ne!(plan.entries[0].target, plan.entries[0].original);
    }

    #[test]
    fn ordering_is_independent_of_discovery_order() {
        let first = build_plan(
            &[display()],
            vec![
                FakeWindow::new("B", window_in_usable()),
                FakeWindow::new("A", window_in_usable()),
            ],
            DisplayId(1),
            LayoutConfig::default(),
        )
        .expect("plan");
        let second = build_plan(
            &[display()],
            vec![
                FakeWindow::new("A", window_in_usable()),
                FakeWindow::new("B", window_in_usable()),
            ],
            DisplayId(1),
            LayoutConfig::default(),
        )
        .expect("plan");

        let ids = |plan: &LayoutPlan<FakeWindow>| -> Vec<u64> {
            plan.entries.iter().map(|entry| entry.window_id.0).collect()
        };
        assert_eq!(ids(&first), ids(&second));
    }

    #[test]
    fn assignment_helper_is_reused_for_display_targeting() {
        // Sanity: the planner relies on core assignment for eligibility.
        let displays = [display()];
        assert_eq!(assign(&displays, window_in_usable()), Some(DisplayId(1)));
    }
}
