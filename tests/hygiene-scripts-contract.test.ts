import { spawnSync } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import path from "node:path";

import { describe, expect, it } from "vitest";

const root = process.cwd();

function runNode(script: string, args: string[] = []) {
  return spawnSync(process.execPath, [script, ...args], {
    cwd: root,
    encoding: "utf8",
    env: { ...process.env, IRIS_AUTO_CLEANUP: "0" },
  });
}

describe("Iris hygiene scripts", () => {
  it("routes development caches and runtime state into .iris-dev", () => {
    const result = runNode("scripts/with-iris-env.mjs", ["--print-env"]);

    expect(result.status, result.stderr).toBe(0);
    const env = JSON.parse(result.stdout) as Record<string, string>;
    const devRoot = path.join(root, ".iris-dev");

    expect(env.IRIS_HOME).toBe(devRoot);
    expect(env.IRIS_DATA_DIR).toBe(path.join(devRoot, "app-data"));
    expect(env.IRIS_CACHE_DIR).toBe(path.join(devRoot, "cache"));
    expect(env.IRIS_TEMP_DIR).toBe(path.join(devRoot, "tmp"));
    expect(env.IRIS_GLOBAL_SKILLS_DIR).toBe(path.join(devRoot, "skills"));
    expect(env.npm_config_cache).toBe(path.join(devRoot, "cache", "npm"));
    expect(env.CARGO_TARGET_DIR).toBe(path.join(devRoot, "target"));
    expect(env.ORT_CACHE_DIR).toBe(path.join(devRoot, "cache", "ort"));
    expect(env.HF_HOME).toBe(path.join(devRoot, "cache", "huggingface"));
    expect(env.HF_HUB_CACHE).toBe(
      path.join(devRoot, "cache", "huggingface", "hub"),
    );
    expect(env.XDG_CACHE_HOME).toBe(path.join(devRoot, "cache", "xdg"));
    expect(env.TEMP).toBe(path.join(devRoot, "tmp"));
    expect(env.TMP).toBe(path.join(devRoot, "tmp"));
    expect(env.TMPDIR).toBe(path.join(devRoot, "tmp"));
  });

  it("exposes hygiene npm scripts and keeps generated artifacts ignored", () => {
    const packageJson = JSON.parse(readFileSync("package.json", "utf8")) as {
      scripts: Record<string, string>;
    };

    expect(packageJson.scripts["hygiene:scan"]).toBe(
      "node scripts/hygiene-scan.mjs",
    );
    expect(packageJson.scripts["hygiene:clean"]).toBe(
      "node scripts/hygiene-clean.mjs",
    );
    expect(packageJson.scripts["hygiene:gitignore-check"]).toBe(
      "node scripts/hygiene-gitignore-check.mjs",
    );

    expect(existsSync("scripts/hygiene-gitignore-check.mjs")).toBe(true);
    const result = runNode("scripts/hygiene-gitignore-check.mjs");
    expect(result.status, result.stderr || result.stdout).toBe(0);
  });
});
