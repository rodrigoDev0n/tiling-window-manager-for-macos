//! Translucent shortcut overlay window (SPEC-007).
//!
//! Uses a borderless, non-activating `NSPanel` with an `NSVisualEffectView`
//! (HUD material) content view so the overlay appears above normal windows
//! without stealing focus. All AppKit work happens on the main thread.

use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSBackingStoreType, NSColor, NSEventMask,
    NSFloatingWindowLevel, NSFont, NSPanel, NSScreen, NSTextView, NSView,
    NSVisualEffectBlendingMode, NSVisualEffectMaterial, NSVisualEffectState, NSVisualEffectView,
    NSWindowCollectionBehavior, NSWindowStyleMask,
};
use objc2_foundation::{NSDate, NSPoint, NSRect, NSRunLoopMode, NSSize, NSString};

use crate::app::overlay;
use crate::core::coordinates;
use crate::core::geometry::{Rect, Size};

/// Vertical inset from the top of the visible frame, in points.
const TOP_INSET: f64 = 48.0;
/// Corner radius of the rounded overlay, in points.
const CORNER_RADIUS: f64 = 12.0;
/// Point size of the monospaced shortcut text.
const FONT_SIZE: f64 = 13.0;
/// Horizontal/vertical padding inside the text view.
const TEXT_INSET_X: f64 = 18.0;
const TEXT_INSET_Y: f64 = 16.0;

/// Failure while constructing or preparing the overlay.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OverlayError {
    /// The display frame or requested size was invalid, or no main display
    /// height could be determined.
    InvalidGeometry,
    /// AppKit work was attempted off the main thread.
    NotMainThread,
}

impl std::fmt::Display for OverlayError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OverlayError::InvalidGeometry => {
                write!(f, "could not compute valid overlay geometry")
            }
            OverlayError::NotMainThread => {
                write!(f, "the overlay requires the main thread")
            }
        }
    }
}

impl std::error::Error for OverlayError {}

/// Prepares the process as a background (accessory) application so the overlay
/// window does not create a Dock icon or app-switcher entry.
pub fn prepare_application() -> Result<(), OverlayError> {
    let mtm = MainThreadMarker::new().ok_or(OverlayError::NotMainThread)?;
    let application = NSApplication::sharedApplication(mtm);
    let _ = application.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
    application.finishLaunching();
    Ok(())
}

/// Pumps pending application events for at most `timeout` seconds.
///
/// This drives AppKit's `/ Carbon's` main event loop, which is what dispatches
/// registered global hot keys to the Carbon handler. Unlike a bare run-loop
/// step, it also lets AppKit update the overlay window. The call blocks until
/// an event arrives or the timeout elapses, so it never busy-waits.
pub fn pump_events(timeout: f64) {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let application = NSApplication::sharedApplication(mtm);
    let expiration = NSDate::dateWithTimeIntervalSinceNow(timeout);
    let mode = NSString::from_str("kCFRunLoopDefaultMode");
    let mode: &NSRunLoopMode = &mode;
    if let Some(event) = application.nextEventMatchingMask_untilDate_inMode_dequeue(
        NSEventMask::Any,
        Some(&expiration),
        mode,
        true,
    ) {
        application.sendEvent(&event);
    }
    application.updateWindows();
}

/// A borderless, non-activating translucent overlay panel.
pub struct OverlayWindow {
    panel: Retained<NSPanel>,
}

impl OverlayWindow {
    /// Builds the overlay inside `visible_frame` (canonical, top-left origin
    /// coordinates), rendering `content` as monospaced text.
    ///
    /// Must be called on the main thread.
    pub fn create(visible_frame: Rect, size: Size, content: &str) -> Result<Self, OverlayError> {
        let mtm = MainThreadMarker::new().ok_or(OverlayError::NotMainThread)?;
        if !visible_frame.is_valid() || !size.is_valid() {
            return Err(OverlayError::InvalidGeometry);
        }

        let origin = overlay::overlay_origin(visible_frame, size, TOP_INSET);
        let canonical_rect = Rect::new(origin.x, origin.y, size.width, size.height);
        let main_height = main_display_height(mtm).ok_or(OverlayError::InvalidGeometry)?;
        let appkit = coordinates::appkit_to_ax(canonical_rect, main_height)
            .ok_or(OverlayError::InvalidGeometry)?;

        let style = NSWindowStyleMask::Borderless | NSWindowStyleMask::NonactivatingPanel;
        let panel = NSPanel::initWithContentRect_styleMask_backing_defer(
            mtm.alloc::<NSPanel>(),
            to_nsrect(appkit),
            style,
            NSBackingStoreType::Buffered,
            false,
        );

        // SAFETY: the panel is owned by the `Retained` below; disabling
        // released-when-closed keeps the Rust owner as the sole lifetime owner.
        unsafe { panel.setReleasedWhenClosed(false) };
        panel.setLevel(NSFloatingWindowLevel);
        panel.setOpaque(false);
        panel.setBackgroundColor(Some(&NSColor::clearColor()));
        panel.setHasShadow(true);
        panel.setFloatingPanel(true);
        panel.setBecomesKeyOnlyIfNeeded(true);
        panel.setCollectionBehavior(
            NSWindowCollectionBehavior::CanJoinAllSpaces
                | NSWindowCollectionBehavior::FullScreenAuxiliary
                | NSWindowCollectionBehavior::Stationary,
        );

        let content_frame =
            NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(size.width, size.height));

        let effect_view =
            NSVisualEffectView::initWithFrame(mtm.alloc::<NSVisualEffectView>(), content_frame);
        effect_view.setMaterial(NSVisualEffectMaterial::HUDWindow);
        effect_view.setBlendingMode(NSVisualEffectBlendingMode::BehindWindow);
        effect_view.setState(NSVisualEffectState::Active);
        effect_view.setEmphasized(true);
        effect_view.setWantsLayer(true);
        if let Some(layer) = effect_view.layer() {
            layer.setMasksToBounds(true);
            layer.setCornerRadius(CORNER_RADIUS);
        }

        let text_view = NSTextView::initWithFrame(mtm.alloc::<NSTextView>(), content_frame);
        text_view.setEditable(false);
        text_view.setSelectable(false);
        text_view.setDrawsBackground(false);
        text_view.setTextContainerInset(NSSize::new(TEXT_INSET_X, TEXT_INSET_Y));
        let font = NSFont::monospacedSystemFontOfSize_weight(FONT_SIZE, 0.0);
        text_view.setFont(Some(&font));
        text_view.setTextColor(Some(&NSColor::labelColor()));
        text_view.setString(&NSString::from_str(content));

        effect_view.addSubview(&text_view);
        let content_view: &NSView = &effect_view;
        panel.setContentView(Some(content_view));

        Ok(Self { panel })
    }

    /// Shows the overlay above other windows without activating the app.
    pub fn show(&self) {
        self.panel.orderFrontRegardless();
    }

    /// Hides the overlay (the panel stays alive for later toggles).
    pub fn hide(&self) {
        self.panel.orderOut(None);
    }
}

impl Drop for OverlayWindow {
    fn drop(&mut self) {
        self.panel.orderOut(None);
        self.panel.close();
    }
}

fn main_display_height(mtm: MainThreadMarker) -> Option<f64> {
    let screens = NSScreen::screens(mtm);
    let primary = screens.iter().next()?;
    let height = primary.frame().size.height;
    (height.is_finite() && height > 0.0).then_some(height)
}

fn to_nsrect(rect: Rect) -> NSRect {
    NSRect::new(
        NSPoint::new(rect.origin.x, rect.origin.y),
        NSSize::new(rect.size.width, rect.size.height),
    )
}
