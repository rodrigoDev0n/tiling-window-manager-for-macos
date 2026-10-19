# Specifications Index

This directory holds the specifications for the macOS tiling window manager.
Specifications are the source of truth for expected behavior; `AGENTS.md` is the
source of truth for engineering conventions. Read `AGENTS.md` and the relevant
specification in full before implementing anything (see the Spec-Driven
Development workflow in `AGENTS.md`).

## Index

| Spec | Title | Status |
| --- | --- | --- |
| [SPEC-001](001-windows-discovery.md) | Window discovery | Draft |
| [SPEC-002](002-native-macos-window-control.md) | Native macOS window control | Draft |
| [SPEC-003](003-native-macos-display-management.md) | Native macOS display management | Draft |
| [SPEC-004](004-platform-independent-layout-engine.md) | Platform-independent layout engine | Draft |
| [SPEC-005](005-native-layout.md) | Native layout application | Draft |
| [SPEC-006](006-windows-event-system.md) | Window event system | Approved |
| [SPEC-007](007-shortcut-overlay.md) | Shortcut overlay window | Approved |
| [SPEC-008](008-shortcut-actions.md) | Shortcut actions | Implemented |
| [SPEC-009](009-automatic-tiling.md) | Automatic tiling | Implemented |
| [SPEC-011](011-desktop-wallpaper.md) | Desktop wallpaper and desktop visibility | Implemented |

## Numbering and supersession notes

- SPEC-007 reserves the former "SPEC-007 = Automatic Tiling" slot; automatic
  tiling is **SPEC-009**.
- SPEC-008 supersedes the SPEC-007 restriction that shortcuts are informational
  only.
- SPEC-009 supersedes the SPEC-005 rule that automatic tiling is out of scope
  and the SPEC-008 FR-005 rule that `SetLayout` does not persist state.
- SPEC-011 skips the SPEC-010 slot (reserved for floating windows /
  per-application rules) and supersedes the config-file restriction in the
  SPEC-008/009 out-of-scope sections.

## Suggested next

- SPEC-010 — Floating windows / per-application rules (not defined yet).
