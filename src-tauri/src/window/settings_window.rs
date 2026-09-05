//! The settings window.
//!
//! Created on demand rather than declared in `tauri.conf.json`, because a
//! window that exists from launch is a window that has to be hidden at launch,
//! and a hidden window that fails to hide is a settings screen in the user's
//! face every time they log in.
//!
//! Unlike the rail this is an ordinary window: decorated, focusable, and present
//! in the taskbar. The rail hides from the taskbar because it is furniture; a
//! settings screen you cannot get back to after clicking behind it is just lost.

use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

pub const SETTINGS_WINDOW_LABEL: &str = "settings";

/// Open the settings window, or bring the existing one forward.
///
/// Never opens a second copy: two settings windows would be two views of one
/// file, and whichever was saved last would silently win.
pub fn open(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(SETTINGS_WINDOW_LABEL) {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
        return;
    }

    let _ = WebviewWindowBuilder::new(
        app,
        SETTINGS_WINDOW_LABEL,
        WebviewUrl::App("index.html".into()),
    )
    .title("tok-ching settings")
    // Sized to the content. The window cannot be resized, so slack at the
    // bottom is not something the user can tidy away themselves.
    .inner_size(440.0, 690.0)
    .resizable(false)
    .center()
    .build();
}
