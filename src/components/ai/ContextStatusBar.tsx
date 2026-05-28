import { FileText, Link2, Scale, Anchor, BookOpen } from "lucide-react";

import type { AiScene, ContextStatus } from "@/types/ai";
import { SCENE_META } from "@/lib/ai/scene-types";

// ─── Types ───────────────────────────────────────────────

interface ContextStatusBarProps {
  scene: AiScene;
  contextStatus: ContextStatus | null;
  noteDisplayTitle: string | null;
  totalPackets?: number;
  corpusNames?: string[];
}

// ─── Component ───────────────────────────────────────────

export function ContextStatusBar({
  scene,
  contextStatus,
  noteDisplayTitle,
  totalPackets,
  corpusNames = [],
}: ContextStatusBarProps) {
  const meta = SCENE_META[scene];
  const isGlobal = meta.defaultScope === "global";

  return (
    <div className="flex items-center gap-3 border-b border-border/60 bg-surface-inset/40 px-3 py-1.5 text-xs text-muted-foreground">
      <span
        className="inline-block h-1.5 w-1.5 rounded-full bg-primary/80"
        title="就绪"
      />
      <span className="font-medium">{meta.label}</span>

      {/* Scope */}
      {isGlobal ? (
        <span className="rounded bg-secondary px-1.5 py-0 text-[10px]">
          库级
        </span>
      ) : noteDisplayTitle ? (
        <span className="max-w-[120px] truncate rounded bg-secondary px-1.5 py-0 text-[10px]">
          {noteDisplayTitle}
        </span>
      ) : null}

      {corpusNames.length > 0 && (
        <>
          {corpusNames.map((name) => (
            <span
              key={name}
              className="max-w-[100px] truncate rounded bg-secondary px-1.5 py-0 text-[10px]"
              title={`语料库：${name}`}
            >
              {name}
            </span>
          ))}
        </>
      )}

      {/* Separator */}
      <span className="h-3 w-px bg-border" />

      {/* Context stats */}
      {contextStatus ? (
        <div className="flex items-center gap-2">
          {totalPackets !== undefined && totalPackets > 0 && (
            <span className="inline-flex items-center gap-0.5">
              <FileText className="h-3 w-3" />
              {totalPackets} 证据
            </span>
          )}
          {contextStatus.regulations_loaded > 0 && (
            <span className="inline-flex items-center gap-0.5">
              <Scale className="h-3 w-3" />
              {contextStatus.regulations_loaded} 法规
            </span>
          )}
          {contextStatus.anchors_loaded > 0 && (
            <span className="inline-flex items-center gap-0.5">
              <Anchor className="h-3 w-3" />
              {contextStatus.anchors_loaded} 锚点
            </span>
          )}
          {contextStatus.links_loaded > 0 && (
            <span className="inline-flex items-center gap-0.5">
              <Link2 className="h-3 w-3" />
              {contextStatus.links_loaded} 链接
            </span>
          )}
          {contextStatus.model_essays_loaded > 0 && (
            <span className="inline-flex items-center gap-0.5">
              <BookOpen className="h-3 w-3" />
              {contextStatus.model_essays_loaded} 范文
            </span>
          )}
          {contextStatus.total_tokens_estimate > 0 && (
            <span className="text-[10px] text-muted-foreground/60">
              ~{Math.round(contextStatus.total_tokens_estimate / 1000)}K tokens
            </span>
          )}
        </div>
      ) : (
        <span className="text-muted-foreground/50">等待上下文…</span>
      )}
    </div>
  );
}
