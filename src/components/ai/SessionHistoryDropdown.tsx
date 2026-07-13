import { History, Pencil, Trash2 } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";

import type { ChatLine } from "@/components/ai/AiMessageList";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { invokeErrorMessage } from "@/lib/credentials";
import {
  assistantSessionDelete,
  assistantSessionList,
  assistantSessionLoad,
  assistantSessionRename,
} from "@/lib/ipc";
import type {
  AiDomain,
  AssistantSessionMessage,
  AssistantSessionRef,
  AssistantSessionSummary,
} from "@/types/ai";

function formatRelativeTime(iso: string): string {
  const elapsed = Date.now() - new Date(iso).getTime();
  const minutes = Math.floor(elapsed / 60_000);
  if (minutes < 1) return "just now";
  if (minutes < 60) return `${minutes} minutes ago`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours} hours ago`;
  const days = Math.floor(hours / 24);
  if (days < 7) return `${days} days ago`;
  return new Date(iso).toLocaleDateString();
}

function toChatLines(messages: AssistantSessionMessage[]): ChatLine[] {
  return messages.map((message) => ({
    role: message.role as ChatLine["role"],
    content: message.content,
    seq: message.seq,
  }));
}

interface SessionHistoryDropdownProps {
  currentSession?: AssistantSessionRef | null;
  disabled?: boolean;
  domain: AiDomain;
  onSelectSession: (session: AssistantSessionRef, messages: ChatLine[]) => void;
  onDeleted?: (session: AssistantSessionRef) => void;
}

/** Domain-scoped history never exposes a storage primary key to the UI. */
export function SessionHistoryDropdown({
  currentSession,
  disabled,
  domain,
  onSelectSession,
  onDeleted,
}: SessionHistoryDropdownProps) {
  const [open, setOpen] = useState(false);
  const [sessions, setSessions] = useState<AssistantSessionSummary[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [editingSession, setEditingSession] =
    useState<AssistantSessionRef | null>(null);
  const [editTitle, setEditTitle] = useState("");
  const containerRef = useRef<HTMLDivElement>(null);

  const refresh = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      setSessions(await assistantSessionList({ domain, limit: 40 }));
    } catch (reason) {
      setError(invokeErrorMessage(reason));
    } finally {
      setLoading(false);
    }
  }, [domain]);

  useEffect(() => {
    if (open) void refresh();
  }, [open, refresh]);

  useEffect(() => {
    if (!open) return;
    const onDocumentClick = (event: MouseEvent) => {
      if (
        containerRef.current &&
        !containerRef.current.contains(event.target as Node)
      ) {
        setOpen(false);
      }
    };
    document.addEventListener("mousedown", onDocumentClick);
    return () => document.removeEventListener("mousedown", onDocumentClick);
  }, [open]);

  const handleSelect = useCallback(
    async (summary: AssistantSessionSummary) => {
      try {
        const messages = await assistantSessionLoad({
          session: summary.session,
        });
        onSelectSession(summary.session, toChatLines(messages));
        setOpen(false);
      } catch (reason) {
        setError(invokeErrorMessage(reason));
      }
    },
    [onSelectSession],
  );

  const handleDelete = useCallback(
    async (session: AssistantSessionRef, event: React.MouseEvent) => {
      event.stopPropagation();
      if (!window.confirm("Delete this conversation permanently?")) return;
      try {
        await assistantSessionDelete(session);
        setSessions((previous) =>
          previous.filter(
            (item) =>
              item.session.domain !== session.domain ||
              item.session.sessionKey !== session.sessionKey,
          ),
        );
        onDeleted?.(session);
      } catch (reason) {
        setError(invokeErrorMessage(reason));
      }
    },
    [onDeleted],
  );

  const handleRenameSubmit = useCallback(
    async (session: AssistantSessionRef) => {
      const title = editTitle.trim();
      if (!title) {
        setEditingSession(null);
        return;
      }
      try {
        await assistantSessionRename({ session, title });
        setSessions((previous) =>
          previous.map((item) =>
            item.session.domain === session.domain &&
            item.session.sessionKey === session.sessionKey
              ? { ...item, title }
              : item,
          ),
        );
        setEditingSession(null);
      } catch (reason) {
        setError(invokeErrorMessage(reason));
      }
    },
    [editTitle],
  );

  return (
    <div className="relative" ref={containerRef}>
      <Button
        type="button"
        variant="outline"
        size="sm"
        className="h-8 shrink-0 gap-1 px-2 text-xs"
        title="Conversation history"
        disabled={disabled}
        data-testid="session-history-trigger"
        onClick={() => setOpen((value) => !value)}
      >
        <History className="h-3.5 w-3.5" />
        History
      </Button>
      {open ? (
        <div
          className="absolute right-0 top-full z-50 mt-1 w-72 rounded-md border border-border bg-popover shadow-md"
          data-testid="session-history-popover"
        >
          <div className="border-b border-border/60 px-3 py-2">
            <p className="text-xs font-medium text-foreground">History</p>
            <p className="text-[10px] text-muted-foreground">
              Conversations in this security domain
            </p>
          </div>
          <div className="max-h-64 overflow-y-auto p-1">
            {loading ? (
              <p className="px-2 py-4 text-center text-xs text-muted-foreground">
                Loading...
              </p>
            ) : error ? (
              <p className="px-2 py-2 text-xs text-destructive">{error}</p>
            ) : sessions.length === 0 ? (
              <p className="px-2 py-4 text-center text-xs text-muted-foreground">
                No saved conversations
              </p>
            ) : (
              sessions.map((summary) => {
                const isCurrent =
                  currentSession?.domain === summary.session.domain &&
                  currentSession.sessionKey === summary.session.sessionKey;
                const isEditing =
                  editingSession?.domain === summary.session.domain &&
                  editingSession.sessionKey === summary.session.sessionKey;
                return (
                  <div
                    key={`${summary.session.domain}:${summary.session.sessionKey}`}
                    role="button"
                    tabIndex={0}
                    data-current={isCurrent}
                    className={`group flex w-full cursor-pointer items-start gap-2 rounded-md px-2 py-2 text-left text-xs hover:bg-muted/60 ${
                      isCurrent ? "bg-muted/50" : ""
                    }`}
                    onClick={() => void handleSelect(summary)}
                    onKeyDown={(event) => {
                      if (event.key === "Enter") void handleSelect(summary);
                    }}
                  >
                    <div className="min-w-0 flex-1">
                      {isEditing ? (
                        <Input
                          className="h-7 text-xs"
                          value={editTitle}
                          autoFocus
                          onClick={(event) => event.stopPropagation()}
                          onChange={(event) => setEditTitle(event.target.value)}
                          onBlur={() =>
                            void handleRenameSubmit(summary.session)
                          }
                          onKeyDown={(event) => {
                            if (event.key === "Enter") {
                              void handleRenameSubmit(summary.session);
                            }
                            if (event.key === "Escape") setEditingSession(null);
                          }}
                        />
                      ) : (
                        <p className="truncate font-medium text-foreground">
                          {summary.title}
                        </p>
                      )}
                      <p className="mt-0.5 text-[10px] text-muted-foreground">
                        {formatRelativeTime(summary.updatedAt)} ·{" "}
                        {summary.messageCount} messages
                      </p>
                    </div>
                    <div className="flex shrink-0 gap-0.5 opacity-0 group-hover:opacity-100">
                      <Button
                        type="button"
                        variant="ghost"
                        size="icon"
                        className="h-6 w-6"
                        title="Rename"
                        onClick={(event) => {
                          event.stopPropagation();
                          setEditingSession(summary.session);
                          setEditTitle(summary.title);
                        }}
                      >
                        <Pencil className="h-3 w-3" />
                      </Button>
                      <Button
                        type="button"
                        variant="ghost"
                        size="icon"
                        className="h-6 w-6 text-destructive"
                        title="Delete"
                        onClick={(event) =>
                          void handleDelete(summary.session, event)
                        }
                      >
                        <Trash2 className="h-3 w-3" />
                      </Button>
                    </div>
                  </div>
                );
              })
            )}
          </div>
        </div>
      ) : null}
    </div>
  );
}
