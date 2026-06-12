import path from "node:path";

import { defineConfig } from "vitest/config";

export default defineConfig({
  resolve: {
    alias: {
      "@": path.resolve(__dirname, "./src"),
    },
  },
  test: {
    environment: "jsdom",
    include: ["tests/e2e/**/*.test.ts"],
    setupFiles: ["tests/vitest-setup.ts"],
    testTimeout: 30_000,
  },
});
