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
import { Button } from "@/components/ui/button";
import { EvidenceChainView } from "@/components/ai/EvidenceChainView";
import { Badge } from "@/components/ui/badge";
import {
  countWebPageFetchPackets,
  countWebSearchPackets,
} from "@/lib/assistant-chrome";
import { sessionEvidenceDetail } from "@/lib/ipc";
import { cn } from "@/lib/utils";
import type { AssistantArtifactDraft } from "@/types/assistant-artifact";
import type { SessionEvidenceDetailRecord } from "@/types/ipc";
import type {
  ContextPacket,
  ContextStatus,
  EvidenceRelation,
} from "@/types/ai";

interface ContextPacketDrawerProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  packets: ContextPacket[];
  selectedIds: string[];
  onSelect: (id: string) => void;
  onOpenSource?: (packet: ContextPacket) => void;
  relations?: EvidenceRelation[];
  contextStatus?: ContextStatus | null;
  /** 点击了回复中的引用，但当前列表中无对应证据包 */
  citationMiss?: string | null;
  sessionId?: number | null;
  onOpenArtifact?: (draft: AssistantArtifactDraft) => void;
}

export function ContextPacketDrawer({
  open,
  onOpenChange,
  packets,
  selectedIds,
  onSelect,
  onOpenSource,
  relations,
  contextStatus = null,
  citationMiss = null,
  sessionId = null,
  onOpenArtifact,
}: ContextPacketDrawerProps) {
  const hasEvidenceChain = relations && relations.length > 0;
  const webSearchCount = useMemo(
    () => countWebSearchPackets(packets),
    [packets],
  );
  const webPageCount = useMemo(
    () => countWebPageFetchPackets(packets),
    [packets],
  );
  const webCount = webSearchCount + webPageCount;
  const localCount = packets.length - webCount;
  const previewCount = useMemo(
    () => packets.filter((p) => p.provisional === true).length,
    [packets],
  );
  const traceableCount = useMemo(
    () =>
      packets.filter(
        (p) => p.source_span || p.heading_path || Boolean(p.content_hash),
      ).length,
    [packets],
  );

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

  const evidenceFromPackets = (): SessionEvidenceDetailRecord[] =>
    packets.map((packet, index) => ({
      id: index + 1,
      sessionId: sessionId ?? 0,
      citationIndex: index + 1,
      citationLabel: packet.citation_label,
      sourceType:
        packet.source_type === "web" ? ("web" as const) : ("local" as const),
      title: packet.title,
      sourcePath: packet.source_path ?? null,
      headingPath: packet.heading_path ?? null,
      retrievalReason: packet.retrieval_reason ?? null,
      url:
        packet.web?.url ??
        (packet.source_type === "web" ? packet.source_path : null),
      normalizedUrl: packet.web?.url?.toLowerCase() ?? null,
      domain: packet.web?.domain ?? null,
      failureReason: packet.web?.failure_reason ?? null,
      conflictGroup: packet.web?.conflict_group ?? null,
      conflictNote: packet.web?.conflict_note ?? null,
      createdAt: new Date().toISOString(),
      detailStatus: packet.source_path ? "source_available" : "source_missing",
      liveExcerpt: packet.excerpt ?? null,
    }));

  const openEvidenceDetail = async () => {
    if (!onOpenArtifact || packets.length === 0) return;
    let evidence: SessionEvidenceDetailRecord[] = evidenceFromPackets();
    if (sessionId != null) {
      try {
        evidence = await sessionEvidenceDetail(sessionId);
      } catch (err) {
        console.warn("[evidence] failed to load session evidence detail:", err);
      }
    }
    onOpenArtifact({
      kind: "session_evidence_detail",
      title: "Evidence Detail",
      sourceRequestId: sessionId ? String(sessionId) : "current-session",
      payload: {
        sessionId: sessionId ?? 0,
        evidence,
      },
      persistent: false,
    });
  };

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
        {previewCount > 0 ? (
          <Badge
            variant="outline"
            className="h-5 shrink-0 border-amber-300 px-1.5 text-[10px] text-amber-800"
          >
            {previewCount} 预览
          </Badge>
        ) : null}
        {localCount > 0 ? (
          <span
            className="inline-flex shrink-0 items-center gap-0.5 text-[10px] font-normal tabular-nums"
            data-testid="evidence-count-local"
          >
            <FileText className="h-3 w-3" />
            {localCount} 证据
          </span>
        ) : null}
        {webSearchCount > 0 ? (
          <span
            className="inline-flex shrink-0 items-center gap-0.5 text-[10px] font-normal tabular-nums"
            data-testid="evidence-count-web-search"
          >
            <Globe className="h-3 w-3" />
            {webSearchCount} 搜索
          </span>
        ) : null}
        {webPageCount > 0 ? (
          <span
            className="inline-flex shrink-0 items-center gap-0.5 text-[10px] font-normal tabular-nums"
            data-testid="evidence-count-web-page"
          >
            <Globe className="h-3 w-3" />
            {webPageCount} 正文
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
        {traceableCount > 0 ? (
          <span
            className="inline-flex shrink-0 items-center gap-0.5 text-[10px] font-normal tabular-nums text-muted-foreground/80"
            data-testid="evidence-count-traceable"
          >
            <Link2 className="h-3 w-3" />
            {traceableCount} 可追溯
          </span>
        ) : null}
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
          <div className="ai-task-surface mx-3 mb-3 max-h-[220px] overflow-auto px-3 pb-3 pt-3">
            {packets.length > 0 && onOpenArtifact ? (
              <div className="mb-2 flex justify-end">
                <Button
                  type="button"
                  variant="outline"
                  size="sm"
                  onClick={openEvidenceDetail}
                >
                  Detail
                </Button>
              </div>
            ) : null}
            <ContextPacketList
              packets={packets}
              selectedIds={selectedIds}
              onSelect={onSelect}
              onOpenSource={onOpenSource}
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
