import { useCallback, useEffect, useRef, useState } from "react";

export interface AiSelectionState {
  text: string;
}

const empty: AiSelectionState = { text: "" };

function selectionInRoot(root: HTMLElement): AiSelectionState {
  const sel = window.getSelection();
  if (!sel || sel.isCollapsed || sel.rangeCount === 0) return empty;
  const range = sel.getRangeAt(0);
  if (!root.contains(range.commonAncestorContainer)) return empty;
  const text = sel.toString().trim();
  if (!text) return empty;
  return { text };
}

/** 监听 AI 消息区域内的文字选区 */
export function useAiMessageSelection(
  rootRef: React.RefObject<HTMLElement | null>,
) {
  const [selection, setSelection] = useState<AiSelectionState>(empty);
  const throttleRef = useRef<number | null>(null);

  const sync = useCallback(() => {
    const root = rootRef.current;
    if (!root) {
      setSelection(empty);
      return;
    }
    setSelection(selectionInRoot(root));
  }, [rootRef]);

  useEffect(() => {
    const onSelectionChange = () => {
      if (throttleRef.current != null) return;
      throttleRef.current = window.setTimeout(() => {
        throttleRef.current = null;
        sync();
      }, 80);
    };
    document.addEventListener("selectionchange", onSelectionChange);
    return () => {
      document.removeEventListener("selectionchange", onSelectionChange);
      if (throttleRef.current != null) {
        window.clearTimeout(throttleRef.current);
      }
    };
  }, [sync]);

  const clear = useCallback(() => {
    setSelection(empty);
    const sel = window.getSelection();
    sel?.removeAllRanges();
  }, []);

  return { selection, sync, clear };
}
