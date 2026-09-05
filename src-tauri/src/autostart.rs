//! Launching at login.
//!
//! **Deliberately not stored in `settings.json`.** Whether the app starts with
//! the session is the operating system's state, not ours: on Windows it is a
//! registry entry the user can add or remove without going near this app. A
//! copy in our own file would be a second source of truth, and the moment the
//! two disagreed the settings screen would be confidently wrong. So the OS is
//! asked every time, and written through.

use tauri::{AppHandle, Runtime};
use tauri_plugin_autostart::ManagerExt;

/// Whether the app is currently registered to launch at login.
///
/// A failure to read the registry reports "off" rather than an error. The
/// question is being asked to draw a checkbox, and a checkbox has no third
/// state; "off" is also the safer thing to show, since it invites the user to
/// set it rather than assuring them of something unverified.
#[tauri::command]
pub fn get_launch_at_login<R: Runtime>(app: AppHandle<R>) -> bool {
    app.autolaunch().is_enabled().unwrap_or(false)
}

/// Register or unregister the app for launch at login.
///
/// Returns the state as it stands *afterwards*, read back from the OS rather
/// than assumed from the request, so a write that silently failed shows up in
/// the UI instead of being papered over.
#[tauri::command]
pub fn set_launch_at_login<R: Runtime>(app: AppHandle<R>, enabled: bool) -> Result<bool, String> {
    let manager = app.autolaunch();

    let outcome = if enabled {
        manager.enable()
    } else {
        manager.disable()
    };

    outcome.map_err(|error| error.to_string())?;

    Ok(manager.is_enabled().unwrap_or(false))
}
