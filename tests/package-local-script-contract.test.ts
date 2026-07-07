import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

describe("local packaging script contract", () => {
  const script = () => readFileSync("scripts/package-local.mjs", "utf8");
  const pkg = () => JSON.parse(readFileSync("package.json", "utf8"));

  it("exposes macOS and Windows self-package npm scripts", () => {
    expect(pkg().scripts).toMatchObject({
      "package:local:mac": "node scripts/package-local.mjs mac",
      "package:local:mac:check": "node scripts/package-local.mjs --check mac",
      "package:local:win": "node scripts/package-local.mjs win",
      "package:local:win:check": "node scripts/package-local.mjs --check win",
      "package:local:win:vec":
        "node scripts/package-local.mjs --sqlite-vec win",
    });
  });

  it("builds macOS through an app intermediate and creates the DMG with hdiutil", () => {
    const source = script();

    expect(source).toContain("--bundles");
    expect(source).toContain('"app"');
    expect(source).not.toMatch(/--bundles["',\s]+["']dmg["']/);
    expect(source).toContain("hdiutil");
    expect(source).toContain("create");
    expect(source).toContain("-srcfolder");
    expect(source).toContain("cpSync(appPath");
    expect(source).not.toContain("bundle_dmg.sh");
  });

  it("defaults Windows packaging away from sqlite-vec and keeps explicit vec opt-in", () => {
    const source = script();

    expect(source).toContain("sqlite-vec");
    expect(source).toContain("--sqlite-vec");
    expect(source).toContain("--no-sqlite-vec");
    expect(source).toContain('target === "win" ? false : true');
    expect(pkg().scripts["package:local:win:vec"]).toBe(
      "node scripts/package-local.mjs --sqlite-vec win",
    );
  });

  it("prints the production Trusted Types enforcement state in package output", () => {
    const source = script();

    expect(source).toContain("trustedTypesStatus");
    expect(source).toContain("require-trusted-types-for");
    expect(source).toContain("trusted-types:");
  });

  it("prepares a Windows NSIS command but only runs it on Windows", () => {
    const source = script();

    expect(source).toContain("tauri.windows.conf.json");
    expect(source).toContain("nsis");
    expect(source).toContain("process.platform");
    expect(source).toContain("win32");
  });
});
