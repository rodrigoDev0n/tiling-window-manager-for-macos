//! Execution-plan domain model shared by the planner and the executor.
//!
//! This module is platform-independent. The generic [`WindowRef`] trait gives
//! the planner and executor access to window metadata without depending on
//! native handles, so the pipeline stays testable with lightweight doubles.

use std::fmt;

use crate::core::display::DisplayId;
use crate::core::event::WindowKey;
use crate::core::geometry::Rect;
use crate::core::layout::{LayoutError, LayoutType};
use crate::core::window::{WindowClassification, WindowId, WindowInfo};

/// Tolerance, in points, used when comparing requested against actual geometry.
///
/// Accessibility APIs return floating-point coordinates, so a small tolerance
/// is required to distinguish "immediately correct" from "constrained".
pub const GEOMETRY_TOLERANCE: f64 = 1.0;

/// Access to a window's metadata without exposing native references.
///
/// Instances carrying a native handle implement this trait for real execution;
/// tests use lightweight fakes.
pub trait WindowRef {
    /// Window metadata read during discovery.
    fn info(&self) -> &WindowInfo;
    /// Window classification used for eligibility decisions.
    fn classification(&self) -> &WindowClassification;
    /// Stable runtime key used to order windows (SPEC-009).
    fn key(&self) -> WindowKey;
}

/// Reason a window was excluded from the layout plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExclusionReason {
    /// The window is not a standard `AXWindow` role.
    NonStandardRole,
    /// The window is minimized.
    Minimized,
    /// The window is in native fullscreen mode.
    Fullscreen,
    /// The window does not support programmatic position or size changes.
    NotControllable,
    /// The window could not be classified reliably (missing role or minimized
    /// attribute), so it is skipped rather than modified.
    Unclassifiable,
    /// The window has no valid geometry.
    InvalidGeometry,
    /// The window does not intersect any active display.
    NoIntersectingDisplay,
    /// The window belongs to a different display than the target.
    NotOnTargetDisplay,
}

impl fmt::Display for ExclusionReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = match self {
            ExclusionReason::NonStandardRole => "not a standard application window",
            ExclusionReason::Minimized => "window is minimized",
            ExclusionReason::Fullscreen => "window is fullscreen",
            ExclusionReason::NotControllable => "window does not support position or size changes",
            ExclusionReason::Unclassifiable => "window could not be classified reliably",
            ExclusionReason::InvalidGeometry => "window has no valid geometry",
            ExclusionReason::NoIntersectingDisplay => "window does not intersect any display",
            ExclusionReason::NotOnTargetDisplay => "window is on another display",
        };
        write!(f, "{text}")
    }
}

/// A window excluded from the plan together with the reason.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Exclusion {
    pub application: String,
    pub title: Option<String>,
    pub reason: ExclusionReason,
}

/// One planned window placement holding its native reference.
#[derive(Debug)]
pub struct PlanEntry<W> {
    pub window_id: WindowId,
    pub original: Rect,
    pub target: Rect,
    pub window: W,
}

/// The deterministic execution plan produced by the planner.
///
/// Building a plan never mutates windows.
#[derive(Debug)]
pub struct LayoutPlan<W> {
    pub display_id: DisplayId,
    pub layout: LayoutType,
    pub discovered: usize,
    pub eligible: usize,
    pub entries: Vec<PlanEntry<W>>,
    pub exclusions: Vec<Exclusion>,
}

/// Errors while building a layout plan.
#[derive(Debug)]
pub enum PlanError {
    /// The requested target display does not exist.
    DisplayNotFound(DisplayId),
    /// The layout engine rejected the inputs.
    Layout(LayoutError),
}

impl fmt::Display for PlanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PlanError::DisplayNotFound(id) => write!(f, "no active display with identifier {id}"),
            PlanError::Layout(error) => write!(f, "layout planning failed: {error}"),
        }
    }
}

impl std::error::Error for PlanError {}

/// Outcome of applying one planned placement.
#[derive(Debug, Clone, PartialEq)]
pub enum WindowResultState {
    /// macOS applied the requested geometry.
    Applied(Rect),
    /// macOS adjusted the geometry beyond the tolerance.
    Constrained(Rect),
    /// The window already matched the target, so it was left untouched.
    AlreadyCorrect,
    /// Dry-run only: the placement was calculated but never applied.
    Planned,
    /// The operation failed and the reason is recorded.
    Failed(String),
}

impl fmt::Display for WindowResultState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WindowResultState::Applied(_) => write!(f, "applied"),
            WindowResultState::Constrained(_) => write!(f, "applied with constraints"),
            WindowResultState::AlreadyCorrect => write!(f, "skipped (already correct)"),
            WindowResultState::Planned => write!(f, "planned (not applied)"),
            WindowResultState::Failed(message) => write!(f, "failed ({message})"),
        }
    }
}

/// Verified result for a single processed window.
#[derive(Debug, Clone, PartialEq)]
pub struct WindowResult {
    pub window_id: WindowId,
    pub application: String,
    pub title: Option<String>,
    pub original: Rect,
    pub target: Rect,
    pub state: WindowResultState,
}

/// Aggregate summary of an execution run.
#[derive(Debug, Clone, PartialEq)]
pub struct ExecutionReport {
    pub display_id: DisplayId,
    pub layout: LayoutType,
    pub discovered: usize,
    pub eligible: usize,
    pub applied: usize,
    pub constrained: usize,
    pub failed: usize,
    pub skipped: usize,
    pub planned: usize,
    pub results: Vec<WindowResult>,
    pub exclusions: Vec<Exclusion>,
}

/// Compares two rectangles component-wise against `tolerance`.
pub fn rects_within(a: Rect, b: Rect, tolerance: f64) -> bool {
    (a.origin.x - b.origin.x).abs() <= tolerance
        && (a.origin.y - b.origin.y).abs() <= tolerance
        && (a.size.width - b.size.width).abs() <= tolerance
        && (a.size.height - b.size.height).abs() <= tolerance
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rects_within_respects_tolerance() {
        let a = Rect::new(0.0, 25.0, 1440.0, 875.0);
        assert!(rects_within(a, a, GEOMETRY_TOLERANCE));
        let b = Rect::new(0.4, 24.7, 1439.5, 875.8);
        assert!(rects_within(a, b, GEOMETRY_TOLERANCE));
        let c = Rect::new(2.0, 0.0, 1440.0, 875.0);
        assert!(!rects_within(a, c, GEOMETRY_TOLERANCE));
    }

    #[test]
    fn exclusion_reasons_have_human_readable_text() {
        assert!(
            ExclusionReason::Fullscreen
                .to_string()
                .contains("fullscreen")
        );
        assert!(
            ExclusionReason::NoIntersectingDisplay
                .to_string()
                .contains("intersect")
        );
    }

    #[test]
    fn result_states_display_summary() {
        assert_eq!(
            WindowResultState::AlreadyCorrect.to_string(),
            "skipped (already correct)"
        );
        assert!(
            WindowResultState::Failed("boom".to_string())
                .to_string()
                .contains("boom")
        );
        assert!(
            WindowResultState::Applied(Rect::new(0.0, 0.0, 1.0, 1.0))
                .to_string()
                .contains("applied")
        );
    }
}
