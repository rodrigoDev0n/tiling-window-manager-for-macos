# SPEC-012: Menu Bar Hiding

**Status:** Implemented  
**Priority:** Medium  
**Dependencies:** SPEC-011 (desktop background windows, `ToggleDesktop` action)  
**Supersedes:** nothing (SPEC-011 §13 keeps the *Dock* out of scope; this spec only reclaims the menu bar from that restriction).  
**Next:** SPEC-013 — named workspaces (not defined yet)

## 1. Objective

Extend the SPEC-011 desktop toggle so `⌘⌥D` hides **everything**: while the
desktop is hidden, the menu bar (and its status items) is covered by the
manager's own background as well, producing a fully clean screen; toggling
back restores the menu bar and the Finder icons together.

## 2. Constraints and Research Findings

1. There is **no public macOS API to hide the menu bar globally**:
   `NSMenu.setMenuBarVisible` and `NSApplication.setPresentationOptions`
   only affect the calling application; `_HIHideMenuBar` writes a persistent
   user default and does not take effect reliably on recent macOS; hiding
   via `CGSSetMenuBarVisibility` is a private API and stays prohibited.
2. Native fullscreen Spaces cannot host other applications' windows, and
   macOS exposes no public API to create or switch Spaces.
3. **Empirically verified** (macOS 15): a borderless non-activating window at
   window level 26 (above the menu bar at 24 and status items at 25) covers
   the menu-bar strip completely. The strip height on the test machine is
   25 pt.

## 3. Design Decisions

1. One **menu-bar cover window per display**, sibling of the SPEC-011
   desktop windows, owned by the same `DesktopBackground` type.
2. The cover paints the **same background** (wallpaper image or flat color)
   as the desktop windows, so the strip looks like a seamless extension of
   the wallpaper.
3. The cover participates in the **same `DesktopState`**: hidden desktop ⇒
   cover ordered in at level 26; shown desktop ⇒ cover ordered out.
4. The cover **absorbs mouse clicks** (the menu bar behind it must not be
   clickable while invisible) and never activates the app (non-activating
   panel), consistent with SPEC-011 §15.2.
5. The cover does **not** join fullscreen auxiliary spaces: fullscreen
   spaces have no menu bar and their windows must never be covered.

## 4. Functional Requirements

### FR-001: Strip Geometry (pure)
A pure function derives the menu-bar strip rectangle of a display from its
`bounds` and `usable` areas: the top inset `bounds.maxY - usable.maxY`, full
width. Displays with a zero inset produce no cover.

### FR-002: Cover Windows
While the desktop background exists, one cover window per display with a
non-zero strip is created borderless at level 26, opaque, no shadow,
stationary, joining all regular Spaces, non-activating, absorbing mouse
events.

### FR-003: Toggle Semantics
`Action::ToggleDesktop` keeps its SPEC-011 meaning and additionally orders
the covers in (desktop hidden) or out (desktop shown). No new action, no new
binding.

### FR-004: Lifecycle
Cover windows live and die with the `DesktopBackground` (created at `run`
startup, closed at shutdown), exactly like the desktop windows.

## 5. Non-Functional Requirements

- No private APIs, no `Command`, no changes to user defaults.
- The strip height is never hard-coded; it is derived per display.
- All AppKit work stays on the main thread.

## 6. Testing Strategy

### Unit Tests (no macOS permission required)
- Strip rect derivation: normal display, display whose usable touches the
  top edge (zero inset), multi-display with different insets.
- Toggle logic covering covers + desktop windows is exercised through the
  existing pure `DesktopState` tests extended to cover presence.

### Manual macOS Tests
- `⌘⌥D` hides icons and menu bar together; status items disappear.
- Clicking the top strip while hidden does nothing (no phantom menus).
- Menu bar reappears on toggle back, unchanged (same menu, same items).
- Fullscreen app: neither the desktop windows nor the covers intrude.

## 7. Acceptance Criteria

### AC-001: Clean Screen
With the desktop hidden, Finder icons and the menu bar (including status
items) are both invisible on every display with a menu-bar strip.

### AC-002: Seamless Background
The covered strip shows the same background as the rest of the screen.

### AC-003: Toggle Round Trip
Toggling twice returns the exact prior menu-bar and icon state.

### AC-004: No Phantom Interaction
While hidden, clicks and hovers on the strip reach neither the menu bar nor
status items.

### AC-005: Regression
SPEC-011 behavior (wallpaper, flat color, diagnostics, `⌘⌥D` icon toggle)
and SPEC-001..010 remain intact; the validation suite passes.

## 8. Out of Scope

- Hiding the Dock (remains out of scope).
- Changing the system auto-hide menu-bar setting.
- Click-to-reveal gestures or hot edges.
- Named workspaces (deferred to SPEC-013).

## 9. Implementation Notes

- Strip geometry: `core::display::menu_bar_strip` derives the top band from
  `bounds` vs `usable` with a 1 pt minimum inset (rounding noise is ignored);
  three unit tests cover the normal case, side/bottom insets, and the
  zero-inset case.
- The covers are `NSPanel`s with the `NonactivatingPanel` style mask (same
  pattern as the overlay) upcast to `NSWindow`, owned by `DesktopBackground`
  and ordered in/out together with the desktop level change.
- The cover level 26 and the strip height were verified empirically on the
  target machine (magenta probe: menu bar 24, status items 25, strip 25 pt).
- AC-001..AC-005: automated suite (223 tests) plus the validation commands;
  interactive verification of the visual result is the manual step on the
  running agent.
