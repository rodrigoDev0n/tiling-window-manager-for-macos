# SPEC-004: Platform-Independent Layout Engine

**Status:** Draft  
**Priority:** High  
**Dependencies:** SPEC-001, SPEC-002, SPEC-003  
**Next:** SPEC-005 — Layout Application

## 1. Objective

Implement a deterministic, platform-independent tiling layout engine in Rust.

The engine must calculate target window rectangles based on:

- Available screen geometry.
- An ordered collection of manageable windows.
- A selected layout algorithm.
- Layout configuration.

The engine must not interact with native macOS APIs or modify actual application windows.

Its sole responsibility is to calculate window placement.

## 2. Architectural Principles

The layout engine MUST:

- Be implemented in pure Rust.
- Remain independent from macOS APIs.
- Use deterministic algorithms.
- Avoid side effects.
- Be testable without Accessibility permissions.
- Reuse geometry primitives from previous specifications.
- Avoid unnecessary heap allocations.
- Support future layout extensions.
- Separate layout computation from layout application.

The engine MUST NOT:

- Import AppKit, Core Graphics, or Accessibility frameworks.
- Access native window handles.
- Execute shell commands.
- Modify actual windows.
- Query display information directly.
- Maintain global mutable state.
- Introduce asynchronous execution.

## 3. Domain Models

### FR-001: Window Representation

Define or reuse a platform-independent window identifier.

The layout engine MUST receive an ordered collection of window identifiers.

Requirements:

- Window identifiers must be unique within one layout calculation.
- Identifiers must not expose native macOS handles.
- Window ordering must be preserved.
- The engine must not depend on process IDs or window titles.
- Duplicate identifiers must produce a validation error.

Suggested representation:

`WindowId`

The exact Rust type must be selected during implementation review.

### FR-002: Geometry Representation

Reuse the geometry types introduced in previous specifications.

Suggested models:

- `Point`
- `Size`
- `Rect`

Requirements:

- Support finite floating-point coordinates.
- Reject non-finite geometry values.
- Reject negative or zero dimensions for usable layout areas.
- Support negative screen origins.
- Avoid implicit coordinate conversions.
- Use the canonical coordinate convention established in SPEC-003.

All layout calculations must use the same coordinate system.

### FR-003: Layout Configuration

Define a platform-independent configuration model.

The configuration MUST support:

- Selected layout type.
- Outer gap.
- Inner gap.
- Master area ratio.
- Number of master windows.

Default configuration:

- Layout: MasterStack
- Outer gap: 8 logical points
- Inner gap: 8 logical points
- Master ratio: 0.55
- Master window count: 1

The configuration MUST:

- Reject negative gaps.
- Reject non-finite values.
- Require a master ratio strictly between 0 and 1.
- Require at least one master window.
- Validate that the requested layout can fit within the available geometry.

No configuration file parsing is required in this specification.

## 4. Layout Algorithms

### FR-004: Master-Stack Layout

Implement a master-stack algorithm.

Behavior:

- The first N windows occupy the master area.
- Remaining windows occupy the stack area.
- Master windows are arranged vertically within the master area.
- Stack windows are arranged vertically within the stack area.
- The master area is positioned on the left.
- The stack area is positioned on the right.
- The master ratio determines the proportion of available horizontal space assigned to the master area.

Rules:

- Zero windows: return an empty layout.
- One window: occupy the entire usable area after outer gaps.
- When all windows fit within the configured master count, use the entire usable area for the master group.
- When stack windows exist, reserve one inner gap between the master and stack areas.
- Distribute remaining space deterministically.
- Preserve the input window order.

### FR-005: Columns Layout

Implement an equal-width column layout.

Behavior:

- Arrange all windows horizontally.
- Each window receives an equal share of available width.
- Apply inner gaps between adjacent windows.
- Apply outer gaps around the complete layout.
- Preserve window ordering.

### FR-006: Rows Layout

Implement an equal-height row layout.

Behavior:

- Arrange all windows vertically.
- Each window receives an equal share of available height.
- Apply inner gaps between adjacent windows.
- Apply outer gaps around the complete layout.
- Preserve window ordering.

### FR-007: Monocle Layout

Implement a monocle layout.

Behavior:

- Every window receives the same target rectangle.
- The target rectangle occupies the usable area after outer gaps.
- Inner gaps are ignored.
- Window ordering is preserved.

The engine MUST NOT:

- Minimize windows.
- Hide windows.
- Change window focus.
- Change window stacking order.

Those responsibilities belong to other components.

## 5. Layout Calculation

### FR-008: Calculation Interface

Expose a platform-independent layout calculation interface.

Suggested conceptual signature:

`calculate_layout(windows, available_area, configuration) -> Result<LayoutResult, LayoutError>`

The implementation may use idiomatic Rust traits or functions.

The result MUST contain:

- Selected layout type.
- Ordered window identifiers.
- Calculated target rectangles.

The engine MUST NOT mutate the input collection.

### FR-009: Deterministic Results

Given identical:

- Window ordering.
- Available geometry.
- Layout type.
- Configuration.

The engine MUST produce identical target rectangles.

The engine must not depend on:

- System time.
- Operating system state.
- Window focus.
- Application names.
- Hash-map iteration order.
- Randomness.

### FR-010: Gap Management

Support:

- Outer gaps around the layout.
- Inner gaps between adjacent tiled windows.

Requirements:

- Gaps must be non-negative.
- Gaps must not cause negative window dimensions.
- Gaps must be applied consistently.
- Monocle ignores inner gaps.
- If gaps consume all available space, return a meaningful error.

For layouts that distribute space along an axis, calculate the available dimension after subtracting all required gaps.

### FR-011: Minimum Geometry

The engine MUST:

- Never produce negative dimensions.
- Never produce zero-sized window rectangles.
- Reject insufficient available space.
- Avoid division by zero.
- Handle empty window collections.
- Handle very small screen areas.
- Avoid producing NaN or infinity.

Application-specific minimum window dimensions are out of scope.

### FR-012: Layout Bounds

For MasterStack, Columns, and Rows:

- All calculated rectangles must remain inside the usable area.
- Rectangles must not overlap.
- Gaps must be preserved within floating-point tolerance.

For Monocle:

- Overlapping rectangles are intentional.
- Every window receives the same target rectangle.

## 6. Precision and Rounding

### FR-013: Floating-Point Geometry

The engine MUST:

- Use the geometry precision established by the core domain.
- Avoid premature integer rounding.
- Ensure calculated dimensions remain finite.
- Use a consistent epsilon for floating-point comparisons in tests.
- Avoid accumulated rounding errors when distributing space.

Pixel rounding and native macOS geometry adjustment belong to SPEC-005.

## 7. Error Handling

### FR-014: Layout Errors

Define meaningful domain errors for:

- Invalid geometry.
- Invalid configuration.
- Duplicate window identifiers.
- Insufficient available space.
- Non-finite calculations.

Errors MUST:

- Be explicit.
- Be testable.
- Avoid panics.
- Provide enough context for troubleshooting.

Do not use `unwrap()` or `expect()` in production paths unless justified by a documented invariant.

## 8. Architecture

Suggested responsibilities:

- `core/geometry.rs`: shared geometry primitives.
- `core/window.rs`: platform-independent window identifiers.
- `core/layout.rs`: layout interfaces and results.
- `layouts/master_stack.rs`: master-stack algorithm.
- `layouts/columns.rs`: column algorithm.
- `layouts/rows.rs`: row algorithm.
- `layouts/monocle.rs`: monocle algorithm.
- `layouts/mod.rs`: layout dispatch.

These are suggested responsibilities, not mandatory filenames.

Reuse existing structures whenever possible.

Do not create unnecessary abstractions.

## 9. Non-Functional Requirements

### NFR-001: Performance

- Layout calculation must not invoke native APIs.
- Avoid unnecessary allocations.
- Prefer linear-time algorithms where practical.
- Do not introduce threading or asynchronous runtimes.
- Do not optimize prematurely.

### NFR-002: Maintainability

- Follow idiomatic Rust conventions.
- Keep layout algorithms independent.
- Avoid duplicated geometry calculations.
- Use explicit types and meaningful names.
- Keep configuration validation centralized.

### NFR-003: Extensibility

The architecture should allow future algorithms such as:

- Binary Space Partitioning.
- Dynamic master ratios.
- Master area positioning.
- User-defined layout strategies.

Do not implement these features yet.

## 10. Acceptance Criteria

### AC-001: Pure Layout Engine

The layout engine compiles and runs without invoking macOS APIs.

### AC-002: Master-Stack

Correctly distributes 0, 1, 2, 3, and N windows.

### AC-003: Columns

Correctly distributes windows into equal-width columns.

### AC-004: Rows

Correctly distributes windows into equal-height rows.

### AC-005: Monocle

Assigns identical target rectangles to all windows.

### AC-006: Gap Handling

Inner and outer gaps are applied correctly.

### AC-007: Determinism

Identical inputs produce identical outputs.

### AC-008: Geometry Safety

All successful calculations produce valid, finite, positive-sized rectangles.

### AC-009: Input Validation

Invalid configuration, duplicate IDs, and insufficient space return meaningful errors.

### AC-010: Coordinate Compatibility

The engine supports negative origins and the coordinate convention defined in SPEC-003.

### AC-011: Regression Safety

Existing SPEC-001, SPEC-002, and SPEC-003 functionality remains intact.

### AC-012: Quality Validation

The following commands pass:

- `cargo fmt --all -- --check`
- `cargo check --all-targets`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test --all-targets`

## 11. Testing Strategy

### Unit Tests

Test all four layout algorithms with:

- Zero windows.
- One window.
- Two windows.
- Three windows.
- Many windows.
- Negative screen origins.
- Non-square display geometry.
- Different inner and outer gaps.
- Very small available areas.
- Invalid configuration.
- Duplicate window identifiers.

### Property-Based Invariants

For every successful layout:

- Output count equals input count.
- Output order matches input order.
- All rectangles are finite.
- All dimensions are positive.
- Results are deterministic.
- All rectangles remain inside available bounds.

For non-overlapping layouts:

- No two rectangles overlap.
- Configured gaps are respected.

Property-based testing frameworks are optional.

### Integration Tests

Verify that the engine can consume display geometry from the core domain without requiring native window control.

Do not modify real windows during testing.

## 12. Out of Scope

- Applying layouts to macOS windows.
- Native window movement.
- Native window resizing.
- Automatic tiling.
- Window event observers.
- Window focus management.
- Global keyboard shortcuts.
- Persistent application state.
- Floating-window rules.
- Application-specific size constraints.
- BSP layouts.
- Native pixel rounding.
- macOS Spaces management.

## 13. Definition of Done

SPEC-004 is complete when:

- All four layout algorithms are implemented.
- All acceptance criteria are evaluated.
- Unit tests cover normal and edge cases.
- Automated validation passes.
- No native macOS APIs are required by layout calculations.
- Existing functionality remains unaffected.
- Any unverified criteria are explicitly documented.

## 14. Approved Amendments (SPEC-004 review)

The following amendments were approved on implementation:

1. **Window identifier (FR-001 open item).** `WindowId` is an opaque
   caller-assigned `u64` newtype in `core/window.rs`. It is unique within one
   calculation (duplicates are rejected), never dereferenced by the engine, and
   mapped to native handles only by SPEC-005. The CLI preview assigns the
   sequential discovery index.
2. **Remainder absorption (FR-013).** Axis distributions are computed with
   `base = (total - (count-1) * inner) / count` and the **last** segment
   absorbs the floating-point remainder so the final edge lands exactly on the
   content boundary. Adjacent gaps are exact within float precision.
3. **Space checks.** A uniform rule requires the outer-gapped content area to
   have positive width and height for every layout, and per-axis `InsufficientSpace`
   when `(count-1) * inner_gap >= total`. Monocle ignores inner gaps.
4. **Read-only preview CLI (added scope).** `tiling_window layout [--layout <kind>]
   [--display <id>] [--outer-gap <f64>] [--inner-gap <f64>] [--ratio <f64>]
   [--master-count <usize>]` computes a layout for the accessible windows whose
   centre lies on the target display (default: main) and prints the target
   rectangles **without modifying any window**. Laying out real windows is
   SPEC-005.
5. **Module placement.** Types live in `core/layout.rs`; the four algorithms and
   the `calculate` dispatcher live in `src/layouts/`; `core/layout.rs` re-exports
   `calculate`. `Shortcut`/`shortcut_rect` from SPEC-002 are unchanged.
6. **Edge case: "more windows than the space can accommodate".** No minimum
   window size constraint (out of scope per FR-011), so arbitrarily many
   windows receive tiny positive rectangles; failure occurs only when gaps or
   the master ratio consume all space.

