import { ChevronDown, ChevronRight, Loader2, CheckCircle2, XCircle, Wrench } from "lucide-react";
import { useState } from "react";

import { Badge } from "@/components/ui/badge";

// ─── Types ───────────────────────────────────────────────

export type ToolCallStatus = "pending" | "running" | "completed" | "failed" | "rejected";

export interface ToolCallInfo {
  id: string;
  name: string;
  arguments?: Record<string, unknown>;
  status: ToolCallStatus;
  result_summary?: string;
  error?: string;
}

// ─── Display Names ───────────────────────────────────────

const TOOL_DISPLAY_NAMES: Record<string, string> = {
  search_hybrid: "混合搜索",
  search_semantic: "语义搜索",
  search_keyword: "关键词搜索",
  get_regulation: "法规查询",
  get_context_packets: "获取证据包",
  get_genre_template: "获取文种模板",
  get_model_essays: "获取范文",
  get_block_links: "获取块级链接",
  web_search: "联网搜索",
  insert_text_at_cursor: "插入文本",
  replace_selection: "替换选区",
  add_tags: "添加标签",
  confirm_block_link: "确认链接",
  save_genre_template: "保存模板",
  update_user_rule: "更新规则",
  create_note_from_deposit: "创建笔记",
};

// ─── Status Icon ─────────────────────────────────────────

function StatusIcon({ status }: { status: ToolCallStatus }) {
  switch (status) {
    case "pending":
      return <Wrench className="h-3 w-3 text-muted-foreground" />;
    case "running":
      return <Loader2 className="h-3 w-3 text-blue-500 animate-spin" />;
    case "completed":
      return <CheckCircle2 className="h-3 w-3 text-emerald-500" />;
    case "failed":
      return <XCircle className="h-3 w-3 text-red-500" />;
    case "rejected":
      return <XCircle className="h-3 w-3 text-amber-500" />;
  }
}

// ─── Status Label ────────────────────────────────────────

function statusLabel(status: ToolCallStatus): string {
  switch (status) {
    case "pending":
      return "等待中";
    case "running":
      return "执行中";
    case "completed":
      return "已完成";
    case "failed":
      return "失败";
    case "rejected":
      return "已拒绝";
  }
}

// ─── Tool Call Bubble Component ──────────────────────────

interface ToolCallBubbleProps {
  toolCall: ToolCallInfo;
}

export function ToolCallBubble({ toolCall }: ToolCallBubbleProps) {
  const [expanded, setExpanded] = useState(false);
  const displayName = TOOL_DISPLAY_NAMES[toolCall.name] ?? toolCall.name;

  return (
    <div className="rounded-md border border-border bg-muted/30 text-xs">
      <button
        type="button"
        className="flex w-full items-center gap-2 px-3 py-2 hover:bg-muted/50 transition-colors"
        onClick={() => setExpanded(!expanded)}
      >
        <StatusIcon status={toolCall.status} />
        <Badge variant="secondary" className="text-[10px] px-1.5 py-0">
          {displayName}
        </Badge>
        <span className="text-muted-foreground">{statusLabel(toolCall.status)}</span>
        <span className="flex-1" />
        {expanded ? (
          <ChevronDown className="h-3 w-3 text-muted-foreground" />
        ) : (
          <ChevronRight className="h-3 w-3 text-muted-foreground" />
        )}
      </button>

      {expanded && (
        <div className="border-t border-border px-3 py-2 space-y-2">
          {/* Arguments */}
          {toolCall.arguments && Object.keys(toolCall.arguments).length > 0 && (
            <div>
              <p className="text-[10px] font-medium text-muted-foreground mb-1">参数</p>
              <pre className="text-[10px] whitespace-pre-wrap break-all bg-background/50 rounded p-2">
                {JSON.stringify(toolCall.arguments, null, 2)}
              </pre>
            </div>
          )}

          {/* Result summary */}
          {toolCall.result_summary && (
            <div>
              <p className="text-[10px] font-medium text-muted-foreground mb-1">结果</p>
              <p className="text-[10px] text-foreground/80">{toolCall.result_summary}</p>
            </div>
          )}

          {/* Error */}
          {toolCall.error && (
            <div>
              <p className="text-[10px] font-medium text-red-500 mb-1">错误</p>
              <p className="text-[10px] text-red-400">{toolCall.error}</p>
            </div>
          )}
        </div>
      )}
    </div>
  );
}

// ─── Tool Call List ──────────────────────────────────────

interface ToolCallListProps {
  toolCalls: ToolCallInfo[];
}

export function ToolCallList({ toolCalls }: ToolCallListProps) {
  if (toolCalls.length === 0) return null;

  return (
    <div className="mt-2 space-y-1">
      {toolCalls.map((tc) => (
        <ToolCallBubble key={tc.id} toolCall={tc} />
      ))}
    </div>
  );
}
