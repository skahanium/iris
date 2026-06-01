import {
  Anchor,
  BookOpen,
  ChevronDown,
  ChevronRight,
  FileText,
  Globe,
  Link2,
  Scale,
} from "lucide-react";
import { useMemo } from "react";

import { ContextPacketList } from "@/components/ai/ContextPacketCard";
import { EvidenceChainView } from "@/components/ai/EvidenceChainView";
import { Badge } from "@/components/ui/badge";
import { countWebPackets } from "@/lib/assistant-chrome";
import { cn } from "@/lib/utils";
import type { ContextPacket, ContextStatus, EvidenceRelation } from "@/types/ai";

interface ContextPacketDrawerProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  packets: ContextPacket[];
  selectedIds: string[];
  onSelect: (id: string) => void;
  relations?: EvidenceRelation[];
  contextStatus?: ContextStatus | null;
  /** 点击了回复中的引用，但当前列表中无对应证据包 */
  citationMiss?: string | null;
}

export function ContextPacketDrawer({
  open,
  onOpenChange,
  packets,
  selectedIds,
  onSelect,
  relations,
  contextStatus = null,
  citationMiss = null,
}: ContextPacketDrawerProps) {
  const hasEvidenceChain = relations && relations.length > 0;
  const webCount = useMemo(() => countWebPackets(packets), [packets]);
  const localCount = packets.length - webCount;

  const extraStats = useMemo(() => {
    if (!contextStatus) return [];
    const items: { icon: typeof Scale; label: string; count: number }[] = [];
    if (contextStatus.regulations_loaded > 0) {
      items.push({
        icon: Scale,
        label: "法规",
        count: contextStatus.regulations_loaded,
      });
    }
    if (contextStatus.anchors_loaded > 0) {
      items.push({
        icon: Anchor,
        label: "锚点",
        count: contextStatus.anchors_loaded,
      });
    }
    if (contextStatus.links_loaded > 0) {
      items.push({
        icon: Link2,
        label: "链接",
        count: contextStatus.links_loaded,
      });
    }
    if (contextStatus.model_essays_loaded > 0) {
      items.push({
        icon: BookOpen,
        label: "范文",
        count: contextStatus.model_essays_loaded,
      });
    }
    return items;
  }, [contextStatus]);

  return (
    <div className="shrink-0 border-b border-border/60">
      <button
        type="button"
        className="flex w-full items-center gap-2 px-3 py-2 text-left text-xs font-medium text-muted-foreground transition-colors duration-base ease-iris-out hover:bg-surface-inset/60 hover:text-foreground"
        aria-expanded={open}
        onClick={() => onOpenChange(!open)}
      >
        {open ? (
          <ChevronDown className="h-3.5 w-3.5 shrink-0" />
        ) : (
          <ChevronRight className="h-3.5 w-3.5 shrink-0" />
        )}
        <span className="shrink-0">证据包</span>
        {localCount > 0 ? (
          <span
            className="inline-flex shrink-0 items-center gap-0.5 text-[10px] font-normal tabular-nums"
            data-testid="evidence-count-local"
          >
            <FileText className="h-3 w-3" />
            {localCount} 证据
          </span>
        ) : null}
        {webCount > 0 ? (
          <span
            className="inline-flex shrink-0 items-center gap-0.5 text-[10px] font-normal tabular-nums"
            data-testid="evidence-count-web"
          >
            <Globe className="h-3 w-3" />
            {webCount} 网络
          </span>
        ) : null}
        {extraStats.map(({ icon: Icon, label, count }) => (
          <span
            key={label}
            className="inline-flex shrink-0 items-center gap-0.5 text-[10px] font-normal tabular-nums text-muted-foreground/80"
          >
            <Icon className="h-3 w-3" />
            {count} {label}
          </span>
        ))}
        <span className="min-w-0 flex-1" />
        {packets.length > 0 ? (
          <Badge
            variant="secondary"
            className="h-5 shrink-0 px-1.5 text-[10px] tabular-nums"
          >
            {packets.length}
          </Badge>
        ) : null}
      </button>
      <div
        className={cn(
          "grid transition-[grid-template-rows] duration-base ease-iris-out motion-reduce:transition-none",
          open ? "grid-rows-[1fr]" : "grid-rows-[0fr]",
        )}
      >
        <div className="overflow-hidden">
          <div className="max-h-[220px] overflow-auto px-3 pb-3 pt-0">
            <ContextPacketList
              packets={packets}
              selectedIds={selectedIds}
              onSelect={onSelect}
              compact
              emptyHint={
                citationMiss
                  ? `未找到与「${citationMiss}」对应的证据包。该引用可能来自模型概括，或对应的检索结果尚未返回。`
                  : undefined
              }
            />
            {hasEvidenceChain ? (
              <div className="mt-3 border-t border-border/40 pt-3">
                <EvidenceChainView packets={packets} relations={relations} />
              </div>
            ) : null}
          </div>
        </div>
      </div>
    </div>
  );
}
