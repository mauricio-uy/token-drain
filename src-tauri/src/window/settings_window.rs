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

/// The rail's `additionalBrowserArgs`, read back from the config.
///
/// WebView2 shares one browser process per user-data directory, and every
/// webview in it must be created with the *same* command line. Building this
/// window without the rail's arguments makes the runtime reject it: the window
/// appears to be created, then dies before it can be shown, and `build()` has
/// already returned `Ok` by then.
///
/// Read from the config rather than duplicated as a literal. Two copies of a
/// string that must be byte-identical is a bug waiting for someone to edit one
/// of them.
fn rail_browser_args(app: &AppHandle) -> Option<String> {
    app.config()
        .app
        .windows
        .iter()
        .find(|window| window.label == crate::RAIL_WINDOW_LABEL)
        .and_then(|window| window.additional_browser_args.clone())
}

/// Open the settings window, or bring the existing one forward.
///
/// Never opens a second copy: two settings windows would be two views of one
/// file, and whichever was saved last would silently win.
pub fn open(app: &AppHandle) -> tauri::Result<()> {
    if let Some(window) = app.get_webview_window(SETTINGS_WINDOW_LABEL) {
        window.unminimize()?;
        window.show()?;
        window.set_focus()?;
        return Ok(());
    }

    let mut builder = WebviewWindowBuilder::new(
        app,
        SETTINGS_WINDOW_LABEL,
        WebviewUrl::App("index.html".into()),
    )
    .title("tok-ching settings")
    // Sized to the content. The window cannot be resized, so slack at the
    // bottom is not something the user can tidy away themselves.
    .inner_size(440.0, 690.0)
    .resizable(false)
    .center();

    if let Some(args) = rail_browser_args(app) {
        builder = builder.additional_browser_args(&args);
    }

    builder.build().map(|_| ())
}

/// Async so WebView2 creation is not blocked by a synchronous IPC command.
#[tauri::command]
pub async fn open_settings(app: AppHandle) -> Result<(), String> {
    open(&app).map_err(|_| "Could not open settings window.".to_owned())
}

#[cfg(test)]
mod tests {
    /// Every declared window must ask for the same browser arguments.
    ///
    /// This is the invariant that broke: `additionalBrowserArgs` was added to
    /// the rail alone, and the settings window — created in code without them —
    /// was rejected by a runtime that had already started its browser process
    /// with a different command line. The failure was silent in both
    /// directions: `build()` returned `Ok`, and nothing logged.
    ///
    /// Windows created in code cannot be checked from here; they go through
    /// `rail_browser_args` instead. This guards the config side, which is where
    /// a second set of arguments would most plausibly be introduced.
    #[test]
    fn declared_windows_share_browser_arguments() {
        let config: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/tauri.conf.json"))
                .expect("tauri.conf.json is readable"),
        )
        .expect("tauri.conf.json is valid JSON");

        let windows = config["app"]["windows"]
            .as_array()
            .expect("app.windows is an array");

        let mut seen: Option<&serde_json::Value> = None;
        for window in windows {
            let args = &window["additionalBrowserArgs"];
            match seen {
                None => seen = Some(args),
                Some(first) => assert_eq!(
                    first,
                    args,
                    "window {:?} declares different additionalBrowserArgs; \
                     WebView2 shares one browser process per user-data directory \
                     and rejects a webview created with a different command line",
                    window["label"]
                ),
            }
        }
    }
}
