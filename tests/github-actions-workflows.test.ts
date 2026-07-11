import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

function readWorkflow(path: string): string {
  return readFileSync(path, "utf8");
}

describe("GitHub Actions workflows", () => {
  it("keeps desktop packaging manual or tag-triggered only", () => {
    const workflow = readWorkflow(".github/workflows/package-desktop.yml");

    expect(workflow).toContain("workflow_dispatch:");
    expect(workflow).toContain("tags:");
    expect(workflow).toContain('      - "v*"');
    expect(workflow).not.toContain("branches:");
    expect(workflow).toContain("permissions:");
    expect(workflow).toContain("contents: read");
    expect(workflow).toContain("cancel-in-progress: true");
  });

  it("packages Windows NSIS and macOS arm64 artifacts through existing scripts", () => {
    const workflow = readWorkflow(".github/workflows/package-desktop.yml");

    expect(workflow).toContain("runs-on: windows-2022");
    expect(workflow).toContain("npm run package:local:win");
    expect(workflow).toContain(
      ".iris-dev/target/release/bundle/nsis/*setup.exe",
    );
    expect(workflow).toContain("runs-on: macos-latest");
    expect(workflow).toContain(
      "node scripts/package-local.mjs --no-sqlite-vec mac",
    );
    expect(workflow).toContain(".iris-dev/target/release/bundle/dmg/*.dmg");
    expect(workflow).toContain("actions/upload-artifact@v6");
    expect(workflow).not.toContain("actions/upload-artifact@v4");
    expect(workflow).not.toContain("package:local:win:vec");
    expect(workflow).not.toContain("releaseDraft");
  });

  it("caches and verifies the embedded BGE staging before desktop packaging", () => {
    const workflow = readWorkflow(".github/workflows/package-desktop.yml");

    expect(workflow).toContain("actions/cache@v5");
    expect(workflow).toContain(".iris-dev/models/bge-small-zh-v1.5");
    expect(workflow).toContain("npm run model:prepare");
  });
  it("creates a draft GitHub Release with packaged assets for v tags", () => {
    const workflow = readWorkflow(".github/workflows/package-desktop.yml");

    expect(workflow).toContain("draft-release:");
    expect(workflow).toContain("needs: [package-windows, package-macos-arm64]");
    expect(workflow).toContain("if: startsWith(github.ref, 'refs/tags/v')");
    expect(workflow).toContain("contents: write");
    expect(workflow).toContain("actions/download-artifact@v7");
    expect(workflow).toContain("name: iris-windows-x64-nsis");
    expect(workflow).toContain("name: iris-macos-arm64-dmg");
    expect(workflow).toContain("TAURI_SIGNING_PRIVATE_KEY");
    expect(workflow).toContain("latest.json");
    expect(workflow).toContain(".app.tar.gz");
    expect(workflow).toContain("*setup.exe.sig");
    expect(workflow).toContain("GH_TOKEN: ${{ secrets.GITHUB_TOKEN }}");
    expect(workflow).toContain('gh release create "$GITHUB_REF_NAME"');
    expect(workflow).toContain("--draft");
    expect(workflow).toContain("--generate-notes");
    expect(workflow).toContain("--verify-tag");
    expect(workflow).toContain("gh release upload");
    expect(workflow).not.toContain("--clobber");
    expect(workflow).not.toContain("softprops/action-gh-release");
  });

  it("fails desktop packaging early when updater signing secrets are missing", () => {
    const workflow = readWorkflow(".github/workflows/package-desktop.yml");

    expect(workflow).toContain("Verify Tauri updater signing secrets");
    expect(workflow).toContain("Missing TAURI_SIGNING_PRIVATE_KEY");
    expect(workflow).toContain("TAURI_SIGNING_PRIVATE_KEY_PASSWORD");
  });

  it("uses Node 24-compatible official actions while keeping project Node 20", () => {
    const ci = readWorkflow(".github/workflows/ci.yml");
    const packageDesktop = readWorkflow(
      ".github/workflows/package-desktop.yml",
    );
    const combined = `${ci}\n${packageDesktop}`;

    expect(combined).toContain("actions/checkout@v7");
    expect(combined).toContain("actions/setup-node@v6");
    expect(combined).toContain("actions/upload-artifact@v6");
    expect(combined).toContain("actions/download-artifact@v7");
    expect(combined).toContain("node-version: 20");
    expect(combined).not.toContain("actions/checkout@v4");
    expect(combined).not.toContain("actions/setup-node@v4");
    expect(combined).not.toContain("actions/upload-artifact@v4");
    expect(combined).not.toContain("actions/download-artifact@v4");
  });

  it("keeps lightweight CI separate from desktop packaging", () => {
    const workflow = readWorkflow(".github/workflows/ci.yml");

    expect(workflow).toContain("pull_request:");
    expect(workflow).toContain("workflow_dispatch:");
    expect(workflow).not.toContain("branches: [main]");
    expect(workflow).toContain("npm ci");
    expect(workflow).toContain("npm run version:check");
    expect(workflow).toContain("npm run format:check");
    expect(workflow).toContain("npm run lint");
    expect(workflow).toContain("npm run typecheck");
    expect(workflow).toContain(
      "npm run test -- tests/package-local-script-contract.test.ts tests/github-actions-workflows.test.ts",
    );
    expect(workflow).not.toContain("package:local:win");
    expect(workflow).not.toContain("package:local:mac");
  });
});
