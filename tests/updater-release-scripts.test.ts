import { spawnSync } from "node:child_process";
import {
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";

import { afterEach, describe, expect, it } from "vitest";

const repoRoot = process.cwd();
const buildScript = path.join(
  repoRoot,
  "scripts",
  "build-updater-manifest.mjs",
);
const verifyScript = path.join(
  repoRoot,
  "scripts",
  "verify-updater-release.mjs",
);
const tempRoots: string[] = [];

function createAssets() {
  const root = mkdtempSync(path.join(tmpdir(), "iris-updater-assets-"));
  tempRoots.push(root);
  const windows = path.join(root, "windows");
  const macos = path.join(root, "macos");
  mkdirSync(windows, { recursive: true });
  mkdirSync(macos, { recursive: true });
  writeFileSync(
    path.join(windows, "Iris_1.2.7_x64-setup.exe"),
    "installer",
    "utf8",
  );
  writeFileSync(
    path.join(windows, "Iris_1.2.7_x64-setup.exe.sig"),
    "windows-signature\n",
    "utf8",
  );
  writeFileSync(path.join(macos, "Iris_1.2.7_aarch64.dmg"), "dmg", "utf8");
  writeFileSync(path.join(macos, "Iris.app.tar.gz"), "updater", "utf8");
  writeFileSync(
    path.join(macos, "Iris.app.tar.gz.sig"),
    "mac-signature\n",
    "utf8",
  );
  return { out: path.join(root, "latest.json"), root };
}

function runScript(script: string, args: string[]) {
  return spawnSync(process.execPath, [script, ...args], {
    cwd: repoRoot,
    encoding: "utf8",
  });
}

function buildManifest(root: string, out: string) {
  return runScript(buildScript, [
    "--version",
    "v1.2.7",
    "--asset-base-url",
    "https://github.com/skahanium/iris/releases/download/v1.2.7",
    "--assets-dir",
    root,
    "--out",
    out,
  ]);
}

afterEach(() => {
  for (const root of tempRoots.splice(0)) {
    rmSync(root, { recursive: true, force: true });
  }
});

describe("updater release scripts", () => {
  it("builds the exact Tauri platform map from signed updater assets", () => {
    const fixture = createAssets();

    const result = buildManifest(fixture.root, fixture.out);

    expect(result.status, result.stderr).toBe(0);
    const manifest = JSON.parse(readFileSync(fixture.out, "utf8")) as {
      version: string;
      platforms: Record<string, { signature: string; url: string }>;
    };
    expect(manifest.version).toBe("1.2.7");
    expect(Object.keys(manifest.platforms)).toEqual([
      "darwin-aarch64",
      "windows-x86_64",
    ]);
    expect(manifest.platforms["darwin-aarch64"]).toEqual({
      signature: "mac-signature",
      url: "https://github.com/skahanium/iris/releases/download/v1.2.7/Iris.app.tar.gz",
    });
    expect(manifest.platforms["windows-x86_64"]).toEqual({
      signature: "windows-signature",
      url: "https://github.com/skahanium/iris/releases/download/v1.2.7/Iris_1.2.7_x64-setup.exe",
    });
  });

  it("rejects an empty updater signature", () => {
    const fixture = createAssets();
    writeFileSync(
      path.join(fixture.root, "macos", "Iris.app.tar.gz.sig"),
      "\n",
      "utf8",
    );

    const result = buildManifest(fixture.root, fixture.out);

    expect(result.status).not.toBe(0);
    expect(result.stderr).toContain("Updater signature is empty");
  });

  it("verifies manifest URLs and signatures against downloaded release assets", () => {
    const fixture = createAssets();
    expect(buildManifest(fixture.root, fixture.out).status).toBe(0);

    const result = runScript(verifyScript, [
      "--version",
      "v1.2.7",
      "--asset-base-url",
      "https://github.com/skahanium/iris/releases/download/v1.2.7",
      "--assets-dir",
      fixture.root,
      "--manifest",
      fixture.out,
    ]);

    expect(result.status, result.stderr).toBe(0);
    expect(result.stdout).toContain("release assets verified");

    const manifest = JSON.parse(readFileSync(fixture.out, "utf8")) as {
      platforms: Record<string, { signature: string }>;
    };
    manifest.platforms["darwin-aarch64"]!.signature = "tampered";
    writeFileSync(
      fixture.out,
      `${JSON.stringify(manifest, null, 2)}\n`,
      "utf8",
    );

    const tampered = runScript(verifyScript, [
      "--version",
      "v1.2.7",
      "--asset-base-url",
      "https://github.com/skahanium/iris/releases/download/v1.2.7",
      "--assets-dir",
      fixture.root,
      "--manifest",
      fixture.out,
    ]);
    expect(tampered.status).not.toBe(0);
    expect(tampered.stderr).toContain("signature does not match");
  });
});
