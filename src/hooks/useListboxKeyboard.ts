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
  /** 当前索引是否不可激活（Enter） */
  isIndexDisabled?: (index: number) => boolean;
  /** 为 false 时 ↑↓ 可移到 disabled 项（仅命令面板：可见但不可执行） */
  skipDisabledOnNavigate?: boolean;
}

export interface ListboxKeyboardKeyEvent {
  key: string;
  preventDefault: () => void;
}

function firstSelectableIndex(
  length: number,
  isDisabled?: (index: number) => boolean,
): number {
  if (length === 0) return 0;
  for (let i = 0; i < length; i++) {
    if (!isDisabled?.(i)) return i;
  }
  return 0;
}

/** 从 from 向两侧找最近的可选项，避免纠正时跳回列表顶部 */
function nearestSelectableIndex(
  length: number,
  from: number,
  isDisabled?: (index: number) => boolean,
): number {
  if (length === 0) return 0;
  if (!isDisabled?.(from)) return from;
  for (let offset = 1; offset < length; offset++) {
    const down = from + offset;
    if (down < length && !isDisabled(down)) return down;
    const up = from - offset;
    if (up >= 0 && !isDisabled(up)) return up;
  }
  return firstSelectableIndex(length, isDisabled);
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
  skipDisabledOnNavigate = true,
}: UseListboxKeyboardOptions) {
  const [highlight, setHighlightState] = useState(0);
  const lengthRef = useRef(length);
  const navDeltaRef = useRef<1 | -1 | 0>(0);
  const onActivateRef = useRef(onActivate);
  const isIndexDisabledRef = useRef(isIndexDisabled);
  const skipDisabledOnNavigateRef = useRef(skipDisabledOnNavigate);

  lengthRef.current = length;
  onActivateRef.current = onActivate;
  isIndexDisabledRef.current = isIndexDisabled;
  skipDisabledOnNavigateRef.current = skipDisabledOnNavigate;

  useEffect(() => {
    setHighlightState(
      skipDisabledOnNavigateRef.current
        ? firstSelectableIndex(lengthRef.current, isIndexDisabledRef.current)
        : 0,
    );
    navDeltaRef.current = 0;
  }, [resetKey]);

  useEffect(() => {
    if (highlight >= length) {
      setHighlightState(Math.max(0, length - 1));
      return;
    }
    if (
      skipDisabledOnNavigateRef.current &&
      isIndexDisabledRef.current?.(highlight)
    ) {
      const corrected = nearestSelectableIndex(
        length,
        highlight,
        isIndexDisabledRef.current,
      );
      setHighlightState(corrected);
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

        let next = current;
        for (let step = 0; step < len; step++) {
          if (wrap) {
            next = (next + delta + len) % len;
          } else {
            next += delta;
            if (next < 0 || next >= len) {
              navDeltaRef.current = 0;
              return current;
            }
          }
          if (
            !skipDisabledOnNavigateRef.current ||
            !isIndexDisabledRef.current?.(next)
          ) {
            return next;
          }
        }
        navDeltaRef.current = 0;
        return current;
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
