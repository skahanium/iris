#!/usr/bin/env node
/**
 * Read-only helper for web capability degradation (capability_degraded events).
 * See docs/ops/web-capability-degradation.md
 */
import { existsSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const home = os.homedir();

function parseArgs(argv) {
  const out = { db: null, runId: null, limit: 20 };
  for (let i = 2; i < argv.length; i += 1) {
    const arg = argv[i];
    if (arg === "--db" && argv[i + 1]) {
      out.db = argv[++i];
    } else if (arg === "--run-id" && argv[i + 1]) {
      out.runId = argv[++i];
    } else if (arg === "--limit" && argv[i + 1]) {
      out.limit = Number(argv[++i]);
    } else if (arg === "--help" || arg === "-h") {
      out.help = true;
    }
  }
  return out;
}

function candidateDbPaths() {
  const irisHome = process.env.IRIS_HOME;
  const irisData = process.env.IRIS_DATA_DIR;
  const paths = [];
  if (irisData) paths.push(path.join(irisData, "iris.db"));
  if (irisHome) paths.push(path.join(irisHome, "app-data", "iris.db"));
  paths.push(
    path.join(root, ".iris-dev", "app-data", "iris.db"),
    path.join(root, ".iris", "app-data", "iris.db"),
    path.join(home, ".iris", "app-data", "iris.db"),
    path.join(
      home,
      "Library",
      "Application Support",
      "com.iris.notes",
      "app-data",
      "iris.db",
    ),
  );
  if (process.env.LOCALAPPDATA) {
    paths.push(
      path.join(
        process.env.LOCALAPPDATA,
        "com.iris.notes",
        "app-data",
        "iris.db",
      ),
    );
  }
  return [...new Set(paths)];
}

function resolveDbPath(explicit) {
  if (explicit) {
    if (!existsSync(explicit)) {
      throw new Error(`数据库不存在: ${explicit}`);
    }
    return explicit;
  }
  const found = candidateDbPaths().find((p) => existsSync(p));
  if (!found) {
    throw new Error(
      `未找到 iris.db。请设置 IRIS_DATA_DIR 或使用 --db <path>。候选: ${candidateDbPaths().join(", ")}`,
    );
  }
  return found;
}

function sqlite(dbPath, sql) {
  const result = spawnSync("sqlite3", ["-header", "-column", dbPath, sql], {
    encoding: "utf8",
  });
  if (result.status !== 0) {
    throw new Error(result.stderr || "sqlite3 执行失败");
  }
  return result.stdout.trim();
}

const DOMAIN_HINT = {
  agent_run_web_provider_auth_failed: "mcp",
  agent_run_web_provider_timeout: "mcp",
  agent_run_web_provider_failed: "mcp",
  agent_run_web_evidence_invalid: "mcp",
  agent_run_mcp_unavailable: "mcp",
  agent_run_web_evidence_required: "harness",
};

function triageLine(code) {
  const domain = DOMAIN_HINT[code] ?? "unknown/llm";
  return `→ 建议故障域: ${domain}`;
}

function printHelp() {
  process.stdout.write(`用法:
  node scripts/diagnose-web-capability-degradation.mjs [--db <iris.db>] [--limit N]
  node scripts/diagnose-web-capability-degradation.mjs --run-id <run_id> [--db <iris.db>]

步骤对应 docs/ops/web-capability-degradation.md（抓取载荷 / 分流 / MCP 健康 / Run 工具事件）。
`);
}

const args = parseArgs(process.argv);
if (args.help) {
  printHelp();
  process.exit(0);
}

const dbPath = resolveDbPath(args.db);
process.stdout.write(`数据库: ${dbPath}\n\n`);

if (args.runId) {
  process.stdout.write(`=== Run ${args.runId}：工具与降级事件 ===\n`);
  process.stdout.write(
    `${sqlite(
      dbPath,
      `SELECT event_seq AS seq, event_type AS type,
        json_extract(payload_json, '$.capability') AS capability,
        json_extract(payload_json, '$.success') AS success,
        json_extract(payload_json, '$.code') AS code,
        json_extract(payload_json, '$.retryable') AS retryable,
        json_extract(payload_json, '$.attemptCount') AS attempt_count
       FROM agent_run_events
       WHERE run_id = '${args.runId.replace(/'/g, "''")}'
         AND event_type IN ('capability_degraded', 'tool_started', 'tool_completed')
       ORDER BY event_seq;`,
    )}\n\n`,
  );
  process.stdout.write(`=== Run ${args.runId}：是否注册 web evidence ===\n`);
  process.stdout.write(
    `${sqlite(
      dbPath,
      `SELECT COUNT(*) AS evidence_rows FROM session_evidence WHERE origin_run_id = '${args.runId.replace(/'/g, "''")}';`,
    )}\n`,
  );
  process.exit(0);
}

process.stdout.write("=== 最近 capability_degraded（步骤 1–2）===\n");
const rows = sqlite(
  dbPath,
  `SELECT run_id, event_seq AS seq, created_at,
    json_extract(payload_json, '$.code') AS code,
    json_extract(payload_json, '$.retryable') AS retryable,
    json_extract(payload_json, '$.attemptCount') AS attempt_count,
    json_extract(payload_json, '$.message') AS message
   FROM agent_run_events
   WHERE event_type = 'capability_degraded'
   ORDER BY created_at DESC
   LIMIT ${Number.isFinite(args.limit) ? args.limit : 20};`,
);
process.stdout.write(`${rows || "(无记录)"}\n\n`);

if (rows) {
  const codes = [...rows.matchAll(/\b(agent_run_[a-z0-9_]+)\b/g)].map(
    (m) => m[1],
  );
  const unique = [...new Set(codes)];
  if (unique.length > 0) {
    process.stdout.write("=== 按 code 分流（步骤 2）===\n");
    for (const code of unique) {
      process.stdout.write(`${code} ${triageLine(code)}\n`);
    }
    process.stdout.write("\n");
  }
}

process.stdout.write("=== web_evidence_provider_health（步骤 3）===\n");
process.stdout.write(
  `${
    sqlite(
      dbPath,
      `SELECT provider_id, consecutive_failures, last_failure_code, latency_ewma_ms, updated_at
     FROM web_evidence_provider_health
     ORDER BY updated_at DESC;`,
    ) || "(无记录)"
  }\n\n`,
);

process.stdout.write("=== Tracing 检索提示（步骤 6）===\n");
process.stdout
  .write(`在 Iris 后端日志中搜索（字段见 run_tool_loop / normal_run_service）:
  - "Run Web decision" → web_mode, web_reason, web_execution
  - "Run model-decided Web capability outcome" → web_failure_code, web_attempt_count, web_duration_bucket
  - circuit_breaker 中文熔断开/关消息 → provider 连续失败
`);
