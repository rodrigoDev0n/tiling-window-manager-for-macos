# SPEC-003: Native macOS Display Management

**Status:** Draft  
**Priority:** High  
**Dependencies:** SPEC-001 — Window Discovery  
**Related:** SPEC-002 — Window Control

## 1. Objective

Implement a reliable display management subsystem for a native macOS tiling window manager written in Rust.

The subsystem must discover connected displays, retrieve their geometry, identify usable screen areas, and provide normalized coordinates for future layout calculations.

The architecture must support multiple displays without introducing virtual workspaces or private macOS APIs.

## 2. Product Philosophy

This project follows a hybrid, minimalistic window management approach.

Core principles:

- Respect native macOS behavior.
- Use public Apple APIs.
- Avoid virtual workspace emulation.
- Support multiple monitors.
- Keep display discovery independent from layout algorithms.
- Prefer predictable behavior over complex automation.
- Minimize dependencies and runtime overhead.

## 3. Functional Requirements

### FR-001: Display Discovery

The system MUST:

- Discover currently connected, active displays.
- Identify the main display.
- Provide a stable identifier for each display during the current display configuration.
- Retrieve display names when available.
- Handle systems with one or multiple monitors.
- Handle displays with negative coordinate origins.
- Avoid relying on display enumeration order.

The system MUST distinguish transient display identifiers from persistent hardware identity.

### FR-002: Display Geometry

For every active display, retrieve:

- Display identifier.
- Full display bounds.
- Display origin.
- Width and height.
- Usable display bounds.
- Main-display status.

All geometry values MUST have documented coordinate units.

Do not assume that display coordinates and Accessibility window coordinates share the same origin or axis orientation.

### FR-003: Usable Display Area

The system MUST:

- Determine the usable area available to ordinary application windows.
- Respect macOS menu bar and Dock reservations as exposed by public APIs.
- Avoid hardcoded menu bar or Dock dimensions.
- Handle displays where the Dock is not present.
- Handle automatically hidden system UI according to the geometry reported by macOS.
- Ensure usable bounds remain within full display bounds.

Use native macOS APIs such as NSScreen visibleFrame when appropriate.

Do not infer usable geometry solely by subtracting fixed margins.

### FR-004: Coordinate Normalization

Implement explicit coordinate conversion between:

- Native display geometry coordinates.
- The coordinate convention used by Accessibility window geometry.
- Platform-independent layout coordinates.

The system MUST:

- Document coordinate origins and axis directions.
- Handle negative coordinates.
- Support displays positioned above, below, left, or right of the main display.
- Preserve floating-point precision where appropriate.
- Avoid implicit or duplicated coordinate transformations.
- Provide round-trip tests for supported transformations.

Use one canonical coordinate convention inside the layout domain.

### FR-005: Display Selection

The system MUST:

- Identify the main display.
- Allow selecting a display by its runtime identifier.
- Reject unknown or unavailable display identifiers.
- Avoid selecting displays solely by their enumeration index.

Display selection must not modify any system settings.

### FR-006: CLI Commands

Extend the existing CLI with:

`cargo run -- displays`

Lists active displays and their geometry.

`cargo run -- display-info <display-id>`

Displays detailed geometry information for one display.

The exact identifier syntax must be reviewed before implementation.

Example output fields:

- ID
- Name
- Main
- X
- Y
- Width
- Height
- Usable X
- Usable Y
- Usable Width
- Usable Height

Existing CLI commands must remain compatible.

### FR-007: Display Configuration Changes

The system MUST provide a mechanism for obtaining a fresh display snapshot.

The design MUST account for:

- Monitor connection.
- Monitor disconnection.
- Resolution changes.
- Display arrangement changes.
- Main-display changes.

Continuous event monitoring is out of scope for this specification.

Native display change notifications will be integrated into the future event system.

### FR-008: Geometry Validation

The system MUST:

- Reject invalid or non-finite geometry values.
- Handle zero-sized or otherwise invalid display bounds.
- Validate coordinate transformations.
- Return meaningful errors for unavailable displays.
- Avoid panics during display enumeration.

## 4. Architecture Requirements

Suggested responsibilities:

- `core/geometry.rs`: platform-independent geometry primitives.
- `core/display.rs`: display domain model.
- `platform/macos/displays.rs`: native display discovery.
- `platform/macos/coordinates.rs`: native coordinate transformations.
- `commands/`: CLI display commands.

These are suggested responsibilities, not mandatory filenames.

Reuse existing modules whenever possible.

### Architectural Constraints

- Core domain models MUST NOT import macOS frameworks.
- Native FFI MUST remain isolated.
- Coordinate conversion MUST be testable independently.
- Display discovery MUST NOT modify window geometry.
- Layout calculations MUST NOT be implemented in this specification.
- Avoid unnecessary global state.
- Respect AppKit main-thread requirements.

## 5. Non-Functional Requirements

### NFR-001: Performance

- No background polling.
- No persistent display observer yet.
- Avoid repeated discovery within one CLI operation.
- Minimize unnecessary native allocations.

### NFR-002: Safety

- Use public macOS APIs.
- Respect Core Foundation ownership rules.
- Do not introduce private APIs.
- Do not disable System Integrity Protection.
- Do not use shell commands for display discovery.

### NFR-003: Compatibility

- Preserve existing SPEC-001 functionality.
- Preserve implemented SPEC-002 functionality.
- Support modern macOS versions compatible with the project's declared minimum deployment target.
- Document unsupported or unavailable native behavior.

## 6. Acceptance Criteria

### AC-001: Display Discovery

All active displays are enumerated successfully.

### AC-002: Main Display

The system correctly identifies the main display.

### AC-003: Display Geometry

Full and usable display bounds are returned in documented coordinate units.

### AC-004: Coordinate Conversion

Coordinate transformations produce correct results for supported display arrangements.

### AC-005: Multiple Displays

The implementation supports displays with different origins and dimensions.

### AC-006: Display Selection

An existing display can be selected by identifier, and unknown identifiers produce meaningful errors.

### AC-007: Error Handling

Invalid geometry and native API failures do not crash the application.

### AC-008: Regression Testing

Existing window discovery and window control functionality remain unaffected.

### AC-009: Automated Validation

All required Cargo formatting, compilation, linting, and test commands pass.

### AC-010: Native Verification

Display bounds and usable areas are manually checked against actual macOS screen arrangements.

### AC-011: Fresh Snapshot

A subsequent CLI invocation reflects the current display configuration after a supported system change.

## 7. Testing Strategy

### Unit Tests

Test:

- Geometry validation.
- Negative coordinates.
- Different display sizes.
- Displays arranged horizontally.
- Displays arranged vertically.
- Coordinate transformations.
- Coordinate round trips.
- Invalid display identifiers.
- Main-display selection logic.

### Native Integration Tests

Verify:

- Display discovery.
- Main-display identification.
- Usable screen geometry.
- Coordinate-system compatibility.
- Existing CLI functionality.

Native tests must be clearly separated from platform-independent tests.

### Manual Tests

1. Run `cargo run -- displays`.
2. Verify the reported main display.
3. Compare reported bounds with macOS display settings.
4. Run `display-info` for an active display.
5. Verify usable screen bounds.
6. Change display resolution and repeat discovery.
7. If a second monitor is available, test multiple arrangements.
8. Confirm existing SPEC-001 and SPEC-002 commands still work.

Unavailable hardware scenarios must remain unverified rather than being marked as passed.

## 8. Out of Scope

- Window movement between displays.
- Automatic monitor assignment.
- Tiling layouts.
- Keyboard shortcuts.
- Background monitoring.
- Native Spaces management.
- Virtual workspaces.
- Changing display resolution.
- Changing display arrangement.
- Managing fullscreen applications.
- Private macOS APIs.

## 9. Definition of Done

SPEC-003 is complete when:

- All functional requirements are implemented.
- Acceptance criteria have been evaluated.
- Automated validation passes.
- Native behavior has been manually verified.
- Existing functionality remains intact.
- Coordinate conventions are documented.
- Known limitations are recorded.

Any unverified acceptance criterion must be explicitly reported.

## 10. Approved Amendments (SPEC-003 review)

The following amendments were approved on implementation:

1. **Coordinate conversion location.** Pure, testable conversions live in
   `core/coordinates.rs` (top-left origin, Y down, units = points) instead of
   `platform/macos/coordinates.rs`.
2. **Main display definition.** "Main display" means the menu-bar display,
   identified by `CGMainDisplayID()` (authored via a small isolated Core
   Graphics FFI in `platform/macos/core_graphics.rs`). It is distinct from
   `NSScreen.mainScreen`, which reports the screen of the key window.
3. **Identifier syntax (FR-006 open item).** `display-info` accepts a decimal
   `CGDirectDisplayID`, or a `0x`-prefixed hexadecimal value. Identifiers are
   runtime-scoped; no public API provides persistent hardware identity.
4. **Display scale.** The display model includes the `backingScaleFactor`
   (`scale`) as informative metadata; all geometry remains in points.
5. **Display identity.** Documented that `CGDirectDisplayID` is stable only for
   the current display configuration; it may change after reconnecting,
   reordering, sleeping, or rebooting displays.

