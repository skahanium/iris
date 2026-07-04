import { existsSync } from "node:fs";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");

if (process.platform !== "darwin") {
  console.error("Dev signing is only supported on macOS.");
  process.exit(2);
}

const targetDir =
  process.env.CARGO_TARGET_DIR || path.join(root, ".iris-dev", "target");
const identity = process.env.IRIS_DEV_CODESIGN_IDENTITY || "-";
const candidates = [
  path.join(targetDir, "debug", "bundle", "macos", "Iris Dev.app"),
  path.join(targetDir, "debug", "bundle", "macos", "Iris.app"),
  path.join(targetDir, "debug", "Iris Dev.app"),
  path.join(targetDir, "debug", "iris"),
];
const targets = candidates.filter((candidate) => existsSync(candidate));

if (targets.length === 0) {
  console.error(
    `No Iris dev app or binary found under ${targetDir}. Run npm run dev:desktop once, then retry.`,
  );
  process.exit(1);
}

for (const target of targets) {
  const result = spawnSync(
    "codesign",
    ["--force", "--deep", "--sign", identity, target],
    { stdio: "inherit" },
  );

  if (result.status !== 0) {
    process.exit(result.status ?? 1);
  }
}
