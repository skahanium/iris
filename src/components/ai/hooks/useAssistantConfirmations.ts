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

interface ToolExecutionOutcome {
  status?: "succeeded" | "failed" | "rejected" | string;
  sideEffectCommitted?: boolean;
  toolName?: string;
  resultSummary?: string | null;
}

interface AssistantResumeOutcome {
  status?: "resumed" | "skipped" | "failed" | string;
  failureClass?: string | null;
  userMessage?: string | null;
}

interface ToolConfirmResult {
  resumed?: boolean;
  content?: string;
  tool_calls?: Parameters<typeof mapChatToolCallsForUi>[0];
  tool_results?: Parameters<typeof mapChatToolCallsForUi>[1];
  evidence_packets?: ContextPacket[];
  usage?: TokenUsage;
  pending_confirmation?: boolean;
  status?: string;
  tool_confirmation_partial?: boolean;
  resume_error_code?: string;
  resume_error_message?: string;
  toolExecutionOutcome?: ToolExecutionOutcome;
  assistantResumeOutcome?: AssistantResumeOutcome;
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

const DOCUMENT_WRITE_TOOLS = new Set([
  "insert_text_at_cursor",
  "replace_selection",
]);

function toolConfirmationFailurePrefix(request: ToolConfirmRequest | null) {
  if (request && DOCUMENT_WRITE_TOOLS.has(request.tool_name)) {
    return "文档修改确认失败";
  }
  return "工具确认失败";
}

function isPartialToolConfirmation(result: ToolConfirmResult): boolean {
  if (result.tool_confirmation_partial) return true;
  return (
    result.toolExecutionOutcome?.sideEffectCommitted === true &&
    result.toolExecutionOutcome.status === "succeeded" &&
    result.assistantResumeOutcome?.status === "failed"
  );
}

function partialToolConfirmationNotice(
  result: ToolConfirmResult,
  _request: ToolConfirmRequest | null,
): string {
  const structuredMessage = result.assistantResumeOutcome?.userMessage?.trim();
  if (structuredMessage) return structuredMessage;
  const content = result.content?.trim();
  if (content) return content;
  const reason =
    result.assistantResumeOutcome?.failureClass ??
    result.resume_error_code ??
    result.resume_error_message ??
    "resume_failed";
  return `工具已执行，但继续生成回复失败：${reason}`;
}

interface AssistantRunPort {
  setFromTaskStatus: (
    status: AssistantTaskStatus,
    intent: AssistantIntent,
  ) => void;
}

interface UseAssistantConfirmationsParams {
  actionIntent: AssistantIntent;
  /// Active conversation id. When it changes while a tool confirmation is
  /// pending, the pending confirmation is invalidated so a stale write from
  /// the previous session can never dispatch into the now-active document.
  activeSessionId: number | null;
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
  activeSessionId,
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
  const [toolConfirmStaleReason, setToolConfirmStaleReason] = useState<
    string | null
  >(null);
  const toolConfirmInFlightRef = useRef<Set<string>>(new Set());
  const toolConfirmSettledRef = useRef<Set<string>>(new Set());

  const confirmTool = deps?.confirmTool ?? toolConfirmIpc;
  const listenForToolConfirmRequests =
    deps?.listenForToolConfirmRequests ?? listenToolConfirmRequest;
  const saveRule = deps?.saveRule ?? profileSetRule;

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;

    void listenForToolConfirmRequests((req) => {
      if (disposed) return;
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
      if (disposed) fn();
      else unlisten = fn;
    });

    return () => {
      disposed = true;
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
      const pendingConfirm = toolConfirmRequest;
      if (!pendingConfirm) {
        // No pending confirmation (e.g. invalidated by a session switch).
        // Dropping here keeps the IPC/backend write from firing.
        return;
      }
      if (
        pendingConfirm.request_id !== requestId ||
        pendingConfirm.tool_call_id !== toolCallId
      ) {
        return;
      }
      if (requestIdRef.current && requestIdRef.current !== requestId) {
        return;
      }

      const confirmKey = `${requestId}:${toolCallId}`;
      if (
        toolConfirmInFlightRef.current.has(confirmKey) ||
        toolConfirmSettledRef.current.has(confirmKey)
      ) {
        return;
      }
      toolConfirmInFlightRef.current.add(confirmKey);
      const intent = actionIntent;
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
          if (isPartialToolConfirmation(result)) {
            const notice = partialToolConfirmationNotice(
              result,
              pendingConfirm,
            );
            nextTaskStatus = "error";
            setMessages((prev) => [
              ...prev,
              { role: "system", content: notice },
            ]);
            setActionState(buildActionState(intent, nextTaskStatus, notice));
            setHarnessRequestId(requestId);
            requestIdRef.current = requestId;
            return;
          }
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
        const prefix = toolConfirmationFailurePrefix(pendingConfirm);
        setMessages((prev) => [
          ...prev,
          { role: "system", content: `${prefix}: ${message}` },
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

  const invalidatePendingToolConfirm = useCallback(
    (reason: string = "会话已切换") => {
      const req = toolConfirmRequest;
      if (!req) return;
      setToolConfirmStaleReason(reason);
      setMessages((prev) => [
        ...prev,
        { role: "system", content: `确认已失效：${reason}` },
      ]);
      setToolConfirmRequest(null);
      toolConfirmInFlightRef.current.clear();
      toolConfirmSettledRef.current.clear();
      requestIdRef.current = null;
      const nextStatus: AssistantTaskStatus = "completed";
      setActionState(buildActionState(actionIntent, nextStatus));
      assistantRun.setFromTaskStatus(nextStatus, actionIntent);
    },
    [
      actionIntent,
      assistantRun,
      buildActionState,
      requestIdRef,
      setActionState,
      setMessages,
      toolConfirmRequest,
    ],
  );

  // Switching the active conversation invalidates any pending write
  // confirmation from the previous session so it can never dispatch into the
  // now-active document.
  const previousActiveSessionIdRef = useRef<number | null>(activeSessionId);
  useEffect(() => {
    if (
      activeSessionId !== previousActiveSessionIdRef.current &&
      toolConfirmRequest !== null
    ) {
      invalidatePendingToolConfirm("会话已切换");
    }
    previousActiveSessionIdRef.current = activeSessionId;
  }, [activeSessionId, invalidatePendingToolConfirm, toolConfirmRequest]);

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
    invalidatePendingToolConfirm,
    ruleConfirmRequest,
    setToolConfirmRequest,
    toolConfirmRequest,
    toolConfirmStaleReason,
  };
}
