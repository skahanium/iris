import { existsSync, readdirSync, readFileSync } from "node:fs";
import * as yamlPlugin from "prettier/plugins/yaml";
import { describe, expect, it } from "vitest";

type PrettierYamlPlugin = typeof yamlPlugin & {
  __parsePrettierYamlConfig(source: string): unknown;
};

interface WorkflowDirectoryEntry {
  isDirectory(): boolean;
  name: string;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function readWorkflow(path: string): string {
  return readFileSync(path, "utf8");
}

function parseWorkflow(path: string): Record<string, unknown> {
  const parsed = (yamlPlugin as PrettierYamlPlugin).__parsePrettierYamlConfig(
    readWorkflow(path),
  );
  if (!isRecord(parsed)) throw new Error(`Invalid workflow: ${path}`);
  return parsed;
}

function workflowFilePaths(
  entries: readonly WorkflowDirectoryEntry[],
): string[] {
  return entries
    .filter((entry) => !entry.isDirectory() && /\.ya?ml$/.test(entry.name))
    .map((entry) => `.github/workflows/${entry.name}`)
    .sort();
}

function workflowJobs(path: string): Record<string, unknown> {
  const jobs = parseWorkflow(path).jobs;
  if (!isRecord(jobs)) throw new Error(`Workflow has no jobs: ${path}`);
  return jobs;
}

function workflowJob(path: string, id: string): Record<string, unknown> {
  const job = workflowJobs(path)[id];
  if (!isRecord(job)) throw new Error(`Workflow has no job ${id}: ${path}`);
  return job;
}

function jobText(path: string, id: string): string {
  return JSON.stringify(workflowJob(path, id));
}

function occurrenceCount(source: string, needle: string): number {
  return source.split(needle).length - 1;
}

function releaseAssetInputs(workflow: string): string[] {
  return Array.from(
    workflow.matchAll(
      /(?:\.iris-dev\/target\/release\/bundle\/[^\s]+|release-assets\/[^\s]+)/g,
    ),
    (match) => (match[0] ?? "").trim().replace(/[),;]+$/, ""),
  );
}

function isAllowedReleaseAssetPath(path: string): boolean {
  return [
    /^\.iris-dev\/target\/release\/bundle\/nsis\/\*setup\.exe(?:\.sig)?$/,
    /^\.iris-dev\/target\/release\/bundle\/dmg\/\*\.dmg$/,
    /^\.iris-dev\/target\/release\/bundle\/macos\/\*\.app\.tar\.gz(?:\.sig)?$/,
    /^release-assets\/windows\/\*\*\/\*setup\.exe(?:\.sig)?$/,
    /^release-assets\/macos\/\*\*\/\*\.dmg$/,
    /^release-assets\/macos\/\*\*\/\*\.app\.tar\.gz(?:\.sig)?$/,
    /^release-assets\/(?:windows|macos)$/,
    /^release-assets\/latest\.json$/,
  ].some((pattern) => pattern.test(path));
}

describe("GitHub Actions workflows", () => {
  const ciPath = ".github/workflows/ci.yml";
  const packagePath = ".github/workflows/package-desktop.yml";
  const verifyPath = ".github/workflows/verify-release.yml";

  it("contains exactly the three lightweight workflows", () => {
    expect(
      workflowFilePaths(
        readdirSync(".github/workflows", { withFileTypes: true }),
      ),
    ).toEqual([ciPath, packagePath, verifyPath]);
    expect(existsSync(".github/workflows/long-conversation-pressure.yml")).toBe(
      false,
    );
    expect(existsSync(".github/workflows/sqlite-vec-scale-ladder.yml")).toBe(
      false,
    );
  });

  it("allows only pinned macOS ARM64 and Windows x64 runners", () => {
    for (const path of [ciPath, packagePath, verifyPath]) {
      const runners = Object.values(workflowJobs(path)).map((job) =>
        isRecord(job) ? job["runs-on"] : undefined,
      );
      expect(runners.length).toBeGreaterThan(0);
      expect(
        runners.every(
          (runner) => runner === "macos-15" || runner === "windows-2022",
        ),
      ).toBe(true);
      expect(readWorkflow(path)).not.toMatch(
        /ubuntu-|linux|apt-get|appimage|flatpak|snapcraft|\.deb\b|\.rpm\b/i,
      );
      expect(readWorkflow(path)).not.toContain("matrix:");
    }
  });

  it("runs one macOS quality job for PRs and adds Windows E2E only outside PRs", () => {
    const workflow = readWorkflow(ciPath);
    const jobs = workflowJobs(ciPath);
    const quality = workflowJob(ciPath, "quality-macos-arm64");
    const windows = workflowJob(ciPath, "windows-desktop-e2e");

    expect(Object.keys(jobs)).toEqual([
      "quality-macos-arm64",
      "windows-desktop-e2e",
    ]);
    expect(workflow).toContain("pull_request:");
    expect(workflow).toContain("branches: [main]");
    expect(workflow).toContain("workflow_dispatch:");
    expect(quality.name).toBe("macOS ARM64 quality");
    expect(quality["runs-on"]).toBe("macos-15");
    expect(windows.name).toBe("Windows x64 desktop E2E");
    expect(windows.if).toBe("github.event_name != 'pull_request'");
    expect(windows["runs-on"]).toBe("windows-2022");
    expect(jobText(ciPath, "windows-desktop-e2e")).toContain(
      "npm run tauri -- build --debug --no-bundle",
    );
    expect(jobText(ciPath, "windows-desktop-e2e")).toContain(
      "npm run test:desktop:windows",
    );
  });

  it("keeps all common checks once in the macOS quality job", () => {
    const quality = jobText(ciPath, "quality-macos-arm64");
    for (const command of [
      "npm ci",
      "npm run version:check",
      "npm run docs:check",
      "npm run format:check",
      "npm run lint",
      "npm run typecheck",
      "npm run test",
      "cargo fmt --all -- --check",
      "cargo clippy --all-targets -- -D warnings",
      "cargo test",
      "npm run agent:eval:smoke",
      "npm audit",
      "npm run audit:rust",
    ]) {
      expect(quality).toContain(command);
    }
    expect(quality).not.toContain("model:prepare");
    expect(quality).not.toContain("--ignored");
    expect(readWorkflow(ciPath)).not.toContain("rag-vector-quality");
    expect(readWorkflow(ciPath)).not.toContain("rag-eval:");
  });

  it("uses the real Cargo target cache with stable platform keys", () => {
    const ci = readWorkflow(ciPath);
    const release = readWorkflow(packagePath);

    expect(`${ci}\n${release}`).not.toContain("src-tauri -> target");
    expect(ci).toMatch(
      /quality-macos-arm64:[\s\S]*?workspaces: src-tauri -> \.\.\/\.iris-dev\/target[\s\S]*?shared-key: macos-arm64/,
    );
    expect(ci).toMatch(
      /windows-desktop-e2e:[\s\S]*?workspaces: src-tauri -> \.\.\/\.iris-dev\/target[\s\S]*?shared-key: windows-x64/,
    );
    expect(release).toContain("workspaces: src-tauri -> ../.iris-dev/target");
  });

  it("validates main ancestry and a successful same-SHA main push CI run", () => {
    const workflow = readWorkflow(packagePath);
    const validation = jobText(packagePath, "validate-release-source");

    expect(workflow).toContain("actions: read");
    expect(validation).toContain("fetch-depth");
    expect(validation).toContain("git merge-base --is-ancestor");
    expect(validation).toContain("actions/workflows/ci.yml/runs");
    expect(workflow).toContain('-f head_sha="$GITHUB_SHA"');
    expect(workflow).toContain("-f branch=main");
    expect(workflow).toContain("-f event=push");
    expect(workflow).toContain("-f status=success");
    expect(validation).toContain("npm run version:check");
    expect(validation).toContain("npm run docs:check");
    expect(validation).toContain("TAURI_SIGNING_PRIVATE_KEY");
    expect(validation).toContain("TAURI_SIGNING_PRIVATE_KEY_PASSWORD");
  });

  it("starts release quality and both packages in parallel after validation", () => {
    for (const jobId of [
      "release-quality-macos-arm64",
      "package-macos-arm64",
      "package-windows-x64",
    ]) {
      expect(workflowJob(packagePath, jobId).needs).toBe(
        "validate-release-source",
      );
    }
    expect(
      workflowJob(packagePath, "release-quality-macos-arm64")["runs-on"],
    ).toBe("macos-15");
    expect(
      workflowJob(packagePath, "release-quality-macos-arm64")[
        "timeout-minutes"
      ],
    ).toBe(30);
  });

  it("runs real-model, full Agent and 50k gates exactly once in macOS release quality", () => {
    const ci = readWorkflow(ciPath);
    const release = readWorkflow(packagePath);
    const quality = jobText(packagePath, "release-quality-macos-arm64");
    const combined = `${ci}\n${release}`;

    expect(ci).not.toContain("npm run agent:eval\n");
    expect(quality).toContain("npm run agent:eval");
    expect(quality).toContain(
      "rag_v2_provisioned_sqlite_vec_model_meets_release_quality_gates",
    );
    expect(quality).toContain(
      "sqlite_vec_50k_scale_fixture_meets_warm_knn_release_gate",
    );
    expect(quality).toContain(
      'IRIS_RAG_PERFORMANCE_REFERENCE":"github-hosted-macos-15-arm64',
    );
    expect(
      occurrenceCount(
        combined,
        "rag_v2_provisioned_sqlite_vec_model_meets_release_quality_gates",
      ),
    ).toBe(1);
    expect(
      occurrenceCount(
        combined,
        "sqlite_vec_50k_scale_fixture_meets_warm_knn_release_gate",
      ),
    ).toBe(1);
  });

  it("leaves package jobs to the self-contained package scripts", () => {
    const mac = jobText(packagePath, "package-macos-arm64");
    const windows = jobText(packagePath, "package-windows-x64");

    expect(mac).toContain("node scripts/package-local.mjs mac");
    expect(windows).toContain("npm run package:local:win");
    for (const job of [mac, windows]) {
      expect(job).not.toContain("model:prepare");
      expect(job).not.toContain("embedding_model_smoke");
      expect(job).not.toContain("verify-desktop-package");
      expect(job).not.toContain("test:desktop:windows");
      expect(job).not.toContain(
        "rag_v2_provisioned_sqlite_vec_model_meets_release_quality_gates",
      );
    }
  });

  it("drafts a release only after quality and both platform packages", () => {
    const workflow = readWorkflow(packagePath);
    const draft = workflowJob(packagePath, "draft-release");

    expect(draft["runs-on"]).toBe("macos-15");
    expect(draft.needs).toEqual([
      "release-quality-macos-arm64",
      "package-macos-arm64",
      "package-windows-x64",
    ]);
    expect(draft.if).toBe("startsWith(github.ref, 'refs/tags/v')");
    expect(workflow).toContain("contents: write");
    expect(workflow).toContain("actions/download-artifact@v7");
    expect(workflow).toContain('gh release create "$GITHUB_REF_NAME"');
    expect(workflow).toContain("--draft");
    expect(workflow).toContain("--verify-tag");
    expect(workflow).toContain("gh release upload");
    expect(workflow).toContain("--clobber");
  });

  it("publishes only macOS ARM64 and Windows x64 assets", () => {
    const workflow = readWorkflow(packagePath);
    const inputs = releaseAssetInputs(workflow);

    expect(inputs.length).toBeGreaterThan(0);
    expect(inputs.filter((path) => !isAllowedReleaseAssetPath(path))).toEqual(
      [],
    );
    expect(workflow).toContain("iris-macos-arm64-dmg");
    expect(workflow).toContain("iris-windows-x64-nsis");
  });

  it("verifies published release assets and latest pointer on macOS", () => {
    const workflow = readWorkflow(verifyPath);

    expect(workflow).toContain("types: [published]");
    expect(workflowJob(verifyPath, "verify-release")["runs-on"]).toBe(
      "macos-15",
    );
    expect(workflow).toContain('gh release download "$TAG_NAME"');
    expect(workflow).toContain("scripts/verify-updater-release.mjs");
    expect(workflow).toContain("for attempt in {1..12}");
    expect(workflow).toContain("sleep 10");
    expect(workflow).toContain("cmp --silent");
    expect(workflow).not.toContain("pwsh");
    expect(workflow).not.toContain("Compare-Object");
  });

  it("uses current official action generations and project Node 20", () => {
    const combined = [ciPath, packagePath, verifyPath]
      .map(readWorkflow)
      .join("\n");

    expect(combined).toContain("actions/checkout@v7");
    expect(combined).toContain("actions/setup-node@v6");
    expect(combined).toContain("actions/cache@v5");
    expect(combined).toContain("actions/upload-artifact@v6");
    expect(combined).toContain("actions/download-artifact@v7");
    expect(combined).toContain("node-version: 20");
    expect(combined).not.toMatch(
      /actions\/(?:checkout|setup-node|upload-artifact|download-artifact)@v4/,
    );
  });
});
