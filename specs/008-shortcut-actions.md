# SPEC-008: Shortcut Actions (Foreground Window Manager)

**Status:** Implemented  
**Priority:** High  
**Dependencies:** SPEC-002 (window control), SPEC-004 (layout engine), SPEC-005 (layout application), SPEC-007 (overlay/hotkey foundations)  
**Supersedes:** the SPEC-007 restriction that shortcuts are informational only.  
**Replaces the SPEC-007 numbering note:** Automatic Tiling is renumbered from SPEC-008 to **SPEC-009**.

## 1. Objective

Make the shortcuts shown by the overlay *functional*. A new foreground command,
`cargo run -- run`, holds the window manager in the foreground, registers the
global shortcuts, and executes them while another application is frontmost.

The first version MUST support exactly three families of actions:

1. **Switch layout** — apply Monocle / Master-Stack / Columns / Rows to the
   display that contains the focused window.
2. **Move focus** — focus the neighbouring tiled window in the pressed
   direction.
3. **Move window** — swap the focused window's frame with its neighbour in the
   pressed direction.

Toggling the overlay (`⌘⌥-`) remains functional within `run`.

## 2. Architectural Principles

The implementation MUST:

- Use public macOS APIs exclusively.
- Keep all AppKit, Carbon and Accessibility code inside `platform/macos`.
- Model actions, directions and key bindings in the platform-independent
  `core` layer.
- Define the binding table in **one** place and derive both the registered
  hotkeys and the overlay reference from it (satisfying SPEC-007 FR-009).
- Reuse the existing layout planner/executor (SPEC-005) for `SetLayout`.
- Reuse the bounded run loop and hotkey mechanism (SPEC-006/SPEC-007).
- Keep `core` free of AppKit/Carbon/AX types.
- Preserve SPEC-001..SPEC-007 behavior and commands.

The implementation MUST NOT:

- Introduce a second event loop or an async runtime.
- Poll for window state when event/hotkey-driven updates suffice.
- Introduce unrestricted global mutable state.
- Apply a layout more than once per action.
- Move or resize a window that is already at its target geometry.

## 3. Execution Mode

### FR-001: `run` Command

Add the command:

```text
cargo run -- run
```

It MUST:

- Verify Accessibility authorization.
- Prepare the process as an accessory application and create the overlay window
  (reusing SPEC-007), shown once at startup.
- Register the global shortcut set from the shared binding table.
- Enter the SPEC-006/SPEC-007 bounded foreground loop.
- On each iteration: pump AppKit events, drain pending hotkey actions, and
  execute each action in order.
- Remain foreground until Ctrl+C and shut down gracefully (SPEC-007 FR-010).

The existing `overlay` command MUST keep working unchanged (regression).

## 4. Action Model

### FR-002: Action Types

Define a platform-independent action:

```text
Action =
  | ToggleOverlay
  | SetLayout(LayoutType)
  | Focus(Direction)
  | MoveWindow(Direction)

Direction = Left | Right | Up | Down
```

`LayoutType` is the existing SPEC-004 enum (`Monocle`, `MasterStack`,
`Columns`, `Rows`). `Action` MUST be `Copy` and contain no native types.

### FR-003: Focus Semantics

`Focus(direction)`:

- Operates on the windows currently eligible on the focused window's display.
- Orders them with the same deterministic comparator used by the layout planner
  (`core::selector::compare_windows`), so results are reproducible.
- Selects the directional neighbour of the focused window's frame and focuses
  it through the Accessibility API (`AXRaise` + focused-window attribute).

### FR-004: Move-Window Semantics

`MoveWindow(direction)`:

- Uses the same eligible, ordered window set and the same neighbour selection
  as `Focus`.
- Swaps the frame of the focused window with that of its neighbour, applying
  each frame once through `control::set_bounds`.
- Leaves focus unchanged.

### FR-005: Layout Semantics

`SetLayout(layout)`:

- Applies `LayoutConfig { layout, ..default }` to the display containing the
  focused window, using the SPEC-005 planner and executor.
- Does not persist layout state; each invocation is independent and
  deterministic (a later `MoveWindow` swaps geometry regardless of layout).

## 5. Neighbour Selection

### FR-006: Directional Neighbour

Add a pure function in `core`:

```text
directional_neighbor(rects: &[Rect], current: usize, direction) -> Option<usize>
```

It MUST:

- Consider only candidates that lie strictly in the requested direction of the
  reference rectangle (based on centres along the primary axis).
- Choose the candidate minimising the primary-axis distance, breaking ties by
  the smallest perpendicular offset, then by the smallest index.
- Return `None` when no candidate lies in that direction.

This function is the single source of truth for both focus and move-window.

## 6. Binding Table

### FR-007: Single Source of Truth

Refactor `core::shortcut` so a single table defines, for each shortcut:

- The logical key and modifiers (platform-independent).
- The `Action`.
- A human-readable description.

```text
Key = Char(char) | Left | Right | Up | Down | Return
Modifiers { command: bool, option: bool, shift: bool }
Binding { key: Key, modifiers: Modifiers, action: Action }
```

`default_shortcuts()` (the overlay reference) MUST be derived from
`default_bindings()`, so the overlay and the hotkeys never diverge.

### FR-008: Default Bindings

| Keys | Action |
| --- | --- |
| `⌘⌥-` | `ToggleOverlay` |
| `⌘⌥M` | `SetLayout(Monocle)` |
| `⌘⌥S` | `SetLayout(MasterStack)` |
| `⌘⌥C` | `SetLayout(Columns)` |
| `⌘⌥R` | `SetLayout(Rows)` |
| `⌘⌥←/→/↑/↓` | `Focus(Left/Right/Up/Down)` |
| `⌘⌥⇧←/→` | `MoveWindow(Left/Right)` |

BSP, toggle-floating, swap-with-master and quit are **removed** from the
default set (out of scope; see §12) so the overlay lists only functional
shortcuts.

## 7. Native Hotkey Layer

### FR-009: Multiple Hotkeys

Generalize `platform/macos/hotkey` from a single toggle to a set:

- `HotkeySet::register(bindings: &[Binding]) -> Result<HotkeySet, HotkeyError>`
  installs **one** Carbon event handler and registers one
  `RegisterEventHotKey` per binding, assigning a stable id per index.
- The Carbon callback MUST NOT execute actions. It only pushes the pressed
  hotkey id into a bounded queue (`Mutex<Vec<u32>>` or equivalent).
- `HotkeySet::take_pending() -> Vec<Action>` drains the queue and maps ids back
  to actions through the stored binding table.
- `Drop` unregisters every hotkey and removes the handler exactly once.
- A mapping from logical `Key`/modifiers to Carbon virtual key codes and
  modifier bits lives in `platform/macos`.
- Registration failure for any binding MUST NOT abort the process; the failure
  is reported and the successfully registered bindings stay active.

The overlay toggle binding replaces the SPEC-007 `register_toggle` path; the
`overlay` command registers the single toggle binding.

## 8. Native Window Operations

### FR-010: Python—focused index

Add `control::focused_index(windows: &[LayoutWindow]) -> Option<usize>` that
compares the Accessibility element of the focused window with each candidate
using `CFEqual`, so focus/move can locate the focused window within the ordered
set without relying on title or geometry.

### FR-011: Focused-Display Filtering

Focus and move actions operate on windows whose centre lies inside the usable
frame of the display containing the focused window (reusing
`display_assignment`). Windows on other displays are ignored in this version.

## 9. Error Handling

### FR-012: Failure Modes

Handle and report distinctly, without crashing the loop:

- Accessibility permission denied (`run` exits with the documented message).
- No focused window (`Focus`/`MoveWindow`/`SetLayout` are no-ops with a
  diagnostic).
- No neighbour in the requested direction (no-op).
- A window disappears or rejects a change (`control::ControlError`); the action
  fails for that window only and the loop continues.
- Hotkey registration failure (reported; loop keeps running).
- Overlay creation failure (reported; the manager keeps running without the
  overlay).

## 10. Performance

### NFR-001

- No polling beyond the bounded run-loop step inherited from SPEC-006/007.
- Discovery happens only when an action is executed.
- `SetLayout` relies on the executor to skip windows already at target.

### NFR-002: Thread Safety

- AppKit/Carbon/AX calls occur on the main thread only.
- The audit callback and the run loop communicate only through the atomic/
  mutex-protected queue.
- No `unsafe` `Send`/`Sync` for native handles.

## 11. Testing Strategy

### Unit Tests (no macOS permission required)

- `directional_neighbor`: each direction; ties; no candidate; empty input.
- Binding table: no duplicate key combinations; every overlay entry maps to a
  binding; description formatting.
- `default_shortcuts()` derivation matches `default_bindings()`.
- Action description/keys formatting.

### Manual macOS Tests

1. `cargo run -- run`; verify the overlay appears and the app is not in the Dock.
2. With another app frontmost, press `⌘⌥C`, `⌘⌥R`, `⌘⌥S`, `⌘⌥M`; verify the
   focused display re-tiles.
3. Press `⌘⌥←/→/↑/↓`; verify focus moves to the expected neighbour.
4. Press `⌘⌥⇧←/→`; verify the focused window swaps frames with its neighbour.
5. Verify `⌘⌥-` toggles the overlay.
6. Ctrl+C exits with no leftover window or hotkey.

## 12. Out of Scope

- BSP layout, floating windows, swap-with-master, quit hotkey.
- Persisting layout/order across `run` invocations.
- Per-application or per-space configuration.
- Cross-display focus movement.
- Animations, gaps hotkeys, ratio adjustment, config files.
- A background daemon or launch agent.

## 13. Acceptance Criteria

### AC-001
`cargo run -- run` starts a foreground process with the overlay and the full
binding set registered.

### AC-002
`⌘⌥M/S/C/R` apply the corresponding layout to the focused display.

### AC-003
`⌘⌥←/→/↑/↓` move focus to the directional neighbour.

### AC-004
`⌘⌥⇧←/→` swap the focused window's frame with its directional neighbour.

### AC-005
`⌘⌥-` toggles the overlay without activating the application.

### AC-006
The overlay reference is derived from the binding table and lists only
functional shortcuts.

### AC-007
Ctrl+C releases window, hotkeys and handler with no leftover process/window.

### AC-008
The existing `overlay`, `apply`, `move`, `focus` and `watch` commands keep
working (regression).

### AC-009: Automated Validation

The following pass:

- `cargo fmt --all -- --check`
- `cargo check --all-targets`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test --all-targets`

### AC-010: Native Verification

Shortcut actions and overlay toggle are manually verified on macOS; unavailable
checks are documented rather than assumed.

## 14. Definition of Done

SPEC-008 is complete when:

- `run` hosts the overlay and all functional shortcuts in the foreground.
- Layout switching, focus movement and window moving work globally.
- The binding table is the single source of truth for overlay and hotkeys.
- Shutdown releases all native resources.
- Automated tests pass and SPEC-001..SPEC-007 behavior is intact.
- Native behavior is manually verified.

Any unavailable or unverified criterion must be explicitly documented.
