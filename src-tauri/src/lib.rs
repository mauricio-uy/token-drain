pub mod cache;
pub mod providers;
pub mod view;
pub mod window;

use tauri::Manager;

use window::placement::RailSide;

/// Label of the rail window, matching `tauri.conf.json`.
const RAIL_WINDOW_LABEL: &str = "rail";

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            if let Some(rail) = app.get_webview_window(RAIL_WINDOW_LABEL) {
                // Dock once immediately, so the window is never briefly visible
                // at the OS default position before jumping into place.
                window::dock(&rail, RailSide::default());
                window::spawn_dock_watcher(rail, RailSide::default());
            }

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
