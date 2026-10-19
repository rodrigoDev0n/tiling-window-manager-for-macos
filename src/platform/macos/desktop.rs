//! Desktop background windows (SPEC-011) and menu-bar covers (SPEC-012).
//!
//! One borderless, non-activating `NSWindow` per active display draws the
//! manager's background (image or flat color) at a desktop window level, so
//! it renders behind every normal application window and visually replaces
//! the system wallpaper. Toggling the desktop only moves the windows between
//! two levels:
//!
//! - **Hidden** (default): above the Finder desktop icon window, so the icons
//!   are covered by the manager's background.
//! - **Shown**: at `kCGDesktopWindowLevel`, below the Finder icon window, so
//!   the icons render on top of the manager's background.
//!
//! While the desktop is hidden, one additional **menu-bar cover** window per
//! display with a menu-bar strip sits above the menu bar (level 26, above the
//! menu bar at 24 and status items at 25), painting the same background so
//! the strip looks like a seamless extension of the wallpaper. Unlike the
//! desktop windows, the covers absorb mouse clicks (SPEC-012 FR-004) and do
//! not join fullscreen auxiliary spaces (SPEC-012 §3.5).
//!
//! The level values were verified empirically with `CGWindowLevelForKey` and
//! `CGWindowListCopyWindowInfo` (macOS 15): the Finder icon window sits at
//! -2147483603, `kCGDesktopWindowLevel` is -2147483623, normal windows are 0,
//! and the menu bar is 24. `-1` therefore sits above the icons and below every
//! normal window, the menu bar, the Dock, and status items.
//!
//! All AppKit work happens on the main thread; the windows never steal focus
//! and ignore mouse events (SPEC-011 §15.2).

use objc2::ClassType;
use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2_app_kit::{
    NSBackingStoreType, NSColor, NSImage, NSPanel, NSWindow, NSWindowCollectionBehavior,
    NSWindowLevel, NSWindowStyleMask,
};
use objc2_foundation::NSString;
use objc2_quartz_core::{CALayer, kCAGravityResizeAspectFill};

use crate::app::desktop::{Background, FlatColor};
use crate::core::coordinates;
use crate::core::display::{self, Display};

/// `kCGDesktopWindowLevelKey`: level between the system wallpaper and the
/// Finder desktop icon window.
const CG_DESKTOP_WINDOW_LEVEL_KEY: i32 = 2;

/// Level used while the desktop icons are hidden (see module docs).
const DESKTOP_HIDDEN_LEVEL: NSWindowLevel = -1;

/// Level of the menu-bar covers: above the main menu bar (`kCGMainMenuBarWindowLevel`,
/// 24) and the status items (`NSStatusWindowLevel`, 25). Verified empirically
/// with a magenta probe window (SPEC-012 §2.3).
const MENU_BAR_COVER_LEVEL: NSWindowLevel = 26;

/// Role of a manager-owned background window; determines its level, mouse
/// behavior, and Space participation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WindowRole {
    /// Desktop background behind all normal windows (SPEC-011).
    Desktop,
    /// Menu-bar strip cover above the menu bar (SPEC-012).
    MenuBarCover,
}

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    /// Returns the window level associated with a `CGWindowLevelKey`.
    fn CGWindowLevelForKey(key: i32) -> i32;
}

/// Failure while preparing the desktop background as a whole.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BackgroundError {
    /// AppKit work was attempted off the main thread.
    NotMainThread,
    /// No primary screen height could be determined for coordinate conversion.
    InvalidGeometry,
}

impl std::fmt::Display for BackgroundError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BackgroundError::NotMainThread => {
                write!(f, "the desktop background requires the main thread")
            }
            BackgroundError::InvalidGeometry => {
                write!(f, "could not determine the primary screen geometry")
            }
        }
    }
}

impl std::error::Error for BackgroundError {}

/// The manager's desktop background: one window per active display, optional
/// menu-bar covers (SPEC-012), and the diagnostics collected while creating
/// them.
pub struct DesktopBackground {
    windows: Vec<Retained<NSWindow>>,
    covers: Vec<Retained<NSWindow>>,
    diagnostics: Vec<String>,
}

impl DesktopBackground {
    /// Creates the background windows for `displays`, initially in the
    /// desktop-**hidden** state (SPEC-011 FR-005, FR-007): the covers are
    /// ordered in so the menu bar is hidden together with the icons.
    ///
    /// A failure affecting one display (invalid bounds, undecodable image)
    /// never prevents the remaining displays from getting a background; it is
    /// reported through [`DesktopBackground::diagnostics`] with flat-color
    /// fallback where applicable.
    pub fn create(displays: &[Display], background: &Background) -> Result<Self, BackgroundError> {
        let mtm = MainThreadMarker::new().ok_or(BackgroundError::NotMainThread)?;
        if displays.is_empty() {
            return Ok(Self {
                windows: Vec::new(),
                covers: Vec::new(),
                diagnostics: Vec::new(),
            });
        }
        let main_height = crate::platform::macos::displays::main_screen_height()
            .ok_or(BackgroundError::InvalidGeometry)?;

        let mut windows = Vec::with_capacity(displays.len());
        let mut covers = Vec::new();
        let mut diagnostics = Vec::new();
        for display in displays {
            let Some(frame) = coordinates::ax_to_appkit(display.bounds, main_height) else {
                diagnostics.push(format!(
                    "display {}: invalid bounds; background skipped",
                    display.id
                ));
                continue;
            };
            if let Some(window) = create_window(
                mtm,
                WindowRole::Desktop,
                frame,
                display.scale,
                display.id.0,
                background,
                &mut diagnostics,
            ) {
                windows.push(window);
            }

            // SPEC-012: cover the menu-bar strip of this display, if any.
            let Some(strip) = display::menu_bar_strip(display) else {
                continue;
            };
            let Some(strip_frame) = coordinates::ax_to_appkit(strip, main_height) else {
                diagnostics.push(format!(
                    "display {}: invalid menu-bar strip; strip not covered",
                    display.id
                ));
                continue;
            };
            if let Some(cover) = create_window(
                mtm,
                WindowRole::MenuBarCover,
                strip_frame,
                display.scale,
                display.id.0,
                background,
                &mut diagnostics,
            ) {
                covers.push(cover);
            }
        }

        Ok(Self {
            windows,
            covers,
            diagnostics,
        })
    }

    /// Reports why individual displays fell back to the flat color or were
    /// skipped (SPEC-011 FR-010).
    pub fn diagnostics(&self) -> &[String] {
        &self.diagnostics
    }

    /// Whether any display shows a background window.
    pub fn is_active(&self) -> bool {
        !self.windows.is_empty()
    }

    /// Applies the visibility state: `true` shows the Finder desktop icons
    /// (background moves below them, menu-bar covers ordered out), `false`
    /// covers the icons and the menu bar (SPEC-011 FR-007, SPEC-012 FR-003).
    /// The painted background itself never changes.
    pub fn set_desktop_visible(&self, visible: bool) {
        let level = if visible {
            desktop_shown_level()
        } else {
            DESKTOP_HIDDEN_LEVEL
        };
        for window in &self.windows {
            window.setLevel(level);
        }
        for cover in &self.covers {
            if visible {
                cover.orderOut(None);
            } else {
                cover.orderFrontRegardless();
            }
        }
    }
}

impl Drop for DesktopBackground {
    fn drop(&mut self) {
        for window in self.windows.iter().chain(&self.covers) {
            window.orderOut(None);
            window.close();
        }
    }
}

/// Creates, configures, fills and shows one background or cover window, or
/// returns `None` (with a diagnostic) when it cannot show a background.
fn create_window(
    mtm: MainThreadMarker,
    role: WindowRole,
    frame: crate::core::geometry::Rect,
    scale: f64,
    display_id: u32,
    background: &Background,
    diagnostics: &mut Vec<String>,
) -> Option<Retained<NSWindow>> {
    if !frame.is_valid() {
        diagnostics.push(format!(
            "display {display_id}: invalid background frame; background skipped"
        ));
        return None;
    }

    let appkit_frame = objc2_foundation::NSRect::new(
        objc2_foundation::NSPoint::new(frame.origin.x, frame.origin.y),
        objc2_foundation::NSSize::new(frame.size.width, frame.size.height),
    );

    // SAFETY: the window is immediately wrapped in a `Retained` and the
    // arguments are plain value types validated above.
    let window = match role {
        WindowRole::Desktop => {
            let window = unsafe {
                NSWindow::initWithContentRect_styleMask_backing_defer(
                    mtm.alloc::<NSWindow>(),
                    appkit_frame,
                    NSWindowStyleMask::Borderless,
                    NSBackingStoreType::Buffered,
                    false,
                )
            };
            // SAFETY: the window is owned by the `Retained` held in `Self`;
            // disabling released-when-closed keeps the Rust owner as the sole
            // lifetime owner.
            unsafe { window.setReleasedWhenClosed(false) };
            window
        }
        WindowRole::MenuBarCover => {
            // A non-activating panel receives the clicks that would
            // otherwise hit the invisible menu bar, without activating the
            // manager (SPEC-012 FR-004; same pattern as the overlay).
            let panel = NSPanel::initWithContentRect_styleMask_backing_defer(
                mtm.alloc::<NSPanel>(),
                appkit_frame,
                NSWindowStyleMask::Borderless | NSWindowStyleMask::NonactivatingPanel,
                NSBackingStoreType::Buffered,
                false,
            );
            // SAFETY: same ownership rationale as the desktop window above.
            unsafe { panel.setReleasedWhenClosed(false) };
            panel.into_super()
        }
    };

    window.setOpaque(true);
    window.setHasShadow(false);
    match role {
        WindowRole::Desktop => {
            window.setLevel(DESKTOP_HIDDEN_LEVEL);
            window.setIgnoresMouseEvents(true);
            // Join every Space (the background is the desktop) and stay out
            // of Mission Control window thumbnails and Exposé reordering.
            window.setCollectionBehavior(
                NSWindowCollectionBehavior::CanJoinAllSpaces
                    | NSWindowCollectionBehavior::Stationary
                    | NSWindowCollectionBehavior::FullScreenAuxiliary,
            );
        }
        WindowRole::MenuBarCover => {
            window.setLevel(MENU_BAR_COVER_LEVEL);
            // SPEC-012 FR-004: while the menu bar is invisible it must not
            // be clickable, so the cover absorbs pointer events.
            window.setIgnoresMouseEvents(false);
            // Fullscreen spaces have no menu bar; the cover must never
            // intrude over a fullscreen application.
            window.setCollectionBehavior(
                NSWindowCollectionBehavior::CanJoinAllSpaces
                    | NSWindowCollectionBehavior::Stationary,
            );
        }
    }

    let fill_color = match background {
        Background::Color(color) => *color,
        Background::Image(_) => FlatColor {
            red: 0,
            green: 0,
            blue: 0,
        },
    };
    window.setBackgroundColor(Some(&srgb_color(fill_color)));

    let Some(content_view) = window.contentView() else {
        diagnostics.push(format!(
            "display {display_id}: window has no content view; background skipped"
        ));
        window.close();
        return None;
    };
    content_view.setWantsLayer(true);
    let Some(layer) = content_view.layer() else {
        diagnostics.push(format!(
            "display {display_id}: content layer unavailable; background skipped"
        ));
        window.close();
        return None;
    };

    apply_background(mtm, &layer, background, scale, display_id, diagnostics);

    window.orderFrontRegardless();
    Some(window)
}

/// Paints `background` into `layer`: an image scaled to fill (aspect-ratio
/// preserving, cropped centered) or a flat color.
fn apply_background(
    mtm: MainThreadMarker,
    layer: &CALayer,
    background: &Background,
    scale: f64,
    display_id: u32,
    diagnostics: &mut Vec<String>,
) {
    match background {
        Background::Color(color) => {
            layer.setBackgroundColor(Some(&srgb_color(*color).CGColor()));
        }
        Background::Image(path) => {
            let file = NSString::from_str(&path.to_string_lossy());
            let Some(image) = NSImage::initWithContentsOfFile(mtm.alloc::<NSImage>(), &file) else {
                diagnostics.push(format!(
                    "display {display_id}: could not decode image {}; using flat color",
                    path.display()
                ));
                layer.setBackgroundColor(Some(&srgb_color(FlatColor::default()).CGColor()));
                return;
            };
            // SAFETY: `contents` is documented to accept an NSImage on macOS
            // (verified by rendering the layer offscreen before adoption), and
            // `image` outlives this call because CALayer retains its contents.
            let object: &AnyObject = image.as_super().as_super();
            unsafe { layer.setContents(Some(object)) };
            // SAFETY: reading a documented extern static from QuartzCore.
            let gravity = unsafe { kCAGravityResizeAspectFill };
            layer.setContentsGravity(gravity);
            layer.setContentsScale(scale);
        }
    }
}

/// Returns the level for the desktop-shown state: `kCGDesktopWindowLevel`,
/// below the Finder icon window and above the system wallpaper windows.
fn desktop_shown_level() -> NSWindowLevel {
    // SAFETY: `CGWindowLevelForKey` is a documented public Core Graphics
    // function taking a plain key constant.
    let level = unsafe { CGWindowLevelForKey(CG_DESKTOP_WINDOW_LEVEL_KEY) };
    level as NSWindowLevel
}

/// Builds an sRGB `NSColor` from 8-bit components.
fn srgb_color(color: FlatColor) -> Retained<NSColor> {
    NSColor::colorWithSRGBRed_green_blue_alpha(
        f64::from(color.red) / 255.0,
        f64::from(color.green) / 255.0,
        f64::from(color.blue) / 255.0,
        1.0,
    )
}
