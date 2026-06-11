import { useContext, useMemo, useState, type ReactNode } from "react";

import { useEditorStats } from "@/hooks/useEditorStats";
import type { AssistantChromeSnapshot } from "@/types/assistant-chrome";
import { EMPTY_ASSISTANT_CHROME } from "@/types/assistant-chrome";

import { StatusBarContextValue, StatusBarCtx } from "./StatusBarContextValue";

// ── Provider ──

export function StatusBarProvider({ children }: { children: ReactNode }) {
  const [aiStatus, setAiStatus] = useState("AI 空闲");
  const [assistantChrome, setAssistantChrome] =
    useState<AssistantChromeSnapshot>(EMPTY_ASSISTANT_CHROME);
  const { editorStats, updateEditorStats } = useEditorStats();
  const [canUndo, setCanUndo] = useState(false);
  const [canRedo, setCanRedo] = useState(false);

  const setUndoRedo = useMemo(
    () => (undo: boolean, redo: boolean) => {
      setCanUndo(undo);
      setCanRedo(redo);
    },
    [],
  );

  const value = useMemo<StatusBarContextValue>(
    () => ({
      aiStatus,
      setAiStatus,
      assistantChrome,
      setAssistantChrome,
      editorStats,
      updateEditorStats,
      canUndo,
      canRedo,
      setUndoRedo,
    }),
    [
      aiStatus,
      assistantChrome,
      editorStats,
      updateEditorStats,
      canUndo,
      canRedo,
      setUndoRedo,
      setAiStatus,
      setAssistantChrome,
    ],
  );

  return (
    <StatusBarCtx.Provider value={value}>{children}</StatusBarCtx.Provider>
  );
}

// ── Hook ──

// eslint-disable-next-line react-refresh/only-export-components
export function useStatusBar(): StatusBarContextValue {
  const ctx = useContext(StatusBarCtx);
  if (!ctx) {
    throw new Error("useStatusBar must be used within <StatusBarProvider>");
  }
  return ctx;
}
