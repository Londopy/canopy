import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";

// set by `tauri dev` when the app runs on another device (mobile dev); unused on desktop
const host = process.env.TAURI_DEV_HOST;

export default defineConfig({
  plugins: [svelte()],
  // keep rust compile errors visible in the terminal
  clearScreen: false,
  server: {
    // tauri.conf.json's devUrl points here, so the port must not drift
    port: 1420,
    strictPort: true,
    host: host || "127.0.0.1",
    hmr: host ? { protocol: "ws", host, port: 1421 } : undefined,
    watch: { ignored: ["**/src-tauri/**", "**/crates/**", "**/target/**"] },
  },
  build: {
    outDir: "dist",
    // webview2, webkitgtk 2.40+ and safari 15+ all handle es2021
    target: "es2021",
    emptyOutDir: true,
  },
});
