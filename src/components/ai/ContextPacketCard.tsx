import { FileText, Link2, Scale, BookOpen, MessageSquare, Globe } from "lucide-react";
import { useMemo } from "react";

import { Badge } from "@/components/ui/badge";
import { Card, CardContent, CardHeader } from "@/components/ui/card";
import type { ContextPacket, SourceType, TrustLevel } from "@/types/ai";

// ─── Source Type Icons ───────────────────────────────────

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

// ─── Trust Level Styling ─────────────────────────────────

const TRUST_STYLES: Record<TrustLevel, { bg: string; text: string; label: string }> = {
  user_note: {
    bg: "bg-emerald-500/10",
    text: "text-emerald-600",
    label: "用户笔记",
  },
  derived_cache: {
    bg: "bg-blue-500/10",
    text: "text-blue-600",
    label: "派生缓存",
  },
  external_web: {
    bg: "bg-amber-500/10",
    text: "text-amber-600",
    label: "外部网页",
  },
  model_generated: {
    bg: "bg-purple-500/10",
    text: "text-purple-600",
    label: "AI 生成",
  },
};

// ─── Component ───────────────────────────────────────────

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
  const trustStyle = TRUST_STYLES[packet.trust_level] ?? TRUST_STYLES.derived_cache;

  const scorePercent = useMemo(
    () => Math.round(packet.score * 100),
    [packet.score],
  );

  const truncatedExcerpt = useMemo(() => {
    if (compact && packet.excerpt.length > 120) {
      return packet.excerpt.slice(0, 120) + "…";
    }
    return packet.excerpt;
  }, [packet.excerpt, compact]);

  return (
    <Card
      className={`cursor-pointer transition-all hover:border-primary/50 ${
        selected ? "border-primary ring-1 ring-primary/20" : ""
      } ${packet.stale ? "opacity-60" : ""}`}
      onClick={() => onSelect?.(packet.id)}
    >
      <CardHeader className="space-y-1 p-3 pb-2">
        <div className="flex items-start justify-between gap-2">
          <div className="flex items-center gap-2 min-w-0">
            <Icon className={`h-4 w-4 shrink-0 ${trustStyle.text}`} />
            <span className="truncate text-sm font-medium">{packet.title}</span>
          </div>
          <div className="flex items-center gap-1.5 shrink-0">
            <Badge variant="secondary" className="text-[10px] px-1.5 py-0">
              {packet.citation_label}
            </Badge>
            <span
              className={`inline-flex items-center rounded-full px-1.5 py-0.5 text-[10px] font-medium ${trustStyle.bg} ${trustStyle.text}`}
            >
              {trustStyle.label}
            </span>
          </div>
        </div>

        {packet.heading_path && (
          <p className="text-xs text-muted-foreground truncate pl-6">
            {packet.heading_path}
          </p>
        )}
      </CardHeader>

      <CardContent className="p-3 pt-0">
        <p className="text-xs leading-relaxed text-foreground/80 whitespace-pre-wrap">
          {truncatedExcerpt}
        </p>

        <div className="mt-2 flex items-center justify-between text-[10px] text-muted-foreground">
          <div className="flex items-center gap-2">
            <span>{SOURCE_LABELS[packet.source_type]}</span>
            {packet.source_path && (
              <span className="truncate max-w-[150px]">
                {packet.source_path.split("/").pop()}
              </span>
            )}
          </div>

          <div className="flex items-center gap-2">
            <span>{packet.retrieval_reason}</span>
            <span
              className={`font-medium ${
                scorePercent >= 80
                  ? "text-emerald-600"
                  : scorePercent >= 50
                    ? "text-amber-600"
                    : "text-muted-foreground"
              }`}
            >
              {scorePercent}%
            </span>
          </div>
        </div>

        {packet.stale && (
          <p className="mt-1 text-[10px] text-amber-600">
            源文件已修改，内容可能过时
          </p>
        )}
      </CardContent>
    </Card>
  );
}

// ─── Compact List ────────────────────────────────────────

interface ContextPacketListProps {
  packets: ContextPacket[];
  selectedIds?: string[];
  onSelect?: (id: string) => void;
  compact?: boolean;
}

export function ContextPacketList({
  packets,
  selectedIds = [],
  onSelect,
  compact = false,
}: ContextPacketListProps) {
  if (packets.length === 0) {
    return (
      <div className="text-center text-sm text-muted-foreground py-4">
        未找到相关证据
      </div>
    );
  }

  return (
    <div className="space-y-2">
      {packets.map((packet) => (
        <ContextPacketCard
          key={packet.id}
          packet={packet}
          selected={selectedIds.includes(packet.id)}
          onSelect={onSelect}
          compact={compact}
        />
      ))}
    </div>
  );
}
