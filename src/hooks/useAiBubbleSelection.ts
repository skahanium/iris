import { useCallback, useRef, useState } from "react";

import type { ContextReference } from "@/types/ai";

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
      setContextReferences((prev) => {
        const next = prev.filter((item) => item.id !== reference.id);
        return [...next, reference];
      });
    },
    [],
  );

  const removeContextReference = useCallback((id: string) => {
    setContextReferences((prev) => prev.filter((item) => item.id !== id));
  }, []);

  const clearContextReferences = useCallback(() => {
    setContextReferences([]);
  }, []);

  const isSelected = useCallback(
    (index: number) => selected.has(index),
    [selected],
  );

  return {
    selected,
    contextReferences,
    handleClick,
    clear,
    isSelected,
    quoteSelectionAsReference,
    removeContextReference,
    clearContextReferences,
  };
}
