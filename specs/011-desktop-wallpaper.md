# SPEC-011: Desktop Wallpaper and Desktop Visibility

**Status:** Implemented  
**Priority:** Medium  
**Dependencies:** SPEC-003 (displays), SPEC-007 (overlay/run-loop foundations), SPEC-008 (run command, binding table)  
**Supersedes:** the SPEC-008/SPEC-009 out-of-scope restriction on config files (only the config-file part; gaps/ratio hotkeys and animations remain out of scope).  
**Next:** SPEC-010 — Floating windows / per-application rules (not defined yet)

## 1. Objective

Give the manager its own desktop:

1. The user points the manager at a **specific folder**; one image from that
   folder is displayed as the desktop background on every active display.
2. The macOS desktop (Finder icons and desktop files) is **hidden by default**
   while the manager runs.
3. A global shortcut **toggles desktop visibility** (show/hide the Finder
   icons) without ever removing the manager's background.
4. If **no wallpaper folder is configured** (or it is empty/unreadable), a
   **minimalist flat-color background** is shown instead.

All behavior is driven by shortcuts and a configuration file; no menu-bar
item, no daemon, no Finder restart.

## 2. Decisions Approved During Scoping

1. The background is a **manager-owned window** (not
   `NSWorkspace.setDesktopImageURL`, not Finder preferences): a borderless
   window placed at a desktop window level so it renders behind all normal
   application windows and behaves visually like the desktop.
2. Wallpaper selection is **random per session**: at `run` startup one
   supported image is chosen from the configured folder; the same image is
   used for the whole process lifetime.
3. Configuration lives in the project's **first TOML config file**, read at
   `run` startup.
4. Desktop visibility **defaults to hidden**.

## 3. Architectural Principles

The implementation MUST:

- Use public macOS APIs exclusively (AppKit window levels, Core Graphics).
- Keep all AppKit window code inside `platform/macos`.
- Keep the configuration model and the wallpaper-selection logic
  platform-independent and unit-testable (`config`, `core` or `app` layer).
- Model the toggle as a platform-independent `Action` in `core::action` and
  register it through the existing single binding table (`core::shortcut`),
  so the overlay reference and the hotkey never diverge (SPEC-008 FR-007).
- Reuse the existing `run` command, run loop and `HotkeySet` (SPEC-007/008)
  unchanged in structure.
- Reuse the SPEC-003 display list to place one background window per active
  display.
- Preserve SPEC-001..SPEC-010 behavior and commands.

The implementation MUST NOT:

- Use private macOS APIs or restart/kill Finder.
- Use `NSWorkspace.setDesktopImageURL` or write system wallpaper preferences.
- Poll: the background reacts only to startup, the toggle hotkey, and
  shutdown.
- Introduce a second event loop, threads, timers, or an async runtime.
- Introduce unrestricted global mutable state.
- Make the background window intercept application input (it must sit behind
  every normal window).

## 4. Configuration

### FR-001: Config File

New platform-independent module `config` (following the `AGENTS.md`
structure) that loads, if present:

```text
$XDG_CONFIG_HOME/tiling_window/config.toml
  (fallback: ~/.config/tiling_window/config.toml)
```

```toml
# All keys optional.
wallpaper_folder = "/Users/me/Pictures/Wallpapers"
background_color = "#1E1E1E"   # hex RRGGBB, used when no/empty folder
```

- Deserialized with `serde` (already an approved dependency class); the TOML
  crate is added to `Cargo.toml` as the single new dependency of this spec.
- A missing config file is **not an error**: defaults apply.
- A present-but-invalid file MUST produce a diagnostic and fall back to
  defaults; it MUST NOT abort `run`.
- `background_color` validates as `#RRGGBB`; an invalid value produces a
  diagnostic and the default color is used.

### FR-002: Defaults

- Default `background_color`: `#1E1E1E` (minimalist dark flat color).
- No default `wallpaper_folder`: absent means flat-color mode.

## 5. Wallpaper Selection

### FR-003: Supported Images

Supported extensions (case-insensitive): `png`, `jpg`, `jpeg`, `heic`,
`tiff`, `tif`, `bmp`, `gif`. Files are read through `std::fs` for listing
only; decoding happens natively via `NSImage` in the platform layer.

### FR-004: Selection Rule (pure)

Add a platform-independent selection function:

```text
select_wallpaper(candidates: &[PathBuf], random_u64: u64) -> Option<&Path>
```

- Returns `None` for an empty candidate list.
- Otherwise returns the candidate at index `random_u64 % candidates.len()`
  over candidates sorted by file name (deterministic given the random value,
  fully unit-testable; the random value is produced once at startup).
- Only regular files with a supported extension are collected; the folder is
  scanned non-recursively.

Behavior matrix:

| Folder configured | Folder has supported images | Background |
| --- | --- | --- |
| No | — | Flat color (default or configured) |
| Yes | Missing/unreadable/empty | Flat color + diagnostic |
| Yes | ≥1 image | Random image of the session |

## 6. Background Window

### FR-005: One Window Per Display

On `run` startup, after displays are known, the platform layer creates one
borderless, non-activating window per active display (SPEC-003 frame,
including Retina scaling):

- `NSWindow` with no title bar/decorations, `ignoresMouseEvents = true` while
  the desktop is hidden is **not** assumed; the window MUST NOT steal focus
  and MUST NOT appear in the Dock or app switcher (the process is already an
  accessory application, SPEC-007).
- The window draws either the selected image (scaled to fill, aspect-ratio
  preserving, cropped centered) or the flat color.
- The window level MUST be chosen so that:
  - it renders **behind all normal application windows**, and
  - it covers the Finder desktop icon window when the desktop is hidden, and
  - it renders **below** the Finder icon window when the desktop is shown
    (so icons appear on top of the manager's background).
  The exact `CGWindowLevelForKey` values are verified during implementation;
  the chosen levels MUST keep the menu bar and Dock above the background.
- Image decode or window creation failure for one display MUST NOT prevent
  the other displays from getting a background; the failure is reported and
  the flat color is used as fallback for that display.

### FR-006: Lifecycle

- Created once per `run` session.
- Never moved/resized in response to window events of other applications
  (out of scope: display reconfiguration mid-session; see §12).
- Closed on shutdown along with the other native resources (SPEC-007
  FR-010).

## 7. Toggle Action

### FR-007: `ToggleDesktop` Action

Extend `core::action`:

```text
Action = ... | ToggleDesktop
```

Semantics:

- Toggles a single platform-independent state flag
  (`desktop_visible: bool`, default `false` = hidden).
- When the flag changes, the platform layer repositions the background
  windows' level between the "hidden" (above Finder icons) and "shown"
  (below Finder icons) levels defined in FR-005. The image/color never
  changes on toggle.
- Toggling is idempotent per press: one press = one state flip.
- If background window creation failed entirely, `ToggleDesktop` is a no-op
  with a diagnostic; it MUST NOT crash the loop.

### FR-008: Default Binding

| Keys | Action | Description |
| --- | --- | --- |
| `⌘⌥D` | `ToggleDesktop` | Show or hide the desktop icons |

- Added to `default_bindings()`; the overlay reference derives from it
  automatically (new "Desktop" group in `default_shortcuts()`).
- No duplicate combination with existing SPEC-008/009 bindings.

## 8. Integration with `run`

### FR-009: Startup Order

`cargo run -- run` (and `run --no-auto`) extend their startup:

1. Verify Accessibility authorization (unchanged).
2. Load config (FR-001) — non-fatal on error.
3. Collect candidates and select the wallpaper (FR-004).
4. Create background windows for active displays (FR-005) with desktop
   **hidden** by default (FR-007).
5. Register the binding set (now including `ToggleDesktop`), init observers,
   initial tiling, enter the run loop (SPEC-009 FR-001 unchanged).

Other commands (`overlay`, `apply`, `move`, `focus`, `watch`, ...) MUST keep
working unchanged; they do not create background windows.

## 9. Error Handling

### FR-010: Failure Modes

Handle and report distinctly, without aborting the run loop:

- Config file missing → defaults, no diagnostic beyond a trace line.
- Config file invalid → diagnostic, defaults.
- Wallpaper folder missing/unreadable/empty → flat color + diagnostic.
- Image decode failure → flat color for that display + diagnostic.
- Display window creation failure → that display keeps no background; others
  unaffected; diagnostic.
- Hotkey registration failure for `ToggleDesktop` → reported; other hotkeys
  stay active (SPEC-008 FR-009 behavior unchanged).

## 10. Non-Functional Requirements

### NFR-001: Performance

- No polling; work happens only at startup, on the toggle hotkey, and at
  shutdown.
- Image decoding happens once per session (startup only).
- The background windows are not part of the tiling path: no per-event
  layout or discovery triggered by this feature.

### NFR-002: Thread Safety

- All AppKit window operations occur on the main thread.
- Config loading and wallpaper selection are pure/synchronous at startup.

### NFR-003: Maintainability

- Selection logic and config parsing are pure and unit-tested without macOS.
- The single binding table remains the only source of hotkeys and overlay
  text.
- Follow `AGENTS.md`; only new dependency: the TOML parser.

## 11. Testing Strategy

### Unit Tests (no macOS permission required)

- Config parsing: missing file, valid file, unknown keys, invalid color,
  empty file, folder key only.
- `select_wallpaper`: empty list → `None`; single candidate; deterministic
  index from a given `random_u64`; sorted-order independence of directory
  order; unsupported extensions filtered.
- Binding table: `⌘⌥D` present exactly once, no duplicate combinations, the
  overlay "Desktop" group derives from the bindings.
- Toggle state: starts hidden, flips per call, idempotent pairs.

### Integration Tests (test doubles)

- Startup with a stub platform layer: flat color used when no folder; image
  used when folder has candidates; per-display failure does not affect other
  displays.
- `ToggleDesktop` repositions levels through the platform trait without
  touching tiling state.

### Manual macOS Tests

1. Configure a folder with images, run `cargo run -- run`; the manager's
   background appears on every display behind all windows.
2. Verify Finder icons are **not** visible while the desktop is hidden.
3. Press `⌘⌥D`; the Finder icons appear on top of the manager's background.
4. Press `⌘⌥D` again; the icons disappear; the background never changes.
5. Remove `wallpaper_folder` (or point it to an empty folder), restart;
   verify the flat `#1E1E1E` background and the reported diagnostic.
6. Put two images in the folder, restart twice; verify a random image is used
   per session (may repeat; not asserted equal/different).
7. Verify the menu bar, the Dock, and all application windows render above
   the background, and that clicking a normal window still focuses it.
8. Verify `run --no-auto` behaves identically for this feature.
9. Ctrl+C exits with no leftover background window.

Unavailable checks MUST be documented rather than marked as passed.

## 12. Acceptance Criteria

### AC-001: Wallpaper Background
With `wallpaper_folder` configured, `run` shows a random image from that
folder as the background of every active display.

### AC-002: Flat Color Fallback
Without a configured folder, or when it is empty/unreadable, the background
is the flat color (`#1E1E1E` default) and a diagnostic is reported.

### AC-003: Desktop Hidden by Default
On startup the Finder desktop icons are not visible; the manager's background
is what remains visible behind normal windows.

### AC-004: Toggle Shortcut
`⌘⌥D` shows the desktop icons on top of the manager's background; pressing it
again hides them; the background is unchanged by toggling.

### AC-005: Overlay Consistency
The overlay lists `⌘⌥D` under a "Desktop" group derived from the binding
table.

### AC-006: Non-Interference
The background windows never steal focus, never appear above normal
windows, and never affect tiling, focus, or move actions.

### AC-007: Config Robustness
Missing/invalid config never aborts `run`.

### AC-008: Regression
All other commands and shortcuts keep working unchanged (SPEC-001..010).

### AC-009: Automated Validation
The following pass:

- `cargo fmt --all -- --check`
- `cargo check --all-targets`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test --all-targets`

### AC-010: Native Verification
Background, hidden-by-default, and toggle are manually verified on macOS;
unavailable checks are documented rather than assumed.

## 13. Out of Scope

- Per-display wallpaper (one image applies to all displays of the session).
- Wallpaper rotation hotkey, scheduled changes, folder watching.
- Reading or writing system wallpaper preferences
  (`NSWorkspace.setDesktopImageURL`, `defaults write com.apple.desktoppicture`).
- Hiding the Dock or the menu bar.
- Reacting to display reconfiguration (added/removed display) mid-session.
- Config keys for gaps, ratio, layouts, or key bindings.
- Floating windows and per-application rules (SPEC-010 candidate).

## 14. Definition of Done

SPEC-011 is complete when:

- `run` shows the configured wallpaper (or flat color) behind all windows on
  every active display.
- Finder desktop icons are hidden by default while `run` is active.
- `⌘⌥D` toggles icon visibility without changing the background.
- Config TOML is loaded defensively and the binding table stays the single
  source for the overlay and hotkeys.
- Automated tests pass and SPEC-001..SPEC-010 behavior is intact.
- Native behavior is manually verified or explicitly documented as
  unverified.

Any unavailable or unverified acceptance criterion must be explicitly
documented.

## 15. Open Decisions for Approval

1. **Window levels.** Exact `CGWindowLevelForKey` constants for the two
   states (hidden above icons / shown below icons) are chosen during
   implementation and documented in code; menu bar and Dock must remain above
   the background.
2. **Mouse events on the background.** Proposal: `ignoresMouseEvents = true`
   so clicks over empty desktop areas fall through to Finder even while icons
   are covered (no interactive element of ours is ever hit).
3. **Config path.** Proposal: `$XDG_CONFIG_HOME/tiling_window/config.toml`
   with `~/.config/tiling_window/config.toml` fallback, instead of
   `~/Library/Application Support`.
4. **Toggle key.** `⌘⌥D` is currently free in the binding table; confirm the
   choice.

## 16. Implementation Notes

Decisions taken while implementing (§15):

1. **Window levels** (verified empirically with `CGWindowLevelForKey` and
   `CGWindowListCopyWindowInfo` on macOS 15): the Finder desktop icon window
   sits at -2147483603, `kCGDesktopWindowLevel` (`CGWindowLevelKey` = 2) is
   -2147483623, normal windows are 0 and the menu bar is 24. Therefore:
   - **Hidden** (default): `-1` — above the icon window, below every normal
     window, the menu bar, and the Dock.
   - **Shown**: `CGWindowLevelForKey(2)` — below the icon window so the icons
     render on top of the manager's background.
2. **Mouse events:** `ignoresMouseEvents = true` as proposed (§15.2).
3. **Config path:** `$XDG_CONFIG_HOME/tiling_window/config.toml` with
   `~/.config/tiling_window/config.toml` fallback (§15.3).
4. **Toggle key:** `⌘⌥D`, as proposed (§15.4).
5. **Image rendering:** `NSImage` contents on the content view's `CALayer`
   with `kCAGravityResizeAspectFill` (aspect-ratio preserving, center
   crop) — verified by rendering the layer offscreen
   (`CALayer.render(in:)`).

Acceptance criteria status:

- **AC-001..AC-009:** covered by automated unit tests plus
  `fmt`/`check`/`clippy`/`test` (220 tests).
- **AC-010:** native level behavior verified empirically at implementation
  time; full interactive verification (toggle on a real desktop with Finder
  icons) remains a manual check and is not claimed as automated.
