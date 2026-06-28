import { History, Pencil, Trash2 } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";

import type { ChatLine } from "@/components/ai/AiMessageList";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  evidenceRecordsToContextPackets,
  toChatLines,
} from "@/lib/ai/session-history";
import { invokeErrorMessage } from "@/lib/credentials";
import {
  classifiedAiThreadList,
  classifiedAiThreadLoad,
  classifiedAiThreadDelete,
  sessionDelete,
  sessionEvidenceList,
  sessionList,
  sessionLoad,
  sessionRename,
} from "@/lib/ipc";
import type { AiDomain } from "@/types/ai";
import type { ClassifiedAiThreadSummary, SessionSummary } from "@/types/ipc";

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

interface SessionHistoryDropdownProps {
  currentSessionId: number | string | null;
  disabled?: boolean;
  domain?: AiDomain;
  onSelectSession: (
    sessionId: number | string,
    messages: ChatLine[],
    ledgerPackets?: ChatLine["evidencePackets"],
  ) => void;
  onDeleted?: (sessionId: number | string) => void;
}

export function SessionHistoryDropdown({
  currentSessionId,
  disabled,
  domain = "normal",
  onSelectSession,
  onDeleted,
}: SessionHistoryDropdownProps) {
  const [open, setOpen] = useState(false);
  const [sessions, setSessions] = useState<SessionSummary[]>([]);
  const [classifiedSessions, setClassifiedSessions] = useState<
    ClassifiedAiThreadSummary[]
  >([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [editingId, setEditingId] = useState<number | string | null>(null);
  const [editTitle, setEditTitle] = useState("");
  const containerRef = useRef<HTMLDivElement>(null);

  const refresh = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      if (domain === "classified") {
        const list = await classifiedAiThreadList();
        setClassifiedSessions(list);
      } else {
        const list = await sessionList({ limit: 40 });
        setSessions(list);
      }
    } catch (e) {
      setError(invokeErrorMessage(e));
    } finally {
      setLoading(false);
    }
  }, [domain]);

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
    async (summary: SessionSummary | ClassifiedAiThreadSummary) => {
      try {
        if (domain === "classified") {
          const thread = await classifiedAiThreadLoad(
            (summary as ClassifiedAiThreadSummary).threadId,
          );
          // Convert classified messages to ChatLine format
          const chatLines: ChatLine[] = thread.messages.map((msg) => ({
            role: msg.role as ChatLine["role"],
            content: msg.content,
            seq: msg.seq,
            evidencePackets: msg.toolCalls ? undefined : undefined,
          }));
          onSelectSession(thread.threadId, chatLines, undefined);
        } else {
          const records = await sessionLoad((summary as SessionSummary).id);
          const ledger = await sessionEvidenceList(
            (summary as SessionSummary).id,
          ).catch(() => []);
          onSelectSession(
            (summary as SessionSummary).id,
            toChatLines(records),
            evidenceRecordsToContextPackets(ledger),
          );
        }
        setOpen(false);
      } catch (e) {
        setError(invokeErrorMessage(e));
      }
    },
    [domain, onSelectSession],
  );

  const handleDelete = useCallback(
    async (sessionId: number | string, event: React.MouseEvent) => {
      event.stopPropagation();
      if (!window.confirm("确定删除这条会话？此操作不可恢复。")) return;
      try {
        if (domain === "classified") {
          await classifiedAiThreadDelete(sessionId as string);
          setClassifiedSessions((prev) =>
            prev.filter((s) => s.threadId !== sessionId),
          );
        } else {
          await sessionDelete(sessionId as number);
          setSessions((prev) => prev.filter((s) => s.id !== sessionId));
        }
        onDeleted?.(sessionId);
      } catch (e) {
        setError(invokeErrorMessage(e));
      }
    },
    [domain, onDeleted],
  );

  const handleRenameSubmit = useCallback(
    async (sessionId: number | string) => {
      const title = editTitle.trim();
      if (!title) {
        setEditingId(null);
        return;
      }
      try {
        if (domain === "normal") {
          await sessionRename(sessionId as number, title);
          setSessions((prev) =>
            prev.map((s) => (s.id === sessionId ? { ...s, title } : s)),
          );
        }
        setEditingId(null);
      } catch (e) {
        setError(invokeErrorMessage(e));
      }
    },
    [domain, editTitle],
  );

  const isClassified = domain === "classified";
  const isEmpty = isClassified
    ? classifiedSessions.length === 0
    : sessions.length === 0;

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
          <div className="border-b border-border/60 px-3 py-2">
            <div className="flex items-start justify-between gap-2">
              <div>
                <p className="text-xs font-medium text-foreground">历史会话</p>
                <p className="text-[10px] text-muted-foreground">
                  {isClassified ? "涉密会话历史" : "显示全部历史会话"}
                </p>
              </div>
            </div>
          </div>
          <div className="max-h-64 overflow-y-auto p-1">
            {loading ? (
              <p className="px-2 py-4 text-center text-xs text-muted-foreground">
                加载中…
              </p>
            ) : error ? (
              <p className="px-2 py-2 text-xs text-destructive">{error}</p>
            ) : isEmpty ? (
              <p className="px-2 py-4 text-center text-xs text-muted-foreground">
                暂无历史会话
              </p>
            ) : isClassified ? (
              classifiedSessions.map((s) => (
                <div
                  key={s.threadId}
                  role="button"
                  tabIndex={0}
                  data-current={s.threadId === currentSessionId}
                  className={`group flex w-full cursor-pointer items-start gap-2 rounded-md px-2 py-2 text-left text-xs hover:bg-muted/60 ${
                    s.threadId === currentSessionId ? "bg-muted/50" : ""
                  }`}
                  onClick={() => void handleSelect(s)}
                  onKeyDown={(e) => {
                    if (e.key === "Enter") void handleSelect(s);
                  }}
                >
                  <div className="min-w-0 flex-1">
                    <p className="truncate font-medium text-foreground">
                      {s.title}
                    </p>
                    <p className="mt-0.5 text-[10px] text-muted-foreground">
                      {formatRelativeTime(s.updatedAt)} · {s.messageCount} 条
                    </p>
                  </div>
                  <div className="flex shrink-0 gap-0.5 opacity-0 group-hover:opacity-100">
                    <Button
                      type="button"
                      variant="ghost"
                      size="icon"
                      className="h-6 w-6 text-destructive"
                      title="删除"
                      onClick={(e) => void handleDelete(s.threadId, e)}
                    >
                      <Trash2 className="h-3 w-3" />
                    </Button>
                  </div>
                </div>
              ))
            ) : (
              sessions.map((s) => (
                <div
                  key={s.id}
                  role="button"
                  tabIndex={0}
                  data-current={s.id === currentSessionId}
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
