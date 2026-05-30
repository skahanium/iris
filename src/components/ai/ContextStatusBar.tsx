import { FileText, Globe, Link2, Scale, Anchor, BookOpen } from "lucide-react";

import { cn } from "@/lib/utils";
import type { ContextStatus } from "@/types/ai";

// ─── Types ───────────────────────────────────────────────

interface ContextStatusBarProps {
  contextStatus: ContextStatus | null;
  totalPackets?: number;
  webPacketCount?: number;
  corpusNames?: string[];
  /** 底栏联网开关：仅底边一条蓝线表示，无其它标签 */
  webSearchEnabled?: boolean;
}

// ─── Component ───────────────────────────────────────────

export function ContextStatusBar({
  contextStatus,
  totalPackets,
  webPacketCount = 0,
  corpusNames = [],
  webSearchEnabled = false,
}: ContextStatusBarProps) {
  const hasStats =
    (totalPackets !== undefined && totalPackets > 0) ||
    (contextStatus &&
      (contextStatus.regulations_loaded > 0 ||
        contextStatus.anchors_loaded > 0 ||
        contextStatus.links_loaded > 0 ||
        contextStatus.model_essays_loaded > 0));

  return (
    <div className="relative flex items-center gap-3 border-b border-border/60 bg-surface-inset/40 px-3 py-1.5 text-xs text-muted-foreground">
      <span
        className={cn(
          "iris-web-accent-line",
          webSearchEnabled && "iris-web-accent-line--on",
        )}
        aria-hidden
      />

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
          <span className="h-3 w-px shrink-0 bg-border" aria-hidden />
        </>
      )}

      {/* Evidence stats */}
      {hasStats ? (
        <div className="flex items-center gap-2">
          {totalPackets !== undefined && totalPackets > 0 && (
            <span className="inline-flex items-center gap-0.5">
              <FileText className="h-3 w-3" />
              {totalPackets} 证据
            </span>
          )}
          {webPacketCount > 0 && (
            <span className="inline-flex items-center gap-0.5">
              <Globe className="h-3 w-3" />
              {webPacketCount} 网络
            </span>
          )}
          {contextStatus?.regulations_loaded ? (
            contextStatus.regulations_loaded > 0 && (
              <span className="inline-flex items-center gap-0.5">
                <Scale className="h-3 w-3" />
                {contextStatus.regulations_loaded} 法规
              </span>
            )
          ) : null}
          {contextStatus?.anchors_loaded ? (
            contextStatus.anchors_loaded > 0 && (
              <span className="inline-flex items-center gap-0.5">
                <Anchor className="h-3 w-3" />
                {contextStatus.anchors_loaded} 锚点
              </span>
            )
          ) : null}
          {contextStatus?.links_loaded ? (
            contextStatus.links_loaded > 0 && (
              <span className="inline-flex items-center gap-0.5">
                <Link2 className="h-3 w-3" />
                {contextStatus.links_loaded} 链接
              </span>
            )
          ) : null}
          {contextStatus?.model_essays_loaded ? (
            contextStatus.model_essays_loaded > 0 && (
              <span className="inline-flex items-center gap-0.5">
                <BookOpen className="h-3 w-3" />
                {contextStatus.model_essays_loaded} 范文
              </span>
            )
          ) : null}
          {contextStatus?.total_tokens_estimate ? (
            contextStatus.total_tokens_estimate > 0 && (
              <span className="text-[10px] text-muted-foreground/60">
                ~{Math.round(contextStatus.total_tokens_estimate / 1000)}K
                tokens
              </span>
            )
          ) : null}
        </div>
      ) : (
        <span className="text-muted-foreground/50">
          检索与证据统计将在此显示
        </span>
      )}
    </div>
  );
}
