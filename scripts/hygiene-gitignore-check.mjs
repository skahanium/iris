import { spawnSync } from "node:child_process";

const ignored = [
  ".iris-dev/cache/npm/_cacache/index-v5/foo",
  ".iris-dev/tmp/session.tmp",
  ".iris/app-data/iris.db",
  ".npm-cache/_logs/a.log",
  ".ort-cache/model.onnx",
  ".fastembed_cache/model.bin",
  ".hf-cache/hub/blob",
  ".cache/tool/state",
  "dist/assets/app.js",
  "target/debug/iris.exe",
  "src-tauri/target/debug/iris.exe",
  "coverage/index.html",
  "playwright-report/index.html",
  "Thumbs.db",
  ".DS_Store",
];

const tracked = [
  "src/App.impl.tsx",
  "src-tauri/src/lib.rs",
  "src-tauri/migrations/001_init.sql",
  "docs/README.md",
  "tests/hygiene-scripts-contract.test.ts",
];

function checkAll(paths) {
  const result = spawnSync("git", ["check-ignore", "--stdin"], {
    input: `${paths.join("\n")}\n`,
    encoding: "utf8",
  });
  if (result.status !== 0 && result.status !== 1) {
    throw new Error(result.stderr || "git check-ignore failed");
  }
  return new Set(result.stdout.split("\n").filter(Boolean));
}

const ignoredByGit = checkAll([...ignored, ...tracked]);

function check(path, expectedIgnored) {
  const isIgnored = ignoredByGit.has(path);
  if (isIgnored !== expectedIgnored) {
    throw new Error(
      `${path} expected ${expectedIgnored ? "ignored" : "tracked"}, got ${isIgnored ? "ignored" : "tracked"}`,
    );
  }
}

for (const item of ignored) check(item, true);
for (const item of tracked) check(item, false);
process.stdout.write("gitignore hygiene check passed\n");
