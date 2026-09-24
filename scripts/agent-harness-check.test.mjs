/** X02：临时夹具运行真实检查器；不写仓库，覆盖范围见 governance.md。 */
import { test } from "node:test";
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import {
  mkdtempSync,
  mkdirSync,
  writeFileSync,
  readFileSync,
  rmSync,
} from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import {
  readMergedRegistry,
  writeMergedRegistrySync,
} from "./agent-harness-registry.mjs";

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const checker = path.join(scriptDir, "agent-harness-check.mjs");

for (const [author, reviewer] of [
  ["dsh-agent", "DSH Agent (复核)"],
  ["cursor-grok-4.6", "Cursor Grok 4.6"],
  ["mimo", "mimo"],
  ["user", "用户（确认）"],
  ["dsh-agent", ""],
  ["dsh-agent", "unattributed"],
  ["dsh-agent", "待复核"],
  ["dsh-agent", "待独立复核（尚无复核者）"],
  ["dsh-agent", "待重验"],
  ["dsh-agent", "other:unknown"],
  ["dsh-agent", "other:dsh-agent"],
  ["dsh-agent", "other: dsh-agent"],
  ["dsh-agent", "other: unknown"],
]) {
  test(`review_regression_同实体或无效复核身份不解封_${reviewer}`, () => {
    const fixture = buildFixture();
    try {
      assert.equal(reconcileFixture(fixture.root), 0);
      writeFileSync(
        path.join(fixture.harness, "contracts/K01.md"),
        K01_BODY.replace(
          "不变量：冻结确认绑定版本。",
          "不变量：冻结确认绑定版本与身份。",
        ),
      );
      assert.equal(reconcileFixture(fixture.root, "architecture"), 0);
      const registry = loadRegistry(fixture.harness);
      const change = registry.changes.at(-1);
      const review = registry.reviews.find(
        (entry) => entry.id === change.review,
      );
      change.author = author;
      Object.assign(review, {
        author: reviewer,
        reviewer: author,
        conclusion: "synchronized",
        evidence: "真实夹具证据",
        reason: "明确核验当前合同",
      });
      saveRegistry(fixture.harness, registry);
      assert.equal(runCheck(fixture.root).exitCode, 1);
      review.author = "independent-fixture";
      saveRegistry(fixture.harness, registry);
      assert.equal(runCheck(fixture.root).exitCode, 0);
      const originalObjects = review.objects;
      review.objects = [
        {
          id: "M01",
          fingerprint: registry.objects.M01.definition.fingerprint,
          revision: registry.objects.M01.revision,
        },
      ];
      saveRegistry(fixture.harness, registry);
      assert.equal(
        runCheck(fixture.root).exitCode,
        1,
        "无关对象不能覆盖当前变更",
      );
      review.objects = originalObjects.map((object) => ({
        ...object,
        stale: true,
      }));
      saveRegistry(fixture.harness, registry);
      assert.ok(
        runCheck(fixture.root).report.gaps.length > 0,
        "stale 标记不可自证当前已复核",
      );
      review.objects = [];
      saveRegistry(fixture.harness, registry);
      assert.equal(runCheck(fixture.root).exitCode, 1, "没有对象绑定不能解封");
    } finally {
      fixture.cleanup();
    }
  });
}

const FILES = {
  "README.md": "W01",
  "rules/governance.md": "W02",
  "requirements/requirements.md": "W03",
  "requirements/current-baseline.md": "W04",
  "requirements/open-items.md": "W05",
  "modules/m01.md": "M01",
  "contracts/K01.md": "K01",
  "tools/tool_a.md": "T01",
  "testing/evidence.md": "W06",
  "decisions/README.md": "W07",
  "registry.json": "FILE-REGISTRY",
};

/** 每个夹具都需要的骨架文件：入口、规则、需求与来源文档。 */
function skeleton() {
  return {
    "README.md": `# 夹具入口

<!-- iris:object W01 kind=rules file=true -->

夹具入口正文。

<!-- iris:end W01 -->

<!-- iris:object W01 kind=rules -->

### W01 夹具规则

夹具入口正文。

<!-- iris:end W01 -->
`,
    "rules/governance.md": `# 夹具治理

<!-- iris:object W02 kind=rules file=true -->

夹具治理正文（规则：结构违规退出码 1，基础设施失败退出码 2）。

<!-- iris:end W02 -->

<!-- iris:object W02 kind=rules -->

### W02 夹具规则

夹具治理正文（规则：结构违规退出码 1，基础设施失败退出码 2）。

<!-- iris:end W02 -->
`,
    "requirements/requirements.md": `# 夹具需求

<!-- iris:object W03 kind=rules file=true -->

夹具需求正文（要求：工作包必须引用需求）。

<!-- iris:end W03 -->

<!-- iris:object W03 kind=rules -->

### W03 夹具规则

夹具需求正文（要求：工作包必须引用需求）。

<!-- iris:end W03 -->

<!-- iris:object N01 kind=requirement owner=M01 -->

### N01 夹具需求

需求：夹具需要一条可引用的需求条目。

<!-- iris:end N01 -->
`,
    "requirements/current-baseline.md": `# 夹具基线

<!-- iris:object W04 kind=rules file=true -->

夹具基线正文（事实：仅用于自测）。

<!-- iris:end W04 -->

<!-- iris:object W04 kind=rules -->

### W04 夹具规则

夹具基线正文（事实：仅用于自测）。

<!-- iris:end W04 -->

<!-- iris:object Q01 kind=issue owner=M01 -->

### Q01 夹具未决问题

- **已确认事实**：夹具问题。
- **所需证据**：夹具证据。

<!-- iris:end Q01 -->
`,
    "requirements/open-items.md": `# 夹具缺口

<!-- iris:object W05 kind=rules file=true -->

夹具缺口正文（缺口：仅用于自测）。

<!-- iris:end W05 -->

<!-- iris:object W05 kind=rules -->

### W05 夹具规则

夹具缺口正文（缺口：仅用于自测）。

<!-- iris:end W05 -->
`,
    "testing/evidence.md": `# 夹具证据要求

<!-- iris:object W06 kind=rules file=true -->

夹具证据正文（要求：证据分类）。

<!-- iris:end W06 -->

<!-- iris:object W06 kind=rules -->

### W06 夹具规则

夹具证据正文（要求：证据分类）。

<!-- iris:end W06 -->

<!-- iris:object V01 kind=evidence name=categories owner=M01 -->

### V01 证据要求

证据按用途分类；未运行与失效证据不能作为通过依据。

<!-- iris:end V01 -->
`,
  };
}

const BASE_CATALOG = {
  sources: {},
  files: FILES,
  fileContainers: {
    W07: "decisions/README.md",
    "FILE-REGISTRY": "registry.json",
  },
  objects: {
    W01: {
      kind: "rules",
      name: "entry",
      title: "夹具入口",
      owner: "W01",
      maturity: "draft",
      definition: { file: "README.md", anchor: "W01" },
    },
    W02: {
      kind: "rules",
      name: "governance",
      title: "夹具治理",
      owner: "W02",
      maturity: "defined",
      definition: { file: "rules/governance.md", anchor: "W02" },
    },
    W03: {
      kind: "rules",
      name: "requirements",
      title: "夹具需求",
      owner: "W03",
      maturity: "defined",
      definition: { file: "requirements/requirements.md", anchor: "W03" },
    },
    W04: {
      kind: "rules",
      name: "baseline",
      title: "夹具基线",
      owner: "W04",
      maturity: "defined",
      definition: { file: "requirements/current-baseline.md", anchor: "W04" },
    },
    W05: {
      kind: "rules",
      name: "open-items",
      title: "夹具缺口",
      owner: "W05",
      maturity: "defined",
      definition: { file: "requirements/open-items.md", anchor: "W05" },
    },
    W06: {
      kind: "rules",
      name: "evidence",
      title: "夹具证据",
      owner: "W06",
      maturity: "defined",
      definition: { file: "testing/evidence.md", anchor: "W06" },
    },
    W07: {
      kind: "rules",
      name: "decisions",
      title: "夹具决定记录",
      owner: "W07",
      maturity: "draft",
      definition: { file: "decisions/README.md", anchor: "W07" },
    },
    N01: {
      kind: "requirement",
      name: "N01",
      title: "夹具需求条目",
      owner: "M01",
      maturity: "draft",
      definition: { file: "requirements/requirements.md", anchor: "N01" },
    },
    Q01: {
      kind: "issue",
      name: "Q01",
      title: "夹具问题",
      owner: "M01",
      maturity: "draft",
      definition: { file: "requirements/current-baseline.md", anchor: "Q01" },
    },
    V01: {
      kind: "evidence",
      name: "categories",
      title: "夹具证据要求",
      owner: "M01",
      maturity: "defined",
      definition: { file: "testing/evidence.md", anchor: "V01" },
    },
    M01: {
      kind: "module",
      name: "module",
      title: "夹具模块",
      owner: "M01",
      maturity: "defined",
      definition: { file: "modules/m01.md", anchor: "M01" },
      implementation: { state: "partial" },
      verification: { state: "none" },
      implements: ["N01"],
    },
    K01: {
      kind: "contract",
      name: "contract",
      title: "夹具合同",
      owner: "M01",
      maturity: "defined",
      definition: { file: "contracts/K01.md", anchor: "K01" },
      applies_to: ["M01"],
    },
    T01: {
      kind: "tool",
      name: "tool_a",
      title: "夹具工具",
      owner: "M01",
      maturity: "defined",
      definition: { file: "tools/tool_a.md", anchor: "T01" },
      implementation: { state: "present" },
      verification: { state: "none" },
      consumes: ["K01"],
    },
    L01: {
      kind: "flow",
      name: "flow",
      title: "夹具链路",
      owner: "M01",
      maturity: "defined",
      definition: { file: "flows/L01.md", anchor: "L01" },
      implements: ["N01"],
      consumes: ["K01"],
    },
  },
};

/** 默认正文：满足各类对象的必备要素。 */
function defaultBodies() {
  return {
    "modules/m01.md": `# 夹具模块

<!-- iris:object M01 kind=module file=true -->

夹具模块正文。

<!-- iris:end M01 -->

<!-- iris:object M01 kind=module name=module -->

## 输入与输出

输入：夹具输入。输出：夹具输出。

## 决定权

决定权：模块内判定。

## 状态

状态：模块持有夹具状态。

## 不变量

不变量：事实只有一个权威来源。

## 异常与恢复

异常：失败按类别反馈并恢复。

## 审计

审计：向诊断提供夹具事件。

## 源码落点

现有基础：夹具源码。

## 兼容

兼容：读边界单向兼容。

## 测试

测试：所需证据类别见 V01。

<!-- iris:end M01 -->
`,
    "contracts/K01.md": `# 夹具合同

<!-- iris:object K01 kind=contract file=true -->

夹具合同正文。

<!-- iris:end K01 -->

<!-- iris:object K01 kind=contract name=contract owner=M01 -->

## 接口

接口：输入与输出。

## 状态转换

状态：转换规则。

## 不变量

不变量：冻结确认绑定版本。

## 异常处理

异常：绑定失败时拒绝执行。

<!-- iris:end K01 -->
`,
    "tools/tool_a.md": `# tool_a

<!-- iris:object T01 kind=tool file=true -->

夹具工具卡。

<!-- iris:end T01 -->

<!-- iris:object T01 kind=tool name=tool_a owner=M01 -->

## 用途与语义归属

用途：夹具。

## 参数与消费

参数：全部消费或明确拒绝。

## 授权

授权：由 C04 判定。

## 副作用

副作用：无文件写入。

## 预算

预算：计入 local 类别。

## 幂等

幂等：重复调用安全。

## 取消

取消：取消后不产生新副作用。

## 失败反馈

失败：字段级错误反馈。

## 暴露规则

暴露：基础工具面。

## 源码落点

源码落点：夹具实现。

## 当前状态

当前状态：present；verification.state=none。

## 相关合同

相关合同：K01。

<!-- iris:end T01 -->
`,
    "flows/L01.md": `# 夹具链路

<!-- iris:object L01 kind=flow file=true -->

夹具链路正文。

<!-- iris:end L01 -->

<!-- iris:object L01 kind=flow name=flow owner=M01 -->

## 步骤与交接

1. M01 → K01 → 交付。

## 引用合同

引用 K01：夹具需要它约束绑定。

<!-- iris:end L01 -->
`,
    "implementation/D01.md": `# 夹具工作包

<!-- iris:object D01 kind=work owner=M01 file=true -->

夹具工作包正文。

<!-- iris:end D01 -->
`,
    "decisions/README.md": `# 夹具决定记录

<!-- iris:object W07 kind=rules file=true -->

夹具决定记录索引。

<!-- iris:end W07 -->

<!-- iris:object W07 kind=rules -->

### W07 夹具规则

夹具决定记录索引。

<!-- iris:end W07 -->
`,
  };
}

/**
 * 生成一个夹具仓库并运行检查器。
 * @param overrides 覆盖或新增文件内容（值为 null 表示删除该文件）
 * @param catalog 覆盖 catalog 对象条目
 */
/**
 * 生成夹具仓库但不运行检查器。返回 { root, harness, cleanup }。
 */
function buildFixture({
  overrides = {},
  objects = {},
  sources = {},
  files = {},
} = {}) {
  const root = mkdtempSync(path.join(tmpdir(), "harness-fixture-"));
  const harness = path.join(root, "agent-harness");
  mkdirSync(harness, { recursive: true });
  mkdirSync(path.join(root, "docs"), { recursive: true });
  writeFileSync(
    path.join(root, "docs", "README.md"),
    "[agent-harness/README.md](../agent-harness/README.md)\n",
  );

  const all = { ...skeleton(), ...defaultBodies() };
  for (const [rel, content] of Object.entries({ ...all, ...overrides })) {
    if (content === null) continue;
    const target = path.join(harness, rel);
    mkdirSync(path.dirname(target), { recursive: true });
    writeFileSync(target, content);
  }
  for (const [rel, content] of Object.entries(overrides)) {
    if (content === null) rmSync(path.join(harness, rel), { force: true });
  }

  const catalog = {
    sources: { ...BASE_CATALOG.sources, ...sources },
    files: { ...BASE_CATALOG.files, ...files },
    fileContainers: { ...BASE_CATALOG.fileContainers },
    objects: Object.fromEntries(
      Object.entries({ ...BASE_CATALOG.objects, ...objects }).filter(
        ([, value]) => value !== null,
      ),
    ),
  };
  writeFileSync(
    path.join(harness, "catalog.mjs"),
    `export const sources = ${JSON.stringify(catalog.sources)};\n` +
      `export const files = ${JSON.stringify(catalog.files)};\n` +
      `export const fileContainers = ${JSON.stringify(catalog.fileContainers)};\n` +
      `export const objects = ${JSON.stringify(catalog.objects)};\n` +
      `export const tools = {};\nexport const toolPlacement = {};\nexport const toolIdByName = {};\n`,
  );
  writeFileSync(
    path.join(harness, "registry.json"),
    `<!-- iris:object FILE-REGISTRY kind=rules file=true -->\n${JSON.stringify(
      {
        schemaVersion: "iris-agent-harness-registry-v1",
        registryRevision: 1,
        sources: {},
        files: {},
        objects: {},
        verify: [],
        changes: [],
        reviews: [],
        issues: {},
      },
      null,
      2,
    )}\n`,
  );

  return {
    root,
    harness,
    cleanup: () => rmSync(root, { recursive: true, force: true }),
  };
}

/** 以 --reconcile 建立指纹基线（忽略失败，由 check 断言真正的行为）。 */
function reconcileFixture(root, classification = "refinement") {
  try {
    execFileSync(
      process.execPath,
      [
        checker,
        "--root",
        root,
        "--reconcile",
        "--author",
        "fixture",
        "--classification",
        classification,
        "--reason",
        "fixture baseline",
        ...(classification === "refinement"
          ? []
          : ["--reviewer", "independent-fixture"]),
      ],
      { encoding: "utf8", stdio: "pipe" },
    );
    return 0;
  } catch (error) {
    return error.status ?? 1;
  }
}

function loadRegistry(harness) {
  return readMergedRegistry(harness);
}

function saveRegistry(harness, registry) {
  writeMergedRegistrySync(harness, registry);
}

const K01_BODY = `# 夹具合同

<!-- iris:object K01 kind=contract file=true -->

夹具合同正文。

<!-- iris:end K01 -->

<!-- iris:object K01 kind=contract name=contract owner=M01 -->

## 接口

接口：输入与输出。

## 状态转换

状态：转换规则。

## 不变量

不变量：冻结确认绑定版本。

## 异常处理

异常：绑定失败时拒绝执行。

<!-- iris:end K01 -->
`;

/** 对已存在的夹具仓库运行检查器，返回 { exitCode, report }。 */
function runCheck(root) {
  try {
    const stdout = execFileSync(
      process.execPath,
      [checker, "--root", root, "--json"],
      {
        encoding: "utf8",
        stdio: "pipe",
      },
    );
    return { exitCode: 0, report: JSON.parse(stdout) };
  } catch (error) {
    const stdout = error.stdout ?? "";
    const parsed = stdout
      ? JSON.parse(stdout)
      : { violations: [], infrastructure: [] };
    return { exitCode: error.status ?? 1, report: parsed };
  }
}

/**
 * 建立基线后运行检查器：夹具的一次完整往返。
 */
function runFixture(options = {}) {
  const fixture = buildFixture(options);
  try {
    reconcileFixture(fixture.root);
    return runCheck(fixture.root);
  } finally {
    fixture.cleanup();
  }
}

const hasViolation = (report, check) =>
  (report.violations ?? []).some((entry) => entry.check === check);
const violationMessages = (report, check) =>
  (report.violations ?? [])
    .filter((entry) => entry.check === check)
    .map((entry) => entry.message)
    .join(" | ");

test("健康夹具通过检查（多消费者共享一个合同是合法的）", () => {
  const { exitCode, report } = runFixture();
  assert.equal(
    exitCode,
    0,
    `预期通过，实际违规：${JSON.stringify(report.violations ?? [], null, 1)}`,
  );
});

test("未登记对象标记阻断检查", () => {
  const { exitCode, report } = runFixture({
    overrides: {
      "modules/m01.md": `# 夹具模块

<!-- iris:object M01 kind=module file=true -->

夹具模块正文。

<!-- iris:end M01 -->

<!-- iris:object C99 kind=component name=ghost -->

## 输入与输出

未登记组件。

<!-- iris:end C99 -->

<!-- iris:object M01 kind=module name=module -->

## 输入与输出

输入。输出。

## 决定权

决定权。

## 状态

状态。

## 不变量

不变量。

## 异常与恢复

异常。

## 审计

审计。

## 源码落点

落点。

## 兼容

兼容。

## 测试

测试。
`,
    },
  });
  assert.equal(exitCode, 1);
  assert.ok(
    hasViolation(report, "registration"),
    JSON.stringify(report.violations),
  );
});

test("标记未闭合阻断检查", () => {
  const { exitCode, report } = runFixture({
    overrides: {
      "contracts/K01.md": `# 夹具合同

<!-- iris:object K01 kind=contract file=true -->

夹具合同正文。

<!-- iris:object K01 kind=contract name=contract owner=M01 -->

## 接口

接口。

## 状态转换

状态。

## 不变量

不变量。

## 异常处理

异常。
`,
    },
  });
  assert.equal(exitCode, 1);
  assert.ok(
    hasViolation(report, "marker-pairing") ||
      hasViolation(report, "file-object"),
  );
});

test("同一 ID 跨文件重复阻断检查", () => {
  const { exitCode, report } = runFixture({
    overrides: {
      "flows/L01.md": `# 夹具链路

<!-- iris:object L01 kind=flow file=true -->

夹具链路正文。

<!-- iris:end L01 -->

<!-- iris:object T01 kind=tool name=tool_a owner=M01 -->

## 用途与语义归属

用途。

## 参数与消费

参数。

## 授权

授权。

## 副作用

副作用：无。

## 预算

预算。

## 幂等

幂等。

## 取消

取消。

## 失败反馈

失败。

## 暴露规则

暴露。

## 源码落点

落点。

## 当前状态

状态。

## 相关合同

K01。

<!-- iris:end T01 -->
`,
    },
  });
  assert.equal(exitCode, 1);
  assert.ok(
    hasViolation(report, "duplicate-id"),
    JSON.stringify(report.violations),
  );
});

test("声明正式定义却找不到对象块阻断检查", () => {
  const { exitCode, report } = runFixture({
    objects: {
      K01: {
        kind: "contract",
        name: "contract",
        title: "夹具合同",
        owner: "M01",
        maturity: "defined",
        definition: { file: "contracts/missing.md", anchor: "K01" },
      },
    },
  });
  assert.ok(exitCode !== 0);
  assert.ok(
    hasViolation(report, "definition") ||
      (report.infrastructure ?? []).length > 0,
  );
});

test("对象移动（改文件位置）登记不一致时阻断", () => {
  const { exitCode, report } = runFixture({
    files: { "contracts/K02.md": "K01" },
    overrides: {
      "contracts/K01.md": `# 夹具合同

<!-- iris:object K01 kind=contract file=true -->

夹具合同正文。

<!-- iris:end K01 -->
`,
      "contracts/K02.md": `# 夹具合同（移动后）

<!-- iris:object K01 kind=contract file=true -->

夹具合同正文。

<!-- iris:end K01 -->

<!-- iris:object K01 kind=contract name=contract owner=M01 -->

## 接口

接口。

## 状态转换

状态。

## 不变量

不变量。

## 异常处理

异常。

<!-- iris:end K01 -->
`,
    },
  });
  assert.ok(exitCode !== 0);
  assert.ok(
    hasViolation(report, "definition") || hasViolation(report, "files"),
  );
});

test("review_regression_外围复核使用独立容器身份", () => {
  // 同一个夹具内：先建立基线，再改变所在文件的前言，再检查。
  const fixture = buildFixture();
  try {
    assert.equal(reconcileFixture(fixture.root), 0, "夹具基线登记失败");
    const first = runCheck(fixture.root);
    assert.equal(first.exitCode, 0, JSON.stringify(first.report.violations));

    writeFileSync(
      path.join(fixture.harness, "modules", "m01.md"),
      `# 夹具模块

<!-- iris:object M01 kind=module file=true -->

夹具模块正文（前言已变化）。

<!-- iris:end M01 -->

<!-- iris:object M01 kind=module name=module -->

## 输入与输出

输入。输出。

## 决定权

决定权。

## 状态

状态。

## 不变量

不变量。

## 异常与恢复

异常。

## 审计

审计。

## 源码落点

落点。

## 兼容

兼容。

## 测试

测试。

<!-- iris:end M01 -->
`,
    );
    const second = runCheck(fixture.root);
    assert.notEqual(second.exitCode, 0, "前言变化后必须报未接受的指纹变化");
    assert.ok(
      hasViolation(second.report, "fingerprint") ||
        hasViolation(second.report, "propagation"),
      JSON.stringify(second.report.violations),
    );
    assert.equal(reconcileFixture(fixture.root, "architecture"), 0);
    const registry = loadRegistry(fixture.harness),
      change = registry.changes.at(-1);
    assert.ok(change.objects.some((object) => object.id === "M01#file"));
    const review = registry.reviews.find((entry) => entry.id === change.review);
    Object.assign(review, {
      conclusion: "synchronized",
      evidence: "fixture container review",
      reason: "checked current file and child",
    });
    saveRegistry(fixture.harness, registry);
    assert.equal(runCheck(fixture.root).exitCode, 0);
  } finally {
    fixture.cleanup();
  }
});

test("子对象正文变化而文件外围不变时阻断", () => {
  const fixture = buildFixture();
  try {
    assert.equal(reconcileFixture(fixture.root), 0, "夹具基线登记失败");
    const first = runCheck(fixture.root);
    assert.equal(first.exitCode, 0, JSON.stringify(first.report.violations));

    changeContract(fixture, "不变量：冻结确认绑定版本（已修订）。");
    const second = runCheck(fixture.root);
    assert.notEqual(second.exitCode, 0, "对象正文变化后必须报未接受的指纹变化");
    assert.ok(
      hasViolation(second.report, "fingerprint"),
      JSON.stringify(second.report.violations),
    );
    assert.ok(
      violationMessages(second.report, "fingerprint").includes("K01"),
      violationMessages(second.report, "fingerprint"),
    );
  } finally {
    fixture.cleanup();
  }
});

test("对象块内标题变化被计入指纹并阻断", () => {
  const fixture = buildFixture();
  try {
    assert.equal(reconcileFixture(fixture.root), 0, "夹具基线登记失败");
    assert.equal(runCheck(fixture.root).exitCode, 0);

    writeFileSync(
      path.join(fixture.harness, "contracts", "K01.md"),
      K01_BODY.replace("## 不变量", "## 可变约束"),
    );
    const second = runCheck(fixture.root);
    assert.notEqual(second.exitCode, 0, "标题是正式定义正文，变化必须被检出");
    assert.ok(
      hasViolation(second.report, "fingerprint"),
      JSON.stringify(second.report.violations),
    );
    assert.ok(
      violationMessages(second.report, "fingerprint").includes("K01"),
      violationMessages(second.report, "fingerprint"),
    );
  } finally {
    fixture.cleanup();
  }
});

test("只有登记（无正式定义）的对象不能作为前置支持可施工声明", () => {
  const { exitCode, report } = runFixture({
    objects: {
      K01: {
        kind: "contract",
        name: "contract",
        title: "只有登记的合同",
        owner: "M01",
        maturity: "registered",
      },
      D01: {
        kind: "work",
        name: "work",
        title: "夹具工作包",
        owner: "M01",
        maturity: "defined",
        definition: { file: "decisions/README.md", anchor: "D01" },
        work: { state: "planned" },
        scope: ["N01"],
        depends_on: ["K01"],
      },
    },
    overrides: {
      "decisions/README.md": `# 夹具决定记录

<!-- iris:object W07 kind=rules file=true -->

夹具决定记录索引。

<!-- iris:end W07 -->

<!-- iris:object W07 kind=rules -->

### W07 夹具规则

夹具决定记录索引正文。

<!-- iris:end W07 -->

<!-- iris:object D01 kind=work owner=M01 -->

## 一、前置条件

前置：K01。

## 二、改动范围

范围：夹具。

## 三、回归证据

证据：夹具。

## 四、回退条件

回退：夹具。

<!-- iris:end D01 -->
`,
    },
  });
  assert.equal(exitCode, 0, JSON.stringify(report.violations));
  const ready = report.readiness?.D01;
  assert.ok(ready, "必须报告 D01 就绪状态");
  assert.equal(ready.startReady, false, "前置只有登记时不得判定可施工");
  assert.ok(
    ready.startReasons.join(" ").includes("尚无正式定义"),
    JSON.stringify(ready),
  );
});

test("声明 defined 但缺少必备要素时阻断", () => {
  const { exitCode, report } = runFixture({
    overrides: {
      "tools/tool_a.md": `# tool_a

<!-- iris:object T01 kind=tool file=true -->

夹具工具卡。

<!-- iris:end T01 -->

<!-- iris:object T01 kind=tool name=tool_a owner=M01 -->

## 用途与语义归属

用途：夹具。

<!-- iris:end T01 -->
`,
    },
  });
  assert.equal(exitCode, 1);
  assert.ok(hasViolation(report, "contract-elements"));
});

function issue(id, title, at) {
  return {
    kind: "issue",
    name: id,
    title,
    owner: "M01",
    maturity: "draft",
    definition: { file: "requirements/current-baseline.md", anchor: id },
    blocks: [{ id: "D01", at }],
  };
}

function workPackage(closes) {
  return {
    kind: "work",
    name: "work",
    title: "夹具工作包",
    owner: "M01",
    maturity: "defined",
    definition: { file: "implementation/D01.md", anchor: "D01" },
    work: { state: "planned" },
    scope: ["N01", "Q01", "Q02"],
    closes,
  };
}

const WORK_PACKAGE_OVERRIDES = {
  "requirements/current-baseline.md": `# 夹具基线

<!-- iris:object W04 kind=rules file=true -->

夹具基线正文（事实：仅用于自测）。

<!-- iris:end W04 -->

<!-- iris:object W04 kind=rules -->

### W04 夹具规则

夹具基线正文（事实：仅用于自测）。

<!-- iris:end W04 -->

<!-- iris:object Q01 kind=issue owner=M01 -->

### Q01 夹具未决问题

- **已确认事实**：夹具问题甲。
- **所需证据**：夹具证据。

<!-- iris:end Q01 -->

<!-- iris:object Q02 kind=issue owner=M01 -->

### Q02 夹具未决问题乙

- **已确认事实**：夹具问题乙。
- **所需证据**：夹具证据。

<!-- iris:end Q02 -->
`,
  "implementation/D01.md": `# 夹具工作包

<!-- iris:object D01 kind=work owner=M01 file=true -->

夹具工作包正文。

<!-- iris:end D01 -->

<!-- iris:object D01 kind=work owner=M01 -->

## 一、前置条件

前置：无。

## 二、改动范围

范围。

## 三、回归证据

证据。

## 四、回退条件

回退。

<!-- iris:end D01 -->
`,
};

test("一个问题关闭但另一阻断仍存在，工作包不能解封", () => {
  const fixture = buildFixture({
    files: { "implementation/D01.md": "D01" },
    objects: {
      Q01: issue("Q01", "夹具问题甲", "acceptance"),
      Q02: issue("Q02", "夹具问题乙", "acceptance"),
      D01: workPackage([]),
    },
    overrides: WORK_PACKAGE_OVERRIDES,
  });
  try {
    assert.equal(reconcileFixture(fixture.root), 0, "夹具基线登记失败");
    const registry = loadRegistry(fixture.harness);
    assert.equal(registry.issues?.Q01?.state, "open");
    assert.equal(registry.issues?.Q02?.state, "open");
    registry.issues.Q01 = { state: "closed" };
    saveRegistry(fixture.harness, registry);
    const cat = path.join(fixture.harness, "catalog.mjs");
    const src = readFileSync(cat, "utf8");
    const objs = JSON.parse(src.match(/export const objects = (.*);/)[1]);
    objs.Q01.blocks = [];
    writeFileSync(
      cat,
      src.replace(
        /export const objects = .*;/,
        `export const objects = ${JSON.stringify(objs)};`,
      ),
    );
    const { report } = runCheck(fixture.root);
    assert.equal(hasViolation(report, "issues"), false);
    const ready = report.readiness?.D01;
    assert.ok(ready);
    assert.equal(ready.acceptanceReady, false);
    const reasons = ready.acceptanceReasons.join(" ");
    assert.ok(reasons.includes("Q02"), JSON.stringify(ready));
    assert.equal(
      reasons.includes("Q01"),
      false,
      `Q01 已关闭，不应再出现在验收阻断里：${reasons}`,
    );
  } finally {
    fixture.cleanup();
  }
});

test("closes 只豁免开始阻断，不能把未关问题写成可验收", () => {
  const fixture = buildFixture({
    files: { "implementation/D01.md": "D01" },
    objects: {
      Q01: issue("Q01", "夹具开始阻断", "start"),
      Q02: issue("Q02", "夹具验收阻断", "acceptance"),
      D01: workPackage(["Q01", "Q02"]),
    },
    overrides: WORK_PACKAGE_OVERRIDES,
  });
  try {
    assert.equal(reconcileFixture(fixture.root), 0, "夹具基线登记失败");
    const { report } = runCheck(fixture.root);
    const ready = report.readiness?.D01;
    assert.ok(ready, JSON.stringify(report.readiness));
    assert.equal(
      ready.startReady,
      true,
      `closes 应豁免 start 阻断：${JSON.stringify(ready)}`,
    );
    assert.equal(
      ready.acceptanceReady,
      false,
      `closes 不得豁免验收：${JSON.stringify(ready)}`,
    );
    const reasons = ready.acceptanceReasons.join(" ");
    assert.ok(reasons.includes("Q02"), reasons);
  } finally {
    fixture.cleanup();
  }
});

test("passed 不能只靠 obsolete 或缺失指纹的证据", () => {
  const fixture = buildFixture();
  try {
    assert.equal(reconcileFixture(fixture.root), 0, "夹具基线登记失败");
    const registry = loadRegistry(fixture.harness);
    const current = registry.objects.K01.definition.fingerprint;
    const catalogPath = path.join(fixture.harness, "catalog.mjs");
    const objects = {
      ...BASE_CATALOG.objects,
      K01: {
        ...BASE_CATALOG.objects.K01,
        verification: { state: "passed" },
      },
    };
    writeFileSync(
      catalogPath,
      `export const sources = ${JSON.stringify(BASE_CATALOG.sources)};\n` +
        `export const files = ${JSON.stringify(BASE_CATALOG.files)};\n` +
        `export const fileContainers = ${JSON.stringify(BASE_CATALOG.fileContainers)};\n` +
        `export const objects = ${JSON.stringify(objects)};\n` +
        `export const tools = {};\nexport const toolPlacement = {};\nexport const toolIdByName = {};\n`,
    );
    registry.verify = [
      {
        object: "K01",
        kind: "V03",
        testPath: "contracts/K01.md",
        command: "fixture",
        environment: "fixture",
        fingerprint: current,
        at: "2026-01-01T00:00:00Z",
        applicability: "obsolete",
        note: "夹具：仅 obsolete",
      },
      {
        object: "K01",
        kind: "V03",
        testPath: "contracts/K01.md",
        command: "fixture",
        environment: "fixture",
        at: "2026-01-02T00:00:00Z",
        applicability: "current",
        note: "夹具：current 但缺指纹",
      },
    ];
    saveRegistry(fixture.harness, registry);
    const { exitCode, report } = runCheck(fixture.root);
    assert.equal(exitCode, 1);
    assert.ok(
      hasViolation(report, "evidence"),
      JSON.stringify(report.violations),
    );
  } finally {
    fixture.cleanup();
  }
});

test("架构变更的 reconcile 不得自签已完成复核", () => {
  const fixture = buildFixture();
  try {
    assert.equal(reconcileFixture(fixture.root), 0, "夹具基线登记失败");
    changeContract(fixture, "不变量：冻结确认绑定版本（架构变更夹具）。");
    assert.equal(
      reconcileFixture(fixture.root, "architecture"),
      0,
      "指定 --reviewer 后 reconcile 应能写入",
    );

    const registry = loadRegistry(fixture.harness);
    const change = [...(registry.changes ?? [])].find(
      (entry) => entry.classification === "architecture",
    );
    assert.ok(change, "必须留下架构变更记录");
    assert.equal(change.author, "fixture");
    const review = (registry.reviews ?? []).find(
      (entry) => entry.id === change.review,
    );
    assert.ok(review, "必须留下待复核记录");
    assert.equal(review.author, "independent-fixture");
    assert.equal(review.reviewer, "fixture");
    assert.notEqual(
      review.conclusion,
      "synchronized",
      "reconcile 不能把待复核写成已完成",
    );

    const pending = runCheck(fixture.root);
    assert.equal(pending.exitCode, 1, "待复核结论不能解封架构变更");
    assert.ok(
      hasViolation(pending.report, "reviews"),
      JSON.stringify(pending.report.violations),
    );

    review.conclusion = "synchronized";
    review.evidence = "夹具独立复核材料";
    review.reason = "夹具确认无额外影响";
    saveRegistry(fixture.harness, registry);
    const released = runCheck(fixture.root);
    assert.equal(
      released.exitCode,
      0,
      JSON.stringify(released.report.violations),
    );
  } finally {
    fixture.cleanup();
  }
});

test("合同正文变化只阻断该对象，不误报无关模块", () => {
  const fixture = buildFixture();
  try {
    assert.equal(reconcileFixture(fixture.root), 0, "夹具基线登记失败");
    changeContract(fixture, "不变量：冻结确认绑定版本（已修订）。");
    const { exitCode, report } = runCheck(fixture.root);
    assert.notEqual(exitCode, 0, "对象正文变化后必须报未接受的指纹变化");
    const fingerprints = violationMessages(report, "fingerprint");
    assert.ok(fingerprints.includes("K01"), fingerprints);
    assert.equal(
      fingerprints.includes("M01"),
      false,
      `无关模块不应出现在指纹违规里：${fingerprints}`,
    );
    const propagation = violationMessages(report, "propagation");
    assert.equal(
      propagation.includes("M01"),
      false,
      `无关模块不应进入外围传播复核：${propagation}`,
    );
  } finally {
    fixture.cleanup();
  }
});

test("作者自签 synchronized 不能解封架构变更", () => {
  const fixture = buildFixture();
  try {
    assert.equal(reconcileFixture(fixture.root), 0, "夹具基线登记失败");
    changeContract(fixture, "不变量：冻结确认绑定版本（架构变更夹具）。");
    assert.equal(
      reconcileFixture(fixture.root, "architecture"),
      0,
      "指定 --reviewer 后 reconcile 应能写入",
    );
    const registry = loadRegistry(fixture.harness);
    const change = [...(registry.changes ?? [])].find(
      (entry) => entry.classification === "architecture",
    );
    const review = (registry.reviews ?? []).find(
      (entry) => entry.id === change.review,
    );
    review.conclusion = "synchronized";
    review.evidence = "夹具独立复核材料";
    review.reason = "夹具确认无额外影响";
    review.author = change.author;
    saveRegistry(fixture.harness, registry);

    const { exitCode, report } = runCheck(fixture.root);
    assert.equal(exitCode, 1, "作者自签不能解封");
    assert.ok(
      violationMessages(report, "reviews").includes("作者"),
      JSON.stringify(report.violations),
    );
  } finally {
    fixture.cleanup();
  }
});

test("架构复核缺少理由不能解封", () => {
  const fixture = buildFixture();
  try {
    assert.equal(reconcileFixture(fixture.root), 0, "夹具基线登记失败");
    changeContract(fixture, "不变量：冻结确认绑定版本（架构变更夹具）。");
    assert.equal(
      reconcileFixture(fixture.root, "architecture"),
      0,
      "指定 --reviewer 后 reconcile 应能写入",
    );
    const registry = loadRegistry(fixture.harness);
    const change = [...(registry.changes ?? [])].find(
      (entry) => entry.classification === "architecture",
    );
    const review = (registry.reviews ?? []).find(
      (entry) => entry.id === change.review,
    );
    review.conclusion = "synchronized";
    review.evidence = "夹具独立复核材料";
    review.reason = "";
    saveRegistry(fixture.harness, registry);

    const { exitCode, report } = runCheck(fixture.root);
    assert.equal(exitCode, 1, "缺少理由不能解封");
    assert.ok(
      violationMessages(report, "reviews").includes("理由"),
      JSON.stringify(report.violations),
    );
  } finally {
    fixture.cleanup();
  }
});

test("架构复核绑定旧指纹不能解封", () => {
  const fixture = buildFixture();
  try {
    assert.equal(reconcileFixture(fixture.root), 0, "夹具基线登记失败");
    changeContract(fixture, "不变量：冻结确认绑定版本（架构变更夹具）。");
    assert.equal(
      reconcileFixture(fixture.root, "architecture"),
      0,
      "指定 --reviewer 后 reconcile 应能写入",
    );
    const registry = loadRegistry(fixture.harness);
    const change = [...(registry.changes ?? [])].find(
      (entry) => entry.classification === "architecture",
    );
    const review = (registry.reviews ?? []).find(
      (entry) => entry.id === change.review,
    );
    review.conclusion = "synchronized";
    review.evidence = "夹具独立复核材料";
    review.reason = "夹具确认无额外影响";
    assert.ok(review.objects?.[0], "复核必须绑定对象指纹");
    review.objects[0].fingerprint = "deadbeefdeadbeefdeadbeefdeadbeef";
    saveRegistry(fixture.harness, registry);

    const { exitCode, report } = runCheck(fixture.root);
    assert.equal(exitCode, 1, "绑定旧指纹不能解封");
    assert.ok(
      violationMessages(report, "reviews").includes("旧指纹"),
      JSON.stringify(report.violations),
    );
  } finally {
    fixture.cleanup();
  }
});

test("复核绑定文件级容器指纹时不得按对象指纹判违规", () => {
  const fixture = buildFixture();
  try {
    assert.equal(reconcileFixture(fixture.root), 0, "夹具基线登记失败");
    // 先制造一次真实变化，复核记录才会存在。
    changeContract(fixture, "不变量：冻结确认绑定版本（容器绑定夹具）。");
    assert.equal(
      reconcileFixture(fixture.root, "architecture"),
      0,
      "接受变化应能写入",
    );

    const registry = loadRegistry(fixture.harness);
    const change = (registry.changes ?? []).at(-1);
    const review = (registry.reviews ?? []).find(
      (entry) => entry.id === change.review,
    );
    assert.ok(
      review,
      `夹具必须产生一条复核记录：${JSON.stringify(registry.changes)}`,
    );
    // 复现待修形状：复核绑定一个**容器登记键**，其指纹是**文件级**指纹。单对象
    const containerFile = Object.entries(registry.files ?? {}).find(
      ([, entry]) => entry.registration === entry.container,
    );
    assert.ok(
      containerFile,
      `夹具必须有一个「容器键等于对象键」的文件：${JSON.stringify(registry.files)}`,
    );
    const [rel, containerEntry] = containerFile;
    const containerBinding = {
      id: containerEntry.registration,
      revision: 0,
      fingerprint: containerEntry.fingerprint,
    };
    review.objects.push(containerBinding);
    // 复核里真实的**对象**绑定（本次改动的是 K01 的对象正文）：它必须仍然被检查。
    // 按名字判容器会把这类绑定一起跳掉，等于悄悄关掉「绑定旧指纹不能解封」这条守卫。
    const objectBinding = review.objects.find(
      (candidate) => candidate.id === "K01",
    );
    assert.ok(objectBinding, "复核必须保留 K01 的对象绑定");
    assert.notEqual(
      objectBinding.fingerprint,
      containerEntry.fingerprint,
      "对象指纹与文件级容器指纹必须不同，否则这条用例没有测到任何东西",
    );
    saveRegistry(fixture.harness, registry);

    const { report } = runCheck(fixture.root);
    assert.deepEqual(
      (report.violations ?? []).filter(
        (item) =>
          item.check === "reviews" && String(item.message).includes("旧指纹"),
      ),
      [],
      `容器指纹不得按对象指纹判定（${rel}）：${JSON.stringify(report.violations)}`,
    );

    // 反例：把**对象**绑定的指纹改坏，守卫必须仍然生效。
    objectBinding.fingerprint = "deadbeefdeadbeefdeadbeefdeadbeef";
    saveRegistry(fixture.harness, registry);
    const tampered = runCheck(fixture.root);
    assert.equal(tampered.exitCode, 1, "对象绑定旧指纹必须仍然阻断");
    assert.ok(
      (tampered.report.violations ?? []).some(
        (item) =>
          item.check === "reviews" && String(item.message).includes("旧指纹"),
      ),
      JSON.stringify(tampered.report.violations),
    );
  } finally {
    fixture.cleanup();
  }
});

test("已退役证据保留旧指纹不算违规，needs-review 仍算", () => {
  const fixture = buildFixture();
  try {
    assert.equal(reconcileFixture(fixture.root), 0, "夹具基线登记失败");
    // 先接受一次**对象正文**变化。顺序很重要：reconcile 会先跑一遍检查，若此时
    writeFileSync(
      path.join(fixture.harness, "modules", "m01.md"),
      defaultBodies()["modules/m01.md"].replace(
        "测试：所需证据类别见 V01。",
        "测试：所需证据类别见 V01（已变化）。",
      ),
    );
    assert.equal(reconcileFixture(fixture.root), 0, "接受内容变化应能写入");

    // 现在写入一条**绑定变更前指纹**的证据：这就是「改写后失绑」的形状，
    // 不再依赖「边改内容边留旧记录」这个 reconcile 本来就不允许的顺序。
    const registry = loadRegistry(fixture.harness);
    const staleFingerprint = "22c25bceaa1444af3346b01eb2270870";
    const current = registry.objects.M01.definition.fingerprint;
    assert.notEqual(
      staleFingerprint,
      current,
      "夹具必须让绑定与当前指纹不同，否则这条用例没有测到任何东西",
    );
    registry.verify = [
      ...(registry.verify ?? []),
      {
        object: "M01",
        kind: "V03",
        testPath: "modules/m01.md",
        command: "fixture",
        environment: "fixture",
        fingerprint: staleFingerprint,
        at: "2026-01-01T00:00:00Z",
        applicability: "current",
        note: "夹具证据（绑定变更前指纹）",
      },
    ];
    saveRegistry(fixture.harness, registry);

    const asCurrent = runCheck(fixture.root);
    assert.equal(asCurrent.exitCode, 1, "绑定旧指纹且声称 current 必须报错");
    assert.ok(
      (asCurrent.report.violations ?? []).some(
        (entry) =>
          entry.check === "verify" && String(entry.message).includes("旧指纹"),
      ),
      JSON.stringify(asCurrent.report.violations),
    );

    const record = registry.verify.at(-1);
    record.applicability = "needs-review";
    saveRegistry(fixture.harness, registry);
    const needsReview = runCheck(fixture.root);
    assert.equal(
      needsReview.exitCode,
      1,
      "needs-review 仍声称适用，旧指纹要报错",
    );

    record.applicability = "obsolete";
    saveRegistry(fixture.harness, registry);
    const obsolete = runCheck(fixture.root);
    const obsoleteViolations = (obsolete.report.violations ?? []).filter(
      (entry) => entry.check === "verify",
    );
    assert.deepEqual(
      obsoleteViolations,
      [],
      `obsolete 证据不再声称适用，保留旧指纹不得阻断：${JSON.stringify(obsolete.report.violations)}`,
    );
  } finally {
    fixture.cleanup();
  }
});

test("证据物漂移即阻断：testFingerprint 与当前测试文件不一致必须报错", () => {
  const fixture = buildFixture();
  try {
    assert.equal(reconcileFixture(fixture.root), 0, "夹具基线登记失败");
    const registry = loadRegistry(fixture.harness);
    const testPath = "agent-harness/modules/m01.md";
    const actual = `sha256:${createHash("sha256")
      .update(readFileSync(path.join(fixture.root, testPath)))
      .digest("hex")}`;
    registry.verify = [
      ...(registry.verify ?? []),
      {
        object: "M01",
        kind: "V03",
        testPath,
        command: "fixture",
        environment: "fixture",
        fingerprint: registry.objects.M01.definition.fingerprint,
        testFingerprint:
          "sha256:0000000000000000000000000000000000000000000000000000000000000000",
        at: "2026-01-01T00:00:00Z",
        applicability: "current",
        note: "夹具证据（故意绑错证据物哈希）",
      },
    ];
    saveRegistry(fixture.harness, registry);

    const drifted = runCheck(fixture.root);
    assert.equal(drifted.exitCode, 1, "testFingerprint 漂移必须报错");
    assert.ok(
      (drifted.report.violations ?? []).some(
        (entry) =>
          entry.check === "verify" &&
          String(entry.message).includes("证据物已漂移"),
      ),
      JSON.stringify(drifted.report.violations),
    );

    const record = registry.verify.at(-1);
    record.testFingerprint = actual;
    saveRegistry(fixture.harness, registry);
    const rebound = runCheck(fixture.root);
    assert.equal(rebound.exitCode, 0, "证据物哈希一致不得报错");

    record.applicability = "obsolete";
    record.testFingerprint = "sha256:0000";
    saveRegistry(fixture.harness, registry);
    const obsolete = runCheck(fixture.root);
    assert.equal(obsolete.exitCode, 0, "obsolete 记录的证据物留痕不阻断");
  } finally {
    fixture.cleanup();
  }
});

test("架构变更缺独立复核时阻断", () => {
  const fixture = buildFixture();
  try {
    assert.equal(reconcileFixture(fixture.root), 0);
    writeFileSync(
      path.join(fixture.harness, "contracts/K01.md"),
      K01_BODY.replace(
        "不变量：冻结确认绑定版本。",
        "不变量：冻结确认绑定版本与权限。",
      ),
    );
    assert.throws(
      () =>
        execFileSync(
          process.execPath,
          [
            checker,
            "--root",
            fixture.root,
            "--reconcile",
            "--author",
            "fixture",
            "--classification",
            "architecture",
            "--reason",
            "架构变更夹具",
          ],
          { encoding: "utf8", stdio: "pipe" },
        ),
      (error) => error.status === 2,
      "缺少独立复核者须拒绝写入",
    );
  } finally {
    fixture.cleanup();
  }
});

test("历史通过不足支持变化后的当前验收（证据指纹过期即阻断）", () => {
  const { exitCode, report } = runFixture({
    objects: {
      K01: {
        kind: "contract",
        name: "contract",
        title: "夹具合同",
        owner: "M01",
        maturity: "defined",
        definition: { file: "contracts/K01.md", anchor: "K01" },
        verification: { state: "passed" },
      },
    },
  });
  assert.equal(exitCode, 1);
  assert.ok(
    hasViolation(report, "evidence") ||
      hasViolation(report, "contract-elements"),
    violationMessages(report, "evidence"),
  );
});

test("注册表损坏时返回基础设施失败而不是伪通过", () => {
  const fixture = buildFixture();
  try {
    writeFileSync(
      path.join(fixture.harness, "registry.json"),
      "<!-- iris:object FILE-REGISTRY kind=rules file=true -->\n{ broken json",
    );
    const check = runCheck(fixture.root);
    assert.equal(check.exitCode, 2, "注册表不可解析必须是基础设施失败");
    assert.ok((check.report.infrastructure ?? []).length > 0);

    // --reconcile 也不能在注册表损坏时继续写入
    let reconcileStatus = 0;
    try {
      execFileSync(
        process.execPath,
        [
          checker,
          "--root",
          fixture.root,
          "--reconcile",
          "--author",
          "fixture",
          "--classification",
          "refinement",
          "--reason",
          "fixture baseline",
        ],
        { encoding: "utf8", stdio: "pipe" },
      );
    } catch (error) {
      reconcileStatus = error.status ?? 1;
    }
    assert.equal(reconcileStatus, 2, "--reconcile 遇到损坏注册表必须失败");
  } finally {
    fixture.cleanup();
  }
});

test("托管文件缺失时返回基础设施失败", () => {
  const { exitCode, report } = runFixture({
    overrides: { "contracts/K01.md": null },
  });
  assert.equal(exitCode, 2);
  assert.ok((report.infrastructure ?? []).length > 0);
});

test("关系成环与未知端点阻断检查", () => {
  const { exitCode, report } = runFixture({
    objects: {
      K01: {
        kind: "contract",
        name: "contract",
        title: "夹具合同",
        owner: "M01",
        maturity: "defined",
        definition: { file: "contracts/K01.md", anchor: "K01" },
        depends_on: ["M01"],
      },
      M01: {
        kind: "module",
        name: "module",
        title: "夹具模块",
        owner: "M01",
        maturity: "defined",
        definition: { file: "modules/m01.md", anchor: "M01" },
        consumes: ["K99"],
      },
    },
  });
  assert.equal(exitCode, 1);
  assert.ok(hasViolation(report, "relations"));
});

test("review_regression_补充复核陈旧不能解封且有效替代不重复阻断", () => {
  const fixture = buildFixture();
  try {
    assert.equal(reconcileFixture(fixture.root), 0);
    writeFileSync(
      path.join(fixture.harness, "contracts/K01.md"),
      K01_BODY.replace(
        "不变量：冻结确认绑定版本。",
        "不变量：冻结确认绑定版本与身份。",
      ),
    );
    assert.equal(reconcileFixture(fixture.root, "architecture"), 0);
    const registry = loadRegistry(fixture.harness),
      change = registry.changes.at(-1),
      review = registry.reviews.find((r) => r.id === change.review);
    Object.assign(review, {
      author: "independent-reviewer",
      reviewer: change.author,
      conclusion: "synchronized",
      evidence: "real evidence",
      reason: "checked K01 current",
    });
    change.objects.push({
      id: "M01",
      fingerprintTo: registry.objects.M01.definition.fingerprint,
    });
    registry.reviews.push({
      ...review,
      id: "supplemental-review",
      objects: [
        {
          id: "M01",
          fingerprint: registry.objects.M01.definition.fingerprint,
          stale: true,
        },
      ],
    });
    saveRegistry(fixture.harness, registry);
    const report = runCheck(fixture.root).report;
    assert.ok(
      report.gaps.some((gap) => gap.object === "M01"),
      "supplemental stale review cannot clear M01 gap",
    );
    registry.reviews.at(-1).objects[0].stale = false;
    const historical = {
      ...review,
      id: "historical-self-review",
      author: change.author,
    };
    registry.reviews.push(historical);
    change.review = historical.id;
    saveRegistry(fixture.harness, registry);
    const released = runCheck(fixture.root);
    assert.equal(
      released.exitCode,
      0,
      JSON.stringify(released.report.violations),
    );
    assert.ok(!released.report.gaps.some((gap) => gap.object === "M01"));
    change.objects.push({ id: "K01#file" });
    const legacy = {
      id: "K01#file",
      fingerprint: registry.files["contracts/K01.md"].fingerprint,
    };
    review.objects.push(legacy);
    saveRegistry(fixture.harness, registry);
    const compatible = runCheck(fixture.root);
    assert.equal(
      compatible.exitCode,
      0,
      JSON.stringify(compatible.report.violations),
    );
    assert.ok(!compatible.report.gaps.some((gap) => gap.object === "K01#file"));
    legacy.fingerprint = registry.objects.K01.definition.fingerprint;
    saveRegistry(fixture.harness, registry);
    assert.equal(
      runCheck(fixture.root).exitCode,
      1,
      "历史容器不能误绑同名子对象指纹",
    );
  } finally {
    fixture.cleanup();
  }
});

function changeContract(fixture, text) {
  writeFileSync(
    path.join(fixture.harness, "contracts/K01.md"),
    K01_BODY.replace("不变量：冻结确认绑定版本。", text),
  );
}
