/**
 * Iris editor Markdown capability declaration — contract-aligned.
 *
 * 不再使用静态字符串数组，而是直接从 Markdown contract 的类型系统
 * 派生编辑器的能力声明。
 *
 * 子项目 2 升级。
 */
import type { MarkdownSyntaxKind } from "@/lib/markdown-contract/types";
import {
  NATIVE_SYNTAX_KINDS,
  RENDER_ONLY_SYNTAX_KINDS,
  PRESERVE_ONLY_SYNTAX_KINDS,
} from "@/lib/markdown-contract/types";

/** 编辑器可全编辑的语法 */
export const EDITOR_NATIVE_SYNTAX = NATIVE_SYNTAX_KINDS;
/** 编辑器可渲染但暂不可安全编辑的语法 */
export const EDITOR_RENDER_ONLY_SYNTAX = RENDER_ONLY_SYNTAX_KINDS;
/** 编辑器必须原样保留的语法 */
export const EDITOR_PRESERVE_SYNTAX = PRESERVE_ONLY_SYNTAX_KINDS;

/** 语法是否可以在编辑器内完整编辑 */
export function isEditorEditable(kind: MarkdownSyntaxKind): boolean {
  return EDITOR_NATIVE_SYNTAX.has(kind);
}

/** 语法是否仅可渲染但不可编辑（如 callout、脚注） */
export function isEditorReadonly(kind: MarkdownSyntaxKind): boolean {
  return EDITOR_RENDER_ONLY_SYNTAX.has(kind);
}

/** 语法是否必须作为 preserve 块原样保留（如 raw HTML） */
export function isEditorPreserved(kind: MarkdownSyntaxKind): boolean {
  return EDITOR_PRESERVE_SYNTAX.has(kind);
}

/**
 * 获取语法在编辑器内的策略标签。
 * 用于向用户展示当前内容的能力状态。
 */
export function editorCapabilityLabel(kind: MarkdownSyntaxKind): string {
  if (isEditorEditable(kind)) return "可编辑";
  if (isEditorReadonly(kind)) return "只读渲染";
  if (isEditorPreserved(kind)) return "原文保留";
  return "不支持";
}
