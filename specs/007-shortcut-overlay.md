# SPEC-007: Shortcut Overlay Window

**Status:** Approved  
**Priority:** Medium  
**Dependencies:** SPEC-001 (accessibility permission plumbing), SPEC-006 (run-loop/observation foundations)  
**Integration:** SPEC-008 — Automatic Tiling (renumbered from the former SPEC-007)  
**Next:** SPEC-008 — Automatic Tiling

> **Note on numbering.** SPEC-006 previously reserved "SPEC-007" for Automatic
> Tiling. This specification takes the SPEC-007 slot and Automatic Tiling is
> renumbered to SPEC-008.

## 1. Objective

Provide a small, translucent, always-on-top overlay window that displays the
keyboard shortcut reference for the window manager.

The overlay appears when the application starts and can be shown or hidden at
any time with a global hotkey. It is purely informational: this specification
does **not** wire the listed shortcuts to actions.

## 2. Architectural Principles

The implementation MUST:

- Use public macOS APIs exclusively.
- Keep all AppKit and window construction inside `platform/macos`.
- Define the shortcut list and overlay model in the platform-independent
  `core`/`app` layers.
- Reuse the bounded run loop established by SPEC-006; do not introduce a second
  event loop or an async runtime.
- Keep the overlay responsive and non-blocking.
- Release native resources on shutdown.
- Preserve existing SPEC-001..SPEC-006 behavior.

The implementation MUST NOT:

- Apply layouts, move, resize, or focus windows.
- Execute any shortcut action beyond toggling the overlay itself.
- Introduce a background daemon that outlives the foreground process.
- Use private macOS APIs.
- Introduce unrestricted global mutable state.

## 3. Execution Mode

### FR-001: Overlay Command

Add a new command:

`cargo run -- overlay`

The command MUST:

- Verify Accessibility authorization (required for the global hotkey).
- Create the overlay window.
- Register the global toggle hotkey.
- Enter a run loop (reusing SPEC-006's bounded run-loop stepping).
- Remain in the foreground until interrupted with Ctrl+C.
- Shut down gracefully on Ctrl+C, releasing the window and hotkey.

The `overlay` command runs in the foreground like `watch`; it does **not**
detach or daemonize.

**Implementation note (run loop).** Dispatching Carbon global hot keys requires
the AppKit event loop (`nextEventMatchingMask:untilDate:inMode:dequeue:`),
which services the Carbon Event Manager; a bare Core Foundation run-loop step
does not. The overlay therefore pumps AppKit events in bounded increments. The
loop remains bounded and non-blocking, matching NFR-001 and the FR-006 intent
that the hotkey "integrates with the main run loop".

## 4. Overlay Window

### FR-002: Window Construction

The overlay MUST be a native macOS window with the following properties:

- Borderless, non-activating panel (`NSPanel`), so it does not steal focus from
  the active application.
- Floating window level, above normal application windows.
- Non-opaque background with translucency (alpha), rendered with a native
  blur/vibrancy material (`NSVisualEffectView`, HUD material) and rounded
  corners.
- A subtle shadow.
- Fixed, compact size that fits the shortcut list without scrolling.

The overlay MUST NOT appear in the Dock or the application switcher.

### FR-003: Content

The overlay MUST render the shortcut list as attributed text:

- One row per shortcut: a key-combination label and its description.
- Monospaced glyphs for key combinations where practical.
- Legible contrast against the translucent background.

### FR-004: Placement

The overlay MUST:

- Be positioned within the visible frame of the display containing the current
  frontmost application (falling back to the main display).
- Be centered horizontally and positioned near the top or center of the screen.
- Never extend beyond the visible frame (respecting the menu bar and Dock).

## 5. Visibility and Global Hotkey

### FR-005: Startup Visibility

On startup the overlay MUST be shown once, without requiring the hotkey.

### FR-006: Toggle Hotkey

A single global hotkey MUST toggle the overlay between visible and hidden.

- The default combination is `⌘⌥/` (Command+Option+Slash).
- The hotkey MUST work while another application is frontmost.
- The toggle MUST NOT activate this application or move focus.

**Open decision (resolved).** The global hotkey mechanism is **option 1**:
Carbon `RegisterEventHotKey` + `InstallEventHandler`. It is a documented
legacy public API (not private), integrates with the main run loop, requires no
event tap, and works independently of the frontmost application. It is provided
by the `Carbon` framework, which must be linked explicitly.

The specification recommends option 1 for a single, reliable, focus-independent
hotkey and MUST document the final choice and its permission requirements.

### FR-007: Hotkey Lifecycle

The system MUST:

- Register the hotkey exactly once.
- Unregister it during shutdown.
- Avoid duplicate registrations across repeated toggles.
- Tolerate registration failure without crashing.

## 6. Shortcut Model

### FR-008: Shortcut Data

The shortcut reference MUST be modeled as an ordered list of entries:

- A key-combination label.
- A human-readable description.
- An optional grouping/section.

This model MUST live in the platform-independent domain layer and MUST be
free of AppKit types.

### FR-009: Default Shortcut Set

The initial default set is informational and reflects the intended roadmap.
Executing these shortcuts is **out of scope** for this specification; only the
overlay toggle is functional.

| Combination | Action |
| --- | --- |
| `⌘⌥/` | Toggle this overlay |
| `⌘⌥M` | Monocle layout |
| `⌘⌥S` | Master/Stack layout |
| `⌘⌥C` | Columns layout |
| `⌘⌥R` | Rows layout |
| `⌘⌥B` | BSP layout |
| `⌘⌥←/→/↑/↓` | Move focus |
| `⌘⌥⇧←/→` | Move window |
| `⌘⌥Return` | Swap with master |
| `⌘⌥F` | Toggle floating |
| `⌘⌥Q` | Quit |

The list MUST be defined in one place so the overlay and future shortcut
handling share a single source of truth.

## 7. Lifecycle

### FR-010: Graceful Shutdown

Shutdown MUST:

- Unregister the global hotkey.
- Remove the hotkey event handler.
- Close and release the overlay window.
- Release Core Foundation/AppKit resources.
- Exit without leaving background processes or windows.

## 8. Performance

### NFR-001

- No polling loops for window visibility; visibility changes are driven by the
  hotkey and the bounded run loop.
- Toggling MUST be immediate (no perceptible delay).
- The overlay MUST NOT block the run loop.

### NFR-002: Thread Safety

- AppKit window creation, mutation, and teardown MUST occur on the main thread.
- Native window and hotkey handles MUST NOT cross threads.
- No `unsafe` `Send`/`Sync` implementations for native handles.

## 9. Error Handling

### FR-011: Failure Modes

Handle and report distinctly:

- Accessibility permission denied (hotkey unavailable).
- Overlay window creation failure.
- Global hotkey registration failure.
- Overlay teardown failure when detectable.

A hotkey registration failure MUST NOT abort the process; the overlay MUST
still be shown once at startup.

## 10. Testing Strategy

### Unit Tests

Test platform-independent logic only:

- Shortcut list construction and ordering.
- Overlay visibility state transitions (shown → hidden → shown).
- Toggle idempotence and no duplicate registration.
- Placement within a visible frame, including multi-display frames.

### Manual macOS Tests

1. Run `cargo run -- overlay`.
2. Verify the translucent overlay appears at startup.
3. Press `⌘⌥/` with another app frontmost; verify the overlay toggles.
4. Verify the overlay does not steal focus or activate the app.
5. Verify content matches the default shortcut set.
6. Verify placement on each connected display.
7. Press Ctrl+C and verify clean shutdown with no leftover window.
8. Repeat without Accessibility permission and verify a clear error.

## 11. Acceptance Criteria

### AC-001: Execution Mode

The `overlay` command starts a foreground process holding the overlay.

### AC-002: Translucent Window

A small, translucent, always-on-top window is displayed.

### AC-003: Shortcut Content

The window displays the shortcut reference from the shared default set.

### AC-004: Startup Visibility

The overlay is shown once at startup without the hotkey.

### AC-005: Toggle Hotkey

A global hotkey toggles overlay visibility without activating the application.

### AC-006: Non-Intrusive

The overlay does not steal focus and does not appear in the Dock or app switcher.

### AC-007: Placement

The overlay stays within the visible frame of the relevant display.

### AC-008: Graceful Shutdown

Ctrl+C releases the window and hotkey with no leftover process or window.

### AC-009: Failure Isolation

A hotkey registration failure still shows the overlay and reports the error.

### AC-010: Regression Safety

Existing SPEC-001..SPEC-006 functionality remains intact.

### AC-011: Automated Validation

The following commands pass:

- `cargo fmt --all -- --check`
- `cargo check --all-targets`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test --all-targets`

### AC-012: Native Verification

The overlay and toggle are manually verified on macOS; unavailable checks are
documented rather than assumed.

## 12. Out of Scope

- Executing any shortcut other than the overlay toggle.
- A configurable/config-file-driven shortcut set.
- Persistent overlay position or appearance.
- Animations and fade transitions.
- A menu bar status item.
- Automatic tiling and layout application.
- Multi-monitor overlay duplication.
- Background daemon or launch-agent installation.
- Private macOS APIs.

## 13. Definition of Done

SPEC-007 is complete when:

- The `overlay` command runs a foreground process with the overlay.
- A translucent, always-on-top shortcut window is rendered.
- The global toggle hotkey works without activating the application.
- The overlay is shown at startup and toggleable thereafter.
- Shutdown releases all native resources.
- Automated tests pass.
- Existing functionality remains intact.
- Native behavior is manually verified.

Any unavailable or unverified acceptance criterion must be explicitly
documented.
