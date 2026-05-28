import { resolveNoteDisplayTitle } from "@/lib/note-display";
import type { SemanticHit } from "@/types/ipc";

export interface ContextQuote {
  filePath: string;
  text: string;
  heading?: string;
}

/** 注入 system 的关联笔记条数（PR-C4：Top 3–5，取 5） */
export const RELATED_NOTES_TOP_K = 5;

/** 语义检索多取几条，过滤当前笔记后仍够 Top-K */
export const RELATED_NOTES_FETCH_LIMIT = 12;

export interface BuildAiSystemContextInput {
  notePath: string | null;
  /** User-facing title; never `untitled-*`. */
  noteDisplayTitle: string | null;
  noteContent: string;
  quote: ContextQuote | null;
  relatedHits: SemanticHit[];
}

/**
 * 排除当前打开笔记，按 path 去重，保留分数最高的一条，取 Top-K。
 */
export function filterRelatedSemanticHits(
  hits: SemanticHit[],
  excludePath: string | null,
  limit = RELATED_NOTES_TOP_K,
): SemanticHit[] {
  const byPath = new Map<string, SemanticHit>();
  for (const hit of hits) {
    if (excludePath && hit.path === excludePath) continue;
    const prev = byPath.get(hit.path);
    if (!prev || hit.score > prev.score) {
      byPath.set(hit.path, hit);
    }
  }
  return [...byPath.values()].sort((a, b) => b.score - a.score).slice(0, limit);
}

/**
 * 将关联笔记片段格式化为 system 段落；无命中返回 null（降级为仅当前笔记）。
 */
export function formatRelatedNotesSection(hits: SemanticHit[]): string | null {
  if (hits.length === 0) return null;
  const blocks = hits.map((h, i) => {
    const label = resolveNoteDisplayTitle({
      path: h.path,
      title: h.title,
    });
    return `[关联 ${i + 1}] ${label}（相关度 ${h.score.toFixed(3)}）\n${h.snippet}`;
  });
  return `以下是与用户问题相关的其他笔记片段（请勿编造未出现的笔记内容）：\n\n${blocks.join("\n\n")}`;
}

/** 组装 AI 侧栏 system 提示各段。 */
export function buildAiSystemParts(input: BuildAiSystemContextInput): string[] {
  const parts: string[] = ["你是 Iris 笔记助手，基于用户笔记内容回答问题。"];

  if (input.notePath && input.noteContent) {
    const label =
      input.noteDisplayTitle?.trim() ||
      resolveNoteDisplayTitle({ path: input.notePath });
    parts.push(
      `当前笔记（${label}）:\n${input.noteContent.slice(0, 8000)}`,
    );
  }

  const relatedSection = formatRelatedNotesSection(input.relatedHits);
  if (relatedSection) {
    parts.push(relatedSection);
  }

  if (input.quote) {
    const quoteLabel = resolveNoteDisplayTitle({ path: input.quote.filePath });
    parts.push(
      `引用自 ${quoteLabel}${input.quote.heading ? ` / ${input.quote.heading}` : ""}:\n${input.quote.text}`,
    );
  }

  return parts;
}

export function buildAiSystemPrompt(input: BuildAiSystemContextInput): string {
  return buildAiSystemParts(input).join("\n\n");
}
