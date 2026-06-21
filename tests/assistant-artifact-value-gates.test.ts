import { describe, expect, it } from "vitest";

import {
  artifactPassesValueGate,
  buildArtifactDraftsFromTaskResult,
} from "@/lib/assistant-artifact-tabs";
import type { AssistantArtifactDraft } from "@/types/assistant-artifact";

function draft(
  kind: AssistantArtifactDraft["kind"],
  payload: unknown,
): AssistantArtifactDraft {
  return {
    kind,
    title: "临时产物",
    sourceRequestId: "req-value-gate",
    payload,
  };
}

describe("assistant artifact value gates", () => {
  it("creates evidence_sources only when real evidence metadata exists", () => {
    const drafts = buildArtifactDraftsFromTaskResult({
      requestId: "research-1",
      runStatus: "completed",
      artifacts: [
        {
          kind: "evidence_sources",
          title: "证据来源",
          status: "ready",
          sourceTask: "research",
          evidenceCount: 2,
          payload: {
            schema: "research_state",
            sources: [{ title: "行业报告", freshness: "needs_check" }],
            conflicts: ["样本口径冲突"],
            evidence_gaps: ["缺少一手收入数据"],
          },
        },
      ],
    });

    expect(drafts).toHaveLength(1);
    expect(drafts[0]).toMatchObject({
      kind: "evidence_sources",
      title: "证据来源",
      sourceRequestId: "research-1",
    });
  });

  it("drops evidence_sources when evidence_count is zero and gaps are mechanical only", () => {
    expect(
      artifactPassesValueGate(
        draft("evidence_sources", {
          schema: "research_state",
          evidence_count: 0,
          evidence_gaps: ["未授权联网，未检索到可用 evidence/source"],
        }),
      ),
    ).toBe(false);
  });

  it("does not create task_process for ordinary completed tasks", () => {
    const drafts = buildArtifactDraftsFromTaskResult({
      requestId: "task-ordinary",
      runStatus: "completed",
      artifacts: [
        {
          kind: "task_process",
          title: "过程详情",
          status: "ready",
          sourceTask: "chat",
          evidenceCount: 0,
          payload: {
            schema: "task_process",
            status: "completed",
            steps: [
              {
                output_summary:
                  "assistant task completed; no process artifact generated for ordinary completion",
              },
            ],
          },
        },
      ],
    });

    expect(drafts).toEqual([]);
  });

  it("keeps task_process for confirmations, failures, pauses, and checkpointed long tasks", () => {
    const cases: Array<unknown> = [
      { schema: "task_process", status: "pending_confirmation" },
      { schema: "task_process", status: "failed", error_message: "工具失败" },
      { schema: "task_process", status: "paused_budget", checkpoint: "cp-1" },
      {
        schema: "task_process",
        status: "running",
        long_task: true,
        checkpoints: [{ id: "cp-1", summary: "完成第一阶段" }],
      },
    ];

    expect(
      cases.map((payload) =>
        artifactPassesValueGate(draft("task_process", payload)),
      ),
    ).toEqual([true, true, true, true]);
  });

  it("creates writing_change for patch, diff, insert, or replace candidates", () => {
    const payloads: Array<unknown> = [
      { schema: "writing_change", patches: [{ id: "patch-1" }] },
      { schema: "writing_change", diff: "@@ -1 +1 @@" },
      { schema: "writing_change", candidates: [{ type: "insert" }] },
      { schema: "writing_change", candidates: [{ type: "replace" }] },
    ];

    expect(
      payloads.map((payload) =>
        artifactPassesValueGate(draft("writing_change", payload)),
      ),
    ).toEqual([true, true, true, true]);
  });

  it("creates structured_result for organize, citation, and document issue results", () => {
    const drafts = buildArtifactDraftsFromTaskResult({
      requestId: "structured-1",
      runStatus: "completed",
      artifacts: [
        {
          kind: "structured_result",
          title: "整理建议",
          status: "ready",
          sourceTask: "organize",
          evidenceCount: 0,
          payload: {
            resultKind: "organize_suggestions",
            suggestions: [{ id: "org-1", reason: "路径应归档" }],
          },
        },
        {
          kind: "structured_result",
          title: "引用核查",
          status: "ready",
          sourceTask: "citation",
          evidenceCount: 0,
          payload: {
            resultKind: "citation_check",
            coverage: { supported: 1, missing: 1 },
          },
        },
        {
          kind: "structured_result",
          title: "文档问题清单",
          status: "ready",
          sourceTask: "document",
          evidenceCount: 0,
          payload: {
            resultKind: "document_issues",
            issues: ["缺少二级标题"],
          },
        },
      ],
    });

    expect(drafts.map((item) => item.kind)).toEqual([
      "structured_result",
      "structured_result",
      "structured_result",
    ]);
    expect(drafts.map((item) => item.title)).toEqual([
      "整理建议",
      "引用核查",
      "文档问题清单",
    ]);
  });

  it("drops structured_result drafts without substantive content", () => {
    expect(
      artifactPassesValueGate(
        draft("structured_result", { resultKind: "citation_check" }),
      ),
    ).toBe(false);
    expect(
      artifactPassesValueGate(
        draft("structured_result", {
          resultKind: "organize_suggestions",
          suggestions: [],
        }),
      ),
    ).toBe(false);
    expect(
      artifactPassesValueGate(
        draft("structured_result", {
          resultKind: "document_issues",
          issues: [],
        }),
      ),
    ).toBe(false);
  });
});
