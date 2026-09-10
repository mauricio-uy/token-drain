import assert from "node:assert/strict";
import { createHash, createPublicKey, verify } from "node:crypto";
import { readFile, stat } from "node:fs/promises";
import { basename, join, resolve } from "node:path";
import { pathToFileURL } from "node:url";

/** Reject incomplete manifests and artifacts signed with a different release key. */
export async function verifyRelease(directory, config, tag = `v${config.version}`) {
  const manifest = JSON.parse(await readFile(join(directory, "latest.json"), "utf8"));
  assert.equal(manifest.version.replace(/^v/, ""), config.version, "Update version must match the app");
  const entry = manifest.platforms?.["windows-x86_64"];
  assert.ok(entry, "Windows x64 updater entry is required");
  const url = new URL(entry.url);
  const endpoint = new URL(config.plugins.updater.endpoints[0]);
  const repository = endpoint.pathname.split("/").slice(1, 3).join("/");
  assert.equal(url.origin, "https://github.com", "Installer must be hosted on GitHub HTTPS");
  assert.ok(url.pathname.startsWith(`/${repository}/releases/download/${tag}/`), "Installer must belong to this repository and tag");
  const name = decodeURIComponent(url.pathname.split("/").at(-1));
  assert.equal(basename(name), name, "Installer must be a plain filename");
  assert.match(name, /setup\.exe$/i, "Updater must select the NSIS installer");
  assert.ok((await stat(join(directory, name))).size > 0, "Installer must not be empty");
  const signature = (await readFile(join(directory, `${name}.sig`), "utf8")).trim();
  assert.equal(entry.signature.trim(), signature, "Manifest signature must match uploaded signature");
  const packet = (encoded) => Buffer.from(Buffer.from(encoded, "base64").toString("utf8").split(/\r?\n/)[1], "base64");
  const publicKey = packet(config.plugins.updater.pubkey);
  const signed = packet(signature);
  assert.equal(publicKey.length, 42, "Invalid Minisign public key");
  assert.equal(signed.length, 74, "Invalid Minisign signature");
  assert.deepEqual(signed.subarray(2, 10), publicKey.subarray(2, 10), "Signing key must match the app's embedded public key");
  const lines = Buffer.from(signature, "base64").toString("utf8").trim().split(/\r?\n/);
  const algorithm = signed.subarray(0, 2).toString("ascii");
  assert.ok(algorithm === "ED" || algorithm === "Ed", "Unsupported Minisign algorithm");
  assert.match(lines[2], /^trusted comment: /, "Missing trusted signature comment");
  const key = createPublicKey({
    key: Buffer.concat([Buffer.from("302a300506032b6570032100", "hex"), publicKey.subarray(10)]),
    format: "der", type: "spki",
  });
  const bytes = await readFile(join(directory, name));
  const message = algorithm === "ED" ? createHash("blake2b512").update(bytes).digest() : bytes;
  assert.ok(verify(null, message, key, signed.subarray(10)), "Installer cryptographic signature verification failed");
  const commentMessage = Buffer.concat([signed.subarray(10), Buffer.from(lines[2].slice(17))]);
  assert.ok(verify(null, commentMessage, key, Buffer.from(lines[3], "base64")), "Trusted comment signature verification failed");
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  const config = JSON.parse(await readFile("src-tauri/tauri.conf.json", "utf8"));
  await verifyRelease(process.argv[2], config, process.argv[3]);
  console.log(`Release ${config.version}: NSIS installer cryptographic signature and update manifest verified.`);
}
