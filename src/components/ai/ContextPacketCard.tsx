import {
  FileText,
  Link2,
  Scale,
  BookOpen,
  MessageSquare,
  Globe,
} from "lucide-react";
import { useMemo } from "react";

import { Badge } from "@/components/ui/badge";
import { SurfaceCard } from "@/components/ui/surface-card";
import type { ContextPacket, SourceType, TrustLevel } from "@/types/ai";
import { cn } from "@/lib/utils";

const SOURCE_ICONS: Record<SourceType, typeof FileText> = {
  note: FileText,
  anchor: Link2,
  regulation: Scale,
  template: BookOpen,
  session: MessageSquare,
  web: Globe,
};

const SOURCE_LABELS: Record<SourceType, string> = {
  note: "笔记",
  anchor: "锚点",
  regulation: "法规",
  template: "模板",
  session: "会话",
  web: "网页",
};

const TRUST_STYLES: Record<TrustLevel, { label: string }> = {
  user_note: { label: "用户笔记" },
  derived_cache: { label: "派生缓存" },
  external_web: { label: "外部网页" },
  model_generated: { label: "AI 生成" },
};

interface ContextPacketCardProps {
  packet: ContextPacket;
  selected?: boolean;
  onSelect?: (id: string) => void;
  compact?: boolean;
}

export function ContextPacketCard({
  packet,
  selected = false,
  onSelect,
  compact = false,
}: ContextPacketCardProps) {
  const Icon = SOURCE_ICONS[packet.source_type] ?? FileText;
  const trustLabel = TRUST_STYLES[packet.trust_level]?.label ?? "缓存";
  const isPreview = packet.provisional === true;

  const scorePercent = useMemo(
    () => Math.round(packet.score * 100),
    [packet.score],
  );

  const truncatedExcerpt = useMemo(() => {
    if (compact && packet.excerpt.length > 120) {
      return `${packet.excerpt.slice(0, 120)}…`;
    }
    return packet.excerpt;
  }, [packet.excerpt, compact]);

  return (
    <SurfaceCard
      selected={selected}
      className={cn(packet.stale && "opacity-60")}
      onClick={() => onSelect?.(packet.id)}
    >
      <div className="flex items-start justify-between gap-2">
        <div className="flex min-w-0 items-center gap-2">
          <Icon className="h-4 w-4 shrink-0 text-muted-foreground" />
          <span className="truncate text-sm font-medium">{packet.title}</span>
        </div>
        <div className="flex shrink-0 items-center gap-1.5">
          {isPreview ? (
            <Badge
              variant="outline"
              className="px-1.5 py-0 text-[10px] text-amber-700"
            >
              预览
            </Badge>
          ) : null}
          <Badge variant="secondary" className="px-1.5 py-0 text-[10px]">
            {packet.citation_label}
          </Badge>
          <span className="inline-flex items-center rounded-full border border-border/80 bg-surface-inset px-1.5 py-0.5 text-[10px] font-medium text-muted-foreground">
            {trustLabel}
          </span>
        </div>
      </div>

      {packet.heading_path ? (
        <p className="mt-1 truncate pl-6 text-xs text-muted-foreground">
          {packet.heading_path}
        </p>
      ) : null}

      <p className="mt-2 whitespace-pre-wrap text-xs leading-relaxed text-foreground/80">
        {truncatedExcerpt}
      </p>

      <div className="mt-2 flex items-center justify-between text-[10px] text-muted-foreground">
        <div className="flex items-center gap-2">
          <span>{SOURCE_LABELS[packet.source_type]}</span>
          {packet.source_path ? (
            <span className="max-w-[150px] truncate">
              {packet.source_path.split("/").pop()}
            </span>
          ) : null}
        </div>
        <div className="flex items-center gap-2 tabular-nums">
          <span>{packet.retrieval_reason}</span>
          <span className="font-medium text-foreground/70">
            {scorePercent}%
          </span>
        </div>
      </div>

      {packet.stale ? (
        <p className="mt-1 text-[10px] text-muted-foreground">
          源文件已修改，内容可能过时
        </p>
      ) : null}
    </SurfaceCard>
  );
}

interface ContextPacketListProps {
  packets: ContextPacket[];
  selectedIds?: string[];
  onSelect?: (id: string) => void;
  compact?: boolean;
  emptyHint?: string;
}

export function ContextPacketList({
  packets,
  selectedIds = [],
  onSelect,
  compact = false,
  emptyHint,
}: ContextPacketListProps) {
  if (packets.length === 0) {
    return (
      <div className="px-1 py-4 text-center text-xs leading-relaxed text-muted-foreground">
        {emptyHint ??
          "本轮未检索到证据包。可尝试用 @ 限定笔记范围，或提出与库内材料相关的问题。"}
      </div>
    );
  }

  const localPackets = packets.filter((p) => p.source_type !== "web");
  const webPackets = packets.filter((p) => p.source_type === "web");

  return (
    <div className="space-y-3">
      {localPackets.length > 0 && (
        <div>
          <p className="mb-1.5 text-[11px] font-medium text-muted-foreground">
            本地证据
          </p>
          <div className="space-y-2">
            {localPackets.map((packet) => (
              <ContextPacketCard
                key={packet.id}
                packet={packet}
                selected={selectedIds.includes(packet.id)}
                onSelect={onSelect}
                compact={compact}
              />
            ))}
          </div>
        </div>
      )}
      {webPackets.length > 0 && (
        <div>
          <p className="mb-1.5 text-[11px] font-medium text-muted-foreground">
            网络证据
          </p>
          <div className="space-y-2">
            {webPackets.map((packet) => (
              <ContextPacketCard
                key={packet.id}
                packet={packet}
                selected={selectedIds.includes(packet.id)}
                onSelect={onSelect}
                compact={compact}
              />
            ))}
          </div>
        </div>
      )}
    </div>
  );
}
