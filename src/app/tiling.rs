//! Automatic-tiling controller (SPEC-009).
//!
//! This module is platform-independent. It consumes normalized [`DomainEvent`]s
//! and maintains:
//!
//! * a persistent, per-display tiling order (the "stack") so layouts stay
//!   stable when titles or geometry change,
//! * a per-display [`LayoutConfig`],
//! * the set of displays that need re-tiling, debounced by a settle interval,
//! * the geometry each window is *expected* to have after the last apply, so
//!   the controller never mistakes its own mutations for user changes.
//!
//! It performs no native calls; the command layer supplies the display snapshot
//! and executes the plans the controller schedules.

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use crate::core::display::{Display, DisplayId};
use crate::core::display_assignment;
use crate::core::event::{AppEvent, DomainEvent, WindowEvent, WindowKey};
use crate::core::geometry::Rect;
use crate::core::layout::{LayoutConfig, LayoutType};
use crate::core::layout_plan::{GEOMETRY_TOLERANCE, rects_within};
use crate::core::selector::compare_windows;
use crate::core::window::WindowInfo;

/// Time a display waits after its last apply before it may be re-tiled again.
pub const RETILE_SETTLE_INTERVAL: Duration = Duration::from_millis(150);

/// Counters describing controller behavior, for diagnostics.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TilingDiagnostics {
    /// Re-tiles recorded through [`TilingState::record_applied`].
    pub retiles: usize,
    /// Geometry events ignored because they matched the expected geometry.
    pub suppressed_geometry_events: usize,
    /// Window-created events observed.
    pub windows_created: usize,
    /// Window-destroyed events observed.
    pub windows_destroyed: usize,
}

/// Platform-independent automatic-tiling state.
#[derive(Debug)]
pub struct TilingState {
    default: LayoutConfig,
    layouts: HashMap<DisplayId, LayoutConfig>,
    order: HashMap<DisplayId, Vec<WindowKey>>,
    dirty: HashSet<DisplayId>,
    last_apply: HashMap<DisplayId, Instant>,
    expected: HashMap<WindowKey, Rect>,
    assignment: HashMap<WindowKey, DisplayId>,
    diagnostics: TilingDiagnostics,
}

impl TilingState {
    /// Creates a controller with the SPEC-004 default layout.
    pub fn new() -> Self {
        Self::with_default_layout(LayoutConfig::default())
    }

    /// Creates a controller whose undiscovered displays use `default`.
    pub fn with_default_layout(default: LayoutConfig) -> Self {
        Self {
            default,
            layouts: HashMap::new(),
            order: HashMap::new(),
            dirty: HashSet::new(),
            last_apply: HashMap::new(),
            expected: HashMap::new(),
            assignment: HashMap::new(),
            diagnostics: TilingDiagnostics::default(),
        }
    }

    pub fn diagnostics(&self) -> TilingDiagnostics {
        self.diagnostics
    }

    /// Returns the layout configuration for `display`.
    pub fn layout_for(&self, display: DisplayId) -> LayoutConfig {
        self.layouts.get(&display).copied().unwrap_or(self.default)
    }

    /// Overrides the layout type for `display`, preserving other settings.
    pub fn set_layout(&mut self, display: DisplayId, layout: LayoutType) {
        let mut config = self.layout_for(display);
        config.layout = layout;
        self.layouts.insert(display, config);
    }

    /// Returns the persistent tiling order for `display` (possibly empty).
    pub fn order_for(&self, display: DisplayId) -> &[WindowKey] {
        self.order.get(&display).map(Vec::as_slice).unwrap_or(&[])
    }

    /// Seeds the order from a fresh registry snapshot and schedules an initial
    /// tile of every active display.
    ///
    /// Windows are grouped by their assigned display and sorted with
    /// `compare_windows`, reproducing the SPEC-005 ordering once.
    pub fn seed(
        &mut self,
        displays: &[Display],
        entries: impl IntoIterator<Item = (WindowKey, WindowInfo)>,
    ) {
        let mut groups: HashMap<DisplayId, Vec<(WindowKey, WindowInfo)>> = HashMap::new();
        for (key, info) in entries {
            let Some(bounds) = info.bounds else {
                continue;
            };
            let Some(display) = display_assignment::assign(displays, bounds) else {
                continue;
            };
            self.assignment.insert(key, display);
            groups.entry(display).or_default().push((key, info));
        }

        for (display, mut list) in groups {
            list.sort_by(|a, b| compare_windows(&a.1, &b.1));
            self.order
                .insert(display, list.into_iter().map(|(key, _)| key).collect());
        }

        for display in displays {
            self.mark_display_dirty(display.id);
        }
    }

    /// Applies one observed event to the controller state.
    pub fn observe(&mut self, event: &DomainEvent, displays: &[Display]) {
        match event {
            DomainEvent::Window { event, .. } => self.observe_window(event, displays),
            DomainEvent::App { event, .. } => self.observe_app(event, displays),
        }
    }

    /// Marks `display` as needing a re-tile.
    pub fn mark_display_dirty(&mut self, display: DisplayId) {
        self.dirty.insert(display);
    }

    /// Swaps two windows within `display`'s tiling order.
    ///
    /// Returns `true` when both windows were present and the order changed.
    pub fn swap_in_order(&mut self, display: DisplayId, a: WindowKey, b: WindowKey) -> bool {
        let Some(list) = self.order.get_mut(&display) else {
            return false;
        };
        let (Some(index_a), Some(index_b)) = (
            list.iter().position(|key| *key == a),
            list.iter().position(|key| *key == b),
        ) else {
            return false;
        };
        list.swap(index_a, index_b);
        true
    }

    /// Returns the displays ready to re-tile, ordered by identifier.
    ///
    /// A display is ready when it is dirty and its settle interval has elapsed
    /// since the last apply.
    pub fn due(&self, now: Instant) -> Vec<DisplayId> {
        let mut ready: Vec<DisplayId> = self
            .dirty
            .iter()
            .copied()
            .filter(|display| {
                self.last_apply
                    .get(display)
                    .is_none_or(|last| now.duration_since(*last) >= RETILE_SETTLE_INTERVAL)
            })
            .collect();
        ready.sort();
        ready
    }

    /// Records the geometry `macOS` actually applied for `display`, clears its
    /// dirty flag, and resets its settle timer.
    pub fn record_applied(
        &mut self,
        display: DisplayId,
        applied: &[(WindowKey, Rect)],
        now: Instant,
    ) {
        self.dirty.remove(&display);
        self.last_apply.insert(display, now);
        self.diagnostics.retiles += 1;
        for (key, rect) in applied {
            self.expected.insert(*key, *rect);
        }
    }

    fn observe_window(&mut self, event: &WindowEvent, displays: &[Display]) {
        match event {
            WindowEvent::Created { pid, wid, bounds } => {
                let key = WindowKey::new(*pid, *wid);
                self.diagnostics.windows_created += 1;
                if let Some(rect) = bounds
                    && let Some(display) = display_assignment::assign(displays, *rect)
                {
                    self.assignment.insert(key, display);
                    self.ensure_in_order(display, key);
                    self.mark_display_dirty(display);
                    return;
                }
                self.mark_all_dirty(displays);
            }
            WindowEvent::Destroyed { pid, wid } => {
                let key = WindowKey::new(*pid, *wid);
                self.diagnostics.windows_destroyed += 1;
                if let Some(display) = self.assignment.remove(&key) {
                    self.mark_display_dirty(display);
                } else {
                    self.mark_all_dirty(displays);
                }
                self.remove_from_all_orders(key);
                self.expected.remove(&key);
            }
            WindowEvent::Moved { pid, wid, bounds } | WindowEvent::Resized { pid, wid, bounds } => {
                self.observe_geometry(WindowKey::new(*pid, *wid), *bounds, displays);
            }
            WindowEvent::Minimized { pid, wid } | WindowEvent::Restored { pid, wid } => {
                let key = WindowKey::new(*pid, *wid);
                if let Some(display) = self.assignment.get(&key).copied() {
                    self.mark_display_dirty(display);
                } else {
                    self.mark_all_dirty(displays);
                }
            }
            WindowEvent::Focused { .. } => {}
        }
    }

    fn observe_geometry(&mut self, key: WindowKey, bounds: Rect, displays: &[Display]) {
        let Some(display) = display_assignment::assign(displays, bounds) else {
            return;
        };

        if let Some(previous) = self.assignment.get(&key).copied()
            && previous != display
        {
            self.remove_from_order(previous, key);
            self.ensure_in_order(display, key);
        } else if !self.assignment.contains_key(&key) {
            self.ensure_in_order(display, key);
        }
        self.assignment.insert(key, display);

        if let Some(expected) = self.expected.get(&key)
            && rects_within(*expected, bounds, GEOMETRY_TOLERANCE)
        {
            self.diagnostics.suppressed_geometry_events += 1;
            return;
        }
        self.mark_display_dirty(display);
    }

    fn observe_app(&mut self, event: &AppEvent, displays: &[Display]) {
        if matches!(
            event,
            AppEvent::Launched { .. } | AppEvent::Terminated { .. }
        ) {
            self.mark_all_dirty(displays);
        }
    }

    fn ensure_in_order(&mut self, display: DisplayId, key: WindowKey) {
        let list = self.order.entry(display).or_default();
        if !list.contains(&key) {
            list.push(key);
        }
    }

    fn remove_from_order(&mut self, display: DisplayId, key: WindowKey) {
        if let Some(list) = self.order.get_mut(&display) {
            list.retain(|candidate| *candidate != key);
        }
    }

    fn remove_from_all_orders(&mut self, key: WindowKey) {
        for list in self.order.values_mut() {
            list.retain(|candidate| *candidate != key);
        }
    }

    fn mark_all_dirty(&mut self, displays: &[Display]) {
        for display in displays {
            self.dirty.insert(display.id);
        }
    }
}

impl Default for TilingState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::event::{AppEvent, RuntimeWindowId};
    use crate::core::window::{ProcessId, WindowInfo};

    fn display(id: u32, main: bool) -> Display {
        Display {
            id: DisplayId(id),
            name: None,
            main,
            scale: 2.0,
            bounds: Rect::new(0.0, 0.0, 1000.0, 1000.0),
            usable: Rect::new(0.0, 25.0, 1000.0, 975.0),
        }
    }

    fn second_display() -> Display {
        Display {
            id: DisplayId(2),
            name: None,
            main: false,
            scale: 2.0,
            bounds: Rect::new(1000.0, 0.0, 1000.0, 1000.0),
            usable: Rect::new(1000.0, 25.0, 1000.0, 975.0),
        }
    }

    fn displays() -> Vec<Display> {
        vec![display(1, true), display(2, false)]
    }

    fn key(pid: i32, id: u64) -> WindowKey {
        WindowKey::new(ProcessId(pid), RuntimeWindowId::new(id))
    }

    fn info(pid: i32, app: &str, bounds: Rect) -> WindowInfo {
        WindowInfo {
            pid: ProcessId(pid),
            application: app.to_string(),
            title: None,
            bounds: Some(bounds),
        }
    }

    fn created(pid: i32, id: u64, bounds: Rect) -> DomainEvent {
        DomainEvent::window(WindowEvent::Created {
            pid: ProcessId(pid),
            wid: RuntimeWindowId::new(id),
            bounds: Some(bounds),
        })
    }

    fn moved(pid: i32, id: u64, bounds: Rect) -> DomainEvent {
        DomainEvent::window(WindowEvent::Moved {
            pid: ProcessId(pid),
            wid: RuntimeWindowId::new(id),
            bounds,
        })
    }

    fn destroyed(pid: i32, id: u64) -> DomainEvent {
        DomainEvent::window(WindowEvent::Destroyed {
            pid: ProcessId(pid),
            wid: RuntimeWindowId::new(id),
        })
    }

    #[test]
    fn seed_orders_and_schedules_all_displays() {
        let mut state = TilingState::new();
        let active = displays();
        // "B" sorts after "A".
        state.seed(
            &active,
            vec![
                (key(1, 1), info(1, "B", Rect::new(500.0, 0.0, 100.0, 100.0))),
                (key(1, 2), info(1, "A", Rect::new(100.0, 0.0, 100.0, 100.0))),
            ],
        );
        assert_eq!(state.order_for(DisplayId(1)), &[key(1, 2), key(1, 1)]);
        assert_eq!(state.due(Instant::now()).len(), 2);
    }

    #[test]
    fn created_window_is_appended_and_marks_dirty() {
        let mut state = TilingState::new();
        let active = displays();
        let now = Instant::now();
        state.record_applied(DisplayId(1), &[], now);
        assert!(state.due(now).is_empty());

        state.observe(&created(1, 1, Rect::new(10.0, 10.0, 100.0, 100.0)), &active);
        assert_eq!(state.order_for(DisplayId(1)), &[key(1, 1)]);
        assert_eq!(state.due(now), vec![]);
        assert_eq!(state.due(now + RETILE_SETTLE_INTERVAL), vec![DisplayId(1)]);
    }

    #[test]
    fn destroyed_window_is_removed_and_marks_dirty() {
        let mut state = TilingState::new();
        let active = displays();
        let now = Instant::now();
        state.observe(&created(1, 1, Rect::new(10.0, 10.0, 100.0, 100.0)), &active);
        state.record_applied(DisplayId(1), &[], now);

        state.observe(&destroyed(1, 1), &active);
        assert!(state.order_for(DisplayId(1)).is_empty());
        assert_eq!(state.due(now), vec![]);
        assert_eq!(state.due(now + RETILE_SETTLE_INTERVAL), vec![DisplayId(1)]);
    }

    #[test]
    fn self_generated_geometry_is_suppressed() {
        let mut state = TilingState::new();
        let active = displays();
        let now = Instant::now();
        let target = Rect::new(0.0, 25.0, 500.0, 500.0);
        state.observe(&created(1, 1, Rect::new(10.0, 10.0, 100.0, 100.0)), &active);
        state.record_applied(DisplayId(1), &[(key(1, 1), target)], now);

        // A move that matches the applied geometry is our own mutation.
        state.observe(&moved(1, 1, target), &active);
        assert!(state.due(now).is_empty());
        assert_eq!(state.diagnostics().suppressed_geometry_events, 1);
    }

    #[test]
    fn user_geometry_change_marks_dirty() {
        let mut state = TilingState::new();
        let active = displays();
        let now = Instant::now();
        let target = Rect::new(0.0, 25.0, 500.0, 500.0);
        state.observe(&created(1, 1, Rect::new(10.0, 10.0, 100.0, 100.0)), &active);
        state.record_applied(DisplayId(1), &[(key(1, 1), target)], now);

        state.observe(&moved(1, 1, Rect::new(200.0, 200.0, 300.0, 300.0)), &active);
        assert_eq!(state.due(now), vec![]);
        assert_eq!(state.due(now + RETILE_SETTLE_INTERVAL), vec![DisplayId(1)]);
    }

    #[test]
    fn focus_does_not_mark_dirty() {
        let mut state = TilingState::new();
        let active = displays();
        let now = Instant::now();
        state.record_applied(DisplayId(1), &[], now);
        state.observe(
            &DomainEvent::window(WindowEvent::Focused {
                pid: ProcessId(1),
                wid: RuntimeWindowId::new(1),
            }),
            &active,
        );
        assert!(state.due(now).is_empty());
    }

    #[test]
    fn settle_interval_debounces() {
        let mut state = TilingState::new();
        let active = displays();
        let now = Instant::now();
        state.observe(&created(1, 1, Rect::new(10.0, 10.0, 100.0, 100.0)), &active);
        assert_eq!(state.due(now), vec![DisplayId(1)]);
        state.record_applied(DisplayId(1), &[], now);

        state.observe(&created(1, 2, Rect::new(20.0, 20.0, 100.0, 100.0)), &active);
        assert!(state.due(now).is_empty());
        assert_eq!(state.due(now + RETILE_SETTLE_INTERVAL), vec![DisplayId(1)]);
    }

    #[test]
    fn no_reordering_on_title_or_geometry_changes() {
        let mut state = TilingState::new();
        let active = displays();
        state.seed(
            &active,
            vec![
                (key(1, 1), info(1, "B", Rect::new(500.0, 0.0, 100.0, 100.0))),
                (key(1, 2), info(1, "A", Rect::new(100.0, 0.0, 100.0, 100.0))),
            ],
        );
        let before = state.order_for(DisplayId(1)).to_vec();

        state.observe(&moved(1, 1, Rect::new(900.0, 900.0, 50.0, 50.0)), &active);
        assert_eq!(state.order_for(DisplayId(1)), before.as_slice());
    }

    #[test]
    fn swap_in_order_exchanges_positions() {
        let mut state = TilingState::new();
        let active = displays();
        state.seed(
            &active,
            vec![
                (key(1, 1), info(1, "A", Rect::new(100.0, 0.0, 100.0, 100.0))),
                (key(1, 2), info(1, "B", Rect::new(500.0, 0.0, 100.0, 100.0))),
            ],
        );
        assert!(state.swap_in_order(DisplayId(1), key(1, 1), key(1, 2)));
        assert_eq!(state.order_for(DisplayId(1)), &[key(1, 2), key(1, 1)]);
        assert!(!state.swap_in_order(DisplayId(1), key(1, 1), key(9, 9)));
    }

    #[test]
    fn set_layout_persists_per_display() {
        let mut state = TilingState::new();
        assert_eq!(
            state.layout_for(DisplayId(1)).layout,
            LayoutType::MasterStack
        );
        state.set_layout(DisplayId(1), LayoutType::Columns);
        assert_eq!(state.layout_for(DisplayId(1)).layout, LayoutType::Columns);
        assert_eq!(
            state.layout_for(DisplayId(2)).layout,
            LayoutType::MasterStack
        );
    }

    #[test]
    fn launched_application_marks_all_displays_dirty() {
        let mut state = TilingState::new();
        let active = displays();
        let now = Instant::now();
        state.record_applied(DisplayId(1), &[], now);
        state.record_applied(DisplayId(2), &[], now);
        state.observe(
            &DomainEvent::app(AppEvent::Launched {
                pid: ProcessId(1),
                name: "App".to_string(),
            }),
            &active,
        );
        assert_eq!(state.due(now), vec![]);
        assert_eq!(
            state.due(now + RETILE_SETTLE_INTERVAL),
            vec![DisplayId(1), DisplayId(2)]
        );
    }

    #[test]
    fn window_moving_displays_reassigns_order() {
        let mut state = TilingState::new();
        let mut active = displays();
        active.push(second_display());
        state.observe(&created(1, 1, Rect::new(10.0, 10.0, 100.0, 100.0)), &active);
        assert_eq!(state.order_for(DisplayId(1)), &[key(1, 1)]);

        // Move into the second display's coordinate space.
        state.observe(&moved(1, 1, Rect::new(1100.0, 10.0, 100.0, 100.0)), &active);
        assert!(state.order_for(DisplayId(1)).is_empty());
        assert_eq!(state.order_for(DisplayId(2)), &[key(1, 1)]);
    }
}
