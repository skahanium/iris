#!/usr/bin/env node
/**
 * Agent Harness 文档体系检查器 (X01)
 *
 * 规则见 agent-harness/rules/objects.md（身份、标记、指纹、引用）
 *   与 agent-harness/rules/governance.md（状态、关系、复核、阻断）。
 *
 * 用法：
 *   node scripts/agent-harness-check.mjs                # 检查；结构违规退出码 1，基础设施失败退出码 2
 *   node scripts/agent-harness-check.mjs --json         # 机器可读输出（含 --registry 用于负例自测）
 *   node scripts/agent-harness-check.mjs --reconcile    # 接受内容变化：写回指纹与新修订，并生成变更记录
 *
 * 纪律（governance.md §8）：
 *   - 结构检查失败返回非零；执行基础设施失败也返回非零，两者用不同退出码；
 *   - 不捕获异常后返回空集合，不把“无法检查”解释为“没有问题”；
 *   - 只计算与报告，不静默修改文档。
 */
import { createHash } from "node:crypto";
import {
  existsSync,
  readFileSync,
  readdirSync,
  statSync,
  writeFileSync,
} from "node:fs";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, "..");

// ── CLI ───────────────────────────────────────────────────────

const argv = process.argv.slice(2);
const options = {
  json: argv.includes("--json"),
  reconcile: argv.includes("--reconcile"),
  root: repoRoot,
  registry: null,
  catalog: null,
  author: "unattributed",
  reason: null,
  classification: null,
  reviewer: null,
};

for (let i = 0; i < argv.length; i += 1) {
  const next = argv[i + 1];
  switch (argv[i]) {
    case "--json":
      break;
    case "--reconcile":
      break;
    case "--root":
      options.root = path.resolve(next);
      i += 1;
      break;
    case "--registry":
      options.registry = path.resolve(next);
      i += 1;
      break;
    case "--catalog":
      options.catalog = path.resolve(next);
      i += 1;
      break;
    case "--author":
      options.author = next;
      i += 1;
      break;
    case "--reason":
      options.reason = next;
      i += 1;
      break;
    case "--classification":
      options.classification = next;
      i += 1;
      break;
    case "--reviewer":
      options.reviewer = next;
      i += 1;
      break;
    default:
      break;
  }
}

const harnessRoot = path.join(options.root, "agent-harness");
const registryPath =
  options.registry ?? path.join(harnessRoot, "registry.json");
const catalogPath = options.catalog ?? path.join(harnessRoot, "catalog.mjs");
const archiveRoot = path.join(harnessRoot, "archive");
const docsIndexPath = path.join(options.root, "docs", "README.md");

// ── 结果收集 ──────────────────────────────────────────────────

const violations = [];
const infrastructure = [];
const warnings = [];
const reports = [];

const violation = (check, message) => violations.push({ check, message });
const broken = (check, message) => infrastructure.push({ check, message });

function readText(filePath, check) {
  try {
    return readFileSync(filePath, "utf8");
  } catch (error) {
    broken(
      check,
      `无法读取 ${path.relative(options.root, filePath)}: ${error.message}`,
    );
    return null;
  }
}

function readJson(filePath, check) {
  const text = readText(filePath, check);
  if (text === null) return null;
  // 登记表用首行标记声明文件级身份（JSON 不能承载注释）；解析前剥掉标记行。
  const payload = text
    .split("\n")
    .filter((line) => !line.startsWith("<!--"))
    .join("\n");
  try {
    return JSON.parse(payload);
  } catch (error) {
    broken(
      check,
      `${path.relative(options.root, filePath)} 不是合法 JSON: ${error.message}`,
    );
    return null;
  }
}

// ── 标记解析 ──────────────────────────────────────────────────

// 容器标识：单对象文件用对象自身 ID 作为文件级容器；多对象文件（评测系统、决定记录、
// 目录索引）用登记表显式指定的容器 ID。见 rules/objects.md §2.3。
const ID_PREFIXES = new Map([
  ["N", "requirement"],
  ["M", "module"],
  ["C", "component"],
  ["E", "eval"],
  ["T", "tool"],
  ["K", "contract"],
  ["L", "flow"],
  ["V", "evidence"],
  ["D", "work"],
  ["R", "decision"],
  ["P", "rules"],
  ["X", "test"],
  ["Q", "issue"],
  ["G", "issue"],
]);

const KINDS = new Set([
  "requirement",
  "module",
  "component",
  "eval",
  "tool",
  "contract",
  "flow",
  "evidence",
  "work",
  "decision",
  "rules",
  "issue",
  "test",
]);

const MARKER_FIELDS = new Set([
  "kind",
  "owner",
  "file",
  "name",
  "scope",
  "title",
]);

const START_RE = /^<!--\s*iris:object\s+(.*?)\s*-->$/;
const END_RE = /^<!--\s*iris:end\s+([A-Za-z0-9_-]+)\s*-->$/;
const FENCE_RE = /^\s*(```|~~~)/;

function parseAttributes(raw, filePath, lineNumber) {
  const tokens = raw.trim().split(/\s+/);
  const id = tokens.shift() ?? "";
  const rest = tokens.join(" ");
  const attributes = {};
  const attrRe = /([a-z_]+)=("[^"]*"|\[[^\]]*\]|[^\s]+)/g;
  let match;
  let consumed = "";
  while ((match = attrRe.exec(rest)) !== null) {
    consumed += match[0];
    const key = match[1];
    if (!MARKER_FIELDS.has(key)) {
      violation(
        "marker-fields",
        `${filePath}:${lineNumber} 标记含未登记字段 ${key}（允许：${[...MARKER_FIELDS].join("/")}）`,
      );
      continue;
    }
    let value = match[2];
    if (value.startsWith('"') && value.endsWith('"'))
      value = value.slice(1, -1);
    attributes[key] = value;
  }
  if (consumed.replace(/\s+/g, "") !== rest.replace(/\s+/g, "")) {
    violation(
      "marker-fields",
      `${filePath}:${lineNumber} 标记内容无法全部解析: ${raw}`,
    );
  }
  return { id, attributes };
}

/**
 * 解析一份托管文件里的对象块。返回 { objects, fileObject }。
 * 规则：标记独占一行、成对匹配、不嵌套、围栏代码块内不解析。
 */
function parseObjects(filePath, fileText) {
  const lines = fileText.split("\n");
  const objects = [];
  const stack = [];
  let inFence = false;
  let fence = null;

  for (let index = 0; index < lines.length; index += 1) {
    const line = lines[index];
    const fenceMatch = line.match(FENCE_RE);
    if (fenceMatch) {
      if (!inFence) {
        inFence = true;
        fence = fenceMatch[1];
      } else if (fence === fenceMatch[1]) {
        inFence = false;
        fence = null;
      }
      continue;
    }
    if (inFence) continue;

    const startMatch = line.match(START_RE);
    if (startMatch) {
      const pending = stack[stack.length - 1] ?? null;
      // 文件级容器允许跨越同级的子对象块（容器 = 外围正文 + 子对象的有序占位）。
      // 其他形式的嵌套仍然禁止。
      const containerMaySpan =
        stack.length === 1 && pending && pending.isFile === true;
      if (stack.length > 0 && !containerMaySpan) {
        violation(
          "marker-nesting",
          `${filePath}:${index + 1} 标记嵌套：${stack[stack.length - 1].id} 未结束就出现新对象块`,
        );
      }
      const { id, attributes } = parseAttributes(
        startMatch[1],
        filePath,
        index + 1,
      );
      const knowsContainer =
        Object.values(catalog?.files ?? {}).includes(id) ||
        Object.prototype.hasOwnProperty.call(catalog?.fileContainers ?? {}, id);
      const isContainerMarker =
        String(attributes.file) === "true" || knowsContainer;
      const declaredId = isContainerMarker
        ? null
        : /^([A-Za-z]+)[0-9]+$/.exec(id);
      if (
        !isContainerMarker &&
        (!declaredId || !ID_PREFIXES.has(declaredId[1]))
      ) {
        violation(
          "marker-id",
          `${filePath}:${index + 1} 未知对象 ID 前缀: ${id}`,
        );
      }
      if (!attributes.kind) {
        violation(
          "marker-fields",
          `${filePath}:${index + 1} ${id} 缺少 kind 字段`,
        );
      } else if (!KINDS.has(attributes.kind)) {
        violation(
          "marker-fields",
          `${filePath}:${index + 1} ${id} 的 kind=${attributes.kind} 不在允许集合内`,
        );
      } else if (
        attributes.file !== "true" &&
        declaredId &&
        ID_PREFIXES.has(declaredId[1]) &&
        ID_PREFIXES.get(declaredId[1]) !== attributes.kind
      ) {
        violation(
          "marker-kind",
          `${filePath}:${index + 1} ${id} 的 kind=${attributes.kind} 与前缀约定 ${ID_PREFIXES.get(declaredId[1])} 不符`,
        );
      }
      const entry = {
        id,
        kind: attributes.kind ?? null,
        owner: attributes.owner ?? null,
        name: attributes.name ?? null,
        isFile: attributes.file === "true",
        startLine: index + 1,
        endLine: null,
        startIndex: index,
        bodyLines: [],
        children: [],
        body: "",
        blockText: "",
        fingerprint: null,
      };
      objects.push(entry);
      stack.push(entry);
      continue;
    }

    const endMatch = line.match(END_RE);
    if (endMatch) {
      if (stack.length === 0) {
        violation(
          "marker-pairing",
          `${filePath}:${index + 1} 结束标记 ${endMatch[1]} 没有对应的开始标记`,
        );
        continue;
      }
      const open = stack.pop();
      if (open.id !== endMatch[1]) {
        violation(
          "marker-pairing",
          `${filePath}:${index + 1} 结束标记 ${endMatch[1]} 与开始标记 ${open.id} 不匹配`,
        );
      }
      open.endLine = index + 1;
      open.blockText = lines.slice(open.startLine - 1, index + 1).join("\n");
      continue;
    }

    if (stack.length > 0) {
      stack.forEach((entry) => entry.bodyLines.push(line));
    }
  }

  for (const entry of stack) {
    violation(
      "marker-pairing",
      `${filePath}:${entry.startLine} 对象 ${entry.id} 的开始标记没有结束标记`,
    );
  }

  // 子对象：除文件级对象外，其余对象都是同级子对象。
  const fileObjects = objects.filter((entry) => entry.isFile);
  const childObjects = objects.filter((entry) => !entry.isFile);
  if (fileObjects.length !== 1) {
    violation(
      "file-object",
      `${filePath} 必须有且只有一个文件级对象（file=true），当前 ${fileObjects.length} 个`,
    );
  }
  for (const child of childObjects) {
    for (const candidate of childObjects) {
      if (candidate === child) continue;
      if (
        candidate.startLine > child.startLine &&
        candidate.startLine < (child.endLine ?? Number.MAX_SAFE_INTEGER)
      ) {
        violation(
          "marker-nesting",
          `${filePath}:${child.startLine} 对象 ${child.id} 的范围包含 ${candidate.id}，不允许嵌套`,
        );
      }
    }
  }
  // 容器跨越子对象时，被跨越的对象的开始标记会落在容器块内；这不属于正文包含。

  const normalized = (text) =>
    text.replace(/\r\n?/g, "\n").replace(/\n+$/, "\n");

  for (const entry of childObjects) {
    entry.body = normalized(entry.bodyLines.join("\n"));
    entry.fingerprint = createHash("sha256")
      .update(`iris-object-v1\n${entry.id}\n${entry.kind ?? ""}\n${entry.body}`)
      .digest("hex")
      .slice(0, 32);
  }

  let fileObject = fileObjects[0] ?? null;
  if (fileObject) {
    const lines2 = fileText.split("\n");
    const start = fileObject.startLine; // 0-based index of the marker line
    const end = fileObject.endLine ?? lines2.length;
    const outer = [
      ...lines2.slice(0, start - 1),
      ...lines2.slice(start, end - 1),
      ...lines2.slice(end),
    ].join("\n");
    let withPlaceholders = outer;
    for (const child of childObjects) {
      const block = lines2.slice(child.startLine - 1, child.endLine).join("\n");
      withPlaceholders = withPlaceholders.replace(
        block,
        `[[child:${child.id}]]`,
      );
    }
    fileObject.body = normalized(withPlaceholders);
    fileObject.fingerprint = createHash("sha256")
      .update(
        `iris-file-v1\n${relativePosix(options.root, filePath)}\n${fileObject.body}`,
      )
      .digest("hex")
      .slice(0, 32);
  }

  // 容器标记可以跨越子对象，因此同一 ID 可能出现多个标记块；它们同属一个逻辑容器：
  // 正文按出现顺序拼接，指纹取第一个块。
  const containers = new Map();
  for (const entry of objects.filter((item) => item.isFile)) {
    const existing = containers.get(entry.id);
    if (existing) {
      existing.body = `${existing.body}\n${entry.body}`;
      continue;
    }
    containers.set(entry.id, entry);
  }
  if (fileObject) {
    const merged = containers.get(fileObject.id);
    if (merged) fileObject = merged;
  }
  if (containers.size > 1) {
    violation(
      "file-object",
      `${filePath} 有 ${containers.size} 个文件级对象，只允许一个`,
    );
  }
  const containerIds = new Set(containers.keys());
  const sameIdChildren = new Map();
  for (const child of childObjects) {
    if (!containerIds.has(child.id)) continue;
    sameIdChildren.set(child.id, child);
  }
  const distinctContainers = [...containers.keys()].filter(
    (id) => !sameIdChildren.has(id) || containers.get(id).id !== id,
  );
  if (distinctContainers.length > 1) {
    violation(
      "file-object",
      `${filePath} 有多对象容器 ${distinctContainers.join("、")}，缺少显式容器约定`,
    );
  }

  return { objects, fileObject, containers };
}

// ── 目录与文件发现 ────────────────────────────────────────────

function walk(dir) {
  const result = [];
  let entries;
  try {
    entries = readdirSync(dir);
  } catch (error) {
    broken(
      "walk",
      `无法列出目录 ${path.relative(options.root, dir)}: ${error.message}`,
    );
    return result;
  }
  for (const entry of entries) {
    const full = path.join(dir, entry);
    let stat;
    try {
      stat = statSync(full);
    } catch (error) {
      broken(
        "walk",
        `无法读取 ${path.relative(options.root, full)}: ${error.message}`,
      );
      continue;
    }
    if (stat.isDirectory()) {
      if (entry === "node_modules" || entry === ".git") continue;
      result.push(...walk(full));
    } else if (stat.isFile()) {
      result.push(full);
    }
  }
  return result;
}

// 工具文件不参与对象标记解析：登记表本身用首行标记声明身份，catalog 是纯代码。
const TOOLING_FILES = new Set(["catalog.mjs"]);

// 单对象文件的文件级容器与子对象共用 ID；登记表中用 `<ID>#file` 与 `<ID>` 区分。
const FILE_OBJECT_SUFFIX = "#file";
const isContainerRegistration = (id) => id.endsWith(FILE_OBJECT_SUFFIX);

function discoverManagedFiles(catalogFiles) {
  const discovered = new Set();
  if (!existsSync(harnessRoot)) {
    broken("discover", "agent-harness/ 不存在");
    return discovered;
  }
  for (const file of walk(harnessRoot)) {
    if (file.startsWith(`${archiveRoot}${path.sep}`)) continue;
    discovered.add(relativePosix(harnessRoot, file));
  }
  for (const rel of Object.keys(catalogFiles)) discovered.add(rel);
  return discovered;
}

// ── 合同要素（成熟度 defined 的判据） ────────────────────────

const CONTRACT_ELEMENTS = {
  module: [
    ["输入输出", /输入|接受哪些输入|接收/, /输出|交付哪些输出/],
    ["决定权", /决定权|决定范围|拥有.*权/],
    ["状态", /状态/],
    ["不变量", /不变量/],
    ["异常恢复", /异常|失败|恢复/],
    ["审计", /审计|诊断|事件/],
    ["源码落点", /源码落点|现有基础|落点/],
    ["兼容", /兼容/],
    ["测试", /测试|验证/],
  ],
  component: [
    ["输入输出", /输入/, /输出/],
    ["决定权", /决定权|决定|拥有/],
    ["状态", /状态/],
    ["不变量", /不变量|禁止/],
    ["异常恢复", /异常|失败|恢复/],
    ["审计", /审计|诊断|事件/],
    ["源码落点", /源码落点|现有基础|落点|文件/],
    ["兼容", /兼容/],
    ["测试", /测试|验证/],
  ],
  tool: [
    ["用途", /用途|语义/],
    ["参数消费", /参数/],
    ["授权", /授权|权限/],
    ["副作用", /副作用/],
    ["预算", /预算|额度/],
    ["幂等", /幂等/],
    ["取消", /取消/],
    ["失败反馈", /失败|错误/],
    ["暴露规则", /暴露|工具面/],
    ["源码落点", /源码落点|落点|文件/],
  ],
  contract: [
    ["接口", /接口|输入|输出/],
    ["状态转换", /状态|转换/],
    ["不变量", /不变量/],
    ["异常", /异常|失败|错误/],
  ],
  flow: [
    ["交接顺序", /顺序|交接|步骤/],
    ["引用合同", /K[0-9]{2}/],
  ],
  requirement: [["要求", /需求|要求/]],
  evidence: [["判据", /判据|证据|要求/]],
  work: [
    ["前置", /前置|依赖/],
    ["改动范围", /范围|改动/],
    ["回归证据", /证据|验收/],
    ["回退条件", /回退/],
  ],
  decision: [
    ["决定", /决定|采用/],
    ["理由", /理由|原因/],
  ],
  rules: [["规则", /规则|必须|不得/]],
  issue: [["事实", /事实|现象|确认/]],
  eval: [
    ["输入输出", /输入/, /输出/],
    ["判据", /判据|评分|标准/],
  ],
  test: [["覆盖", /覆盖|用例|检查/]],
};

function missingContractElements(kind, body) {
  const elements = CONTRACT_ELEMENTS[kind] ?? [];
  return elements
    .filter(([, ...patterns]) => !patterns.every((re) => re.test(body)))
    .map(([label]) => label);
}

// ── 主流程 ────────────────────────────────────────────────────

async function loadCatalog() {
  if (!existsSync(catalogPath)) {
    broken(
      "catalog",
      `catalog 不存在: ${path.relative(options.root, catalogPath)}`,
    );
    return null;
  }
  try {
    return await import(pathToFileURL(catalogPath).href);
  } catch (error) {
    broken("catalog", `catalog 无法加载: ${error.message}`);
    return null;
  }
}

function normalizeText(text) {
  return text.replace(/\r\n?/g, "\n").replace(/\n+$/, "\n");
}

/**
 * 托管文件的相对路径一律以 **POSIX 分隔符** 表示。
 *
 * `path.relative` 在 Windows 返回反斜杠、在 POSIX 返回正斜杠；文件级指纹把这个
 * 路径拼进哈希输入，若直接用平台结果，同一份内容会在不同平台算出不同指纹，
 * 于是「Windows 上全红、macOS 上全绿」。登记表只在一种平台上生成，因此这里固定
 * 分隔符，使指纹与运行平台无关。对象指纹不含路径，本来就不受此影响。
 */
function relativePosix(from, to) {
  return path.relative(from, to).split(path.sep).join("/");
}

/**
 * 修订号只在**内容指纹发生变化**时递增；首次登记为 1，取消定义回落到 0。
 * 对象移动（换文件、换行）不改变修订，但会更新位置并进入影响检查。
 */
function computeRevision(
  previous,
  fingerprint,
  found,
  previousFingerprint = null,
) {
  if (!found) return previous?.revision ?? 0;
  if (!previous) return 1;
  if (previousFingerprint === fingerprint) return previous.revision ?? 1;
  return (previous.revision ?? 1) + 1;
}

function sourceFingerprint(relPath, text) {
  return createHash("sha256")
    .update(`iris-file-v1\n${relPath}\n${normalizeText(text)}`)
    .digest("hex")
    .slice(0, 32);
}

function checkRelations(catalog, definitions) {
  const ids = new Set(Object.keys(catalog.objects));
  const EDGES = [
    "implements",
    "consumes",
    "verifies",
    "depends_on",
    "supersedes",
    "refs",
    "applies_to",
  ];
  for (const [id, entry] of Object.entries(catalog.objects)) {
    if (isContainerRegistration(id)) continue;
    for (const edge of EDGES) {
      for (const target of entry[edge] ?? []) {
        if (!ids.has(target)) {
          violation("relations", `${id} 的 ${edge} 指向不存在的对象 ${target}`);
        }
        if (target === id) {
          violation("relations", `${id} 的 ${edge} 指向自身`);
        }
      }
    }
    for (const edge of ["blocks"]) {
      for (const target of entry[edge] ?? []) {
        if (!ids.has(target.id)) {
          violation(
            "relations",
            `${id} 的 blocks 指向不存在的对象 ${target.id}`,
          );
        }
        if (target.at !== "start" && target.at !== "acceptance") {
          violation(
            "relations",
            `${id} 的 blocks(${target.id}) 必须注明 at=start 或 at=acceptance`,
          );
        }
      }
    }
    for (const target of entry.closes ?? []) {
      if (!ids.has(target)) {
        violation("relations", `${id} 的 closes 指向不存在的对象 ${target}`);
        continue;
      }
      if ((catalog.objects[target].kind ?? "") !== "issue") {
        violation("relations", `${id} 的 closes(${target}) 不是 issue 对象`);
      }
    }
    if (entry.kind !== "issue" && !entry.owner) {
      violation("relations", `${id} 缺少 owner（责任归属必须落到模块或组件）`);
    }
    if (entry.owner && !ids.has(entry.owner)) {
      violation("relations", `${id} 的 owner=${entry.owner} 不是登记对象`);
    }
    if (entry.kind === "work") {
      if (!(entry.scope ?? []).some((target) => target.startsWith("N"))) {
        violation(
          "relations",
          `${id} 的 scope 未引用任何需求（N*），工作包缺少需求依据`,
        );
      }
      for (const scoped of entry.scope ?? []) {
        if (!ids.has(scoped)) {
          violation("relations", `${id} 的 scope 指向未登记对象 ${scoped}`);
        }
      }
      if (!entry.work?.state) {
        violation("relations", `${id} 缺少 work.state`);
      }
    }
    if (entry.definition && !definitions.has(id)) {
      violation(
        "definition",
        `${id} 声明了正式定义 ${entry.definition.file}#${entry.definition.anchor}，但没有解析到对应对象块`,
      );
    }
    if (
      !entry.definition &&
      entry.maturity &&
      entry.maturity !== "registered"
    ) {
      violation(
        "definition",
        `${id} 的 maturity=${entry.maturity}，但没有 definition 位置（registered 以上必须有正式定义）`,
      );
    }
    if (entry.definition && entry.maturity === "registered") {
      violation(
        "definition",
        `${id} maturity=registered 时不得声明 definition`,
      );
    }
  }

  // depends_on / supersedes 无环
  for (const edge of ["depends_on", "supersedes"]) {
    const visit = (id, stack) => {
      if (stack.includes(id)) {
        violation("cycles", `${edge} 成环: ${[...stack, id].join(" → ")}`);
        return;
      }
      for (const target of catalog.objects[id]?.[edge] ?? []) {
        if (!ids.has(target)) continue;
        visit(target, [...stack, id]);
      }
    };
    for (const id of ids) visit(id, []);
  }
}

function computeBlocked(catalog) {
  const blocked = new Map();
  for (const [id, entry] of Object.entries(catalog.objects)) {
    for (const target of entry.blocks ?? []) {
      const list = blocked.get(target.id) ?? [];
      list.push({ issue: id, at: target.at });
      blocked.set(target.id, list);
    }
  }
  return blocked;
}

function computeReadiness(
  catalog,
  definitions,
  registryObjects,
  violationsOut,
) {
  const blocked = computeBlocked(catalog);
  const result = new Map();

  for (const [id, entry] of Object.entries(catalog.objects)) {
    if (entry.kind !== "work") continue;
    const state = entry.work?.state ?? "planned";
    const startReasons = [];
    const acceptanceReasons = [];

    for (const target of entry.depends_on ?? []) {
      const targetEntry = catalog.objects[target];
      if (!targetEntry) continue;
      if (targetEntry.kind === "work") {
        if ((targetEntry.work?.state ?? "planned") !== "done") {
          startReasons.push(`前置工作包 ${target} 尚未完成`);
        }
        continue;
      }
      if (
        targetEntry.maturity === "superseded" ||
        targetEntry.maturity === "retired"
      ) {
        startReasons.push(`前置 ${target} 已退役或已被替代`);
        continue;
      }
      if (targetEntry.maturity === "registered" || !definitions.has(target)) {
        startReasons.push(`前置 ${target} 尚无正式定义`);
      }
    }

    for (const blocker of blocked.get(id) ?? []) {
      if ((entry.closes ?? []).includes(blocker.issue)) continue;
      const blockerOpen =
        (registryObjects?.get(blocker.issue)?.state ?? "open") === "open";
      if (!blockerOpen) continue;
      if (blocker.at === "start") {
        startReasons.push(`被 ${blocker.issue} 阻断开始`);
      } else {
        acceptanceReasons.push(`被 ${blocker.issue} 阻断验收（at=acceptance）`);
      }
    }

    for (const scoped of entry.scope ?? []) {
      const scopedEntry = catalog.objects[scoped];
      if (!scopedEntry || scopedEntry.kind !== "issue") continue;
      if ((registryObjects?.get(scoped)?.state ?? "open") !== "open") continue;
      if ((entry.closes ?? []).includes(scoped)) continue;
      acceptanceReasons.push(
        `作用域内未决问题 ${scoped} 处于 open 且未被本工作包关闭`,
      );
    }

    for (const target of entry.depends_on ?? []) {
      const targetEntry = catalog.objects[target];
      if (!targetEntry || targetEntry.kind === "work") continue;
      if (targetEntry.kind === "issue") {
        if ((registryObjects?.get(target)?.state ?? "open") === "open") {
          acceptanceReasons.push(`前置问题 ${target} 处于 open`);
        }
        continue;
      }
      if (targetEntry.maturity !== "defined") {
        acceptanceReasons.push(
          `前置 ${target} 的文档成熟度为 ${targetEntry.maturity ?? "registered"}`,
        );
      }
    }

    const startReady =
      startReasons.length === 0 && state !== "blocked" && state !== "cancelled";
    const acceptanceReady = startReady && acceptanceReasons.length === 0;
    result.set(id, {
      state,
      startReady,
      acceptanceReady,
      startReasons,
      acceptanceReasons,
    });

    // 声明完成却缺少条件的，是硬违规：不能把未就绪写成已完成。
    if (state === "done" && !acceptanceReady && violationsOut) {
      violationsOut.push({
        check: "readiness",
        message: `${id} 声明 work.state=done，但验收条件不成立：${[
          ...startReasons,
          ...acceptanceReasons,
        ].join("；")}`,
      });
    }
    if (state === "active" && !startReady && violationsOut) {
      violationsOut.push({
        check: "readiness",
        message: `${id} 声明 work.state=active，但开始条件不成立：${startReasons.join("；")}`,
      });
    }
  }
  return result;
}

function checkReviews(catalog, registry, definitions, currentFingerprints) {
  const changes = registry?.changes ?? [];
  const reviews = registry?.reviews ?? [];
  const changeById = new Map(changes.map((entry) => [entry.id, entry]));
  const reviewById = new Map(reviews.map((entry) => [entry.id, entry]));

  for (const change of changes) {
    if (
      !["architecture", "refinement", "governance"].includes(
        change.classification,
      )
    ) {
      violation(
        "changes",
        `变更 ${change.id} 的 classification=${change.classification} 非法`,
      );
    }
    if (!change.reason || String(change.reason).trim().length < 4) {
      violation("changes", `变更 ${change.id} 缺少分类理由`);
    }
    if (!change.author) violation("changes", `变更 ${change.id} 缺少 author`);
    for (const object of change.objects ?? []) {
      // 文件级容器在权威 JSON 里以 `<ID>#file` 记录，它不是 catalog.objects 条目。
      if (isContainerRegistration(object.id)) continue;
      if (!catalog.objects[object.id]) {
        violation("changes", `变更 ${change.id} 涉及未登记对象 ${object.id}`);
      }
    }
    if (
      change.classification === "architecture" ||
      change.classification === "governance"
    ) {
      if (!change.review) {
        violation(
          "reviews",
          `架构/治理变更 ${change.id} 在解封前必须有独立复核记录`,
        );
        continue;
      }
      const review = reviewById.get(change.review);
      if (!review) {
        violation(
          "reviews",
          `变更 ${change.id} 引用的复核 ${change.review} 不存在`,
        );
        continue;
      }
      if (review.change !== change.id) {
        violation(
          "reviews",
          `复核 ${review.id} 的 change=${review.change} 与变更不匹配`,
        );
      }
      if (review.author && review.author === change.author) {
        violation(
          "reviews",
          `复核 ${review.id} 的作者与变更作者相同（架构变更的作者不能自行作出无影响结论）`,
        );
      }
      if (
        review.reviewer &&
        review.author &&
        review.reviewer === review.author
      ) {
        violation("reviews", `复核 ${review.id} 的 reviewer 与 author 相同`);
      }
      if (!review.reason || String(review.reason).trim().length < 4) {
        violation("reviews", `复核 ${review.id} 缺少判断理由`);
      }
      if (!review.evidence) {
        violation("reviews", `复核 ${review.id} 缺少可追溯评审记录`);
      }
      if (
        review.conclusion !== "no-impact" &&
        review.conclusion !== "synchronized"
      ) {
        violation(
          "reviews",
          `架构/治理变更 ${change.id} 的复核结论为 ${review.conclusion ?? "缺失"}，不能解封（需要 no-impact 或 synchronized）`,
        );
      }
      for (const object of review.objects ?? []) {
        const recorded = currentFingerprints.get(object.id);
        if (!recorded) {
          violation("reviews", `复核 ${review.id} 引用未登记对象 ${object.id}`);
          continue;
        }
        if (!object.fingerprint) continue;
        if (object.fingerprint !== recorded.fingerprint) {
          violation(
            "reviews",
            `复核 ${review.id} 绑定的是 ${object.id} 的旧指纹（复核之后对象又变化，不能解封）`,
          );
        }
      }
    }
  }

  for (const review of reviews) {
    if (!changeById.has(review.change)) {
      violation(
        "reviews",
        `复核 ${review.id} 引用的变更 ${review.change} 不存在`,
      );
    }
  }

  // 验证结论必须能追溯到绑定当前指纹的证据记录。
  for (const [id, entry] of Object.entries(catalog.objects)) {
    if (entry.verification?.state !== "passed") continue;
    const records = (registry?.verify ?? []).filter(
      (record) => record.object === id,
    );
    if (records.length === 0) {
      violation(
        "evidence",
        `${id} 声明 verification.state=passed，但没有 verify 证据记录（历史通过不足以支持当前验收）`,
      );
      continue;
    }
    const current = currentFingerprints.get(id)?.fingerprint;
    if (
      current &&
      records.every(
        (record) => record.fingerprint && record.fingerprint !== current,
      )
    ) {
      violation(
        "evidence",
        `${id} 的证据绑定的是旧指纹：当前版本不再适用（需重新验证或标为 needs-review）`,
      );
    }
  }

  for (const entry of registry?.verify ?? []) {
    if (!catalog.objects[entry.object]) {
      violation("verify", `verify 记录引用未登记对象 ${entry.object}`);
    }
    if (
      !["current", "needs-review", "obsolete"].includes(entry.applicability)
    ) {
      violation(
        "verify",
        `verify(${entry.object}) 的 applicability=${entry.applicability} 非法`,
      );
    }
    const recorded = currentFingerprints.get(entry.object);
    if (
      recorded &&
      entry.fingerprint &&
      entry.fingerprint !== recorded.fingerprint
    ) {
      violation(
        "verify",
        `verify(${entry.object}) 绑定的是旧指纹，当前版本不再适用`,
      );
    }
  }
}

function checkArchive() {
  if (!existsSync(archiveRoot)) return;
  for (const batch of readdirSync(archiveRoot)) {
    const batchDir = path.join(archiveRoot, batch);
    if (!statSync(batchDir).isDirectory()) continue;
    const manifestPath = path.join(batchDir, "MANIFEST.md");
    if (!existsSync(manifestPath)) {
      violation("archive", `归档目录缺少 MANIFEST.md: archive/${batch}`);
      continue;
    }
    const manifest = readText(manifestPath, "archive");
    if (manifest === null) continue;
    // MANIFEST 允许两种登记粒度：逐文件相对路径，或 `dir/**` 形式的整个子树。
    // 后者用于搬迁时整目录迁入、逐文件清单无信息增量的归档批次。
    const subtreePatterns = [
      ...manifest.matchAll(/([A-Za-z0-9._/-]+)\/\*\*/g),
    ].map((match) => match[1].replace(/\/$/, ""));
    for (const file of walk(batchDir)) {
      if (file === manifestPath) continue;
      const rel = relativePosix(batchDir, file);
      const coveredBySubtree = subtreePatterns.some(
        (prefix) => rel === prefix || rel.startsWith(`${prefix}/`),
      );
      if (
        !coveredBySubtree &&
        !manifest.includes(rel) &&
        !manifest.includes(path.basename(file))
      ) {
        violation(
          "archive",
          `归档文件未在 MANIFEST 登记: archive/${batch}/${rel}`,
        );
      }
    }
    for (const match of manifest.matchAll(/\]\(([^)]+)\)/g)) {
      const raw = match[1].trim().replace(/^<|>$/g, "").split("#", 1)[0];
      if (!raw || /^(?:https?:|mailto:|app:)/i.test(raw)) continue;
      const target = path.resolve(batchDir, raw);
      if (!target.startsWith(`${batchDir}${path.sep}`)) continue;
      if (!existsSync(target)) {
        violation(
          "archive",
          `MANIFEST 登记的归档文件不存在: archive/${batch}/${raw}`,
        );
      }
    }
  }
}

function checkDocsIndex() {
  if (!existsSync(docsIndexPath)) {
    broken("docs-index", "docs/README.md 不存在");
    return;
  }
  const index = readText(docsIndexPath, "docs-index");
  if (index === null) return;
  if (!index.includes("agent-harness/README.md")) {
    violation(
      "docs-index",
      "docs/README.md 必须登记 agent-harness/README.md 作为现行入口",
    );
  }
}

function checkSources(catalog, registry) {
  for (const [id, source] of Object.entries(catalog.sources ?? {})) {
    const full = path.join(options.root, source.path);
    if (!existsSync(full)) {
      broken("sources", `来源对象 ${id} 的文件不存在: ${source.path}`);
      continue;
    }
    const text = readText(full, "sources");
    if (text === null) continue;
    const recorded = registry?.sources?.[id];
    if (!recorded) {
      violation(
        "sources",
        `来源对象 ${id} 未登记指纹（运行 --reconcile 建立基线）`,
      );
      continue;
    }
    const fingerprint = sourceFingerprint(source.path, text);
    if (recorded.fingerprint !== fingerprint && !options.reconcile) {
      violation(
        "sources",
        `来源 ${id}（${source.path}）内容变化而登记未接受：登记 ${recorded.fingerprint}，当前 ${fingerprint}`,
      );
    }
    for (const ref of source.refs ?? []) {
      if (!catalog.objects[ref]) {
        violation("sources", `来源 ${id} 的 refs 指向未登记对象 ${ref}`);
      }
    }
  }
}

function writeRegistry(registry, next) {
  const ordered = {
    schemaVersion: next.schemaVersion,
    registryRevision: next.registryRevision,
    baseline: next.baseline,
    sources: next.sources,
    files: next.files,
    objects: Object.fromEntries(
      Object.keys(next.objects)
        .sort()
        .map((id) => [id, next.objects[id]]),
    ),
    verify: next.verify,
    changes: next.changes,
    reviews: next.reviews,
    issues: next.issues,
    notes: next.notes ?? [],
  };
  writeFileSync(
    registryPath,
    `<!-- iris:object FILE-REGISTRY kind=rules file=true -->\n${JSON.stringify(ordered, null, 2)}\n`,
    "utf8",
  );
  return registry;
}

// ── 执行 ──────────────────────────────────────────────────────

const catalog = await loadCatalog();

if (catalog && !infrastructure.length) {
  const registry = readJson(registryPath, "registry") ?? {
    schemaVersion: "iris-agent-harness-registry-v1",
    registryRevision: 0,
    sources: {},
    files: {},
    objects: {},
    verify: [],
    changes: [],
    reviews: [],
    issues: {},
  };

  const catalogFiles = catalog.files ?? {};
  const managedFiles = discoverManagedFiles(catalogFiles);
  const parsed = new Map();
  const definitions = new Map();
  // 容器对象（file=true）与其文件内子对象共同构成该文件的一个逻辑对象。
  const containersById = new Map();
  const containerFileOf = new Map();
  const containerFingerprints = new Map();
  const currentFingerprints = new Map();

  for (const rel of [...managedFiles].sort()) {
    const full = path.join(harnessRoot, rel);
    if (!existsSync(full) || !statSync(full).isFile()) {
      if (Object.prototype.hasOwnProperty.call(catalogFiles, rel)) {
        broken("files", `托管文件不存在: agent-harness/${rel}`);
      } else {
        broken("files", `磁盘清单包含已消失的文件: agent-harness/${rel}`);
      }
      continue;
    }
    const text = readText(full, "files");
    if (text === null) continue;
    if (TOOLING_FILES.has(rel)) continue;
    if (rel === "registry.json") {
      // 登记表是 JSON，不能承载注释；它用首行标记声明自己的文件级身份。
      const firstLine = text.split("\n", 1)[0];
      const marker = firstLine.match(
        /^<!--\s*iris:object\s+(FILE-REGISTRY)\s+kind=rules\s+file=true\s*-->$/,
      );
      const entry = {
        id: "FILE-REGISTRY",
        kind: "rules",
        owner: null,
        name: null,
        isFile: true,
        startLine: 1,
        endLine: null,
        bodyLines: text.split("\n").slice(1),
        children: [],
        body: "",
        fingerprint: null,
      };
      entry.body = normalizeText(entry.bodyLines.join("\n"));
      entry.fingerprint = createHash("sha256")
        .update(`iris-file-v1\n${rel}\n${entry.body}`)
        .digest("hex")
        .slice(0, 32);
      if (!marker) {
        if (existsSync(full)) {
          violation(
            "files",
            "agent-harness/registry.json 缺少首行文件级标记：<!-- iris:object FILE-REGISTRY kind=rules file=true -->",
          );
        }
        continue;
      }
      parsed.set(rel, {
        objects: [entry],
        fileObject: entry,
        containers: new Map([[entry.id, entry]]),
      });
      containersById.set(rel, entry);
      containerFileOf.set(entry.id, rel);
      continue;
    }
    if (!/\.(md|mjs|js|mts)$/.test(rel)) continue;
    const result = parseObjects(full, text);
    parsed.set(rel, result);
    for (const [id, entry] of result.containers ?? []) {
      if (containersById.has(rel)) continue; // parseObjects 已按 ID 归并
      containersById.set(rel, entry);
      containerFileOf.set(id, rel);
      containerFingerprints.set(id, entry.fingerprint);
    }
    const containerIdsInFile = new Set(
      (result.objects ?? [])
        .filter((entry) => entry.isFile)
        .map((entry) => entry.id),
    );
    for (const entry of result.objects) {
      if (entry.isFile) continue;
      const container = containersById.get(rel);
      const sharesWithContainer = container && container.id === entry.id;
      const other = definitions.get(entry.id);
      if (other && other.file !== rel) {
        violation(
          "duplicate-id",
          `对象 ID 跨文件重复: ${entry.id}（已定义于 ${other.file}）`,
        );
        continue;
      }
      if (other && other.file === rel && !sharesWithContainer) {
        violation(
          "duplicate-id",
          `同一文件内 ${entry.id} 出现两个定义块（agent-harness/${rel}）`,
        );
        continue;
      }
      definitions.set(entry.id, {
        ...entry,
        file: rel,
        container: sharesWithContainer ? container : null,
      });
    }
  }

  // 文件级对象登记
  const fileEntries = {};
  const registrationOf = (containerId, definitionsInFile) => {
    const shared = definitionsInFile.filter(
      (entry) => entry.id === containerId,
    );
    return shared.length === 1
      ? `${containerId}${FILE_OBJECT_SUFFIX}`
      : containerId;
  };
  for (const [rel, result] of parsed) {
    const declared = catalogFiles[rel];
    const actual = result.fileObject?.id ?? null;
    if (declared && actual && declared !== actual) {
      violation(
        "files",
        `agent-harness/${rel} 的文件级对象是 ${actual}，登记为 ${declared}`,
      );
    }
    if (declared && !actual && rel !== "registry.json") {
      violation(
        "files",
        `agent-harness/${rel} 缺少文件级对象（登记为 ${declared}）`,
      );
    }
    if (result.fileObject) {
      const children = result.objects.filter((entry) => !entry.isFile);
      fileEntries[rel] = {
        container: result.fileObject.id,
        registration: registrationOf(result.fileObject.id, children),
        fingerprint: result.fileObject.fingerprint,
      };
      if (declared && rel !== "registry.json") {
        // 登记表自身是自指的：--reconcile 写入指纹会改变它自己的外围正文，
        // 因此它的容器指纹不参与比对（内容变化仍由 changes 记录体现）。
        const recorded = registry.files?.[rel];
        if (
          recorded &&
          recorded.fingerprint !== result.fileObject.fingerprint &&
          !options.reconcile
        ) {
          violation(
            "fingerprint",
            `${rel} 的外围正文变化而登记未接受：登记 ${recorded.fingerprint}，当前 ${result.fileObject.fingerprint}`,
          );
        }
      }
    }
  }

  // 未登记对象
  for (const [id, entry] of definitions) {
    if (!catalog.objects[id]) {
      violation(
        "registration",
        `未登记对象 ${id}（${entry.file}:${entry.startLine}）；登记表必须覆盖全部对象标记`,
      );
    }
  }

  // 容器登记核对：单对象文件的容器必须有对应的文件级对象
  for (const [containerId, rel] of Object.entries(
    catalog.fileContainers ?? {},
  )) {
    const container = containersById.get(rel);
    if (!container) {
      violation(
        "files",
        `容器 ${containerId} 在 agent-harness/${rel} 没有文件级对象`,
      );
      continue;
    }
    if (container.id !== containerId) {
      violation(
        "files",
        `agent-harness/${rel} 的容器 ID 是 ${container.id}，登记为 ${containerId}`,
      );
    }
    currentFingerprints.set(`${containerId}${FILE_OBJECT_SUFFIX}`, {
      file: rel,
      line: container.startLine,
      fingerprint: container.fingerprint,
      kind: container.kind,
      name: null,
    });
  }

  // 定义位置核对（子对象必须在登记位置解析到）
  for (const [id, entry] of Object.entries(catalog.objects)) {
    if (isContainerRegistration(id)) continue;
    // 单对象文件的容器与定义是同一个逻辑对象：只有显式登记为「容器 ID」的
    // （多对象文件，见 fileContainers）才不能用容器标记充当定义。
    const containerOnlyIds = new Set(
      Object.keys(catalog.fileContainers ?? {}).filter(
        (key) => !Object.values(catalog.files ?? {}).includes(key),
      ),
    );
    const containerFallback = containerOnlyIds.has(id)
      ? undefined
      : containersById.get(entry.definition?.file);
    const found =
      definitions.get(id) ??
      (containerFallback
        ? { ...containerFallback, file: entry.definition?.file }
        : undefined);
    if (!entry.definition) {
      continue;
    }
    const expected = entry.definition;
    if (!found) continue;
    if (found.file !== expected.file) {
      violation(
        "definition",
        `${id} 的正式定义在 ${found.file}，登记为 ${expected.file}`,
      );
    }
    if (entry.kind && found.kind !== entry.kind) {
      violation(
        "definition",
        `${id} 的 kind=${found.kind} 与登记 ${entry.kind} 不符`,
      );
    }
    currentFingerprints.set(id, {
      file: found.file,
      line: found.startLine,
      fingerprint: found.fingerprint,
      kind: found.kind,
      name: found.name,
    });
  }

  checkRelations(catalog, definitions);

  // 成熟度：defined 的合同要素。
  // 正文取该对象的定义块，并并入其所在文件的容器正文——文件级对象与子对象共同构成
  // 一个逻辑对象，容器里的模块级说明同样是该对象合同的一部分。
  for (const [id, entry] of Object.entries(catalog.objects)) {
    if (entry.maturity !== "defined") continue;
    if (isContainerRegistration(id)) continue;
    const found = definitions.get(id);
    if (!found) continue;
    const container = containersById.get(found.file) ?? null;
    const mergedBody = container
      ? `${container.body}\n${found.body}`
      : found.body;
    // 必备要素核对面是对象块的完整文本（含小节标题）；对象指纹同样覆盖范围内全部正文（见 rules/objects.md §3.1）。
    const mergedText = container
      ? `${container.blockText ?? container.body}\n${found.blockText ?? found.body}`
      : (found.blockText ?? found.body);
    let missing = missingContractElements(entry.kind, mergedText);
    if (missing.length > 0) {
      // 容器解释性文字不得成为断点：正式定义块自身齐备即通过。
      const ownMissing = missingContractElements(
        entry.kind,
        found.blockText ?? found.body,
      );
      if (ownMissing.length === 0) missing = [];
    }
    if (missing.length > 0) {
      violation(
        "contract-elements",
        `${id} 声明 maturity=defined，但正文缺少必备要素: ${missing.join("、")}`,
      );
    }
  }

  // 文件级对象对其子对象的隐式依赖：文件外围变化则子对象进入复核
  for (const [rel, result] of parsed) {
    const recorded = registry.files?.[rel];
    if (!recorded) continue;
    if (recorded.fingerprint === result.fileObject?.fingerprint) continue;
    for (const child of result.objects.filter((entry) => !entry.isFile)) {
      const entry = catalog.objects[child.id];
      if (!entry || entry.maturity === "registered") continue;
      if (options.reconcile) continue;
      violation(
        "propagation",
        `${rel} 的外围正文变化，子对象 ${child.id} 进入复核（公共说明变化不能无人负责）`,
      );
    }
  }

  // 对象指纹：正文变化必须被检出。文件级指纹把子对象整块替换为占位符，
  // 因此只比对 files[] 会漏掉「外围不变、对象正文已变」的情况。
  if (!options.reconcile) {
    for (const [id, recorded] of currentFingerprints) {
      if (isContainerRegistration(id)) continue;
      const previous = registry.objects?.[id]?.definition?.fingerprint;
      if (!previous || previous === recorded.fingerprint) continue;
      violation(
        "fingerprint",
        `${id} 的对象正文变化而登记未接受：登记 ${previous}，当前 ${recorded.fingerprint}`,
      );
    }
  }

  checkReviews(catalog, registry, definitions, currentFingerprints);
  checkArchive();
  checkDocsIndex();
  checkSources(catalog, registry);

  const blocked = computeBlocked(catalog);
  const issueStates = new Map(
    Object.entries(registry.issues ?? {}).map(([id, entry]) => [id, entry]),
  );
  const readiness = computeReadiness(
    catalog,
    definitions,
    issueStates,
    violations,
  );

  if (options.reconcile) {
    // 接受变化是显式动作：必须给出作者与理由，且分类不能含糊。
    const classification = options.classification ?? "architecture";
    if (
      !["architecture", "refinement", "governance"].includes(classification)
    ) {
      broken(
        "reconcile",
        `--classification 只能是 architecture / refinement / governance，收到 ${classification}`,
      );
    }
    if (!options.reason || options.reason.trim().length < 8) {
      broken("reconcile", "--reconcile 必须用 --reason 说明变化的分类理由");
    }
    if (options.author === "unattributed") {
      broken("reconcile", "--reconcile 必须用 --author 记录变更作者");
    }
    if (
      (classification === "architecture" || classification === "governance") &&
      !options.reviewer
    ) {
      broken(
        "reconcile",
        "架构/治理变更必须用 --reviewer 指定独立复核者；未经复核的变化不得解封",
      );
    }
    const now = new Date().toISOString();
    const changed = [];
    const objectsOut = {};
    // 文件级容器也必须进入变更记录：容器指纹变化会让该文件内全部子对象进入复核
    // （见 rules/objects.md §2.4），静默吸收它等于把一次影响面记录抹掉。
    for (const [rel, recorded] of Object.entries(registry.files ?? {})) {
      // 登记表自身是自指的：写入就会改变它自己的容器指纹，不能据此制造变更记录。
      if (rel === "registry.json") continue;
      const current = fileEntries[rel];
      if (!current) continue;
      if (recorded.fingerprint === current.fingerprint) continue;
      const baseId = recorded.registration ?? recorded.container ?? rel;
      const containerId = isContainerRegistration(baseId)
        ? baseId
        : `${baseId}${FILE_OBJECT_SUFFIX}`;
      changed.push({
        id: containerId,
        from: 0,
        to: 0,
        fingerprintFrom: recorded.fingerprint,
        fingerprintTo: current.fingerprint,
      });
    }
    for (const [id, entry] of Object.entries(catalog.objects)) {
      const found = definitions.get(id);
      const previous = registry.objects?.[id];
      const previousFingerprint = previous?.definition?.fingerprint ?? null;
      const fingerprint = found?.fingerprint ?? null;
      if (found && previous && previousFingerprint !== fingerprint) {
        changed.push({
          id,
          from: previous.revision ?? 0,
          to: (previous.revision ?? 0) + 1,
          fingerprintFrom: previousFingerprint,
          fingerprintTo: fingerprint,
        });
      }
      objectsOut[id] = {
        kind: entry.kind,
        name: entry.name ?? null,
        title: entry.title ?? null,
        owner: entry.owner ?? null,
        maturity: entry.maturity ?? "registered",
        revision: computeRevision(
          previous,
          fingerprint,
          found,
          previousFingerprint,
        ),
        definition: found
          ? {
              file: found.file,
              anchor: id,
              line: found.startLine,
              endLine: found.endLine,
              fingerprint,
            }
          : null,
        implementation: entry.implementation ??
          previous?.implementation ?? { state: "unknown" },
        verification: entry.verification ??
          previous?.verification ?? { state: "none" },
        work: entry.work ?? previous?.work ?? null,
        scope: entry.scope ?? [],
        relations: Object.fromEntries(
          [
            "implements",
            "consumes",
            "verifies",
            "depends_on",
            "supersedes",
            "blocks",
            "closes",
            "applies_to",
          ]
            .filter((edge) => (entry[edge] ?? []).length > 0)
            .map((edge) => [edge, entry[edge]]),
        ),
        note: entry.note ?? previous?.note ?? null,
      };
    }

    const nextRegistry = {
      ...registry,
      registryRevision: (registry.registryRevision ?? 0) + 1,
      baseline: {
        architectureDefinition: "2670739f",
        reviewBaseline: "2d357038",
        recordedAt: now,
      },
      sources: Object.fromEntries(
        Object.entries(catalog.sources ?? {}).map(([id, source]) => {
          const full = path.join(options.root, source.path);
          const text = existsSync(full) ? readFileSync(full, "utf8") : "";
          return [
            id,
            {
              path: source.path,
              kind: source.kind,
              role: source.role,
              refs: source.refs ?? [],
              fingerprint: sourceFingerprint(source.path, text),
            },
          ];
        }),
      ),
      files: fileEntries,
      objects: objectsOut,
      verify: registry.verify ?? [],
      changes: registry.changes ?? [],
      reviews: registry.reviews ?? [],
      issues: Object.fromEntries(
        Object.entries(catalog.objects)
          .filter(([, entry]) => entry.kind === "issue")
          .map(([id]) => [
            id,
            { state: registry.issues?.[id]?.state ?? "open" },
          ]),
      ),
    };

    if (changed.length > 0) {
      const sequence = (nextRegistry.changes?.length ?? 0) + 1;
      const changeId = `CHG-${now.slice(0, 10)}-${sequence}`;
      const reviewId = `REV-${now.slice(0, 10)}-${sequence}`;
      const needsReview = classification !== "refinement";
      nextRegistry.changes = [
        ...(nextRegistry.changes ?? []),
        {
          id: changeId,
          at: now,
          author: options.author,
          classification,
          reason: options.reason,
          review: needsReview ? reviewId : null,
          objects: changed.map((entry) => ({
            id: entry.id,
            revisionFrom: entry.from,
            revisionTo: entry.to,
            fingerprintFrom: entry.fingerprintFrom,
            fingerprintTo: entry.fingerprintTo,
          })),
        },
      ];
      if (needsReview) {
        nextRegistry.reviews = [
          ...(nextRegistry.reviews ?? []),
          {
            id: reviewId,
            change: changeId,
            at: now,
            author: options.reviewer,
            reviewer: options.author,
            conclusion: "needs-reverification",
            reason: options.reason,
            evidence: "--reconcile 只登记待复核，不能作为独立复核完成依据",
            objects: changed.map((entry) => ({
              id: entry.id,
              revision: entry.to,
              fingerprint: entry.fingerprintTo,
            })),
          },
        ];
      }
    }

    writeRegistry(registry, nextRegistry);
    if (changed.length > 0) {
      warnings.push(
        `已接受 ${changed.length} 个对象的内容变化并生成变更记录（仍需独立复核）: ${changed
          .map((entry) => entry.id)
          .join(", ")}`,
      );
    }
  }

  const report = {
    ok: violations.length === 0 && infrastructure.length === 0,
    checks: {
      managedFiles: managedFiles.size,
      objects: Object.keys(catalog.objects).length,
      definitions: definitions.size,
      sources: Object.keys(catalog.sources ?? {}).length,
    },
    readiness: Object.fromEntries([...readiness.entries()]),
    blocked: Object.fromEntries(
      [...blocked.entries()].map(([id, entries]) => [
        id,
        entries.map((entry) => `${entry.issue}@${entry.at}`),
      ]),
    ),
    violations,
    infrastructure,
    warnings,
  };

  reports.push(report);
}

if (infrastructure.length > 0) {
  if (options.json) {
    process.stdout.write(
      `${JSON.stringify({ ok: false, infrastructure, violations, warnings }, null, 2)}\n`,
    );
  } else {
    process.stderr.write(
      `agent-harness:check INFRASTRUCTURE FAILURE（${infrastructure.length} 项；这不等于“发现规范问题”）:\n`,
    );
    for (const entry of infrastructure) {
      process.stderr.write(`  ✗ [${entry.check}] ${entry.message}\n`);
    }
  }
  process.exit(2);
}

// ── 输出与退出码 ──────────────────────────────────────────────
// 单一输出路径：结构违规返回 1，基础设施失败返回 2，二者都完整执行过检查循环时分别报告。

if (options.json) {
  process.stdout.write(
    `${JSON.stringify(
      {
        ok: violations.length === 0 && infrastructure.length === 0,
        infrastructure,
        violations,
        warnings,
        readiness: reports[0]?.readiness ?? {},
        blocked: reports[0]?.blocked ?? {},
        checks: reports[0]?.checks ?? {},
      },
      null,
      2,
    )}\n`,
  );
} else {
  for (const warning of warnings) process.stdout.write(`  ! ${warning}\n`);
  if (violations.length > 0) {
    process.stderr.write(
      `agent-harness:check FAILED（${violations.length} 项违规）:\n`,
    );
    for (const entry of violations) {
      process.stderr.write(`  ✗ [${entry.check}] ${entry.message}\n`);
    }
  } else {
    const checks = reports[0]?.checks;
    process.stdout.write(
      checks
        ? `agent-harness:check PASSED（${checks.managedFiles} 个托管文件，${checks.definitions} 个已定义对象，${checks.objects} 个登记对象）\n`
        : "agent-harness:check PASSED\n",
    );
  }
}

if (infrastructure.length > 0) process.exit(2);
if (violations.length > 0) process.exit(1);
process.exit(0);
