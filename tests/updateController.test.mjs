import assert from "node:assert/strict";
import test from "node:test";
import { createUpdateController } from "../src/lib/updateController.ts";

const settle = () => new Promise((resolve) => setImmediate(resolve));
function fixture(checkOverride) {
  const calls = { check: 0, download: 0, install: 0, close: 0 };
  const item = {
    version: "0.2.0", body: "Release notes",
    async download(receive) {
      calls.download++;
      receive({ event: "Started", data: { contentLength: 100 } });
      receive({ event: "Progress", data: { chunkLength: 50 } });
      receive({ event: "Finished" });
    },
    async install() { calls.install++; },
    async close() { calls.close++; },
  };
  const states = [];
  const controller = createUpdateController(async () => {
    calls.check++;
    return checkOverride ? checkOverride() : item;
  }, (state) => states.push(state));
  return { controller, calls, item, states };
}

test("background updates make no requests until opted in", async () => {
  const { controller, calls } = fixture();
  await controller.check(true);
  controller.setAutomatic(false);
  assert.equal(calls.check, 0);
  controller.dispose();
});

test("manual check requires a separate installation and reports progress", async () => {
  const { controller, calls, states } = fixture();
  await controller.check();
  assert.equal(controller.snapshot().phase, "available");
  assert.equal(calls.download, 0);
  await Promise.all([controller.install(), controller.install()]);
  assert.equal(calls.install, 1);
  assert.equal(calls.close, 1);
  assert.ok(states.some((state) => state.progress === 50));
  controller.dispose();
});

test("overlapping checks share a single native request", async () => {
  let resolve;
  const { controller, calls } = fixture(() => new Promise((done) => { resolve = done; }));
  const first = controller.check();
  await settle();
  await controller.check();
  assert.equal(calls.check, 1);
  resolve(null);
  await first;
  assert.equal(controller.snapshot().phase, "current");
  controller.dispose();
});

test("opt-out during download prevents installation and releases native resources", async () => {
  const { controller, item, calls } = fixture();
  let finish;
  item.download = () => new Promise((done) => { finish = done; });
  controller.setAutomatic(true);
  await settle();
  controller.setAutomatic(false);
  finish();
  await settle();
  assert.equal(calls.install, 0);
  assert.equal(calls.close, 1);
  assert.equal(controller.snapshot().phase, "idle");
  controller.dispose();
});

test("unmount during check prevents download and closes the late native resource", async () => {
  let finish;
  const { controller, item, calls, states } = fixture(() => new Promise((done) => { finish = done; }));
  controller.setAutomatic(true);
  await settle();
  controller.dispose();
  const count = states.length;
  finish(item);
  await settle();
  assert.equal(calls.download, 0);
  assert.equal(calls.close, 1);
  assert.equal(states.length, count);
});

test("signature or download failure never installs and a later check can retry", async () => {
  const { controller, item, calls } = fixture();
  item.download = async () => { throw new Error("Signature verification failed"); };
  controller.setAutomatic(true);
  await settle();
  assert.equal(controller.snapshot().phase, "error");
  assert.equal(calls.install, 0);
  assert.equal(calls.close, 1);
  await controller.check();
  assert.equal(controller.snapshot().phase, "available");
  controller.dispose();
});

test("network failures are visible without leaking native error details", async () => {
  const { controller } = fixture(() => { throw new Error("private native details"); });
  await controller.check();
  assert.equal(controller.snapshot().phase, "error");
  assert.ok(!JSON.stringify(controller.snapshot()).includes("private native details"));
  controller.dispose();
});

test("opting in installs automatically and repeated settings events do not duplicate it", async () => {
  const { controller, calls } = fixture();
  controller.setAutomatic(true);
  controller.setAutomatic(true);
  await settle();
  assert.equal(calls.check, 1);
  assert.equal(calls.install, 1);
  assert.equal(calls.close, 1);
  controller.dispose();
});
