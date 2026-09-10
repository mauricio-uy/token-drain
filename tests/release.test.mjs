import assert from "node:assert/strict";
import test from "node:test";
import { createHash, generateKeyPairSync, sign } from "node:crypto";
import { mkdtemp, readFile, writeFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { verifyRelease } from "../scripts/verify-release.mjs";

// Ephemeral test keys never sign production artifacts.
const encode = (length, keyId = 1) => {
  const packet = Buffer.alloc(length);
  packet.fill(keyId, 2, 10);
  return Buffer.from(`untrusted comment: test fixture\n${packet.toString("base64")}\n`).toString("base64");
};
async function fixture(t) {
  const directory = await mkdtemp(join(tmpdir(), "token-drain-release-test-"));
  t.after(() => rm(directory, { recursive: true, force: true }));
  const config = { version: "0.2.0", plugins: { updater: {
    pubkey: encode(42), endpoints: ["https://github.com/mauricio-uy/token-drain/releases/latest/download/latest.json"],
  } } };
  const name = "Token.Drain_0.2.0_x64-setup.exe";
  const entry = { url: `https://github.com/mauricio-uy/token-drain/releases/download/v0.2.0/${name}`, signature: encode(74) };
  const { publicKey, privateKey } = generateKeyPairSync("ed25519");
  const keyId = Buffer.alloc(8, 1);
  const publicPacket = Buffer.concat([Buffer.from("Ed"), keyId, publicKey.export({ format: "der", type: "spki" }).subarray(-32)]);
  config.plugins.updater.pubkey = Buffer.from(`untrusted comment: fixture\n${publicPacket.toString("base64")}\n`).toString("base64");
  const digest = createHash("blake2b512").update("synthetic installer fixture").digest();
  const signature = sign(null, digest, privateKey);
  const packet = Buffer.concat([Buffer.from("ED"), keyId, signature]);
  const comment = "test fixture";
  const globalSignature = sign(null, Buffer.concat([signature, Buffer.from(comment)]), privateKey);
  entry.signature = Buffer.from(`untrusted comment: fixture\n${packet.toString("base64")}\ntrusted comment: ${comment}\n${globalSignature.toString("base64")}\n`).toString("base64");
  const manifest = { version: "0.2.0", platforms: { "windows-x86_64": entry } };
  const save = () => writeFile(join(directory, "latest.json"), JSON.stringify(manifest));
  await writeFile(join(directory, name), "synthetic installer fixture");
  await writeFile(join(directory, `${name}.sig`), entry.signature);
  await save();
  return { directory, config, entry, manifest, save };
}

test("release validation accepts a complete NSIS manifest with matching key identity", async (t) => {
  const f = await fixture(t);
  await verifyRelease(f.directory, f.config);
});

test("release validation rejects another tag, host, version and signing key", async (t) => {
  const f = await fixture(t);
  const original = f.entry.url;
  for (const url of [original.replace("v0.2.0/", "v0.1.0/"), original.replace("github.com", "example.com")]) {
    f.entry.url = url;
    await f.save();
    await assert.rejects(verifyRelease(f.directory, f.config));
  }
  f.entry.url = original;
  f.manifest.version = "0.1.0";
  await f.save();
  await assert.rejects(verifyRelease(f.directory, f.config), /version/i);
  f.manifest.version = "0.2.0";
  await f.save();
  f.config.plugins.updater.pubkey = encode(42, 2);
  await assert.rejects(verifyRelease(f.directory, f.config), /Signing key/);
});

test("release validation rejects missing platforms and substituted signatures", async (t) => {
  const f = await fixture(t);
  f.entry.signature = encode(74, 2);
  await f.save();
  await assert.rejects(verifyRelease(f.directory, f.config), /signature/);
  delete f.manifest.platforms["windows-x86_64"];
  await f.save();
  await assert.rejects(verifyRelease(f.directory, f.config), /Windows x64/);
});

test("only the rail has native updater permissions", async () => {
  const shared = JSON.parse(await readFile("src-tauri/capabilities/default.json", "utf8"));
  const updater = JSON.parse(await readFile("src-tauri/capabilities/updater.json", "utf8"));
  assert.ok(!shared.permissions.some((permission) => permission.startsWith("updater:")));
  assert.deepEqual(updater.windows, ["rail"]);
  assert.ok(updater.permissions.includes("updater:default"));
});

test("release validation rejects modified installer bytes", async (t) => {
  const f = await fixture(t);
  await writeFile(join(f.directory, "Token.Drain_0.2.0_x64-setup.exe"), "tampered installer");
  await assert.rejects(verifyRelease(f.directory, f.config), /cryptographic signature/);
});
