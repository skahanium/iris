import { existsSync, readdirSync, readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

function readWorkflow(path: string): string {
  return readFileSync(path, "utf8");
}

function readPackageScripts(): Record<string, string> {
  const manifest: unknown = JSON.parse(readFileSync("package.json", "utf8"));
  if (typeof manifest !== "object" || manifest === null) {
    return {};
  }
  const scripts = (manifest as { scripts?: unknown }).scripts;
  if (typeof scripts !== "object" || scripts === null) {
    return {};
  }
  return Object.fromEntries(
    Object.entries(scripts as Record<string, unknown>).filter(
      (entry): entry is [string, string] => typeof entry[1] === "string",
    ),
  );
}

function workflowJobBlocks(workflow: string): string[] {
  const jobs = workflow.slice(workflow.indexOf("jobs:"));
  return jobs.split(/(?=^  [\w-]+:\s*$)/m);
}

function runBlocks(job: string): string[] {
  const lines = job.split("\n");
  const blocks: string[] = [];

  for (let index = 0; index < lines.length; index += 1) {
    const currentLine = lines[index];
    if (currentLine === undefined) continue;
    const run = currentLine.match(/^(\s*)(?:-\s+)?run:\s*(.*)$/);
    if (!run) continue;

    const indent = (run[1] ?? "").length;
    const inline = run[2] ?? "";
    if (inline !== "|" && inline !== ">" && inline !== "") {
      const blockLines = [inline];
      while (
        blockLines.at(-1)?.trimEnd().endsWith("\\") &&
        index + 1 < lines.length
      ) {
        index += 1;
        const continuation = lines[index];
        if (continuation === undefined) break;
        blockLines.push(continuation);
      }
      blocks.push(blockLines.join("\n"));
      continue;
    }

    const blockLines: string[] = [];
    for (index += 1; index < lines.length; index += 1) {
      const line = lines[index];
      if (line === undefined) continue;
      if (line.trim() && line.search(/\S/) <= indent) {
        index -= 1;
        break;
      }
      blockLines.push(line);
    }
    blocks.push(blockLines.join("\n"));
  }

  return blocks.map((block) => block.replace(/\s*\\\r?\n[ \t]*/g, " "));
}

function tauriBuildCommands(command: string): string[] {
  const build =
    /\b(?:npm\s+run\s+tauri(?:\s+--)?\s+build|cargo\s+tauri\s+build|node\s+\S*tauri-cli\.mjs\s+build)\b[^\r\n]*/g;
  return Array.from(command.matchAll(build), (match) =>
    (match[0] ?? "").trim(),
  );
}

function scriptTauriBuildCommands(
  command: string,
): Array<{ final: string; report: string }> {
  const scripts = readPackageScripts();
  const npmRun = /\bnpm\s+run\s+([\w:-]+)([^\r\n]*)/g;

  return Array.from(command.matchAll(npmRun)).flatMap((match) => {
    const scriptName = match[1];
    if (!scriptName) return [];
    const script = scripts[scriptName];
    if (!script) return [];
    const invocationArgs = (match[2] ?? "").replace(/^\s*--\s*/, " ");
    return tauriBuildCommands(`${script}${invocationArgs}`).map((final) => ({
      final,
      report: (match[0] ?? "").trim(),
    }));
  });
}

function isAllowedUbuntuTauriBuild(command: string): boolean {
  return /--no-bundle\b/.test(command) && !/--bundles\b/.test(command);
}

function forbiddenUbuntuTauriBundleCommands(workflow: string): string[] {
  const ubuntuJobBlocks = workflowJobBlocks(workflow).filter((job) =>
    /^    runs-on: ubuntu-/m.test(job),
  );

  const violations = ubuntuJobBlocks
    .flatMap((job) =>
      runBlocks(job).flatMap((block) => [
        ...tauriBuildCommands(block).map((command) => ({
          final: command,
          report: command,
        })),
        ...scriptTauriBuildCommands(block),
      ]),
    )
    .filter((command) => !isAllowedUbuntuTauriBuild(command.final))
    .map((command) => command.report);

  return [...new Set(violations)];
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
    expect(workflow).toContain("node scripts/package-local.mjs mac");
    expect(workflow).not.toContain("--no-sqlite-vec");
    expect(workflow).not.toContain("--sqlite-vec");
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
    expect(workflow).toContain("Verify Windows desktop package");
    expect(workflow).toContain("Verify macOS desktop package");
    expect(workflow).toContain("scripts/verify-desktop-package.mjs");
  });

  it("gates tag packaging on complete frontend and Rust checks", () => {
    const workflow = readWorkflow(".github/workflows/package-desktop.yml");

    expect(workflow).toContain("release-quality:");
    expect(workflow).toContain("npm run version:check");
    expect(workflow).toContain("npm run docs:check");
    expect(workflow).toContain("npm run format:check");
    expect(workflow).toContain("npm run lint");
    expect(workflow).toContain("npm run typecheck");
    expect(workflow).toContain("npm run test");
    expect(workflow).toContain("cargo fmt --all -- --check");
    expect(workflow).toContain("cargo clippy --all-targets -- -D warnings");
    expect(workflow).toContain("cargo test");
    expect(workflow).toMatch(/package-windows:[\s\S]*needs: release-quality/);
    expect(workflow).toMatch(
      /package-macos-arm64:[\s\S]*needs: release-quality/,
    );
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
    expect(workflow).toContain("--clobber");
    expect(workflow).toContain("scripts/verify-updater-release.mjs");
    expect(workflow).toContain("shopt -s globstar nullglob");
    expect(workflow).toContain("release-assets/windows/**/*setup.exe");
    expect(workflow).toContain("release-assets/macos/**/*.dmg");
    expect(workflow).toContain("release-assets/macos/**/*.app.tar.gz");
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
    expect(workflow).toContain("npm run test");
    expect(workflow).toContain("cargo fmt --all -- --check");
    expect(workflow).toContain("cargo clippy --all-targets -- -D warnings");
    expect(workflow).toContain("cargo test");
    expect(workflow).not.toContain("package:local:win");
    expect(workflow).not.toContain("package:local:mac");
  });

  it("uses Ubuntu only to validate sqlite-vec and releases macOS and Windows assets only", () => {
    const ci = readWorkflow(".github/workflows/ci.yml");
    const packageWorkflow = readWorkflow(
      ".github/workflows/package-desktop.yml",
    );

    expect(ci).toContain("runs-on: ubuntu-24.04");
    expect(ci).toContain("--features sqlite-vec");
    expect(ci).toContain("sqlite_vec_v3_mirrors_all_canonical_caches");
    expect(ci).toContain(
      "semantic_search_uses_sqlite_vec_knn_and_never_skips_a_large_index",
    );
    expect(packageWorkflow).toContain("runs-on: windows-2022");
    expect(packageWorkflow).toContain("runs-on: macos-latest");
    expect(packageWorkflow).toContain("cargo test --features sqlite-vec");
    expect(packageWorkflow).not.toContain("--no-sqlite-vec");
  });

  it("permits Linux runners but forbids Linux packages and release assets in every workflow", () => {
    const workflowPaths = readdirSync(".github/workflows", {
      withFileTypes: true,
    })
      .filter((entry) => !entry.isDirectory() && entry.name.endsWith(".yml"))
      .map((entry) => `.github/workflows/${entry.name}`);
    const linuxReleasePatterns = [
      /\b(?:package|bundle)[-_: /]linux\b/i,
      /(?:release-assets|(?:assets|artifacts?)[/_-])linux\b/i,
      /\b(?:AppImage|Flatpak|Snapcraft)\b/i,
      /\.(?:deb|rpm)\b/i,
      /(?:actions\/upload-artifact|gh release (?:create|upload))[\s\S]{0,500}\b(?:linux|AppImage|Flatpak|Snapcraft)\b/i,
    ];

    expect(workflowPaths).toContain(".github/workflows/ci.yml");
    expect(workflowPaths).toContain(
      ".github/workflows/sqlite-vec-scale-ladder.yml",
    );
    expect(
      workflowPaths.some((path) => readWorkflow(path).includes("ubuntu-")),
    ).toBe(true);
    for (const workflowPath of workflowPaths) {
      const workflow = readWorkflow(workflowPath);
      for (const pattern of linuxReleasePatterns) {
        expect(
          workflow,
          `${workflowPath} must not contain ${pattern}`,
        ).not.toMatch(pattern);
      }
    }
  });

  it("forbids Ubuntu Tauri bundles but permits explicit no-bundle and non-Linux builds", () => {
    const ubuntuJob = (command: string) => `jobs:
  ubuntu-quality:
    runs-on: ubuntu-24.04
    steps:
      - run: ${command}
`;

    expect(
      forbiddenUbuntuTauriBundleCommands(ubuntuJob("npm run tauri -- build")),
    ).toEqual(["npm run tauri -- build"]);
    expect(
      forbiddenUbuntuTauriBundleCommands(ubuntuJob("cargo tauri build")),
    ).toEqual(["cargo tauri build"]);
    for (const bundle of ["deb", "rpm", "appimage"]) {
      expect(
        forbiddenUbuntuTauriBundleCommands(
          ubuntuJob(`npm run tauri -- build --bundles ${bundle}`),
        ),
      ).toEqual([`npm run tauri -- build --bundles ${bundle}`]);
    }
    expect(
      forbiddenUbuntuTauriBundleCommands(
        ubuntuJob("npm run tauri -- build --no-bundle"),
      ),
    ).toEqual([]);
    expect(
      forbiddenUbuntuTauriBundleCommands(
        ubuntuJob("npm run tauri -- build --no-bundle --bundles deb"),
      ),
    ).toEqual(["npm run tauri -- build --no-bundle --bundles deb"]);
    expect(
      forbiddenUbuntuTauriBundleCommands(ubuntuJob("npm run tauri:build:vec")),
    ).toEqual(["npm run tauri:build:vec"]);
    expect(
      forbiddenUbuntuTauriBundleCommands(
        ubuntuJob(`npm run tauri -- build --no-bundle \\
        --bundles deb`),
      ),
    ).toEqual(["npm run tauri -- build --no-bundle --bundles deb"]);
    expect(
      forbiddenUbuntuTauriBundleCommands(`jobs:
  windows-package:
    runs-on: windows-2022
    steps:
      - run: npm run tauri -- build
`),
    ).toEqual([]);
  });

  it("has no Ubuntu Tauri bundle command in any repository workflow", () => {
    const workflowPaths = readdirSync(".github/workflows", {
      withFileTypes: true,
    })
      .filter((entry) => !entry.isDirectory() && entry.name.endsWith(".yml"))
      .map((entry) => `.github/workflows/${entry.name}`);

    for (const workflowPath of workflowPaths) {
      expect(
        forbiddenUbuntuTauriBundleCommands(readWorkflow(workflowPath)),
      ).toEqual([]);
    }
  });

  it("runs the sqlite-vec scale ladder every day without publishing Linux packages", () => {
    const workflowPath = ".github/workflows/sqlite-vec-scale-ladder.yml";

    expect(existsSync(workflowPath)).toBe(true);
    const workflow = readWorkflow(workflowPath);
    expect(workflow).toContain("schedule:");
    expect(workflow).toContain('- cron: "0 19 * * *"');
    expect(workflow).toContain("runs-on: ubuntu-24.04");
    expect(workflow).toContain("--features sqlite-vec");
    expect(workflow).toContain("sqlite_vec_knn_scale_ladder");
    expect(workflow).not.toContain("actions/upload-artifact");
    expect(workflow).not.toContain("gh release");
    expect(workflow).not.toContain("package:local");
  });

  it("verifies updater assets again after a GitHub Release is published", () => {
    const workflow = readWorkflow(".github/workflows/verify-release.yml");

    expect(workflow).toContain("types: [published]");
    expect(workflow).toContain('gh release download "$TAG_NAME"');
    expect(workflow).toContain("scripts/verify-updater-release.mjs");
    expect(workflow).toContain("releases/latest/download/latest.json");
    expect(workflow).toContain("--retry-all-errors");
    expect(workflow).toContain("Compare-Object");
  });
});
