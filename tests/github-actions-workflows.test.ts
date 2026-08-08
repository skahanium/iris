import { existsSync, readdirSync, readFileSync } from "node:fs";
import * as yamlPlugin from "prettier/plugins/yaml";
import { describe, expect, it } from "vitest";

function readWorkflow(path: string): string {
  return readFileSync(path, "utf8");
}

interface WorkflowDirectoryEntry {
  isDirectory(): boolean;
  name: string;
}

function workflowFilePaths(
  entries: readonly WorkflowDirectoryEntry[],
): string[] {
  return entries
    .filter((entry) => !entry.isDirectory() && /\.ya?ml$/.test(entry.name))
    .map((entry) => `.github/workflows/${entry.name}`);
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

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

type PrettierYamlPlugin = typeof yamlPlugin & {
  __parsePrettierYamlConfig(source: string): unknown;
};

function runnerMayUseUbuntu(runner: unknown): boolean {
  if (Array.isArray(runner)) {
    return runner.some((value) => runnerMayUseUbuntu(value));
  }
  if (typeof runner !== "string") return true;

  const normalized = runner.toLowerCase();
  return !(
    normalized.startsWith("windows-") || normalized.startsWith("macos-")
  );
}

function ubuntuWorkflowRunBlocks(workflow: string): string[] {
  const parsed = (yamlPlugin as PrettierYamlPlugin).__parsePrettierYamlConfig(
    workflow,
  );
  if (!isRecord(parsed) || !isRecord(parsed.jobs)) return [];

  return Object.values(parsed.jobs).flatMap((job) => {
    if (
      !isRecord(job) ||
      !runnerMayUseUbuntu(job["runs-on"]) ||
      !Array.isArray(job.steps)
    ) {
      return [];
    }

    return job.steps.flatMap((step) =>
      isRecord(step) && typeof step.run === "string" ? [step.run] : [],
    );
  });
}

const tauriCliBinary = String.raw`(?:tauri(?:\.cmd)?|(?:\S*\/)?node_modules\/\.bin\/tauri(?:\.cmd)?)`;
const tauriCliLauncher = String.raw`(?:(?:npm\s+run|cargo)\s+|(?:npx|npm\s+exec)(?:\s+(?!${tauriCliBinary}(?:\s|$))\S+)*\s+)`;
const nestedNodeTauriBuild =
  /(?:^|\s)node\s+\S*tauri-cli\.mjs(?:\s+--)?\s+build\b[^\r\n]*/g;

function shellCommandSegments(command: string): string[] {
  const shellCommand = command.replace(/\s*\\\r?\n[ \t]*/g, " ");
  const segments: string[] = [];
  let start = 0;
  let quote: '"' | "'" | undefined;

  for (let index = 0; index < shellCommand.length; index += 1) {
    const character = shellCommand[index];
    if (character === undefined) continue;
    if (character === "\\" && quote !== "'") {
      index += 1;
      continue;
    }
    if (quote) {
      if (character === quote) quote = undefined;
      continue;
    }
    if (character === '"' || character === "'") {
      quote = character;
      continue;
    }

    const nextCharacter = shellCommand[index + 1];
    const separatorLength =
      character === "\n" || character === ";" || character === "|"
        ? nextCharacter === character
          ? 2
          : 1
        : character === "&"
          ? nextCharacter === "&"
            ? 2
            : 1
          : 0;
    if (separatorLength === 0) continue;

    const segment = shellCommand.slice(start, index).trim();
    if (segment) segments.push(segment);
    index += separatorLength - 1;
    start = index + 1;
  }

  const segment = shellCommand.slice(start).trim();
  if (segment) segments.push(segment);
  return segments;
}

function tauriBuildCommands(
  command: string,
  allowNestedNodeTauriCli = false,
): string[] {
  const build = new RegExp(
    String.raw`^(?:${tauriCliLauncher})?(?:${tauriCliBinary}|node\s+\S*tauri-cli\.mjs)(?:\s+--)?\s+build\b[^\r\n]*`,
  );
  const commands = shellCommandSegments(command).flatMap((segment) => {
    const match = segment.match(build);
    return match?.[0] ? [match[0]] : [];
  });

  if (!allowNestedNodeTauriCli) return commands;
  return [
    ...new Set([
      ...commands,
      ...Array.from(command.matchAll(nestedNodeTauriBuild), (match) =>
        (match[0] ?? "").trim(),
      ),
    ]),
  ];
}

function scriptTauriBuildCommands(
  command: string,
): Array<{ final: string; report: string }> {
  const scripts = readPackageScripts();
  const npmRun = /^npm\s+run\s+([\w:-]+)(.*)$/;

  return shellCommandSegments(command).flatMap((segment) => {
    const match = segment.match(npmRun);
    if (!match) return [];
    const scriptName = match[1];
    if (!scriptName) return [];
    const script = scripts[scriptName];
    if (!script) return [];
    const invocationArgs = (match[2] ?? "").replace(/^\s*--\s*/, " ");
    return tauriBuildCommands(`${script}${invocationArgs}`, true).map(
      (final) => ({
        final,
        report: (match[0] ?? "").trim(),
      }),
    );
  });
}

function isAllowedUbuntuTauriBuild(command: string): boolean {
  return /--no-bundle\b/.test(command) && !/--bundles\b/.test(command);
}

function forbiddenUbuntuTauriBundleCommands(workflow: string): string[] {
  const violations = ubuntuWorkflowRunBlocks(workflow)
    .flatMap((block) => [
      ...tauriBuildCommands(block).map((command) => ({
        final: command,
        report: command,
      })),
      ...scriptTauriBuildCommands(block),
    ])
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

  it("makes Agent smoke, baseline and dependency audits non-bypassable quality gates", () => {
    const ci = readWorkflow(".github/workflows/ci.yml");
    const packageDesktop = readWorkflow(
      ".github/workflows/package-desktop.yml",
    );

    expect(ci).toMatch(
      /rust-tests:[\s\S]*?run: npm run agent:eval:smoke\n[\s\S]*?run: npm audit\n[\s\S]*?run: npm run audit:rust\n[\s\S]*?rag-eval:/,
    );
    expect(packageDesktop).toMatch(
      /release-quality:[\s\S]*?run: npm run agent:eval:smoke\n[\s\S]*?run: npm run agent:eval\n[\s\S]*?run: npm audit\n[\s\S]*?run: npm run audit:rust\n[\s\S]*?package-windows:/,
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

  it("scans both YAML workflow file extensions", () => {
    expect(
      workflowFilePaths([
        { name: "ci.yml", isDirectory: () => false },
        { name: "package-desktop.yaml", isDirectory: () => false },
        { name: "notes.txt", isDirectory: () => false },
        { name: "archive.yml", isDirectory: () => true },
      ]),
    ).toEqual([
      ".github/workflows/ci.yml",
      ".github/workflows/package-desktop.yaml",
    ]);
  });

  it("permits Linux runners but forbids Linux packages and release assets in every workflow", () => {
    const workflowPaths = workflowFilePaths(
      readdirSync(".github/workflows", { withFileTypes: true }),
    );
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
    const ubuntuBlockJob = (chomping: string, command: string) => `jobs:
  ubuntu-quality:
    runs-on: ubuntu-24.04
    steps:
      - run: ${chomping}
          ${command}
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
      forbiddenUbuntuTauriBundleCommands(`jobs:
  ubuntu-array:
    runs-on: [ubuntu-24.04]
    steps:
      - run: npx tauri build
`),
    ).toEqual(["npx tauri build"]);
    expect(
      forbiddenUbuntuTauriBundleCommands(`jobs:
  matrix-quality:
    strategy:
      matrix:
        os: [ubuntu-24.04, windows-2022]
    runs-on: \${{ matrix.os }}
    steps:
      - run: npx tauri build
`),
    ).toEqual(["npx tauri build"]);
    for (const command of [
      "npm run tauri -- build",
      "npm run tauri:build:vec",
      "npx tauri build",
      "npm exec tauri build",
    ]) {
      expect(forbiddenUbuntuTauriBundleCommands(ubuntuJob(command))).toEqual([
        command,
      ]);
    }
    for (const command of [
      "npm run tauri -- build --no-bundle",
      "npm run tauri:build:vec -- --no-bundle",
      "npx tauri build --no-bundle",
      "npm exec tauri build --no-bundle",
    ]) {
      expect(forbiddenUbuntuTauriBundleCommands(ubuntuJob(command))).toEqual(
        [],
      );
    }
    const semanticTauriCliVariants = [
      "./node_modules/.bin/tauri build",
      "npm exec -- tauri build",
      "npx --yes tauri build",
    ];
    expect(
      semanticTauriCliVariants.flatMap((command) =>
        forbiddenUbuntuTauriBundleCommands(ubuntuJob(command)),
      ),
    ).toEqual(semanticTauriCliVariants);
    expect(
      semanticTauriCliVariants.flatMap((command) =>
        forbiddenUbuntuTauriBundleCommands(ubuntuJob(`${command} --no-bundle`)),
      ),
    ).toEqual([]);
    expect(
      forbiddenUbuntuTauriBundleCommands(
        ubuntuBlockJob(
          ">-",
          `npx tauri build --no-bundle
          --bundles deb`,
        ),
      ),
    ).toEqual(["npx tauri build --no-bundle --bundles deb"]);
    expect(
      forbiddenUbuntuTauriBundleCommands(
        ubuntuBlockJob(
          ">-",
          `npx tauri build --no-bundle
            --bundles deb`,
        ),
      ),
    ).toEqual([]);
    expect(
      forbiddenUbuntuTauriBundleCommands(ubuntuJob('"npx tauri build"')),
    ).toEqual(["npx tauri build"]);
    expect(
      forbiddenUbuntuTauriBundleCommands(
        ubuntuJob('"npx tauri build --no-bundle"'),
      ),
    ).toEqual([]);
    expect(
      forbiddenUbuntuTauriBundleCommands(
        ubuntuBlockJob(
          "|-",
          `echo "npm run tauri:build:vec"
          # "npm run tauri:build:vec"`,
        ),
      ),
    ).toEqual([]);
    for (const ordinaryOrNonTauriCommand of [
      'echo "npx --yes tauri build"',
      'echo "npx --yes tauri build; ./node_modules/.bin/tauri build"',
      'echo "npx tauri build & npx tauri build"',
      "npm exec -- eslint build",
    ]) {
      expect(
        forbiddenUbuntuTauriBundleCommands(
          ubuntuJob(ordinaryOrNonTauriCommand),
        ),
      ).toEqual([]);
    }
    for (const chomping of ["|-", ">-", "|+"]) {
      expect(
        forbiddenUbuntuTauriBundleCommands(
          ubuntuBlockJob(chomping, "npx tauri build"),
        ),
      ).toEqual(["npx tauri build"]);
      expect(
        forbiddenUbuntuTauriBundleCommands(
          ubuntuBlockJob(chomping, "npx tauri build --no-bundle"),
        ),
      ).toEqual([]);
    }
    expect(
      forbiddenUbuntuTauriBundleCommands(
        ubuntuBlockJob(
          "|-",
          `npm run tauri -- build --no-bundle \\
          --bundles deb`,
        ),
      ),
    ).toEqual(["npm run tauri -- build --no-bundle --bundles deb"]);
    expect(
      forbiddenUbuntuTauriBundleCommands(
        ubuntuJob("npx tauri build --no-bundle & npx tauri build"),
      ),
    ).toEqual(["npx tauri build"]);
    expect(
      forbiddenUbuntuTauriBundleCommands(
        ubuntuJob("npx tauri build --no-bundle && npx tauri build"),
      ),
    ).toEqual(["npx tauri build"]);
    expect(
      forbiddenUbuntuTauriBundleCommands(`jobs:
  windows-package:
    runs-on: windows-2022
    steps:
      - run: npm run tauri -- build
`),
    ).toEqual([]);
    expect(
      forbiddenUbuntuTauriBundleCommands(`jobs:
  macos-package:
    runs-on: macos-latest
    steps:
      - run: npx tauri build
`),
    ).toEqual([]);
  });

  it("has no Ubuntu Tauri bundle command in any repository workflow", () => {
    const workflowPaths = workflowFilePaths(
      readdirSync(".github/workflows", { withFileTypes: true }),
    );

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
    expect(workflow).toContain(
      "sqlite_vec_50k_scale_fixture_meets_warm_knn_release_gate",
    );
    expect(workflow).toContain(
      "IRIS_RAG_PERFORMANCE_REFERENCE: github-hosted-ubuntu-24.04-x64",
    );
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
