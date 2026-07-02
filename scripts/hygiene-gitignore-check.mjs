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

function check(path, expectedIgnored) {
  const result = spawnSync("git", ["check-ignore", "--quiet", path], {
    encoding: "utf8",
  });
  const ignoredByGit = result.status === 0;
  if (ignoredByGit !== expectedIgnored) {
    throw new Error(
      `${path} expected ${expectedIgnored ? "ignored" : "tracked"}, got ${ignoredByGit ? "ignored" : "tracked"}`,
    );
  }
}

for (const item of ignored) check(item, true);
for (const item of tracked) check(item, false);
process.stdout.write("gitignore hygiene check passed\n");
