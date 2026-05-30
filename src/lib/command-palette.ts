import type { OverlayId } from "@/hooks/useOverlayManager";

export type CommandPaletteAction =
  | { type: "openOverlay"; overlay: OverlayId }
  | { type: "newNote" }
  | { type: "saveVersion" }
  | { type: "closeTab" }
  | { type: "toggleAiPanel" }
  | { type: "toggleZen" }
  | { type: "toggleOutline" }
  | { type: "toggleTheme" }
  | { type: "toggleWebSearch" }
  | { type: "rescanVault" }
  | { type: "zoomIn" }
  | { type: "zoomOut" }
  | { type: "zoomReset" }
  | { type: "sendSelectionToAi" };

export interface CommandPaletteItem {
  id: string;
  label: string;
  group: string;
  keywords: string;
  /** Lucide 组件名，见 command-palette-icons.ts */
  icon?: string;
  shortcut?: string;
  disabled?: boolean;
  /** 不在命令面板列表中展示（如「打开命令面板」本身） */
  hiddenInPalette?: boolean;
  action: CommandPaletteAction;
}

export interface CommandPaletteContext {
  hasVault: boolean;
  hasActiveNote: boolean;
}

export function buildCommandPaletteItems(
  ctx: CommandPaletteContext,
): CommandPaletteItem[] {
  const noteOnly = !ctx.hasActiveNote;
  const vaultOnly = !ctx.hasVault;

  return [
    {
      id: "command-palette",
      label: "命令面板",
      group: "通用",
      keywords: "command palette 命令 面板",
      icon: "Command",
      shortcut: "⌘/Ctrl+Shift+P",
      hiddenInPalette: true,
      action: { type: "openOverlay", overlay: "commandPalette" },
    },
    {
      id: "quick-open",
      label: "快速打开笔记",
      group: "导航",
      keywords: "quick open file 文件 搜索 切换",
      icon: "Search",
      shortcut: "⌘/Ctrl+P",
      disabled: vaultOnly,
      action: { type: "openOverlay", overlay: "quickOpen" },
    },
    {
      id: "file-sheet",
      label: "浏览笔记库",
      group: "导航",
      keywords: "file tree vault 文件树 浏览 笔记库 管理",
      icon: "FolderTree",
      shortcut: "⌘/Ctrl+Shift+E",
      disabled: vaultOnly,
      action: { type: "openOverlay", overlay: "fileSheet" },
    },
    {
      id: "recycle-bin",
      label: "回收站",
      group: "导航",
      keywords: "recycle trash bin 回收站 删除 恢复 撤销",
      icon: "Trash2",
      shortcut: "⌘/Ctrl+Shift+U",
      disabled: vaultOnly,
      action: { type: "openOverlay", overlay: "recycleBin" },
    },
    {
      id: "search",
      label: "全文搜索",
      group: "导航",
      keywords: "search find 查找",
      icon: "FileSearch",
      shortcut: "⌘/Ctrl+Shift+F",
      disabled: vaultOnly,
      action: { type: "openOverlay", overlay: "search" },
    },
    {
      id: "backlinks",
      label: "反向链接",
      group: "导航",
      keywords: "backlink 反链 链接",
      icon: "GitBranch",
      shortcut: "⌘/Ctrl+Shift+B",
      disabled: vaultOnly || noteOnly,
      action: { type: "openOverlay", overlay: "backlinks" },
    },
    {
      id: "tags",
      label: "标签",
      group: "导航",
      keywords: "tag 标签",
      icon: "Tag",
      shortcut: "⌘/Ctrl+Shift+T",
      disabled: vaultOnly,
      action: { type: "openOverlay", overlay: "tags" },
    },
    {
      id: "graph",
      label: "知识图谱",
      group: "导航",
      keywords: "graph 图谱 关系",
      icon: "Network",
      shortcut: "⌘/Ctrl+Shift+G",
      disabled: vaultOnly,
      action: { type: "openOverlay", overlay: "graph" },
    },
    {
      id: "new-note",
      label: "新建笔记",
      group: "笔记",
      keywords: "new note 创建",
      icon: "Plus",
      disabled: vaultOnly,
      action: { type: "newNote" },
    },
    {
      id: "save-version",
      label: "保存并创建版本快照",
      group: "笔记",
      keywords: "save version 保存 定稿",
      icon: "Save",
      shortcut: "⌘/Ctrl+S",
      disabled: vaultOnly || noteOnly,
      action: { type: "saveVersion" },
    },
    {
      id: "close-tab",
      label: "关闭当前标签",
      group: "笔记",
      keywords: "close tab 关闭",
      icon: "X",
      shortcut: "⌘/Ctrl+W",
      disabled: noteOnly,
      action: { type: "closeTab" },
    },
    {
      id: "version",
      label: "版本时间线",
      group: "笔记",
      keywords: "version history 历史 快照",
      icon: "GitBranch",
      shortcut: "⌘/Ctrl+Shift+V",
      disabled: vaultOnly || noteOnly,
      action: { type: "openOverlay", overlay: "version" },
    },
    {
      id: "toggle-outline",
      label: "显示 / 隐藏文档目录",
      group: "视图",
      keywords: "outline 目录 大纲",
      icon: "BookOpen",
      shortcut: "⌘/Ctrl+Shift+O",
      disabled: noteOnly,
      action: { type: "toggleOutline" },
    },
    {
      id: "toggle-zen",
      label: "Zen 专注模式",
      group: "视图",
      keywords: "zen focus 专注 沉浸",
      icon: "Minimize2",
      shortcut: "⌘/Ctrl+.",
      action: { type: "toggleZen" },
    },
    {
      id: "toggle-theme",
      label: "切换浅色 / 深色主题",
      group: "视图",
      keywords: "theme dark light 主题 外观",
      icon: "Sun",
      action: { type: "toggleTheme" },
    },
    {
      id: "settings",
      label: "设置",
      group: "视图",
      keywords: "settings preferences 偏好",
      icon: "Settings",
      shortcut: "⌘/Ctrl+,",
      action: { type: "openOverlay", overlay: "settings" },
    },
    {
      id: "toggle-ai",
      label: "显示 / 隐藏 AI 侧栏",
      group: "AI",
      keywords: "ai assistant 助手 侧栏",
      icon: "PanelRight",
      shortcut: "⌘/Ctrl+Shift+A",
      action: { type: "toggleAiPanel" },
    },
    {
      id: "send-selection-ai",
      label: "将选中文本发送到 AI",
      group: "AI",
      keywords: "send selection quote 引用 选中",
      icon: "Sparkles",
      disabled: noteOnly,
      action: { type: "sendSelectionToAi" },
    },
    {
      id: "toggle-web-search",
      label: "切换联网搜索",
      group: "AI",
      keywords: "web search 联网 搜索",
      icon: "Globe",
      action: { type: "toggleWebSearch" },
    },
    {
      id: "skills",
      label: "管理 AI Skills",
      group: "AI",
      keywords: "skills skill 技能 安装 注入 prompt",
      icon: "Lightbulb",
      action: { type: "openOverlay", overlay: "skills" },
    },
    {
      id: "zoom-in",
      label: "放大编辑器",
      group: "编辑器",
      keywords: "zoom in 放大 字号",
      icon: "ZoomIn",
      shortcut: "⌘/Ctrl++",
      disabled: noteOnly,
      action: { type: "zoomIn" },
    },
    {
      id: "zoom-out",
      label: "缩小编辑器",
      group: "编辑器",
      keywords: "zoom out 缩小 字号",
      icon: "ZoomOut",
      shortcut: "⌘/Ctrl+-",
      disabled: noteOnly,
      action: { type: "zoomOut" },
    },
    {
      id: "zoom-reset",
      label: "重置编辑器缩放",
      group: "编辑器",
      keywords: "zoom reset 缩放 100",
      icon: "RotateCcw",
      shortcut: "⌘/Ctrl+0",
      disabled: noteOnly,
      action: { type: "zoomReset" },
    },
    {
      id: "rescan-vault",
      label: "重建库索引",
      group: "库",
      keywords: "index reindex 索引 同步",
      icon: "RotateCcw",
      shortcut: "⌘/Ctrl+Shift+I",
      disabled: vaultOnly,
      action: { type: "rescanVault" },
    },
  ];
}

/** 按标签、分组、关键词过滤；保持原有顺序（含不可用项）。 */
export function filterCommandPaletteItems(
  items: CommandPaletteItem[],
  query: string,
): CommandPaletteItem[] {
  const visible = items.filter((item) => !item.hiddenInPalette);
  const q = query.trim().toLowerCase();
  if (!q) return [...visible];

  return visible.filter(
    (item) =>
      item.label.toLowerCase().includes(q) ||
      item.group.toLowerCase().includes(q) ||
      item.keywords.toLowerCase().includes(q),
  );
}

export function groupCommandPaletteItems(
  items: CommandPaletteItem[],
): { group: string; items: CommandPaletteItem[] }[] {
  const order: string[] = [];
  const map = new Map<string, CommandPaletteItem[]>();

  for (const item of items) {
    if (!map.has(item.group)) {
      map.set(item.group, []);
      order.push(item.group);
    }
    map.get(item.group)?.push(item);
  }

  return order.map((group) => ({
    group,
    items: map.get(group) ?? [],
  }));
}
