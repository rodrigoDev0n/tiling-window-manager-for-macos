//! Global hotkey registration through the Carbon Event Manager (SPEC-007,
//! SPEC-008).
//!
//! `RegisterEventHotKey`/`InstallEventHandler` are documented (legacy, but
//! public) Carbon APIs. They integrate with the process's main run loop, work
//! while another application is frontmost, and need no event tap or helper
//! process.
//!
//! [`HotkeySet`] registers one Carbon handler and one hotkey per [`Binding`].
//! The Carbon callback only stores the pressed hotkey id in a bounded queue; the
//! foreground run loop maps ids back to [`Action`]s through the binding table
//! and owns the actual window mutation. Native handles stay on the main thread
//! and are never shared across threads.
//!
//! Only one [`HotkeySet`] should be alive at a time: the callback queue is a
//! process-wide static.

use std::collections::VecDeque;
use std::ffi::c_void;
use std::mem::size_of;
use std::ptr;
use std::sync::Mutex;

use crate::core::action::Action;
use crate::core::shortcut::{Binding, Key, Modifiers, format_binding};

/// The Command modifier bit (`cmdKey`).
const CMD: u32 = 0x0100;
/// The Shift modifier bit (`shiftKey`).
const SHIFT: u32 = 0x0200;
/// The Option modifier bit (`optionKey`).
const OPTION: u32 = 0x0800;

/// Four-character signature identifying this application's hotkeys ("OVER").
const SIGNATURE: u32 = 0x4F56_4552;

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

/// Failure while installing the Carbon event handler.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HotkeyError {
    /// `InstallEventHandler` returned a non-zero `OSStatus`.
    InstallFailed(i32),
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
        }
    }
}

impl std::error::Error for HotkeyError {}

/// Queue of hotkey ids received from the Carbon callback.
///
/// The callback only pushes; the run loop drains through
/// [`HotkeySet::take_pending`]. Holding the lock lasts for the duration of the
/// push/pop only, never while native APIs are invoked.
static PENDING_HOTKEYS: Mutex<VecDeque<u32>> = Mutex::new(VecDeque::new());

/// The virtual key code (`kVK_*`) for a logical [`Key`], or `None` when the
/// key has no ANSI key code.
pub fn virtual_key_code(key: Key) -> Option<u32> {
    match key {
        Key::Char('/') => Some(0x2C), // kVK_ANSI_Slash
        Key::Char('-') => Some(0x1B), // kVK_ANSI_Minus
        Key::Char(character) => ansi_letter_code(character),
        Key::Up => Some(0x7E),
        Key::Down => Some(0x7D),
        Key::Left => Some(0x7B),
        Key::Right => Some(0x7C),
    }
}

fn ansi_letter_code(character: char) -> Option<u32> {
    match character.to_ascii_lowercase() {
        'a' => Some(0x00),
        'b' => Some(0x0B),
        'c' => Some(0x08),
        'd' => Some(0x02),
        'e' => Some(0x0E),
        'f' => Some(0x03),
        'g' => Some(0x05),
        'h' => Some(0x04),
        'i' => Some(0x22),
        'j' => Some(0x26),
        'k' => Some(0x28),
        'l' => Some(0x25),
        'm' => Some(0x2E),
        'n' => Some(0x2D),
        'o' => Some(0x1F),
        'p' => Some(0x23),
        'q' => Some(0x0C),
        'r' => Some(0x0F),
        's' => Some(0x01),
        't' => Some(0x11),
        'u' => Some(0x20),
        'v' => Some(0x09),
        'w' => Some(0x0D),
        'x' => Some(0x07),
        'y' => Some(0x10),
        'z' => Some(0x06),
        _ => None,
    }
}

/// The Carbon modifier bits for `modifiers`.
pub fn modifier_bits(modifiers: Modifiers) -> u32 {
    let mut bits = 0;
    if modifiers.command {
        bits |= CMD;
    }
    if modifiers.option {
        bits |= OPTION;
    }
    if modifiers.shift {
        bits |= SHIFT;
    }
    bits
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

    if status == NO_ERR && hotkey_id.signature == SIGNATURE {
        if let Ok(mut queue) = PENDING_HOTKEYS.lock() {
            queue.push_back(hotkey_id.id);
        }
    }

    NO_ERR
}

/// A set of registered global hotkeys; unregisters them all and removes the
/// single event handler on drop.
pub struct HotkeySet {
    bindings: Vec<Binding>,
    hotkeys: Vec<EventHotKeyRef>,
    handler: EventHandlerRef,
    failures: Vec<String>,
}

impl HotkeySet {
    /// Registers one global hotkey per binding through a single Carbon handler.
    ///
    /// A binding that cannot be mapped to a key code or fails to register is
    /// skipped; its reason is reported by [`HotkeySet::failures`] and the
    /// remaining bindings stay active. Only an event-handler install failure
    /// aborts registration and raises [`HotkeyError`].
    pub fn register(bindings: &[Binding]) -> Result<Self, HotkeyError> {
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

        let mut set = HotkeySet {
            bindings: bindings.to_vec(),
            hotkeys: Vec::with_capacity(bindings.len()),
            handler,
            failures: Vec::new(),
        };

        for (index, binding) in bindings.iter().enumerate() {
            match virtual_key_code(binding.key) {
                None => set
                    .failures
                    .push(format!("unsupported key for {}", format_binding(binding))),
                Some(code) => {
                    // SAFETY: the target and hotkey id are valid for the duration
                    // of the call; `hotkey` receives the registration handle.
                    let hotkey_id = EventHotKeyID {
                        signature: SIGNATURE,
                        id: index as u32,
                    };
                    let mut hotkey: EventHotKeyRef = ptr::null_mut();
                    let status = unsafe {
                        RegisterEventHotKey(
                            code,
                            modifier_bits(binding.modifiers),
                            hotkey_id,
                            GetApplicationEventTarget(),
                            0,
                            &mut hotkey,
                        )
                    };
                    if status == NO_ERR {
                        set.hotkeys.push(hotkey);
                    } else {
                        set.failures.push(format!(
                            "failed to register {} (status {status})",
                            format_binding(binding)
                        ));
                    }
                }
            }
        }

        Ok(set)
    }

    /// Drains the pending hotkey presses and returns their actions, preserving
    /// order.
    pub fn take_pending(&self) -> Vec<Action> {
        let ids = {
            let Ok(mut queue) = PENDING_HOTKEYS.lock() else {
                return Vec::new();
            };
            queue.drain(..).collect::<Vec<u32>>()
        };

        ids.into_iter()
            .filter_map(|id| self.bindings.get(id as usize))
            .map(|binding| binding.action)
            .collect()
    }

    /// Reasons individual bindings could not be registered, in binding order.
    pub fn failures(&self) -> &[String] {
        &self.failures
    }
}

impl Drop for HotkeySet {
    fn drop(&mut self) {
        // SAFETY: every handle was produced by the matching registration
        // function and is unregistered exactly once here.
        for hotkey in &self.hotkeys {
            unsafe {
                UnregisterEventHotKey(*hotkey);
            }
        }
        // SAFETY: `handler` was installed by `register` and removed once.
        unsafe {
            RemoveEventHandler(self.handler);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::action::Direction;
    use crate::core::layout::LayoutType;

    #[test]
    fn pending_ids_are_drained_once() {
        PENDING_HOTKEYS.lock().unwrap().clear();
        PENDING_HOTKEYS.lock().unwrap().push_back(2);
        PENDING_HOTKEYS.lock().unwrap().push_back(1);

        let bindings = default_test_bindings();
        let set = HotkeySet {
            bindings: bindings.clone(),
            hotkeys: Vec::new(),
            handler: ptr::null_mut(),
            failures: Vec::new(),
        };

        let actions = set.take_pending();
        assert_eq!(actions.len(), 2);
        assert!(PENDING_HOTKEYS.lock().is_ok_and(|queue| queue.is_empty()));
        assert!(set.take_pending().is_empty());
    }

    #[test]
    fn pending_out_of_range_ids_are_ignored() {
        PENDING_HOTKEYS.lock().unwrap().clear();
        PENDING_HOTKEYS.lock().unwrap().push_back(99);
        let set = HotkeySet {
            bindings: default_test_bindings(),
            hotkeys: Vec::new(),
            handler: ptr::null_mut(),
            failures: Vec::new(),
        };
        assert!(set.take_pending().is_empty());
    }

    fn default_test_bindings() -> Vec<Binding> {
        vec![
            Binding {
                key: Key::Char('-'),
                modifiers: Modifiers::command_option(),
                action: Action::ToggleOverlay,
                description: String::new(),
            },
            Binding {
                key: Key::Char('m'),
                modifiers: Modifiers::command_option(),
                action: Action::SetLayout(LayoutType::Monocle),
                description: String::new(),
            },
            Binding {
                key: Key::Left,
                modifiers: Modifiers::command_option_shift(),
                action: Action::MoveWindow(Direction::Left),
                description: String::new(),
            },
        ]
    }

    #[test]
    fn default_hotkey_uses_command_option_minus() {
        let binding = &default_test_bindings()[0];
        assert_eq!(modifier_bits(binding.modifiers), 0x0900);
        assert_eq!(virtual_key_code(binding.key), Some(0x1B));
    }

    #[test]
    fn letter_keys_map_to_ansi_key_codes() {
        assert_eq!(virtual_key_code(Key::Char('m')), Some(0x2E));
        assert_eq!(virtual_key_code(Key::Char('S')), Some(0x01));
        assert_eq!(virtual_key_code(Key::Char('/')), Some(0x2C));
        assert_eq!(virtual_key_code(Key::Char('-')), Some(0x1B));
    }

    #[test]
    fn arrow_keys_map_to_function_key_codes() {
        assert_eq!(virtual_key_code(Key::Up), Some(0x7E));
        assert_eq!(virtual_key_code(Key::Down), Some(0x7D));
        assert_eq!(virtual_key_code(Key::Left), Some(0x7B));
        assert_eq!(virtual_key_code(Key::Right), Some(0x7C));
    }

    #[test]
    fn modifier_bits_reflect_the_selected_modifiers() {
        let none = Modifiers {
            command: false,
            option: false,
            shift: false,
        };
        assert_eq!(modifier_bits(none), 0);
        assert_eq!(modifier_bits(Modifiers::command_option_shift()), 0x0B00);
    }

    #[test]
    fn unsupported_chars_have_no_key_code() {
        assert_eq!(virtual_key_code(Key::Char('1')), None);
    }
}
