import type { Editor } from "@tiptap/react";
import { useCallback, useEffect, useRef } from "react";

import {
  buildInlineAiSelectionReferentPrompt,
  buildInlineAiUserMessage,
} from "@/lib/inline-ai-prompts";
import { createContextReference } from "@/lib/context-reference";
import { getEditorSelectionSnapshot } from "@/lib/iris-clipboard";
import {
  buildSlashCommandMessage,
  parseSlashActionId,
  slashActionId,
} from "@/lib/slash-command-prompts";
import {
  assistantExecute,
  listenLlmDone,
  listenLlmError,
  listenLlmToken,
  llmAbort,
  llmGenerate,
} from "@/lib/ipc";
import type { ChatMessage } from "@/types/ipc";
import type { AiDomain, ContextReference } from "@/types/ai";

export const INLINE_AI_INSERT_AFTER_SELECTION = "insert_after_selection";
export const INLINE_AI_REPLACE_SELECTION = "replace_selection";

export interface UseInlineAiOptions {
  provider?: string;
  domain?: AiDomain;
  notePath?: string | null;
  getNoteContent?: () => string;
  onStatus?: (status: string) => void;
}

export interface AiStreamRequest {
  action: string;
  originalText: string;
  messages: ChatMessage[];
  system?: string;
}

export function getActiveAiStreamAttrs(editor: Editor): {
  originalText: string;
  action: string;
} | null {
  let result: { originalText: string; action: string } | null = null;
  editor.state.doc.descendants((node) => {
    if (node.type.name === "aiStream" && result === null) {
      const raw = node.attrs as { originalText?: unknown; action?: unknown };
      const action = typeof raw.action === "string" ? raw.action : "";
      if (!action) return;
      const originalText =
        typeof raw.originalText === "string" ? raw.originalText : "";
      result = { originalText, action };
    }
  });
  return result;
}

export function buildRetryRequest(ctx: {
  originalText: string;
  action: string;
}): AiStreamRequest {
  const slashCmd = parseSlashActionId(ctx.action);
  if (slashCmd) {
    return {
      action: ctx.action,
      originalText: ctx.originalText,
      messages: [{ role: "user", content: buildSlashCommandMessage(slashCmd) }],
    };
  }
  return {
    action: ctx.action,
    originalText: ctx.originalText,
    messages: [
      {
        role: "user",
        content: buildInlineAiUserMessage(ctx.action, ctx.originalText),
      },
    ],
  };
}

export function buildInlineSelectionReference(
  editor: Editor,
  filePath: string | null,
): ContextReference | null {
  const snapshot = getEditorSelectionSnapshot(editor);
  if (!snapshot) return null;
  return createContextReference({
    kind: "selection",
    filePath,
    content: snapshot.content,
    utf8Range: null,
    editorRange: snapshot.editorRange,
  });
}

/**
 * 内联 AI：选区 → ai-stream 流式生成；支持接受 / 回退 / 重试；`/` 命令写入 ai-stream。
 */
export function useInlineAi({
  provider = "openai",
  domain = "normal",
  notePath = null,
  getNoteContent,
  onStatus,
}: UseInlineAiOptions = {}) {
  const requestIdRef = useRef<string | null>(null);
  const streamBufRef = useRef("");
  const rafRef = useRef<number | undefined>(undefined);
  const unlistenRef = useRef<Array<() => void>>([]);
  const slashSystemRef = useRef<string | undefined>(undefined);

  const detachListeners = useCallback(() => {
    if (rafRef.current !== undefined) {
      cancelAnimationFrame(rafRef.current);
      rafRef.current = undefined;
    }
    for (const u of unlistenRef.current) u();
    unlistenRef.current = [];
  }, []);

  const flushStreamToEditor = useCallback((editor: Editor) => {
    if (rafRef.current !== undefined) {
      cancelAnimationFrame(rafRef.current);
      rafRef.current = undefined;
    }
    editor.commands.updateAiStream(streamBufRef.current);
  }, []);

  const markStreamReady = useCallback(
    (editor: Editor) => {
      flushStreamToEditor(editor);
      editor.commands.setAiStreamStatus("ready");
      onStatus?.("AI 空闲");
    },
    [flushStreamToEditor, onStatus],
  );

  const attachListeners = useCallback(
    async (editor: Editor) => {
      detachListeners();
      const unlistenToken = await listenLlmToken((ev) => {
        if (!requestIdRef.current && ev.request_id) {
          requestIdRef.current = ev.request_id;
        } else if (
          requestIdRef.current &&
          ev.request_id &&
          ev.request_id !== requestIdRef.current
        ) {
          return;
        }
        streamBufRef.current += ev.token;
        if (rafRef.current === undefined) {
          rafRef.current = requestAnimationFrame(() => {
            rafRef.current = undefined;
            editor.commands.updateAiStream(streamBufRef.current);
          });
        }
      });
      const unlistenDone = await listenLlmDone((ev) => {
        if (
          requestIdRef.current &&
          ev.request_id &&
          ev.request_id !== requestIdRef.current
        ) {
          return;
        }
        markStreamReady(editor);
      });
      const unlistenError = await listenLlmError((ev) => {
        if (
          requestIdRef.current &&
          ev.request_id &&
          ev.request_id !== requestIdRef.current
        ) {
          return;
        }
        flushStreamToEditor(editor);
        editor.commands.setAiStreamStatus("error");
        onStatus?.(`AI 错误: ${ev.error ?? "未知错误"}`);
      });
      unlistenRef.current = [unlistenToken, unlistenDone, unlistenError];
    },
    [detachListeners, flushStreamToEditor, markStreamReady, onStatus],
  );

  const streamIntoAiNode = useCallback(
    async (editor: Editor, request: AiStreamRequest) => {
      streamBufRef.current = "";
      editor.commands.clearAiStreamContent();
      editor.commands.setAiStreamStatus("streaming");
      onStatus?.("AI 处理中…");

      await attachListeners(editor);

      const system =
        request.system ??
        (parseSlashActionId(request.action)
          ? slashSystemRef.current
          : undefined);

      try {
        if (requestIdRef.current) {
          await llmAbort(requestIdRef.current);
        }
        requestIdRef.current = null;
        const rid = await llmGenerate({
          provider,
          messages: request.messages,
          system,
          stream: true,
        });
        requestIdRef.current = rid;
        // llmGenerate 在流结束后才 resolve；若 llm:done 因 request_id 竞态未触发，此处兜底
        markStreamReady(editor);
      } catch (e) {
        editor.commands.setAiStreamStatus("error");
        onStatus?.(`AI 错误: ${e instanceof Error ? e.message : String(e)}`);
        throw e;
      }
    },
    [provider, onStatus, attachListeners, markStreamReady],
  );

  const runClassifiedAssistantIntoAiNode = useCallback(
    async (editor: Editor, request: AiStreamRequest) => {
      if (!notePath) {
        editor.commands.setAiStreamStatus("error");
        onStatus?.("AI 错误: 缺少涉密文档路径");
        return;
      }

      streamBufRef.current = "";
      editor.commands.clearAiStreamContent();
      editor.commands.setAiStreamStatus("streaming");
      onStatus?.("涉密 AI 处理中…");

      try {
        const hasSelection = request.originalText.trim().length > 0;
        const selectionReference = hasSelection
          ? buildInlineSelectionReference(editor, notePath)
          : null;
        const response = await assistantExecute({
          aiDomain: "classified",
          agentIntent: hasSelection ? "rewrite_selection" : "chat",
          intent: hasSelection ? "writing" : "chat",
          message: hasSelection
            ? buildInlineAiSelectionReferentPrompt(request.action)
            : (request.messages[0]?.content ?? ""),
          notePath,
          noteContent: null,
          contextReferences: selectionReference ? [selectionReference] : [],
          selection: hasSelection ? request.originalText : null,
          cursorContext: getNoteContent?.() ?? null,
          webAuthorized: false,
        });
        const content =
          response.kind === "writing"
            ? (response.payload.patches[0]?.replacement_text ?? "")
            : response.kind === "chat"
              ? (response.payload.content ?? "")
              : "";
        streamBufRef.current = content;
        editor.commands.updateAiStream(content);
        editor.commands.setAiStreamStatus("ready");
        onStatus?.("AI 空闲");
      } catch (e) {
        editor.commands.setAiStreamStatus("error");
        onStatus?.(`AI 错误: ${e instanceof Error ? e.message : String(e)}`);
        throw e;
      }
    },
    [getNoteContent, notePath, onStatus],
  );

  const run = useCallback(
    async (editor: Editor, action: string) => {
      const { from, to } = editor.state.selection;
      const originalText = editor.state.doc.textBetween(from, to, "\n").trim();
      if (!originalText) return;

      editor.commands.insertAiStreamForSelection({ originalText, action });
      const request = {
        action,
        originalText,
        messages: [
          {
            role: "user",
            content: buildInlineAiUserMessage(action, originalText),
          },
        ],
      };
      if (domain === "classified") {
        await runClassifiedAssistantIntoAiNode(editor, request);
      } else {
        await streamIntoAiNode(editor, request);
      }
    },
    [domain, runClassifiedAssistantIntoAiNode, streamIntoAiNode],
  );

  const runSlash = useCallback(
    async (editor: Editor, command: string, noteMarkdown: string) => {
      slashSystemRef.current = noteMarkdown.slice(0, 8000) || undefined;
      const action = slashActionId(command);

      editor.commands.insertAiStreamAtCursor({
        originalText: "",
        action,
      });

      const request = {
        action,
        originalText: "",
        messages: [
          { role: "user", content: buildSlashCommandMessage(command) },
        ],
        system: slashSystemRef.current,
      };
      if (domain === "classified") {
        await runClassifiedAssistantIntoAiNode(editor, request);
      } else {
        await streamIntoAiNode(editor, request);
      }
    },
    [domain, runClassifiedAssistantIntoAiNode, streamIntoAiNode],
  );

  const retry = useCallback(
    async (editor: Editor) => {
      const ctx = getActiveAiStreamAttrs(editor);
      if (!ctx) return;
      const request = buildRetryRequest(ctx);
      if (parseSlashActionId(ctx.action) && slashSystemRef.current) {
        request.system = slashSystemRef.current;
      }
      if (domain === "classified") {
        await runClassifiedAssistantIntoAiNode(editor, request);
      } else {
        await streamIntoAiNode(editor, request);
      }
    },
    [domain, runClassifiedAssistantIntoAiNode, streamIntoAiNode],
  );

  const abort = useCallback(async () => {
    if (requestIdRef.current) {
      await llmAbort(requestIdRef.current);
      requestIdRef.current = null;
    }
    detachListeners();
  }, [detachListeners]);

  const dismiss = useCallback(
    (_editor: Editor) => {
      void abort();
    },
    [abort],
  );

  const finish = useCallback(() => {
    requestIdRef.current = null;
    streamBufRef.current = "";
    detachListeners();
    onStatus?.("AI 空闲");
  }, [detachListeners, onStatus]);

  useEffect(
    () => () => {
      detachListeners();
    },
    [detachListeners],
  );

  return { run, runSlash, retry, abort, dismiss, finish };
}
