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
    expect(packageJson.scripts["dev:desktop:vec"]).toBe(
      "node scripts/with-iris-env.mjs -- node scripts/tauri-cli.mjs dev --features sqlite-vec",
    );
    expect(packageJson.scripts["dev:desktop:sign"]).toBe(
      "node scripts/with-iris-env.mjs -- node scripts/sign-dev-macos.mjs",
    );
    expect(packageJson.scripts["tauri:build:vec"]).toBe(
      "node scripts/with-iris-env.mjs -- node scripts/tauri-cli.mjs build --features sqlite-vec",
    );
    expect(launcher).toContain("OS_ACTIVITY_MODE");
    expect(launcher).toContain("disable");
    expect(launcher).toContain('process.platform === "darwin"');
    expect(launcher).toContain('args[0] === "dev"');
    expect(launcher).toContain("src-tauri/tauri.dev.conf.json");
    expect(launcher).toContain('tauriArgs.push("--config", devConfig)');
    expect(launcher).toContain('"tauri"');
  });

  it("keeps a stable macOS dev bundle identifier and explicit signing script", () => {
    const devConfig = JSON.parse(
      readFileSync("src-tauri/tauri.dev.conf.json", "utf8"),
    ) as {
      identifier: string;
      productName: string;
    };
    const signer = readFileSync("scripts/sign-dev-macos.mjs", "utf8");

    expect(devConfig.identifier).toBe("com.iris.notes.dev");
    expect(devConfig.productName).toBe("Iris Dev");
    expect(signer).toContain("IRIS_DEV_CODESIGN_IDENTITY");
    expect(signer).toContain('"codesign"');
    expect(signer).toContain('"--sign"');
  });
});
