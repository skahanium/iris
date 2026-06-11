import type { Editor } from "@tiptap/react";
import { useCallback, useEffect, useState, type RefObject } from "react";

import type { AssistantSelectionQuote } from "@/components/ai/UnifiedAssistantPanel";
import { isClassifiedVaultPath } from "@/lib/classified-path";
import { settingsGet, settingsSet } from "@/lib/ipc";
import {
  EMPTY_ASSISTANT_CHROME,
  type AssistantChromeSnapshot,
} from "@/types/assistant-chrome";

interface UseAiSidecarBridgeParams {
  activePathRef: RefObject<string | null>;
  editorRef: RefObject<Editor | null>;
  setAiStatus: (message: string) => void;
}

export function useAiSidecarBridge({
  activePathRef,
  editorRef,
  setAiStatus,
}: UseAiSidecarBridgeParams) {
  const [aiPanelOpen, setAiPanelOpen] = useState(true);
  const [webSearchEnabled, setWebSearchEnabled] = useState(false);
  const [selectionQuote, setSelectionQuote] =
    useState<AssistantSelectionQuote | null>(null);
  const [prefillMessage, setPrefillMessage] = useState<string | null>(null);
  const [assistantChrome, setAssistantChrome] =
    useState<AssistantChromeSnapshot>(EMPTY_ASSISTANT_CHROME);

  useEffect(() => {
    void settingsGet<boolean>("web_search_enabled").then((enabled) => {
      if (enabled === true) {
        setWebSearchEnabled(true);
      }
    });
  }, []);

  const setWebSearch = useCallback((enabled: boolean) => {
    setWebSearchEnabled(enabled);
    void settingsSet("web_search_enabled", enabled);
  }, []);

  const toggleWebSearch = useCallback(() => {
    setWebSearchEnabled((prev) => {
      const next = !prev;
      void settingsSet("web_search_enabled", next);
      return next;
    });
  }, []);

  const sendSelectionToAi = useCallback(
    (options?: { prefill?: string }) => {
      const ed = editorRef.current;
      const path = activePathRef.current;
      if (!ed || !path) return;
      if (isClassifiedVaultPath(path)) {
        setAiStatus("涉密笔记不能发送到 AI");
        return;
      }
      const { from, to } = ed.state.selection;
      const text = ed.state.doc.textBetween(from, to, "\n");
      if (!text) {
        setAiStatus("请先在编辑器中选中文本");
        return;
      }
      setSelectionQuote({ filePath: path, text });
      setPrefillMessage(options?.prefill ?? null);
      setAiPanelOpen(true);
    },
    [activePathRef, editorRef, setAiStatus],
  );

  return {
    aiPanelOpen,
    assistantChrome,
    prefillMessage,
    selectionQuote,
    setAiPanelOpen,
    setAssistantChrome,
    setWebSearch,
    sendSelectionToAi,
    toggleWebSearch,
    webSearchEnabled,
  };
}
