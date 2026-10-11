//! Native macOS event observers.
//!
//! This module owns every native observer and translates Accessibility and
//! NSWorkspace notifications into platform-independent [`DomainEvent`]s.
//!
//! Threading model
//! ---------------
//! Everything here is designed to run on the process **main thread**, which
//! keeps the AppKit main-thread requirements satisfied and avoids sending
//! non-`Send` native handles across threads. [`run_loop_step`] drives a bounded
//! run-loop iteration; callbacks only enqueue normalized events. The dedicated
//! run-loop thread alternative was rejected because it would force native
//! element handles (which are not `Send`/`Sync`) across a thread boundary for no
//! functional gain.
//!
//! Ownership
//! ---------
//! * An `AXObserver` is created under the Create rule and released by
//!   [`AxObserver`] on drop.
//! * The run-loop source returned by `AXObserverGetRunLoopSource` is owned by
//!   the observer and is removed from the run loop before the observer drops.
//! * Callbacks receive a borrowed element and notification name that are only
//!   valid during the call; this module copies out what it needs.
//! * The observer callback is handed a raw pointer to the [`ObserverManager`].
//!   The manager must therefore outlive every registered observer.

use std::collections::HashMap;
use std::os::raw::c_void;
use std::ptr::{self, NonNull};

use block2::RcBlock;
use core_foundation::base::TCFType;
use core_foundation::string::CFString;
use core_foundation_sys::base::{CFEqual, CFRelease, CFRetain, CFTypeRef};
use core_foundation_sys::runloop::{
    CFRunLoopAddSource, CFRunLoopGetCurrent, CFRunLoopRemoveSource, CFRunLoopRunInMode,
    kCFRunLoopDefaultMode,
};
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, ProtocolObject};
use objc2_app_kit::{
    NSWorkspace, NSWorkspaceDidActivateApplicationNotification,
    NSWorkspaceDidDeactivateApplicationNotification, NSWorkspaceDidLaunchApplicationNotification,
    NSWorkspaceDidTerminateApplicationNotification,
};
use objc2_foundation::{NSNotification, NSNotificationCenter, NSObjectProtocol};

use crate::app::event_processor::DiscoveredWindow;
use crate::core::event::{AppEvent, DomainEvent, RuntimeWindowId, WindowEvent};
use crate::core::window::ProcessId;
use crate::platform::macos::accessibility::{
    self, AX_ERROR_SUCCESS, AXObserverRef, AXUIElementRef,
};
use crate::platform::macos::{applications, windows};

/// Accessibility notifications registered on the application element.
const APPLICATION_NOTIFICATIONS: [&str; 2] = [
    accessibility::AX_WINDOW_CREATED_NOTIFICATION,
    accessibility::AX_FOCUSED_WINDOW_CHANGED_NOTIFICATION,
];

/// Accessibility notifications registered on each individual window element.
const WINDOW_NOTIFICATIONS: [&str; 5] = [
    accessibility::AX_UI_ELEMENT_DESTROYED_NOTIFICATION,
    accessibility::AX_MOVED_NOTIFICATION,
    accessibility::AX_RESIZED_NOTIFICATION,
    accessibility::AX_WINDOW_MINIATURIZED_NOTIFICATION,
    accessibility::AX_WINDOW_DEMINIATURIZED_NOTIFICATION,
];

/// A runtime window identity anchored by one retained native element.
///
/// `AXUIElement` handles are not stable across independent attribute fetches,
/// so pointer addresses cannot identify a window: a fresh fetch of the same
/// window yields a different address. Instead we retain one canonical element
/// per window and match later handles with `CFEqual`, which compares the
/// underlying accessibility object. Runtime-scoped; never persisted.
struct IdentityEntry {
    pid: ProcessId,
    id: RuntimeWindowId,
    element: AXUIElementRef,
}

/// Failure modes for observer setup.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObserverError {
    /// `AXObserverCreate` failed.
    CreationFailed(i32),
    /// The application element could not be created.
    ApplicationElementUnavailable,
}

impl std::fmt::Display for ObserverError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ObserverError::CreationFailed(code) => {
                write!(f, "could not create accessibility observer (error {code})")
            }
            ObserverError::ApplicationElementUnavailable => {
                write!(f, "could not create the application accessibility element")
            }
        }
    }
}

impl std::error::Error for ObserverError {}

/// Counters describing observer setup and callback behavior.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ObserverDiagnostics {
    /// Observers that failed to be created.
    pub observer_creation_failures: usize,
    /// Notification registrations rejected by the target application.
    pub registration_failures: usize,
    /// Notifications rejected as unsupported by the target application.
    pub unsupported_notifications: usize,
    /// Callbacks referencing an element with no known identity.
    pub unknown_elements: usize,
    /// Observers removed during shutdown.
    pub observers_removed: usize,
    /// Enqueues that found the event queue at capacity.
    pub overflow_events: usize,
}

/// Maximum number of normalized events buffered before the loop drains them.
const EVENT_QUEUE_CAPACITY: usize = 1024;

/// Returns whether `event` is a coalescable geometry update.
fn is_geometry_event(event: &DomainEvent) -> bool {
    matches!(
        event,
        DomainEvent::Window {
            event: WindowEvent::Moved { .. } | WindowEvent::Resized { .. },
            ..
        }
    )
}

/// Owns a native `AXObserver` and releases it on drop.
struct AxObserver {
    observer: AXObserverRef,
}

impl Drop for AxObserver {
    fn drop(&mut self) {
        // SAFETY: the observer was created under the Create rule.
        unsafe { CFRelease(self.observer as CFTypeRef) };
    }
}

/// Owns native observers for every watched application and normalizes their
/// notifications into [`DomainEvent`]s.
pub struct ObserverManager {
    identity: Vec<IdentityEntry>,
    observers: HashMap<ProcessId, AxObserver>,
    observer_pids: HashMap<usize, ProcessId>,
    app_names: HashMap<ProcessId, String>,
    events: Vec<DomainEvent>,
    next_id: u64,
    active_pid: Option<ProcessId>,
    diagnostics: ObserverDiagnostics,
    workspace_center: Option<Retained<NSNotificationCenter>>,
    workspace_tokens: Vec<Retained<ProtocolObject<dyn NSObjectProtocol>>>,
}

impl ObserverManager {
    /// Creates an empty manager. No native observer is created yet.
    pub fn new() -> Self {
        Self {
            identity: Vec::new(),
            observers: HashMap::new(),
            observer_pids: HashMap::new(),
            app_names: HashMap::new(),
            events: Vec::new(),
            next_id: 1,
            active_pid: None,
            diagnostics: ObserverDiagnostics::default(),
            workspace_center: None,
            workspace_tokens: Vec::new(),
        }
    }

    pub fn diagnostics(&self) -> ObserverDiagnostics {
        self.diagnostics
    }

    /// Subscribes to NSWorkspace application lifecycle notifications.
    ///
    /// The callbacks only enqueue events; the caller processes them from the
    /// main loop. Must be called on the main thread before the run loop runs.
    pub fn subscribe_workspace(&mut self) {
        if self.workspace_center.is_some() {
            return;
        }
        let workspace = NSWorkspace::sharedWorkspace();
        let center = workspace.notificationCenter();
        let context = self as *mut ObserverManager as usize;

        let subscriptions: [(WorkspaceKind, &NSStringName); 4] = unsafe {
            [
                (
                    WorkspaceKind::Launch,
                    NSWorkspaceDidLaunchApplicationNotification,
                ),
                (
                    WorkspaceKind::Terminate,
                    NSWorkspaceDidTerminateApplicationNotification,
                ),
                (
                    WorkspaceKind::Activate,
                    NSWorkspaceDidActivateApplicationNotification,
                ),
                (
                    WorkspaceKind::Deactivate,
                    NSWorkspaceDidDeactivateApplicationNotification,
                ),
            ]
        };

        for (kind, name) in subscriptions {
            let block = RcBlock::new(move |_notification: NonNull<NSNotification>| {
                // SAFETY: the manager outlives the observer and callbacks only
                // run on the main thread.
                let manager = unsafe { &mut *(context as *mut ObserverManager) };
                manager.handle_workspace(kind);
            });
            // SAFETY: `name` is a valid notification name and `block` is a
            // well-formed callback; the center copies the block.
            let token = unsafe {
                center.addObserverForName_object_queue_usingBlock(Some(name), None, None, &block)
            };
            self.workspace_tokens.push(token);
        }

        self.workspace_center = Some(center);
    }

    /// Recomputes the set of running applications and enqueues the difference
    /// as launch and termination events.
    pub fn sync_applications(&mut self) {
        let current: HashMap<ProcessId, String> = applications::running_applications()
            .into_iter()
            .map(|app| (app.pid, app.name))
            .collect();

        let mut launched: Vec<(ProcessId, String)> = current
            .iter()
            .filter(|(pid, _)| !self.app_names.contains_key(pid))
            .map(|(pid, name)| (*pid, name.clone()))
            .collect();
        launched.sort_by_key(|(pid, _)| pid.0);

        let terminated: Vec<ProcessId> = self
            .app_names
            .keys()
            .filter(|pid| !current.contains_key(pid))
            .copied()
            .collect();

        for (pid, name) in launched {
            self.enqueue(DomainEvent::app(AppEvent::Launched { pid, name }));
        }
        for pid in terminated {
            self.enqueue(DomainEvent::app(AppEvent::Terminated { pid }));
        }

        self.app_names = current;
    }

    /// Registers accessibility observers for `pid`.
    pub fn register_application(
        &mut self,
        pid: ProcessId,
        name: &str,
    ) -> Result<(), ObserverError> {
        if self.observers.contains_key(&pid) {
            return Ok(());
        }

        let mut observer: AXObserverRef = ptr::null();
        // SAFETY: `callback` is a valid AXObserver callback and `observer`
        // points to writable storage.
        let error =
            unsafe { accessibility::AXObserverCreate(pid.0, ax_observer_callback, &mut observer) };
        if error != AX_ERROR_SUCCESS || observer.is_null() {
            self.diagnostics.observer_creation_failures += 1;
            return Err(ObserverError::CreationFailed(error));
        }

        // SAFETY: `pid` comes from NSWorkspace; the returned element is owned.
        let application_element = unsafe { accessibility::AXUIElementCreateApplication(pid.0) };
        if application_element.is_null() {
            // SAFETY: the observer was created under the Create rule.
            unsafe { CFRelease(observer as CFTypeRef) };
            return Err(ObserverError::ApplicationElementUnavailable);
        }
        let _application_guard =
            unsafe { accessibility::CfGuard::new(application_element as CFTypeRef) };

        let context = self as *mut ObserverManager as *mut c_void;
        for notification in APPLICATION_NOTIFICATIONS {
            self.try_register(observer, application_element, notification, context);
        }
        windows::for_each_window(pid, |element| {
            for notification in WINDOW_NOTIFICATIONS {
                self.try_register(observer, element, notification, context);
            }
        });

        // SAFETY: `observer` is valid; the source is owned by the observer. It
        // is added to the main run loop.
        let source = unsafe { accessibility::AXObserverGetRunLoopSource(observer) };
        // SAFETY: `source` is valid and the mode is the default run loop mode.
        unsafe { CFRunLoopAddSource(CFRunLoopGetCurrent(), source, kCFRunLoopDefaultMode) };

        self.observer_pids.insert(observer as usize, pid);
        self.observers.insert(pid, AxObserver { observer });
        self.app_names.insert(pid, name.to_string());
        Ok(())
    }

    /// Removes the observers and identities of `pid`.
    pub fn remove_application(&mut self, pid: ProcessId) {
        if let Some(observer) = self.observers.remove(&pid) {
            // SAFETY: the source is owned by the observer and was added to the
            // main run loop; remove it before releasing the observer.
            let source = unsafe { accessibility::AXObserverGetRunLoopSource(observer.observer) };
            unsafe { CFRunLoopRemoveSource(CFRunLoopGetCurrent(), source, kCFRunLoopDefaultMode) };
            self.observer_pids.remove(&(observer.observer as usize));
            self.diagnostics.observers_removed += 1;
        }
        self.release_identity_for(pid);
        self.app_names.remove(&pid);
    }

    /// Removes every observer and releases native resources.
    pub fn shutdown(&mut self) {
        let pids: Vec<ProcessId> = self.observers.keys().copied().collect();
        for pid in pids {
            self.remove_application(pid);
        }
        if let Some(center) = self.workspace_center.take() {
            for token in self.workspace_tokens.drain(..) {
                let object: &AnyObject = token.as_ref();
                // SAFETY: the token was returned by this center and has not been
                // removed yet.
                unsafe { center.removeObserver(object) };
            }
        }
        self.release_all_identity();
    }

    /// Drains and returns the events queued by callbacks.
    pub fn drain_events(&mut self) -> Vec<DomainEvent> {
        std::mem::take(&mut self.events)
    }

    /// Interns `element` under `pid` and returns its stable runtime identity.
    ///
    /// Used by automatic tiling (SPEC-009 FR-016) so control-discovered windows
    /// share the identity space of the event registry. Must run on the main
    /// thread.
    pub fn resolve_identity(&mut self, pid: ProcessId, element: AXUIElementRef) -> RuntimeWindowId {
        self.intern_identity(pid, element)
    }

    /// Enqueues a normalized event without blocking a native callback.
    ///
    /// The queue is capped at [`EVENT_QUEUE_CAPACITY`]. On overflow the oldest
    /// coalescable geometry event is dropped first, because it is superseded by
    /// newer geometry; creation, destruction, and application lifecycle events
    /// are never silently discarded. Every overflow is counted so diagnostics
    /// can surface it.
    fn enqueue(&mut self, event: DomainEvent) {
        if self.events.len() >= EVENT_QUEUE_CAPACITY {
            self.diagnostics.overflow_events += 1;
            if let Some(index) = self.events.iter().position(is_geometry_event) {
                self.events.remove(index);
            }
        }
        self.events.push(event);
    }

    /// Returns the runtime identity whose canonical element `CFEqual`s
    /// `element`, if it is already known.
    fn find_identity(&self, pid: ProcessId, element: AXUIElementRef) -> Option<RuntimeWindowId> {
        self.identity
            .iter()
            .find(|entry| {
                entry.pid == pid
                    // SAFETY: both references are valid AXUIElements.
                    && unsafe { CFEqual(entry.element as CFTypeRef, element as CFTypeRef) } != 0
            })
            .map(|entry| entry.id)
    }

    /// Returns the identity of `element`, assigning and anchoring a new one on
    /// first sight.
    fn intern_identity(&mut self, pid: ProcessId, element: AXUIElementRef) -> RuntimeWindowId {
        if let Some(id) = self.find_identity(pid, element) {
            return id;
        }
        let id = self.allocate_id();
        // SAFETY: `element` is valid for the call; retain it so the canonical
        // handle stays valid for later `CFEqual` comparisons.
        let element = unsafe { CFRetain(element as CFTypeRef) as AXUIElementRef };
        self.identity.push(IdentityEntry { pid, id, element });
        id
    }

    fn release_identity_for(&mut self, pid: ProcessId) {
        let mut kept = Vec::with_capacity(self.identity.len());
        for entry in self.identity.drain(..) {
            if entry.pid == pid {
                // SAFETY: the element was retained by `intern_identity`.
                unsafe { CFRelease(entry.element as CFTypeRef) };
            } else {
                kept.push(entry);
            }
        }
        self.identity = kept;
    }

    fn release_all_identity(&mut self) {
        for entry in self.identity.drain(..) {
            // SAFETY: the element was retained by `intern_identity`.
            unsafe { CFRelease(entry.element as CFTypeRef) };
        }
    }

    /// Builds a fresh snapshot of every window belonging to a watched
    /// application, assigning runtime identities on first sight.
    ///
    /// Used for initial synchronization and reconciliation. It does not emit
    /// events; the caller feeds the snapshot to the reconciliation path.
    pub fn snapshot(&mut self) -> Vec<DiscoveredWindow> {
        let watched: Vec<(ProcessId, String)> = self
            .observers
            .keys()
            .map(|pid| (*pid, self.app_names.get(pid).cloned().unwrap_or_default()))
            .collect();

        let mut result = Vec::new();
        for (pid, name) in watched {
            windows::for_each_window(pid, |element| {
                let id = self.intern_identity(pid, element);
                let title = windows::copy_string(element, accessibility::AX_TITLE_ATTRIBUTE)
                    .filter(|title| !title.is_empty());
                let minimized = windows::copy_bool(element, accessibility::AX_MINIMIZED_ATTRIBUTE)
                    .unwrap_or(false);
                result.push(DiscoveredWindow {
                    pid,
                    id,
                    application: name.clone(),
                    title,
                    bounds: windows::window_bounds(element),
                    minimized,
                });
            });
        }
        result
    }

    fn try_register(
        &mut self,
        observer: AXObserverRef,
        element: AXUIElementRef,
        notification: &str,
        context: *mut c_void,
    ) {
        let name = CFString::new(notification);
        // SAFETY: `observer` and `element` are valid; `name` is a valid
        // CFString; `context` is the manager pointer.
        let error = unsafe {
            accessibility::AXObserverAddNotification(
                observer,
                element,
                name.as_concrete_TypeRef(),
                context,
            )
        };
        if error == AX_ERROR_SUCCESS {
            return;
        }
        if error == accessibility::AX_ERROR_NOTIFICATION_UNSUPPORTED {
            self.diagnostics.unsupported_notifications += 1;
        } else {
            self.diagnostics.registration_failures += 1;
        }
    }

    fn allocate_id(&mut self) -> RuntimeWindowId {
        let id = RuntimeWindowId::new(self.next_id);
        self.next_id += 1;
        id
    }

    /// Resolves the runtime identity of `element`, assigning one and reporting
    /// its creation when it is seen for the first time.
    fn identify(&mut self, pid: ProcessId, element: AXUIElementRef) -> Option<RuntimeWindowId> {
        let observer = self.observers.get(&pid)?.observer;
        if let Some(id) = self.find_identity(pid, element) {
            return Some(id);
        }
        let id = self.intern_identity(pid, element);

        let context = self as *mut ObserverManager as *mut c_void;
        for notification in WINDOW_NOTIFICATIONS {
            self.try_register(observer, element, notification, context);
        }
        let bounds = windows::window_bounds(element);
        self.enqueue(DomainEvent::window(WindowEvent::Created {
            pid,
            wid: id,
            bounds,
        }));
        Some(id)
    }

    fn handle_workspace(&mut self, kind: WorkspaceKind) {
        match kind {
            WorkspaceKind::Launch | WorkspaceKind::Terminate => self.sync_applications(),
            WorkspaceKind::Activate | WorkspaceKind::Deactivate => self.sync_activation(),
        }
    }

    fn sync_activation(&mut self) {
        let Some(front) = applications::frontmost_application() else {
            return;
        };
        if self.active_pid == Some(front.pid) {
            return;
        }
        if let Some(previous) = self.active_pid {
            self.enqueue(DomainEvent::app(AppEvent::Deactivated { pid: previous }));
        }
        self.active_pid = Some(front.pid);
        self.enqueue(DomainEvent::app(AppEvent::Activated { pid: front.pid }));
    }

    fn handle_ax(&mut self, observer: AXObserverRef, element: AXUIElementRef, name: &str) {
        let Some(pid) = self.observer_pids.get(&(observer as usize)).copied() else {
            return;
        };

        match name {
            accessibility::AX_WINDOW_CREATED_NOTIFICATION => self.handle_window_created(pid),
            accessibility::AX_FOCUSED_WINDOW_CHANGED_NOTIFICATION => self.handle_focus_changed(pid),
            accessibility::AX_UI_ELEMENT_DESTROYED_NOTIFICATION => {
                self.handle_destroyed(pid, element)
            }
            accessibility::AX_MOVED_NOTIFICATION => self.handle_geometry(pid, element, false),
            accessibility::AX_RESIZED_NOTIFICATION => self.handle_geometry(pid, element, true),
            accessibility::AX_WINDOW_MINIATURIZED_NOTIFICATION => {
                self.handle_minimized(pid, element, true)
            }
            accessibility::AX_WINDOW_DEMINIATURIZED_NOTIFICATION => {
                self.handle_minimized(pid, element, false)
            }
            _ => {}
        }
    }

    fn handle_window_created(&mut self, pid: ProcessId) {
        windows::for_each_window(pid, |element| {
            // `identify` emits a Created event only for genuinely new elements.
            let _ = self.identify(pid, element);
        });
    }

    fn handle_focus_changed(&mut self, pid: ProcessId) {
        // SAFETY: `pid` comes from NSWorkspace; the returned element is owned.
        let application_element = unsafe { accessibility::AXUIElementCreateApplication(pid.0) };
        if application_element.is_null() {
            return;
        }
        let guard = windows::copy_attribute(
            application_element,
            accessibility::AX_FOCUSED_WINDOW_ATTRIBUTE,
        );
        // SAFETY: the application element was created under the Create rule.
        unsafe { CFRelease(application_element as CFTypeRef) };

        let Some(guard) = guard else {
            return;
        };
        let element = guard.as_ref() as AXUIElementRef;
        if element.is_null() {
            return;
        }
        if let Some(id) = self.identify(pid, element) {
            self.enqueue(DomainEvent::window(WindowEvent::Focused { pid, wid: id }));
        }
    }

    fn handle_destroyed(&mut self, pid: ProcessId, element: AXUIElementRef) {
        let position = self.identity.iter().position(|entry| {
            entry.pid == pid
                // SAFETY: both references are valid AXUIElements.
                && unsafe { CFEqual(entry.element as CFTypeRef, element as CFTypeRef) } != 0
        });
        match position {
            Some(index) => {
                let entry = self.identity.remove(index);
                // SAFETY: the element was retained by `intern_identity`.
                unsafe { CFRelease(entry.element as CFTypeRef) };
                self.enqueue(DomainEvent::window(WindowEvent::Destroyed {
                    pid,
                    wid: entry.id,
                }));
            }
            None => self.diagnostics.unknown_elements += 1,
        }
    }

    fn handle_geometry(&mut self, pid: ProcessId, element: AXUIElementRef, resized: bool) {
        let Some(id) = self.identify(pid, element) else {
            self.diagnostics.unknown_elements += 1;
            return;
        };
        let Some(bounds) = windows::window_bounds(element) else {
            return;
        };
        let event = if resized {
            WindowEvent::Resized {
                pid,
                wid: id,
                bounds,
            }
        } else {
            WindowEvent::Moved {
                pid,
                wid: id,
                bounds,
            }
        };
        self.enqueue(DomainEvent::window(event));
    }

    fn handle_minimized(&mut self, pid: ProcessId, element: AXUIElementRef, minimized: bool) {
        let Some(id) = self.identify(pid, element) else {
            self.diagnostics.unknown_elements += 1;
            return;
        };
        let event = if minimized {
            WindowEvent::Minimized { pid, wid: id }
        } else {
            WindowEvent::Restored { pid, wid: id }
        };
        self.enqueue(DomainEvent::window(event));
    }
}

impl Default for ObserverManager {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for ObserverManager {
    fn drop(&mut self) {
        self.release_all_identity();
    }
}

/// Workspace notification categories processed by the manager.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WorkspaceKind {
    Launch,
    Terminate,
    Activate,
    Deactivate,
}

/// Convenience alias for the notification name statics.
type NSStringName = objc2_foundation::NSNotificationName;

/// Callback installed on every `AXObserver`.
///
/// # Safety
///
/// `refcon` must be a pointer to an [`ObserverManager`] that outlives the
/// observer. `element` and `notification` are borrowed for the call only.
unsafe extern "C" fn ax_observer_callback(
    observer: AXObserverRef,
    element: AXUIElementRef,
    notification: core_foundation_sys::string::CFStringRef,
    refcon: *mut c_void,
) {
    if refcon.is_null() {
        return;
    }
    // SAFETY: `notification` is a valid CFString borrowed for this call.
    let name = unsafe { CFString::wrap_under_get_rule(notification) }.to_string();
    // SAFETY: `refcon` points to the manager provided at registration time.
    let manager = unsafe { &mut *(refcon as *mut ObserverManager) };
    manager.handle_ax(observer, element, &name);
}

/// Runs one bounded run-loop iteration, processing native callbacks.
///
/// Must be called on the main thread. The timeout keeps the loop responsive to
/// shutdown checks and geometry coalescing without busy-waiting.
pub fn run_loop_step(timeout_seconds: f64) {
    // SAFETY: `CFRunLoopRunInMode` runs the current thread's run loop for the
    // requested time and returns; the default mode is valid.
    unsafe {
        CFRunLoopRunInMode(kCFRunLoopDefaultMode, timeout_seconds, 0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::geometry::Rect;

    fn app_event(pid: i32, name: &str) -> DomainEvent {
        DomainEvent::app(AppEvent::Launched {
            pid: ProcessId(pid),
            name: name.to_string(),
        })
    }

    fn moved_event(pid: i32, wid: u64) -> DomainEvent {
        DomainEvent::window(WindowEvent::Moved {
            pid: ProcessId(pid),
            wid: RuntimeWindowId::new(wid),
            bounds: Rect::new(0.0, 0.0, 10.0, 10.0),
        })
    }

    fn drained_is_geometry(event: &DomainEvent) -> bool {
        is_geometry_event(event)
    }

    #[test]
    fn drain_empties_the_queue() {
        let mut manager = ObserverManager::new();
        manager.enqueue(app_event(1, "a"));
        assert_eq!(manager.drain_events().len(), 1);
        assert!(manager.drain_events().is_empty());
    }

    #[test]
    fn overflow_evicts_geometry_before_lifecycle() {
        let mut manager = ObserverManager::new();
        manager.enqueue(moved_event(1, 1));
        for index in 0..EVENT_QUEUE_CAPACITY - 1 {
            manager.enqueue(app_event(2, &format!("app-{index}")));
        }
        assert_eq!(manager.events.len(), EVENT_QUEUE_CAPACITY);

        manager.enqueue(app_event(3, "extra"));

        assert_eq!(manager.diagnostics.overflow_events, 1);
        assert_eq!(manager.events.len(), EVENT_QUEUE_CAPACITY);
        assert!(
            manager
                .events
                .iter()
                .all(|event| !drained_is_geometry(event))
        );
        assert!(manager.events.iter().any(|event| matches!(
            event,
            DomainEvent::App {
                event: AppEvent::Launched { name, .. },
                ..
            } if name == "extra"
        )));
    }

    #[test]
    fn lifecycle_overflow_is_counted_and_never_dropped() {
        let mut manager = ObserverManager::new();
        let total = EVENT_QUEUE_CAPACITY + 5;
        for index in 0..total {
            manager.enqueue(app_event(1, &format!("app-{index}")));
        }
        assert_eq!(manager.diagnostics.overflow_events, 5);
        assert_eq!(manager.events.len(), total);
    }
}
