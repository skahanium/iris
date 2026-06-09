import { useContext, useMemo, useRef, useState, type ReactNode } from "react";

import type { AssistantChromeSnapshot } from "@/types/assistant-chrome";
import { EMPTY_ASSISTANT_CHROME } from "@/types/assistant-chrome";

import { StatusBarContextValue, StatusBarCtx } from "./StatusBarContextValue";

// ── Provider ──

export function StatusBarProvider({ children }: { children: ReactNode }) {
  const [aiStatus, setAiStatus] = useState("AI 空闲");
  const [assistantChrome, setAssistantChrome] =
    useState<AssistantChromeSnapshot>(EMPTY_ASSISTANT_CHROME);
  const [editorStats, setEditorStats] = useState({
    characterCount: 0,
    readingMinutes: 1,
  });
  const [canUndo, setCanUndo] = useState(false);
  const [canRedo, setCanRedo] = useState(false);

  // Debounce editor stats so rapid keystrokes don't trigger a context update.
  const editorStatsRef = useRef(editorStats);
  editorStatsRef.current = editorStats;
  const editorStatsTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const updateEditorStats = useMemo(
    () => (stats: { characterCount: number; readingMinutes: number }) => {
      editorStatsRef.current = stats;
      if (editorStatsTimerRef.current) return;
      editorStatsTimerRef.current = setTimeout(() => {
        editorStatsTimerRef.current = null;
        setEditorStats({ ...editorStatsRef.current });
      }, 2000);
    },
    [],
  );

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
      aiStatus, assistantChrome, editorStats, updateEditorStats,
      canUndo, canRedo, setUndoRedo, setAiStatus, setAssistantChrome,
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
