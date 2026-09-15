import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

import {
  AUDIT_HEALTH_FAILURE_HEADLINE,
  DIAGNOSTIC_QUERY_FAILURE_HEADLINE,
  attributionLabel,
  completenessLabel,
} from "@/lib/assistant-run-diagnostic";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

describe("C27 diagnostic query contract", () => {
  it("registers a session-bound diagnose command next to Run get", () => {
    const lib = read("src-tauri/src/lib.rs");
    const commands = read("src-tauri/src/commands/assistant_commands.rs");
    const contract = read("src-tauri/src/ai_runtime/run_contract.rs");
    const query = read("src-tauri/src/ai_runtime/diagnostic_query.rs");

    expect(lib).toContain(
      "commands::assistant_commands::assistant_run_diagnose",
    );
    expect(commands).toContain("pub async fn assistant_run_diagnose");
    expect(commands).toContain("diagnose_run(");
    expect(contract).toContain("pub struct AssistantRunDiagnoseRequest");
    expect(contract).toContain("pub(crate) run_id: String");
    expect(query).toContain("pub(crate) fn diagnose_run");
    expect(query).toContain("recovery_exhausted");
    expect(query).toContain("IssueClass::DiagnosticGap");
    expect(query).toContain("AttributionStatus::Suspected");
    expect(query).toContain("未登记工具");
    expect(query).toContain("HandshakeStart");
    expect(query).toContain("HandshakeEnd | BoundaryEventKind::MissingEnd");
  });

  it("exposes a typed ipc wrapper that cannot omit runId", () => {
    const ipc = read("src/lib/ipc.ts");
    const types = read("src/types/ai.ts");

    expect(ipc).toContain('invoke<DiagnosticReport>("assistant_run_diagnose"');
    expect(ipc).toContain("export async function assistantRunDiagnose");
    expect(types).toContain("export interface AssistantRunDiagnoseRequest");
    expect(types).toContain("export interface DiagnosticReport");
    expect(types).toContain("attributionStatus: AttributionStatus");
    expect(types).toContain("auditHealth: DiagnosticAuditHealth");
    const diagnoseRequest = types
      .split("export interface AssistantRunDiagnoseRequest")[1]
      ?.split("export type AttributionStatus")[0];
    expect(diagnoseRequest).toContain("runId: string");
    expect(diagnoseRequest).not.toContain("runId?: string");
  });

  it("enters diagnosis from the current answer or failure without copying a Run ID", () => {
    const list = read("src/components/ai/AiMessageList.tsx");
    const panel = read("src/components/ai/UnifiedAssistantPanel.impl.tsx");
    const failed = read("src/components/ai/AssistantRunCapabilityDegraded.tsx");
    const entry = read("src/components/ai/AssistantRunDiagnostic.tsx");

    expect(list).toContain("AssistantRunDiagnosticEntry");
    expect(list).toContain("session && m.runId");
    expect(panel).toContain("session={runSession}");
    expect(panel).toContain("AssistantRunWebVerificationFailed");
    expect(failed).toContain("runId={failure.diagnosticId}");
    expect(failed).toContain("诊断入口需要当前会话");
    expect(failed).not.toContain("诊断编号");
    expect(entry).toContain("查看诊断");
    expect(entry).toContain("attemptId");
    expect(entry).not.toContain("opacity-0");
  });

  it("keeps attribution, completeness, and audit health as separate labels", () => {
    expect(attributionLabel("confirmed")).toEqual({
      symbol: "●",
      text: "已证实",
    });
    expect(attributionLabel("unattributed").text).toBe("无法归因");
    expect(completenessLabel("persist-failed").text).toBe("审计写入失败");
    expect(DIAGNOSTIC_QUERY_FAILURE_HEADLINE).toContain("诊断查询失败");
    expect(DIAGNOSTIC_QUERY_FAILURE_HEADLINE).toContain("空成功");
    expect(DIAGNOSTIC_QUERY_FAILURE_HEADLINE).not.toContain("未发现问题");
    expect(AUDIT_HEALTH_FAILURE_HEADLINE).toContain("独立健康信号");
  });
});
