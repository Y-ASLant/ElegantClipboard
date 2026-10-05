import path from "path";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

const host = process.env.TAURI_DEV_HOST;

// https://vite.dev/config/
export default defineConfig(async () => ({
  plugins: [react()],

  // Path alias
  resolve: {
    alias: {
      "@": path.resolve(import.meta.dirname, "./src"),
    },
  },

  // Build optimization
  build: {
    // WebView2 is Chromium-based — target esnext to skip all transpilation
    target: 'esnext',
    rolldownOptions: {
      input: {
        main: path.resolve(import.meta.dirname, "index.html"),
        settings: path.resolve(import.meta.dirname, "settings.html"),
      },
      output: {
        codeSplitting: {
          groups: [
            { test: /[\\/]react(-dom)?[\\/]/, name: 'vendor-react' },
            { test: /@radix-ui/, name: 'vendor-radix' },
            { test: /@dnd-kit/, name: 'vendor-dnd' },
            { test: /@tauri-apps/, name: 'vendor-tauri' },
            { test: /react-virtuoso/, name: 'vendor-virtuoso' },
          ],
        },
      },
    },
  },

  // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
  //
  // 1. prevent Vite from obscuring rust errors
  clearScreen: false,
  // 2. tauri expects a fixed port, fail if that port is not available
  server: {
    port: 14200,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      // 3. tell Vite to ignore watching `src-tauri`
      ignored: ["**/src-tauri/**"],
    },
  },
}));
