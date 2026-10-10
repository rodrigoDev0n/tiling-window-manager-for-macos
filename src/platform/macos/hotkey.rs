//! Global hotkey registration through the Carbon Event Manager (SPEC-007).
//!
//! `RegisterEventHotKey`/`InstallEventHandler` are documented (legacy, but
//! public) Carbon APIs. They integrate with the process's main run loop, work
//! while another application is frontmost, and need no event tap or helper
//! process, which is why SPEC-007 selects them for the overlay toggle.
//!
//! The Carbon callback only flips an atomic flag; the foreground run loop owns
//! the actual window mutation. Native handles stay on the main thread and are
//! never shared across threads.

use std::ffi::c_void;
use std::mem::size_of;
use std::ptr;
use std::sync::atomic::{AtomicBool, Ordering};

/// The Command modifier bit (`cmdKey`).
pub const CMD: u32 = 0x0100;
/// The Option modifier bit (`optionKey`).
pub const OPTION: u32 = 0x0800;
/// Virtual key code for `/` (`kVK_ANSI_Slash`).
pub const SLASH: u32 = 0x2C;

/// Four-character signature identifying this application's hotkeys ("OVER").
const OVERLAY_SIGNATURE: u32 = 0x4F56_4552;
/// Identifier of the overlay toggle hotkey within the signature.
const OVERLAY_HOTKEY_ID: u32 = 1;

const NO_ERR: i32 = 0;
// `kEventClassKeyboard`/`kEventHotKeyPressed`.
const EVENT_CLASS_KEYBOARD: u32 = 0x6B65_7962;
const EVENT_HOT_KEY_PRESSED: u32 = 5;
// `kEventParamDirectObject` and `typeEventHotKeyID`.
const PARAM_DIRECT_OBJECT: u32 = 0x2D2D_2D2D;
const TYPE_EVENT_HOT_KEY_ID: u32 = 0x686B_6964;

type EventTargetRef = *mut c_void;
type EventHandlerRef = *mut c_void;
type EventHotKeyRef = *mut c_void;
type EventRef = *const c_void;
type EventHandlerCallRef = *mut c_void;

#[repr(C)]
#[derive(Clone, Copy)]
struct EventHotKeyID {
    signature: u32,
    id: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct EventTypeSpec {
    event_class: u32,
    event_kind: u32,
}

#[link(name = "Carbon", kind = "framework")]
unsafe extern "C" {
    fn RegisterEventHotKey(
        in_hot_key_code: u32,
        in_hot_key_modifiers: u32,
        in_hot_key_id: EventHotKeyID,
        in_target: EventTargetRef,
        in_options: u32,
        out_ref: *mut EventHotKeyRef,
    ) -> i32;

    fn UnregisterEventHotKey(in_hot_key: EventHotKeyRef) -> i32;

    fn GetApplicationEventTarget() -> EventTargetRef;

    fn InstallEventHandler(
        in_target: EventTargetRef,
        in_handler: Option<unsafe extern "C" fn(EventHandlerCallRef, EventRef, *mut c_void) -> i32>,
        in_num_types: u32,
        in_list: *const EventTypeSpec,
        in_user_data: *mut c_void,
        out_ref: *mut EventHandlerRef,
    ) -> i32;

    fn RemoveEventHandler(in_handler: EventHandlerRef) -> i32;

    fn GetEventParameter(
        in_event: EventRef,
        in_name: u32,
        in_desired_type: u32,
        out_actual_type: *mut u32,
        in_buffer_size: u32,
        out_actual_size: *mut u32,
        out_data: *mut c_void,
    ) -> i32;
}

/// Failure while installing the handler or registering the hotkey.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HotkeyError {
    /// `InstallEventHandler` returned a non-zero `OSStatus`.
    InstallFailed(i32),
    /// `RegisterEventHotKey` returned a non-zero `OSStatus`.
    RegisterFailed(i32),
}

impl std::fmt::Display for HotkeyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HotkeyError::InstallFailed(status) => {
                write!(
                    f,
                    "failed to install the hotkey event handler (status {status})"
                )
            }
            HotkeyError::RegisterFailed(status) => {
                write!(f, "failed to register the global hotkey (status {status})")
            }
        }
    }
}

impl std::error::Error for HotkeyError {}

/// Set by the Carbon callback; cleared by [`take_toggle_request`].
static TOGGLE_REQUESTED: AtomicBool = AtomicBool::new(false);

/// Returns and clears a pending toggle request made by the global hotkey.
///
/// This is the only channel from the native callback to the run loop; the
/// callback itself never touches the window.
pub fn take_toggle_request() -> bool {
    TOGGLE_REQUESTED.swap(false, Ordering::SeqCst)
}

unsafe extern "C" fn hotkey_handler(
    _handler_call_ref: EventHandlerCallRef,
    event: EventRef,
    _user_data: *mut c_void,
) -> i32 {
    let mut hotkey_id = EventHotKeyID {
        signature: 0,
        id: 0,
    };
    // SAFETY: `event` is a valid EventRef supplied by the Carbon Event Manager;
    // the parameters point at stack storage sized exactly for EventHotKeyID.
    let status = unsafe {
        GetEventParameter(
            event,
            PARAM_DIRECT_OBJECT,
            TYPE_EVENT_HOT_KEY_ID,
            ptr::null_mut(),
            size_of::<EventHotKeyID>() as u32,
            ptr::null_mut(),
            (&mut hotkey_id as *mut EventHotKeyID).cast::<c_void>(),
        )
    };

    if status == NO_ERR
        && hotkey_id.signature == OVERLAY_SIGNATURE
        && hotkey_id.id == OVERLAY_HOTKEY_ID
    {
        TOGGLE_REQUESTED.store(true, Ordering::SeqCst);
    }

    NO_ERR
}

/// A registered global hotkey; unregisters and removes its handler on drop.
pub struct Hotkey {
    hotkey: EventHotKeyRef,
    handler: EventHandlerRef,
}

impl Hotkey {
    /// Registers the overlay toggle hotkey (`⌘⌥/`) with the application target.
    pub fn register_toggle() -> Result<Self, HotkeyError> {
        let event_type = EventTypeSpec {
            event_class: EVENT_CLASS_KEYBOARD,
            event_kind: EVENT_HOT_KEY_PRESSED,
        };

        let mut handler: EventHandlerRef = ptr::null_mut();
        // SAFETY: the application event target is owned by the process; the
        // handler and the event type list outlive the installed handler for the
        // lifetime of this struct.
        let status = unsafe {
            InstallEventHandler(
                GetApplicationEventTarget(),
                Some(hotkey_handler),
                1,
                &event_type,
                ptr::null_mut(),
                &mut handler,
            )
        };
        if status != NO_ERR {
            return Err(HotkeyError::InstallFailed(status));
        }

        let hotkey_id = EventHotKeyID {
            signature: OVERLAY_SIGNATURE,
            id: OVERLAY_HOTKEY_ID,
        };
        let mut hotkey: EventHotKeyRef = ptr::null_mut();
        // SAFETY: the target and hotkey id are valid for the duration of the
        // call; `hotkey` receives the registration handle.
        let status = unsafe {
            RegisterEventHotKey(
                SLASH,
                CMD | OPTION,
                hotkey_id,
                GetApplicationEventTarget(),
                0,
                &mut hotkey,
            )
        };
        if status != NO_ERR {
            // SAFETY: `handler` was installed successfully above.
            unsafe { RemoveEventHandler(handler) };
            return Err(HotkeyError::RegisterFailed(status));
        }

        Ok(Self { hotkey, handler })
    }
}

impl Drop for Hotkey {
    fn drop(&mut self) {
        // SAFETY: both handles were produced by the matching registration
        // functions and are unregistered exactly once here.
        unsafe {
            UnregisterEventHotKey(self.hotkey);
            RemoveEventHandler(self.handler);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toggle_request_is_consumed_once() {
        assert!(!take_toggle_request());
        TOGGLE_REQUESTED.store(true, Ordering::SeqCst);
        assert!(take_toggle_request());
        assert!(!take_toggle_request());
    }

    #[test]
    fn default_hotkey_is_command_option_slash() {
        assert_eq!(CMD | OPTION, 0x0900);
        assert_eq!(SLASH, 0x2C);
    }
}
