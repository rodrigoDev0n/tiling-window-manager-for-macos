//! Screen work-area lookup via `NSScreen`.
//!
//! `NSScreen` reports frames in AppKit's global coordinate space (origin at the
//! bottom-left of the primary display, Y growing upward), while window geometry
//! from the Accessibility API uses the top-left origin convention (Y growing
//! downward). This module performs the conversion so callers always deal with
//! AX-style [`Rect`] values.

use objc2::MainThreadMarker;
use objc2_app_kit::NSScreen;
use objc2_foundation::NSPoint;

use crate::core::geometry::Rect;

/// Returns the visible frame (work area) of the screen containing `window`,
/// expressed in Accessibility coordinates.
///
/// Returns `None` when the window rectangle is invalid, when the current thread
/// is not the main thread, or when no screen matches.
pub fn work_area_for_window(window: Rect) -> Option<Rect> {
    if !window.is_valid() {
        return None;
    }

    let mtm = MainThreadMarker::new()?;
    let screens = NSScreen::screens(mtm);

    let primary_height = screens.iter().next()?.frame().size.height;
    // Use the window centre so that windows flush against a screen edge (for
    // example a window at y = 0) are still attributed to a screen.
    let centre_x = window.origin.x + window.size.width / 2.0;
    let centre_y = window.origin.y + window.size.height / 2.0;
    let appkit_point = NSPoint::new(centre_x, primary_height - centre_y);

    let screen = screens
        .iter()
        .find(|screen| contains_point(screen.frame(), appkit_point))?;

    let visible = screen.visibleFrame();
    let ax_y = primary_height - (visible.origin.y + visible.size.height);

    let work_area = Rect::new(
        visible.origin.x,
        ax_y,
        visible.size.width,
        visible.size.height,
    );
    work_area.is_valid().then_some(work_area)
}

fn contains_point(frame: objc2_foundation::NSRect, point: NSPoint) -> bool {
    point.x >= frame.origin.x
        && point.x < frame.origin.x + frame.size.width
        && point.y >= frame.origin.y
        && point.y < frame.origin.y + frame.size.height
}
