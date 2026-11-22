import { fileURLToPath } from "node:url";
import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";

// The website (site/): its own build, apart from the app's. The app's build
// (vite.config.ts) starts from index.html and fills dist/, which is what Tauri
// ships; nothing here adds to either. Vercel runs `npm run build:site` and
// serves site/dist (see vercel.json).

const repo = fileURLToPath(new URL(".", import.meta.url));
const site = fileURLToPath(new URL("./site", import.meta.url));

export default defineConfig({
  root: site,
  plugins: [svelte({ configFile: `${repo}/svelte.config.js` })],
  clearScreen: false,
  build: {
    outDir: "dist",
    emptyOutDir: true,
  },
  server: {
    // Not 1420: that is the app's own dev server.
    port: 1430,
    strictPort: true,
    host: "127.0.0.1",
    // The site draws on the app's own source (src/), one folder up.
    fs: { allow: [repo] },
  },
});
