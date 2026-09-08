import assert from "node:assert/strict";
import test from "node:test";
import { latestWrite } from "../src/lib/latestWrite.ts";

const settle = () => new Promise((resolve) => setImmediate(resolve));

test("rapid edits serialize and coalesce without publishing an old reply", async () => {
  const calls = [];
  const stored = [];
  const queue = latestWrite((value) => new Promise((resolve) => {
    calls.push({ value, resolve });
  }), (value) => stored.push(value), assert.fail);

  queue(10);
  for (let value = 20; value <= 400; value += 10) queue(value);
  assert.equal(calls.length, 1);
  calls[0].resolve(10);
  await settle();
  assert.deepEqual(stored, []);
  assert.equal(calls.length, 2);
  assert.equal(calls[1].value, 400);
  calls[1].resolve(400);
  await settle();
  assert.deepEqual(stored, [400]);
});

test("failed writes do not block later edits and the backend's value wins", async () => {
  const stored = [];
  const errors = [];
  const queue = latestWrite(async (value) => {
    if (value === 1) throw new Error("Disk unavailable");
    return Math.min(value, 400);
  }, (value) => stored.push(value), (cause) => errors.push(cause.message));
  queue(1);
  await settle();
  assert.deepEqual(errors, ["Disk unavailable"]);
  queue(500);
  await settle();
  assert.deepEqual(stored, [400]);
});

test("a failed superseded write still drains the latest value", async () => {
  let reject;
  const stored = [];
  const queue = latestWrite((value) => value === false
    ? new Promise((_, fail) => { reject = fail; })
    : Promise.resolve(value), (value) => stored.push(value), assert.fail);
  queue(false);
  queue(true);
  reject(new Error("Temporary failure"));
  await settle();
  assert.deepEqual(stored, [true]);
});
