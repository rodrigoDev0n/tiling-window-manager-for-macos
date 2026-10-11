# tiling_window

A native, event-driven tiling window manager for macOS written in Rust.

`tiling_window` discovers application windows through the macOS Accessibility
API, arranges them with a platform-independent layout engine, and keeps every
display tiled automatically as windows open, close, move, or change state — all
without polling, background daemons, or private Apple APIs.

The project is built incrementally under a Spec-Driven Development workflow.
Specifications are the source of truth for behavior; see [`AGENTS.md`](AGENTS.md)
for engineering conventions and [`specs/`](specs/README.md) for the full index.

## Features

- **Native macOS integration** — public Accessibility, Core Graphics, and AppKit
  APIs only. No private frameworks.
- **Automatic tiling** — the foreground manager re-tiles each affected display
  when windows or applications change, with debounced, idempotent, convergent
  updates.
- **Multiple layouts** — master/stack, columns, rows, and monocle.
- **Per-display state** — every active display keeps its own persistent tiling
  order and layout configuration for the lifetime of the process.
- **Global shortcuts** — functional hotkeys plus a translucent shortcut overlay.
- **Event-driven** — a bounded foreground run loop consumes normalized window
  and application events; no threads, timers, or async runtime.
- **Platform-independent core** — layout algorithms and the tiling controller
  contain no macOS types and are fully unit-testable.

## Requirements

- macOS 11.0 or later.
- Rust stable toolchain (edition 2024).
- **Accessibility permission** granted to the running binary or the installed
  `.app` bundle.

## Build

```sh
cargo build --release
```

The release binary is produced at `target/release/tiling_window`.

## Install as a login agent (optional)

`scripts/install-agent.sh` packages the release binary as a signed
`TilingWindow.app` bundle and installs a per-user `LaunchAgent` so it starts at
login:

```sh
./scripts/install-agent.sh
```

The script builds the release binary if needed, code-signs the bundle with a
stable self-signed identity (so the Accessibility grant survives rebuilds), and
registers the agent under `~/Library/LaunchAgents`. Signing material is stored
outside the repository, under `~/Library/Application Support/tiling-window`.

To remove it:

```sh
./scripts/uninstall-agent.sh
```

## Accessibility permission

Window inspection, movement, and observation all require Accessibility access.
The `permissions` command reports the current status and prints instructions:

```sh
cargo run -- permissions
```

Grant it manually under **System Settings → Privacy & Security → Accessibility**
by enabling the entry for the program (or adding it with `+`). After a rebuild
you may need to toggle the entry off and on again.

## Usage

```sh
cargo run -- <command> [options]
```

Run `cargo run -- --help` (or with no arguments) to print the full command and
selector reference.

### Commands

| Command | Description |
| --- | --- |
| `permissions` | Report Accessibility authorization status. |
| `windows` | List accessible windows of running applications. |
| `move` | Move/resize a window to an absolute or predefined geometry. |
| `focus` | Focus (raise) a window. |
| `displays` | List active displays and their geometry. |
| `display-info <id>` | Show detailed geometry for one display. |
| `layout` | Preview a tiling layout without changing windows. |
| `apply <layout>` | Apply a tiling layout to windows on a display. |
| `watch` | Monitor window events until interrupted (Ctrl+C). |
| `overlay` | Show a translucent shortcut overlay (toggle: `⌘⌥-`). |
| `run [--no-auto]` | Foreground window manager: overlay, shortcuts, automatic tiling. |
| `-h`, `--help` | Show help. |

### Window selectors

Selectors default to `--focused`.

| Selector | Description |
| --- | --- |
| `--focused` | The focused window of the frontmost application. |
| `--pid <PID>` | Restrict to a process. |
| `--app <NAME>` | Restrict to an application name. |
| `--title <SUBSTRING>` | Restrict to a title substring. |
| `--index <N>` | Pick the N-th match in a deterministic order. |

### `move` geometry

```sh
cargo run -- move --x <f64> --y <f64> --width <f64> --height <f64>
cargo run -- move --to <left|right|maximize|center>
```

### `layout` and `apply` options

```sh
cargo run -- layout --layout <masterstack|columns|rows|monocle> [--display <id>] \
    [--outer-gap <f64>] [--inner-gap <f64>] [--ratio <f64>] [--master-count <usize>]

cargo run -- apply <masterstack|columns|rows|monocle> [--display <id>] [--dry-run]
```

### Foreground manager

```sh
cargo run -- run          # overlay + shortcuts + automatic tiling (default)
cargo run -- run --no-auto # overlay + shortcuts only (no automatic layout)
```

`run` stays in the foreground until Ctrl+C, pumping AppKit events, draining
hotkey actions and observer events, and re-tiling displays the controller
reports as ready. All other commands keep working independently.

## Keyboard shortcuts

Shortcuts are defined in a single binding table (`src/core/shortcut.rs`) that
drives both the overlay reference and the registered global hotkeys, so the
displayed list and the executed actions never diverge.

| Shortcut | Action |
| --- | --- |
| `⌘⌥-` | Show or hide the overlay |
| `⌘⌥S` | Master/Stack layout |
| `⌘⌥C` | Columns layout |
| `⌘⌥R` | Rows layout |
| `⌘⌥M` | Monocle layout |
| `⌘⌥←/→/↑/↓` | Move focus |
| `⌘⌥⇧←/→` | Reorder the focused window and re-tile |

## Layouts

| Layout | Behavior |
| --- | --- |
| `masterstack` | A left-hand master column plus a right-hand stack column. |
| `columns` | Equal-width vertical columns. |
| `rows` | Equal-height horizontal rows. |
| `monocle` | All windows share the full usable area. |

The default `LayoutConfig` is `masterstack` with `outer_gap = 8.0`,
`inner_gap = 8.0`, `master_ratio = 0.55`, and `master_count = 1`.

## How it works

1. **Discovery** — accessible, controllable windows are discovered through the
   Accessibility API together with a stable runtime identity (`WindowKey` =
   process id + runtime window id).
2. **Event system** — observers normalize application and window lifecycle
   changes into platform-independent domain events and maintain a runtime
   registry. Geometry events are coalesced.
3. **Tiling controller** — a per-display tiling order is seeded on startup and
   kept stable: new windows append, destroyed windows are removed, and only
   explicit reordering changes the order. Title, geometry, and focus changes
   never reshuffle it.
4. **Debounced re-tiling** — dirty displays are re-tiled after a settle interval
   (default 150 ms). Programmatic geometry changes are matched against the
   geometry macOS actually applied and suppressed, which prevents feedback loops
   and guarantees convergence for constrained windows.
5. **Layout engine** — pure, platform-independent algorithms turn a work area
   plus an ordered window collection into deterministic target rectangles.
6. **Execution** — target rectangles are applied through the Accessibility API;
   windows already within the geometry tolerance are skipped.

## Architecture

```
src/
├── main.rs                 # Entry point, command dispatch, exit codes
├── app/                    # Foreground manager, tiling controller, overlay
│   ├── tiling.rs           # Automatic tiling controller / dirty policy
│   ├── event_processor.rs  # Event normalization and coalescing
│   ├── registry.rs         # Runtime window registry
│   ├── layout_planner.rs   # Builds layout plans from state
│   ├── layout_executor.rs  # Applies plans, records expected geometry
│   ├── overlay.rs          # Overlay content/state
│   └── ...
├── platform/macos/         # All AppKit / Carbon / Accessibility code
│   ├── accessibility.rs    # Permission checks, AX helpers
│   ├── applications.rs     # NSRunningApplication helpers
│   ├── displays.rs         # Display discovery and geometry
│   ├── windows.rs          # Window discovery
│   ├── control.rs          # Window move/resize
│   ├── observers.rs        # Native event observers
│   ├── hotkey.rs           # Global hotkey registration
│   ├── overlay.rs          # Overlay NSWindow
│   └── core_graphics.rs    # Core Graphics bindings
├── core/                   # Platform-independent domain models
│   ├── action.rs           # Actions and directions
│   ├── coordinates.rs      # Coordinate systems
│   ├── display*.rs         # Display model and assignment
│   ├── event.rs            # Domain events and WindowKey
│   ├── geometry.rs         # Rect / Size primitives
│   ├── layout*.rs          # Layout config, plan, execution report
│   ├── navigation.rs       # Directional neighbour search
│   ├── selector.rs         # Window selection and ordering
│   ├── shortcut.rs         # Binding table (overlay + hotkeys)
│   └── window.rs           # Window identifiers and metadata
├── layouts/                # Pure layout algorithms
│   ├── master_stack.rs
│   ├── columns.rs
│   ├── rows.rs
│   └── monocle.rs
├── commands/               # CLI parsing and dispatch
├── error.rs                # Application error types
└── ...
```

**Architectural rules**

- `core` never depends on macOS APIs; layout algorithms operate on
  platform-independent geometry.
- All AppKit, Carbon, and Accessibility code stays inside `platform/macos`,
  behind safe Rust abstractions.
- Commands decide *what* to do; the platform layer performs the native work.
- AppKit/Carbon/AX calls happen on the main thread only; no manual `Send`/`Sync`
  for native handles.

## Development

Before considering a change complete, run:

```sh
cargo fmt --all -- --check
cargo check --all-targets
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets
```

For release validation:

```sh
cargo build --release
```

All platform-independent logic (geometry, layouts, tiling order and policy,
selectors, configuration validation) is covered by unit tests that do not
require Accessibility permissions.

## Specifications

Feature behavior is defined by specification. Read the relevant spec in full
before implementing.

| Spec | Title | Status |
| --- | --- | --- |
| [SPEC-001](specs/001-windows-discovery.md) | Window discovery | Draft |
| [SPEC-002](specs/002-native-macos-window-control.md) | Native macOS window control | Draft |
| [SPEC-003](specs/003-native-macos-display-management.md) | Native macOS display management | Draft |
| [SPEC-004](specs/004-platform-independent-layout-engine.md) | Platform-independent layout engine | Draft |
| [SPEC-005](specs/005-native-layout.md) | Native layout application | Draft |
| [SPEC-006](specs/006-windows-event-system.md) | Window event system | Approved |
| [SPEC-007](specs/007-shortcut-overlay.md) | Shortcut overlay window | Approved |
| [SPEC-008](specs/008-shortcut-actions.md) | Shortcut actions | Implemented |
| [SPEC-009](specs/009-automatic-tiling.md) | Automatic tiling | Implemented |

See [`specs/README.md`](specs/README.md) for numbering and supersession notes.

## Limitations

- macOS does not expose unrestricted window management through public APIs.
  Some applications do not support programmatic resizing, and system windows may
  not be manageable.
- Fullscreen windows require special handling and are not tiled.
- Multi-Space behavior is limited by public macOS APIs; Spaces are not managed.
- Accessibility permission must be granted explicitly and can be lost when the
  binary is rebuilt (re-grant, or use the signed `.app` bundle).
- Tiling order and layout state are runtime-only and are not persisted across
  `run` invocations.

## Roadmap

- SPEC-010 — Floating windows and per-application rules (not defined yet).
- BSP layout, cross-display window movement, config files, and animation
  improvements (see SPEC-009 §15, Out of Scope).
