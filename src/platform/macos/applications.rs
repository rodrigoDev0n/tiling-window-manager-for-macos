//! Discovery of running GUI applications via `NSWorkspace`.

use objc2::rc::autoreleasepool;
use objc2_app_kit::{NSApplicationActivationPolicy, NSWorkspace};

use crate::core::window::ProcessId;

/// A running application discovered through `NSWorkspace`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunningApplication {
    pub pid: ProcessId,
    pub name: String,
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
                let name = app
                    .localizedName()
                    .map(|name| name.to_string())
                    .unwrap_or_else(|| format!("PID {}", pid.0));
                Some(RunningApplication { pid, name })
            })
            .collect()
    })
}
