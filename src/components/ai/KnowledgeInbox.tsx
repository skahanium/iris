import { useState, useCallback, useEffect } from "react";
import {
  Inbox,
  Archive,
  FileText,
  Trash2,
  ChevronDown,
  ChevronRight,
} from "lucide-react";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader } from "@/components/ui/card";
import { ScrollArea } from "@/components/ui/scroll-area";
import {
  inboxList,
  inboxCounts,
  inboxUpdateStatus,
  inboxDelete,
} from "@/lib/ipc";

// ─── Types ───────────────────────────────────────────────

interface KnowledgeDeposit {
  id: number;
  session_id: number | null;
  source_note: string | null;
  deposit_type: string;
  content: string;
  status: string;
  target_path: string | null;
  created_at: string;
  updated_at: string;
}

interface InboxCounts {
  inbox: number;
  archived: number;
  written: number;
}

// ─── Component ───────────────────────────────────────────

export function KnowledgeInbox() {
  const [deposits, setDeposits] = useState<KnowledgeDeposit[]>([]);
  const [counts, setCounts] = useState<InboxCounts>({
    inbox: 0,
    archived: 0,
    written: 0,
  });
  const [activeTab, setActiveTab] = useState<"inbox" | "archived" | "written">(
    "inbox",
  );
  const [expandedId, setExpandedId] = useState<number | null>(null);

  const refresh = useCallback(async () => {
    try {
      const [items, cnt] = await Promise.all([
        inboxList({ status: activeTab }),
        inboxCounts(),
      ]);
      setDeposits(items as unknown as KnowledgeDeposit[]);
      setCounts(cnt as unknown as InboxCounts);
    } catch {
      // ignore
    }
  }, [activeTab]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const handleArchive = useCallback(
    async (id: number) => {
      await inboxUpdateStatus({
        deposit_id: id,
        new_status: "archived",
      });
      void refresh();
    },
    [refresh],
  );

  const handleDelete = useCallback(
    async (id: number) => {
      await inboxDelete({ deposit_id: id });
      void refresh();
    },
    [refresh],
  );

  const tabs = [
    {
      key: "inbox" as const,
      label: "收件箱",
      count: counts.inbox,
      icon: Inbox,
    },
    {
      key: "archived" as const,
      label: "已归档",
      count: counts.archived,
      icon: Archive,
    },
    {
      key: "written" as const,
      label: "已写入",
      count: counts.written,
      icon: FileText,
    },
  ];

  return (
    <div className="flex h-full flex-col">
      {/* Header */}
      <div className="border-b border-border p-3">
        <div className="mb-2 flex items-center gap-2">
          <Inbox className="h-4 w-4 text-primary" />
          <span className="text-sm font-medium">AI 收件箱</span>
        </div>

        <div className="flex gap-1">
          {tabs.map((tab) => (
            <button
              key={tab.key}
              type="button"
              className={`flex items-center gap-1 rounded-md px-2 py-1 text-xs transition-colors ${
                activeTab === tab.key
                  ? "bg-primary text-primary-foreground"
                  : "text-muted-foreground hover:bg-muted"
              }`}
              onClick={() => setActiveTab(tab.key)}
            >
              <tab.icon className="h-3 w-3" />
              {tab.label}
              {tab.count > 0 && (
                <Badge
                  variant={activeTab === tab.key ? "secondary" : "outline"}
                  className="ml-0.5 text-[10px]"
                >
                  {tab.count}
                </Badge>
              )}
            </button>
          ))}
        </div>
      </div>

      {/* Content */}
      <ScrollArea className="flex-1">
        {deposits.length === 0 ? (
          <div className="flex h-full flex-col items-center justify-center text-muted-foreground">
            <Inbox className="mb-2 h-8 w-8 opacity-30" />
            <p className="text-sm">
              {activeTab === "inbox" ? "收件箱为空" : "没有记录"}
            </p>
          </div>
        ) : (
          <div className="space-y-2 p-3">
            {deposits.map((deposit) => (
              <Card key={deposit.id}>
                <CardHeader className="p-2 pb-1">
                  <div className="flex items-center gap-2">
                    <button
                      type="button"
                      className="flex flex-1 items-center gap-1 text-left"
                      onClick={() =>
                        setExpandedId(
                          expandedId === deposit.id ? null : deposit.id,
                        )
                      }
                    >
                      {expandedId === deposit.id ? (
                        <ChevronDown className="h-3 w-3 shrink-0" />
                      ) : (
                        <ChevronRight className="h-3 w-3 shrink-0" />
                      )}
                      <Badge variant="outline" className="text-[10px]">
                        {deposit.deposit_type}
                      </Badge>
                      <span className="truncate text-xs">
                        {deposit.content.slice(0, 50)}
                        {deposit.content.length > 50 ? "…" : ""}
                      </span>
                    </button>

                    {activeTab === "inbox" && (
                      <div className="flex shrink-0 gap-1">
                        <Button
                          variant="ghost"
                          size="icon"
                          className="h-6 w-6"
                          title="归档"
                          onClick={() => void handleArchive(deposit.id)}
                        >
                          <Archive className="h-3 w-3" />
                        </Button>
                        <Button
                          variant="ghost"
                          size="icon"
                          className="h-6 w-6 text-destructive"
                          title="删除"
                          onClick={() => void handleDelete(deposit.id)}
                        >
                          <Trash2 className="h-3 w-3" />
                        </Button>
                      </div>
                    )}
                  </div>
                </CardHeader>

                {expandedId === deposit.id && (
                  <CardContent className="p-2 pt-0">
                    <div className="whitespace-pre-wrap rounded-md bg-muted p-2 text-xs">
                      {deposit.content}
                    </div>
                    <div className="mt-1 text-[10px] text-muted-foreground">
                      {deposit.source_note && (
                        <span>来源: {deposit.source_note} | </span>
                      )}
                      <span>
                        创建于{" "}
                        {new Date(deposit.created_at).toLocaleDateString()}
                      </span>
                    </div>
                  </CardContent>
                )}
              </Card>
            ))}
          </div>
        )}
      </ScrollArea>
    </div>
  );
}
