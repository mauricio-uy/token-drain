import { useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";

const REPOSITORY_URL = "https://github.com/mauricio-uy/token-drain";

/** Open the source repository in the system browser, outside the app WebView. */
export function RepositoryLink() {
  const [failed, setFailed] = useState(false);
  return <footer className="settings-footer">
    <a className="settings-repository" href={REPOSITORY_URL} target="_blank" rel="noopener noreferrer"
      onClick={(event) => {
        event.preventDefault();
        setFailed(false);
        void openUrl(REPOSITORY_URL).catch(() => setFailed(true));
      }}>
      <svg viewBox="0 0 24 24" width="18" height="18" fill="currentColor" aria-hidden="true">
        <path d="M12 .297C5.37.297 0 5.67 0 12.297c0 5.303 3.438 9.8 8.205 11.385.6.113.82-.258.82-.577 0-.285-.01-1.04-.015-2.04-3.338.724-4.043-1.61-4.043-1.61-.546-1.387-1.333-1.756-1.333-1.756-1.09-.745.083-.729.083-.729 1.205.084 1.838 1.237 1.838 1.237 1.07 1.835 2.809 1.305 3.495.998.108-.776.418-1.305.762-1.605-2.665-.305-5.467-1.334-5.467-5.931 0-1.31.469-2.381 1.236-3.221-.124-.303-.536-1.524.117-3.176 0 0 1.008-.322 3.301 1.23A11.52 11.52 0 0 1 12 6.098c1.02.005 2.045.138 3.003.404 2.291-1.552 3.297-1.23 3.297-1.23.655 1.652.243 2.873.119 3.176.77.84 1.235 1.911 1.235 3.221 0 4.609-2.807 5.624-5.479 5.921.43.372.823 1.102.823 2.222 0 1.606-.015 2.898-.015 3.293 0 .322.216.694.825.576C20.565 22.092 24 17.592 24 12.297c0-6.627-5.373-12-12-12"/>
      </svg>
      View on GitHub <span className="settings-external" aria-hidden="true">↗</span>
      <span className="settings-sr-only"> (opens in your browser)</span>
    </a>
    {failed && <p className="settings-warning" role="alert">Could not open your browser. Try again or visit github.com/mauricio-uy/token-drain.</p>}
  </footer>;
}
