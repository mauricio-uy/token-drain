pub mod cache;
pub mod providers;
pub mod runtime;
pub mod tray;
pub mod view;
pub mod window;

use tauri::{Emitter, Manager};

use runtime::{get_usage_snapshot, refresh_now, UsageState, USAGE_UPDATED_EVENT};
use window::interaction::{set_interactive_regions, InteractiveRegions};
use window::placement::RailSide;

/// Label of the rail window, matching `tauri.conf.json`.
const RAIL_WINDOW_LABEL: &str = "rail";

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(InteractiveRegions::default())
        .invoke_handler(tauri::generate_handler![
            set_interactive_regions,
            get_usage_snapshot,
            refresh_now
        ])
        .setup(|app| {
            if let Some(rail) = app.get_webview_window(RAIL_WINDOW_LABEL) {
                // Dock once immediately, so the window is never briefly visible
                // at the OS default position before jumping into place.
                window::dock(&rail, RailSide::default());
                window::spawn_dock_watcher(rail.clone(), RailSide::default());
                window::interaction::spawn_cursor_watcher(app.handle().clone(), rail);
            }

            let cache_directory = app.path().app_data_dir()?;
            let emitter = app.handle().clone();

            let state: std::sync::Arc<UsageState> =
                runtime::start(&cache_directory, move |views| {
                    let _ = emitter.emit(USAGE_UPDATED_EVENT, views);
                })?;

            app.manage(state);

            // After the state is managed, so the tray's Refresh item can reach
            // it the first time it is clicked.
            tray::build(app.handle())?;

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
