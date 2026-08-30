import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";

const host = process.env.TAURI_DEV_HOST;

export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
  },
  test: {
    include: ["src/**/*.test.{ts,tsx}", "packages/extension/src/**/*.test.ts"],
    environment: "jsdom",
    setupFiles: "./src/test/setup.ts",
    css: true,
  },
});
