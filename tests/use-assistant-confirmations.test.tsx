import { act, createElement, type Dispatch, type SetStateAction } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { useAssistantConfirmations } from "@/components/ai/hooks/useAssistantConfirmations";
import type { ChatLine } from "@/components/ai/AiMessageList";
import type { ToolConfirmRequest } from "@/components/ai/ToolConfirmDialog";
import type {
  AssistantActionState,
  AssistantIntent,
  AssistantTaskStatus,
  ContextPacket,
  TokenUsage,
} from "@/types/ai";

type HookApi = ReturnType<typeof useAssistantConfirmations>;
type ConfirmTool = (params: {
  request_id: string;
  tool_call_id: string;
  decision: "approve" | "reject" | "modify";
  modified_args?: unknown;
}) => Promise<unknown>;

const request: ToolConfirmRequest = {
  request_id: "req-1",
  tool_call_id: "tool-1",
  tool_name: "search_notes",
  arguments: {},
};

function buildActionState(
  intent: AssistantIntent,
  status: AssistantTaskStatus,
): AssistantActionState {
  return { intent, status, label: `${intent}:${status}` };
}

function Harness({
  onReady,
  confirmTool,
  setRunStatus,
  setMessages,
}: {
  onReady: (api: HookApi) => void;
  confirmTool: ConfirmTool;
  setRunStatus: (status: AssistantTaskStatus, intent: AssistantIntent) => void;
  setMessages?: Dispatch<SetStateAction<ChatLine[]>>;
}) {
  const api = useAssistantConfirmations({
    actionIntent: "chat",
    buildActionState,
    ensureAssistantStreamSlot: vi.fn(),
    setActionState: vi.fn(),
    setActivityHint: vi.fn(),
    setHarnessRequestId: vi.fn(),
    setMessages: setMessages ?? vi.fn(),
    setPackets: vi.fn(),
    setSessionTokenUsage: vi.fn(),
    setStreaming: vi.fn(),
    requestIdRef: { current: "req-1" },
    assistantRun: {
      setFromTaskStatus: setRunStatus,
    },
    deps: {
      confirmTool,
      listenForToolConfirmRequests: async () => () => undefined,
      saveRule: vi.fn(),
    },
  });
  onReady(api);
  return null;
}

describe("useAssistantConfirmations", () => {
  let container: HTMLDivElement;
  let root: Root;
  let api!: HookApi;
  let confirmTool: ConfirmTool;
  let runStatuses: Array<[AssistantTaskStatus, AssistantIntent]>;

  function render() {
    root.render(
      createElement(Harness, {
        onReady: (value) => {
          api = value;
        },
        confirmTool,
        setRunStatus: (status, intent) => {
          runStatuses.push([status, intent]);
        },
      }),
    );
  }

  beforeEach(async () => {
    container = document.createElement("div");
    document.body.appendChild(container);
    root = createRoot(container);
    runStatuses = [];
    confirmTool = vi.fn(async () => ({
      resumed: true,
      content: "confirmed",
      tool_calls: [],
      tool_results: [],
      evidence_packets: [] as ContextPacket[],
      usage: {
        prompt_tokens: 1,
        completion_tokens: 2,
        total_tokens: 3,
      } as TokenUsage,
      pending_confirmation: false,
      status: "completed",
    }));
    await act(async () => {
      render();
    });
  });

  afterEach(() => {
    act(() => {
      root.unmount();
    });
    container.remove();
  });

  it("suppresses duplicate resume calls for the same tool confirmation", async () => {
    await act(async () => {
      api.setToolConfirmRequest(request);
    });

    await act(async () => {
      await Promise.all([
        api.handleToolConfirm("req-1", "tool-1", "approve"),
        api.handleToolConfirm("req-1", "tool-1", "approve"),
      ]);
    });

    expect(confirmTool).toHaveBeenCalledTimes(1);
    expect(confirmTool).toHaveBeenCalledWith({
      request_id: "req-1",
      tool_call_id: "tool-1",
      decision: "approve",
      modified_args: undefined,
    });
    expect(runStatuses.at(-1)).toEqual(["completed", "chat"]);
  });

  it("turns an update_user_rule request into a rule confirmation", async () => {
    let listener: ((req: ToolConfirmRequest) => void) | null = null;
    confirmTool = vi.fn();
    function ListenerHarness({ onReady }: { onReady: (api: HookApi) => void }) {
      const api = useAssistantConfirmations({
        actionIntent: "chat",
        buildActionState,
        ensureAssistantStreamSlot: vi.fn(),
        setActionState: vi.fn(),
        setActivityHint: vi.fn(),
        setHarnessRequestId: vi.fn(),
        setMessages: vi.fn(),
        setPackets: vi.fn(),
        setSessionTokenUsage: vi.fn(),
        setStreaming: vi.fn(),
        requestIdRef: { current: null },
        assistantRun: { setFromTaskStatus: vi.fn() },
        deps: {
          confirmTool,
          listenForToolConfirmRequests: async (handler) => {
            listener = handler;
            return () => undefined;
          },
          saveRule: vi.fn(),
        },
      });
      onReady(api);
      return null;
    }

    act(() => {
      root.unmount();
    });
    root = createRoot(container);
    await act(async () => {
      root.render(
        createElement(ListenerHarness, {
          onReady: (value) => {
            api = value;
          },
        }),
      );
    });

    await act(async () => {
      listener?.({
        ...request,
        tool_name: "update_user_rule",
        arguments: { rule_type: "tone", rule: "Be concise" },
      });
    });

    expect(api.ruleConfirmRequest).toEqual({
      rule: "Be concise",
      rule_type: "tone",
      source: "ai_detected",
    });
    expect(api.toolConfirmRequest).toBeNull();
  });

  it("shows a readable localized message when tool confirmation fails", async () => {
    const messages: ChatLine[] = [];
    confirmTool = vi.fn(async () => {
      throw new Error("network unavailable");
    });

    act(() => {
      root.unmount();
    });
    root = createRoot(container);
    await act(async () => {
      root.render(
        createElement(Harness, {
          onReady: (value) => {
            api = value;
          },
          confirmTool,
          setRunStatus: (status, intent) => {
            runStatuses.push([status, intent]);
          },
          setMessages: (update) => {
            const next =
              typeof update === "function" ? update(messages) : update;
            messages.splice(0, messages.length, ...next);
          },
        }),
      );
    });

    await act(async () => {
      api.setToolConfirmRequest(request);
    });
    await act(async () => {
      await api.handleToolConfirm("req-1", "tool-1", "approve");
    });

    expect(messages.at(-1)).toEqual({
      role: "system",
      content: "工具确认失败: network unavailable",
    });
    expect(messages.at(-1)?.content).not.toContain("宸");
    expect(runStatuses.at(-1)).toEqual(["error", "chat"]);
  });
});
