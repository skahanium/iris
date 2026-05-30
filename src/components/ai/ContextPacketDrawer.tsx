import { ChevronDown, ChevronRight } from "lucide-react";

import { ContextPacketList } from "@/components/ai/ContextPacketCard";
import { EvidenceChainView } from "@/components/ai/EvidenceChainView";
import { Badge } from "@/components/ui/badge";
import type { ContextPacket, EvidenceRelation } from "@/types/ai";
import { cn } from "@/lib/utils";

interface ContextPacketDrawerProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  packets: ContextPacket[];
  selectedIds: string[];
  onSelect: (id: string) => void;
  relations?: EvidenceRelation[];
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
  citationMiss = null,
}: ContextPacketDrawerProps) {
  const hasEvidenceChain = relations && relations.length > 0;

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
        <span className="flex-1">证据包</span>
        {packets.length > 0 ? (
          <Badge
            variant="secondary"
            className="h-5 px-1.5 text-[10px] tabular-nums"
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
            {hasEvidenceChain && (
              <div className="mt-3 border-t border-border/40 pt-3">
                <EvidenceChainView packets={packets} relations={relations} />
              </div>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
