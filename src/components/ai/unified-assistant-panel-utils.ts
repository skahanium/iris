import {
  BookOpen,
  FileSearch,
  FolderTree,
  Layers,
  ListChecks,
  MessageSquareText,
  PenSquare,
  Quote,
  type LucideIcon,
} from "lucide-react";

import type {
  AssistantActionState,
  AssistantIntent,
  AssistantSurfaceState,
  DocumentCheckType,
} from "@/types/ai";
import { assistantIntentLabel } from "@/lib/assistant-routing";

export function assistantIcon(intent: AssistantIntent): LucideIcon {
  switch (intent) {
    case "knowledge":
      return FileSearch;
    case "writing":
      return PenSquare;
    case "citation":
      return Quote;
    case "organize":
      return FolderTree;
    case "research":
      return BookOpen;
    case "chapter":
      return Layers;
    case "document":
      return ListChecks;
    case "chat":
      return MessageSquareText;
  }
}

export function buildActionState(
  intent: AssistantIntent,
  status: AssistantActionState["status"],
  detail: string | null = null,
): AssistantActionState {
  const surface: AssistantSurfaceState =
    intent === "research"
      ? "research_focus"
      : intent === "writing" ||
          intent === "citation" ||
          intent === "organize" ||
          intent === "chapter" ||
          intent === "document"
        ? "diff_review"
        : "conversation";

  return {
    intent,
    status,
    label: assistantIntentLabel(intent),
    surface,
    contextSource:
      intent === "writing" || intent === "citation"
        ? "selection"
        : intent === "knowledge" || intent === "research"
          ? "scope"
          : "document",
    detail,
  };
}

export function determineOrganizeTaskType(message: string): string {
  if (message.includes("标签")) return "tag_suggestions";
  if (message.includes("标题")) return "title_suggestions";
  return "full_audit";
}

export function determineDocumentCheckType(message: string): DocumentCheckType {
  if (message.includes("引用")) return "citation_gap_check";
  if (message.includes("风格")) return "style_consistency";
  if (message.includes("跨文档")) return "cross_doc_reference";
  return "outline_check";
}

export function buildTaskSummary(intent: AssistantIntent, count?: number): string {
  switch (intent) {
    case "writing":
      return count && count > 0
        ? `已生成 ${count} 条补丁建议，等待你确认。`
        : "没有生成新的补丁建议。";
    case "citation":
      return "已完成当前段落的引用检查。";
    case "organize":
      return count && count > 0
        ? `已整理出 ${count} 条库内建议。`
        : "暂时没有新的整理建议。";
    case "research":
      return "研究结果已准备好。";
    case "chapter":
      return count && count > 0
        ? `已生成 ${count} 条章节补丁，等待你确认。`
        : "章节任务已完成，暂无新补丁。";
    case "document":
      return count && count > 0
        ? `文档检查完成，有 ${count} 条补丁待确认。`
        : "文档检查完成。";
    case "knowledge":
      return "已完成知识查阅。";
    case "chat":
      return "已完成本轮对话。";
  }
}
