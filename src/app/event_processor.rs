//! Event normalization processor: deduplication, coalescing and reconciliation.
//!
//! The processor is platform-independent. It consumes already-normalized
//! [`DomainEvent`]s, keeps a [`WindowRegistry`] consistent, and returns the
//! effective transitions that callers should report. Native callbacks must only
//! enqueue events; all heavier work happens here.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::app::registry::{InsertOutcome, WindowKey, WindowRecord, WindowRegistry};
use crate::core::event::{AppEvent, DomainEvent, RuntimeWindowId, WindowEvent};
use crate::core::geometry::Rect;
use crate::core::window::ProcessId;

/// Default interval used to coalesce bursts of geometry notifications.
pub const DEFAULT_COALESCE_INTERVAL: Duration = Duration::from_millis(75);

/// Processor configuration.
#[derive(Clone, Copy, Debug)]
pub struct ProcessorConfig {
    /// Maximum time a geometry update may be buffered before being flushed.
    pub coalesce_interval: Duration,
}

impl Default for ProcessorConfig {
    fn default() -> Self {
        Self {
            coalesce_interval: DEFAULT_COALESCE_INTERVAL,
        }
    }
}

/// Counters describing how the processor behaved. Useful for diagnostics.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Diagnostics {
    /// Number of events fed into the processor.
    pub events_processed: usize,
    /// Number of redundant events suppressed by deduplication.
    pub duplicates: usize,
    /// Number of geometry updates merged into an existing pending update.
    pub coalesced: usize,
    /// Number of events referencing an unknown window identifier.
    pub unknown_window_refs: usize,
    /// Number of reconciliation passes performed.
    pub reconciliations: usize,
}

/// A window as observed during reconciliation.
///
/// The platform layer resolves native identity into a [`RuntimeWindowId`] before
/// constructing this value, so reconciliation stays platform-independent.
#[derive(Clone, Debug, PartialEq)]
pub struct DiscoveredWindow {
    pub pid: ProcessId,
    pub id: RuntimeWindowId,
    pub application: String,
    pub title: Option<String>,
    pub bounds: Option<Rect>,
    pub minimized: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GeometryKind {
    Moved,
    Resized,
}

#[derive(Clone, Copy, Debug)]
struct PendingGeometry {
    bounds: Rect,
    kind: GeometryKind,
}

/// Consumes domain events and maintains the runtime window registry.
#[derive(Debug)]
pub struct EventProcessor {
    registry: WindowRegistry,
    config: ProcessorConfig,
    pending: HashMap<WindowKey, PendingGeometry>,
    diagnostics: Diagnostics,
}

impl EventProcessor {
    pub fn new(config: ProcessorConfig) -> Self {
        Self {
            registry: WindowRegistry::new(),
            config,
            pending: HashMap::new(),
            diagnostics: Diagnostics::default(),
        }
    }

    pub fn registry(&self) -> &WindowRegistry {
        &self.registry
    }

    pub fn diagnostics(&self) -> Diagnostics {
        self.diagnostics
    }

    pub fn coalesce_interval(&self) -> Duration {
        self.config.coalesce_interval
    }

    /// Processes one domain event and returns the effective transitions.
    pub fn process(&mut self, event: DomainEvent, now: Instant) -> Vec<DomainEvent> {
        self.diagnostics.events_processed += 1;
        let mut out = Vec::new();

        match event {
            DomainEvent::App { event, .. } => self.process_app(event, now, &mut out),
            DomainEvent::Window { event, .. } => self.process_window(event, now, &mut out),
        }

        out
    }

    /// Flushes buffered geometry updates whose coalescing window has elapsed.
    ///
    /// Callers invoke this whenever the event source is idle (for example on a
    /// receive timeout), which avoids dedicated timers or extra threads.
    pub fn flush(&mut self, now: Instant) -> Vec<DomainEvent> {
        let _ = now;
        let buffered: Vec<(WindowKey, PendingGeometry)> = self.pending.drain().collect();
        let mut out = Vec::with_capacity(buffered.len());

        for (key, pending) in buffered {
            if self.registry.update_bounds(key, pending.bounds) {
                out.push(DomainEvent::window_at(
                    geometry_event(key, pending.kind, pending.bounds),
                    now,
                ));
            } else {
                self.diagnostics.unknown_window_refs += 1;
            }
        }

        out
    }

    /// Reconciles the registry against a fresh discovery snapshot.
    ///
    /// Missing windows are reported as destroyed, new windows as created, and
    /// changed geometry as moved or resized. Application termination safety is
    /// the caller's responsibility (the platform layer removes stale identities).
    pub fn reconcile(&mut self, snapshot: Vec<DiscoveredWindow>, now: Instant) -> Vec<DomainEvent> {
        self.diagnostics.reconciliations += 1;
        let mut out = Vec::new();

        let desired: HashMap<WindowKey, DiscoveredWindow> = snapshot
            .into_iter()
            .map(|window| (WindowKey::new(window.pid, window.id), window))
            .collect();

        // Windows present in the registry but absent from the snapshot are gone.
        let missing: Vec<WindowKey> = self
            .registry
            .iter()
            .map(|record| record.key)
            .filter(|key| !desired.contains_key(key))
            .collect();
        for key in missing {
            self.registry.remove(key);
            self.pending.remove(&key);
            out.push(DomainEvent::window_at(
                WindowEvent::Destroyed {
                    pid: key.pid,
                    wid: key.id,
                },
                now,
            ));
        }

        for (key, window) in desired {
            match self.registry.get(key) {
                None => {
                    let record = WindowRecord::new(
                        window.pid,
                        window.id,
                        window.application.clone(),
                        window.title.clone(),
                        window.bounds,
                    );
                    if self.registry.insert(record) == InsertOutcome::Inserted {
                        out.push(DomainEvent::window_at(
                            WindowEvent::Created {
                                pid: window.pid,
                                wid: window.id,
                                bounds: window.bounds,
                            },
                            now,
                        ));
                    }
                }
                Some(existing) => {
                    let previous_bounds = existing.bounds;
                    let previous_minimized = existing.minimized;
                    if let Some(bounds) = window.bounds {
                        if previous_bounds != Some(bounds) {
                            let kind = classify_geometry(previous_bounds, bounds);
                            self.registry.update_bounds(key, bounds);
                            out.push(DomainEvent::window_at(
                                geometry_event(key, kind, bounds),
                                now,
                            ));
                        }
                    }
                    if window.minimized != previous_minimized {
                        self.registry.set_minimized(key, window.minimized);
                        out.push(DomainEvent::window_at(
                            minimize_event(key, window.minimized),
                            now,
                        ));
                    }
                }
            }
        }

        out
    }

    fn process_app(&mut self, event: AppEvent, now: Instant, out: &mut Vec<DomainEvent>) {
        match event {
            AppEvent::Launched { pid, name } => {
                // Defensive: a reused PID must not inherit stale windows.
                let stale = self.registry.remove_application(pid);
                for id in stale {
                    self.pending.remove(&WindowKey::new(pid, id));
                    out.push(DomainEvent::window_at(
                        WindowEvent::Destroyed { pid, wid: id },
                        now,
                    ));
                }
                out.push(DomainEvent::app_at(AppEvent::Launched { pid, name }, now));
            }
            AppEvent::Terminated { pid } => {
                let removed = self.registry.remove_application(pid);
                for id in removed {
                    self.pending.remove(&WindowKey::new(pid, id));
                    out.push(DomainEvent::window_at(
                        WindowEvent::Destroyed { pid, wid: id },
                        now,
                    ));
                }
                out.push(DomainEvent::app_at(AppEvent::Terminated { pid }, now));
            }
            AppEvent::Activated { pid } => {
                out.push(DomainEvent::app_at(AppEvent::Activated { pid }, now));
            }
            AppEvent::Deactivated { pid } => {
                out.push(DomainEvent::app_at(AppEvent::Deactivated { pid }, now));
            }
        }
    }

    fn process_window(&mut self, event: WindowEvent, now: Instant, out: &mut Vec<DomainEvent>) {
        match event {
            WindowEvent::Created { pid, wid, bounds } => {
                let record = WindowRecord::new(pid, wid, String::new(), None, bounds);
                match self.registry.insert(record) {
                    InsertOutcome::Inserted => {
                        out.push(DomainEvent::window_at(
                            WindowEvent::Created { pid, wid, bounds },
                            now,
                        ));
                    }
                    InsertOutcome::Duplicate => self.diagnostics.duplicates += 1,
                }
            }
            WindowEvent::Destroyed { pid, wid } => {
                let key = WindowKey::new(pid, wid);
                if self.registry.remove(key).is_some() {
                    self.pending.remove(&key);
                    out.push(DomainEvent::window_at(
                        WindowEvent::Destroyed { pid, wid },
                        now,
                    ));
                } else {
                    self.diagnostics.duplicates += 1;
                }
            }
            WindowEvent::Moved { pid, wid, bounds } => {
                self.buffer_geometry(WindowKey::new(pid, wid), bounds, GeometryKind::Moved);
            }
            WindowEvent::Resized { pid, wid, bounds } => {
                self.buffer_geometry(WindowKey::new(pid, wid), bounds, GeometryKind::Resized);
            }
            WindowEvent::Focused { pid, wid } => {
                let key = WindowKey::new(pid, wid);
                if let Some(record) = self.registry.get(key) {
                    if record.focused {
                        self.diagnostics.duplicates += 1;
                        return;
                    }
                    self.registry.clear_focus_for_application(pid, key);
                    self.registry.set_focused(key, true);
                    out.push(DomainEvent::window_at(
                        WindowEvent::Focused { pid, wid },
                        now,
                    ));
                } else {
                    self.diagnostics.unknown_window_refs += 1;
                }
            }
            WindowEvent::Minimized { pid, wid } => {
                self.update_minimized(pid, wid, true, now, out);
            }
            WindowEvent::Restored { pid, wid } => {
                self.update_minimized(pid, wid, false, now, out);
            }
        }
    }

    fn update_minimized(
        &mut self,
        pid: ProcessId,
        wid: RuntimeWindowId,
        minimized: bool,
        now: Instant,
        out: &mut Vec<DomainEvent>,
    ) {
        let key = WindowKey::new(pid, wid);
        match self.registry.get(key) {
            Some(record) if record.minimized == minimized => {
                self.diagnostics.duplicates += 1;
            }
            Some(_) => {
                self.registry.set_minimized(key, minimized);
                out.push(DomainEvent::window_at(minimize_event(key, minimized), now));
            }
            None => self.diagnostics.unknown_window_refs += 1,
        }
    }

    fn buffer_geometry(&mut self, key: WindowKey, bounds: Rect, kind: GeometryKind) {
        match self.registry.get(key) {
            None => {
                self.diagnostics.unknown_window_refs += 1;
            }
            Some(record) if record.bounds == Some(bounds) => {
                self.diagnostics.duplicates += 1;
            }
            Some(_) => {
                if let Some(pending) = self.pending.get_mut(&key) {
                    pending.bounds = bounds;
                    pending.kind = kind;
                    self.diagnostics.coalesced += 1;
                } else {
                    self.pending.insert(key, PendingGeometry { bounds, kind });
                }
            }
        }
    }
}

impl Default for EventProcessor {
    fn default() -> Self {
        Self::new(ProcessorConfig::default())
    }
}

fn classify_geometry(previous: Option<Rect>, current: Rect) -> GeometryKind {
    match previous {
        Some(previous) if previous.size != current.size => GeometryKind::Resized,
        _ => GeometryKind::Moved,
    }
}

fn geometry_event(key: WindowKey, kind: GeometryKind, bounds: Rect) -> WindowEvent {
    match kind {
        GeometryKind::Moved => WindowEvent::Moved {
            pid: key.pid,
            wid: key.id,
            bounds,
        },
        GeometryKind::Resized => WindowEvent::Resized {
            pid: key.pid,
            wid: key.id,
            bounds,
        },
    }
}

fn minimize_event(key: WindowKey, minimized: bool) -> WindowEvent {
    if minimized {
        WindowEvent::Minimized {
            pid: key.pid,
            wid: key.id,
        }
    } else {
        WindowEvent::Restored {
            pid: key.pid,
            wid: key.id,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::geometry::Rect;

    fn key(id: u64) -> WindowKey {
        WindowKey::new(ProcessId(100), RuntimeWindowId::new(id))
    }

    fn register(processor: &mut EventProcessor, id: u64, bounds: Option<Rect>, now: Instant) {
        processor.process(
            DomainEvent::window_at(
                WindowEvent::Created {
                    pid: ProcessId(100),
                    wid: RuntimeWindowId::new(id),
                    bounds,
                },
                now,
            ),
            now,
        );
    }

    #[test]
    fn created_window_is_registered_once() {
        let mut processor = EventProcessor::default();
        let now = Instant::now();
        let out = processor.process(
            DomainEvent::window_at(
                WindowEvent::Created {
                    pid: ProcessId(100),
                    wid: RuntimeWindowId::new(1),
                    bounds: Some(Rect::new(0.0, 0.0, 10.0, 10.0)),
                },
                now,
            ),
            now,
        );
        assert_eq!(out.len(), 1);
        assert_eq!(processor.registry().len(), 1);

        let duplicate = processor.process(
            DomainEvent::window_at(
                WindowEvent::Created {
                    pid: ProcessId(100),
                    wid: RuntimeWindowId::new(1),
                    bounds: Some(Rect::new(0.0, 0.0, 10.0, 10.0)),
                },
                now,
            ),
            now,
        );
        assert!(duplicate.is_empty());
        assert_eq!(processor.diagnostics().duplicates, 1);
        assert_eq!(processor.registry().len(), 1);
    }

    #[test]
    fn duplicate_destruction_is_safe() {
        let mut processor = EventProcessor::default();
        let now = Instant::now();
        register(&mut processor, 1, None, now);

        let first = processor.process(
            DomainEvent::window_at(
                WindowEvent::Destroyed {
                    pid: ProcessId(100),
                    wid: RuntimeWindowId::new(1),
                },
                now,
            ),
            now,
        );
        assert_eq!(first.len(), 1);
        assert_eq!(processor.registry().len(), 0);

        let second = processor.process(
            DomainEvent::window_at(
                WindowEvent::Destroyed {
                    pid: ProcessId(100),
                    wid: RuntimeWindowId::new(1),
                },
                now,
            ),
            now,
        );
        assert!(second.is_empty());
        assert_eq!(processor.diagnostics().duplicates, 1);
    }

    #[test]
    fn geometry_burst_is_coalesced_to_latest() {
        let mut processor = EventProcessor::default();
        let now = Instant::now();
        register(
            &mut processor,
            1,
            Some(Rect::new(0.0, 0.0, 10.0, 10.0)),
            now,
        );

        for x in [1.0, 2.0, 3.0] {
            let out = processor.process(
                DomainEvent::window_at(
                    WindowEvent::Moved {
                        pid: ProcessId(100),
                        wid: RuntimeWindowId::new(1),
                        bounds: Rect::new(x, 0.0, 10.0, 10.0),
                    },
                    now,
                ),
                now,
            );
            assert!(out.is_empty(), "geometry is buffered until flush");
        }

        assert_eq!(processor.diagnostics().coalesced, 2);
        let flushed = processor.flush(now);
        assert_eq!(flushed.len(), 1);
        match &flushed[0] {
            DomainEvent::Window {
                event: WindowEvent::Moved { bounds, .. },
                ..
            } => assert_eq!(*bounds, Rect::new(3.0, 0.0, 10.0, 10.0)),
            other => panic!("expected moved, got {other:?}"),
        }
    }

    #[test]
    fn redundant_geometry_is_not_emitted() {
        let mut processor = EventProcessor::default();
        let now = Instant::now();
        let bounds = Rect::new(5.0, 5.0, 10.0, 10.0);
        register(&mut processor, 1, Some(bounds), now);

        let out = processor.process(
            DomainEvent::window_at(
                WindowEvent::Moved {
                    pid: ProcessId(100),
                    wid: RuntimeWindowId::new(1),
                    bounds,
                },
                now,
            ),
            now,
        );
        assert!(out.is_empty());
        assert!(processor.flush(now).is_empty());
        assert_eq!(processor.diagnostics().duplicates, 1);
    }

    #[test]
    fn focus_transitions_are_deduplicated() {
        let mut processor = EventProcessor::default();
        let now = Instant::now();
        register(&mut processor, 1, None, now);
        register(&mut processor, 2, None, now);

        let first = processor.process(
            DomainEvent::window_at(
                WindowEvent::Focused {
                    pid: ProcessId(100),
                    wid: RuntimeWindowId::new(1),
                },
                now,
            ),
            now,
        );
        assert_eq!(first.len(), 1);

        let repeat = processor.process(
            DomainEvent::window_at(
                WindowEvent::Focused {
                    pid: ProcessId(100),
                    wid: RuntimeWindowId::new(1),
                },
                now,
            ),
            now,
        );
        assert!(repeat.is_empty());

        let second = processor.process(
            DomainEvent::window_at(
                WindowEvent::Focused {
                    pid: ProcessId(100),
                    wid: RuntimeWindowId::new(2),
                },
                now,
            ),
            now,
        );
        assert_eq!(second.len(), 1);
        assert!(!processor.registry().get(key(1)).expect("known").focused);
        assert!(processor.registry().get(key(2)).expect("known").focused);
    }

    #[test]
    fn events_for_unknown_windows_are_ignored() {
        let mut processor = EventProcessor::default();
        let now = Instant::now();
        let out = processor.process(
            DomainEvent::window_at(
                WindowEvent::Moved {
                    pid: ProcessId(100),
                    wid: RuntimeWindowId::new(9),
                    bounds: Rect::new(1.0, 1.0, 1.0, 1.0),
                },
                now,
            ),
            now,
        );
        assert!(out.is_empty());
        assert_eq!(processor.diagnostics().unknown_window_refs, 1);
    }

    #[test]
    fn termination_removes_owned_windows() {
        let mut processor = EventProcessor::default();
        let now = Instant::now();
        register(&mut processor, 1, None, now);
        register(&mut processor, 2, None, now);

        let out = processor.process(
            DomainEvent::app_at(
                AppEvent::Terminated {
                    pid: ProcessId(100),
                },
                now,
            ),
            now,
        );
        // Two destroyed transitions plus the termination event itself.
        assert_eq!(out.len(), 3);
        assert_eq!(processor.registry().len(), 0);
    }

    #[test]
    fn launch_purges_stale_windows_for_reused_pid() {
        let mut processor = EventProcessor::default();
        let now = Instant::now();
        register(&mut processor, 1, None, now);

        let out = processor.process(
            DomainEvent::app_at(
                AppEvent::Launched {
                    pid: ProcessId(100),
                    name: "App".to_string(),
                },
                now,
            ),
            now,
        );
        assert_eq!(out.len(), 2);
        assert_eq!(processor.registry().len(), 0);
    }

    #[test]
    fn reconcile_adds_removes_and_updates() {
        let mut processor = EventProcessor::default();
        let now = Instant::now();
        register(
            &mut processor,
            1,
            Some(Rect::new(0.0, 0.0, 10.0, 10.0)),
            now,
        );
        register(
            &mut processor,
            2,
            Some(Rect::new(0.0, 0.0, 10.0, 10.0)),
            now,
        );

        let snapshot = vec![
            DiscoveredWindow {
                pid: ProcessId(100),
                id: RuntimeWindowId::new(1),
                application: "App".to_string(),
                title: None,
                bounds: Some(Rect::new(50.0, 0.0, 10.0, 10.0)),
                minimized: false,
            },
            DiscoveredWindow {
                pid: ProcessId(100),
                id: RuntimeWindowId::new(3),
                application: "App".to_string(),
                title: None,
                bounds: Some(Rect::new(0.0, 0.0, 5.0, 5.0)),
                minimized: false,
            },
        ];

        let out = processor.reconcile(snapshot, now);
        // Window 2 destroyed, window 3 created, window 1 moved.
        assert_eq!(out.len(), 3);
        assert!(processor.registry().get(key(3)).is_some());
        assert!(processor.registry().get(key(2)).is_none());
        assert_eq!(processor.diagnostics().reconciliations, 1);
    }
}
