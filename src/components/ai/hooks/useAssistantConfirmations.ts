import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type Dispatch,
  type MutableRefObject,
  type SetStateAction,
} from "react";

import { accumulateTokenUsage } from "@/lib/token-usage";
import { invokeErrorMessage } from "@/lib/credentials";
import {
  listenToolConfirmRequest,
  profileSetRule,
  toolConfirm as toolConfirmIpc,
} from "@/lib/ipc";
import { mapChatToolCallsForUi } from "@/lib/map-chat-tool-calls";
import { mergeContextPackets } from "@/lib/ai/merge-context-packets";
import { skillInstallSuccessNotice } from "@/lib/skill-install-notice";
import type {
  AssistantActionState,
  AssistantIntent,
  AssistantTaskStatus,
  ContextPacket,
  TokenUsage,
} from "@/types/ai";

import type { ChatLine } from "../AiMessageList";
import type { RuleConfirmRequest } from "../RuleConfirmDialog";
import type { ToolConfirmRequest } from "../ToolConfirmDialog";

type ToolDecision = "approve" | "reject" | "modify";

interface ToolConfirmResult {
  resumed?: boolean;
  content?: string;
  tool_calls?: Parameters<typeof mapChatToolCallsForUi>[0];
  tool_results?: Parameters<typeof mapChatToolCallsForUi>[1];
  evidence_packets?: ContextPacket[];
  usage?: TokenUsage;
  pending_confirmation?: boolean;
  status?: string;
  installed_skill?: string;
}

type ListenForToolConfirmRequests = (
  handler: (request: ToolConfirmRequest) => void,
) => Promise<() => void>;

type ConfirmTool = (params: {
  request_id: string;
  tool_call_id: string;
  decision: ToolDecision;
  modified_args?: unknown;
}) => Promise<unknown>;

type SaveRule = (params: {
  key: string;
  description: string;
  source: RuleConfirmRequest["source"];
}) => Promise<unknown>;

interface AssistantRunPort {
  setFromTaskStatus: (
    status: AssistantTaskStatus,
    intent: AssistantIntent,
  ) => void;
}

interface UseAssistantConfirmationsParams {
  actionIntent: AssistantIntent;
  assistantRun: AssistantRunPort;
  buildActionState: (
    intent: AssistantIntent,
    status: AssistantTaskStatus,
    error?: string,
  ) => AssistantActionState;
  ensureAssistantStreamSlot: () => void;
  requestIdRef: MutableRefObject<string | null>;
  setActionState: Dispatch<SetStateAction<AssistantActionState>>;
  setActivityHint: Dispatch<SetStateAction<string | null>>;
  setHarnessRequestId: Dispatch<SetStateAction<string | null>>;
  setMessages: Dispatch<SetStateAction<ChatLine[]>>;
  setPackets: Dispatch<SetStateAction<ContextPacket[]>>;
  setSessionTokenUsage: Dispatch<SetStateAction<TokenUsage | null>>;
  setStreaming: Dispatch<SetStateAction<boolean>>;
  deps?: {
    confirmTool?: ConfirmTool;
    listenForToolConfirmRequests?: ListenForToolConfirmRequests;
    saveRule?: SaveRule;
  };
}

export function useAssistantConfirmations({
  actionIntent,
  assistantRun,
  buildActionState,
  ensureAssistantStreamSlot,
  requestIdRef,
  setActionState,
  setActivityHint,
  setHarnessRequestId,
  setMessages,
  setPackets,
  setSessionTokenUsage,
  setStreaming,
  deps,
}: UseAssistantConfirmationsParams) {
  const [toolConfirmRequest, setToolConfirmRequest] =
    useState<ToolConfirmRequest | null>(null);
  const [ruleConfirmRequest, setRuleConfirmRequest] =
    useState<RuleConfirmRequest | null>(null);
  const toolConfirmInFlightRef = useRef<Set<string>>(new Set());
  const toolConfirmSettledRef = useRef<Set<string>>(new Set());

  const confirmTool = deps?.confirmTool ?? toolConfirmIpc;
  const listenForToolConfirmRequests =
    deps?.listenForToolConfirmRequests ?? listenToolConfirmRequest;
  const saveRule = deps?.saveRule ?? profileSetRule;

  useEffect(() => {
    let unlisten: (() => void) | undefined;

    void listenForToolConfirmRequests((req) => {
      if (req.tool_name === "update_user_rule") {
        const ruleType =
          typeof req.arguments.rule_type === "string"
            ? req.arguments.rule_type
            : "custom_rules";
        const ruleText =
          typeof req.arguments.rule === "string"
            ? req.arguments.rule
            : JSON.stringify(req.arguments);
        setRuleConfirmRequest({
          rule: ruleText,
          rule_type: ruleType,
          source: "ai_detected",
        });
      } else {
        setToolConfirmRequest(req);
      }
    }).then((fn) => {
      unlisten = fn;
    });

    return () => {
      unlisten?.();
    };
  }, [listenForToolConfirmRequests]);

  const handleToolConfirm = useCallback(
    async (
      requestId: string,
      toolCallId: string,
      decision: ToolDecision,
      modifiedArgs?: unknown,
    ) => {
      const confirmKey = `${requestId}:${toolCallId}`;
      if (
        toolConfirmInFlightRef.current.has(confirmKey) ||
        toolConfirmSettledRef.current.has(confirmKey)
      ) {
        return;
      }
      toolConfirmInFlightRef.current.add(confirmKey);
      const intent = actionIntent;
      const pendingConfirm = toolConfirmRequest;
      setToolConfirmRequest(null);
      setStreaming(true);
      setActivityHint(
        decision === "reject" ? "已拒绝，正在生成替代回答..." : "继续执行中...",
      );
      ensureAssistantStreamSlot();
      let nextTaskStatus: AssistantTaskStatus = "completed";
      try {
        const result = (await confirmTool({
          request_id: requestId,
          tool_call_id: toolCallId,
          decision,
          modified_args: modifiedArgs,
        })) as ToolConfirmResult;
        if (!result.resumed) {
          nextTaskStatus = "completed";
          setActionState(buildActionState(intent, nextTaskStatus));
          return;
        }
        const toolCalls = mapChatToolCallsForUi(
          result.tool_calls,
          result.tool_results,
        );
        const content = result.content?.trim() ?? "";
        if (result.evidence_packets?.length) {
          setPackets((prev) =>
            mergeContextPackets(prev, result.evidence_packets ?? []),
          );
        }
        if (result.usage) {
          setSessionTokenUsage((prev) =>
            accumulateTokenUsage(prev, result.usage!),
          );
        }
        setMessages((prev) => {
          const next = [...prev];
          const last = next[next.length - 1];
          if (last?.role === "assistant") {
            next[next.length - 1] = {
              ...last,
              content,
              toolCalls,
            };
          } else {
            next.push({ role: "assistant", content, toolCalls });
          }
          if (
            (decision === "approve" || decision === "modify") &&
            pendingConfirm?.tool_name === "skills_install"
          ) {
            const notice = skillInstallSuccessNotice({
              installedSkill: result.installed_skill,
              preview: pendingConfirm.preview,
              arguments: pendingConfirm.arguments,
            });
            if (notice) {
              next.push({ role: "system", content: notice });
            }
          }
          return next;
        });
        const stillPending =
          result.pending_confirmation === true ||
          result.status === "pending_tools" ||
          (toolCalls?.some((t) => t.status === "pending") ?? false);
        nextTaskStatus = stillPending ? "awaiting_confirmation" : "completed";
        setActionState(buildActionState(intent, nextTaskStatus));
        if (!stillPending) {
          setHarnessRequestId(null);
          requestIdRef.current = null;
        }
      } catch (error) {
        const message = invokeErrorMessage(error);
        nextTaskStatus = "error";
        setMessages((prev) => [
          ...prev,
          { role: "system", content: `宸ュ叿纭澶辫触: ${message}` },
        ]);
        setActionState(buildActionState(intent, nextTaskStatus, message));
      } finally {
        toolConfirmInFlightRef.current.delete(confirmKey);
        toolConfirmSettledRef.current.add(confirmKey);
        setStreaming(false);
        setActivityHint(null);
        assistantRun.setFromTaskStatus(nextTaskStatus, intent);
      }
    },
    [
      actionIntent,
      assistantRun,
      buildActionState,
      confirmTool,
      ensureAssistantStreamSlot,
      requestIdRef,
      setActionState,
      setActivityHint,
      setHarnessRequestId,
      setMessages,
      setPackets,
      setSessionTokenUsage,
      setStreaming,
      toolConfirmRequest,
    ],
  );

  const dismissToolConfirm = useCallback(() => {
    const req = toolConfirmRequest;
    if (req) {
      void handleToolConfirm(req.request_id, req.tool_call_id, "reject");
      return;
    }
    setToolConfirmRequest(null);
    assistantRun.setFromTaskStatus("completed", actionIntent);
  }, [actionIntent, assistantRun, handleToolConfirm, toolConfirmRequest]);

  const handleRuleConfirm = useCallback(
    async (request: RuleConfirmRequest) => {
      const key =
        request.rule_type && request.rule_type !== "custom_rules"
          ? request.rule_type
          : "custom_rules";
      await saveRule({
        key,
        description: request.rule,
        source: request.source,
      });
      setRuleConfirmRequest(null);
    },
    [saveRule],
  );

  const closeRuleConfirm = useCallback(() => {
    setRuleConfirmRequest(null);
  }, []);

  return {
    closeRuleConfirm,
    dismissToolConfirm,
    handleRuleConfirm,
    handleToolConfirm,
    ruleConfirmRequest,
    setToolConfirmRequest,
    toolConfirmRequest,
  };
}
