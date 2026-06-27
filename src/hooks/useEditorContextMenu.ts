import type { Editor } from "@tiptap/react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";

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

export interface EditorContextMenuDomainContext {
  aiDomain?: "normal" | "classified";
  classifiedUnlocked?: boolean;
}

const closed: EditorContextMenuState = { open: false, x: 0, y: 0 };

const SELECTION_CONTEXT_HINT_KEY = "iris.hint.selection-context";

interface EditorSelectionRange {
  from: number;
  to: number;
}

export function useEditorContextMenu(
  editor: Editor | null,
  hasNote: boolean,
  onSelectionHint?: () => void,
  locked = false,
  domainContext: EditorContextMenuDomainContext = {},
) {
  const [menu, setMenu] = useState<EditorContextMenuState>(closed);
  const lastSelectionRef = useRef<EditorSelectionRange | null>(null);
  const preserveSelectionUntilRef = useRef(0);

  useEffect(() => {
    lastSelectionRef.current = null;
    preserveSelectionUntilRef.current = 0;
    if (!editor) return;

    const rememberSelection = () => {
      const { from, to } = editor.state.selection;
      if (from !== to) {
        lastSelectionRef.current = { from, to };
        return;
      }
      if (Date.now() > preserveSelectionUntilRef.current) {
        lastSelectionRef.current = null;
      }
    };

    const preserveSelectionOnRightMouseDown = (event: MouseEvent) => {
      if (event.button !== 2) return;
      const { from, to } = editor.state.selection;
      if (from === to) return;
      lastSelectionRef.current = { from, to };
      preserveSelectionUntilRef.current = Date.now() + 500;
    };

    rememberSelection();
    editor.view.dom.addEventListener(
      "mousedown",
      preserveSelectionOnRightMouseDown,
      true,
    );
    editor.on("selectionUpdate", rememberSelection);
    return () => {
      editor.view.dom.removeEventListener(
        "mousedown",
        preserveSelectionOnRightMouseDown,
        true,
      );
      editor.off("selectionUpdate", rememberSelection);
    };
  }, [editor]);

  const restoreSelectionForContextMenu = useCallback(() => {
    if (!editor) return false;
    const { from, to } = editor.state.selection;
    if (from !== to) {
      lastSelectionRef.current = { from, to };
      return true;
    }

    const previous = lastSelectionRef.current;
    if (!previous) return false;
    const max = editor.state.doc.content.size;
    if (
      previous.from < 0 ||
      previous.to < 0 ||
      previous.from > max ||
      previous.to > max ||
      previous.from === previous.to
    ) {
      lastSelectionRef.current = null;
      return false;
    }

    editor.commands.setTextSelection(previous);
    preserveSelectionUntilRef.current = 0;
    return true;
  }, [editor]);

  const groups = useMemo((): IrisContextMenuGroup[] => {
    if (!menu.open || !editor) return [];
    const { from, to } = editor.state.selection;
    const actionContext: EditorActionContext = {
      hasNote,
      hasSelection: from !== to,
      streaming: editorHasActiveAiStream(editor),
      isLocked: locked,
      aiDomain: domainContext.aiDomain,
      classifiedUnlocked: domainContext.classifiedUnlocked,
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
  }, [
    menu.open,
    editor,
    hasNote,
    locked,
    domainContext.aiDomain,
    domainContext.classifiedUnlocked,
  ]);

  const openAt = useCallback((x: number, y: number) => {
    setMenu({ open: true, x, y });
  }, []);

  const close = useCallback(() => {
    setMenu(closed);
  }, []);

  const handleContextMenu = useCallback(
    (event: React.MouseEvent) => {
      if (locked) return;
      if (!editor || !hasNote) return;
      event.preventDefault();
      event.stopPropagation();
      restoreSelectionForContextMenu();
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
    [
      editor,
      hasNote,
      locked,
      onSelectionHint,
      openAt,
      restoreSelectionForContextMenu,
    ],
  );

  return {
    menu,
    groups,
    openAt,
    close,
    handleContextMenu,
  };
}
