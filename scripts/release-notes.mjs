import assert from "node:assert/strict";
import { readFile, writeFile, appendFile } from "node:fs/promises";
import { randomUUID } from "node:crypto";
import { resolve } from "node:path";
import { pathToFileURL } from "node:url";

/** Release notes always describe the exact version being built. */
export function releaseNotes(changelog, version) {
  const lines = changelog.split(/\r?\n/);
  const start = lines.findIndex((line) => line.startsWith(`## [${version}] - `));
  assert.ok(start >= 0, `CHANGELOG.md must contain version ${version}`);
  const end = lines.findIndex((line, index) => index > start && line.startsWith("## "));
  const section = lines.slice(start + 1, end < 0 ? undefined : end).join("\n").trim();
  assert.ok(section, "Release notes must not be empty");
  return `${section}\n\n### Installation\n\nDownload the Windows x64 setup.exe installer below. Sign in through your provider's CLI to make its usage available.\n\nThe installer is signed for Tauri update verification, but is not Windows Authenticode signed. Windows may show a SmartScreen warning.\n`;
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  const { version } = JSON.parse(await readFile("package.json", "utf8"));
  const body = releaseNotes(await readFile("CHANGELOG.md", "utf8"), version);
  if (process.argv[2]) await writeFile(process.argv[2], body);
  else process.stdout.write(body);
  if (process.env.GITHUB_OUTPUT) {
    const delimiter = randomUUID();
    await appendFile(process.env.GITHUB_OUTPUT, `body<<${delimiter}\n${body}\n${delimiter}\n`);
  }
}
