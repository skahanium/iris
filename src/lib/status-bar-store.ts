// ── Module-level external store for high-churn UI state ───────────────────
//
// This store lets App.tsx set status-bar state without causing the entire
// component tree to re-render.  Only components that explicitly subscribe
// (via `useSyncExternalStore`) receive updates.
//
// Why not React Context?  Every context-value change re-renders *all*
// consumers, even components that only read a different slice of the value.
// An external store with `useSyncExternalStore` is the idiomatic React 18+
// pattern for this: per-component granular re-renders, zero boilerplate.

import { useSyncExternalStore } from "react";

import type { AssistantChromeSnapshot } from "@/types/assistant-chrome";
import { EMPTY_ASSISTANT_CHROME } from "@/types/assistant-chrome";

// ── Types ──────────────────────────────────────────────────────────────────

export interface EditorStats {
  characterCount: number;
  readingMinutes: number;
}

interface Store {
  aiStatus: string;
  assistantChrome: AssistantChromeSnapshot;
  editorStats: EditorStats;
  canUndo: boolean;
  canRedo: boolean;
}

// ── Store internals ────────────────────────────────────────────────────────

let store: Store = {
  aiStatus: "AI 空闲",
  assistantChrome: EMPTY_ASSISTANT_CHROME,
  editorStats: { characterCount: 0, readingMinutes: 1 },
  canUndo: false,
  canRedo: false,
};

const listeners = new Set<() => void>();

function notify() {
  for (const fn of listeners) fn();
}

function subscribe(fn: () => void): () => void {
  listeners.add(fn);
  return () => {
    listeners.delete(fn);
  };
}

function getSnapshot(): Store {
  return store;
}

// ── Setters (called from App.tsx, inside or outside React) ─────────────────

export function setAiStatus(value: string) {
  if (store.aiStatus !== value) {
    store = { ...store, aiStatus: value };
    notify();
  }
}

export function setEditorStats(value: EditorStats) {
  const prev = store.editorStats;
  if (
    prev.characterCount !== value.characterCount ||
    prev.readingMinutes !== value.readingMinutes
  ) {
    store = { ...store, editorStats: value };
    notify();
  }
}

export function setUndoRedo(undo: boolean, redo: boolean) {
  if (store.canUndo !== undo || store.canRedo !== redo) {
    store = { ...store, canUndo: undo, canRedo: redo };
    notify();
  }
}

export function setAssistantChrome(value: AssistantChromeSnapshot) {
  store = { ...store, assistantChrome: value };
  notify();
}

// ── React hook (called from StatusBar or any subscriber) ───────────────────

export function useStatusBarStore(): Store {
  return useSyncExternalStore(subscribe, getSnapshot, getSnapshot);
}
