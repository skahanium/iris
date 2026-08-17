import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

describe("local packaging script contract", () => {
  const script = () => readFileSync("scripts/package-local.mjs", "utf8");
  const pkg = () => JSON.parse(readFileSync("package.json", "utf8"));

  it("provides an explicit desktop development command that prepares and enables embeddings", () => {
    const tauriCli = readFileSync("scripts/tauri-cli.mjs", "utf8");

    expect(pkg().scripts["dev:desktop:embedding"]).toBe(
      "node scripts/with-iris-env.mjs -- node scripts/tauri-cli.mjs dev --embedding",
    );
    expect(tauriCli).toContain('"--embedding"');
    expect(tauriCli).toContain("IRIS_ENABLE_EMBEDDINGS");
    expect(tauriCli).toContain("model:prepare");
  });

  it("runs a real embedding inference smoke test inside the self-contained package script", () => {
    const source = script();

    expect(source).toContain("model:prepare");
    expect(source).toContain("embedding_model_smoke");
    expect(source).toContain("--ignored");
    expect(source.indexOf("model:prepare")).toBeLessThan(
      source.indexOf("embedding_model_smoke"),
    );
  });

  it("allows CI packaging to skip the embedding smoke test explicitly", () => {
    const source = script();

    expect(source).toContain("IRIS_PACKAGE_SKIP_SMOKE");
    expect(source).toContain('!== "1"');
    expect(source).toContain("runEmbeddingAndSqliteVecSmoke()");
    expect(source).toContain("smokeStatusLabel");
  });

  it("exposes macOS and Windows self-package npm scripts", () => {
    expect(pkg().scripts).toMatchObject({
      "package:local:mac": "node scripts/package-local.mjs mac",
      "package:local:mac:check": "node scripts/package-local.mjs --check mac",
      "package:local:win": "node scripts/package-local.mjs win",
      "package:local:win:check": "node scripts/package-local.mjs --check win",
    });
    expect(pkg().scripts["package:local:win:vec"]).toBeUndefined();
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

  it("lets Tauri ad-hoc sign the macOS app before updater artifacts are created", () => {
    const source = script();

    expect(source).toContain('signingIdentity: "-"');
    expect(source).not.toContain("function signMacApp");
    expect(source).not.toContain('run("ad-hoc sign Iris.app"');
    expect(source).toContain("verify desktop package");
  });

  it("uses the default sqlite-vec feature for every desktop package without a bypass", () => {
    const source = script();
    const cargo = readFileSync("src-tauri/Cargo.toml", "utf8");

    expect(source).toContain("sqlite-vec");
    expect(source).toContain("embedding_model_smoke");
    expect(source).not.toContain("--sqlite-vec");
    expect(source).not.toContain("--no-sqlite-vec");
    expect(source).not.toContain('"enabled" : "disabled"');
    expect(cargo).toContain('default = ["sqlite-vec"]');
  });

  it("prints the production Trusted Types enforcement state in package output", () => {
    const source = script();

    expect(source).toContain("trustedTypesStatus");
    expect(source).toContain("require-trusted-types-for");
    expect(source).toContain("trusted-types:");
  });

  it("prepares a Windows NSIS command but only runs it on Windows", () => {
    const source = script();

    expect(source).toContain('"--config"');
    expect(source).toContain("nsis");
    expect(source).toContain("process.platform");
    expect(source).toContain("win32");
    expect(source).toContain("verify-desktop-package.mjs");
    expect(source).toContain("resetTargetBundle");
    expect(source).toContain('path.join(bundleRoot, "nsis")');
    expect(source).toContain('path.join(releaseRoot, "nsis")');
  });
});
