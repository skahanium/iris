import { ArrowRight } from "lucide-react";

import { Badge } from "@/components/ui/badge";
import type {
  ContextPacket,
  EvidenceRelation,
  EvidenceRelationType,
} from "@/types/ai";

interface EvidenceChainViewProps {
  packets: ContextPacket[];
  relations: EvidenceRelation[];
}

const RELATION_LABELS: Record<EvidenceRelationType, string> = {
  supports: "支持",
  contradicts: "矛盾",
  prerequisite: "前提",
  consequence: "结果",
  parallel: "并列",
};

const RELATION_COLORS: Record<EvidenceRelationType, string> = {
  supports: "bg-green-100 text-green-800",
  contradicts: "bg-red-100 text-red-800",
  prerequisite: "bg-blue-100 text-blue-800",
  consequence: "bg-purple-100 text-purple-800",
  parallel: "bg-gray-100 text-gray-800",
};

export function EvidenceChainView({
  packets,
  relations,
}: EvidenceChainViewProps) {
  const packetMap = new Map(packets.map((p) => [p.id, p]));

  if (packets.length === 0) {
    return null;
  }

  return (
    <div className="space-y-3">
      <h4 className="text-xs font-medium text-muted-foreground">证据链</h4>

      <div className="space-y-2">
        {packets.map((packet) => (
          <div
            key={packet.id}
            className="rounded-md border border-border/60 bg-surface-inset/40 p-2"
          >
            <div className="flex items-center gap-1.5">
              <Badge variant="secondary" className="px-1 py-0 text-[10px]">
                {packet.source_type}
              </Badge>
              <span className="truncate text-xs font-medium">
                {packet.title}
              </span>
            </div>
            <p className="mt-1 line-clamp-2 text-[11px] leading-relaxed text-muted-foreground">
              {packet.excerpt}
            </p>
            {packet.heading_path || packet.retrieval_reason ? (
              <div
                className="mt-1 truncate text-[10px] text-muted-foreground/80"
                data-testid="evidence-chain-meta"
              >
                {packet.heading_path ?? packet.retrieval_reason}
              </div>
            ) : null}
          </div>
        ))}
      </div>

      {relations.length > 0 && (
        <div className="space-y-1.5">
          <h5 className="text-[11px] font-medium text-muted-foreground">
            关联关系
          </h5>
          {relations.map((relation, index) => {
            const source = packetMap.get(relation.sourceId);
            const target = packetMap.get(relation.targetId);
            return (
              <div
                key={`${relation.sourceId}-${relation.targetId}-${index}`}
                className="flex items-center gap-1.5 text-[11px]"
              >
                <span className="max-w-[80px] truncate text-foreground/80">
                  {source?.title ?? relation.sourceId}
                </span>
                <ArrowRight className="h-3 w-3 shrink-0 text-muted-foreground" />
                <Badge
                  className={`${RELATION_COLORS[relation.relationType]} border-0 px-1 py-0 text-[10px]`}
                >
                  {RELATION_LABELS[relation.relationType]}
                </Badge>
                <span className="max-w-[80px] truncate text-foreground/80">
                  {target?.title ?? relation.targetId}
                </span>
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}
