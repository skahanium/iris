import { useCallback, useEffect, useMemo, useState } from "react";

import { IrisContextMenu } from "@/components/ui/iris-context-menu";
import {
  filterEditorActions,
  groupContextMenuActions,
  isEditorActionEnabled,
  type EditorActionContext,
} from "@/lib/editor-actions";
import { useAiMessageSelection } from "@/hooks/useAiMessageSelection";
import { writeClipboardText } from "@/lib/iris-clipboard";

interface AiMessageSelectionUiProps {
  messageListRef: React.RefObject<HTMLDivElement | null>;
  streaming: boolean;
  onQuoteToInput: (text: string) => void;
}

function selectionTextInRoot(root: HTMLElement): string {
  const sel = window.getSelection();
  if (!sel || sel.isCollapsed || !sel.rangeCount) return "";
  const range = sel.getRangeAt(0);
  if (!root.contains(range.commonAncestorContainer)) return "";
  return sel.toString().trim();
}

/** AI 消息区右键菜单（`iris_only`，无选区浮动条） */
export function AiMessageSelectionUi({
  messageListRef,
  streaming,
  onQuoteToInput,
}: AiMessageSelectionUiProps) {
  const { selection, sync } = useAiMessageSelection(messageListRef);
  const [selectionSnapshot, setSelectionSnapshot] = useState("");
  const [menu, setMenu] = useState<{ open: boolean; x: number; y: number }>({
    open: false,
    x: 0,
    y: 0,
  });

  const ctx: EditorActionContext = useMemo(
    () => ({
      hasNote: true,
      hasSelection: Boolean(selectionSnapshot || selection.text),
      streaming,
    }),
    [selection.text, selectionSnapshot, streaming],
  );

  const menuGroups = useMemo(
    () =>
      groupContextMenuActions(
        filterEditorActions("context_menu", "ai_message", ctx),
      ).map(({ group, items }) => ({
        group,
        items: items.map((a) => ({
          id: a.id,
          label: a.label,
          icon: a.icon,
          disabled: !isEditorActionEnabled(a, ctx),
        })),
      })),
    [ctx],
  );

  const runAction = useCallback(
    async (id: string, textOverride?: string) => {
      const text = textOverride ?? (selectionSnapshot || selection.text);
      if (!text) return;
      if (id === "copy") {
        await writeClipboardText(text);
        return;
      }
      if (id === "quote-to-input") {
        onQuoteToInput(text);
      }
    },
    [onQuoteToInput, selection.text, selectionSnapshot],
  );

  const handleContextMenu = useCallback(
    (event: MouseEvent) => {
      const root = messageListRef.current;
      if (!root) return;
      const target = event.target;
      if (!(target instanceof Node) || !root.contains(target)) return;

      const text = selectionTextInRoot(root);
      if (!text) return;

      event.preventDefault();
      event.stopPropagation();
      setSelectionSnapshot(text);
      sync();
      setMenu({ open: true, x: event.clientX, y: event.clientY });
    },
    [messageListRef, sync],
  );

  useEffect(() => {
    const root = messageListRef.current;
    if (!root) return;
    root.addEventListener("contextmenu", handleContextMenu);
    return () => root.removeEventListener("contextmenu", handleContextMenu);
  }, [messageListRef, handleContextMenu]);

  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.defaultPrevented) return;
      if (event.key.toLowerCase() !== "c") return;
      if (!event.metaKey && !event.ctrlKey) return;
      const target = event.target;
      if (
        target instanceof HTMLInputElement ||
        target instanceof HTMLTextAreaElement ||
        (target instanceof HTMLElement && target.isContentEditable)
      ) {
        return;
      }
      const root = messageListRef.current;
      if (!root) return;
      const text = selectionTextInRoot(root);
      if (!text) return;
      event.preventDefault();
      void writeClipboardText(text);
    };

    document.addEventListener("keydown", handleKeyDown);
    return () => document.removeEventListener("keydown", handleKeyDown);
  }, [messageListRef]);

  return (
    <IrisContextMenu
      open={menu.open}
      x={menu.x}
      y={menu.y}
      groups={menuGroups}
      onSelect={(id) => {
        void runAction(id, selectionSnapshot || undefined);
      }}
      onClose={() => {
        setMenu({ open: false, x: 0, y: 0 });
        setSelectionSnapshot("");
      }}
    />
  );
}
