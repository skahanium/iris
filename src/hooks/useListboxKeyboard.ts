import { useCallback, useEffect, useRef, useState } from "react";

export interface UseListboxKeyboardOptions {
  /** 当前列表项数量 */
  length: number;
  /** 是否响应键盘（如浮层未打开时可设为 false） */
  enabled?: boolean;
  /** 在首尾循环（Slash 菜单）还是夹紧（命令面板） */
  wrap?: boolean;
  /** 变化时重置高亮到 0（如搜索词、打开浮层） */
  resetKey?: string | number;
  /** Enter 时激活当前项 */
  onActivate?: (index: number) => void;
  /** 当前索引是否不可激活 */
  isIndexDisabled?: (index: number) => boolean;
}

export interface ListboxKeyboardKeyEvent {
  key: string;
  preventDefault: () => void;
}

/**
 * 命令列表 / Quick Open / Slash 菜单共享的 ↑↓ Enter 键盘导航。
 */
export function useListboxKeyboard({
  length,
  enabled = true,
  wrap = false,
  resetKey,
  onActivate,
  isIndexDisabled,
}: UseListboxKeyboardOptions) {
  const [highlight, setHighlightState] = useState(0);
  const lengthRef = useRef(length);
  const navDeltaRef = useRef<1 | -1 | 0>(0);
  const onActivateRef = useRef(onActivate);
  const isIndexDisabledRef = useRef(isIndexDisabled);

  lengthRef.current = length;
  onActivateRef.current = onActivate;
  isIndexDisabledRef.current = isIndexDisabled;

  useEffect(() => {
    setHighlightState(0);
    navDeltaRef.current = 0;
  }, [resetKey]);

  useEffect(() => {
    if (highlight >= length) {
      setHighlightState(Math.max(0, length - 1));
    }
  }, [length, highlight]);

  const setHighlight = useCallback((index: number) => {
    navDeltaRef.current = 0;
    setHighlightState(index);
  }, []);

  const moveHighlight = useCallback(
    (delta: 1 | -1) => {
      if (lengthRef.current === 0) return;
      navDeltaRef.current = delta;
      setHighlightState((current) => {
        const len = lengthRef.current;
        if (len === 0) return 0;
        const next = current + delta;
        if (wrap) {
          return (next + len) % len;
        }
        if (next < 0 || next >= len) {
          navDeltaRef.current = 0;
          return current;
        }
        return next;
      });
    },
    [wrap],
  );

  const handleKeyDown = useCallback(
    (event: ListboxKeyboardKeyEvent): boolean => {
      if (!enabled) return false;
      if (event.key === "ArrowDown") {
        event.preventDefault();
        moveHighlight(1);
        return true;
      }
      if (event.key === "ArrowUp") {
        event.preventDefault();
        moveHighlight(-1);
        return true;
      }
      if (event.key === "Enter") {
        event.preventDefault();
        const index = highlight;
        if (isIndexDisabledRef.current?.(index)) {
          return true;
        }
        onActivateRef.current?.(index);
        return true;
      }
      return false;
    },
    [enabled, highlight, moveHighlight],
  );

  return {
    highlight,
    setHighlight,
    moveHighlight,
    handleKeyDown,
    navDeltaRef,
  };
}
