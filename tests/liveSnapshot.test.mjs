import assert from "node:assert/strict";
import test from "node:test";
import { liveSnapshot } from "../src/lib/liveSnapshot.ts";

const settle = () => new Promise((resolve) => setImmediate(resolve));

test("a delayed snapshot cannot overwrite an event received during the read", async () => {
  let publish, resolveRead;
  const received = [];
  const stop = liveSnapshot(async (receive) => {
    publish = receive;
    return () => {};
  }, () => new Promise((resolve) => { resolveRead = resolve; }), (value) => received.push(value));
  await settle();
  publish("new");
  resolveRead("old");
  await settle();
  assert.deepEqual(received, ["new"]);
  stop();
});

test("unmount during listener setup unregisters without starting a snapshot read", async () => {
  let connected;
  let stopped = 0;
  const stop = liveSnapshot(() => new Promise((resolve) => { connected = resolve; }),
    () => assert.fail("must not read after unmount"), assert.fail);
  stop();
  connected(() => stopped++);
  await settle();
  assert.equal(stopped, 1);
});

test("an initial read is delivered and later events stop on unmount", async () => {
  let publish;
  const received = [];
  const stop = liveSnapshot(async (receive) => {
    publish = receive;
    return () => {};
  }, async () => "initial", (value) => received.push(value));
  await settle();
  publish("live");
  stop();
  publish("late");
  assert.deepEqual(received, ["initial", "live"]);
});

test("a rejected initialization is handled", async () => {
  const errors = [];
  liveSnapshot(async () => { throw new Error("Disconnected"); }, assert.fail,
    assert.fail, (cause) => errors.push(cause.message));
  await settle();
  assert.deepEqual(errors, ["Disconnected"]);
});
