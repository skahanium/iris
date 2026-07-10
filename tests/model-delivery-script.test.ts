import { spawnSync } from "node:child_process";
import {
  existsSync,
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
const scriptPath = path.join(
  repoRoot,
  "scripts",
  "prepare-embedding-model.mjs",
);
const tempRoots: string[] = [];
const revision = "fcecc3c5fef6becfa2b2bdda15c1c938857be534";

const fixtureFiles = [
  {
    path: "onnx/model.onnx",
    content: "fixture-model",
    sha256: "e6954ed9cce48114b2875996c433c4ada96795c4f7b5401b3ed7578086bbe242",
  },
  {
    path: "tokenizer.json",
    content: "fixture-tokenizer",
    sha256: "0296190b7dec5529d7e17a0a21b94c2e8020dbe0419b574995fff68e53428a63",
  },
  {
    path: "config.json",
    content: "fixture-config",
    sha256: "b0d4913f313bf0e18b6ec2d81d84ff748bdd415a26cb32243978b22b248567bc",
  },
  {
    path: "special_tokens_map.json",
    content: "fixture-special-tokens",
    sha256: "853942c1aa08a6e422bd062a57f5cd051b7520c60f02990f7f41385f9cc2b7d9",
  },
  {
    path: "tokenizer_config.json",
    content: "fixture-tokenizer-config",
    sha256: "f4e5df112fe47989bb7166ea2e357b1854b34d2836fa7beb5cdfd9079b988880",
  },
];

function writeFile(root: string, relativePath: string, content: string) {
  const target = path.join(root, relativePath);
  mkdirSync(path.dirname(target), { recursive: true });
  writeFileSync(target, content, "utf8");
}

function createFixtureManifest(root: string) {
  const manifestPath = path.join(root, "model.manifest.json");
  writeFileSync(
    manifestPath,
    `${JSON.stringify(
      {
        schemaVersion: 1,
        id: "bge-small-zh-v1.5",
        repository: "Xenova/bge-small-zh-v1.5",
        revision,
        license: "MIT",
        files: fixtureFiles.map(({ path: relativePath, sha256 }) => ({
          path: relativePath,
          url: `https://huggingface.co/Xenova/bge-small-zh-v1.5/resolve/${revision}/${relativePath}`,
          sha256,
        })),
      },
      null,
      2,
    )}\n`,
    "utf8",
  );
  return manifestPath;
}

function runPrepare(manifestPath: string, outputPath: string) {
  return spawnSync(
    process.execPath,
    [
      scriptPath,
      "--offline",
      "--manifest",
      manifestPath,
      "--output",
      outputPath,
    ],
    { cwd: repoRoot, encoding: "utf8" },
  );
}

function createFixture() {
  const root = mkdtempSync(path.join(tmpdir(), "iris-model-delivery-"));
  tempRoots.push(root);
  const staging = path.join(root, "staging");
  for (const file of fixtureFiles) writeFile(staging, file.path, file.content);
  return { manifest: createFixtureManifest(root), root, staging };
}

afterEach(() => {
  for (const root of tempRoots.splice(0)) {
    rmSync(root, { recursive: true, force: true });
  }
});

describe("embedded BGE model preparation", () => {
  it("pins the bundled Xenova BGE source, license, revision, and model checksum", () => {
    const manifestPath = path.join(
      repoRoot,
      "src-tauri",
      "resources",
      "embedding-models",
      "bge-small-zh-v1.5.manifest.json",
    );

    expect(existsSync(manifestPath)).toBe(true);
    const manifest = JSON.parse(readFileSync(manifestPath, "utf8")) as {
      id: string;
      license: string;
      repository: string;
      revision: string;
      files: Array<{ path: string; sha256?: string }>;
    };
    const packageJson = JSON.parse(readFileSync("package.json", "utf8")) as {
      scripts: Record<string, string>;
    };
    const tauriConfig = JSON.parse(
      readFileSync("src-tauri/tauri.conf.json", "utf8"),
    ) as { bundle: { resources?: Record<string, string> } };
    const windowsConfig = JSON.parse(
      readFileSync("src-tauri/tauri.windows.conf.json", "utf8"),
    ) as { bundle: { resources?: Record<string, string> } };
    expect(manifest).toMatchObject({
      id: "bge-small-zh-v1.5",
      repository: "Xenova/bge-small-zh-v1.5",
      revision,
      license: "MIT",
    });
    expect(manifest.files.map((file) => file.path)).toEqual([
      "onnx/model.onnx",
      "tokenizer.json",
      "config.json",
      "special_tokens_map.json",
      "tokenizer_config.json",
    ]);
    expect(manifest.files[0]?.sha256).toBe(
      "69a0b846f4f116b5e6aabf9546ea6754d02264f3211a13a1bd69b31b8040749a",
    );
    expect(packageJson.scripts["model:prepare"]).toBe(
      "node scripts/prepare-embedding-model.mjs",
    );
    const packageLocal = readFileSync("scripts/package-local.mjs", "utf8");
    expect(packageLocal).toContain("prepare embedded BGE model");
    expect(packageLocal).toContain('"model:prepare"');
    expect(tauriConfig.bundle.resources).toBeUndefined();
    expect(windowsConfig.bundle.resources).toBeUndefined();
    expect(packageLocal).toContain("TAURI_CONFIG");
    expect(packageLocal).toContain('"../.iris-dev/models/bge-small-zh-v1.5"');
  });
  it("accepts a complete offline staging directory only when every pinned artifact matches", () => {
    const fixture = createFixture();

    const result = runPrepare(fixture.manifest, fixture.staging);

    expect(result.status, result.stderr).toBe(0);
    expect(
      readFileSync(
        path.join(fixture.staging, ".iris-model-ready.json"),
        "utf8",
      ),
    ).toContain(revision);
  });

  it("fails explicitly on a pinned artifact checksum mismatch and does not mark it ready", () => {
    const fixture = createFixture();
    writeFile(fixture.staging, "onnx/model.onnx", "tampered-model");

    const result = runPrepare(fixture.manifest, fixture.staging);

    expect(result.status).not.toBe(0);
    expect(result.stderr).toContain("SHA-256 mismatch for onnx/model.onnx");
    expect(
      existsSync(path.join(fixture.staging, ".iris-model-ready.json")),
    ).toBe(false);
  });

  it("fails explicitly when an offline staging directory is missing a required model file", () => {
    const fixture = createFixture();
    rmSync(path.join(fixture.staging, "tokenizer.json"), { force: true });

    const result = runPrepare(fixture.manifest, fixture.staging);

    expect(result.status).not.toBe(0);
    expect(result.stderr).toContain(
      "Missing required model artifact: tokenizer.json",
    );
  });
});
