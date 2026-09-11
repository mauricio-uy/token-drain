import test from "node:test";
import assert from "node:assert/strict";
import { releaseNotes } from "../scripts/release-notes.mjs";

test("release notes select the requested version without unreleased or older changes", () => {
  const notes = releaseNotes("## [Unreleased]\nFuture work\n## [0.2.0] - 2026-09-12\nNew feature\n## [0.1.0] - 2026-09-11\nOld feature", "0.2.0");
  assert.ok(notes.includes("New feature"));
  assert.ok(!notes.includes("Future work"));
  assert.ok(!notes.includes("Old feature"));
});

test("release notes reject a missing or empty version section", () => {
  assert.throws(() => releaseNotes("## [Unreleased]\n", "0.1.0"));
  assert.throws(() => releaseNotes("## [0.1.0] - 2026-09-11\n\n", "0.1.0"));
});
