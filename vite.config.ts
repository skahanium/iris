import path from "node:path";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

export default defineConfig({
  plugins: [react()],
  resolve: {
    alias: {
      "@": path.resolve(__dirname, "./src"),
    },
  },
  clearScreen: false,
  server: {
    // 127.0.0.1 避免 Windows WebView2 / 系统代理下 localhost 无法加载
    host: "127.0.0.1",
    port: 1420,
    strictPort: true,
    watch: {
      ignored: [
        "**/src-tauri/**",
        "**/dist/**",
        "**/.cursor/**",
        "**/.fastembed_cache/**",
        "**/.iris/**",
        "**/coverage/**",
        "**/node_modules/**",
      ],
    },
  },
  envPrefix: ["VITE_", "TAURI_"],
  build: {
    target:
      process.env.TAURI_ENV_PLATFORM === "windows" ? "chrome105" : "safari13",
    minify: !process.env.TAURI_ENV_DEBUG ? "esbuild" : false,
    sourcemap: !!process.env.TAURI_ENV_DEBUG,
    rollupOptions: {
      output: {
        manualChunks(id) {
          if (id.includes("node_modules/@tiptap/pm/")) return "prosemirror";
          if (id.includes("node_modules/@tiptap/")) return "tiptap";
        },
      },
    },
    chunkSizeWarningLimit: 500,
  },
});
