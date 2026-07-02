import { existsSync, readdirSync, statSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const home = os.homedir();
const candidates = [
  path.join(root, ".iris-dev"),
  path.join(root, ".iris"),
  path.join(home, ".iris"),
  process.env.LOCALAPPDATA
    ? path.join(process.env.LOCALAPPDATA, "com.iris.notes")
    : null,
  process.env.APPDATA ? path.join(process.env.APPDATA, "com.iris.notes") : null,
].filter(Boolean);

function sizeOf(entry) {
  if (!existsSync(entry)) return 0;
  const stat = statSync(entry);
  if (!stat.isDirectory()) return stat.size;
  return readdirSync(entry).reduce(
    (sum, child) => sum + sizeOf(path.join(entry, child)),
    0,
  );
}

const report = candidates.map((entry) => ({
  path: entry,
  exists: existsSync(entry),
  bytes: existsSync(entry) ? sizeOf(entry) : 0,
}));

process.stdout.write(`${JSON.stringify({ root, report }, null, 2)}\n`);
