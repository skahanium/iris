import { useCallback, useMemo, useRef, useState } from "react";

import type { ContextReference } from "@/types/ai";

function sameUtf8Range(
  left: ContextReference["utf8Range"],
  right: ContextReference["utf8Range"],
) {
  if (left === right) return true;
  if (!left || !right) return false;
  return left.start === right.start && left.end === right.end;
}

function sameEditorRange(
  left: ContextReference["editorRange"],
  right: ContextReference["editorRange"],
) {
  if (left === right) return true;
  if (!left || !right) return false;
  return left.from === right.from && left.to === right.to;
}

function sameContextReference(left: ContextReference, right: ContextReference) {
  return (
    left.id === right.id &&
    left.kind === right.kind &&
    left.filePath === right.filePath &&
    left.contentHash === right.contentHash &&
    left.excerpt === right.excerpt &&
    left.stale === right.stale &&
    left.invalidReason === right.invalidReason &&
    sameUtf8Range(left.utf8Range, right.utf8Range) &&
    sameEditorRange(left.editorRange, right.editorRange) &&
    left.headingPath === right.headingPath &&
    left.anchor === right.anchor
  );
}

/**
 * AI 对话气泡选中状态管理。
 *
 * 支持三种选中模式：
 * - 单击：选中单条，清除其他
 * - Ctrl/Cmd+单击：切换选中状态（多选）
 * - Shift+单击：范围选中（从上次选中到当前）
 */
export function useAiBubbleSelection() {
  const [selected, setSelected] = useState<Set<number>>(() => new Set());
  const [contextReferences, setContextReferences] = useState<
    ContextReference[]
  >([]);
  const contextReferencesRef = useRef<ContextReference[]>([]);
  const lastIndexRef = useRef<number>(-1);

  const handleClick = useCallback(
    (
      index: number,
      event: { shiftKey: boolean; metaKey: boolean; ctrlKey: boolean },
    ) => {
      if (event.shiftKey && lastIndexRef.current >= 0) {
        // Range select
        const from = Math.min(lastIndexRef.current, index);
        const to = Math.max(lastIndexRef.current, index);
        setSelected((prev) => {
          const next = new Set(prev);
          for (let i = from; i <= to; i++) next.add(i);
          return next;
        });
      } else if (event.metaKey || event.ctrlKey) {
        // Toggle
        setSelected((prev) => {
          const next = new Set(prev);
          if (next.has(index)) next.delete(index);
          else next.add(index);
          return next;
        });
        lastIndexRef.current = index;
      } else {
        // Single select
        setSelected(new Set([index]));
        lastIndexRef.current = index;
      }
    },
    [],
  );

  const clear = useCallback(() => {
    setSelected(new Set());
    lastIndexRef.current = -1;
  }, []);

  const quoteSelectionAsReference = useCallback(
    (reference: ContextReference) => {
      const existing = contextReferencesRef.current.find(
        (item) => item.id === reference.id,
      );
      if (existing && sameContextReference(existing, reference)) {
        return;
      }
      setContextReferences((prev) => {
        const next = [
          ...prev.filter((item) => item.id !== reference.id),
          reference,
        ];
        contextReferencesRef.current = next;
        return next;
      });
    },
    [],
  );

  const removeContextReference = useCallback((id: string) => {
    if (!contextReferencesRef.current.some((item) => item.id === id)) {
      return;
    }
    setContextReferences((prev) => {
      const next = prev.filter((item) => item.id !== id);
      contextReferencesRef.current = next;
      return next;
    });
  }, []);

  const clearContextReferences = useCallback(() => {
    if (contextReferencesRef.current.length === 0) {
      return;
    }
    contextReferencesRef.current = [];
    setContextReferences([]);
  }, []);

  const isSelected = useCallback(
    (index: number) => selected.has(index),
    [selected],
  );

  return useMemo(
    () => ({
      selected,
      contextReferences,
      handleClick,
      clear,
      isSelected,
      quoteSelectionAsReference,
      removeContextReference,
      clearContextReferences,
    }),
    [
      selected,
      contextReferences,
      handleClick,
      clear,
      isSelected,
      quoteSelectionAsReference,
      removeContextReference,
      clearContextReferences,
    ],
  );
}
