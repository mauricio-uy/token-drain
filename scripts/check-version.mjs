import { readFile } from "node:fs/promises";

const sources = [
  ["package.json", async (contents) => JSON.parse(contents).version],
  ["src-tauri/Cargo.toml", async (contents) => {
    const match = contents.match(/^version\s*=\s*"([^"]+)"\s*$/m);
    if (!match) throw new Error("could not find the package version");
    return match[1];
  }],
  ["src-tauri/tauri.conf.json", async (contents) => JSON.parse(contents).version],
];

const versions = await Promise.all(sources.map(async ([path, parse]) => {
  const contents = await readFile(path, "utf8");
  return [path, await parse(contents)];
}));

const expected = versions[0][1];
const mismatches = versions.filter(([, version]) => version !== expected);

if (mismatches.length > 0) {
  console.error("Version mismatch:");
  for (const [path, version] of versions) console.error(`  ${path}: ${version}`);
  process.exitCode = 1;
} else {
  console.log(`Version ${expected} is consistent across all release manifests.`);
}
