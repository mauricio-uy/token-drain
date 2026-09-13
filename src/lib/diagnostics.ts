import { invoke } from "@tauri-apps/api/core";

/** Read the native package version shown in the Settings diagnostics section. */
export function getAppVersion(): Promise<string> {
  return invoke<string>("get_app_version");
}

/** Open the native diagnostics directory in Explorer. */
export function openLogDirectory(): Promise<void> {
  return invoke("open_log_directory");
}
