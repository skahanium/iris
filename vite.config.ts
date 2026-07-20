import path from "node:path";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

function manualVendorChunk(id: string): string | undefined {
  const normalized = id.replace(/\\/g, "/");
  if (!normalized.includes("/node_modules/")) return undefined;

  if (
    normalized.includes("/node_modules/react/") ||
    normalized.includes("/node_modules/react-dom/") ||
    normalized.includes("/node_modules/scheduler/")
  ) {
    return "react-vendor";
  }

  if (normalized.includes("/node_modules/@tauri-apps/")) {
    return "tauri-vendor";
  }

  if (
    normalized.includes("/node_modules/@tiptap/pm/") ||
    normalized.includes("/node_modules/prosemirror-")
  ) {
    return "prosemirror";
  }

  if (normalized.includes("/node_modules/@tiptap/")) {
    return "tiptap";
  }

  if (
    normalized.includes("/node_modules/marked/") ||
    normalized.includes("/node_modules/turndown") ||
    normalized.includes("/node_modules/dompurify/") ||
    normalized.includes("/node_modules/lowlight/") ||
    normalized.includes("/node_modules/highlight.js/")
  ) {
    return "markdown-vendor";
  }

  if (normalized.includes("/node_modules/lucide-react/")) {
    return "icons-vendor";
  }

  if (normalized.includes("/node_modules/@tanstack/react-virtual/")) {
    return "virtualization-vendor";
  }

  if (
    normalized.includes("/node_modules/@radix-ui/") ||
    normalized.includes("/node_modules/@floating-ui/") ||
    normalized.includes("/node_modules/class-variance-authority/") ||
    normalized.includes("/node_modules/clsx/") ||
    normalized.includes("/node_modules/tailwind-merge/")
  ) {
    return "ui-vendor";
  }

  return "vendor";
}

export default defineConfig({
  plugins: [react()],
  resolve: {
    alias: {
      "@": path.resolve(__dirname, "./src"),
      "lowlight/core": path.resolve(
        __dirname,
        "./node_modules/lowlight/lib/index.js",
      ),
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
          return manualVendorChunk(id);
        },
      },
    },
    chunkSizeWarningLimit: 500,
  },
});
