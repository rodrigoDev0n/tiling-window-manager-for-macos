//! Layout execution (SPEC-005 FR-006, FR-007, FR-008).
//!
//! The executor applies a [`LayoutPlan`] through a narrow [`LayoutBackend`]
//! trait, revalidates the applied geometry against the requested target, and
//! aggregates per-window results into an [`ExecutionReport`]. One failing
//! window never aborts the remaining placements, and a window that already
//! matches its target is left untouched.

use crate::core::geometry::Rect;
use crate::core::layout_plan::{
    ExecutionReport, GEOMETRY_TOLERANCE, LayoutPlan, PlanEntry, WindowRef, WindowResult,
    WindowResultState, rects_within,
};

/// Native window mutation surface used by the executor.
///
/// Implementations translate platform errors into a human-readable message so
/// the executor stays platform-independent.
pub trait LayoutBackend<W> {
    /// Applies `rect` to `window` and returns the geometry the platform
    /// actually applied (read back after the operation).
    fn set_bounds(&self, window: &W, rect: Rect) -> Result<Rect, String>;
}

/// Whether the executor may mutate windows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionMode {
    /// Apply placements through the backend.
    Apply,
    /// Calculate results without ever calling the backend.
    DryRun,
}

/// Executes `plan`, applying each placement and collecting results.
///
/// For every entry the executor first checks whether the original geometry
/// already matches the target within [`GEOMETRY_TOLERANCE`]; if so, the window
/// is skipped and no native call is made. Otherwise the placement is applied
/// through `backend` and the returned geometry is compared against the target.
///
/// In [`ExecutionMode::DryRun`] the backend is never called and every placement
/// is reported as [`WindowResultState::Planned`].
pub fn execute<W, B>(plan: LayoutPlan<W>, backend: &B, mode: ExecutionMode) -> ExecutionReport
where
    W: WindowRef,
    B: LayoutBackend<W>,
{
    let mut results: Vec<WindowResult> = Vec::with_capacity(plan.entries.len());
    let mut applied = 0usize;
    let mut constrained = 0usize;
    let mut failed = 0usize;
    let mut skipped = 0usize;
    let mut planned = 0usize;

    for PlanEntry {
        window_id,
        original,
        target,
        window,
    } in plan.entries
    {
        let info = window.info().clone();
        let state = match mode {
            ExecutionMode::DryRun => {
                planned += 1;
                WindowResultState::Planned
            }
            ExecutionMode::Apply if rects_within(original, target, GEOMETRY_TOLERANCE) => {
                skipped += 1;
                WindowResultState::AlreadyCorrect
            }
            ExecutionMode::Apply => match backend.set_bounds(&window, target) {
                Ok(actual) if rects_within(actual, target, GEOMETRY_TOLERANCE) => {
                    applied += 1;
                    WindowResultState::Applied(actual)
                }
                Ok(actual) => {
                    constrained += 1;
                    WindowResultState::Constrained(actual)
                }
                Err(message) => {
                    failed += 1;
                    WindowResultState::Failed(message)
                }
            },
        };

        results.push(WindowResult {
            window_id,
            application: info.application,
            title: info.title,
            original,
            target,
            state,
        });
    }

    ExecutionReport {
        display_id: plan.display_id,
        layout: plan.layout,
        discovered: plan.discovered,
        eligible: plan.eligible,
        applied,
        constrained,
        failed,
        skipped,
        planned,
        results,
        exclusions: plan.exclusions,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::display::DisplayId;
    use crate::core::layout::LayoutType;
    use crate::core::window::{ProcessId, WindowClassification, WindowId, WindowInfo};
    use std::cell::RefCell;

    struct FakeWindow {
        info: WindowInfo,
        key: crate::core::event::WindowKey,
    }

    impl FakeWindow {
        fn new(bounds: Rect) -> Self {
            Self {
                info: WindowInfo {
                    pid: ProcessId(1),
                    application: "App".to_string(),
                    title: None,
                    bounds: Some(bounds),
                },
                key: crate::core::event::WindowKey::new(
                    ProcessId(1),
                    crate::core::event::RuntimeWindowId::new(0),
                ),
            }
        }
    }

    impl WindowRef for FakeWindow {
        fn info(&self) -> &WindowInfo {
            &self.info
        }
        fn key(&self) -> crate::core::event::WindowKey {
            self.key
        }
        fn classification(&self) -> &WindowClassification {
            // Not used by the executor; provide a valid stand-in.
            static CLASSIFICATION: std::sync::OnceLock<WindowClassification> =
                std::sync::OnceLock::new();
            CLASSIFICATION.get_or_init(|| WindowClassification {
                role: Some("AXWindow".to_string()),
                minimized: Some(false),
                fullscreen: Some(false),
                controllable: Some(true),
            })
        }
    }

    /// Records calls and lets tests control the applied geometry.
    struct StubBackend {
        calls: RefCell<usize>,
        applied: Rect,
    }

    impl LayoutBackend<FakeWindow> for StubBackend {
        fn set_bounds(&self, _window: &FakeWindow, _rect: Rect) -> Result<Rect, String> {
            *self.calls.borrow_mut() += 1;
            Ok(self.applied)
        }
    }

    fn plan(entries: Vec<(u64, Rect, Rect)>) -> LayoutPlan<FakeWindow> {
        let entries: Vec<PlanEntry<FakeWindow>> = entries
            .into_iter()
            .map(|(id, original, target)| {
                let window = FakeWindow::new(original);
                crate::core::layout_plan::PlanEntry {
                    window_id: WindowId(id),
                    original,
                    target,
                    window,
                }
            })
            .collect();
        LayoutPlan {
            display_id: DisplayId(1),
            layout: LayoutType::MasterStack,
            discovered: 3,
            eligible: entries.len(),
            entries,
            exclusions: Vec::new(),
        }
    }

    #[test]
    fn already_correct_window_is_not_modified() {
        let target = Rect::new(0.0, 25.0, 1440.0, 875.0);
        let plan = plan(vec![(0, target, target)]);
        let backend = StubBackend {
            calls: RefCell::new(0),
            applied: target,
        };
        let report = execute(plan, &backend, ExecutionMode::Apply);
        assert_eq!(*backend.calls.borrow(), 0);
        assert_eq!(report.skipped, 1);
        assert_eq!(report.applied, 0);
        assert!(matches!(
            report.results[0].state,
            WindowResultState::AlreadyCorrect
        ));
    }

    #[test]
    fn applied_geometry_matching_target_is_successful() {
        let original = Rect::new(100.0, 100.0, 100.0, 100.0);
        let target = Rect::new(0.0, 25.0, 1440.0, 875.0);
        let plan = plan(vec![(0, original, target)]);
        let backend = StubBackend {
            calls: RefCell::new(0),
            applied: target,
        };
        let report = execute(plan, &backend, ExecutionMode::Apply);
        assert_eq!(*backend.calls.borrow(), 1);
        assert_eq!(report.applied, 1);
        match &report.results[0].state {
            WindowResultState::Applied(rect) => assert_eq!(*rect, target),
            other => panic!("expected applied, got {other:?}"),
        }
    }

    #[test]
    fn constrained_geometry_is_flagged() {
        let original = Rect::new(100.0, 100.0, 100.0, 100.0);
        let target = Rect::new(0.0, 25.0, 1440.0, 875.0);
        // Platform clamps the height, so the read-back differs.
        let clamped = Rect::new(0.0, 25.0, 1440.0, 400.0);
        let plan = plan(vec![(0, original, target)]);
        let backend = StubBackend {
            calls: RefCell::new(0),
            applied: clamped,
        };
        let report = execute(plan, &backend, ExecutionMode::Apply);
        assert_eq!(report.constrained, 1);
        match &report.results[0].state {
            WindowResultState::Constrained(rect) => assert_eq!(*rect, clamped),
            other => panic!("expected constrained, got {other:?}"),
        }
    }

    #[test]
    fn backend_error_is_recorded_as_failure() {
        struct FailingBackend;
        impl LayoutBackend<FakeWindow> for FailingBackend {
            fn set_bounds(&self, _window: &FakeWindow, _rect: Rect) -> Result<Rect, String> {
                Err("window rejected the change".to_string())
            }
        }

        let original = Rect::new(100.0, 100.0, 100.0, 100.0);
        let target = Rect::new(0.0, 25.0, 1440.0, 875.0);
        let plan = plan(vec![(0, original, target)]);
        let report = execute(plan, &FailingBackend, ExecutionMode::Apply);
        assert_eq!(report.failed, 1);
        match &report.results[0].state {
            WindowResultState::Failed(message) => assert!(message.contains("rejected")),
            other => panic!("expected failure, got {other:?}"),
        }
    }

    #[test]
    fn one_failure_does_not_stop_other_windows() {
        struct MixedBackend;
        impl LayoutBackend<FakeWindow> for MixedBackend {
            fn set_bounds(&self, _window: &FakeWindow, rect: Rect) -> Result<Rect, String> {
                if rect.origin.x < 500.0 {
                    Err("nope".to_string())
                } else {
                    Ok(rect)
                }
            }
        }

        let a = Rect::new(0.0, 25.0, 1440.0, 875.0);
        let plan = plan(vec![
            (0, Rect::new(10.0, 10.0, 100.0, 100.0), a),
            (
                1,
                Rect::new(10.0, 10.0, 100.0, 100.0),
                Rect::new(600.0, 25.0, 800.0, 875.0),
            ),
        ]);
        let report = execute(plan, &MixedBackend, ExecutionMode::Apply);
        assert_eq!(report.failed, 1);
        assert_eq!(report.applied, 1);
        assert_eq!(report.results.len(), 2);
    }

    #[test]
    fn report_aggregates_counts() {
        let target_a = Rect::new(0.0, 25.0, 720.0, 875.0);
        let plan = plan(vec![
            (0, Rect::new(5.0, 5.0, 100.0, 100.0), target_a),
            (1, target_a, target_a), // skipped
        ]);
        let backend = StubBackend {
            calls: RefCell::new(0),
            applied: target_a,
        };
        let report = execute(plan, &backend, ExecutionMode::Apply);
        assert_eq!(report.applied, 1);
        assert_eq!(report.skipped, 1);
        assert_eq!(report.eligible, 2);
        assert_eq!(report.exclusions.len(), 0);
    }

    #[test]
    fn dry_run_never_calls_backend() {
        let target = Rect::new(0.0, 25.0, 1440.0, 875.0);
        let plan = plan(vec![
            (0, Rect::new(100.0, 100.0, 100.0, 100.0), target),
            (1, Rect::new(200.0, 200.0, 100.0, 100.0), target),
        ]);
        let backend = StubBackend {
            calls: RefCell::new(0),
            applied: target,
        };
        let report = execute(plan, &backend, ExecutionMode::DryRun);
        assert_eq!(*backend.calls.borrow(), 0);
        assert_eq!(report.planned, 2);
        assert_eq!(report.applied, 0);
        assert_eq!(report.skipped, 0);
        assert!(
            report
                .results
                .iter()
                .all(|r| matches!(r.state, WindowResultState::Planned))
        );
    }
}
