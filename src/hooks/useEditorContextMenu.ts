import type { Editor } from "@tiptap/react";
import { useCallback, useMemo, useState } from "react";

import type { IrisContextMenuGroup } from "@/components/ui/iris-context-menu";
import {
  filterEditorActions,
  groupContextMenuActions,
  isEditorActionEnabled,
  type EditorActionContext,
} from "@/lib/editor-actions";
import { editorHasActiveAiStream } from "@/lib/editor-ai-stream";

export interface EditorContextMenuState {
  open: boolean;
  x: number;
  y: number;
}

const closed: EditorContextMenuState = { open: false, x: 0, y: 0 };

const SELECTION_CONTEXT_HINT_KEY = "iris.hint.selection-context";

export function useEditorContextMenu(
  editor: Editor | null,
  hasNote: boolean,
  onSelectionHint?: () => void,
) {
  const [menu, setMenu] = useState<EditorContextMenuState>(closed);

  const groups = useMemo((): IrisContextMenuGroup[] => {
    if (!menu.open || !editor) return [];
    const { from, to } = editor.state.selection;
    const actionContext: EditorActionContext = {
      hasNote,
      hasSelection: from !== to,
      streaming: editorHasActiveAiStream(editor),
    };
    const actions = filterEditorActions(
      "context_menu",
      "editor",
      actionContext,
    );
    const withDocTranslate = actions.map((a) => {
      if (
        !actionContext.hasSelection &&
        (a.id === "translate" || a.id === "fix-grammar")
      ) {
        return { ...a, menuGroup: "ai_document" as const };
      }
      return a;
    });
    return groupContextMenuActions(withDocTranslate).map(
      ({ group, items }) => ({
        group,
        items: items.map((a) => ({
          id: a.id,
          label: a.label,
          icon: a.icon,
          disabled: !isEditorActionEnabled(a, actionContext),
        })),
      }),
    );
  }, [menu.open, editor, hasNote]);

  const openAt = useCallback((x: number, y: number) => {
    setMenu({ open: true, x, y });
  }, []);

  const close = useCallback(() => {
    setMenu(closed);
  }, []);

  const handleContextMenu = useCallback(
    (event: React.MouseEvent) => {
      if (!editor || !hasNote) return;
      event.preventDefault();
      event.stopPropagation();
      const { from, to } = editor.state.selection;
      if (
        from !== to &&
        onSelectionHint &&
        !localStorage.getItem(SELECTION_CONTEXT_HINT_KEY)
      ) {
        localStorage.setItem(SELECTION_CONTEXT_HINT_KEY, "1");
        onSelectionHint();
      }
      openAt(event.clientX, event.clientY);
    },
    [editor, hasNote, onSelectionHint, openAt],
  );

  return {
    menu,
    groups,
    openAt,
    close,
    handleContextMenu,
  };
}
