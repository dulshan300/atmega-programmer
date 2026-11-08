/// <reference types="vitest/config" />
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import { fileURLToPath, URL } from "node:url";

const host = process.env.TAURI_DEV_HOST;

export default defineConfig({
  plugins: [react(), tailwindcss()],
  resolve: {
    alias: {
      "@": fileURLToPath(new URL("./src", import.meta.url)),
      "@devices": fileURLToPath(new URL("./devices", import.meta.url)),
      "@blocks": fileURLToPath(new URL("./blocks", import.meta.url)),
      "@templates": fileURLToPath(new URL("./templates", import.meta.url)),
    },
  },
  clearScreen: false,
  // Pre-bundle dependencies of lazily loaded workspaces. Otherwise Vite finds
  // them only when a workspace is first opened, re-optimizes, and the
  // in-flight dynamic import fails ("Outdated Optimize Dep").
  optimizeDeps: {
    include: [
      "monaco-editor/editor",
      "monaco-editor/languages/definitions/cpp/register",
      "@monaco-editor/react",
      "@xyflow/react",
      "@tauri-apps/plugin-dialog",
      "@tauri-apps/api/window",
    ],
  },
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host ? { protocol: "ws", host, port: 1421 } : undefined,
    watch: { ignored: ["**/src-tauri/**", "**/crates/**", "**/target/**", "**/.tools/**"] },
  },
  envPrefix: ["VITE_", "TAURI_ENV_*"],
  build: {
    target: process.env.TAURI_ENV_PLATFORM === "windows" ? "chrome105" : "safari15",
    minify: !process.env.TAURI_ENV_DEBUG ? "oxc" : false,
    sourcemap: !!process.env.TAURI_ENV_DEBUG,
  },
  test: {
    globals: true,
    environment: "jsdom",
    setupFiles: ["./src/test/setup.ts"],
    include: ["src/**/*.test.{ts,tsx}"],
    css: false,
  },
});
