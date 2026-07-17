import { useCallback, useState } from "react";

import { IrisContextMenu } from "@/components/ui/iris-context-menu";
import {
  filterEditorActions,
  groupContextMenuActions,
  isEditorActionEnabled,
  type EditorActionContext,
} from "@/lib/editor-actions";
import {
  applyTextFieldCaret,
  copyTextFieldSelection,
  IrisClipboardError,
  pasteIntoTextField,
} from "@/lib/iris-clipboard";
import type { MentionTextEdit } from "@/lib/ai-context-scope";

interface AiComposerContextMenuProps {
  textareaRef: React.RefObject<HTMLTextAreaElement | null>;
  value: string;
  onValueChange: (value: string, edit?: MentionTextEdit) => void;
  children: React.ReactNode;
}

/** AI 输入框自定义右键（剪贴板） */
export function AiComposerContextMenu({
  textareaRef,
  value,
  onValueChange,
  children,
}: AiComposerContextMenuProps) {
  const [menu, setMenu] = useState<{ open: boolean; x: number; y: number }>({
    open: false,
    x: 0,
    y: 0,
  });

  const handleContextMenu = useCallback((event: React.MouseEvent) => {
    event.preventDefault();
    event.stopPropagation();
    setMenu({ open: true, x: event.clientX, y: event.clientY });
  }, []);

  const ctx: EditorActionContext = {
    hasNote: true,
    hasSelection:
      textareaRef.current != null &&
      textareaRef.current.selectionStart !== textareaRef.current.selectionEnd,
    streaming: false,
  };

  const groups = groupContextMenuActions(
    filterEditorActions("context_menu", "ai_composer", ctx),
  ).map(({ group, items }) => ({
    group,
    items: items.map((a) => ({
      id: a.id,
      label: a.label,
      icon: a.icon,
      disabled: !isEditorActionEnabled(a, ctx),
    })),
  }));

  const runAction = useCallback(
    async (id: string) => {
      const el = textareaRef.current;
      if (!el) return;
      const start = el.selectionStart ?? 0;
      const end = el.selectionEnd ?? start;
      const selection = { start, end };

      try {
        switch (id) {
          case "copy":
            await copyTextFieldSelection(value, selection);
            break;
          case "paste": {
            const pasted = await pasteIntoTextField(value, selection);
            if (!pasted) return;
            onValueChange(pasted.value, {
              from: start,
              to: end,
              insertedTextLength:
                pasted.value.length - (value.length - (end - start)),
            });
            applyTextFieldCaret(el, pasted.caret);
            break;
          }
          case "select-all":
            el.select();
            break;
          default:
            break;
        }
      } catch (err) {
        if (err instanceof IrisClipboardError) {
          // ignore
        }
      }
    },
    [onValueChange, textareaRef, value],
  );

  return (
    <div onContextMenu={handleContextMenu}>
      {children}
      <IrisContextMenu
        open={menu.open}
        x={menu.x}
        y={menu.y}
        groups={groups}
        onSelect={(id) => void runAction(id)}
        onClose={() => setMenu({ open: false, x: 0, y: 0 })}
      />
    </div>
  );
}
