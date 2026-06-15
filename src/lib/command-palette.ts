import type {
  ManagementCenterDetail,
  ManagementCenterSection,
  OverlayId,
} from "@/hooks/useOverlayManager";
import { formatShortcut, type KeyChord } from "@/lib/utils";

export type CommandPaletteAction =
  | { type: "openOverlay"; overlay: OverlayId }
  | {
      type: "openManagementCenter";
      section: ManagementCenterSection;
      detail?: ManagementCenterDetail;
    }
  | { type: "openClassifiedPanel" }
  | { type: "openFindReplace"; mode: "find" | "replace" }
  | { type: "newNote" }
  | { type: "saveNote" }
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
  | { type: "sendSelectionToAi" }
  | { type: "noop" };

export interface CommandPaletteItem {
  id: string;
  label: string;
  group: string;
  keywords: string;
  /** Lucide 组件名，见 command-palette-icons.ts */
  icon?: string;
  disabled?: boolean;
  /** 不在命令面板列表中展示（如「打开命令面板」本身） */
  hiddenInPalette?: boolean;
  /** 全局快捷键绑定（无则仅在命令面板可用） */
  chord?: KeyChord;
  action: CommandPaletteAction;
}

export interface CommandPaletteContext {
  hasVault: boolean;
  hasActiveNote: boolean;
}

/** 由 chord 生成命令面板右侧快捷键展示 */
export function formatCommandPaletteItemShortcut(
  item: CommandPaletteItem,
): string | undefined {
  const chord = item.chord;
  if (!chord) return undefined;
  return formatShortcut(chord);
}

export function buildCommandPaletteItems(
  ctx: CommandPaletteContext,
): CommandPaletteItem[] {
  const noteOnly = !ctx.hasActiveNote;
  const vaultOnly = !ctx.hasVault;

  return [
    {
      id: "quick-open",
      label: "快速打开笔记",
      group: "知识库",
      keywords: "quick open file 文件 搜索 切换",
      icon: "Search",
      disabled: vaultOnly,
      chord: { key: "P", mod: true, requireVault: true },
      action: { type: "openOverlay", overlay: "quickOpen" },
    },
    {
      id: "file-sheet",
      label: "浏览笔记库",
      group: "知识库",
      keywords: "file tree vault 文件树 浏览 笔记库 管理",
      icon: "FolderTree",
      disabled: vaultOnly,
      chord: { key: "E", mod: true, shift: true, requireVault: true },
      action: {
        type: "openManagementCenter",
        section: "notes",
        detail: "file-sheet",
      },
    },
    {
      id: "recycle-bin",
      label: "回收站",
      group: "知识库",
      keywords: "recycle trash bin 回收站 删除 恢复 撤销",
      icon: "Trash2",
      disabled: vaultOnly,
      action: {
        type: "openManagementCenter",
        section: "notes",
        detail: "recycle-bin",
      },
    },
    {
      id: "classified-panel",
      label: "涉密面板",
      group: "保险库",
      keywords: "classified vault 涉密 保险库 加密 锁定",
      icon: "Lock",
      hiddenInPalette: true,
      disabled: vaultOnly,
      chord: { key: "L", mod: true, shift: true, requireVault: true },
      action: { type: "openClassifiedPanel" },
    },
    {
      id: "search",
      label: "全库搜索",
      group: "知识库",
      keywords: "search find 查找 全库",
      icon: "FileSearch",
      disabled: vaultOnly,
      chord: { key: "F", mod: true, shift: true, requireVault: true },
      action: { type: "openOverlay", overlay: "search" },
    },
    {
      id: "document-find",
      label: "本文档查找",
      group: "笔记",
      keywords: "find search document 当前 文档 查找",
      icon: "Search",
      disabled: noteOnly,
      chord: { key: "F", mod: true, requireNote: true },
      action: { type: "openFindReplace", mode: "find" },
    },
    {
      id: "document-replace",
      label: "本文档替换",
      group: "笔记",
      keywords: "replace find document 当前 文档 替换",
      icon: "Replace",
      disabled: noteOnly,
      chord: { key: "H", mod: true, requireNote: true },
      action: { type: "openFindReplace", mode: "replace" },
    },
    {
      id: "knowledge-relations",
      label: "知识关联",
      group: "知识库",
      keywords: "backlink 反链 链接 tag 标签 relation 关联",
      icon: "Link2",
      disabled: vaultOnly,
      action: { type: "openOverlay", overlay: "knowledgeRelations" },
    },
    {
      id: "graph",
      label: "知识图谱",
      group: "知识库",
      keywords: "graph 图谱 关系",
      icon: "Network",
      disabled: vaultOnly,
      action: { type: "openOverlay", overlay: "graph" },
    },
    {
      id: "rescan-vault",
      label: "重建库索引",
      group: "知识库",
      keywords: "index reindex 索引 同步",
      icon: "RotateCcw",
      disabled: vaultOnly,
      action: { type: "rescanVault" },
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
      id: "save-note",
      label: "保存笔记",
      group: "笔记",
      keywords: "save 保存 ctrl+s",
      icon: "Save",
      disabled: vaultOnly || noteOnly,
      chord: { key: "S", mod: true, requireNote: true },
      action: { type: "saveNote" },
    },
    {
      id: "close-tab",
      label: "关闭当前标签",
      group: "笔记",
      keywords: "close tab 关闭",
      icon: "X",
      disabled: noteOnly,
      chord: { key: "W", mod: true, requireNote: true },
      action: { type: "closeTab" },
    },
    {
      id: "version",
      label: "版本时间线",
      group: "笔记",
      keywords: "version history 历史 快照",
      icon: "GitBranch",
      disabled: vaultOnly || noteOnly,
      chord: { key: "V", mod: true, shift: true, requireNote: true },
      action: { type: "openOverlay", overlay: "version" },
    },
    {
      id: "toggle-outline",
      label: "显示 / 隐藏文档目录",
      group: "笔记",
      keywords: "outline 目录 大纲",
      icon: "BookOpen",
      disabled: noteOnly,
      action: { type: "toggleOutline" },
    },
    {
      id: "toggle-zen",
      label: "Zen 专注模式",
      group: "系统",
      keywords: "zen focus 专注 沉浸",
      icon: "Minimize2",
      chord: { key: ".", mod: true },
      action: { type: "toggleZen" },
    },
    {
      id: "toggle-theme",
      label: "切换浅色 / 深色主题",
      group: "系统",
      keywords: "theme dark light 主题 外观",
      icon: "Sun",
      action: { type: "toggleTheme" },
    },
    {
      id: "management-center",
      label: "管理中心",
      group: "系统",
      keywords: "settings preferences 偏好 设置 管理 中心",
      icon: "Settings",
      chord: { key: ",", mod: true },
      action: { type: "openManagementCenter", section: "overview" },
    },
    {
      id: "zoom-in",
      label: "放大编辑器",
      group: "系统",
      keywords: "zoom in 放大 字号",
      icon: "ZoomIn",
      disabled: noteOnly,
      chord: { key: "+", mod: true, requireNote: true },
      action: { type: "zoomIn" },
    },
    {
      id: "zoom-out",
      label: "缩小编辑器",
      group: "系统",
      keywords: "zoom out 缩小 字号",
      icon: "ZoomOut",
      disabled: noteOnly,
      chord: { key: "-", mod: true, requireNote: true },
      action: { type: "zoomOut" },
    },
    {
      id: "zoom-reset",
      label: "重置编辑器缩放",
      group: "系统",
      keywords: "zoom reset 缩放 100",
      icon: "Maximize2",
      disabled: noteOnly,
      chord: { key: "0", mod: true, requireNote: true },
      action: { type: "zoomReset" },
    },
    {
      id: "toggle-ai",
      label: "显示 / 隐藏 AI 侧栏",
      group: "AI",
      keywords: "ai assistant 助手 侧栏",
      icon: "PanelRight",
      chord: { key: "A", mod: true, shift: true },
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
      id: "ai-system-center",
      label: "AI 管理",
      group: "AI",
      keywords:
        "ai system center management model search skills memory rules 系统 中心 管理 模型 联网 人格 规则",
      icon: "SlidersHorizontal",
      action: { type: "openManagementCenter", section: "ai" },
    },
    {
      id: "skills",
      label: "管理 AI Skills",
      group: "AI",
      keywords: "skills skill 技能 安装 注入 prompt",
      icon: "Wrench",
      action: { type: "openManagementCenter", section: "ai" },
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
      item.keywords.toLowerCase().includes(q) ||
      (formatCommandPaletteItemShortcut(item)?.toLowerCase().includes(q) ??
        false),
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

const USAGE_KEY = "iris-command-usage";

function loadUsage(): Record<string, number> {
  try {
    return JSON.parse(localStorage.getItem(USAGE_KEY) || "{}");
  } catch {
    return {};
  }
}

export function recordCommandUsage(id: string): void {
  try {
    const usage = loadUsage();
    usage[id] = (usage[id] || 0) + 1;
    localStorage.setItem(USAGE_KEY, JSON.stringify(usage));
  } catch {
    /* ignore */
  }
}

/** 组内按使用频次排序，组间顺序保持稳定。 */
export function sortCommandPaletteItems(
  items: CommandPaletteItem[],
): CommandPaletteItem[] {
  const usage = loadUsage();
  const grouped = groupCommandPaletteItems(items);

  return grouped.flatMap(({ items: groupItems }) => {
    const withUsage = groupItems.map((item, index) => ({
      item,
      usage: usage[item.id] || 0,
      index,
    }));
    withUsage.sort((a, b) => {
      if (a.item.disabled !== b.item.disabled) {
        return a.item.disabled ? 1 : -1;
      }
      if (a.usage !== b.usage) return b.usage - a.usage;
      return a.index - b.index;
    });
    return withUsage.map((x) => x.item);
  });
}
