/** @deprecated 使用 `editor-actions`；保留导出供旧引用 */
import {
  EDITOR_ACTIONS,
  SLASH_DOCUMENT_COMMAND_IDS,
  slashMenuActions,
  type EditorActionContext,
} from "@/lib/editor-actions";

export interface SlashCommandDef {
  id: string;
  menuLabel: string;
  paletteLabel: string;
  icon: string;
  keywords: string;
}

/** 文档级 `/` 命令（命令面板不再注册） */
export const SLASH_COMMANDS: SlashCommandDef[] = SLASH_DOCUMENT_COMMAND_IDS.map(
  (id) => {
    const action = EDITOR_ACTIONS.find(
      (a) => a.slashCommandId === id || a.id === id,
    )!;
    return {
      id,
      menuLabel: action.shortLabel ?? action.label,
      paletteLabel: `AI ${action.label}`,
      icon: action.icon,
      keywords: action.keywords ?? id,
    };
  },
);

export function slashCommandById(id: string): SlashCommandDef | undefined {
  return SLASH_COMMANDS.find((c) => c.id === id);
}

export function buildSlashItemsFromContext(ctx: EditorActionContext) {
  return slashMenuActions(ctx).map((a) => ({
    /** 注册表动作 id，供 `runEditorAction` 使用 */
    id: a.id,
    label: a.shortLabel ?? a.label,
    icon: a.icon,
    keywords: a.keywords,
  }));
}
