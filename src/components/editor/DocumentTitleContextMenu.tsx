import { useCallback, useMemo, useState } from "react";

import { IrisContextMenu } from "@/components/ui/iris-context-menu";
import {
  applyTextFieldCaret,
  copyTextFieldSelection,
  cutTextFieldSelection,
  IrisClipboardError,
  pasteIntoTextField,
} from "@/lib/iris-clipboard";

interface DocumentTitleContextMenuProps {
  inputRef: React.RefObject<HTMLInputElement | HTMLTextAreaElement | null>;
  value: string;
  onValueChange: (value: string) => void;
  children: React.ReactNode;
}

/** 文档标题输入框自定义右键（剪贴板，`iris_only`） */
export function DocumentTitleContextMenu({
  inputRef,
  value,
  onValueChange,
  children,
}: DocumentTitleContextMenuProps) {
  const [menu, setMenu] = useState({ open: false, x: 0, y: 0 });

  const groups = useMemo(
    () => [
      {
        group: "剪贴板",
        items: [
          { id: "cut", label: "剪切", icon: "Scissors" },
          { id: "copy", label: "复制", icon: "Copy" },
          { id: "paste", label: "粘贴", icon: "ClipboardPaste" },
          { id: "select-all", label: "全选", icon: "TextSelect" },
        ],
      },
    ],
    [],
  );

  const handleContextMenu = useCallback((event: React.MouseEvent) => {
    event.preventDefault();
    event.stopPropagation();
    setMenu({ open: true, x: event.clientX, y: event.clientY });
  }, []);

  const runAction = useCallback(
    async (id: string) => {
      const el = inputRef.current;
      if (!el) return;
      el.focus();
      const start = el.selectionStart ?? 0;
      const end = el.selectionEnd ?? start;
      const selection = { start, end };

      try {
        switch (id) {
          case "cut": {
            const cut = await cutTextFieldSelection(value, selection);
            if (!cut) return;
            onValueChange(cut.value);
            applyTextFieldCaret(el, cut.caret);
            break;
          }
          case "copy":
            await copyTextFieldSelection(value, selection);
            break;
          case "paste": {
            const pasted = await pasteIntoTextField(value, selection);
            if (!pasted) return;
            onValueChange(pasted.value);
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
          // 剪贴板不可用时保持静默
        }
      }
    },
    [inputRef, onValueChange, value],
  );

  return (
    <div onContextMenu={handleContextMenu}>
      {children}
      <IrisContextMenu
        open={menu.open}
        x={menu.x}
        y={menu.y}
        groups={groups.map((g) => ({
          ...g,
          items: g.items.map((item) => ({
            ...item,
            disabled:
              item.id === "cut" &&
              (inputRef.current?.selectionStart ?? 0) ===
                (inputRef.current?.selectionEnd ?? 0),
          })),
        }))}
        onSelect={(id) => void runAction(id)}
        onClose={() => setMenu({ open: false, x: 0, y: 0 })}
      />
    </div>
  );
}
