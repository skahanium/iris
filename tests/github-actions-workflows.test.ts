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
    expect(workflow).toContain("actions/upload-artifact@v4");
    expect(workflow).not.toContain("package:local:win:vec");
    expect(workflow).not.toContain("releaseDraft");
  });

  it("keeps lightweight CI separate from desktop packaging", () => {
    const workflow = readWorkflow(".github/workflows/ci.yml");

    expect(workflow).toContain("pull_request:");
    expect(workflow).toContain("branches: [main]");
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
