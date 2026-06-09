import { createContext, type Dispatch, type SetStateAction } from "react";

import type { AssistantChromeSnapshot } from "@/types/assistant-chrome";

export interface StatusBarContextValue {
  aiStatus: string;
  setAiStatus: Dispatch<SetStateAction<string>>;
  assistantChrome: AssistantChromeSnapshot;
  setAssistantChrome: Dispatch<SetStateAction<AssistantChromeSnapshot>>;
  /** Debounced editor stats — updates at most every 2s via `updateEditorStats`. */
  editorStats: { characterCount: number; readingMinutes: number };
  updateEditorStats: (stats: {
    characterCount: number;
    readingMinutes: number;
  }) => void;
  canUndo: boolean;
  canRedo: boolean;
  setUndoRedo: (undo: boolean, redo: boolean) => void;
}

export const StatusBarCtx = createContext<StatusBarContextValue | null>(null);
