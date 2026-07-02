import { existsSync, rmSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const yes = process.argv.includes("--yes");
const allowed = [
  path.join(root, ".iris-dev", "tmp"),
  path.join(root, ".iris-dev", "cache"),
];

if (!yes) {
  process.stdout.write(
    "Dry run. Re-run with --yes to remove Iris-owned temp/cache directories.\n",
  );
}

for (const entry of allowed) {
  const resolved = path.resolve(entry);
  if (!resolved.startsWith(root) || !existsSync(resolved)) continue;
  process.stdout.write(
    `${yes ? "removing" : "would remove"}: ${resolved}${os.EOL}`,
  );
  if (yes) rmSync(resolved, { recursive: true, force: true });
}
