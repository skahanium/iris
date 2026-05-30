import {
  ChevronDown,
  ChevronRight,
  Loader2,
  CheckCircle2,
  XCircle,
  Wrench,
} from "lucide-react";
import { useState } from "react";

import { Badge } from "@/components/ui/badge";
import type { ToolCallInfo, ToolCallStatus } from "@/types/ai";

// Re-export for backward compatibility
export type { ToolCallInfo, ToolCallStatus };

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
  read_note: "读取笔记",
  list_vault: "列出笔记库",
  get_outline: "文档大纲",
  get_backlinks: "反向链接",
  web_search: "联网搜索",
  insert_text_at_cursor: "插入文本",
  replace_selection: "替换选区",
  add_tags: "添加标签",
  confirm_block_link: "确认链接",
  save_genre_template: "保存模板",
  update_user_rule: "更新规则",
  create_note_from_deposit: "创建笔记",
  spawn_subagent: "子任务 Agent",
  conclude_reasoning: "结束推理",
};

// ─── Status Icon ─────────────────────────────────────────

function StatusIcon({ status }: { status: ToolCallStatus }) {
  switch (status) {
    case "pending":
      return <Wrench className="h-3 w-3 text-muted-foreground" />;
    case "running":
      return <Loader2 className="h-3 w-3 animate-spin text-primary" />;
    case "completed":
      return <CheckCircle2 className="h-3 w-3 text-foreground/70" />;
    case "failed":
      return <XCircle className="h-3 w-3 text-destructive" />;
    case "rejected":
      return <XCircle className="h-3 w-3 text-muted-foreground" />;
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

function subagentTaskSummary(toolCall: ToolCallInfo): string | null {
  if (toolCall.name !== "spawn_subagent") return null;
  const task = toolCall.arguments?.task;
  return typeof task === "string" ? task : null;
}

export function ToolCallBubble({ toolCall }: ToolCallBubbleProps) {
  const [expanded, setExpanded] = useState(false);
  const displayName = TOOL_DISPLAY_NAMES[toolCall.name] ?? toolCall.name;
  const subTask = subagentTaskSummary(toolCall);
  const statusLine =
    subTask ??
    (toolCall.name === "spawn_subagent" && toolCall.result_summary
      ? toolCall.result_summary
      : statusLabel(toolCall.status));

  return (
    <div className="rounded-lg border border-border/80 bg-surface-elevated text-xs shadow-sm">
      <button
        type="button"
        className="flex w-full items-center gap-2 px-3 py-2 transition-colors hover:bg-muted/50"
        onClick={() => setExpanded(!expanded)}
      >
        <StatusIcon status={toolCall.status} />
        <Badge variant="secondary" className="px-1.5 py-0 text-[10px]">
          {displayName}
        </Badge>
        <span className="min-w-0 flex-1 truncate text-muted-foreground">
          {statusLine}
        </span>
        {toolCall.duration_ms !== undefined && (
          <span className="text-[10px] text-muted-foreground/70">
            {toolCall.duration_ms}ms
          </span>
        )}
        {toolCall.tokens_used !== undefined && (
          <span className="text-[10px] text-muted-foreground/70">
            {toolCall.tokens_used} tokens
          </span>
        )}
        <span className="flex-1" />
        {expanded ? (
          <ChevronDown className="h-3 w-3 text-muted-foreground" />
        ) : (
          <ChevronRight className="h-3 w-3 text-muted-foreground" />
        )}
      </button>

      {expanded && (
        <div className="space-y-2 border-t border-border px-3 py-2">
          {/* Arguments */}
          {toolCall.arguments && Object.keys(toolCall.arguments).length > 0 && (
            <div>
              <p className="mb-1 text-[10px] font-medium text-muted-foreground">
                参数
              </p>
              <pre className="whitespace-pre-wrap break-all rounded bg-background/50 p-2 text-[10px]">
                {JSON.stringify(toolCall.arguments, null, 2)}
              </pre>
            </div>
          )}

          {/* Result summary */}
          {toolCall.result_summary && (
            <div>
              <p className="mb-1 text-[10px] font-medium text-muted-foreground">
                结果
              </p>
              <p className="text-[10px] text-foreground/80">
                {toolCall.result_summary}
              </p>
            </div>
          )}

          {/* Error */}
          {toolCall.error && (
            <div>
              <p className="mb-1 text-[10px] font-medium text-red-500">错误</p>
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
