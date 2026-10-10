# SPEC-006: Native macOS Window Event System

**Status:** Approved  
**Priority:** Critical  
**Dependencies:** SPEC-001, SPEC-002, SPEC-003  
**Integration:** SPEC-005 — Layout Application  
**Next:** SPEC-007 — Shortcut Overlay Window

> **Revision:** Amendments approved during specification review are folded
> inline and marked "Amendment". They resolve run-loop ownership (main thread),
> runtime identity stability (`CFEqual`-anchored, non-persistent), startup
> phase ordering, queue bounds, FFI module isolation, the error model, and
> native handle thread-affinity.

## 1. Objective

Implement a reliable, event-driven window observation system for a native macOS tiling window manager written in Rust.

The system must detect relevant application and window lifecycle events using public macOS APIs, normalize them into platform-independent domain events, and maintain an accurate runtime window registry.

This specification establishes the event infrastructure required for future automatic tiling.

The system must not automatically move, resize, or reorganize windows.

## 2. Architectural Principles

The implementation MUST:

- Use public macOS APIs exclusively.
- Prefer native event notifications over continuous polling.
- Keep native observers isolated inside the macOS platform layer.
- Normalize native notifications into domain events.
- Maintain explicit observer lifetimes.
- Handle application and window termination safely.
- Avoid blocking native callback threads.
- Preserve existing window discovery and control behavior.
- Support clean startup and shutdown.
- Remain compatible with the future automatic tiling system.

The implementation MUST NOT:

- Apply layouts automatically.
- Modify window geometry.
- Introduce virtual workspaces.
- Use private macOS APIs.
- Disable System Integrity Protection.
- Introduce unrestricted global mutable state.
- Implement infinite polling loops.
- Assume every application supports every Accessibility notification.

## 3. Native Event Sources

### FR-001: Accessibility Observers

Implement native Accessibility observers using AXObserver APIs.

The system MUST:

- Create observers for eligible application processes.
- Register supported Accessibility notifications.
- Attach observer run-loop sources to a valid CFRunLoop.
- Keep observers alive for their required lifetime.
- Remove observers and release resources during shutdown.
- Handle observer creation and registration failures.
- Avoid duplicate subscriptions.
- Respect Accessibility authorization requirements.

The implementation MUST document native ownership and callback lifetime assumptions.

**Amendment (FFI isolation).** All `AXObserver` FFI declarations (types, error
codes, and notification-name constants) live in
`platform/macos/accessibility.rs`. Observer construction, callback wiring, and
event normalization live in `platform/macos/observers.rs`. No native handle is
exposed to `core`, `app`, or the commands layer.

### FR-002: Application Lifecycle Notifications

Use public macOS application lifecycle notifications where appropriate.

The system MUST detect:

- Application launched.
- Application terminated.
- Application activated.
- Application deactivated, when available.

The implementation SHOULD use NSWorkspace notifications where appropriate.

When a new application launches, the system must attempt to register relevant observers.

When an application terminates, the system must release its observers and invalidate associated window references.

### FR-003: Window Notifications

The system MUST attempt to observe supported notifications for:

- Window created.
- Window destroyed.
- Window moved.
- Window resized.
- Focused window changed.
- Window minimized.
- Window restored.

Relevant Accessibility notifications include:

- kAXWindowCreatedNotification.
- kAXUIElementDestroyedNotification.
- kAXMovedNotification.
- kAXResizedNotification.
- kAXFocusedWindowChangedNotification.
- kAXWindowMiniaturizedNotification.
- kAXWindowDeminiaturizedNotification.

Notification availability must be checked against the SDK and the target macOS version.

The system MUST tolerate applications that do not support specific notifications.

A successful subscription does not guarantee that an application will emit every expected event.

## 4. Domain Event Model

### FR-004: Event Normalization

Define platform-independent domain events.

Suggested event variants:

- ApplicationLaunched
- ApplicationTerminated
- ApplicationActivated
- ApplicationDeactivated
- WindowCreated
- WindowDestroyed
- WindowMoved
- WindowResized
- WindowFocused
- WindowMinimized
- WindowRestored

Each event MUST contain relevant metadata where available:

- Event type.
- Process ID.
- Internal window identifier, when resolvable.
- Event timestamp.
- Relevant geometry, when available.

Native AXUIElement references MUST NOT be exposed directly through the domain event API.

### FR-005: Event Identity

The system MUST:

- Distinguish applications by process identity.
- Distinguish multiple windows belonging to the same application.
- Reuse existing window identity abstractions where appropriate.
- Avoid relying solely on window titles.
- Avoid assuming process IDs uniquely identify windows.
- Avoid persisting native Accessibility references across executions.
- Handle unresolved window identities explicitly.

The design MUST address PID reuse and stale references.

**Amendment (identity stability).** Runtime window identity is a
non-persistent, process-scoped identifier (`RuntimeWindowId`) assigned when a
window is first interned in the registry. It is never serialized and never
persisted across executions.

The native `AXUIElement` handle MUST NOT be used as an identity token by
address: a fresh attribute fetch of the same window yields a different
address. Instead the platform layer retains one canonical element per window
under the Create rule and resolves later handles with `CFEqual`, which compares
the underlying accessibility object. Retained elements are released when the
owning application terminates and during shutdown.

If a window cannot be resolved to a known identity, the system MUST report the
event as an unknown reference rather than fabricate a new logical window.

## 5. Runtime Window Registry

### FR-006: Window State Registry

Implement an in-memory registry of currently known windows.

The registry SHOULD track:

- Internal window identifier.
- Owning application.
- Process identity.
- Last known geometry.
- Focus state.
- Minimized state.
- Manageability status.
- Associated display when known.

The registry MUST:

- Support window insertion.
- Support window removal.
- Support state updates.
- Prevent duplicate logical entries.
- Handle out-of-order events.
- Avoid exposing native references outside the platform layer.

The registry is runtime-only and must not persist window state to disk.

### FR-007: Initial State Synchronization

On startup, the system MUST:

1. Verify Accessibility authorization.
2. Discover currently running applications.
3. Discover accessible windows.
4. Initialize the runtime registry.
5. Register relevant observers.
6. Reconcile changes that occur during initialization.

The implementation MUST explicitly address the race between initial discovery and observer registration.

The exact synchronization strategy must be reviewed before implementation.

**Amendment (startup phases).** Startup MUST proceed in this order:

1. Verify Accessibility authorization and fail fast if it is denied.
2. Discover currently running applications.
3. Register observers for those applications.
4. Seed the runtime registry from a snapshot taken *after* registration.
5. Begin processing the event queue.

Because notifications emitted between steps 2 and 4 may be missed, the system
MUST perform one reconciliation pass after the initial snapshot to close the
race window. A window created during registration is therefore detected either
by its `AXWindowCreated` notification or by the reconciliation pass, never
lost.

### FR-008: Registry Reconciliation

The system MUST provide a bounded reconciliation mechanism for recovering from missed or unsupported notifications.

Reconciliation MUST:

- Use existing window discovery.
- Compare a fresh snapshot against registry state.
- Detect missing or newly discovered windows.
- Correct stale geometry when observable.
- Avoid modifying actual windows.
- Avoid continuous high-frequency polling.

Reconciliation MAY be triggered by:

- Application lifecycle changes.
- Observer registration failures.
- Detected state inconsistencies.
- Explicit user requests.

Periodic reconciliation is not required for this specification.

## 6. Event Processing

### FR-009: Event Queue

Native callbacks MUST remain lightweight.

The system MUST:

- Avoid expensive window enumeration inside callbacks.
- Avoid layout calculations inside callbacks.
- Forward normalized events to a processing component.
- Preserve ordering where guaranteed by the event source.
- Document ordering limitations across independent sources.
- Prevent unbounded memory growth.

A bounded channel or equivalent mechanism SHOULD be used.

**Amendment (queue bounds).** The event queue has a capacity of 1024 events
(`EVENT_QUEUE_CAPACITY`). Native callbacks MUST NOT block. On overflow the
oldest coalescable geometry event is evicted first, because newer geometry
supersedes it; every overflow is recorded as a diagnostic. Creation,
destruction, and application lifecycle events are never silently discarded:
under a pathological burst of lifecycle events the queue may briefly exceed the
cap, and that overflow is recorded rather than hidden.

### FR-010: Event Deduplication

The processor MUST handle repeated notifications.

Requirements:

- Avoid emitting redundant state changes.
- Preserve meaningful transitions.
- Do not discard creation or destruction events solely because they occur close together.
- Avoid assuming event timestamps uniquely identify events.
- Make deduplication behavior testable.

### FR-011: Event Coalescing

Window movement and resizing may generate bursts of notifications.

The system SHOULD:

- Coalesce redundant geometry updates.
- Preserve the most recent observable geometry.
- Use configurable internal coalescing intervals if necessary.
- Avoid delaying lifecycle events unnecessarily.
- Avoid introducing excessive timers or threads.

Coalescing MUST NOT corrupt window state.

### FR-012: Event Ordering

The system MUST account for:

- Notifications arriving after a window is destroyed.
- Duplicate destruction notifications.
- Focus events arriving before discovery.
- Application termination before observer cleanup.
- Geometry events for unknown windows.

The registry MUST remain internally consistent.

## 7. Native Lifecycle Management

### FR-013: Observer Lifecycle

The system MUST:

- Register observers when appropriate.
- Track registered notifications.
- Avoid duplicate observer registration.
- Remove subscriptions when applications terminate.
- Release Core Foundation resources correctly.
- Remove run-loop sources during shutdown.
- Handle callback contexts safely.

### FR-014: Run Loop Integration

The implementation MUST:

- Use a valid CFRunLoop for AXObserver sources.
- Respect AppKit main-thread requirements.
- Avoid blocking the run loop with expensive operations.
- Clearly define ownership of the event-processing loop.
- Avoid unnecessary async runtimes.

If a dedicated run-loop thread is required, justify its use and document thread ownership.

**Amendment (run-loop ownership — resolved).** The event loop runs on the
process **main thread**. `AXObserver` run-loop sources are attached to the main
`CFRunLoop`, which is driven in bounded iterations
(`CFRunLoopRunInMode` with a short timeout) so the loop can check a stop flag
between iterations. This satisfies the AppKit main-thread requirement and keeps
non-`Send`/non-`Sync` native handles confined to a single thread.

The dedicated run-loop thread alternative was evaluated and rejected: it would
force `AXUIElement` and observer handles across a thread boundary for no
functional gain and would add synchronization complexity. Thread ownership is
therefore: all native observers and their callbacks execute only on the main
thread; callbacks only enqueue events and perform no blocking work.

### FR-015: Graceful Shutdown

The system MUST support controlled shutdown.

Shutdown MUST:

- Stop accepting new subscriptions.
- Remove active observers.
- Release native resources.
- Stop event processing.
- Avoid use-after-free in native callbacks.
- Exit without leaving background processes running.

## 8. CLI Commands

### FR-016: Event Monitoring

Extend the CLI with:

`cargo run -- watch`

Starts a foreground event monitor.

The command MUST:

- Verify Accessibility permissions.
- Initialize the runtime registry.
- Register supported observers.
- Print normalized events.
- Continue running until interrupted.
- Shut down gracefully on Ctrl+C.

Example output:

`[12:30:01] WINDOW_CREATED pid=532 window=17`

`[12:30:02] WINDOW_MOVED pid=532 window=17 x=120 y=80`

`[12:30:04] WINDOW_RESIZED pid=532 window=17 width=800 height=600`

`[12:30:07] WINDOW_DESTROYED pid=532 window=17`

Window identifiers shown in output are runtime-scoped.

### FR-017: Event Diagnostics

The event monitor MUST report:

- Observer registration failures.
- Unsupported notifications.
- Permission errors.
- Unknown window references.
- Reconciliation failures.
- Event queue overflow.
- Cleanup failures when detectable.

Diagnostics MUST distinguish expected platform limitations from unexpected failures.

## 9. Performance Requirements

### NFR-001: Resource Efficiency

- No continuous high-frequency polling.
- No busy-wait loops.
- No unnecessary background processes.
- Avoid excessive native object retention.
- Avoid unbounded queues.
- Keep callback execution lightweight.

### NFR-002: Event Processing

- Process events incrementally.
- Avoid full window rediscovery for every geometry event.
- Avoid unnecessary registry writes.
- Do not invoke layout application from callbacks.

### NFR-003: Thread Safety

- Respect native API thread-affinity requirements.
- Do not assume AXUIElement or CFRunLoop handles are Send or Sync.
- Avoid unsafe Send/Sync implementations without documented justification.
- Synchronize shared state only when necessary.

**Amendment.** Native `AXUIElement` and `AXObserver` handles MUST never be moved
across threads. All observer registration, callback execution, and handle
release happen on the main thread.

## 10. Error Handling

### FR-018: Native Failures

Handle:

- Accessibility permission denied or revoked.
- Application termination during registration.
- Unsupported notifications.
- Invalid Accessibility elements.
- Native callback errors.
- Run-loop initialization failures.
- Event processing failures.
- Partial observer registration.

The system MUST degrade gracefully where possible.

One application's observer failure MUST NOT automatically terminate observation of unrelated applications.

**Amendment (error model).** Setup failures MUST be distinguishable:

- `ObserverCreationFailed` — `AXObserverCreate` returned an error.
- `NotificationRegistrationFailed` — a specific notification could not be added.
- `RunLoopStartFailed` — the run-loop source could not be attached.
- `UnsupportedNotification` — the target does not support a notification;
  expected platform limitation, not an error.

Unsupported notifications are counted separately from genuine registration
failures so diagnostics can separate expected limitations from defects.

## 11. Testing Strategy

### Unit Tests

Test:

- Domain event construction.
- Registry insertion and removal.
- Duplicate events.
- Out-of-order events.
- Stale geometry updates.
- Unknown window references.
- Event coalescing.
- Application termination cleanup.
- Reconciliation logic.
- Queue overflow policies.

### Integration Tests

Use mock event sources.

Verify:

- Events update registry state.
- Observer failures are isolated.
- Duplicate notifications do not corrupt state.
- Window destruction removes registry entries.
- Application termination removes owned windows.
- Reconciliation restores consistency.
- Shutdown releases logical resources.

### Manual macOS Tests

1. Run `cargo run -- watch`.
2. Open a new application window.
3. Verify creation detection.
4. Move the window.
5. Verify movement detection.
6. Resize the window.
7. Verify resize detection.
8. Focus another window.
9. Verify focus changes.
10. Minimize and restore a window.
11. Close a window.
12. Terminate an application.
13. Stop the monitor with Ctrl+C.
14. Verify clean shutdown.
15. Repeat with applications that expose incomplete Accessibility support.

Unsupported notifications must be documented rather than silently marked as verified.

## 12. Acceptance Criteria

### AC-001: Native Observation

Supported macOS window events are detected through native observers.

### AC-002: Application Lifecycle

Application launch and termination events are detected.

### AC-003: Event Normalization

Native events are converted into platform-independent domain events.

### AC-004: Window Registry

The runtime registry remains consistent during normal lifecycle operations.

### AC-005: Duplicate Handling

Duplicate notifications do not produce duplicate logical windows.

### AC-006: Out-of-Order Events

Unexpected event ordering does not crash the system or corrupt registry state.

### AC-007: Event Efficiency

The system avoids continuous high-frequency polling and unbounded event queues.

### AC-008: Observer Cleanup

Observers and native resources are released during shutdown.

### AC-009: Foreground CLI

The `watch` command displays events until interrupted.

### AC-010: Failure Isolation

Unsupported or failed subscriptions do not prevent observation of unrelated applications.

### AC-011: No Window Mutation

The event system does not move, resize, minimize, or focus windows.

### AC-012: Regression Safety

Existing SPEC-001 through SPEC-005 functionality remains intact.

### AC-013: Automated Validation

The following commands pass:

- `cargo fmt --all -- --check`
- `cargo check --all-targets`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test --all-targets`

### AC-014: Native Verification

Relevant supported events are manually verified on macOS.

## 13. Out of Scope

- Automatic layout application.
- Window geometry mutation.
- Global keyboard shortcuts.
- Background daemon installation.
- Persistent window state.
- Virtual workspaces.
- Native Spaces manipulation.
- Window animations.
- Layout recalculation.
- Automatic focus changes.
- Periodic high-frequency polling.
- Private macOS APIs.

## 14. Definition of Done

SPEC-006 is complete when:

- Native observers are functional.
- Application lifecycle events are detected.
- Supported window events are normalized.
- The runtime registry is maintained.
- Event processing handles duplicates and stale events.
- Reconciliation is available.
- The foreground CLI monitor works.
- Native observers shut down safely.
- Automated tests pass.
- Existing functionality remains intact.
- Native behavior is manually verified.

Any unavailable or unverified acceptance criterion must be explicitly documented.


