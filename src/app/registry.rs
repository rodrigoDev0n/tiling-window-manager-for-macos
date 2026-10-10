//! Runtime-only window registry.
//!
//! The registry tracks the currently known windows as platform-independent
//! records. It never stores native Accessibility references and never persists
//! to disk. Native element handles stay inside the macOS platform layer.

use std::collections::HashMap;

use crate::core::event::RuntimeWindowId;
use crate::core::geometry::Rect;
use crate::core::window::ProcessId;

/// Logical key for one managed window.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct WindowKey {
    pub pid: ProcessId,
    pub id: RuntimeWindowId,
}

impl WindowKey {
    pub const fn new(pid: ProcessId, id: RuntimeWindowId) -> Self {
        Self { pid, id }
    }
}

/// Last-known platform-independent state of one window.
#[derive(Clone, Debug, PartialEq)]
pub struct WindowRecord {
    pub key: WindowKey,
    pub application: String,
    pub title: Option<String>,
    pub bounds: Option<Rect>,
    pub focused: bool,
    pub minimized: bool,
}

impl WindowRecord {
    pub fn new(
        pid: ProcessId,
        id: RuntimeWindowId,
        application: impl Into<String>,
        title: Option<String>,
        bounds: Option<Rect>,
    ) -> Self {
        Self {
            key: WindowKey::new(pid, id),
            application: application.into(),
            title,
            bounds,
            focused: false,
            minimized: false,
        }
    }
}

/// Outcome of inserting a window into the registry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InsertOutcome {
    /// The window was not previously known and was added.
    Inserted,
    /// A logical entry for the same key already existed; it was left unchanged.
    Duplicate,
}

/// In-memory registry of currently known windows.
#[derive(Debug, Default)]
pub struct WindowRegistry {
    windows: HashMap<WindowKey, WindowRecord>,
}

impl WindowRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the number of tracked windows.
    pub fn len(&self) -> usize {
        self.windows.len()
    }

    /// Returns a window record, if known.
    pub fn get(&self, key: WindowKey) -> Option<&WindowRecord> {
        self.windows.get(&key)
    }

    /// Inserts a window, preventing duplicate logical entries.
    pub fn insert(&mut self, record: WindowRecord) -> InsertOutcome {
        if self.windows.contains_key(&record.key) {
            return InsertOutcome::Duplicate;
        }
        self.windows.insert(record.key, record);
        InsertOutcome::Inserted
    }

    /// Removes and returns a window record, if known.
    pub fn remove(&mut self, key: WindowKey) -> Option<WindowRecord> {
        self.windows.remove(&key)
    }

    /// Removes every window owned by `pid`, returning the removed keys.
    pub fn remove_application(&mut self, pid: ProcessId) -> Vec<RuntimeWindowId> {
        let keys: Vec<WindowKey> = self
            .windows
            .keys()
            .filter(|key| key.pid == pid)
            .copied()
            .collect();
        let mut removed = Vec::with_capacity(keys.len());
        for key in keys {
            if self.windows.remove(&key).is_some() {
                removed.push(key.id);
            }
        }
        removed
    }

    /// Updates the last-known geometry. Returns whether the window was known.
    pub fn update_bounds(&mut self, key: WindowKey, bounds: Rect) -> bool {
        match self.windows.get_mut(&key) {
            Some(record) => {
                record.bounds = Some(bounds);
                true
            }
            None => false,
        }
    }

    /// Updates the focus flag. Returns whether the window was known.
    pub fn set_focused(&mut self, key: WindowKey, focused: bool) -> bool {
        match self.windows.get_mut(&key) {
            Some(record) => {
                record.focused = focused;
                true
            }
            None => false,
        }
    }

    /// Updates the minimized flag. Returns whether the window was known.
    pub fn set_minimized(&mut self, key: WindowKey, minimized: bool) -> bool {
        match self.windows.get_mut(&key) {
            Some(record) => {
                record.minimized = minimized;
                true
            }
            None => false,
        }
    }

    /// Clears the focus flag of every window owned by `pid` except `keep`.
    pub fn clear_focus_for_application(&mut self, pid: ProcessId, keep: WindowKey) {
        for record in self.windows.values_mut() {
            if record.key.pid == pid && record.key != keep {
                record.focused = false;
            }
        }
    }

    /// Iterates over every tracked window.
    pub fn iter(&self) -> impl Iterator<Item = &WindowRecord> {
        self.windows.values()
    }
}
