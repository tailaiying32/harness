// Sets the app's version everywhere it is written: package.json and its lock
// file, tauri.conf.json, Cargo.toml and its lock file.
//   node scripts/set-version.mjs <patch|minor|major|x.y.z>
// Prints the new version.

import { readFileSync, writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("..", import.meta.url));
const read = (file) => readFileSync(root + file, "utf8");
const write = (file, text) => writeFileSync(root + file, text);

const now = JSON.parse(read("src-tauri/tauri.conf.json")).version;
const [major, minor, patch] = now.split(".").map(Number);
const asked = process.argv[2] ?? "";
const next =
  asked === "patch" ? `${major}.${minor}.${patch + 1}`
  : asked === "minor" ? `${major}.${minor + 1}.0`
  : asked === "major" ? `${major + 1}.0.0`
  : /^\d+\.\d+\.\d+$/.test(asked) ? asked
  : null;
if (!next) {
  console.error("usage: node scripts/set-version.mjs <patch|minor|major|x.y.z>");
  process.exit(2);
}

/** Replaces the one place `from` is written in a file; stops if it isn't there exactly once. */
function swap(file, from, to) {
  const text = read(file);
  if (text.split(from).length !== 2) throw new Error(`${file}: expected one ${JSON.stringify(from)}`);
  write(file, text.replace(from, to));
}

swap("src-tauri/tauri.conf.json", `"version": "${now}"`, `"version": "${next}"`);
swap("package.json", `"version": "${now}"`, `"version": "${next}"`);
const lock = JSON.parse(read("package-lock.json"));
lock.version = next;
lock.packages[""].version = next;
write("package-lock.json", JSON.stringify(lock, null, 2) + "\n");

const crate = read("src-tauri/Cargo.toml").match(/^name = "(.+)"$/m)[1];
swap("src-tauri/Cargo.toml", `name = "${crate}"\nversion = "${now}"`, `name = "${crate}"\nversion = "${next}"`);
swap("src-tauri/Cargo.lock", `name = "${crate}"\nversion = "${now}"`, `name = "${crate}"\nversion = "${next}"`);

console.log(next);
