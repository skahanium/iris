/** 第二波：快照与历史流水的拆分、合并、obsolete 压缩。不写仓库。 */
import { test } from "node:test";
import assert from "node:assert/strict";
import {
  mkdtempSync,
  mkdirSync,
  writeFileSync,
  readFileSync,
  rmSync,
  existsSync,
} from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { isIndependentReview } from "./agent-harness-identity.mjs";
import {
  HISTORY_SCHEMA,
  SNAPSHOT_SCHEMA,
  closedIssueBlockViolations,
  compressHistoricalVerify,
  mergeRegistry,
  parseRegistryText,
  readMergedRegistry,
  splitRegistry,
  unknownRegistrySections,
  writeMergedRegistrySync,
} from "./agent-harness-registry.mjs";

const merged = {
  schemaVersion: SNAPSHOT_SCHEMA,
  registryRevision: 2,
  baseline: { recordedAt: "2026-09-22T00:00:00Z" },
  sources: { SRC: { path: "x.md", fingerprint: "aa" } },
  files: { "registry.json": { container: "FILE-REGISTRY", fingerprint: "bb" } },
  objects: { K01: { kind: "contract" } },
  verify: [
    {
      object: "K01",
      kind: "V03",
      testPath: "a.test.mjs",
      command: "npm test",
      environment: "local",
      fingerprint: "current-fp",
      at: "2026-09-22T00:00:00Z",
      applicability: "current",
      note: "现行",
    },
    {
      object: "K01",
      kind: "V03",
      testPath: "old.test.mjs",
      command: "npm test",
      environment: "local",
      fingerprint: "old-fp",
      at: "2026-09-21T00:00:00Z",
      applicability: "obsolete",
      note: "已被取代，保留为历史证据。后续又改了一句。",
      sourceFingerprint: "deadbeef",
    },
  ],
  changes: [{ id: "CHG-1", objects: [{ id: "K01" }] }],
  reviews: [{ id: "REV-1", change: "CHG-1" }],
  issues: { Q01: { state: "open" } },
  gaps: [{ object: "V02", review: "REV-84" }],
  notes: [{ topic: "x", detail: "y" }],
};

test("splitRegistry 把 WAL 与 obsolete verify 放到 history，工作副本只留当前 verify", () => {
  const { snapshot, history } = splitRegistry(merged);
  assert.equal(snapshot.schemaVersion, SNAPSHOT_SCHEMA);
  assert.equal(history.schemaVersion, HISTORY_SCHEMA);
  assert.equal(snapshot.history, "registry-history.json");
  assert.equal(snapshot.changes, undefined);
  assert.equal(snapshot.reviews, undefined);
  assert.deepEqual(
    snapshot.verify.map((entry) => entry.applicability),
    ["current"],
  );
  assert.equal(snapshot.verify[0].testPath, "a.test.mjs");
  assert.deepEqual(history.changes, merged.changes);
  assert.deepEqual(history.reviews, merged.reviews);
  assert.equal(history.verify.length, 1);
  assert.deepEqual(history.verify[0], {
    object: "K01",
    kind: "V03",
    fingerprint: "old-fp",
    applicability: "obsolete",
    reason: "已被取代，保留为历史证据。",
  });
  assert.deepEqual(snapshot.gaps, merged.gaps);
  assert.deepEqual(snapshot.notes, merged.notes);
  assert.deepEqual(snapshot.issues, merged.issues);
});

test("mergeRegistry 能还原 changes／reviews／全部 verify，且不改写指纹", () => {
  const { snapshot, history } = splitRegistry(merged);
  const again = mergeRegistry(snapshot, history);
  assert.deepEqual(again.changes, merged.changes);
  assert.deepEqual(again.reviews, merged.reviews);
  assert.equal(again.verify.length, 2);
  assert.equal(
    again.verify.find((entry) => entry.applicability === "current").fingerprint,
    "current-fp",
  );
  assert.equal(
    again.verify.find((entry) => entry.applicability === "obsolete")
      .fingerprint,
    "old-fp",
  );
  assert.equal(again.objects.K01.kind, "contract");
});

test("没有 history 文件时，单体 registry.json 仍可当作合并结果", () => {
  const again = mergeRegistry(merged, null);
  assert.equal(again.changes.length, 1);
  assert.equal(again.reviews.length, 1);
  assert.equal(again.verify.length, 2);
});

test("parseRegistryText 剥掉首行标记", () => {
  const parsed = parseRegistryText(
    `<!-- iris:object FILE-REGISTRY kind=rules file=true -->\n{"schemaVersion":"iris-agent-harness-registry-v1"}\n`,
  );
  assert.equal(parsed.schemaVersion, SNAPSHOT_SCHEMA);
});

test("compressHistoricalVerify 不改写 fingerprint，只留一行原因", () => {
  const compressed = compressHistoricalVerify({
    object: "V02",
    kind: "V03",
    fingerprint: "keep-me",
    applicability: "obsolete",
    testPath: "drop.mjs",
    note: "第一句。\n第二句。",
  });
  assert.deepEqual(compressed, {
    object: "V02",
    kind: "V03",
    fingerprint: "keep-me",
    applicability: "obsolete",
    reason: "第一句。",
  });
});

test("未知顶层段会被 unknownRegistrySections 列出，漏段不能静默丢", () => {
  const extra = unknownRegistrySections({ ...merged, extraWal: [1] });
  assert.deepEqual(extra, ["extraWal"]);
  assert.deepEqual(unknownRegistrySections(merged), []);
});

test("writeMergedRegistrySync 写出两份文件，读回后 WAL 仍在", () => {
  const dir = mkdtempSync(path.join(tmpdir(), "registry-split-"));
  try {
    writeMergedRegistrySync(dir, merged);
    assert.equal(existsSync(path.join(dir, "registry.json")), true);
    assert.equal(existsSync(path.join(dir, "registry-history.json")), true);
    const snapshot = parseRegistryText(
      readFileSync(path.join(dir, "registry.json"), "utf8"),
    );
    assert.equal(snapshot.changes, undefined);
    const loaded = readMergedRegistry(dir);
    assert.equal(loaded.changes[0].id, "CHG-1");
    assert.equal(loaded.reviews[0].id, "REV-1");
    assert.equal(
      loaded.verify.filter((entry) => entry.applicability === "current").length,
      1,
    );
    assert.equal(
      loaded.verify.filter((entry) => entry.applicability === "obsolete")[0]
        .fingerprint,
      "old-fp",
    );
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test("closed issue 不得残留 catalog blocks", () => {
  const catalog = {
    objects: {
      Q01: { kind: "issue", blocks: [{ id: "D01", at: "acceptance" }] },
      Q02: { kind: "issue", blocks: [{ id: "D01", at: "acceptance" }] },
      D01: { kind: "work" },
    },
  };
  const mixed = closedIssueBlockViolations(catalog, {
    Q01: { state: "closed" },
    Q02: { state: "open" },
  });
  assert.equal(mixed.length, 1);
  assert.match(mixed[0], /Q01/);
  assert.equal(
    closedIssueBlockViolations(catalog, {
      Q01: { state: "open" },
      Q02: { state: "open" },
    }).length,
    0,
  );
  const cleared = {
    objects: {
      Q01: { kind: "issue" },
      Q02: { kind: "issue", blocks: [{ id: "D01", at: "acceptance" }] },
    },
  };
  assert.equal(
    closedIssueBlockViolations(cleared, { Q01: { state: "closed" } }).length,
    0,
  );
});

test("拆开的快照缺少 history 文件时 readMergedRegistry 抛错", () => {
  const dir = mkdtempSync(path.join(tmpdir(), "registry-split-missing-"));
  try {
    mkdirSync(dir, { recursive: true });
    const { snapshot } = splitRegistry(merged);
    writeFileSync(
      path.join(dir, "registry.json"),
      `<!-- iris:object FILE-REGISTRY kind=rules file=true -->\n${JSON.stringify(snapshot)}\n`,
    );
    assert.throws(() => readMergedRegistry(dir), /registry-history/);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test("复核者与编写方同填独立身份不能解封", () => {
  const change = {
    id: "CHG-1",
    author: "cursor-grok-4.6",
    objects: [{ id: "X01" }],
  };
  const payload = {
    change: "CHG-1",
    author: "mimo",
    reviewer: "mimo",
    conclusion: "synchronized",
    reason: "核验拆表与指纹",
    evidence: "docs/eval/results/example.md",
    objects: [{ id: "X01", fingerprint: "aa" }],
  };
  assert.equal(isIndependentReview(payload, change), false);
  assert.equal(
    isIndependentReview({ ...payload, reviewer: "cursor-grok-4.6" }, change),
    true,
  );
});
