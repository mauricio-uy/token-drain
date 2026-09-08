import assert from "node:assert/strict";
import test from "node:test";
import { formatUsd } from "../src/lib/money.ts";

test("USD amounts distinguish missing values, zero, credit and debt", () => {
  assert.equal(formatUsd(null), "—");
  assert.equal(formatUsd(Number.NaN), "—");
  assert.equal(formatUsd(0), "$0.00");
  assert.equal(formatUsd(12.5), "$12.50");
  assert.equal(formatUsd(-0.5), "-$0.50");
  assert.equal(formatUsd(12500, true), "$12.5K");
});
