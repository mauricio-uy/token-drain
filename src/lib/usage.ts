import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useEffect, useState } from "react";
import { liveSnapshot } from "./liveSnapshot";

/** Mirrors `BadgeState` in `src-tauri/src/view.rs`. */
export type BadgeState =
  | "pending"
  | "ok"
  | "stale"
  | "reauth"
  | "unavailable"
  | "error";

export type UsageWindow = {
  usedPercent: number;
  windowMinutes: number;
  /** Unix milliseconds, or null when the provider did not say. */
  resetsAt: number | null;
};

export type ProviderUsage = {
  provider: string;
  session: UsageWindow | null;
  weekly: UsageWindow | null;
  plan: string | null;
  fetchedAt: number;
};

export type Remediation = {
  message: string;
  command: string | null;
  resolvesItself: boolean;
};

export type ProviderView = {
  provider: string;
  state: BadgeState;
  /**
   * The figures to draw. Null for every failure state — a failing provider has
   * no current reading, and showing one would be inventing data.
   */
  usage: ProviderUsage | null;
  /** The last successful snapshot, whatever the current state. */
  lastKnown: ProviderUsage | null;
  remediation: Remediation | null;
};

const USAGE_UPDATED_EVENT = "usage-updated";

export async function getUsageSnapshot(): Promise<ProviderView[]> {
  return invoke<ProviderView[]>("get_usage_snapshot");
}

/** Ask for an immediate poll. The backend still enforces its minimum interval. */
export async function refreshNow(): Promise<void> {
  await invoke("refresh_now");
}

/**
 * Current usage for every provider.
 *
 * Reads the snapshot once on mount so cached figures appear immediately, then
 * follows the backend's updates. Without the initial read the rail would stay
 * empty until the first poll completed, which is exactly what the cache exists
 * to prevent.
 */
export function useUsage(): ProviderView[] {
  const [views, setViews] = useState<ProviderView[]>([]);

  useEffect(() => liveSnapshot(
    (receive) => listen<ProviderView[]>(USAGE_UPDATED_EVENT, (event) => receive(event.payload)),
    getUsageSnapshot,
    setViews,
  ), []);

  return views;
}
