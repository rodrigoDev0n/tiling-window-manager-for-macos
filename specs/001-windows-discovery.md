# SPEC-001: macOS Window Discovery

**Status:** Draft  
**Priority:** High  
**Dependencies:** None

## 1. Objective

Implement native macOS window discovery using Rust and public Apple APIs.

The application must identify accessible windows and retrieve their basic metadata.

## 2. Functional Requirements

### FR-001: Accessibility Permissions

The system MUST:

- Detect whether Accessibility permissions are granted.
- Provide a command to inspect permission status.
- Display actionable instructions when permissions are missing.
- Avoid repeatedly prompting the user.

### FR-002: Window Discovery

The system MUST:

- Discover running applications.
- Enumerate accessible application windows.
- Retrieve application names and process IDs.
- Retrieve window titles when available.
- Retrieve window position and dimensions.
- Handle windows disappearing during enumeration.

### FR-003: CLI

The application MUST support:

`cargo run -- permissions`

`cargo run -- windows`

The windows command MUST display the discovered windows in a readable table.

## 3. Non-Functional Requirements

- Written in Rust.
- Use public macOS APIs.
- Isolate native FFI code.
- Avoid unnecessary memory allocations.
- Handle errors without crashing.
- No background polling.
- No external shell commands for window discovery.

## 4. Acceptance Criteria

- AC-001: The project compiles successfully.
- AC-002: The permissions command reports Accessibility authorization status.
- AC-003: The windows command enumerates accessible windows when permission is granted.
- AC-004: Window geometry is returned when supported.
- AC-005: Missing permissions produce an actionable error.
- AC-006: Inaccessible windows do not crash the application.
- AC-007: All platform-independent unit tests pass.
- AC-008: Formatting and Clippy validation pass.

## 5. Out of Scope

- Window movement.
- Window resizing.
- Automatic tiling.
- Keyboard shortcuts.
- Background services.
- Workspace management.
- Private macOS APIs.

## 6. Verification

Run:

- `cargo fmt --all -- --check`
- `cargo check --all-targets`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test --all-targets`

Native behavior must also be verified manually on macOS with Accessibility permission granted.

## 7. Definition of Done

The specification is complete when all acceptance criteria are satisfied and the implementation has been validated.

If a criterion cannot be verified, it must be reported as unverified rather than marked complete.

