//! Native display discovery and work-area lookup.
//!
//! Geometry is read from `NSScreen` (points, AppKit global space) and converted
//! to the canonical top-left, Y-down layout convention through
//! [`crate::core::coordinates`]. Display identity comes from Core Graphics.

use objc2::MainThreadMarker;
use objc2_app_kit::NSScreen;
use objc2_foundation::{NSNumber, NSString};

use crate::core::coordinates;
use crate::core::display::{Display, DisplayId};
use crate::core::geometry::{Point, Rect};
use crate::platform::macos::core_graphics;

/// Enumerates the active displays with their geometry in canonical coordinates.
///
/// Returns an empty vector when the current thread is not the main thread or
/// when no displays are reported.
pub fn snapshot() -> Vec<Display> {
    let Some(mtm) = MainThreadMarker::new() else {
        return Vec::new();
    };

    let screens = NSScreen::screens(mtm);
    let Some(primary) = screens.iter().next() else {
        return Vec::new();
    };
    let main_height = primary.frame().size.height;
    let main_id = core_graphics::main_display_id();

    screens
        .iter()
        .filter_map(|screen| {
            let id = screen_number(&screen)?;
            let bounds = coordinates::appkit_to_ax(nsrect_to_rect(screen.frame()), main_height)?;
            let usable =
                coordinates::appkit_to_ax(nsrect_to_rect(screen.visibleFrame()), main_height)?;

            Some(Display {
                id: DisplayId(id),
                name: Some(screen.localizedName().to_string()),
                main: id == main_id,
                scale: screen.backingScaleFactor(),
                bounds,
                usable,
            })
        })
        .collect()
}

/// Returns the visible frame (work area) of the screen containing `window`,
/// expressed in canonical (Accessibility) coordinates.
///
/// Returns `None` when the window rectangle is invalid, the thread is not the
/// main thread, or no screen matches.
pub fn work_area_for_window(window: Rect) -> Option<Rect> {
    if !window.is_valid() {
        return None;
    }

    let mtm = MainThreadMarker::new()?;
    let screens = NSScreen::screens(mtm);
    let primary_height = screens.iter().next()?.frame().size.height;

    // Use the window centre so that windows flush against a screen edge (for
    // example a window at y = 0) are still attributed to a screen.
    let centre = Point::new(
        window.origin.x + window.size.width / 2.0,
        window.origin.y + window.size.height / 2.0,
    );
    let appkit_centre = coordinates::ax_point_to_appkit(centre, primary_height)?;

    let screen = screens
        .iter()
        .find(|screen| contains_point(screen.frame(), appkit_centre))?;

    coordinates::appkit_to_ax(nsrect_to_rect(screen.visibleFrame()), primary_height)
}

/// Returns the height in points of the primary (menu-bar) screen.
///
/// Used as the reference height for canonical/AppKit coordinate conversion.
/// Returns `None` off the main thread or when no screen is reported.
pub fn main_screen_height() -> Option<f64> {
    let mtm = MainThreadMarker::new()?;
    let screens = NSScreen::screens(mtm);
    let height = screens.iter().next()?.frame().size.height;
    (height.is_finite() && height > 0.0).then_some(height)
}

fn screen_number(screen: &NSScreen) -> Option<u32> {
    let key = NSString::from_str("NSScreenNumber");
    let description = screen.deviceDescription();
    let value = description.objectForKey(&key)?;
    let number = value.downcast_ref::<NSNumber>()?;
    Some(number.unsignedIntValue())
}

fn nsrect_to_rect(rect: objc2_foundation::NSRect) -> Rect {
    Rect::new(
        rect.origin.x,
        rect.origin.y,
        rect.size.width,
        rect.size.height,
    )
}

fn contains_point(frame: objc2_foundation::NSRect, point: Point) -> bool {
    point.x >= frame.origin.x
        && point.x < frame.origin.x + frame.size.width
        && point.y >= frame.origin.y
        && point.y < frame.origin.y + frame.size.height
}
