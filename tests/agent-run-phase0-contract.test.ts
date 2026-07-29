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
    expect(architecture).toContain("MCP 当前只承载 Web capability mapping");
    expect(ipcDocs).toContain("不包含工具参数或原始输出");
    expect(ipcDocs).toContain("Direct 与 ToolLoop 不支持进程级续跑");
    expect(routingDocs).toContain("MCP 当前只承载 Web capability mapping");

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
});
