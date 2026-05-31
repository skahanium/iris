import type { NodeViewProps } from "@tiptap/react";
import { NodeViewWrapper } from "@tiptap/react";
import { Braces } from "lucide-react";

const SYNTAX_KIND_LABELS: Record<string, string> = {
  raw_html: "Raw HTML",
  html_comment: "HTML 注释",
  unknown: "不可编辑",
};

function truncate(text: string, maxLen = 120): string {
  if (text.length <= maxLen) return text;
  return `${text.slice(0, maxLen)}…`;
}

export function PreserveNodeView({ node }: NodeViewProps) {
  const originalRaw = (node.attrs.originalRaw as string) || "";
  const syntaxKind = (node.attrs.syntaxKind as string) || "unknown";
  const label = SYNTAX_KIND_LABELS[syntaxKind] ?? "不可编辑";
  const truncated = truncate(originalRaw);

  return (
    <NodeViewWrapper
      className="my-2 select-none rounded border border-dashed border-muted-foreground/30 bg-muted/30 px-3 py-2"
      contentEditable={false}
    >
      <div className="flex items-center gap-2 text-[11px] text-muted-foreground">
        <Braces className="h-3 w-3 shrink-0" />
        <span className="font-medium">{label}</span>
        <span className="text-muted-foreground/50">· 只读 · 原文保留</span>
      </div>
      <div
        className="mt-1 whitespace-pre-wrap break-all font-mono text-[11px] text-muted-foreground/70"
        title={originalRaw.length > truncated.length ? originalRaw : undefined}
      >
        {truncated}
      </div>
    </NodeViewWrapper>
  );
}
