#!/usr/bin/env node
/**
 * 按对象 ID 查询 catalog、issues.state、current verify 与 open gaps。
 * 日常施工不必打开整份 registry.json。
 *
 * 用法：
 *   node scripts/agent-harness-show.mjs C25
 *   npm run agent-harness:show -- C25
 */

import path from "node:path";
import { fileURLToPath } from "node:url";

import { fileContainers, files, objects } from "../agent-harness/catalog.mjs";
import { readMergedRegistry } from "./agent-harness-registry.mjs";

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, "..");
const harnessDir = path.join(repoRoot, "agent-harness");

const RELATION_KEYS = [
  "implements",
  "consumes",
  "verifies",
  "depends_on",
  "supersedes",
  "blocks",
  "closes",
  "applies_to",
];

function loadRegistry() {
  return readMergedRegistry(harnessDir);
}

function formatValue(value) {
  if (value == null) return "—";
  if (Array.isArray(value)) {
    if (value.length === 0) return "—";
    if (typeof value[0] === "object" && value[0]?.id) {
      return value.map((item) => `${item.id}@${item.at ?? "?"}`).join(", ");
    }
    return value.join(", ");
  }
  if (typeof value === "object") {
    const compact = Object.entries(value)
      .filter(([, entry]) => entry != null && entry !== "")
      .map(([key, entry]) => `${key}=${entry}`)
      .join(", ");
    return compact || "—";
  }
  return String(value);
}

function writeBlock(title, rows) {
  process.stdout.write(`${title}\n`);
  for (const [label, value] of rows) {
    process.stdout.write(`  ${label}: ${value}\n`);
  }
}

const rawId = process.argv[2]?.trim();
if (!rawId || rawId === "--help" || rawId === "-h") {
  process.stdout.write(
    "用法: node scripts/agent-harness-show.mjs <对象ID>\n例如: npm run agent-harness:show -- C25\n",
  );
  process.exit(rawId ? 0 : 2);
}

const objectId = rawId.endsWith("#file") ? rawId.slice(0, -5) : rawId;
const catalogEntry = objects[objectId] ?? objects[rawId] ?? null;
const registry = loadRegistry();
const recorded = registry.objects?.[objectId] ?? registry.objects?.[rawId];
const issue = registry.issues?.[objectId] ?? registry.issues?.[rawId];
const fileRel =
  Object.entries(files).find(
    ([, containerId]) => containerId === objectId,
  )?.[0] ??
  fileContainers[objectId] ??
  null;
const fileRecord = fileRel ? registry.files?.[fileRel] : null;
const verifyRecords = (registry.verify ?? []).filter(
  (entry) => entry.object === objectId || entry.object === rawId,
);
const currentVerify = verifyRecords.filter(
  (entry) => entry.applicability === "current",
);
const otherVerify = verifyRecords.filter(
  (entry) => entry.applicability !== "current",
);
const gaps = (registry.gaps ?? []).filter(
  (entry) =>
    entry.object === objectId ||
    entry.object === rawId ||
    entry.object === `${objectId}#file`,
);

if (!catalogEntry && !recorded && !fileRecord && !issue) {
  process.stderr.write(`未找到对象 ${rawId}\n`);
  const needle = objectId.toLowerCase();
  const hints = Object.keys(objects)
    .filter(
      (id) =>
        id.toLowerCase().includes(needle) ||
        id.startsWith(objectId.slice(0, 2)),
    )
    .slice(0, 8);
  if (hints.length > 0) {
    process.stderr.write(`接近: ${hints.join(", ")}\n`);
  }
  process.exit(1);
}

process.stdout.write(`${objectId}\n`);

if (catalogEntry) {
  writeBlock("catalog", [
    ["kind", formatValue(catalogEntry.kind)],
    ["title", formatValue(catalogEntry.title)],
    ["owner", formatValue(catalogEntry.owner)],
    ["maturity", formatValue(catalogEntry.maturity)],
    ["work.state", formatValue(catalogEntry.work?.state)],
    ["implementation.state", formatValue(catalogEntry.implementation?.state)],
    ["verification.state", formatValue(catalogEntry.verification?.state)],
    ["definition", formatValue(catalogEntry.definition?.file)],
    ...RELATION_KEYS.filter((key) => (catalogEntry[key] ?? []).length > 0).map(
      (key) => [key, formatValue(catalogEntry[key])],
    ),
    ["note", formatValue(catalogEntry.note)],
  ]);
} else {
  process.stdout.write("catalog: （无 catalog 条目）\n");
}

writeBlock("issues", [["state", issue ? issue.state : "（非 issue）"]]);

if (recorded) {
  writeBlock("registry.object", [
    ["revision", formatValue(recorded.revision)],
    ["fingerprint", formatValue(recorded.definition?.fingerprint)],
    ["line", formatValue(recorded.definition?.line)],
    ["implementation.state", formatValue(recorded.implementation?.state)],
    ["verification.state", formatValue(recorded.verification?.state)],
    ["work.state", formatValue(recorded.work?.state)],
  ]);
} else {
  process.stdout.write("registry.object: （尚无登记对象；需 --reconcile）\n");
}

if (fileRecord) {
  writeBlock("registry.file", [
    ["path", formatValue(fileRel)],
    ["container", formatValue(fileRecord.container)],
    ["registration", formatValue(fileRecord.registration)],
    ["fingerprint", formatValue(fileRecord.fingerprint)],
  ]);
}

process.stdout.write(`verify.current (${currentVerify.length})\n`);
if (currentVerify.length === 0) {
  process.stdout.write("  —\n");
} else {
  for (const entry of currentVerify) {
    process.stdout.write(
      `  ${entry.kind}  ${entry.fingerprint ?? "无指纹"}  ${entry.testPath ?? "—"}\n`,
    );
  }
}
process.stdout.write(`verify.other (${otherVerify.length})\n`);
if (otherVerify.length > 0) {
  const byApplicability = otherVerify.reduce((counts, entry) => {
    const key = entry.applicability ?? "unknown";
    counts[key] = (counts[key] ?? 0) + 1;
    return counts;
  }, {});
  process.stdout.write(`  ${formatValue(byApplicability)}\n`);
} else {
  process.stdout.write("  —\n");
}

process.stdout.write(`gaps (${gaps.length})\n`);
if (gaps.length === 0) {
  process.stdout.write("  —\n");
} else {
  for (const gap of gaps) {
    process.stdout.write(
      `  ${gap.object}  ${gap.review ?? "无复核"}  ${gap.reason}\n`,
    );
  }
}
