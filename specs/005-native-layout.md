# SPEC-005: Native macOS Layout Application

**Status:** Draft  
**Priority:** Critical  
**Dependencies:** SPEC-001, SPEC-002, SPEC-003, SPEC-004  
**Next:** SPEC-006 — Window Event System

## 1. Objective

Implement a safe layout application system that connects the platform-independent Rust layout engine with native macOS window control.

The system must discover eligible windows, calculate their target positions, and apply the selected layout using public macOS Accessibility APIs.

This specification introduces manual tiling through CLI commands.

Automatic tiling, background services, global keyboard shortcuts, and event-driven synchronization are out of scope.

## 2. Architectural Principles

The implementation MUST:

- Reuse existing window discovery, control, display management, and layout engine components.
- Keep layout calculations independent from macOS.
- Separate layout planning from execution.
- Validate operations before modifying windows.
- Use public macOS APIs exclusively.
- Avoid unnecessary native API calls.
- Handle partial failures without crashing.
- Report actual execution results.
- Preserve existing CLI behavior.

The implementation MUST NOT:

- Duplicate the layout algorithms.
- Introduce background polling.
- Implement automatic tiling.
- Use private macOS APIs.
- Disable System Integrity Protection.
- Introduce unnecessary asynchronous execution.
- Modify windows outside the selected target display.
- Modify system display settings.

## 3. Functional Requirements

### FR-001: Window Discovery Integration

The system MUST:

- Reuse SPEC-001 window discovery.
- Retrieve accessible windows and their metadata.
- Identify each window independently.
- Avoid using PID alone as a unique identifier.
- Maintain native window references only for the lifetime of the operation.
- Handle windows disappearing during discovery or execution.

The system MUST preserve the ordering of selected windows when passing them to the layout engine.

### FR-002: Window Eligibility

Before calculating a layout, the system MUST determine which windows are eligible.

A window is eligible when:

- It belongs to a normal application window.
- It is accessible through public macOS APIs.
- Its position and size can be modified.
- It has valid geometry.
- It is not minimized.
- It is not in native fullscreen mode.
- It belongs to the selected display according to the approved assignment strategy.

The system MUST:

- Exclude unsupported windows.
- Exclude system dialogs when reliably identifiable.
- Avoid modifying windows whose classification is uncertain.
- Record exclusion reasons.
- Avoid using application names as the only eligibility criterion.

Windows that cannot be classified reliably MUST be skipped rather than modified.

### FR-003: Display Selection

The system MUST support selecting a target display.

Default behavior:

- Use the main display when no display is specified.

The system MUST:

- Reuse SPEC-003 display discovery.
- Use the selected display's usable bounds.
- Reject unavailable display identifiers.
- Respect the coordinate convention established in SPEC-003.
- Avoid assuming all displays share the same origin.

For this specification, only windows assigned to the selected display are eligible.

### FR-004: Window-to-Display Assignment

The system MUST implement a deterministic strategy for assigning windows to displays.

Default strategy:

- Assign each window to the display with the largest intersection area between the window rectangle and the display's full bounds.
- If there is a tie, prefer the display containing the window center.
- If the tie remains unresolved, use a documented deterministic display-ID ordering.
- If the window does not intersect any display, exclude it.

The strategy MUST:

- Work with negative coordinates.
- Support windows spanning multiple displays.
- Be testable without macOS APIs.
- Avoid modifying windows assigned to another display.

### FR-005: Layout Planning

Implement a layout planning stage that:

1. Discovers available displays.
2. Selects the target display.
3. Discovers windows.
4. Filters eligible windows.
5. Assigns windows to displays.
6. Preserves deterministic window ordering.
7. Invokes the SPEC-004 layout engine.
8. Produces a layout execution plan.

The execution plan MUST contain:

- Target display identifier.
- Selected layout type.
- Window identifiers.
- Original window geometry.
- Target window geometry.
- Eligibility and exclusion information.

The plan MUST NOT modify any window.

### FR-006: Layout Application

Implement an execution component that applies the generated plan.

For every eligible window:

- Revalidate that the window still exists.
- Revalidate required modification capabilities.
- Verify that the selected native reference still corresponds to the intended window.
- Apply target size and position using the SPEC-002 window control interface.
- Read back actual geometry.
- Compare actual geometry against target geometry.
- Record the result.

The implementation MUST account for native applications that adjust their window geometry.

The exact order of position and size operations must be chosen based on native behavior and documented.

### FR-007: Geometry Verification

After modifying a window, the system MUST:

- Retrieve its actual position.
- Retrieve its actual dimensions.
- Compare actual and requested geometry.
- Use an explicitly documented tolerance.
- Distinguish successful operations from constrained or unsuccessful operations.

A successful native API call MUST NOT automatically be treated as successful layout application.

### FR-008: Partial Failure Handling

Layout execution MUST support partial failures.

Examples:

- A window closes during execution.
- An application rejects resizing.
- A window becomes inaccessible.
- macOS applies minimum size constraints.
- Accessibility permissions are revoked.

The system MUST:

- Continue processing other independent windows when safe.
- Record failures individually.
- Avoid indefinite retries.
- Avoid panics.
- Return an execution summary.

Automatic rollback of previously modified windows is out of scope.

### FR-009: CLI Commands

Extend the CLI with:

`cargo run -- apply <layout>`

Applies the selected layout to eligible windows on the main display.

`cargo run -- apply <layout> --display <display-id>`

Applies the layout to the selected display.

`cargo run -- apply <layout> --dry-run`

Calculates and displays the execution plan without modifying windows.

Supported layout names:

- `master-stack`
- `columns`
- `rows`
- `monocle`

The implementation MUST preserve existing commands.

### FR-010: Dry-Run Mode

The system MUST support a non-mutating preview.

Dry-run MUST:

- Perform discovery.
- Validate window eligibility.
- Calculate the selected layout.
- Display original and target geometry.
- Show excluded windows and reasons.
- Avoid invoking native window mutation APIs.

Dry-run MUST NOT change window positions, sizes, focus, or display configuration.

### FR-011: Execution Report

After execution, the system MUST report:

- Selected display.
- Selected layout.
- Number of discovered windows.
- Number of eligible windows.
- Number of excluded windows.
- Number of successfully modified windows.
- Number of constrained windows.
- Number of failed windows.
- Actual resulting geometry for processed windows.
- Relevant errors and exclusion reasons.

Results MUST distinguish:

- Applied successfully.
- Applied with constraints.
- Failed.
- Skipped.

### FR-012: Deterministic Ordering

The system MUST define a deterministic ordering for eligible windows.

For the initial implementation:

- Prefer the existing stable discovery ordering when guaranteed.
- Otherwise use a documented deterministic ordering derived from available window metadata.

Ordering MUST NOT depend on hash-map iteration order.

If reliable ordering cannot be established, the implementation review MUST propose an explicit policy.

The layout engine MUST receive a uniquely identified, ordered window collection.

### FR-013: Safe Repeated Execution

Running the same layout repeatedly without changing windows, display geometry, or configuration SHOULD produce equivalent target geometry.

The system MUST:

- Avoid unnecessary mutations when actual geometry already matches the target within tolerance.
- Avoid introducing cumulative coordinate drift.
- Preserve deterministic planning.
- Avoid relying on previously persisted native references.

## 4. Architecture

Suggested responsibilities:

- `application/layout_planner.rs`: layout planning and orchestration.
- `application/layout_executor.rs`: execution and result collection.
- `application/window_eligibility.rs`: window filtering.
- `core/display_assignment.rs`: pure window-to-display assignment.
- `core/layout_plan.rs`: execution plan domain model.
- `commands/`: CLI integration.

These are suggested responsibilities, not mandatory filenames.

Reuse existing modules and conventions.

### Separation of Concerns

Layout Engine:

- Calculates target rectangles.
- Does not know about macOS.

Layout Planner:

- Selects windows and displays.
- Creates execution plans.
- Does not mutate windows.

Layout Executor:

- Applies the plan through platform interfaces.
- Verifies resulting geometry.
- Collects execution results.

Native macOS Layer:

- Owns Accessibility references.
- Performs actual window movement and resizing.
- Translates native errors.

## 5. Non-Functional Requirements

### NFR-001: Performance

- No background polling.
- Avoid redundant native calls.
- Avoid recalculating the same layout during execution.
- Keep processing proportional to the number of windows where practical.
- Do not introduce Tokio without a concrete requirement.

### NFR-002: Safety

- Never modify a window that failed eligibility validation.
- Never modify a window outside the selected display.
- Never use ambiguous window references.
- Never assume a window remains available between discovery and execution.
- Never silently ignore native failures.
- Avoid unintended changes to window focus.

### NFR-003: Maintainability

- Follow AGENTS.md.
- Preserve modular architecture.
- Keep platform-independent logic testable.
- Avoid duplicate domain models.
- Avoid unnecessary dependencies.
- Document important architectural decisions.

## 6. Acceptance Criteria

### AC-001: Manual Tiling

The CLI can apply a supported layout to eligible windows on the main display.

### AC-002: Layout Selection

All four SPEC-004 layouts can be selected.

### AC-003: Display Selection

The CLI supports selecting an available display.

### AC-004: Eligibility Filtering

Unsupported, minimized, fullscreen, and unmanageable windows are excluded safely.

### AC-005: Display Assignment

Windows are assigned to displays deterministically.

### AC-006: Dry Run

Dry-run displays the calculated plan without modifying windows.

### AC-007: Geometry Verification

Actual geometry is read after each applied operation.

### AC-008: Partial Failures

One window failing does not automatically prevent safe execution for other windows.

### AC-009: Repeated Execution

Repeated application of the same layout does not introduce geometry drift.

### AC-010: Regression Safety

Existing SPEC-001 through SPEC-004 functionality remains intact.

### AC-011: Automated Validation

The following commands pass:

- `cargo fmt --all -- --check`
- `cargo check --all-targets`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test --all-targets`

### AC-012: Native Verification

Manual tests confirm that real macOS windows are arranged according to the calculated layout, within documented platform constraints.

## 7. Testing Strategy

### Unit Tests

Test:

- Window eligibility decisions.
- Display intersection calculations.
- Display assignment.
- Layout plan construction.
- Empty window collections.
- Deterministic ordering.
- Geometry tolerance comparisons.
- Partial execution result aggregation.
- Invalid display selection.

### Integration Tests

Use test doubles for platform operations.

Verify:

- Planning never mutates windows.
- Dry-run never invokes mutation APIs.
- Execution applies the correct targets.
- Failed windows are reported.
- Other eligible windows continue when safe.
- Already correctly positioned windows are not modified.
- Existing CLI commands remain functional.

### Manual macOS Tests

1. Open three manageable application windows.
2. Run `apply master-stack --dry-run`.
3. Verify the proposed rectangles.
4. Run `apply master-stack`.
5. Confirm windows are repositioned.
6. Run the same command again.
7. Confirm no cumulative geometry drift.
8. Test `columns`, `rows`, and `monocle`.
9. Test an unsupported or restricted window.
10. If multiple displays are available, verify display isolation.

Do not mark unavailable hardware scenarios as passed.

## 8. Out of Scope

- Automatic tiling.
- Background daemons.
- Window event observers.
- Global keyboard shortcuts.
- Automatic focus management.
- Native Spaces manipulation.
- Virtual workspaces.
- Window animations.
- Persistent window ordering.
- Automatic rollback.
- Dynamic layout configuration files.
- Interactive resizing.
- BSP layouts.
- Cross-display window movement.

## 9. Definition of Done

SPEC-005 is complete when:

- All four layouts can be applied manually.
- Dry-run is functional and non-mutating.
- Window eligibility is validated.
- Display selection works.
- Native geometry changes are verified.
- Partial failures are reported.
- Repeated execution is stable.
- Automated tests pass.
- Native macOS behavior has been manually verified.
- Existing specifications remain unaffected.

Any unverified acceptance criterion MUST be explicitly reported.

