import {
  Bookmark,
  ChevronDown,
  ChevronRight,
  RotateCw,
  Trash2,
} from "lucide-react";
import { useCallback, useEffect, useMemo, useState } from "react";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { IrisOverlay } from "@/components/ui/iris-overlay";
import { ScrollArea } from "@/components/ui/scroll-area";
import {
  versionDelete,
  versionFinalizeCurrent,
  versionList,
  versionPreview,
  versionRestore,
} from "@/lib/ipc";
import type { VersionEntry } from "@/types/ipc";

import { buildRestoreConfirmMessage } from "./version-restore-confirm";
import {
  formatVersionDisplayTime,
  groupVersions,
  isFinalizedEntry,
  isGroupExpanded,
  kindLabel,
  type CollapsedVersionGroup,
} from "./version-timeline-groups";

interface VersionTimelineProps {
  open: boolean;
  onClose: () => void;
  notePath: string | null;
  currentContent?: string;
  getCurrentContent?: () => string;
  hasUnsavedEdits?: boolean;
  onRestore: (content: string) => void;
}

const PREVIEW_MAX = 2000;

export function VersionTimeline({
  open,
  onClose,
  notePath,
  currentContent,
  getCurrentContent,
  hasUnsavedEdits = false,
  onRestore,
}: VersionTimelineProps) {
  const [versions, setVersions] = useState<VersionEntry[]>([]);
  const [preview, setPreview] = useState<string | null>(null);
  const [previewId, setPreviewId] = useState<number | null>(null);
  const [finalizeLabel, setFinalizeLabel] = useState("");
  const [finalizing, setFinalizing] = useState(false);
  const [expandedGroups, setExpandedGroups] = useState<Set<string>>(
    () => new Set(),
  );

  const layout = useMemo(() => groupVersions(versions), [versions]);

  const refresh = useCallback(() => {
    if (!notePath) return;
    void versionList(notePath).then(setVersions);
  }, [notePath]);

  useEffect(() => {
    if (open) refresh();
  }, [open, refresh]);

  const toggleGroup = (groupKey: string) => {
    setExpandedGroups((prev) => {
      const next = new Set(prev);
      if (next.has(groupKey)) {
        next.delete(groupKey);
      } else {
        next.add(groupKey);
      }
      return next;
    });
  };

  const handlePreview = async (id: number) => {
    const content = await versionPreview(id);
    setPreview(content);
    setPreviewId(id);
  };

  const handleRestore = async (id: number) => {
    const target = versions.find((v) => v.id === id);
    if (!target) return;

    const message = buildRestoreConfirmMessage(target, hasUnsavedEdits);
    if (!window.confirm(message)) return;

    const liveContent = getCurrentContent?.() ?? currentContent ?? "";
    const result = await versionRestore(id, liveContent);
    onRestore(result.content);
    refresh();
    setPreview(null);
    setPreviewId(null);
  };

  const handleDelete = async (id: number) => {
    const target = versions.find((v) => v.id === id);
    if (
      target &&
      (target.is_finalized || target.kind === "finalize") &&
      !window.confirm("确定删除此定稿版本？删除后无法恢复。")
    ) {
      return;
    }

    await versionDelete(id);
    refresh();
    if (previewId === id) {
      setPreview(null);
      setPreviewId(null);
    }
  };

  const handleFinalizeCurrent = async () => {
    if (!notePath) return;
    setFinalizing(true);
    try {
      const liveContent = getCurrentContent?.() ?? currentContent ?? "";
      await versionFinalizeCurrent(
        notePath,
        liveContent,
        finalizeLabel.trim() || null,
      );
      setFinalizeLabel("");
      refresh();
    } finally {
      setFinalizing(false);
    }
  };

  const renderEntryActions = (v: VersionEntry) =>
    previewId === v.id &&
    preview !== null && (
      <div className="mt-2 flex gap-1">
        <Button
          type="button"
          size="sm"
          variant="outline"
          title="将当前正文替换为此版本（恢复前会自动备份）"
          onClick={() => void handleRestore(v.id)}
        >
          <RotateCw className="mr-1 h-3 w-3" />
          恢复
        </Button>
        <Button
          type="button"
          size="sm"
          variant="ghost"
          onClick={() => void handleDelete(v.id)}
        >
          <Trash2 className="h-3 w-3" />
        </Button>
      </div>
    );

  const renderEntry = (v: VersionEntry) => (
    <div
      key={v.id}
      className={`border-b border-border/50 px-4 py-2.5 text-sm transition-colors duration-base ease-iris-out ${
        previewId === v.id
          ? "bg-command-highlight"
          : "hover:bg-surface-inset/50"
      }`}
    >
      <button
        type="button"
        data-testid="version-entry-row"
        className="w-full text-left"
        onClick={() => void handlePreview(v.id)}
      >
        <div className="flex flex-wrap items-center gap-2">
          <span className="text-xs text-muted-foreground">
            {formatVersionDisplayTime(v)}
          </span>
          {!isFinalizedEntry(v) && (
            <span className="rounded bg-muted px-1 py-0 text-[10px] text-muted-foreground">
              {kindLabel(v.kind)}
            </span>
          )}
          {v.is_finalized && (
            <span className="rounded bg-primary/20 px-1 py-0 text-[10px] text-primary">
              定稿
            </span>
          )}
          {v.label && (
            <span className="text-xs text-muted-foreground">{v.label}</span>
          )}
        </div>
        <div className="mt-0.5 text-xs text-muted-foreground/70">
          {v.word_count.toLocaleString()} 字
        </div>
      </button>
      {renderEntryActions(v)}
    </div>
  );

  const renderCollapsedGroup = (group: CollapsedVersionGroup) => {
    const expanded = isGroupExpanded(expandedGroups, group.groupKey);
    return (
      <div key={group.groupKey} className="border-b border-border/50">
        <button
          type="button"
          data-testid="version-group-toggle"
          className="flex w-full items-center gap-2 px-4 py-2 text-left text-xs text-muted-foreground transition-colors duration-base ease-iris-out hover:bg-surface-inset/60"
          onClick={() => toggleGroup(group.groupKey)}
          aria-expanded={expanded}
        >
          {expanded ? (
            <ChevronDown className="h-3.5 w-3.5 shrink-0" />
          ) : (
            <ChevronRight className="h-3.5 w-3.5 shrink-0" />
          )}
          <span>{group.label}</span>
        </button>
        {expanded && group.entries.map((v) => renderEntry(v))}
      </div>
    );
  };

  const displayContent = currentContent ?? "";
  const currentPreview = displayContent.slice(0, PREVIEW_MAX);
  const historyPreview = preview?.slice(0, PREVIEW_MAX) ?? "";

  return (
    <IrisOverlay open={open} onClose={onClose} title="版本历史" size="wide">
      {notePath && (
        <div className="shrink-0 border-b border-border/60 bg-surface-inset/30 px-4 py-3">
          <p className="mb-2 text-xs text-muted-foreground">
            将当前正文保存为定稿版本（永久保留）
          </p>
          <div className="flex items-center gap-1.5">
            <Input
              className="h-8 flex-1 text-xs"
              placeholder="定稿名称（可选）"
              value={finalizeLabel}
              onChange={(e) => setFinalizeLabel(e.target.value)}
              disabled={finalizing}
            />
            <Button
              type="button"
              size="sm"
              variant="outline"
              disabled={finalizing}
              onClick={() => void handleFinalizeCurrent()}
            >
              <Bookmark className="mr-1 h-3.5 w-3.5" />
              定稿
            </Button>
          </div>
        </div>
      )}

      {preview !== null && previewId !== null && (
        <div className="shrink-0 border-b border-border/60 bg-surface-inset/30 px-4 py-3">
          <p className="mb-2 text-xs font-medium text-foreground">对比</p>
          <div className="grid grid-cols-2 gap-2">
            <div className="min-w-0">
              <p className="mb-1 text-[10px] text-muted-foreground">当前版</p>
              <pre className="max-h-32 overflow-auto whitespace-pre-wrap rounded border border-border bg-muted/20 p-2 font-mono text-[10px] leading-relaxed">
                {currentPreview}
                {displayContent.length > PREVIEW_MAX && "…"}
              </pre>
            </div>
            <div className="min-w-0">
              <p className="mb-1 text-[10px] text-muted-foreground">选中版本</p>
              <pre className="max-h-32 overflow-auto whitespace-pre-wrap rounded border border-border bg-muted/20 p-2 font-mono text-[10px] leading-relaxed">
                {historyPreview}
                {(preview?.length ?? 0) > PREVIEW_MAX && "…"}
              </pre>
            </div>
          </div>
        </div>
      )}

      <ScrollArea className="min-h-0 flex-1">
        {layout.isEmpty ? (
          <p className="p-3 text-xs text-muted-foreground">暂无版本快照</p>
        ) : (
          <>
            {layout.finalized.length > 0 && (
              <section>
                <h3 className="px-4 py-2 text-[11px] font-medium tracking-wider text-muted-foreground">
                  定稿
                </h3>
                {layout.finalized.map((v) => renderEntry(v))}
              </section>
            )}
            {layout.days.map((day) => (
              <section key={day.bucket}>
                <h3 className="px-4 py-2 text-[11px] font-medium tracking-wider text-muted-foreground">
                  {day.title}
                </h3>
                {day.visible.map((v) => renderEntry(v))}
                {day.collapsed.map((g) => renderCollapsedGroup(g))}
              </section>
            ))}
          </>
        )}
      </ScrollArea>
    </IrisOverlay>
  );
}
