import { useCallback, useEffect, useRef, useState } from "react";

import {
  clampSessionCharDeltaForDisplay,
  type SessionCharDelta,
} from "@/lib/session-char-delta";

export interface EditorStats {
  characterCount: number;
  readingMinutes: number;
  sessionCharsAdded: number;
  sessionCharsRemoved: number;
}

export interface EditorStatsUpdate {
  characterCount: number;
  readingMinutes: number;
}

const DEFAULT_EDITOR_STATS: EditorStats = {
  characterCount: 0,
  readingMinutes: 1,
  sessionCharsAdded: 0,
  sessionCharsRemoved: 0,
};

const EDITOR_STATS_UI_DEBOUNCE_MS = 2000;

function emptyDelta(): SessionCharDelta {
  return { added: 0, removed: 0 };
}

export function useEditorStats() {
  const [editorStats, setEditorStats] =
    useState<EditorStats>(DEFAULT_EDITOR_STATS);
  const editorStatsRef = useRef<EditorStats>(DEFAULT_EDITOR_STATS);
  const editorStatsTimerRef = useRef<ReturnType<typeof setTimeout> | null>(
    null,
  );
  const sessionAccumRef = useRef(new Map<string, SessionCharDelta>());
  const sessionBaselineCharCountRef = useRef(new Map<string, number>());
  const activeSessionIdRef = useRef<string | null>(null);

  const displayFromAccum = useCallback(
    (sessionId: string): SessionCharDelta => {
      const accum = sessionAccumRef.current.get(sessionId) ?? emptyDelta();
      return clampSessionCharDeltaForDisplay(accum);
    },
    [],
  );

  const sessionDisplayDelta = useCallback(
    (sessionId: string, characterCount: number): SessionCharDelta => {
      const baseline = sessionBaselineCharCountRef.current.get(sessionId);
      if (baseline !== undefined && baseline === characterCount) {
        return emptyDelta();
      }
      return displayFromAccum(sessionId);
    },
    [displayFromAccum],
  );

  const syncActiveSessionToUi = useCallback(
    (characterCount?: number) => {
      const sessionId = activeSessionIdRef.current;
      if (sessionId === null) {
        return;
      }
      const count = characterCount ?? editorStatsRef.current.characterCount;
      const display = sessionDisplayDelta(sessionId, count);
      editorStatsRef.current = {
        ...editorStatsRef.current,
        sessionCharsAdded: display.added,
        sessionCharsRemoved: display.removed,
      };
    },
    [sessionDisplayDelta],
  );

  const flushEditorStatsToUi = useCallback(() => {
    syncActiveSessionToUi();
    setEditorStats({ ...editorStatsRef.current });
  }, [syncActiveSessionToUi]);

  const scheduleEditorStatsUiFlush = useCallback(() => {
    if (editorStatsTimerRef.current) return;
    editorStatsTimerRef.current = setTimeout(() => {
      editorStatsTimerRef.current = null;
      flushEditorStatsToUi();
    }, EDITOR_STATS_UI_DEBOUNCE_MS);
  }, [flushEditorStatsToUi]);

  const setActiveEditorSession = useCallback(
    (sessionId: string | null) => {
      if (activeSessionIdRef.current === sessionId) return;
      activeSessionIdRef.current = sessionId;
      if (sessionId === null) {
        return;
      }
      const display = displayFromAccum(sessionId);
      editorStatsRef.current = {
        ...editorStatsRef.current,
        sessionCharsAdded: display.added,
        sessionCharsRemoved: display.removed,
      };
      setEditorStats({ ...editorStatsRef.current });
    },
    [displayFromAccum],
  );

  const resetSessionCharDelta = useCallback(
    (sessionId: string, baselineCharacterCount?: number) => {
      sessionAccumRef.current.set(sessionId, emptyDelta());
      if (baselineCharacterCount !== undefined) {
        sessionBaselineCharCountRef.current.set(
          sessionId,
          baselineCharacterCount,
        );
      }
      if (activeSessionIdRef.current === sessionId) {
        if (editorStatsTimerRef.current) {
          clearTimeout(editorStatsTimerRef.current);
          editorStatsTimerRef.current = null;
        }
        syncActiveSessionToUi(baselineCharacterCount);
        setEditorStats({ ...editorStatsRef.current });
      }
    },
    [syncActiveSessionToUi],
  );

  const applySessionCharDelta = useCallback(
    (sessionId: string, delta: SessionCharDelta) => {
      const prev = sessionAccumRef.current.get(sessionId) ?? emptyDelta();
      sessionAccumRef.current.set(sessionId, {
        added: prev.added + delta.added,
        removed: prev.removed + delta.removed,
      });
      if (activeSessionIdRef.current !== sessionId) {
        return;
      }
      syncActiveSessionToUi();
      scheduleEditorStatsUiFlush();
    },
    [scheduleEditorStatsUiFlush, syncActiveSessionToUi],
  );

  const updateEditorStats = useCallback(
    (stats: EditorStatsUpdate) => {
      editorStatsRef.current = {
        characterCount: stats.characterCount,
        readingMinutes: stats.readingMinutes,
        sessionCharsAdded: editorStatsRef.current.sessionCharsAdded,
        sessionCharsRemoved: editorStatsRef.current.sessionCharsRemoved,
      };
      syncActiveSessionToUi(stats.characterCount);
      scheduleEditorStatsUiFlush();
    },
    [scheduleEditorStatsUiFlush, syncActiveSessionToUi],
  );

  const resetEditorStats = useCallback(() => {
    sessionAccumRef.current.clear();
    sessionBaselineCharCountRef.current.clear();
    activeSessionIdRef.current = null;
    editorStatsRef.current = DEFAULT_EDITOR_STATS;
    setEditorStats({ ...DEFAULT_EDITOR_STATS });
    if (editorStatsTimerRef.current) {
      clearTimeout(editorStatsTimerRef.current);
      editorStatsTimerRef.current = null;
    }
  }, []);

  const clearSessionCharDelta = useCallback((sessionId: string) => {
    sessionAccumRef.current.delete(sessionId);
    sessionBaselineCharCountRef.current.delete(sessionId);
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
    resetSessionCharDelta,
    applySessionCharDelta,
    setActiveEditorSession,
    clearSessionCharDelta,
  };
}
