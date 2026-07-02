import path from "node:path";
import { defineConfig } from "vitest/config";

export default defineConfig({
  resolve: {
    alias: {
      "@": path.resolve(__dirname, "./src"),
      "lowlight/core": path.resolve(
        __dirname,
        "./node_modules/lowlight/lib/index.js",
      ),
    },
  },
  test: {
    environment: "jsdom",
    include: ["tests/**/*.test.ts", "tests/**/*.test.tsx"],
    setupFiles: ["tests/vitest-setup.ts"],
  },
});
