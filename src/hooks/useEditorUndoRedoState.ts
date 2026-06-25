import type { Editor } from "@tiptap/react";
import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type RefObject,
} from "react";

interface UseEditorUndoRedoStateOptions {
  activePath: string | null;
  editorRef: RefObject<Editor | null>;
}

export function useEditorUndoRedoState({
  activePath,
  editorRef,
}: UseEditorUndoRedoStateOptions) {
  const [editorInstance, setEditorInstance] = useState<Editor | null>(null);
  const [canUndo, setCanUndo] = useState(false);
  const [canRedo, setCanRedo] = useState(false);
  const stateRef = useRef({ canUndo: false, canRedo: false });
  const cleanupRef = useRef<(() => void) | null>(null);
  const rafRef = useRef<number | null>(null);

  const updateState = useCallback((ed: Editor | null) => {
    const next = ed
      ? {
          canUndo: ed.can().undo(),
          canRedo: ed.can().redo(),
        }
      : { canUndo: false, canRedo: false };
    const prev = stateRef.current;
    stateRef.current = next;
    if (prev.canUndo !== next.canUndo) setCanUndo(next.canUndo);
    if (prev.canRedo !== next.canRedo) setCanRedo(next.canRedo);
  }, []);

  const cancelRefresh = useCallback(() => {
    if (rafRef.current === null) return;
    cancelAnimationFrame(rafRef.current);
    rafRef.current = null;
  }, []);

  const scheduleUndoRedoStateRefresh = useCallback(
    (ed: Editor | null = editorRef.current) => {
      if (rafRef.current !== null) {
        cancelAnimationFrame(rafRef.current);
      }
      rafRef.current = requestAnimationFrame(() => {
        rafRef.current = null;
        const currentEditor = ed && !ed.isDestroyed ? ed : editorRef.current;
        updateState(
          currentEditor && !currentEditor.isDestroyed ? currentEditor : null,
        );
      });
    },
    [editorRef, updateState],
  );

  const clearTransactionListener = useCallback(() => {
    cleanupRef.current?.();
    cleanupRef.current = null;
  }, []);

  useEffect(() => {
    if (!activePath) {
      clearTransactionListener();
      cancelRefresh();
      setEditorInstance(null);
      updateState(null);
    }
  }, [activePath, cancelRefresh, clearTransactionListener, updateState]);

  useEffect(() => cancelRefresh, [cancelRefresh]);

  const handleEditorReady = useCallback(
    (ed: Editor | null) => {
      clearTransactionListener();
      editorRef.current = ed;
      if (!ed) {
        cancelRefresh();
        setEditorInstance(null);
        updateState(null);
        return;
      }

      setEditorInstance(ed);
      updateState(ed);

      const handleTransaction = ({ editor }: { editor: Editor }) => {
        scheduleUndoRedoStateRefresh(editor);
      };

      ed.on("transaction", handleTransaction);
      cleanupRef.current = () => {
        ed.off("transaction", handleTransaction);
      };
    },
    [
      cancelRefresh,
      clearTransactionListener,
      editorRef,
      scheduleUndoRedoStateRefresh,
      updateState,
    ],
  );

  return {
    canRedo,
    canUndo,
    editorInstance,
    handleEditorReady,
    scheduleUndoRedoStateRefresh,
  };
}
