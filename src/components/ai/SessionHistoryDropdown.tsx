import { History, Pencil, Trash2 } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";

import type { ChatLine } from "@/components/ai/AiMessageList";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { invokeErrorMessage } from "@/lib/credentials";
import {
  sessionClearAll,
  sessionDelete,
  sessionList,
  sessionLoad,
  sessionRename,
} from "@/lib/ipc";
import type { AiScene } from "@/types/ai";
import type { SessionSummary } from "@/types/ipc";

function formatRelativeTime(iso: string): string {
  const then = new Date(iso).getTime();
  const diff = Date.now() - then;
  const minutes = Math.floor(diff / 60_000);
  if (minutes < 1) return "刚刚";
  if (minutes < 60) return `${minutes} 分钟前`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours} 小时前`;
  const days = Math.floor(hours / 24);
  if (days < 7) return `${days} 天前`;
  return new Date(iso).toLocaleDateString();
}

function toChatLines(
  records: Awaited<ReturnType<typeof sessionLoad>>,
): ChatLine[] {
  return records
    .filter(
      (m) => m.role === "user" || m.role === "assistant" || m.role === "system",
    )
    .map((m) => ({
      role: m.role as ChatLine["role"],
      content: m.content,
      seq: m.seq,
      created_at: m.created_at,
    }));
}

interface SessionHistoryDropdownProps {
  scene: AiScene;
  notePath: string | null;
  currentSessionId: number | null;
  disabled?: boolean;
  onSelectSession: (sessionId: number, messages: ChatLine[]) => void;
  onDeleted?: (sessionId: number) => void;
  onClearedAll?: () => void;
}

export function SessionHistoryDropdown({
  scene,
  notePath,
  currentSessionId,
  disabled,
  onSelectSession,
  onDeleted,
  onClearedAll,
}: SessionHistoryDropdownProps) {
  const [open, setOpen] = useState(false);
  const [sessions, setSessions] = useState<SessionSummary[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [editingId, setEditingId] = useState<number | null>(null);
  const [editTitle, setEditTitle] = useState("");
  const containerRef = useRef<HTMLDivElement>(null);

  const refresh = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const list = await sessionList({
        scene,
        note_path: notePath,
        limit: 40,
      });
      setSessions(list);
    } catch (e) {
      setError(invokeErrorMessage(e));
    } finally {
      setLoading(false);
    }
  }, [scene, notePath]);

  useEffect(() => {
    if (open) void refresh();
  }, [open, refresh]);

  useEffect(() => {
    if (!open) return;
    const onDocClick = (e: MouseEvent) => {
      if (
        containerRef.current &&
        !containerRef.current.contains(e.target as Node)
      ) {
        setOpen(false);
      }
    };
    document.addEventListener("mousedown", onDocClick);
    return () => document.removeEventListener("mousedown", onDocClick);
  }, [open]);

  const handleSelect = useCallback(
    async (summary: SessionSummary) => {
      try {
        const records = await sessionLoad(summary.id);
        onSelectSession(summary.id, toChatLines(records));
        setOpen(false);
      } catch (e) {
        setError(invokeErrorMessage(e));
      }
    },
    [onSelectSession],
  );

  const handleDelete = useCallback(
    async (sessionId: number, event: React.MouseEvent) => {
      event.stopPropagation();
      if (!window.confirm("确定删除这条会话？此操作不可恢复。")) return;
      try {
        await sessionDelete(sessionId);
        setSessions((prev) => prev.filter((s) => s.id !== sessionId));
        onDeleted?.(sessionId);
      } catch (e) {
        setError(invokeErrorMessage(e));
      }
    },
    [onDeleted],
  );

  const handleRenameSubmit = useCallback(
    async (sessionId: number) => {
      const title = editTitle.trim();
      if (!title) {
        setEditingId(null);
        return;
      }
      try {
        await sessionRename(sessionId, title);
        setSessions((prev) =>
          prev.map((s) => (s.id === sessionId ? { ...s, title } : s)),
        );
        setEditingId(null);
      } catch (e) {
        setError(invokeErrorMessage(e));
      }
    },
    [editTitle],
  );

  const handleClearAll = useCallback(async () => {
    if (!window.confirm("确定清空当前场景下的全部历史会话？此操作不可恢复。")) {
      return;
    }
    try {
      await sessionClearAll({ scene, note_path: notePath });
      setSessions([]);
      onClearedAll?.();
    } catch (e) {
      setError(invokeErrorMessage(e));
    }
  }, [scene, notePath, onClearedAll]);

  return (
    <div className="relative" ref={containerRef}>
      <Button
        type="button"
        variant="outline"
        size="sm"
        className="h-8 shrink-0 gap-1 px-2 text-xs"
        title="历史会话"
        disabled={disabled}
        data-testid="session-history-trigger"
        onClick={() => setOpen((v) => !v)}
      >
        <History className="h-3.5 w-3.5" />
        历史
      </Button>
      {open ? (
        <div
          className="absolute right-0 top-full z-50 mt-1 w-72 rounded-md border border-border bg-popover shadow-md"
          data-testid="session-history-popover"
        >
          <div className="flex items-start justify-between gap-2 border-b border-border/60 px-3 py-2">
            <div>
              <p className="text-xs font-medium text-foreground">历史会话</p>
              <p className="text-[10px] text-muted-foreground">
                仅显示当前场景下的对话
              </p>
            </div>
            {sessions.length > 0 ? (
              <Button
                type="button"
                variant="ghost"
                size="sm"
                className="h-7 shrink-0 px-2 text-[10px] text-destructive"
                onClick={() => void handleClearAll()}
              >
                清空全部
              </Button>
            ) : null}
          </div>
          <div className="max-h-64 overflow-y-auto p-1">
            {loading ? (
              <p className="px-2 py-4 text-center text-xs text-muted-foreground">
                加载中…
              </p>
            ) : error ? (
              <p className="px-2 py-2 text-xs text-destructive">{error}</p>
            ) : sessions.length === 0 ? (
              <p className="px-2 py-4 text-center text-xs text-muted-foreground">
                暂无历史会话
              </p>
            ) : (
              sessions.map((s) => (
                <div
                  key={s.id}
                  role="button"
                  tabIndex={0}
                  className={`group flex w-full cursor-pointer items-start gap-2 rounded-md px-2 py-2 text-left text-xs hover:bg-muted/60 ${
                    s.id === currentSessionId ? "bg-muted/50" : ""
                  }`}
                  onClick={() => void handleSelect(s)}
                  onKeyDown={(e) => {
                    if (e.key === "Enter") void handleSelect(s);
                  }}
                >
                  <div className="min-w-0 flex-1">
                    {editingId === s.id ? (
                      <Input
                        className="h-7 text-xs"
                        value={editTitle}
                        autoFocus
                        onClick={(e) => e.stopPropagation()}
                        onChange={(e) => setEditTitle(e.target.value)}
                        onBlur={() => void handleRenameSubmit(s.id)}
                        onKeyDown={(e) => {
                          if (e.key === "Enter") void handleRenameSubmit(s.id);
                          if (e.key === "Escape") setEditingId(null);
                        }}
                      />
                    ) : (
                      <p className="truncate font-medium text-foreground">
                        {s.title}
                      </p>
                    )}
                    <p className="mt-0.5 text-[10px] text-muted-foreground">
                      {formatRelativeTime(s.updated_at)} · {s.message_count} 条
                    </p>
                  </div>
                  <div className="flex shrink-0 gap-0.5 opacity-0 group-hover:opacity-100">
                    <Button
                      type="button"
                      variant="ghost"
                      size="icon"
                      className="h-6 w-6"
                      title="重命名"
                      onClick={(e) => {
                        e.stopPropagation();
                        setEditingId(s.id);
                        setEditTitle(s.title);
                      }}
                    >
                      <Pencil className="h-3 w-3" />
                    </Button>
                    <Button
                      type="button"
                      variant="ghost"
                      size="icon"
                      className="h-6 w-6 text-destructive"
                      title="删除"
                      onClick={(e) => void handleDelete(s.id, e)}
                    >
                      <Trash2 className="h-3 w-3" />
                    </Button>
                  </div>
                </div>
              ))
            )}
          </div>
        </div>
      ) : null}
    </div>
  );
}
