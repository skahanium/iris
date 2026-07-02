import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

describe("desktop dev script", () => {
  it("suppresses macOS AppKit input-method noise only for desktop dev launches", () => {
    const packageJson = JSON.parse(readFileSync("package.json", "utf8")) as {
      scripts: Record<string, string>;
    };
    const launcher = readFileSync("scripts/tauri-cli.mjs", "utf8");

    expect(packageJson.scripts["dev:desktop"]).toBe(
      "node scripts/with-iris-env.mjs -- node scripts/tauri-cli.mjs dev",
    );
    expect(packageJson.scripts.tauri).toBe(
      "node scripts/with-iris-env.mjs -- node scripts/tauri-cli.mjs",
    );
    expect(launcher).toContain("OS_ACTIVITY_MODE");
    expect(launcher).toContain("disable");
    expect(launcher).toContain('process.platform === "darwin"');
    expect(launcher).toContain('args[0] === "dev"');
    expect(launcher).toContain('"tauri"');
  });
});
