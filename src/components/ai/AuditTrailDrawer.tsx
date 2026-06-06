import { History } from "lucide-react";
import { useEffect, useState } from "react";

import { Badge } from "@/components/ui/badge";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { ScrollArea } from "@/components/ui/scroll-area";
import { toolAuditQuery, type ToolAuditEntry } from "@/lib/ipc";

interface AuditTrailDrawerProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  requestId: string | null;
}

export function AuditTrailDrawer({
  open,
  onOpenChange,
  requestId,
}: AuditTrailDrawerProps) {
  const [entries, setEntries] = useState<ToolAuditEntry[]>([]);
  const [loading, setLoading] = useState(false);

  useEffect(() => {
    if (open && requestId) {
      setLoading(true);
      toolAuditQuery(requestId)
        .then(setEntries)
        .catch(() => setEntries([]))
        .finally(() => setLoading(false));
    } else {
      setEntries([]);
    }
  }, [open, requestId]);

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-md">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2 text-sm">
            <History className="h-4 w-4" />
            工具调用审计
          </DialogTitle>
          <DialogDescription className="text-xs">
            {requestId
              ? `请求 ${requestId.slice(0, 12)}…`
              : "查看当前会话的工具调用记录"}
          </DialogDescription>
        </DialogHeader>

        <ScrollArea className="max-h-[60vh]">
          {loading ? (
            <p className="text-xs text-muted-foreground">加载中…</p>
          ) : entries.length === 0 ? (
            <p className="text-xs text-muted-foreground">暂无工具调用记录</p>
          ) : (
            <div className="space-y-2">
              {entries.map((entry) => (
                <AuditEntry key={entry.id} entry={entry} />
              ))}
            </div>
          )}
        </ScrollArea>
      </DialogContent>
    </Dialog>
  );
}

function AuditEntry({ entry }: { entry: ToolAuditEntry }) {
  const isWriteTool = [
    "insert_text_at_cursor",
    "replace_selection",
    "add_tags",
    "confirm_block_link",
    "save_genre_template",
    "update_user_rule",
    "create_note_from_deposit",
  ].includes(entry.tool_name);

  return (
    <div className="rounded-md border border-border/60 px-3 py-2">
      <div className="flex items-center gap-2">
        <span className={entry.success ? "text-green-600" : "text-red-500"}>
          {entry.success ? "✓" : "✗"}
        </span>
        <span className="font-mono text-xs font-medium">{entry.tool_name}</span>
        {isWriteTool && (
          <Badge variant="outline" className="h-4 text-[9px] text-amber-600">
            写入
          </Badge>
        )}
        {entry.subagent_depth > 0 && (
          <Badge variant="outline" className="h-4 text-[9px]">
            子agent depth={entry.subagent_depth}
          </Badge>
        )}
      </div>

      {entry.arguments_summary && (
        <p className="mt-1 truncate text-[11px] text-muted-foreground">
          参数：{entry.arguments_summary}
        </p>
      )}

      {entry.result_summary && (
        <p className="mt-0.5 truncate text-[11px] text-muted-foreground">
          结果：{entry.result_summary}
        </p>
      )}

      <div className="mt-1 flex items-center gap-3 text-[10px] text-muted-foreground/60">
        {entry.duration_ms != null && <span>{entry.duration_ms}ms</span>}
        {entry.scene && <span>场景：{entry.scene}</span>}
        <span>轮次 {entry.harness_round}</span>
      </div>
    </div>
  );
}
