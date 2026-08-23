#!/usr/bin/env node
import { createHash } from "node:crypto";
import { existsSync, readFileSync, readdirSync, statSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(scriptDir, "..");
const activeHarnessRoot = path.join(root, "agent-harness");
const harnessArchiveRoot = path.join(activeHarnessRoot, "archive");
const activeHarnessFiles = [
  "README.md",
  "01-authority-and-invariants.md",
  "02-current-state-and-debt.md",
  "03-target-architecture.md",
  "04-research-and-tool-contracts.md",
  "05-implementation-roadmap.md",
  "06-evaluation-performance-and-acceptance.md",
  "appendices/A-status-and-test-traceability.md",
  "appendices/B-current-fact-contract-matrix.md",
  "appendices/C-decisions-and-deferred.md",
].map((relativePath) => path.join(activeHarnessRoot, relativePath));

// ── CLI ────────────────────────────────────────────────────

const args = process.argv.slice(2);
let expectedMigrationGroups = null;
const forbiddenPhrases = [];

for (let i = 0; i < args.length; i += 1) {
  if (args[i] === "--expected-migration-group" && args[i + 1]) {
    expectedMigrationGroups = Number.parseInt(args[i + 1], 10);
    i += 1;
  } else if (args[i] === "--forbidden-phrase" && args[i + 1]) {
    forbiddenPhrases.push(args[i + 1]);
    i += 1;
  }
}

// ── Helpers ────────────────────────────────────────────────

const failures = [];

function fail(message) {
  failures.push(message);
}

function readJson(filePath) {
  return JSON.parse(readFileSync(filePath, "utf8"));
}

function walk(dir, predicate, shouldDescend = () => true) {
  const entries = [];
  try {
    for (const entry of readdirSync(dir)) {
      const full = path.join(dir, entry);
      const stat = statSync(full);
      if (
        stat.isDirectory() &&
        !entry.startsWith(".") &&
        shouldDescend(full, entry)
      ) {
        entries.push(...walk(full, predicate, shouldDescend));
      } else if (stat.isFile() && predicate(full)) {
        entries.push(full);
      }
    }
  } catch {
    // directory walk failure is fine
  }
  return entries;
}

// ── 1. Version consistency ─────────────────────────────────

function checkVersionConsistency() {
  const pkg = readJson(path.join(root, "package.json"));
  const cargoToml = readFileSync(
    path.join(root, "src-tauri", "Cargo.toml"),
    "utf8",
  );
  const tauriConf = readJson(path.join(root, "src-tauri", "tauri.conf.json"));

  const pkgVersion = pkg.version;
  const cargoMatch = cargoToml.match(/^version\s*=\s*"([^"]+)"/m);
  const cargoVersion = cargoMatch ? cargoMatch[1] : null;
  const tauriVersion = tauriConf.version;

  if (!pkgVersion) fail("package.json missing version field");
  if (!cargoVersion) fail("Cargo.toml missing version field");
  if (!tauriVersion) fail("tauri.conf.json missing version field");

  if (pkgVersion && cargoVersion && pkgVersion !== cargoVersion) {
    fail(
      `Version mismatch: package.json=${pkgVersion} vs Cargo.toml=${cargoVersion}`,
    );
  }
  if (pkgVersion && tauriVersion && pkgVersion !== tauriVersion) {
    fail(
      `Version mismatch: package.json=${pkgVersion} vs tauri.conf.json=${tauriVersion}`,
    );
  }

  const userAgentFile = path.join(
    root,
    "src-tauri",
    "src",
    "llm",
    "fetch_web_page.rs",
  );
  if (existsSync(userAgentFile) && pkgVersion) {
    const uaContent = readFileSync(userAgentFile, "utf8");
    if (!uaContent.includes(`Iris/${pkgVersion}`)) {
      fail(
        `User-Agent in llm/fetch_web_page.rs does not reference Iris/${pkgVersion}`,
      );
    }
  }
}

function checkReleaseDocumentationFacts() {
  const version = readJson(path.join(root, "package.json")).version;
  const currentVersion = `v${version}`;
  const facts = [
    {
      path: "README.md",
      required: [
        `当前开发版本**：${currentVersion}`,
        "Xenova/bge-small-zh-v1.5",
        "默认启用的 sqlite-vec",
      ],
      forbidden: ["AllMiniLML6V2", "v1.2.6 计划"],
    },
    {
      path: "docs/eval/semantic-search.md",
      required: [
        currentVersion,
        "Xenova/bge-small-zh-v1.5",
        "512 维",
        "macOS + Windows",
      ],
      forbidden: ["AllMiniLML6V2", "Rust cosine"],
    },
    {
      path: "docs/eval/rag-v2-broker-evaluation.md",
      required: [currentVersion, "macOS + Windows", "sqlite-vec"],
      forbidden: ["# v1.2.6"],
    },
    {
      path: "SECURITY.md",
      required: [`当前开发版本 \`${currentVersion}\``],
      forbidden: ["当前开发版本 `v1.2.6`"],
    },
    {
      path: "CONTRIBUTING.md",
      required: ["macOS ARM64", "Windows x64", "sqlite-vec"],
      forbidden: ["Ubuntu/Linux", "Linux 额外要求"],
    },
  ];

  for (const fact of facts) {
    const content = readFileSync(path.join(root, fact.path), "utf8");
    for (const required of fact.required) {
      if (!content.includes(required)) {
        fail(`${fact.path} is missing current release fact: ${required}`);
      }
    }
    for (const forbidden of fact.forbidden) {
      if (content.includes(forbidden)) {
        fail(`${fact.path} contains stale release fact: ${forbidden}`);
      }
    }
  }
}

function checkRagFixtureContract() {
  const version = readJson(path.join(root, "package.json")).version;
  const fixtureRoot = path.join(
    root,
    "docs",
    "eval",
    "fixtures",
    "rag-v2-vault",
  );
  const labelsPath = path.join(fixtureRoot, "labels.json");
  const metadataPath = path.join(fixtureRoot, "fixture-metadata.json");
  const labelsContent = readFileSync(labelsPath, "utf8");
  const labels = JSON.parse(labelsContent);
  const metadata = readJson(metadataPath);
  const fixtureDocumentFacts = [
    {
      path: "docs/eval/rag-v2-broker-evaluation.md",
      statement: `frozen at ${metadata.fixtureVersion}`,
    },
    {
      path: "docs/eval/semantic-search.md",
      statement: `冻结于 ${metadata.fixtureVersion} 的历史 fixture`,
    },
  ];

  if (metadata.schemaVersion !== "iris-rag-fixture-metadata-v1") {
    fail("rag-v2 fixture metadata must use iris-rag-fixture-metadata-v1");
  }
  if (metadata.fixtureStatus !== "historical_frozen") {
    fail("rag-v2 fixture metadata must mark the label set historical_frozen");
  }
  if (metadata.fixtureVersion !== labels.version) {
    fail(
      `rag-v2 fixture version mismatch: metadata=${metadata.fixtureVersion} vs labels=${labels.version}`,
    );
  }
  if (metadata.currentEvaluationVersion !== `v${version}`) {
    fail(
      `rag-v2 current evaluation version must be v${version}, got ${metadata.currentEvaluationVersion}`,
    );
  }
  const labelsSha256 = createHash("sha256").update(labelsContent).digest("hex");
  if (metadata.labelsSha256 !== labelsSha256) {
    fail("rag-v2 fixture metadata labelsSha256 does not match labels.json");
  }

  for (const document of fixtureDocumentFacts) {
    const content = readFileSync(path.join(root, document.path), "utf8");
    if (!content.includes(document.statement)) {
      fail(`${document.path} must describe the ${document.statement} fixture`);
    }
    if (!content.includes(`v${version}`)) {
      fail(
        `${document.path} must distinguish the current v${version} evaluation`,
      );
    }
  }
}

// ── 2. Migration count ─────────────────────────────────────

function checkMigrationCount() {
  const migrationsDir = path.join(root, "src-tauri", "migrations");
  const upFiles = readdirSync(migrationsDir).filter(
    (file) => file.endsWith(".sql") && !file.endsWith(".down.sql"),
  );
  const numbers = upFiles
    .map((file) => Number.parseInt(file.match(/^(\d+)_/)?.[1] ?? "0", 10))
    .filter(Boolean);
  const count = upFiles.length;
  const maxNumber = numbers.length > 0 ? Math.max(...numbers) : 0;

  const architecture = readFileSync(path.join(root, "ARCHITECTURE.md"), "utf8");
  const migrationLine = architecture
    .split("\n")
    .find((line) => line.includes("**") && line.includes("组**增量迁移"));
  const match = migrationLine?.match(
    /\*\*(\d+) 组\*\*增量迁移（`001` 至 `(\d+)`）/,
  );

  if (!match) {
    fail("ARCHITECTURE.md missing parseable migration count line");
    return;
  }

  const documentedCount = Number.parseInt(match[1], 10);
  const documentedMax = Number.parseInt(match[2], 10);
  const expectedCount = expectedMigrationGroups ?? count;
  if (documentedCount !== expectedCount || documentedMax !== maxNumber) {
    fail(
      `ARCHITECTURE.md migration count: docs say ${documentedCount} groups (001-${documentedMax}), actual is ${count} groups (001-${maxNumber})`,
    );
  }
}
function validateMarkdownLinks(filePath) {
  const content = readFileSync(filePath, "utf8");
  const linkRe = /\]\(([^)]+)\)/g;
  let match;
  while ((match = linkRe.exec(content)) !== null) {
    const rawTarget = match[1].trim().replace(/^<|>$/g, "");
    if (
      rawTarget.startsWith("#") ||
      /^(?:https?:|mailto:|app:)/i.test(rawTarget)
    ) {
      continue;
    }
    const relativeTarget = rawTarget.split("#", 1)[0];
    const target = path.resolve(path.dirname(filePath), relativeTarget);
    if (!existsSync(target)) {
      fail(
        `${path.relative(root, filePath)} links to missing path: ${rawTarget}`,
      );
    }
  }
}

function checkDocLinks() {
  validateMarkdownLinks(path.join(root, "docs", "README.md"));
  for (const filePath of activeHarnessFiles.filter(existsSync)) {
    validateMarkdownLinks(filePath);
  }
}

function checkAgentHarnessDocumentation() {
  const docsIndexPath = path.join(root, "docs", "README.md");
  const docsIndex = readFileSync(docsIndexPath, "utf8");
  if (!docsIndex.includes("../agent-harness/README.md")) {
    fail("docs/README.md must link the active Agent Harness entry");
  }

  for (const required of activeHarnessFiles) {
    if (!existsSync(required)) {
      fail(
        `active Agent Harness document is missing: ${path.relative(root, required)}`,
      );
    }
  }

  for (const retired of ["refactor", "structured-tools", "REFACTOR.md"]) {
    if (existsSync(path.join(root, retired))) {
      fail(`retired root Agent Harness path still exists: ${retired}`);
    }
  }

  for (const archived of [
    path.join(harnessArchiveRoot, "2026-08-pre-unification", "MANIFEST.md"),
    path.join(harnessArchiveRoot, "2026-08-pre-unification", "refactor"),
    path.join(
      harnessArchiveRoot,
      "2026-08-pre-unification",
      "structured-tools",
    ),
    path.join(harnessArchiveRoot, "2026-08-pre-unification", "REFACTOR.md"),
  ]) {
    if (!existsSync(archived)) {
      fail(
        `Agent Harness archive is incomplete: ${path.relative(root, archived)}`,
      );
    }
  }

  const historyLink = "archive/2026-08-pre-unification/MANIFEST.md";
  for (const filePath of activeHarnessFiles.filter(existsSync)) {
    const content = readFileSync(filePath, "utf8");
    if (/\]\((?:\.\.\/)*(?:refactor|structured-tools)\//.test(content)) {
      fail(`${path.relative(root, filePath)} links a retired root document`);
    }
    if (
      filePath !== activeHarnessFiles[0] &&
      /\]\([^)]*archive\//.test(content)
    ) {
      fail(
        `${path.relative(root, filePath)} treats the archive as an active reference`,
      );
    }
  }
  const harnessReadme = readFileSync(activeHarnessFiles[0], "utf8");
  if (!harnessReadme.includes(historyLink)) {
    fail(
      "agent-harness/README.md must retain the single archive history entry",
    );
  }
}

function checkRetiredArchitectureReferences() {
  const indexPath = path.join(root, "docs", "README.md");
  const indexContent = readFileSync(indexPath, "utf8");
  if (indexContent.includes("agent-harness-refactor")) {
    fail("docs/README.md must not link to a historical architecture directory");
  }

  const commandSources = [
    path.join(root, "src-tauri", "src", "lib.rs"),
    path.join(root, "src", "types", "ipc.ts"),
    path.join(root, "src", "lib", "ipc.ts"),
  ]
    .filter(existsSync)
    .map((file) => readFileSync(file, "utf8"))
    .join("\n");
  for (const retired of [
    "llm_providers",
    "version_cleanup_cmd",
    "document_title_audit_cmd",
    "skills_paths",
    "classified_ai_retrieval_clear",
  ]) {
    if (commandSources.includes(retired)) {
      fail(
        `retired IPC command remains in a current contract source: ${retired}`,
      );
    }
  }
}

// ── 4. Forbidden phrases ───────────────────────────────────

function isNegationContext(line) {
  return /(?:不|禁止|无|没有|不做|不含|排除|免)/.test(line);
}

function lineContainsPhrase(line, phrase) {
  return line.includes(phrase) && !isNegationContext(line);
}

function checkForbiddenPhrases() {
  const phrases = forbiddenPhrases.length > 0 ? forbiddenPhrases : [];

  const docFiles = walk(path.join(root, "docs"), (f) => f.endsWith(".md"));
  const excludedRootDirectories = new Set([
    "node_modules",
    "src-tauri",
    "src",
    ".git",
    ".worktrees",
    "iris-2.0-planning",
    "target",
  ]);
  const rootMdFiles = walk(
    root,
    (f) => f.endsWith(".md"),
    (full, entry) =>
      !excludedRootDirectories.has(entry) &&
      full !== harnessArchiveRoot &&
      !full.startsWith(`${harnessArchiveRoot}${path.sep}`),
  );

  const allFiles = [...docFiles, ...rootMdFiles];

  for (const filePath of allFiles) {
    const lines = readFileSync(filePath, "utf8").split("\n");
    for (let i = 0; i < lines.length; i += 1) {
      for (const phrase of phrases) {
        if (lineContainsPhrase(lines[i], phrase)) {
          const rel = path.relative(root, filePath);
          fail(`Forbidden phrase "${phrase}" found in ${rel}:${i + 1}`);
        }
      }
    }
  }

  // Check key docs for credential-manager promotion (not denial)
  for (const f of [
    path.join(root, "CONTRIBUTING.md"),
    path.join(root, "docs", "ipc-api-reference.md"),
    path.join(root, "docs", "ops", "performance-guide.md"),
  ]) {
    if (!existsSync(f)) continue;
    const lines = readFileSync(f, "utf8").split("\n");
    for (let i = 0; i < lines.length; i += 1) {
      if (lineContainsPhrase(lines[i], "OS 凭据管理器")) {
        fail(
          `${path.relative(root, f)}:${i + 1} — "OS 凭据管理器" (must say AES-256-GCM)`,
        );
      }
    }
  }

  // Verify Skills descriptions: if they mention URL/Git/external install, it must be in denial context
  for (const f of [
    path.join(root, "README.md"),
    path.join(root, "ROADMAP.md"),
    path.join(root, "ARCHITECTURE.md"),
  ]) {
    if (!existsSync(f)) continue;
    const lines = readFileSync(f, "utf8").split("\n");
    for (let i = 0; i < lines.length; i += 1) {
      const ln = lines[i];
      if (!ln.toLowerCase().includes("skill")) continue;
      if (
        lineContainsPhrase(ln, "URL") ||
        lineContainsPhrase(ln, "Git") ||
        lineContainsPhrase(ln, "external")
      ) {
        fail(
          `${path.relative(root, f)}:${i + 1} — Skills line references URL/Git/external outside denial context`,
        );
      }
    }
  }
}

// ── 5. IPC command index ───────────────────────────────────

function checkIpcIndex() {
  const ipcRefPath = path.join(root, "docs", "ipc-api-reference.md");
  if (!existsSync(ipcRefPath)) return;

  const ipcContent = readFileSync(ipcRefPath, "utf8");
  for (const command of [
    "embedding_scheduler_status",
    "embedding_scheduler_start",
    "embedding_scheduler_set_paused",
    "embedding_scheduler_set_foreground_busy",
  ]) {
    if (!ipcContent.includes(command)) {
      fail(`docs/ipc-api-reference.md missing ${command} entry`);
    }
  }
  if (!ipcContent.includes("EmbeddingIndexStatus")) {
    fail("docs/ipc-api-reference.md missing EmbeddingIndexStatus reference");
  }
}

// ── Run ─────────────────────────────────────────────────────

checkVersionConsistency();
checkReleaseDocumentationFacts();
checkRagFixtureContract();
checkMigrationCount();
checkDocLinks();
checkAgentHarnessDocumentation();
checkRetiredArchitectureReferences();
checkForbiddenPhrases();
checkIpcIndex();

if (failures.length > 0) {
  process.stderr.write(`docs:check FAILED (${failures.length} issue(s)):\n`);
  for (const f of failures) {
    process.stderr.write(`  ✗ ${f}\n`);
  }
  process.exit(1);
}

process.stdout.write("docs:check PASSED\n");
process.exit(0);
