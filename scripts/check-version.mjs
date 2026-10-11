#!/usr/bin/env node
// Fails when the version drifts between Cargo.toml [workspace.package], package.json files, or the
// release tag. tauri.conf.json deliberately has no `version`: Tauri reads it from Cargo.
//
// Usage: node scripts/check-version.mjs [vX.Y.Z[-N]]
//
// Tags must be `vMAJOR.MINOR.PATCH` with an optional NUMERIC-only prerelease (`v0.2.0-1`):
// WiX (.msi) rejects identifiers such as `-rc.1`, and it would only fail on the Windows leg after
// the other legs finished.
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const read = (p) => readFileSync(join(root, p), "utf8");
const errors = [];

const cargo = read("Cargo.toml");
const section = cargo.split(/^\[workspace\.package\]/m)[1]?.split(/^\[/m)[0] ?? "";
const cargoVersion = /^version\s*=\s*"([^"]+)"/m.exec(section)?.[1];
if (!cargoVersion) errors.push("Cargo.toml: [workspace.package] version not found");

const versions = { "Cargo.toml": cargoVersion };
for (const f of ["package.json", "apps/desktop/package.json", "apps/remote/package.json"]) {
  versions[f] = JSON.parse(read(f)).version;
}
const conf = JSON.parse(read("apps/desktop/src-tauri/tauri.conf.json"));
if ("version" in conf) {
  errors.push("tauri.conf.json must not set `version` (Tauri reads it from Cargo.toml)");
}

for (const [f, v] of Object.entries(versions)) {
  if (v !== cargoVersion) errors.push(`${f} has version ${v}, Cargo.toml has ${cargoVersion}`);
}
if (cargoVersion && !/^\d+\.\d+\.\d+(-\d+)?$/.test(cargoVersion)) {
  errors.push(`version ${cargoVersion} must be X.Y.Z or X.Y.Z-N (numeric prerelease, WiX)`);
}

const tag = process.argv[2];
if (tag) {
  if (!/^v\d+\.\d+\.\d+(-\d+)?$/.test(tag)) {
    errors.push(`tag ${tag} must look like v1.2.3 or v1.2.3-4 (numeric prerelease only)`);
  } else if (tag.slice(1) !== cargoVersion) {
    errors.push(`tag ${tag} does not match the project version ${cargoVersion}`);
  }
}

if (errors.length) {
  console.error(errors.map((e) => `error: ${e}`).join("\n"));
  process.exit(1);
}
console.log(`version ${cargoVersion} is consistent${tag ? ` with tag ${tag}` : ""}`);
