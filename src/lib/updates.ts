import { check } from "@tauri-apps/plugin-updater";
import { emitTo, listen } from "@tauri-apps/api/event";
import { useEffect, useState } from "react";
import { getSettings, type Settings } from "./settings";
import { liveSnapshot } from "./liveSnapshot";
import { createUpdateController, type UpdateStatus } from "./updateController";

const REQUEST = "update-request";
const STATUS = "update-status";
const INTERVAL_MS = 6 * 60 * 60 * 1000;
type Request = "status" | "check" | "install";

/** The rail owns updates so closing Settings cannot interrupt an installation. */
export function useAutomaticUpdates(): void {
  useEffect(() => {
    if (!import.meta.env.PROD) return;
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    const publish = (state: UpdateStatus) => {
      void emitTo("settings", STATUS, state).catch(() => {});
    };
    const controller = createUpdateController(() => check({ timeout: 15_000 }), publish);
    void listen<Request>(REQUEST, ({ payload }) => {
      if (cancelled) return;
      if (payload === "status") publish(controller.snapshot());
      if (payload === "check") void controller.check();
      if (payload === "install") void controller.install();
    }).then((stop) => {
      if (cancelled) stop();
      else { unlisten = stop; publish(controller.snapshot()); }
    }).catch(() => {});
    const stopSettings = liveSnapshot(
      (receive) => listen<Settings>("settings-updated", (event) => receive(event.payload)),
      getSettings,
      (settings) => controller.setAutomatic(settings.automaticUpdatesEnabled),
    );
    const timer = window.setInterval(() => void controller.check(true), INTERVAL_MS);
    return () => {
      cancelled = true;
      stopSettings();
      unlisten?.();
      window.clearInterval(timer);
      controller.dispose();
    };
  }, []);
}

/** Subscribe before requesting the rail's status, including active downloads. */
export function useUpdates() {
  const [status, setStatus] = useState<UpdateStatus>({ phase: "idle" });
  const [connected, setConnected] = useState(false);
  useEffect(() => {
    if (!import.meta.env.PROD) return;
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    let received = false;
    const timeout = window.setTimeout(() => {
      if (!cancelled && !received) setStatus({ phase: "error", message: "Update service is unavailable. Restart the app and try again." });
    }, 5_000);
    void listen<UpdateStatus>(STATUS, ({ payload }) => {
      if (!cancelled) { received = true; setStatus(payload); setConnected(true); }
    }).then(async (stop) => {
      if (cancelled) { stop(); return; }
      unlisten = stop;
      await emitTo("rail", REQUEST, "status");
    }).catch(() => {
      if (!cancelled) setStatus({ phase: "error", message: "Update service is unavailable. Restart the app and try again." });
    });
    return () => { cancelled = true; window.clearTimeout(timeout); unlisten?.(); };
  }, []);
  const request = async (action: Request) => {
    try { await emitTo("rail", REQUEST, action); }
    catch { setStatus({ phase: "error", message: "Update service is unavailable. Restart the app and try again." }); }
  };
  return { status, connected, check: () => request("check"), install: () => request("install") };
}
