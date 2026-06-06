/** 编辑区 / AI 区共享动作注册表（`/`、右键） */

export type ActionScope = "editor" | "ai_message" | "ai_composer";

export type ActionSurface = "slash" | "context_menu";

export type ActionKind =
  | "tiptap"
  | "inline_ai"
  | "slash_flow"
  | "clipboard"
  | "assistant"
  | "send_prefill";

export interface EditorActionDef {
  id: string;
  label: string;
  /** `/` 菜单短标签 */
  shortLabel?: string;
  icon: string;
  keywords?: string;
  scopes: ActionScope[];
  surfaces: ActionSurface[];
  requiresSelection?: boolean;
  requiresNote?: boolean;
  kind: ActionKind;
  /** slash_flow 时对应的 slash 命令 id */
  slashCommandId?: string;
  /** inline_ai 时传给 useInlineAi.run 的 action */
  inlineActionId?: string;
  /** send_prefill 预填文案 */
  prefill?: string;
  /** 右键菜单分组 */
  menuGroup?: "clipboard" | "ai_selection" | "ai_document" | "ai_message";
}

export const EDITOR_ACTIONS: EditorActionDef[] = [
  {
    id: "cut",
    label: "剪切",
    icon: "Scissors",
    scopes: ["editor"],
    surfaces: ["context_menu"],
    kind: "tiptap",
    requiresSelection: true,
    menuGroup: "clipboard",
  },
  {
    id: "copy",
    label: "复制",
    icon: "Copy",
    scopes: ["editor", "ai_message", "ai_composer"],
    surfaces: ["context_menu"],
    kind: "tiptap",
    menuGroup: "clipboard",
  },
  {
    id: "paste",
    label: "粘贴",
    icon: "ClipboardPaste",
    scopes: ["editor", "ai_composer"],
    surfaces: ["context_menu"],
    kind: "clipboard",
    menuGroup: "clipboard",
  },
  {
    id: "select-all",
    label: "全选",
    icon: "TextSelect",
    scopes: ["editor", "ai_composer"],
    surfaces: ["context_menu"],
    kind: "tiptap",
    menuGroup: "clipboard",
  },
  {
    id: "rewrite",
    label: "改写",
    icon: "Sparkles",
    scopes: ["editor"],
    surfaces: ["context_menu"],
    requiresSelection: true,
    kind: "inline_ai",
    inlineActionId: "rewrite",
    menuGroup: "ai_selection",
  },
  {
    id: "expand",
    label: "扩写",
    icon: "Sparkles",
    scopes: ["editor"],
    surfaces: ["context_menu"],
    requiresSelection: true,
    kind: "inline_ai",
    inlineActionId: "expand",
    menuGroup: "ai_selection",
  },
  {
    id: "simplify",
    label: "简化",
    icon: "Sparkles",
    scopes: ["editor"],
    surfaces: ["context_menu"],
    requiresSelection: true,
    kind: "inline_ai",
    inlineActionId: "simplify",
    menuGroup: "ai_selection",
  },
  {
    id: "translate",
    label: "翻译",
    shortLabel: "翻译",
    icon: "ArrowLeftRight",
    keywords: "translate 翻译",
    scopes: ["editor"],
    surfaces: ["slash", "context_menu"],
    kind: "inline_ai",
    inlineActionId: "translate",
    slashCommandId: "translate",
    menuGroup: "ai_selection",
  },
  {
    id: "fix-grammar",
    label: "修复语法",
    shortLabel: "修复语法",
    icon: "Languages",
    keywords: "fix grammar 语法 纠错",
    scopes: ["editor"],
    surfaces: ["slash", "context_menu"],
    kind: "inline_ai",
    inlineActionId: "fix-grammar",
    slashCommandId: "fix-grammar",
    menuGroup: "ai_selection",
  },
  {
    id: "summarize",
    label: "总结",
    shortLabel: "总结",
    icon: "FileText",
    keywords: "summarize 总结 摘要",
    scopes: ["editor"],
    surfaces: ["slash", "context_menu"],
    requiresNote: true,
    kind: "slash_flow",
    slashCommandId: "summarize",
    menuGroup: "ai_document",
  },
  {
    id: "outline",
    label: "生成大纲",
    shortLabel: "生成大纲",
    icon: "ListTree",
    keywords: "outline 大纲",
    scopes: ["editor"],
    surfaces: ["slash", "context_menu"],
    requiresNote: true,
    kind: "slash_flow",
    slashCommandId: "outline",
    menuGroup: "ai_document",
  },
  {
    id: "brainstorm",
    label: "头脑风暴",
    shortLabel: "头脑风暴",
    icon: "Lightbulb",
    keywords: "brainstorm 创意",
    scopes: ["editor"],
    surfaces: ["slash", "context_menu"],
    requiresNote: true,
    kind: "slash_flow",
    slashCommandId: "brainstorm",
    menuGroup: "ai_document",
  },
  {
    id: "send-to-ai",
    label: "发送到 AI",
    icon: "Sparkles",
    scopes: ["editor"],
    surfaces: ["context_menu"],
    requiresSelection: true,
    kind: "assistant",
    menuGroup: "ai_selection",
  },
  {
    id: "cite",
    label: "引用",
    icon: "Sparkles",
    scopes: ["editor"],
    surfaces: ["context_menu"],
    requiresSelection: true,
    kind: "send_prefill",
    prefill: "请为选区补充引用依据",
    menuGroup: "ai_selection",
  },
  {
    id: "check",
    label: "检查",
    icon: "Sparkles",
    scopes: ["editor"],
    surfaces: ["context_menu"],
    requiresSelection: true,
    kind: "send_prefill",
    prefill: "检查这一段的引用是否充分",
    menuGroup: "ai_selection",
  },
  {
    id: "quote-to-input",
    label: "引用到输入",
    icon: "Quote",
    scopes: ["ai_message"],
    surfaces: ["context_menu"],
    requiresSelection: true,
    kind: "assistant",
    menuGroup: "ai_message",
  },
];

export function editorActionById(id: string): EditorActionDef | undefined {
  return EDITOR_ACTIONS.find((a) => a.id === id);
}

export interface EditorActionContext {
  hasNote: boolean;
  hasSelection: boolean;
  streaming: boolean;
  isLocked?: boolean;
}

export function isEditorActionEnabled(
  action: EditorActionDef,
  ctx: EditorActionContext,
): boolean {
  if (ctx.isLocked) {
    if (action.id === "copy" || action.id === "select-all") {
      return ctx.hasNote;
    }
    return false;
  }
  if (
    ctx.streaming &&
    (action.kind === "inline_ai" || action.kind === "slash_flow")
  ) {
    return false;
  }
  if (action.requiresNote && !ctx.hasNote) return false;
  if (action.requiresSelection && !ctx.hasSelection) return false;
  return true;
}

export function filterEditorActions(
  surface: ActionSurface,
  scope: ActionScope,
  ctx: EditorActionContext,
): EditorActionDef[] {
  return EDITOR_ACTIONS.filter((action) => {
    if (!action.scopes.includes(scope)) return false;
    if (!action.surfaces.includes(surface)) return false;

    if (surface === "slash" && ctx.hasSelection) {
      if (
        action.requiresSelection ||
        action.menuGroup === "ai_selection" ||
        action.kind === "inline_ai"
      ) {
        return false;
      }
    }

    if (surface === "context_menu" && action.menuGroup === "ai_document") {
      if (ctx.hasSelection) return false;
      return isEditorActionEnabled(action, ctx);
    }

    if (
      surface === "context_menu" &&
      !ctx.hasSelection &&
      (action.id === "translate" || action.id === "fix-grammar")
    ) {
      return isEditorActionEnabled(action, ctx);
    }

    if (
      surface === "context_menu" &&
      action.menuGroup === "ai_selection" &&
      action.slashCommandId &&
      !ctx.hasSelection
    ) {
      return false;
    }

    if (
      surface === "context_menu" &&
      action.menuGroup === "ai_selection" &&
      !action.slashCommandId &&
      !ctx.hasSelection
    ) {
      return false;
    }

    if (
      surface === "slash" &&
      action.id === "send-to-ai" &&
      !ctx.hasSelection
    ) {
      return false;
    }

    return isEditorActionEnabled(action, ctx);
  });
}

export function groupContextMenuActions(
  actions: EditorActionDef[],
): { group: string; items: EditorActionDef[] }[] {
  const order = [
    "clipboard",
    "ai_selection",
    "ai_document",
    "ai_message",
  ] as const;
  const labels: Record<(typeof order)[number], string> = {
    clipboard: "剪贴板",
    ai_selection: "AI · 选区",
    ai_document: "AI · 文档",
    ai_message: "AI",
  };
  const buckets = new Map<string, EditorActionDef[]>();
  for (const a of actions) {
    const g = a.menuGroup ?? "ai_message";
    const list = buckets.get(g) ?? [];
    list.push(a);
    buckets.set(g, list);
  }
  return order
    .filter((g) => buckets.has(g))
    .map((g) => ({ group: labels[g], items: buckets.get(g)! }));
}

/** `/` 菜单项（无选区时文档级；有选区时仅文档级命令） */
export function slashMenuActions(ctx: EditorActionContext): EditorActionDef[] {
  return filterEditorActions("slash", "editor", ctx).sort((a, b) => {
    const sel = (d: EditorActionDef) =>
      d.requiresSelection || d.menuGroup === "ai_selection" ? 0 : 1;
    return sel(a) - sel(b);
  });
}

/** 兼容旧 `SLASH_COMMANDS` 的文档级命令 */
export const SLASH_DOCUMENT_COMMAND_IDS = [
  "summarize",
  "outline",
  "brainstorm",
  "fix-grammar",
  "translate",
] as const;
