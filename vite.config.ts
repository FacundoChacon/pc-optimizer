import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vitest/config";

// Puerto fijo: tauri.conf.json (devUrl, devCsp) depende de localhost:1420.
export default defineConfig({
  plugins: [react(), tailwindcss()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    watch: { ignored: ["**/src-tauri/**"] },
  },
  envPrefix: ["VITE_", "TAURI_ENV_"],
  build: {
    target: "chrome120",
    // Nunca generar URIs data: (la CSP de producción usa img-src 'self').
    assetsInlineLimit: 0,
  },
  test: {
    environment: "jsdom",
    setupFiles: ["src/tests/setup.ts"],
    include: ["src/tests/**/*.test.{ts,tsx}"],
    restoreMocks: true,
  },
});
