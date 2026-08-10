import { useCallback, useState, type RefObject } from "react";

import { IrisContextMenu } from "@/components/ui/iris-context-menu";
import type { AssistantComposerHandle } from "@/components/ui/ai-composer";
import {
  filterEditorActions,
  groupContextMenuActions,
  isEditorActionEnabled,
  type EditorActionContext,
} from "@/lib/editor-actions";
import {
  copyTextFieldSelection,
  IrisClipboardError,
  pasteIntoTextField,
} from "@/lib/iris-clipboard";

interface AiComposerContextMenuProps {
  composerRef: RefObject<AssistantComposerHandle | null>;
  children: React.ReactNode;
}

/** AI 输入框自定义右键：剪切、复制、粘贴、全选。 */
export function AiComposerContextMenu({
  composerRef,
  children,
}: AiComposerContextMenuProps) {
  const [menu, setMenu] = useState<{ open: boolean; x: number; y: number }>({
    open: false,
    x: 0,
    y: 0,
  });
  const editor = composerRef.current?.getEditor() ?? null;
  const selection = editor?.state.selection;
  const selectedText =
    editor && selection && !selection.empty
      ? editor.state.doc.textBetween(selection.from, selection.to, "\n", "\n")
      : "";
  const ctx: EditorActionContext = {
    hasNote: true,
    hasSelection: selectedText.length > 0,
    streaming: false,
  };
  const groups = groupContextMenuActions(
    filterEditorActions("context_menu", "ai_composer", ctx),
  ).map(({ group, items }) => ({
    group,
    items: items.map((action) => ({
      id: action.id,
      label: action.label,
      icon: action.icon,
      disabled: !isEditorActionEnabled(action, ctx),
    })),
  }));

  const handleContextMenu = useCallback((event: React.MouseEvent) => {
    event.preventDefault();
    event.stopPropagation();
    setMenu({ open: true, x: event.clientX, y: event.clientY });
  }, []);

  const runAction = useCallback(
    async (id: string) => {
      const currentEditor = composerRef.current?.getEditor();
      if (!currentEditor) return;
      const range = currentEditor.state.selection;
      const text = !range.empty
        ? currentEditor.state.doc.textBetween(range.from, range.to, "\n", "\n")
        : "";
      try {
        switch (id) {
          case "copy":
            if (text)
              await copyTextFieldSelection(text, {
                start: 0,
                end: text.length,
              });
            break;
          case "cut":
            if (text) {
              await copyTextFieldSelection(text, {
                start: 0,
                end: text.length,
              });
              currentEditor.commands.deleteSelection();
            }
            break;
          case "paste": {
            const pasted = await pasteIntoTextField(text, {
              start: 0,
              end: text.length,
            });
            if (pasted)
              currentEditor.chain().focus().insertContent(pasted.value).run();
            break;
          }
          case "select-all":
            currentEditor.commands.selectAll();
            break;
          default:
            break;
        }
      } catch (error) {
        if (error instanceof IrisClipboardError) return;
      }
    },
    [composerRef],
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
