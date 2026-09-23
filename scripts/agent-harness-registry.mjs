/**
 * 登记表工作副本与历史流水的拆分／合并。
 *
 * `registry.json` 只保留当前快照；`changes`／`reviews`／非 current 的 verify
 * 写在 `registry-history.json`。流水文件不进 `catalog.files`，避免每次
 * reconcile 打漂容器指纹。检查器与查询脚本都通过本模块读合并视图。
 */

import { existsSync, readFileSync, writeFileSync } from "node:fs";
import path from "node:path";

export const SNAPSHOT_SCHEMA = "iris-agent-harness-registry-v1";
export const HISTORY_SCHEMA = "iris-agent-harness-registry-history-v1";
export const HISTORY_FILENAME = "registry-history.json";
export const SNAPSHOT_FILENAME = "registry.json";
export const SNAPSHOT_MARKER =
  "<!-- iris:object FILE-REGISTRY kind=rules file=true -->";

const SNAPSHOT_KEYS = [
  "schemaVersion",
  "registryRevision",
  "baseline",
  "sources",
  "files",
  "objects",
  "verify",
  "issues",
  "gaps",
  "notes",
  "history",
];

const HISTORY_KEYS = ["schemaVersion", "changes", "reviews", "verify"];

const MERGED_KEYS = [
  "schemaVersion",
  "registryRevision",
  "baseline",
  "sources",
  "files",
  "objects",
  "verify",
  "changes",
  "reviews",
  "issues",
  "gaps",
  "notes",
];

/** 剥掉登记表首行标记后再 JSON.parse。 */
export function parseRegistryText(text) {
  const payload = String(text ?? "")
    .split("\n")
    .filter((line) => !line.startsWith("<!--"))
    .join("\n");
  return JSON.parse(payload);
}

function firstSentence(text) {
  const raw = String(text ?? "").trim();
  if (!raw) return "obsolete";
  const match = raw.match(/^[^。.\n]+[。.]?/);
  return (match ? match[0] : raw).trim() || "obsolete";
}

/**
 * obsolete 记录压缩为对象、种类、指纹、适用性与一行原因；不改写 fingerprint。
 * 非 obsolete 记录原样保留（例如 needs-review 仍须能阻断）。
 */
export function compressHistoricalVerify(entry) {
  if (entry?.applicability !== "obsolete") return { ...entry };
  return {
    object: entry.object,
    kind: entry.kind,
    fingerprint: entry.fingerprint,
    applicability: "obsolete",
    reason: firstSentence(entry.reason ?? entry.note),
  };
}

export function splitRegistry(merged) {
  const verify = merged.verify ?? [];
  const snapshot = {
    schemaVersion: merged.schemaVersion ?? SNAPSHOT_SCHEMA,
    registryRevision: merged.registryRevision ?? 0,
    baseline: merged.baseline ?? null,
    sources: merged.sources ?? {},
    files: merged.files ?? {},
    objects: Object.fromEntries(
      Object.keys(merged.objects ?? {})
        .sort()
        .map((id) => [id, merged.objects[id]]),
    ),
    verify: verify.filter((entry) => entry.applicability === "current"),
    issues: merged.issues ?? {},
    gaps: merged.gaps ?? [],
    notes: merged.notes ?? [],
    history: HISTORY_FILENAME,
  };
  const history = {
    schemaVersion: HISTORY_SCHEMA,
    changes: merged.changes ?? [],
    reviews: merged.reviews ?? [],
    verify: verify
      .filter((entry) => entry.applicability !== "current")
      .map(compressHistoricalVerify),
  };
  return { snapshot, history };
}

export function mergeRegistry(snapshot, history) {
  if (!history) {
    const { history: _pointer, ...rest } = snapshot ?? {};
    return rest;
  }
  const current = (snapshot.verify ?? []).filter(
    (entry) => entry.applicability === "current",
  );
  const historical = history.verify ?? [];
  const { history: _pointer, ...rest } = snapshot ?? {};
  return {
    ...rest,
    changes: history.changes ?? [],
    reviews: history.reviews ?? [],
    verify: [...current, ...historical],
  };
}

/** 已关闭 issue 不得在 catalog 残留 blocks（第一波手工清理的机械钉）。 */
export function closedIssueBlockViolations(catalog, issues) {
  const messages = [];
  for (const [id, entry] of Object.entries(catalog?.objects ?? {})) {
    if (entry?.kind !== "issue") continue;
    if (issues?.[id]?.state !== "closed") continue;
    if ((entry.blocks ?? []).length === 0) continue;
    const targets = entry.blocks
      .map((block) => block?.id ?? block)
      .filter(Boolean)
      .join("、");
    messages.push(`${id} 已关闭，catalog 不得残留 blocks（仍指向 ${targets}）`);
  }
  return messages;
}

export function unknownRegistrySections(merged) {
  const allowed = new Set([...MERGED_KEYS, "history"]);
  return Object.keys(merged ?? {}).filter(
    (key) => !allowed.has(key) && merged[key] !== undefined,
  );
}

export function snapshotPath(dir) {
  return path.join(dir, SNAPSHOT_FILENAME);
}

export function historyPath(dir) {
  return path.join(dir, HISTORY_FILENAME);
}

function isSplitSnapshot(snapshot) {
  if (!snapshot) return false;
  if (snapshot.history === HISTORY_FILENAME) return true;
  const hasWal =
    (snapshot.changes?.length ?? 0) > 0 || (snapshot.reviews?.length ?? 0) > 0;
  return !hasWal && snapshot.history != null;
}

/**
 * 读工作副本；若已拆表则合并流水。拆开的快照缺少 history 文件时抛错。
 * 未拆的单体 registry.json（仍带 changes／reviews）保持可加载。
 */
export function readMergedRegistry(dir) {
  const file = snapshotPath(dir);
  const snapshot = parseRegistryText(readFileSync(file, "utf8"));
  const histFile = historyPath(dir);
  if (existsSync(histFile)) {
    const history = parseRegistryText(readFileSync(histFile, "utf8"));
    return mergeRegistry(snapshot, history);
  }
  if (isSplitSnapshot(snapshot)) {
    throw new Error(
      `拆开的登记表缺少 ${HISTORY_FILENAME}（${dir}）：流水不在工作副本里，不能当缺失处理`,
    );
  }
  return mergeRegistry(snapshot, null);
}

function serializeSnapshot(snapshot) {
  return `${SNAPSHOT_MARKER}\n${JSON.stringify(snapshot, null, 2)}\n`;
}

function serializeHistory(history) {
  return `${JSON.stringify(history, null, 2)}\n`;
}

/** 测试与不经 Prettier 的写入；生产路径用检查器里的 formatRegistryJson。 */
export function writeMergedRegistrySync(dir, merged) {
  const unknown = unknownRegistrySections(merged);
  if (unknown.length > 0) {
    throw new Error(`writeRegistry 会丢弃以下段：${unknown.join("、")}`);
  }
  const { snapshot, history } = splitRegistry(merged);
  writeFileSync(snapshotPath(dir), serializeSnapshot(snapshot), "utf8");
  writeFileSync(historyPath(dir), serializeHistory(history), "utf8");
}

export { SNAPSHOT_KEYS, HISTORY_KEYS };
