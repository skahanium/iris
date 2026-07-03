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
const scriptPath = path.join(repoRoot, "scripts", "set-version.mjs");
const tempRoots: string[] = [];

function writeFixtureFile(root: string, relativePath: string, content: string) {
  const target = path.join(root, relativePath);
  mkdirSync(path.dirname(target), { recursive: true });
  writeFileSync(target, content, "utf8");
}

function readFixtureFile(root: string, relativePath: string): string {
  return readFileSync(path.join(root, relativePath), "utf8");
}

function createVersionFixture(version = "1.2.1") {
  const root = mkdtempSync(path.join(tmpdir(), "iris-version-script-"));
  tempRoots.push(root);

  writeFixtureFile(
    root,
    "package.json",
    `${JSON.stringify(
      {
        name: "iris",
        private: true,
        version,
        scripts: {},
        dependencies: { dep: "^1.1.0" },
      },
      null,
      2,
    )}\n`,
  );
  writeFixtureFile(
    root,
    "package-lock.json",
    `${JSON.stringify(
      {
        name: "iris",
        version,
        lockfileVersion: 3,
        packages: {
          "": {
            name: "iris",
            version,
            dependencies: { dep: "^1.1.0" },
          },
          "node_modules/dep": { version: "1.1.0" },
        },
      },
      null,
      2,
    )}\n`,
  );
  writeFixtureFile(
    root,
    "src-tauri/tauri.conf.json",
    `${JSON.stringify(
      {
        productName: "Iris",
        version,
        identifier: "com.iris.notes",
      },
      null,
      2,
    )}\n`,
  );
  writeFixtureFile(
    root,
    "src-tauri/Cargo.toml",
    `[package]\nname = "iris"\nversion = "${version}"\nedition = "2021"\n\n[dependencies]\ndep = { version = "1.1.0" }\n`,
  );
  writeFixtureFile(
    root,
    "src-tauri/Cargo.lock",
    `[[package]]\nname = "dep"\nversion = "1.1.0"\n\n[[package]]\nname = "iris"\nversion = "${version}"\ndependencies = ["dep"]\n\n[[package]]\nname = "other"\nversion = "${version}"\n`,
  );
  writeFixtureFile(
    root,
    "README.md",
    `# Iris\n\n**Current version**: v${version}. Current release track v${version}.\n\n| User guide | Docs site (v1.1.0 archived) |\n`,
  );
  writeFixtureFile(
    root,
    "docs/README.md",
    `# Docs\n\n## Current version\n\n**v${version}**. Old v1.1.x planning is archived.\n`,
  );
  writeFixtureFile(
    root,
    "ROADMAP.md",
    `# Roadmap\n\nIris roadmap. Current baseline is **v${version}**.\n\n## v${version} (current baseline) - done\n\n- E2E target: v1.1.0 archived.\n`,
  );
  writeFixtureFile(
    root,
    "CHANGELOG.md",
    `# Changelog\n\n## [${version}] - Current\n\nCompared with v1.1.0.\n\n### Known limitations (v1.1.0 archived)\n`,
  );
  writeFixtureFile(
    root,
    "src/components/settings/ManagementCenterPanel.tsx",
    `export const about = <>Version ${version} · GNU Affero General Public License v3.0</>;\n`,
  );
  writeFixtureFile(
    root,
    "src-tauri/src/llm/fetch_web_page.rs",
    `const USER_AGENT: &str = "Iris/${version} (+https://github.com/skahanium/iris)";\nconst API_VERSION: &str = "2025-06-18";\n`,
  );

  return root;
}

function runVersionScript(root: string, args: string[]) {
  return spawnSync(process.execPath, [scriptPath, "--root", root, ...args], {
    cwd: repoRoot,
    encoding: "utf8",
  });
}

afterEach(() => {
  for (const root of tempRoots.splice(0)) {
    rmSync(root, { recursive: true, force: true });
  }
});

describe("set-version release fact synchronizer", () => {
  it("passes check mode for the current repository version facts", () => {
    const result = spawnSync(process.execPath, [scriptPath, "--check"], {
      cwd: repoRoot,
      encoding: "utf8",
    });

    expect(result.stderr).toBe("");
    expect(result.status).toBe(0);
  });

  it("updates release fact files without touching historical or dependency versions", () => {
    const root = createVersionFixture();
    const result = runVersionScript(root, ["1.2.2"]);

    expect(result.stderr).toBe("");
    expect(result.status).toBe(0);

    const packageJson = JSON.parse(readFixtureFile(root, "package.json")) as {
      version: string;
      dependencies: Record<string, string>;
    };
    expect(packageJson.version).toBe("1.2.2");
    expect(packageJson.dependencies.dep).toBe("^1.1.0");

    const packageLock = JSON.parse(
      readFixtureFile(root, "package-lock.json"),
    ) as {
      version: string;
      packages: Record<string, { version?: string }>;
    };
    expect(packageLock.version).toBe("1.2.2");
    expect(packageLock.packages[""]?.version).toBe("1.2.2");
    expect(packageLock.packages["node_modules/dep"]?.version).toBe("1.1.0");

    expect(readFixtureFile(root, "src-tauri/Cargo.toml")).toContain(
      'version = "1.2.2"',
    );
    expect(readFixtureFile(root, "src-tauri/Cargo.toml")).toContain(
      'dep = { version = "1.1.0" }',
    );
    expect(readFixtureFile(root, "src-tauri/Cargo.lock")).toContain(
      'name = "iris"\nversion = "1.2.2"',
    );
    expect(readFixtureFile(root, "src-tauri/Cargo.lock")).toContain(
      'name = "other"\nversion = "1.2.1"',
    );

    expect(readFixtureFile(root, "src-tauri/tauri.conf.json")).toContain(
      '"version": "1.2.2"',
    );
    expect(readFixtureFile(root, "README.md")).toContain(
      "**Current version**: v1.2.2. Current release track v1.2.2.",
    );
    expect(readFixtureFile(root, "README.md")).toContain("v1.1.0 archived");
    expect(readFixtureFile(root, "docs/README.md")).toContain("**v1.2.2**");
    expect(readFixtureFile(root, "ROADMAP.md")).toContain(
      "Current baseline is **v1.2.2**.",
    );
    expect(readFixtureFile(root, "ROADMAP.md")).toContain(
      "## v1.2.2 (current baseline) - done",
    );
    expect(readFixtureFile(root, "ROADMAP.md")).toContain("v1.1.0 archived");
    expect(readFixtureFile(root, "CHANGELOG.md")).toContain(
      "## [1.2.2] - Current",
    );
    expect(readFixtureFile(root, "CHANGELOG.md")).toContain(
      "Known limitations (v1.1.0 archived)",
    );
    expect(
      readFixtureFile(
        root,
        "src/components/settings/ManagementCenterPanel.tsx",
      ),
    ).toContain("Version 1.2.2 · GNU Affero General Public License v3.0");
    expect(
      readFixtureFile(root, "src-tauri/src/llm/fetch_web_page.rs"),
    ).toContain("Iris/1.2.2");
    expect(
      readFixtureFile(root, "src-tauri/src/llm/fetch_web_page.rs"),
    ).toContain('"2025-06-18"');

    const check = runVersionScript(root, ["--check"]);
    expect(check.stderr).toBe("");
    expect(check.status).toBe(0);
  });

  it("rejects invalid explicit versions without writing files", () => {
    for (const invalidVersion of ["1.2", "v1.2.3", "1.2.3.4"]) {
      const root = createVersionFixture();
      const before = readFixtureFile(root, "package.json");
      const result = runVersionScript(root, [invalidVersion]);

      expect(result.status).not.toBe(0);
      expect(readFixtureFile(root, "package.json")).toBe(before);
    }
  });
});
