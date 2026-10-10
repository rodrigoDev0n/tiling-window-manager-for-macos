# SPEC-002: Native macOS Window Control

**Status:** Draft
**Priority:** High
**Dependencies:** SPEC-001

## 1. Objective

Enable deterministic movement and resizing of macOS application windows using
public Apple APIs, building directly on the Accessibility (AX) integration
introduced in SPEC-001.

The feature exposes a CLI that selects an individual window in a single process
invocation and applies a target geometry to it, then reports the geometry that
macOS actually applied.

## 2. Scope

In scope:

- Selecting an individual window within one CLI invocation.
- Setting absolute geometry (`x`, `y`, `width`, `height`).
- Basic geometry shortcuts: `left`, `right`, `maximize`, `center`.
- Reading back the applied geometry after a change.
- Detecting and reporting windows that do not support programmatic control.

Out of scope:

- Automatic tiling and layout algorithms (BSP, monocle, floating).
- Keyboard shortcuts and background services.
- Workspace / Space management.
- Persistent window identifiers across separate process executions.
- Private or undocumented macOS APIs.

## 3. Functional Requirements

### FR-001: Window Selection

The system MUST select a single window for the operation, resolved within the
same process invocation that performs it. Supported selectors (mutually
exclusive):

- `--focused` — the focused window of the frontmost application. **Default.**
- `--pid <PID>` — restrict to the given process.
- `--app <NAME>` — restrict to a localized application name.
- `--title <SUBSTRING>` — case-insensitive substring of the window title.
- `--index <N>` — zero-based position within a deterministic ordering.

The system MUST apply a deterministic ordering `(application, title, x, y, pid)`
with a final tie-breaker `(width, height)` so that `--index` is reproducible
within an invocation.

The system MUST behave as follows:

- Exactly one match: operate on it.
- Zero matches: fail with a `WindowNotFound` error.
- More than one match without `--index`: fail with an `AmbiguousSelection` error
  that lists the candidate windows.

The system MUST NOT persist or trust window indices across separate executions.

### FR-002: Geometry Read

The system MUST read `AXPosition` and `AXSize` for the selected window before
any change when required for a shortcut (e.g. `center`).

### FR-003: Absolute Geometry

The system MUST set `AXPosition` and `AXSize` from provided `x`, `y`, `width`,
and `height` values.

- All four values MUST be supplied together for an absolute move.
- Values MUST be finite and dimensions MUST be strictly positive; invalid input
  MUST be rejected with an `InvalidArgument` error.

### FR-004: Geometry Shortcuts

The system MUST support `--to <left|right|maximize|center>`, computed against
the work area (visible frame) of the screen containing the selected window:

- `left`: left half of the work area.
- `right`: right half of the work area.
- `maximize`: the entire work area.
- `center`: current size (clamped to the work area), centered in the work area.

The shortcut computation MUST be a pure, platform-independent function taking
the work area and the current window rect.

### FR-005: Applied Geometry Read-back

After setting geometry, the system MUST re-read `AXPosition` and `AXSize` and
report the **actual applied rectangle**, which may differ from the request due
to macOS clamping or application constraints.

### FR-006: Unsupported Windows

The system MUST verify settability via `AXUIElementIsAttributeSettable` before
writing. When `AXPosition` or `AXSize` is not settable, or when the write
returns an unsupported/illegal-argument/cannot-complete `AXError`, the system
MUST:

- Fail with an `UnsupportedWindow` (or `ControlFailed`) error for that window.
- NOT crash.
- Leave other windows unaffected.

### FR-007: CLI

The system MUST support:

- `cargo run -- windows` — unchanged behavior from SPEC-001.
- `cargo run -- move [SELECTOR] --x <f64> --y <f64> --width <f64> --height <f64>`
- `cargo run -- move [SELECTOR] --to <left|right|maximize|center>`
- `cargo run -- focus [SELECTOR]`

The `move` command MUST print the applied rectangle on success.

### FR-008: Error Reporting

The system MUST report, without crashing:

- Missing Accessibility permission (unchanged from SPEC-001).
- Unknown selectors, missing/contradictory arguments.
- `WindowNotFound`, `AmbiguousSelection` (with candidates), `UnsupportedWindow`,
  `ControlFailed`.

## 4. Non-Functional Requirements

- Written in Rust, using public macOS APIs only.
- No new external dependencies.
- All FFI isolated in `platform/macos`.
- `core` MUST remain free of macOS dependencies.
- No use of `unwrap()`/`expect()`/`panic!()` in production paths.
- No background polling; one-shot commands only.
- No private APIs (`_AXUIElementGetWindow`, undocumented attributes) unless
  explicitly approved.
- Existing SPEC-001 behavior (permissions, discovery, table output) MUST be
  preserved.

## 5. Acceptance Criteria

- AC-001: The project compiles successfully.
- AC-002: `permissions` behavior is unchanged from SPEC-001.
- AC-003: `windows` behavior is unchanged from SPEC-001.
- AC-004: `move` with absolute geometry repositions and/or resizes a supported
  window on macOS.
- AC-005: `move` reports the actual applied rectangle read back from AX.
- AC-006: `--to left|right|maximize|center` produce the expected rectangles in
  pure unit tests.
- AC-007: An ambiguous selector fails with a candidate list.
- AC-008: A selector with zero matches fails with `WindowNotFound`.
- AC-009: A non-settable window fails with a clear error and no crash.
- AC-010: `--index` ordering is deterministic across repeated discovery in the
  same process.
- AC-011: Invalid geometry is rejected with `InvalidArgument`.
- AC-012: All platform-independent unit tests pass.
- AC-013: Formatting and Clippy validation pass.
- AC-014: No private macOS APIs are used.

## 6. Verification

Run:

- `cargo fmt --all -- --check`
- `cargo check --all-targets`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test --all-targets`

Native behavior (AC-004, AC-005, AC-009) MUST also be verified manually on
macOS with Accessibility permission granted.

## 7. Definition of Done

The specification is complete when all acceptance criteria are satisfied and the
implementation has been validated.

If a criterion cannot be verified, it MUST be reported as unverified rather than
marked complete.
