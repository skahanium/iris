import type { Editor } from "@tiptap/react";
import { useCallback, useRef } from "react";

import { buildInlineAiUserMessage } from "@/lib/inline-ai-prompts";
import {
  buildSlashCommandMessage,
  parseSlashActionId,
  slashActionId,
} from "@/lib/slash-command-prompts";
import {
  listenLlmDone,
  listenLlmError,
  listenLlmToken,
  llmAbort,
  llmGenerate,
} from "@/lib/ipc";
import type { ChatMessage, LlmTokenEvent } from "@/types/ipc";

export interface UseInlineAiOptions {
  provider?: string;
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

/**
 * 内联 AI：选区 → ai-stream 流式生成；支持接受 / 回退 / 重试；`/` 命令写入 ai-stream。
 */
export function useInlineAi({ provider = "openai", onStatus }: UseInlineAiOptions = {}) {
  const requestIdRef = useRef<string | null>(null);
  const streamBufRef = useRef("");
  const unlistenRef = useRef<Array<() => void>>([]);
  const slashSystemRef = useRef<string | undefined>(undefined);

  const detachListeners = useCallback(() => {
    for (const u of unlistenRef.current) u();
    unlistenRef.current = [];
  }, []);

  const attachListeners = useCallback(
    async (editor: Editor) => {
      detachListeners();
      const unlistenToken = await listenLlmToken((payload) => {
        const ev = payload as LlmTokenEvent;
        if (requestIdRef.current && ev.request_id !== requestIdRef.current) {
          return;
        }
        streamBufRef.current += ev.token;
        editor.commands.updateAiStream(streamBufRef.current);
      });
      const unlistenDone = await listenLlmDone((payload) => {
        const ev = payload as { request_id?: string };
        if (requestIdRef.current && ev.request_id === requestIdRef.current) {
          editor.commands.setAiStreamStatus("ready");
          onStatus?.("AI 空闲");
        }
      });
      const unlistenError = await listenLlmError((payload) => {
        const ev = payload as { request_id?: string; error?: string };
        if (requestIdRef.current && ev.request_id === requestIdRef.current) {
          editor.commands.setAiStreamStatus("error");
          onStatus?.(`AI 错误: ${ev.error ?? "未知错误"}`);
        }
      });
      unlistenRef.current = [unlistenToken, unlistenDone, unlistenError];
    },
    [detachListeners, onStatus],
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
        const rid = await llmGenerate({
          provider,
          messages: request.messages,
          system,
          stream: true,
        });
        requestIdRef.current = rid;
      } catch (e) {
        editor.commands.setAiStreamStatus("error");
        onStatus?.(`AI 错误: ${e instanceof Error ? e.message : String(e)}`);
        throw e;
      }
    },
    [provider, onStatus, attachListeners],
  );

  const run = useCallback(
    async (editor: Editor, action: string) => {
      const { from, to } = editor.state.selection;
      const originalText = editor.state.doc.textBetween(from, to, "\n").trim();
      if (!originalText) return;

      editor.commands.insertAiStreamForSelection({ originalText, action });
      await streamIntoAiNode(editor, {
        action,
        originalText,
        messages: [
          {
            role: "user",
            content: buildInlineAiUserMessage(action, originalText),
          },
        ],
      });
    },
    [streamIntoAiNode],
  );

  const runSlash = useCallback(
    async (editor: Editor, command: string, noteMarkdown: string) => {
      slashSystemRef.current = noteMarkdown.slice(0, 8000) || undefined;
      const action = slashActionId(command);

      editor.commands.insertAiStreamAtCursor({
        originalText: "",
        action,
      });

      await streamIntoAiNode(editor, {
        action,
        originalText: "",
        messages: [
          { role: "user", content: buildSlashCommandMessage(command) },
        ],
        system: slashSystemRef.current,
      });
    },
    [streamIntoAiNode],
  );

  const retry = useCallback(
    async (editor: Editor) => {
      const ctx = getActiveAiStreamAttrs(editor);
      if (!ctx) return;
      const request = buildRetryRequest(ctx);
      if (parseSlashActionId(ctx.action) && slashSystemRef.current) {
        request.system = slashSystemRef.current;
      }
      await streamIntoAiNode(editor, request);
    },
    [streamIntoAiNode],
  );

  const abort = useCallback(async () => {
    if (requestIdRef.current) {
      await llmAbort(requestIdRef.current);
      requestIdRef.current = null;
    }
    detachListeners();
  }, [detachListeners]);

  return { run, runSlash, retry, abort };
}
