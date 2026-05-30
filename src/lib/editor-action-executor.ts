import type { Editor } from "@tiptap/react";

import { editorActionById, type EditorActionDef } from "@/lib/editor-actions";
import {
  copyEditorSelection,
  cutEditorSelection,
  IrisClipboardError,
  pasteIntoEditor,
} from "@/lib/iris-clipboard";

export interface RunEditorActionHandlers {
  onInlineAi: (action: string) => void;
  onSlashCommand: (command: string) => void;
  onSendToAi: (options?: { prefill?: string }) => void;
  onStatus?: (message: string) => void;
}

/** 执行编辑区注册表动作 */
export async function runEditorAction(
  actionId: string,
  editor: Editor | null,
  handlers: RunEditorActionHandlers,
): Promise<void> {
  const action = editorActionById(actionId);
  if (!action || !editor) return;

  const { from, to } = editor.state.selection;
  const hasSelection = from !== to;

  switch (action.kind) {
    case "clipboard": {
      if (action.id === "paste") {
        try {
          await pasteIntoEditor(editor);
        } catch (err) {
          if (err instanceof IrisClipboardError) {
            handlers.onStatus?.("无法读取剪贴板");
          }
        }
      }
      return;
    }
    case "tiptap": {
      const chain = editor.chain().focus();
      switch (action.id) {
        case "cut":
          if (!hasSelection) return;
          try {
            await cutEditorSelection(editor);
          } catch (err) {
            if (err instanceof IrisClipboardError) {
              handlers.onStatus?.("无法访问剪贴板");
            }
          }
          break;
        case "copy":
          try {
            const copied = await copyEditorSelection(editor);
            if (!copied && !hasSelection) return;
          } catch (err) {
            if (err instanceof IrisClipboardError) {
              handlers.onStatus?.("无法访问剪贴板");
            }
          }
          break;
        case "select-all":
          chain.selectAll().run();
          break;
        default:
          break;
      }
      return;
    }
    case "inline_ai": {
      if (!hasSelection && action.slashCommandId) {
        handlers.onSlashCommand(action.slashCommandId);
        return;
      }
      const inlineId = action.inlineActionId ?? action.id;
      if (!hasSelection) {
        handlers.onStatus?.("请先选中要处理的文字");
        return;
      }
      handlers.onInlineAi(inlineId);
      return;
    }
    case "slash_flow": {
      const cmd = action.slashCommandId ?? action.id;
      handlers.onSlashCommand(cmd);
      return;
    }
    case "assistant": {
      if (action.id === "send-to-ai") {
        handlers.onSendToAi();
        return;
      }
      break;
    }
    case "send_prefill": {
      if (!hasSelection) {
        handlers.onStatus?.("请先选中文字");
        return;
      }
      handlers.onSendToAi({ prefill: action.prefill });
      return;
    }
    default:
      break;
  }
}

export function actionToSlashItem(action: EditorActionDef): {
  id: string;
  label: string;
  icon: string;
} {
  return {
    id: action.slashCommandId ?? action.id,
    label: action.shortLabel ?? action.label,
    icon: action.icon,
  };
}
