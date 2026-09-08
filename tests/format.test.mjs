import assert from "node:assert/strict";
import test from "node:test";
import { formatReset, formatAge } from "../src/lib/format.ts";

test("distant resets name the full calendar date", () => {
  const now = new Date(2026, 8, 8, 12).getTime();
  const reset = new Date(2026, 9, 8, 16, 5).getTime();
  assert.equal(formatReset(reset, now), "Resets 8 Oct 2026, 16:05");
  assert.equal(formatReset(Number.NaN, now), null);
});

test("reset times use English weekdays and the local clock", () => {
  const now = new Date(2026, 8, 8, 12, 0).getTime();
  const tomorrow = new Date(2026, 8, 9, 16, 5).getTime();
  assert.equal(formatReset(tomorrow, now), "Resets Wed 16:05");
  assert.equal(formatReset(new Date(2026, 8, 8, 16, 5).getTime(), now), "Resets 16:05");
});

test("unknown, elapsed and imminent resets stay distinct", () => {
  const now = new Date(2026, 8, 8, 12, 0).getTime();
  assert.equal(formatReset(null, now), null);
  assert.equal(formatReset(now, now), "Resetting");
  assert.equal(formatReset(now + 1000, now), "Resets in 1 min");
  assert.equal(formatReset(now + 30 * 60000, now), "Resets in 30 min");
  assert.equal(formatAge(now + 1000, now), "just now");
});
