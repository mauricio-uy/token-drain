import { check } from "@tauri-apps/plugin-updater";
import { useEffect, useRef } from "react";
import { getSettings, type Settings } from "./settings";
import { liveSnapshot } from "./liveSnapshot";
import { listen } from "@tauri-apps/api/event";

const SETTINGS_UPDATED_EVENT = "settings-updated";
const UPDATE_TIMEOUT_MS = 15_000;

/**
 * Check once per rail lifetime after the user opts in to automatic updates.
 *
 * The check and download remain in the native updater rather than the WebView:
 * it obtains the package from the configured GitHub Release endpoint and
 * verifies its signature against the embedded public key before installation.
 * Development sessions never contact GitHub, and a network failure is silent so
 * an unavailable update host cannot disrupt the usage widget.
 */
export function useAutomaticUpdates(): void {
  const attempted = useRef(false);

  useEffect(() => {
    let cancelled = false;

    const checkForUpdate = async () => {
      if (attempted.current || cancelled || !import.meta.env.PROD) return;
      attempted.current = true;

      try {
        const update = await check({ timeout: UPDATE_TIMEOUT_MS });
        if (!cancelled && update) {
          await update.downloadAndInstall();
        }
      } catch {
        // The rail is useful without updates. Retry at the next app launch.
      }
    };

    return liveSnapshot(
      (receive) => listen<Settings>(SETTINGS_UPDATED_EVENT, (event) => receive(event.payload)),
      getSettings,
      (settings) => {
        if (settings.automaticUpdatesEnabled) void checkForUpdate();
      },
    );
  }, []);
}
