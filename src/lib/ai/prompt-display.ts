// Prompt display utilities — for debugging and transparency.
// Shows assembled prompts in the frontend for development/debug purposes.

import type { AiScene, ContextPacket } from "@/types/ai";

// ─── Prompt Section Types ────────────────────────────────

export interface PromptSection {
  label: string;
  content: string;
  type: "system" | "evidence" | "rules" | "query";
}

// ─── Prompt Display ──────────────────────────────────────

/**
 * Build a human-readable representation of the assembled prompt.
 * Used for debugging and transparency in the UI.
 */
export function buildPromptDisplay(
  scene: AiScene,
  packets: ContextPacket[],
  userRules: string[],
  query: string,
): PromptSection[] {
  const sections: PromptSection[] = [];

  // System prompt section
  const persona = getPersonaDescription(scene);
  sections.push({
    label: "系统提示",
    content: persona,
    type: "system",
  });

  // Evidence packets section
  if (packets.length > 0) {
    const evidenceLines = packets.map(
      (p) =>
        `[${p.citation_label}] ${p.title}\n` +
        `  来源: ${p.source_path ?? "未知"}\n` +
        `  相关度: ${Math.round(p.score * 100)}%\n` +
        `  ${p.excerpt}`,
    );
    sections.push({
      label: `证据包 (${packets.length} 条)`,
      content: evidenceLines.join("\n\n"),
      type: "evidence",
    });
  }

  // User rules section
  if (userRules.length > 0) {
    sections.push({
      label: `用户规则 (${userRules.length} 条)`,
      content: userRules.map((r) => `- ${r}`).join("\n"),
      type: "rules",
    });
  }

  // Query section
  sections.push({
    label: "用户查询",
    content: query,
    type: "query",
  });

  return sections;
}

/**
 * Get persona description for a scene.
 */
function getPersonaDescription(scene: AiScene): string {
  switch (scene) {
    case "knowledge_lookup":
      return "你是「知识管家」，帮助用户在本地知识库中查找、解释、引用材料。回答必须基于证据包，引用时使用 [citation_label] 格式。";
    case "exemplar_learning":
      return "你是「学习伴侣」，帮助用户分析范文结构、表达方式和写作技巧。可以建议可复用模板，但必须经用户确认才能保存。";
    case "drafting_assist":
      return "你是「写作伴侣」，帮助用户在文稿创作中提供低干扰写作辅助。写入操作必须经过用户确认。";
    case "research_synthesis":
      return "你是「研究助理」，帮助用户对多材料进行论证组织和证据缺口分析。联网研究必须经过用户授权。";
  }
}

/**
 * Estimate token count for a prompt section (rough: 1 token per 2 Chinese chars).
 */
export function estimateTokens(text: string): number {
  const chineseChars = (text.match(/[\u4e00-\u9fff]/g) ?? []).length;
  const otherChars = text.length - chineseChars;
  return Math.ceil(chineseChars / 2 + otherChars / 4);
}

/**
 * Build a compact summary of the prompt for display in the UI.
 */
export function buildPromptSummary(
  scene: AiScene,
  packetCount: number,
  ruleCount: number,
  estimatedTokens: number,
): string {
  const parts: string[] = [];
  parts.push(getPersonaDescription(scene).slice(0, 30) + "…");
  if (packetCount > 0) parts.push(`${packetCount} 条证据`);
  if (ruleCount > 0) parts.push(`${ruleCount} 条规则`);
  parts.push(`~${Math.round(estimatedTokens / 1000)}K tokens`);
  return parts.join(" | ");
}
