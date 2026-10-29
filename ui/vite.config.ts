import react from "@vitejs/plugin-react";
import { defineConfig } from "vitest/config";

// Tauri loads the dev server at a fixed port (crates/app/tauri.conf.json → build.devUrl).
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 5173,
    strictPort: true,
  },
  envPrefix: ["VITE_", "TAURI_ENV_"],
  build: {
    target: "es2022",
    // The grid library is large; the app is loaded from disk, so one chunk is fine.
    chunkSizeWarningLimit: 2000,
  },
  test: {
    include: ["src/**/*.test.ts"],
    environment: "node",
    // The palette test reads tokens.css as text; other stylesheets stay out of the tests.
    css: { include: [/tokens\.css/] },
  },
});
