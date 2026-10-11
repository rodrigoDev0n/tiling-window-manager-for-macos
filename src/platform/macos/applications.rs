//! Discovery of running GUI applications via `NSWorkspace`.

use objc2::rc::autoreleasepool;
use objc2_app_kit::{NSApplicationActivationPolicy, NSRunningApplication, NSWorkspace};

use crate::core::window::ProcessId;

/// A running application discovered through `NSWorkspace`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunningApplication {
    pub pid: ProcessId,
    pub name: String,
}

/// Returns the frontmost user-facing application, if any.
///
/// While this process runs as an accessory application (overlay/shortcut
/// manager), the system may report *this* process as frontmost even though the
/// user's focus never left another application. An accessory application does
/// not own the menu bar, so in that case the menu-bar-owning application is the
/// real active application (SPEC-008 FR-002/FR-003). Returning this process
/// would make focus/layout actions target a windowless app.
pub fn frontmost_application() -> Option<RunningApplication> {
    let own_pid = ProcessId(std::process::id() as i32);
    autoreleasepool(|_| {
        let workspace = NSWorkspace::sharedWorkspace();

        if let Some(app) = workspace.frontmostApplication() {
            let pid = ProcessId(app.processIdentifier());
            if pid != own_pid {
                return Some(describe(&app, pid));
            }
        }

        let app = workspace.menuBarOwningApplication()?;
        let pid = ProcessId(app.processIdentifier());
        (pid != own_pid).then(|| describe(&app, pid))
    })
}

/// Builds a [`RunningApplication`] from a native application handle.
fn describe(application: &NSRunningApplication, pid: ProcessId) -> RunningApplication {
    let name = application
        .localizedName()
        .map(|name| name.to_string())
        .unwrap_or_else(|| format!("PID {}", pid.0));
    RunningApplication { pid, name }
}

/// Enumerates running applications that present a regular user interface.
///
/// Background agents, daemons, and this process itself are excluded so that the
/// caller only receives applications that can plausibly own windows.
pub fn running_applications() -> Vec<RunningApplication> {
    let own_pid = ProcessId(std::process::id() as i32);

    autoreleasepool(|_| {
        let workspace = NSWorkspace::sharedWorkspace();
        let applications = workspace.runningApplications();

        applications
            .iter()
            .filter(|app| app.activationPolicy() == NSApplicationActivationPolicy::Regular)
            .filter_map(|app| {
                let pid = ProcessId(app.processIdentifier());
                if pid == own_pid {
                    return None;
                }
                Some(describe(&app, pid))
            })
            .collect()
    })
}
