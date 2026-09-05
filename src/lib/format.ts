import { useEffect, useState } from "react";

const MINUTE = 60_000;
const HOUR = 60 * MINUTE;

/**
 * Render a reset time the way a person reads a countdown.
 *
 * Under an hour the useful fact is how long is left, so it counts down. Beyond
 * that a duration stops being legible — "resets in 41 hours" takes real effort —
 * and a wall-clock time is what someone actually plans around.
 *
 * This is computed in the UI on every render rather than stored, because a
 * formatted countdown is only true at the instant it is produced. A string
 * persisted at fetch time would still say "in 51 min" an hour later.
 */
export function formatReset(resetsAt: number | null, now: number): string | null {
  if (resetsAt === null) return null;

  const remaining = resetsAt - now;

  if (remaining <= 0) return "Resetting";

  if (remaining < HOUR) {
    const minutes = Math.max(1, Math.round(remaining / MINUTE));
    return `Resets in ${minutes} min`;
  }

  const target = new Date(resetsAt);
  const time = target.toLocaleTimeString(undefined, {
    hour: "numeric",
    minute: "2-digit",
  });

  const sameDay = new Date(now).toDateString() === target.toDateString();
  if (sameDay) return `Resets ${time}`;

  const weekday = target.toLocaleDateString(undefined, { weekday: "short" });
  return `Resets ${weekday} ${time}`;
}

/**
 * A clock that ticks often enough for a minute countdown to stay honest.
 *
 * Thirty seconds, so "in 3 min" is never more than half a minute stale, and the
 * widget still spends essentially all its time idle.
 */
export function useNow(intervalMs = 30_000): number {
  const [now, setNow] = useState(() => Date.now());

  useEffect(() => {
    const timer = window.setInterval(() => setNow(Date.now()), intervalMs);
    return () => window.clearInterval(timer);
  }, [intervalMs]);

  return now;
}
