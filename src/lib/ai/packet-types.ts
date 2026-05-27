// Re-exports with display helpers for ContextPacket
export type { ContextPacket, TrustLevel, SourceType } from "@/types/ai";

export const TRUST_LABELS: Record<string, string> = {
  user_note: "用户笔记",
  derived_cache: "派生缓存",
  external_web: "外部网页",
  model_generated: "模型生成",
};

export const SOURCE_LABELS: Record<string, string> = {
  note: "笔记",
  anchor: "语义锚点",
  regulation: "法规",
  template: "模板",
  session: "会话",
  web: "网页",
};
