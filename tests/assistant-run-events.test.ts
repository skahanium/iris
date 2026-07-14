import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

import {
  createAssistantRunEventState,
  reduceAssistantRunEvent,
  replayAssistantRunEvents,
} from "@/lib/assistant-run-events";
import type {
  AssistantRunAccepted,
  AssistantRunControlRequest,
  AssistantRunEvent,
  AssistantRunGetRequest,
  AssistantRunStartRequest,
  EvidenceRef,
  ExecutionEnvelope,
} from "@/types/ai";

const runId = "run-001";
const session = { domain: "normal", sessionKey: "session-key-001" } as const;

type AssistantRunEventFor<Type extends AssistantRunEvent["type"]> = Extract<
  AssistantRunEvent,
  { type: Type }
>;

function event<Type extends AssistantRunEvent["type"]>(
  seq: number,
  type: Type,
  payload: AssistantRunEventFor<Type>["payload"],
  stateVersion = seq,
): AssistantRunEventFor<Type> {
  return {
    runId,
    seq,
    stateVersion,
    type,
    timestamp: "2026-07-13T00:00:00.000Z",
    payload,
  } as AssistantRunEventFor<Type>;
}

function reduce(events: readonly AssistantRunEvent[]) {
  return events.reduce(
    reduceAssistantRunEvent,
    createAssistantRunEventState(runId),
  );
}

describe("Assistant Run 前端合同", () => {
  it("keeps model overrides and structured run diagnostics forward-compatible", () => {
    const request = {
      clientRequestId: "client-request-routing-001",
      message: "Find current MCP guidance",
      explicitReferences: [],
      webEnabled: true,
      securityDomain: "normal",
      modelOverride: { providerId: "openai", modelId: "gpt-5" },
    } satisfies AssistantRunStartRequest;

    const diagnostics = [
      {
        kind: "provider_switched",
        providerId: "openai",
        modelId: "gpt-5",
        reasonCode: "transient_failure",
      },
      {
        kind: "confirmation_required",
        confirmationId: "confirmation-routing-001",
        planHash: "sha256:plan",
        summary: "Update one note",
        effect: "apply",
        targets: [
          { kind: "note", label: "notes/agent.md", risk: "bounded_write" },
        ],
        expiresAt: "2026-07-15T00:00:00.000Z",
      },
      {
        kind: "failed",
        code: "agent_run_no_capable_model",
        message: "No model satisfies the requested capabilities.",
      },
      {
        kind: "failed",
        code: "agent_run_web_provider_timeout",
        message: "联网证据服务响应超时。",
      },
      {
        kind: "failed",
        code: "agent_run_web_provider_failed",
        message: "联网证据服务暂时不可用。",
      },
      {
        kind: "failed",
        code: "agent_run_web_evidence_invalid",
        message: "联网证据服务未返回可用结果。",
      },
    ] satisfies AssistantRunEvent["payload"][];

    expect(request.modelOverride?.modelId).toBe("gpt-5");
    expect(diagnostics).toHaveLength(6);
  });

  it("以正交 ExecutionEnvelope 保留 Run 的安全执行边界", () => {
    const envelope = {
      effect: "draft",
      context: "explicit_references",
      freshness: "web_required",
      effort: "tool_loop",
      securityDomain: "normal",
      risk: "read_only",
      modalities: ["text"],
      materialNeeds: ["authority", "exemplar"],
      requiredCapabilities: ["model.respond", "vault.search"],
      explicitConstraints: [
        { kind: "do_not_modify", value: "true" },
        { kind: "local_only", value: null },
      ],
    } satisfies ExecutionEnvelope;

    expect(envelope.materialNeeds).toEqual(["authority", "exemplar"]);
    expect(envelope.requiredCapabilities).toEqual([
      "model.respond",
      "vault.search",
    ]);
  });

  it("EvidenceRef 只包含稳定标识与安全展示元数据", () => {
    const evidence = {
      evidenceId: "evidence-001",
      sourceKind: "web",
      displayLabel: "[1] 官方来源",
      title: "公开标题",
      stale: false,
    } satisfies EvidenceRef;

    expect(Object.keys(evidence).sort()).toEqual([
      "displayLabel",
      "evidenceId",
      "sourceKind",
      "stale",
      "title",
    ]);
  });

  it("以判别联合表达全部安全事件载荷", () => {
    const payloads = [
      { kind: "accepted", turnId: "turn-001", sessionKey: "session-key-001" },
      { kind: "stage_changed", state: "preparing", stage: "正在准备" },
      { kind: "content_delta", delta: "可展示内容" },
      {
        kind: "tool_started",
        capability: "vault.search",
        toolCallId: "tool-001",
      },
      {
        kind: "tool_completed",
        capability: "vault.search",
        toolCallId: "tool-001",
        summary: "已找到 2 条资料",
      },
      {
        kind: "confirmation_required",
        confirmationId: "confirmation-001",
        planHash: "sha256:plan",
        summary: "将更新 1 个文件",
      },
      {
        kind: "permission_denied",
        code: "agent_run_permission_denied",
        message: "未授权该操作",
      },
      {
        kind: "provider_switched",
        providerId: "provider-002",
        reason: "transient_failure",
      },
      { kind: "evidence_registered", evidenceId: "evidence-001" },
      { kind: "paused", reason: "等待用户" },
      { kind: "resumed", reason: "用户已继续" },
      { kind: "completed", messageId: "message-001" },
      {
        kind: "failed",
        code: "agent_run_provider_unavailable",
        message: "模型暂不可用",
      },
      { kind: "cancelled", reason: "用户取消" },
    ] satisfies AssistantRunEvent["payload"][];

    expect(payloads).toHaveLength(14);
  });

  it("不允许 event type 与判别载荷 kind 脱节", () => {
    const mismatched: AssistantRunEvent = {
      runId,
      seq: 1,
      stateVersion: 1,
      type: "tool_started",
      timestamp: "2026-07-13T00:00:00.000Z",
      // @ts-expect-error `tool_started` must carry a `tool_started` payload.
      payload: { kind: "content_delta", delta: "不应被当作工具事件" },
    };

    expect(mismatched.type).toBe("tool_started");
  });

  it("以不携带 scene、intent 或当前文档的 Start DTO 表达显式边界", () => {
    const request = {
      clientRequestId: "client-request-001",
      session,
      message: "请仅根据这段选区起草摘要",
      contentParts: [{ type: "text", text: "请仅根据这段选区起草摘要" }],
      explicitReferences: [],
      explicitAction: {
        effect: "draft",
        target: { referenceId: "ref-001", contentHash: "sha256:target" },
        selectionSnapshot: {
          referenceId: "ref-001",
          contentHash: "sha256:selection",
          utf8Range: { start: 0, end: 12 },
          text: "显式选区正文",
        },
      },
      webEnabled: false,
      securityDomain: "normal",
    } satisfies AssistantRunStartRequest;

    expect(Object.keys(request).sort()).toEqual([
      "clientRequestId",
      "contentParts",
      "explicitAction",
      "explicitReferences",
      "message",
      "securityDomain",
      "session",
      "webEnabled",
    ]);
  });

  it("以安全会话引用、接受响应和幂等 control DTO 表达执行合同", () => {
    const accepted = {
      runId,
      turnId: "turn-001",
      session,
      state: "accepted",
      stateVersion: 1,
    } satisfies AssistantRunAccepted;
    const approve = {
      session,
      runId,
      expectedStateVersion: 4,
      action: {
        type: "approve_change",
        confirmationId: "confirmation-001",
        planHash: "sha256:plan",
      },
    } satisfies AssistantRunControlRequest;
    const cancel = {
      session,
      runId,
      expectedStateVersion: 4,
      action: { type: "cancel" },
    } satisfies AssistantRunControlRequest;
    const get = { session, runId } satisfies AssistantRunGetRequest;

    expect(accepted.state).toBe("accepted");
    expect(approve.action.type).toBe("approve_change");
    expect(cancel.action.type).toBe("cancel");
    expect(get.session.domain).toBe("normal");
  });

  it("在阶段 4 通过 ipc 边界导出 Run 合同并提供类型安全调用", () => {
    const ipc = readFileSync("src/lib/ipc.ts", "utf8");

    expect(ipc).toContain("export type {");
    expect(ipc).toContain("AssistantRunStartRequest");
    expect(ipc).toContain('"assistant_run_start"');
  });
});

describe("Assistant Run 事件归约", () => {
  it("preserves structured confirmation and provider diagnostics while accepting legacy events", () => {
    const state = reduce([
      event(1, "accepted", {
        kind: "accepted",
        turnId: "turn-001",
        sessionKey: "session-key-001",
      }),
      event(2, "stage_changed", {
        kind: "stage_changed",
        state: "preparing",
        stage: "Preparing",
      }),
      event(3, "stage_changed", {
        kind: "stage_changed",
        state: "running",
        stage: "Running",
      }),
      event(4, "provider_switched", {
        kind: "provider_switched",
        providerId: "openai",
        modelId: "gpt-5",
        reasonCode: "transient_failure",
      }),
      event(5, "confirmation_required", {
        kind: "confirmation_required",
        confirmationId: "confirmation-001",
        planHash: "sha256:plan",
        summary: "Update one note",
        effect: "apply",
        targets: [
          { kind: "note", label: "notes/agent.md", risk: "bounded_write" },
        ],
        expiresAt: "2026-07-15T00:00:00.000Z",
      }),
    ]);

    expect(state.provider).toEqual({
      providerId: "openai",
      modelId: "gpt-5",
      reasonCode: "transient_failure",
    });
    expect(state.pendingConfirmation).toMatchObject({
      confirmationId: "confirmation-001",
      effect: "apply",
      targets: [{ label: "notes/agent.md" }],
    });
    expect(state.state).toBe("awaiting_confirmation");
  });

  it("拒绝运行时收到的 type 与 payload kind 错配事件", () => {
    const state = reduceAssistantRunEvent(createAssistantRunEventState(runId), {
      runId,
      seq: 1,
      stateVersion: 1,
      type: "tool_started",
      timestamp: "2026-07-13T00:00:00.000Z",
      payload: { kind: "content_delta", delta: "不得应用" },
    } as unknown as AssistantRunEvent);

    expect(state.lastSeq).toBe(0);
    expect(state.content).toBe("");
    expect(state.events).toEqual([]);
  });

  it("按 run_id 与 seq 去重，不重复拼接 content delta", () => {
    const delta = event(
      4,
      "content_delta",
      {
        kind: "content_delta",
        delta: "第一段",
      },
      3,
    );
    const state = reduce([
      event(1, "accepted", {
        kind: "accepted",
        turnId: "turn-001",
        sessionKey: "session-key-001",
      }),
      event(
        2,
        "stage_changed",
        {
          kind: "stage_changed",
          state: "preparing",
          stage: "正在准备",
        },
        2,
      ),
      event(
        3,
        "stage_changed",
        {
          kind: "stage_changed",
          state: "running",
          stage: "正在生成",
        },
        3,
      ),
      delta,
      delta,
    ]);

    expect(state.lastSeq).toBe(4);
    expect(state.content).toBe("第一段");
    expect(state.events).toHaveLength(4);
    expect(state.resyncFromSeq).toBeNull();
  });

  it("缓冲乱序事件并仅在连续序列完整后应用", () => {
    const state = reduce([
      event(1, "accepted", {
        kind: "accepted",
        turnId: "turn-001",
        sessionKey: "session-key-001",
      }),
      event(4, "content_delta", { kind: "content_delta", delta: "已补齐" }, 3),
    ]);

    expect(state.lastSeq).toBe(1);
    expect(state.content).toBe("");
    expect(state.pendingEvents.map((pending) => pending.seq)).toEqual([4]);
    expect(state.resyncFromSeq).toBe(2);

    const preparing = reduceAssistantRunEvent(
      state,
      event(
        2,
        "stage_changed",
        {
          kind: "stage_changed",
          state: "preparing",
          stage: "正在准备",
        },
        2,
      ),
    );
    const recovered = reduceAssistantRunEvent(
      preparing,
      event(
        3,
        "stage_changed",
        {
          kind: "stage_changed",
          state: "running",
          stage: "准备完成",
        },
        3,
      ),
    );
    expect(recovered.lastSeq).toBe(4);
    expect(recovered.state).toBe("running");
    expect(recovered.content).toBe("已补齐");
    expect(recovered.pendingEvents).toEqual([]);
    expect(recovered.resyncFromSeq).toBeNull();
  });

  it("终态不可被后续事件离开", () => {
    const state = reduce([
      event(1, "accepted", {
        kind: "accepted",
        turnId: "turn-001",
        sessionKey: "session-key-001",
      }),
      event(
        2,
        "stage_changed",
        {
          kind: "stage_changed",
          state: "preparing",
          stage: "正在准备",
        },
        2,
      ),
      event(
        3,
        "stage_changed",
        {
          kind: "stage_changed",
          state: "running",
          stage: "正在生成",
        },
        3,
      ),
      event(4, "completed", { kind: "completed", messageId: "message-001" }, 4),
      event(
        5,
        "stage_changed",
        {
          kind: "stage_changed",
          state: "running",
          stage: "不应恢复",
        },
        5,
      ),
    ]);

    expect(state.state).toBe("completed");
    expect(state.lastSeq).toBe(4);
    expect(state.summary).toBeNull();
    expect(state.events.map((applied) => applied.seq)).toEqual([1, 2, 3, 4]);
  });

  it("终态抵达时丢弃更高序号的乱序缓存并完成重同步", () => {
    const afterGap = reduce([
      event(1, "accepted", {
        kind: "accepted",
        turnId: "turn-001",
        sessionKey: "session-key-001",
      }),
      event(6, "content_delta", { kind: "content_delta", delta: "过期" }, 5),
    ]);
    const completed = [
      event(
        2,
        "stage_changed",
        { kind: "stage_changed", state: "preparing", stage: "准备" },
        2,
      ),
      event(
        3,
        "stage_changed",
        { kind: "stage_changed", state: "running", stage: "运行" },
        3,
      ),
      event(
        4,
        "content_delta",
        { kind: "content_delta", delta: "最终内容" },
        3,
      ),
      event(5, "completed", { kind: "completed", messageId: "message-001" }, 4),
    ].reduce(reduceAssistantRunEvent, afterGap);

    expect(completed.state).toBe("completed");
    expect(completed.content).toBe("最终内容");
    expect(completed.pendingEvents).toEqual([]);
    expect(completed.resyncFromSeq).toBeNull();
  });

  it("重连时可由已持久化事件重放出同一 UI 状态", () => {
    const persisted = [
      event(1, "accepted", {
        kind: "accepted",
        turnId: "turn-001",
        sessionKey: "session-key-001",
      }),
      event(
        2,
        "stage_changed",
        {
          kind: "stage_changed",
          state: "preparing",
          stage: "正在准备",
        },
        2,
      ),
      event(
        3,
        "stage_changed",
        {
          kind: "stage_changed",
          state: "running",
          stage: "正在生成",
        },
        3,
      ),
      event(
        4,
        "content_delta",
        { kind: "content_delta", delta: "可重放内容" },
        3,
      ),
      event(5, "completed", { kind: "completed", messageId: "message-001" }, 4),
    ];

    const live = reduce(persisted);
    const replayed = replayAssistantRunEvents(runId, [
      persisted[4]!,
      persisted[2]!,
      persisted[0]!,
      persisted[3]!,
      persisted[1]!,
    ]);

    expect(replayed).toEqual(live);
    expect(replayed.state).toBe("completed");
    expect(replayed.content).toBe("可重放内容");
    expect(replayed.summary).toBeNull();
  });
});
