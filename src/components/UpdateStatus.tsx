import { getVersion } from "@tauri-apps/api/app";
import { useEffect, useState } from "react";
import { useUpdates } from "../lib/updates";

/** Manual updates do not opt the user into background network calls. */
export function UpdateStatus() {
  const { status, connected, check, install } = useUpdates();
  const [version, setVersion] = useState<string>();
  useEffect(() => { void getVersion().then(setVersion).catch(() => {}); }, []);
  const busy = ["checking", "downloading", "installing"].includes(status.phase);
  const message = {
    idle: "Check GitHub for a newer version.",
    checking: "Checking for updates…",
    current: "You're up to date.",
    available: `Version ${status.version} is available.`,
    downloading: status.progress === undefined ? "Downloading update…" : `Downloading update… ${status.progress}%`,
    installing: "Installing update. Token Drain will restart…",
    error: status.message,
  }[status.phase];
  return <div className="settings-update-status">
    {version && <p className="settings-hint">Installed version: {version}</p>}
    <p className="settings-hint" role="status" aria-live="polite">
      {import.meta.env.PROD ? message : "Updates are available in the installed app."}
    </p>
    {status.phase === "downloading" && <progress aria-label="Update download" max={100} value={status.progress} />}
    {status.checkedAt && <p className="settings-hint">Last checked: {new Date(status.checkedAt).toLocaleString("en")}</p>}
    {status.notes && <details className="settings-update-notes">
      <summary>What's new</summary>
      <p>{status.notes}</p>
    </details>}
    <div className="settings-choices">
      <button type="button" className="settings-reset" disabled={!connected || busy} onClick={() => void check()}>
        {status.phase === "error" ? "Try again" : "Check for updates"}
      </button>
      {status.phase === "available" && <button type="button" className="settings-reset" onClick={() => void install()}>
        Install and restart
      </button>}
    </div>
  </div>;
}
