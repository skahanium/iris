import { readFileSync } from "node:fs";
import { join } from "node:path";

import { describe, expect, it } from "vitest";

const root = process.cwd();

function read(path: string): string {
  return readFileSync(join(root, path), "utf8");
}

function sourceBlock(source: string, start: string, end: string): string {
  const startIndex = source.indexOf(start);
  const endIndex = source.indexOf(end, startIndex);

  expect(
    startIndex,
    `missing source block start: ${start}`,
  ).toBeGreaterThanOrEqual(0);
  expect(endIndex, `missing source block end: ${end}`).toBeGreaterThan(
    startIndex,
  );
  return source.slice(startIndex, endIndex);
}

describe("阶段 0：Agent Run 契约基线", () => {
  it("将现有执行、回放与涉密边界记录为可回归的契约", () => {
    const architecture = read("ARCHITECTURE.md");
    const roadmap = read("ROADMAP.md");
    const ipcDocs = read("docs/ipc-api-reference.md");
    const routingDocs = read("docs/llm-routing.md");
    const runContract = read("src-tauri/src/ai_runtime/run_contract.rs");
    const runIntake = read("src-tauri/src/ai_runtime/run_intake.rs");
    const runEngine = [
      read("src-tauri/src/ai_runtime/run_engine/mod.rs"),
      read("src-tauri/src/ai_runtime/run_engine/recovery.rs"),
    ].join("\n");
    const classifiedSession = read(
      "src-tauri/src/ai_runtime/classified_session.rs",
    );
    const classifiedSecurityTests = read(
      "src-tauri/tests/classified_ai_security.rs",
    );
    const eventPayload = sourceBlock(
      runContract,
      "pub(crate) enum RunEventPayload",
      "/// Persisted, ordered and replayable event emitted for an Agent Run.",
    );

    expect(runContract).toContain("Direct");
    expect(runContract).toContain("ToolLoop");
    expect(runContract).toContain("Durable");
    expect(runEngine).toContain("Effort::Durable");
    expect(classifiedSession).toContain("CEF-encrypted file");

    expect(architecture).toContain("不包含工具参数或原始输出");
    expect(architecture).toContain("Direct 与 ToolLoop 不支持进程级续跑");
    expect(architecture).toContain(
      "通用 MCP 只开放另一条独立的 `external.read` 边界",
    );
    expect(ipcDocs).toContain("不包含工具参数或原始输出");
    expect(ipcDocs).toContain("Direct 与 ToolLoop 不支持进程级续跑");
    expect(ipcDocs).toContain("AssistantRunStartRequest.externalToolGrants");
    expect(routingDocs).toContain(
      "通用 MCP 只读工具走独立的 `external.read` 路径",
    );
    expect(routingDocs).toContain(
      "Composer 必须逐 Run 显式提交 binding ID/hash",
    );

    expect(roadmap).toContain("六阶段受控演进验收矩阵");
    expect(roadmap).toContain("阶段 0：契约校准与回归基线");
    expect(roadmap).toContain("阶段 5：可信且可解释的 Skills 激活");
    expect(roadmap).toContain("不构成发布版本承诺");

    expect(runContract).toContain("pub(crate) web_enabled: bool");
    expect(runIntake).toContain("request.web_enabled");
    expect(architecture).toContain("联网开关是 `web.search` 的唯一授权源");
    expect(roadmap).toContain(
      "阶段 0 基线门禁：`web_enabled` / `web.search` 是唯一授权来源",
    );

    expect(runContract).toContain("ApproveChange");
    expect(runContract).toContain("confirmation_id: String");
    expect(runContract).toContain("plan_hash: String");
    expect(ipcDocs).toContain("校验目标、计划 hash 与最新内容 hash");
    expect(roadmap).toContain(
      "Markdown Apply 写入必须经过用户确认，并校验 plan hash 与内容 hash",
    );

    expect(eventPayload).toContain("ToolStarted {");
    expect(eventPayload).toContain("ToolCompleted {");
    expect(eventPayload).not.toMatch(/\b(arguments|raw_output)\s*:/);
    expect(architecture).toContain("事件不包含工具参数或原始输出");
    expect(roadmap).toContain("持久化事件 DTO 不包含工具参数或原始输出");

    expect(classifiedSession).toContain("classified_io::encrypt_cef");
    expect(classifiedSecurityTests).toContain("has_csef_magic");
    expect(architecture).toContain("CEF 加密持久化边界");
    expect(roadmap).toContain("classified 隔离必须保持为 CEF 加密持久化边界");
  });

  it("将涉密 Run 的易失执行边界与 normal-domain 回放边界写为一致事实", () => {
    const architecture = read("ARCHITECTURE.md");
    const ipcDocs = read("docs/ipc-api-reference.md");
    const aiTypes = read("src/types/ai.ts");
    const assistantCommands = read(
      "src-tauri/src/commands/assistant_commands.rs",
    );
    const classifiedEphemeral = read(
      "src-tauri/src/ai_runtime/classified_ephemeral.rs",
    );
    const runContract = read("src-tauri/src/ai_runtime/run_contract.rs");
    const ipc = read("src/lib/ipc.ts");
    const runGetCommand = sourceBlock(
      assistantCommands,
      "pub async fn assistant_run_get",
      "/// Mint a short-lived capability",
    );

    expect(classifiedEphemeral).toContain(
      "This module deliberately owns no database or CEF handle",
    );
    expect(classifiedEphemeral).toContain(
      "Process-local storage for classified execution",
    );
    expect(classifiedEphemeral).toContain(
      "Return only lifecycle metadata for a transient classified Run",
    );
    expect(runGetCommand).toContain("SecurityDomain::Classified");
    expect(runGetCommand).toContain(".get(run_id)");
    expect(runGetCommand).toContain("None => Ok(None)");
    expect(aiTypes).toContain(
      "Omit only for a normal-domain session to recover its latest non-terminal Run",
    );
    expect(runContract).toContain(
      "Omit only for a normal-domain session to recover",
    );
    expect(runContract).toContain("sessions require a Run ID");
    expect(ipc).toContain("assistant_classified_run_take_result");

    expect(architecture).toContain("normal-domain Run 在 accepted 后持久化");
    expect(architecture).toContain("涉密 Run 仅在当前进程内易失执行");
    expect(architecture).toContain(
      "仅可在同一进程内按显式 run ID 读取无正文的易失快照与安全事件",
    );
    expect(ipcDocs).toContain(
      "只接受显式 `runId`，按该 ID 读取无正文的易失快照与安全事件",
    );
    expect(ipcDocs).toContain("不支持省略 `runId` 的“最近活动 Run”查询");
    expect(ipcDocs).toContain("`assistant_classified_run_take_result`");
  });

  it("冻结 Agent/RAG 可靠性修复的基线、阻断项与后续验收规格", () => {
    const roadmap = read("ROADMAP.md");
    const remediationPlan = read(
      "docs/superpowers/plans/2026-08-07-iris-agent-rag-reliability-remediation.md",
    );
    const baseline = JSON.parse(
      read("docs/eval/results/v1.2.18-agent-rag-stage0-baseline.json"),
    ) as {
      revision: string;
      deterministicRag: { metrics: { anySourceRecallAt5: number } };
      agentSmoke: { smokeStatus: string; completedCases: number };
      securityAudit: { highSeverityVulnerabilities: number };
    };

    expect(roadmap).toContain("Agent / RAG 可靠性修复优先级");
    expect(roadmap).toContain("v1.2.19 的新功能交付不应绕过这些门禁");
    expect(remediationPlan).toContain("MAX_COSINE_FALLBACK_CHUNKS");
    expect(remediationPlan).toContain("block_links");
    expect(remediationPlan).toContain("sqlite-vec 0.1.9");
    expect(remediationPlan).toContain(
      "sqlite_vec_v3_mirrors_all_canonical_caches_insert_update_and_delete",
    );
    expect(remediationPlan).toContain("semantic_anchor_embeddings_v2");
    expect(remediationPlan).toContain("regulation_embeddings_v2");
    expect(remediationPlan).toContain("searchAvailable");
    expect(remediationPlan).toContain("24 个闭环用例");
    expect(remediationPlan).toContain(
      "不新增远程向量数据库、重排序服务或长期记忆",
    );
    expect(baseline.revision).toBe("478ba3cdcbdb70f45ecade471a845b0d0b416acd");
    expect(baseline.deterministicRag.metrics.anySourceRecallAt5).toBe(0.96);
    expect(baseline.agentSmoke.smokeStatus).toBe("failed");
    expect(baseline.agentSmoke.completedCases).toBe(12);
    expect(baseline.securityAudit.highSeverityVulnerabilities).toBe(13);
    expect(remediationPlan).toContain("brace-expansion");
  });
});
