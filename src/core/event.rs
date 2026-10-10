//! Platform-independent domain events emitted by the window observation system.
//!
//! Native Accessibility references must never cross this boundary.

use std::time::Instant;

use crate::core::geometry::Rect;
use crate::core::window::ProcessId;

/// Runtime-scoped window identifier assigned on registry insertion.
///
/// This identifier is non-persistent and process-scoped. It must never be
/// serialized or used to retain native Accessibility elements across
/// executions.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RuntimeWindowId(pub u64);

impl RuntimeWindowId {
    pub const fn new(id: u64) -> Self {
        Self(id)
    }
}

/// Event metadata attached to every domain event.
#[derive(Clone, Debug, PartialEq)]
pub struct EventMetadata {
    pub timestamp: Instant,
}

/// Application lifecycle events.
#[derive(Clone, Debug, PartialEq)]
pub enum AppEvent {
    /// A new application was launched.
    Launched { pid: ProcessId, name: String },
    /// An application terminated.
    Terminated { pid: ProcessId },
    /// An application became active (frontmost).
    Activated { pid: ProcessId },
    /// An application became inactive.
    Deactivated { pid: ProcessId },
}

/// Window lifecycle and state events.
#[derive(Clone, Debug, PartialEq)]
pub enum WindowEvent {
    /// A window was created.
    Created {
        pid: ProcessId,
        wid: RuntimeWindowId,
        bounds: Option<Rect>,
    },
    /// A window was destroyed.
    Destroyed {
        pid: ProcessId,
        wid: RuntimeWindowId,
    },
    /// A window moved to a new position.
    Moved {
        pid: ProcessId,
        wid: RuntimeWindowId,
        bounds: Rect,
    },
    /// A window was resized.
    Resized {
        pid: ProcessId,
        wid: RuntimeWindowId,
        bounds: Rect,
    },
    /// A window gained focus.
    Focused {
        pid: ProcessId,
        wid: RuntimeWindowId,
    },
    /// A window was minimized.
    Minimized {
        pid: ProcessId,
        wid: RuntimeWindowId,
    },
    /// A window was restored from minimized state.
    Restored {
        pid: ProcessId,
        wid: RuntimeWindowId,
    },
}

/// Normalized platform-independent event.
#[derive(Clone, Debug, PartialEq)]
pub enum DomainEvent {
    App {
        meta: EventMetadata,
        event: AppEvent,
    },
    Window {
        meta: EventMetadata,
        event: WindowEvent,
    },
}

impl DomainEvent {
    pub fn app(event: AppEvent) -> Self {
        Self::app_at(event, Instant::now())
    }

    pub fn window(event: WindowEvent) -> Self {
        Self::window_at(event, Instant::now())
    }

    pub fn app_at(event: AppEvent, timestamp: Instant) -> Self {
        Self::App {
            meta: EventMetadata { timestamp },
            event,
        }
    }

    pub fn window_at(event: WindowEvent, timestamp: Instant) -> Self {
        Self::Window {
            meta: EventMetadata { timestamp },
            event,
        }
    }
}
