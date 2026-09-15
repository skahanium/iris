/**
 * Agent Harness 检查器负例自测（X02）
 *
 * 覆盖 document.md §5 要求的检查器测试清单。每个用例在系统临时目录里生成一份
 * 最小夹具仓库（catalog.mjs + agent-harness/*），再以子进程运行真实检查器，
 * 断言退出码与违规类别。**夹具不写入仓库。**
 *
 * 运行：npm run agent-harness:test（node --test）
 */
import { test } from "node:test";
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdtempSync, mkdirSync, writeFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const checker = path.join(scriptDir, "agent-harness-check.mjs");

// ── 夹具构造 ─────────────────────────────────────────────────

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

// ── 基线 ─────────────────────────────────────────────────────

test("健康夹具通过检查（多消费者共享一个合同是合法的）", () => {
  const { exitCode, report } = runFixture();
  assert.equal(
    exitCode,
    0,
    `预期通过，实际违规：${JSON.stringify(report.violations ?? [], null, 1)}`,
  );
});

// ── 结构与登记 ───────────────────────────────────────────────

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

test("外围正文变化使子对象进入复核", () => {
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
  } finally {
    fixture.cleanup();
  }
});

// ── 成熟度与就绪 ─────────────────────────────────────────────

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

// ── 变更、复核与阻断 ─────────────────────────────────────────

test("一个问题关闭但另一阻断仍存在，工作包不能解封", () => {
  const { report } = runFixture({
    objects: {
      Q01: {
        kind: "issue",
        name: "Q01",
        title: "夹具问题",
        owner: "M01",
        maturity: "draft",
        definition: { file: "requirements/current-baseline.md", anchor: "Q01" },
        blocks: [{ id: "D01", at: "acceptance" }],
      },
      D01: {
        kind: "work",
        name: "work",
        title: "夹具工作包",
        owner: "M01",
        maturity: "defined",
        definition: { file: "decisions/README.md", anchor: "D01" },
        work: { state: "planned" },
        scope: ["N01", "Q01"],
        closes: [],
      },
    },
    overrides: {
      "implementation/D01.md": `# 夹具工作包

<!-- iris:object D01 kind=work owner=M01 file=true -->

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
    },
  });
  const ready = report.readiness?.D01;
  assert.ok(ready);
  assert.equal(ready.acceptanceReady, false);
  assert.ok(ready.acceptanceReasons.join(" ").includes("Q01"));
});

test("架构变更缺独立复核时阻断", () => {
  const { exitCode, report } = runFixture();
  assert.equal(exitCode, 0);
  const root = mkdtempSync(path.join(tmpdir(), "harness-review-"));
  try {
    // 用真实检查器的 --reconcile 生成一条架构变更记录（无复核者时应拒绝执行）
    const harness = path.join(root, "agent-harness");
    mkdirSync(harness, { recursive: true });
    mkdirSync(path.join(root, "docs"), { recursive: true });
    writeFileSync(
      path.join(root, "docs", "README.md"),
      "[agent-harness/README.md](../agent-harness/README.md)\n",
    );
    for (const [rel, content] of Object.entries({
      ...skeleton(),
      ...defaultBodies(),
    })) {
      const target = path.join(harness, rel);
      mkdirSync(path.dirname(target), { recursive: true });
      writeFileSync(target, content);
    }
    writeFileSync(
      path.join(harness, "catalog.mjs"),
      `export const sources = {};\nexport const files = ${JSON.stringify(FILES)};\n` +
        `export const fileContainers = ${JSON.stringify(BASE_CATALOG.fileContainers)};\n` +
        `export const objects = ${JSON.stringify(BASE_CATALOG.objects)};\n` +
        `export const tools = {};\nexport const toolPlacement = {};\nexport const toolIdByName = {};\n`,
    );
    writeFileSync(
      path.join(harness, "registry.json"),
      `<!-- iris:object FILE-REGISTRY kind=rules file=true -->\n${JSON.stringify(
        {
          schemaVersion: "iris-agent-harness-registry-v1",
          registryRevision: 0,
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
    let status = 0;
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
          "architecture",
          "--reason",
          "架构变更夹具",
        ],
        { encoding: "utf8", stdio: "pipe" },
      );
    } catch (error) {
      status = error.status ?? 1;
    }
    assert.equal(status, 2, "架构变更未指定复核者时必须按基础设施失败拒绝写入");
  } finally {
    rmSync(root, { recursive: true, force: true });
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
