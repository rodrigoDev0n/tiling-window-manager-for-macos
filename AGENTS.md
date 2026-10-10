#AGENTS.md — macOS Tiling Window Manager

## 1. Project Overview

This project is a native macOS tiling window manager written in Rust.

The goal is to build a lightweight, fast, reliable, and modular window manager capable of automatically arranging application windows across multiple monitors and macOS Spaces.

The project must prioritize:

- Performance and low memory consumption.
- Memory safety and predictable behavior.
- Modular and maintainable architecture.
- Native macOS integration.
- Event-driven window management.
- Minimal external dependencies.
- Robust error handling.
- Testability and long-term maintainability.

## 2. Technology Stack

- **Language:** Rust (stable toolchain).
- **Build system:** Cargo.
- **Platform:** macOS.
- **Native integration:** macOS Accessibility API, Core Foundation, Core Graphics, and AppKit where appropriate.
- **Async runtime:** Tokio, only when asynchronous I/O is necessary.
- **Serialization:** Serde, when configuration or structured data requires it.
- **Logging:** tracing and tracing-subscriber.
- **Testing:** Rust built-in testing framework.

Do not introduce additional frameworks or dependencies without a clear technical justification.

## 3. Rust Coding Standards

All code must follow idiomatic Rust practices.

### Mandatory rules

- Follow the official Rust API Guidelines.
- Prefer ownership and borrowing over unnecessary cloning.
- Prefer references and slices when ownership is not required.
- Use enums and pattern matching to model application states.
- Prefer strong types over primitive values when representing domain concepts.
- Keep functions small, cohesive, and focused.
- Avoid unnecessary allocations.
- Prefer iterators when they improve readability.
- Use traits to define abstractions between components.
- Avoid excessive generics or premature abstraction.
- Document public APIs when their purpose or invariants are not obvious.

### Strictly prohibited

- Do not use `unwrap()` or `expect()` in production paths unless a documented invariant makes failure impossible.
- Do not use `panic!()` for recoverable errors.
- Do not suppress compiler warnings without justification.
- Do not use `unsafe` unless required for native macOS interoperability.
- Do not introduce global mutable state without a strong justification.
- Do not use `std::process::Command` to implement functionality already available through appropriate native APIs without explaining the tradeoff.
- Do not silently ignore errors.

## 4. Architecture

Follow a modular, event-driven architecture.

Separate the project into the following logical responsibilities:

```text
src/
├── main.rs
├── app/
│   ├── mod.rs
│   └── state.rs
├── platform/
│   ├── mod.rs
│   └── macos/
│       ├── mod.rs
│       ├── accessibility.rs
│       ├── applications.rs
│       ├── displays.rs
│       ├── windows.rs
│       └── events.rs
├── core/
│   ├── mod.rs
│   ├── geometry.rs
│   ├── layout.rs
│   ├── window.rs
│   └── workspace.rs
├── layouts/
│   ├── mod.rs
│   ├── bsp.rs
│   ├── floating.rs
│   └── monocle.rs
├── config/
│   ├── mod.rs
│   └── settings.rs
├── commands/
│   └── mod.rs
└── error.rs
```

This structure is a starting point, not a requirement to create unused files.

### Architectural constraints

- The `core` module must not depend directly on macOS APIs.
- Platform-specific functionality must remain inside `platform/macos`.
- Layout algorithms must operate on platform-independent window geometry.
- Commands must be separated from their execution mechanisms.
- Application state changes must be explicit and predictable.
- Prefer dependency injection through constructors and traits.
- Avoid circular dependencies.
- Do not create abstractions without a concrete need.

## 5. macOS Integration

Use native macOS APIs whenever practical.

### Accessibility API

Use the macOS Accessibility API to:

- Discover manageable application windows.
- Read window positions and dimensions.
- Move and resize supported windows.
- Observe supported window and application changes.
- Identify focused windows when available.

### Native interoperability

- Isolate Objective-C and Core Foundation interoperability.
- Keep native handles behind safe Rust abstractions.
- Document ownership and lifetime requirements.
- Release native resources correctly.
- Validate native API return values.
- Handle applications that do not expose standard accessibility behavior.

### Platform limitations

Never assume that macOS provides unrestricted window management.

- Accessibility permissions must be explicitly granted by the user.
- Some applications do not support programmatic resizing.
- System windows may not be manageable.
- Fullscreen windows require special handling.
- macOS Spaces functionality may be limited by public APIs.
- Multi-monitor behavior must account for coordinate systems and scaling.

Do not rely on undocumented or private macOS APIs without explicit user approval.

## 6. Window Management Rules

Window management must be deterministic.

- Never move or resize a window unnecessarily.
- Do not continuously poll window positions when event-based observation is sufficient.
- Avoid repeated layout calculations when the relevant state has not changed.
- Preserve floating-window geometry when switching layouts.
- Handle windows being closed during an operation.
- Prevent recursive event loops caused by programmatic window changes.
- Validate dimensions before applying them.
- Support multi-monitor layouts without assuming identical resolutions.
- Do not block the main event loop with expensive operations.

## 7. Layout Engine

Layout algorithms must be independent from macOS.

A layout must receive:

- Available screen geometry.
- A collection of manageable windows.
- Layout configuration.
- Window constraints when available.

A layout must produce a deterministic collection of target window rectangles.

Layout calculations must be pure functions whenever possible.

Implement layouts incrementally. Start with a simple tiling layout before introducing BSP, monocle, or advanced layouts.

Avoid unnecessary recalculation or native API calls during layout computation.

## 8. Error Handling

Use `Result<T, E>` for fallible operations.

- Prefer explicit error types for domain-specific failures.
- Use `thiserror` when structured error definitions are useful.
- Use `anyhow` only at appropriate application boundaries.
- Preserve underlying error context.
- Differentiate permission errors, unsupported operations, unavailable windows, and transient failures.
- Recover gracefully when a window disappears or an application stops responding.

Never silently swallow errors.

## 9. Concurrency and Performance

- Prefer event-driven processing over polling.
- Avoid unnecessary threads.
- Do not introduce Tokio solely for window management.
- Respect AppKit main-thread requirements.
- Do not assume native macOS handles are `Send` or `Sync`.
- Never implement `Send` or `Sync` manually for native handles without proving their thread-safety.
- Use channels for communication between components when appropriate.
- Avoid holding locks while invoking native APIs.
- Debounce bursts of window events when necessary.
- Measure performance before optimizing.

## 10. Testing

All platform-independent business logic must be testable without macOS APIs.

### Required tests

- Window geometry calculations.
- Screen boundary calculations.
- Layout positioning.
- Window insertion and removal.
- Layout resizing.
- Multiple-monitor scenarios.
- Empty window collections.
- Invalid or unsupported dimensions.
- Configuration parsing and validation.

Use mocks or test doubles for platform interfaces when needed.

Do not require Accessibility permissions for pure unit tests.

## 11. Validation Commands

Before considering a task complete, run:

```bash
cargo fmt --all -- --check
cargo check --all-targets
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets
```

For release validation:

```bash
cargo build --release
```

Do not claim a task is complete if required validation fails.

If validation cannot run, explain the limitation explicitly.

## 12. Dependency Management

- Use Cargo exclusively for Rust dependency management.
- Prefer well-maintained crates with clear documentation.
- Avoid adding dependencies for trivial functionality.
- Do not introduce multiple crates that solve the same problem without justification.
- Do not change dependency versions unnecessarily.
- Review the safety implications of FFI dependencies.
- Never execute installation scripts without understanding their purpose.

## 13. Agent Workflow

Before modifying code:

1. Inspect the existing project structure.
2. Read the relevant modules and tests.
3. Identify the smallest necessary change.
4. Preserve existing behavior unless a change is explicitly requested.
5. Explain significant architectural changes before implementing them.

During implementation:

1. Follow existing project conventions.
2. Implement the smallest coherent solution.
3. Add or update relevant tests.
4. Avoid unrelated refactoring.
5. Do not introduce unnecessary dependencies.
6. Do not overwrite existing work without justification.

After implementation:

1. Format the code.
2. Run compiler checks.
3. Run Clippy.
4. Run tests.
5. Report what changed.
6. Identify remaining limitations.

## 14. Agent Restrictions

The agent MUST NOT:

- Rewrite the entire project to solve a localized issue.
- Delete existing tests to make a build pass.
- Replace working implementations without justification.
- Introduce unsafe code as a shortcut.
- Modify unrelated modules.
- Change public interfaces unnecessarily.
- Add speculative functionality.
- Introduce hidden background processes.
- Disable macOS security protections.
- Bypass Accessibility permissions.
- Use private macOS APIs without explicit approval.
- Claim successful testing without executing the tests.

## 15. Engineering Philosophy

Prioritize correctness, simplicity, and maintainability over cleverness.

Build the window manager incrementally.

The preferred development sequence is:

1. Native macOS integration and permission checks.
2. Window discovery and geometry retrieval.
3. Window movement and resizing.
4. Pure layout engine.
5. Window event observation.
6. Automatic layout synchronization.
7. Multi-monitor support.
8. Keyboard commands and configuration.
9. Advanced layouts and performance improvements.

Always prefer a small, reliable implementation over a large, fragile one.

## Spec-Driven Development Workflow

This project follows Spec-Driven Development (SDD).

All significant features must be defined through specifications before implementation.

### Mandatory workflow

1. Read `AGENTS.md`.
2. Inspect `specs/README.md` and identify the relevant specification.
3. Read the complete specification before writing code.
4. Analyze functional requirements, constraints, and acceptance criteria.
5. Inspect existing code and tests.
6. Identify ambiguities, missing requirements, or conflicts.
7. Propose a concise implementation plan.
8. Wait for user approval before implementing a new feature.
9. Implement only the approved scope.
10. Create or update tests based on acceptance criteria.
11. Execute the required validation commands.
12. Report which acceptance criteria passed, failed, or remain unverified.

### Specification rules

- Never implement a feature without an approved specification.
- Never silently modify requirements.
- Never assume missing requirements.
- Ask for clarification when ambiguity affects behavior or architecture.
- Do not implement features marked as out of scope.
- Maintain traceability between requirements, implementation, and tests.
- Treat specifications as the source of truth for expected behavior.
- Treat `AGENTS.md` as the source of truth for engineering conventions.
- If a specification conflicts with `AGENTS.md`, report the conflict before proceeding.

### Specification review

When asked to review a specification:

- Do not modify production code.
- Identify technical risks.
- Identify missing edge cases.
- Evaluate compatibility with macOS public APIs.
- Evaluate testability.
- Propose improvements.
- Wait for approval before implementation.


