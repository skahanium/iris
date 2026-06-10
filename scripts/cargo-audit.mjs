import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import path from "node:path";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const cargoLock = path.join(root, "src-tauri", "Cargo.lock");

const args = [
  "audit",
  "--file",
  cargoLock,
  "--deny",
  "warnings",
  ...process.argv.slice(2),
];

const result = spawnSync("cargo", args, {
  cwd: root,
  stdio: "inherit",
});

process.exit(result.status ?? 1);
