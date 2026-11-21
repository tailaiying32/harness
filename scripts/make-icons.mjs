// Makes the icon files the build ships (src-tauri/icons) from the drawing in
// src/lib/appIcon.ts. Run it after changing the drawing: `npm run icons`.
// Needs Node 24, which loads the TypeScript file as it is.

import { execFileSync } from "node:child_process";
import { cpSync, mkdtempSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { iconSvg } from "../src/lib/appIcon.ts";

// The icon the app ships with is Harness Dark's (see themes.ts).
const colours = { bg: "#111210", accent: "#d9a35b", faint: "#4d4b43" };

const root = fileURLToPath(new URL("..", import.meta.url));
const icons = join(root, "src-tauri", "icons");
const work = mkdtempSync(join(tmpdir(), "app-icon-"));
const tauri = join(root, "node_modules", ".bin", "tauri");

function make(name, options, out) {
  const svg = join(work, `${name}.svg`);
  writeFileSync(svg, iconSvg(colours, options));
  execFileSync(tauri, ["icon", svg, "--output", out], { cwd: root, stdio: "inherit" });
  // This app has no phone build, and nothing uses the 64px picture.
  for (const extra of ["android", "ios", "64x64.png"]) rmSync(join(out, extra), { recursive: true, force: true });
}

// A Mac's icon keeps the margin every Dock icon has.
make("mac", {}, icons);
// Windows icons fill their square.
const windows = join(work, "windows");
make("windows", { bleed: true }, windows);
for (const file of readdirSync(windows)) {
  if (file === "icon.ico" || file.startsWith("Square") || file === "StoreLogo.png") cpSync(join(windows, file), join(icons, file));
}

rmSync(work, { recursive: true, force: true });
