import type { AssistantIntent, AssistantTaskStatus } from "@/types/ai";

export interface AssistantRouteInput {
  message: string;
  hasSelection: boolean;
  notePath: string | null;
  explicitScope: boolean;
}

const WRITING_KEYWORDS = [
  "改写",
  "重写",
  "润色",
  "扩写",
  "续写",
  "简化",
  "压缩",
  "总结这段",
  "重组",
];

const CITATION_KEYWORDS = [
  "引用",
  "引证",
  "依据",
  "证据",
  "出处",
  "核查",
  "检查",
];

const ORGANIZE_KEYWORDS = [
  "整理",
  "归档",
  "标签",
  "分类",
  "标题",
  "资料库",
  "知识库",
];

const RESEARCH_KEYWORDS = [
  "研究",
  "调研",
  "对比",
  "取舍",
  "综述",
  "深挖",
  "分析",
];

const KNOWLEDGE_KEYWORDS = [
  "查一下",
  "查阅",
  "搜索",
  "搜一下",
  "库里",
  "文档里",
  "找一下",
  "什么是",
];

const CHAPTER_KEYWORDS = ["章节", "这一章", "本章", "章内", "heading"];

const DOCUMENT_KEYWORDS = [
  "大纲检查",
  "全文检查",
  "文档检查",
  "风格一致",
  "跨文档",
  "引用缺口",
  "outline",
];

function includesAny(haystack: string, needles: string[]): boolean {
  return needles.some((needle) => haystack.includes(needle));
}

export function resolveAssistantIntent(
  input: AssistantRouteInput,
): AssistantIntent {
  const message = input.message.trim().toLowerCase();

  if (!message) {
    return input.hasSelection ? "writing" : "chat";
  }

  if (
    includesAny(message, RESEARCH_KEYWORDS) &&
    (input.explicitScope || !input.notePath || message.length > 12)
  ) {
    return "research";
  }

  if (input.notePath && includesAny(message, DOCUMENT_KEYWORDS)) {
    return "document";
  }

  if (input.notePath && includesAny(message, CHAPTER_KEYWORDS)) {
    return "chapter";
  }

  if (includesAny(message, ORGANIZE_KEYWORDS)) {
    return "organize";
  }

  if (input.hasSelection && includesAny(message, CITATION_KEYWORDS)) {
    return "citation";
  }

  if (input.hasSelection && includesAny(message, WRITING_KEYWORDS)) {
    return "writing";
  }

  if (includesAny(message, KNOWLEDGE_KEYWORDS) || input.explicitScope) {
    return "knowledge";
  }

  if (input.hasSelection && input.notePath) {
    return "writing";
  }

  return "chat";
}

export function assistantIntentLabel(intent: AssistantIntent): string {
  switch (intent) {
    case "knowledge":
      return "知识查阅";
    case "writing":
      return "改写选区";
    case "citation":
      return "检查引用";
    case "organize":
      return "整理建议";
    case "research":
      return "研究中";
    case "chapter":
      return "章节写作";
    case "document":
      return "文档检查";
    case "chat":
      return "对话";
  }
}

export function assistantStatusText(status: AssistantTaskStatus): string {
  switch (status) {
    case "idle":
      return "待命";
    case "running":
      return "处理中";
    case "awaiting_confirmation":
      return "等待确认";
    case "completed":
      return "已完成";
    case "error":
      return "出错";
  }
}
