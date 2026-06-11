import { useCallback, useEffect, useRef, useState } from "react";

export interface EditorStats {
  characterCount: number;
  readingMinutes: number;
}

const DEFAULT_EDITOR_STATS: EditorStats = {
  characterCount: 0,
  readingMinutes: 1,
};

export function useEditorStats() {
  const [editorStats, setEditorStats] =
    useState<EditorStats>(DEFAULT_EDITOR_STATS);
  const editorStatsRef = useRef<EditorStats>(DEFAULT_EDITOR_STATS);
  const editorStatsTimerRef = useRef<ReturnType<typeof setTimeout> | null>(
    null,
  );

  const updateEditorStats = useCallback((stats: EditorStats) => {
    editorStatsRef.current = stats;
    if (editorStatsTimerRef.current) return;
    editorStatsTimerRef.current = setTimeout(() => {
      editorStatsTimerRef.current = null;
      setEditorStats({ ...editorStatsRef.current });
    }, 2000);
  }, []);

  const resetEditorStats = useCallback(() => {
    editorStatsRef.current = DEFAULT_EDITOR_STATS;
    setEditorStats({ ...DEFAULT_EDITOR_STATS });
  }, []);

  useEffect(() => {
    return () => {
      if (editorStatsTimerRef.current) {
        clearTimeout(editorStatsTimerRef.current);
        editorStatsTimerRef.current = null;
      }
    };
  }, []);

  return {
    editorStats,
    updateEditorStats,
    resetEditorStats,
  };
}
