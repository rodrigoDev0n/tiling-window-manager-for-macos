# SPEC-009: Automatic Tiling

**Status:** Implemented  
**Priority:** Critical  
**Dependencies:** SPEC-001, SPEC-002, SPEC-003 (discovery/control/displays), SPEC-004 (layout engine), SPEC-005 (layout planner/executor), SPEC-006 (event system), SPEC-007 (overlay/run-loop), SPEC-008 (foreground manager, binding table)  
**Supersedes:** the SPEC-005 restriction that automatic tiling is out of scope, and the SPEC-008 FR-005 rule that `SetLayout` does not persist state.  
**Next:** SPEC-010 — Floating windows / per-application rules (not defined yet)

## 1. Objective

Make the foreground manager *automatically* keep windows tiled. When a relevant
window or application event occurs, the affected display is re-tiled to its
persistent layout, reusing the SPEC-005 planner/executor and the SPEC-006 event
stream.

The system must arrange windows on startup and after changes caused by the user
or by applications, without polling and without fighting its own mutations.

This specification makes the following working decisions (approved during
scoping):

1. `cargo run -- run` **enables automatic tiling by default**; `cargo run -- run
   --no-auto` preserves the exact SPEC-008 behavior (overlay + shortcuts, no
   automatic layout).
2. Automatic tiling manages **all active displays** independently, each with its
   own persistent layout state.
3. A window appearing or disappearing causes the **affected display to re-tile
   automatically** (insert on appear, reclaim space on disappear).

## 2. Architectural Principles

The implementation MUST:

- Use public macOS APIs exclusively.
- Keep all AppKit, Carbon and Accessibility code inside `platform/macos`.
- Reuse SPEC-006 observers/normalization (`EventProcessor`, `WindowRegistry`),
  the SPEC-005 planner/executor, and the SPEC-008 run loop and binding table.
- Introduce a new platform-independent controller that decides *when* and
  *whether* to re-tile; native work stays in the platform layer.
- Keep `core` free of AppKit/Carbon/AX types.
- Maintain a **persistent per-display tiling order** so tiling is stable and
  does not reshuffle when a window title changes.
- Prevent recursive re-tiling: programmatic geometry changes must not be
  mistaken for user changes.
- Be idempotent and convergent: repeated execution over unchanged state MUST
  not mutate windows or drift.
- Re-tile only displays that changed, and only after change bursts settle.
- Discover windows only when a re-tile actually runs.
- Preserve SPEC-001..SPEC-008 behavior and commands.

The implementation MUST NOT:

- Introduce a second event loop, an async runtime, or a background daemon.
- Poll window state when event/hotkey-driven updates suffice.
- Introduce unrestricted global mutable state.
- Persist window state, layout state, or ordering across executions.
- Modify windows on a display that is not being re-tiled.
- Use private macOS APIs.

## 3. Execution Mode

### FR-001: `run` Command Extension

Extend the SPEC-008 `run` command:

```text
cargo run -- run            # foreground manager + automatic tiling (default)
cargo run -- run --no-auto  # foreground manager exactly as SPEC-008
```

`cargo run -- run` MUST:

- Verify Accessibility authorization (unchanged from SPEC-008).
- Prepare the process as an accessory application, create the overlay, and show
  it once at startup (reusing SPEC-007/SPEC-008).
- Register the global binding set from the shared binding table (SPEC-008).
- Initialize the SPEC-006 observers and registry using the approved startup
  order (FR-007 of SPEC-006), then perform the **initial tiling** (FR-011).
- Enter the SPEC-006/SPEC-008 bounded foreground loop.
- On each iteration: pump AppKit events, drain hotkey actions, drain observer
  events into the `EventProcessor`, feed effective transitions to the tiling
  controller, and re-tile the displays the controller reports as ready.
- Remain foreground until Ctrl+C and shut down gracefully.

`cargo run -- run --no-auto` MUST behave exactly like the current SPEC-008 `run`:
no stack, no automatic layout, and `MoveWindow` swapping frames (SPEC-008
FR-004).

All other commands (`overlay`, `apply`, `move`, `focus`, `watch`, ...) MUST keep
working unchanged.

## 4. Persistent Tiling Order

Automatic tiling requires a stable ordering. Deriving order from window metadata
on every event (as `build_plan` does today via `compare_windows`) would reshuffle
tiles whenever a title changes. SPEC-009 therefore introduces a runtime,
per-display tiling order.

### FR-002: Tiling Order Model

Add a platform-independent model in `app` (for example `app::tiling_order`):

```text
TilingOrder {
  per_display: HashMap<DisplayId, Vec<WindowKey>>,
}
```

- Order entries are `WindowKey` values (the SPEC-006 runtime identity:
  `ProcessId` + `RuntimeWindowId`).
- The order is runtime-only and MUST NOT be persisted across executions.
- The model MUST be pure (no native types) and fully unit-testable.

### FR-003: Order Maintenance

The controller MUST keep the order consistent with observed events:

- **Seed:** on startup, each active display's order is the eligible windows on
  that display sorted by `compare_windows` (deterministic, reproducing the
  SPEC-005 ordering once).
- **Create:** a window first seen on display `D` is appended to `order[D]` if
  absent.
- **Destroy:** a destroyed window is removed from every order.
- **Display change:** a window whose assigned display changes moves from the old
  order to the end of the new order.
- **Idempotence:** re-observing a known window never duplicates or reorders it.

### FR-004: Stability Requirement

Once seeded, the order MUST NOT change because of:

- Window title changes.
- Geometry changes (move/resize by the user).
- Focus changes.

New windows append; removed windows leave; `MoveWindow` (FR-014) is the only
action that reorders.

## 5. Trigger Policy

### FR-005: Dirty Events

The controller maintains a set of dirty displays. An observed transition marks
the display associated with the affected window as dirty when it is one of:

- `WindowCreated` (display from the new bounds).
- `WindowDestroyed` (display from the controller's last-known assignment).
- `WindowMinimized` / `WindowRestored`.
- `WindowMoved` / `WindowResized` **when not self-generated** (FR-006).
- `ApplicationLaunched` / `ApplicationTerminated` (each affected window's
  last-known display; if unknown, mark every active display).

### FR-006: Self-Generated Event Suppression

Programmatic re-tiling emits the same `WindowMoved`/`WindowResized`
notifications as user actions. The controller MUST distinguish them:

- After applying a plan for display `D`, the controller records, per window, the
  geometry macOS **actually applied** (the `Applied`/`Constrained` read-back
  from the SPEC-005 `ExecutionReport`, or the unchanged target for
  `AlreadyCorrect`).
- The controller ignores (does not mark dirty) any geometry event whose bounds
  match the recorded expected geometry for that window within
  `GEOMETRY_TOLERANCE`.
- A geometry event that differs from the expected geometry is treated as a user
  change and marks the display dirty.
- Recording the **actual** (possibly constrained) geometry, not the requested
  target, is what guarantees convergence for windows macOS clamps.
- Expected entries are removed when the window is destroyed or leaves the
  display.

### FR-007: Non-Dirty Events

`WindowFocused`, `ApplicationActivated`, and `ApplicationDeactivated` MUST NOT
mark a display dirty; they do not change the tiling.

## 6. Re-tiling

### FR-008: Re-tile Decision and Debounce

- The controller exposes a query returning the displays ready to re-tile
  (dirty and past the settle interval).
- A single settle interval (`RETILE_SETTLE_INTERVAL`, default **150 ms**)
  debounces bursts. It is driven by the bounded run-loop step and a monotonic
  clock; no timers or threads are introduced.
- At most one re-tile per display per settle interval.
- The SPEC-006 `EventProcessor` coalescing (geometry) also applies, so retiles
  observe settled geometry.

### FR-009: Plan and Apply

For each ready display `D`, the platform-aware loop MUST:

1. Discover readable, controllable windows via the native layer (FR-016).
2. Build the plan with the SPEC-005 planner, ordering eligible windows by
   `order[D]` (FR-002), using the per-display `LayoutConfig` (FR-012).
3. Execute the plan with the SPEC-005 executor in `ExecutionMode::Apply`.
4. Record the resulting expected geometry (FR-006) and clear `D` from the dirty
   set.
5. Report failures per window without aborting other displays.

Planning MUST NOT mutate windows; only the executor's `set_bounds` does.

### FR-010: Convergence and Idempotence

- Applying a layout over unchanged state MUST be a no-op: the executor already
  skips windows within `GEOMETRY_TOLERANCE` (SPEC-005 FR-013), and self-generated
  events are suppressed (FR-006).
- A constrained window MUST reach a fixed point: its actual geometry is recorded
  as expected, so it does not re-trigger retiling.
- The system MUST NOT enter a re-tile loop with no external change.

### FR-011: Initial Tiling

After the SPEC-006 startup sequence (observers registered, registry seeded,
reconciliation performed) and once the order is seeded (FR-003), the manager
MUST tile every active display once. This is the only re-tile that does not
originate from a dirty event.

## 7. Layout State

### FR-012: Per-Display Layout Configuration

- Each active display has a `LayoutConfig` (SPEC-004), defaulting to
  `LayoutConfig::default()` (`MasterStack`).
- The state is keyed by `DisplayId`; a display that disconnects may drop its
  state.
- Gaps, ratio, and master count are not user-configurable by shortcuts in this
  specification.

### FR-013: `SetLayout` Persistence

`SetLayout(layout)` (SPEC-008 FR-005) is amended:

- It sets the `LayoutConfig.layout` for the display containing the focused window
  and re-tiles **that display** immediately.
- It persists for the lifetime of the `run` process, affecting later automatic
  re-tiles.
- If there is no focused window, it is a no-op with a diagnostic (SPEC-008
  FR-012).

## 8. Focus and Move under Automatic Tiling

### FR-014: `Focus` Uses the Tiling Order

`Focus(direction)` MUST select the directional neighbour using the same ordered
eligible window set as the layout, where the order is now `order[D]` (FR-002)
rather than a fresh `compare_windows` sort. Focus only moves focus; it never
changes tiling.

### FR-015: `MoveWindow` Reorders Then Re-tiles

When automatic tiling is active, `MoveWindow(direction)` is amended:

- It swaps the focused window's position with its directional neighbour in
  `order[D]`.
- It then re-tiles display `D`.
- Because tiling is computed from the order, the swap becomes the new layout.

This supersedes the SPEC-008 FR-004 frame-swap under automatic tiling. Under
`run --no-auto`, the SPEC-008 frame-swap behavior is preserved.

## 9. Platform Interface and Identity

### FR-016: Stable Window Identity

Re-tiling must match the SPEC-006 registry identity:

- The native layer MUST expose discovered controllable windows together with a
  stable `WindowKey`, resolved by `CFEqual` against the canonical accessibility
  elements interned by SPEC-006 (not by title, geometry, or address).
- `LayoutWindow` (or an equivalent) MUST carry this key, and the `WindowRef`
  trait MUST expose `fn key(&self) -> WindowKey`.
- A discovered window that is not yet interned MUST be interned with the same
  rule used by SPEC-006 so identities match; if it cannot be resolved, it is
  excluded from automatic tiling rather than given a fabricated identity.

### FR-017: Discovery Timing

Window discovery MUST occur only when a re-tile or a focus/move action executes.
The event stream updates the registry incrementally; automatic tiling must not
enumerate windows on every notification.

## 10. Diagnostics

### FR-018: Reporting

The manager MUST report, without aborting the loop:

- Number of re-tiles performed and their display identifiers.
- Per re-tile: applied / constrained / failed / skipped counts (reusing the
  SPEC-005 `ExecutionReport`).
- Attempts to re-tile a display with no eligible windows.
- Suppressed self-generated events (aggregate counter).
- Identity-resolution failures.
- Permission loss.

Diagnostics MUST distinguish expected platform limitations from defects.

## 11. Non-Functional Requirements

### NFR-001: Performance

- No polling beyond the bounded run-loop step inherited from SPEC-006/007.
- Discovery and layout computation happen only when a re-tile actually runs.
- Work is proportional to the number of changed displays and their windows.
- No new threads, timers, or async runtimes.

### NFR-002: Thread Safety

- AppKit/Carbon/AX calls occur on the main thread only.
- The controller and registry are plain main-thread state; no `unsafe`
  `Send`/`Sync` for native handles.
- Observer callbacks only enqueue normalized events (SPEC-006).

### NFR-003: Maintainability

- The controller and tiling order are platform-independent and unit-tested.
- Reuse existing domain models; do not duplicate `LayoutConfig`, `WindowKey`,
  `ExecutionReport`, or display assignment.
- Follow `AGENTS.md`.

## 12. Error Handling

### FR-019: Failure Modes

Handle and report distinctly, without crashing the loop:

- Accessibility permission denied (`run` exits with the documented message).
- No active display / no eligible windows (no-op with a diagnostic).
- A window disappears or rejects a change (`ControlError`); only that window
  fails and the display's other windows continue.
- Identity resolution failure (window excluded from tiling).
- Plan errors (unknown display) (no-op with a diagnostic).
- Overlay/hotkey failures (already handled by SPEC-007/008; they MUST NOT
  prevent automatic tiling).
- Observer failure (already isolated by SPEC-006).

## 13. Testing Strategy

### Unit Tests (no macOS permission required)

- Tiling order: seeding, append on create, removal on destroy, display change,
  no reordering on title/geometry/focus changes, no duplicates.
- Dirty policy: each trigger event; focus/activation do not mark dirty.
- Self-generated suppression: matching expected geometry is ignored; differing
  geometry marks dirty; constrained expected geometry converges.
- Debounce: bursts collapse to one re-tile; at most one per interval.
- Retile decision: empty/one-window displays; multiple dirty displays; a
  display with no eligible windows.
- Layout state: default per display; `SetLayout` updates only the focused
  display; persistence across re-tiles.
- Move/focus: swap reorders the stack; focus uses stack order.
- Controller idempotence: re-tiling unchanged state produces no mutations.

### Integration Tests (test doubles)

- Planning never mutates windows.
- A create event leads to one re-tile of the affected display only.
- A self-generated move does not cause a second re-tile.
- One failing window does not stop other windows.
- Displays are independent: a change on one does not re-tile another.
- `run --no-auto` performs no automatic layout (regression).

### Manual macOS Tests

1. `cargo run -- run`; verify the overlay appears and all displays are tiled at
   startup.
2. Open a new window on a display; verify it joins the layout automatically.
3. Close a window; verify the remaining windows reclaim the space.
4. Drag the focused window elsewhere; verify it returns to its tiled slot.
5. Minimize/restore a window; verify the layout updates and recovers.
6. Press `⌘⌥C/R/S/M`; verify the focused display switches and stays switched.
7. Press `⌘⌥⇧←/→`; verify the focused window reorders and the display re-tiles.
8. With multiple displays, verify changes on one do not disturb the other.
9. Verify `cargo run -- run --no-auto` performs no automatic layout.
10. Ctrl+C exits with no leftover window or hotkey.

Unavailable checks (for example multi-display hardware, Spaces) MUST be
documented rather than marked as passed.

## 14. Acceptance Criteria

### AC-001: Default Automatic Tiling
`cargo run -- run` tiles all active displays at startup and keeps them tiled.

### AC-002: Window Lifecycle
Opening and closing a window automatically re-tiles the affected display.

### AC-003: User Change Reversion
Manually moving a tiled window causes it to be re-tiled.

### AC-004: No Feedback Loop
Programmatic re-tiling does not trigger further re-tiles; unchanged state
produces no mutations.

### AC-005: Layout Persistence
`⌘⌥M/S/C/R` set the focused display's layout and persist across later re-tiles.

### AC-006: Focus and Move
`⌘⌥←/→/↑/↓` move focus within the tiling order; `⌘⌥⇧←/→` reorder and re-tile.

### AC-007: Display Isolation
A change on one display does not re-tile another display.

### AC-008: Stability
Title, geometry, and focus changes never reshuffle the tiling order on their
own.

### AC-009: Opt-Out
`cargo run -- run --no-auto` preserves the exact SPEC-008 behavior.

### AC-010: Regression Safety
`overlay`, `apply`, `move`, `focus`, and `watch` keep working unchanged.

### AC-011: Graceful Shutdown
Ctrl+C releases window, hotkeys, observers, and handlers with no leftover
process or window.

### AC-012: Automated Validation
The following pass:

- `cargo fmt --all -- --check`
- `cargo check --all-targets`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test --all-targets`

### AC-013: Native Verification
Automatic tiling, re-tile on window lifecycle, layout persistence, and reorder
are manually verified on macOS; unavailable checks are documented rather than
assumed.

## 15. Out of Scope

- Floating windows and per-application ignore rules (candidate for SPEC-010).
- Persistent layout/order across `run` invocations.
- Cross-display window movement by shortcut.
- Config files, gaps/ratio hotkeys, animations.
- Multi-Space management and native Spaces manipulation.
- A background daemon or launch agent.
- BSP layout.
- Automatic focus changes beyond the explicit `Focus` action.
- Private macOS APIs.

## 16. Definition of Done

SPEC-009 is complete when:

- `run` automatically keeps all active displays tiled by default.
- Automatic re-tiling reacts to window and application lifecycle changes.
- Self-generated changes never cause feedback loops.
- A persistent per-display tiling order stabilizes layouts.
- `SetLayout`, `Focus`, and `MoveWindow` respect automatic tiling.
- `run --no-auto` preserves SPEC-008 behavior.
- Automated tests pass and SPEC-001..SPEC-008 behavior is intact.
- Native behavior is manually verified or explicitly documented as unverified.

Any unavailable or unverified acceptance criterion must be explicitly
documented.

## 17. Open Decisions for Approval

1. **Settle interval.** Proposed default `RETILE_SETTLE_INTERVAL = 150 ms`.
2. **`MoveWindow` direction binding.** SPEC-008 binds `⌘⌥⇧←/→` only; automatic
   tiling reorders horizontally for these. Vertical reorder (`⌘⌥⇧↑/↓`) is not
   added in this specification.
3. **Multi-display identity.** The order/state is per `DisplayId`; a display
   disconnect/reconnect resets its stack by re-seeding.
4. **Focused-display default for `SetLayout`.** `SetLayout` continues to target
   the display containing the focused window (SPEC-008 FR-005).
