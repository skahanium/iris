import { useCallback, useState } from "react";
import {
  Check,
  ChevronDown,
  ChevronRight,
  FileText,
  Folder,
  Link,
  Loader2,
  RefreshCw,
  Tag,
  X,
} from "lucide-react";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { invokeErrorMessage } from "@/lib/credentials";
import { organizeApply, organizeExecute } from "@/lib/ipc";
import { cn } from "@/lib/utils";
import type { OrganizeSuggestion, OrganizeSuggestionType } from "@/types/ai";

// ─── Suggestion Type Config ─────────────────────────────

const SUGGESTION_TYPE_CONFIG: Record<
  OrganizeSuggestionType,
  { label: string; icon: typeof FileText; className: string }
> = {
  rename_title: {
    label: "重命名标题",
    icon: FileText,
    className: "text-blue-600 bg-blue-500/10",
  },
  add_tag: {
    label: "添加标签",
    icon: Tag,
    className: "text-green-600 bg-green-500/10",
  },
  move_to_folder: {
    label: "移动到文件夹",
    icon: Folder,
    className: "text-purple-600 bg-purple-500/10",
  },
  assign_corpus: {
    label: "归入语料库",
    icon: Folder,
    className: "text-orange-600 bg-orange-500/10",
  },
  add_block_link: {
    label: "添加链接",
    icon: Link,
    className: "text-cyan-600 bg-cyan-500/10",
  },
  extract_template: {
    label: "提取模板",
    icon: FileText,
    className: "text-pink-600 bg-pink-500/10",
  },
};

// ─── Component ──────────────────────────────────────────

interface OrganizePanelProps {
  onApplied?: () => void;
}

export function OrganizePanel({ onApplied }: OrganizePanelProps) {
  const [loading, setLoading] = useState(false);
  const [applying, setApplying] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [suggestions, setSuggestions] = useState<OrganizeSuggestion[]>([]);
  const [selectedIds, setSelectedIds] = useState<Set<string>>(new Set());
  const [expandedIds, setExpandedIds] = useState<Set<string>>(new Set());

  const handleRunTask = useCallback(async (taskType: string) => {
    setLoading(true);
    setError(null);
    try {
      const res = await organizeExecute({ task_type: taskType });
      setSuggestions(res.batch.suggestions as OrganizeSuggestion[]);
      setSelectedIds(new Set());
    } catch (e) {
      setError(invokeErrorMessage(e));
    } finally {
      setLoading(false);
    }
  }, []);

  const toggleSelect = useCallback((id: string) => {
    setSelectedIds((prev) => {
      const next = new Set(prev);
      if (next.has(id)) {
        next.delete(id);
      } else {
        next.add(id);
      }
      return next;
    });
  }, []);

  const toggleExpand = useCallback((id: string) => {
    setExpandedIds((prev) => {
      const next = new Set(prev);
      if (next.has(id)) {
        next.delete(id);
      } else {
        next.add(id);
      }
      return next;
    });
  }, []);

  const handleSelectAll = useCallback(() => {
    setSelectedIds(new Set(suggestions.map((s) => s.id)));
  }, [suggestions]);

  const handleDeselectAll = useCallback(() => {
    setSelectedIds(new Set());
  }, []);

  const handleAcceptSelected = useCallback(async () => {
    const toApply = suggestions.filter((s) => selectedIds.has(s.id));
    if (toApply.length === 0) return;
    setApplying(true);
    setError(null);
    try {
      const res = await organizeApply(toApply);
      setSuggestions((prev) =>
        prev.filter((s) => !res.applied.includes(s.id)),
      );
      setSelectedIds(new Set());
      if (res.errors.length > 0) {
        setError(res.errors.join("\n"));
      }
      onApplied?.();
    } catch (e) {
      setError(invokeErrorMessage(e));
    } finally {
      setApplying(false);
    }
  }, [selectedIds, suggestions, onApplied]);

  const handleRejectSelected = useCallback(() => {
    // Remove selected suggestions
    setSuggestions((prev) => prev.filter((s) => !selectedIds.has(s.id)));
    setSelectedIds(new Set());
  }, [selectedIds]);

  return (
    <Card className="border-border/60">
      <CardHeader className="pb-2">
        <div className="flex items-center justify-between">
          <CardTitle className="text-sm font-medium">笔记库整理</CardTitle>
          <div className="flex gap-1.5">
            <Button
              type="button"
              size="sm"
              variant="outline"
              onClick={() => handleRunTask("full_audit")}
              disabled={loading}
            >
              {loading ? (
                <Loader2 className="mr-1 h-3.5 w-3.5 animate-spin" />
              ) : (
                <RefreshCw className="mr-1 h-3.5 w-3.5" />
              )}
              全面审计
            </Button>
            <Button
              type="button"
              size="sm"
              variant="outline"
              onClick={() => handleRunTask("title_suggestions")}
              disabled={loading}
            >
              标题优化
            </Button>
            <Button
              type="button"
              size="sm"
              variant="outline"
              onClick={() => handleRunTask("tag_suggestions")}
              disabled={loading}
            >
              标签建议
            </Button>
          </div>
        </div>
      </CardHeader>
      <CardContent className="space-y-3">
        {error ? (
          <p className="rounded-md bg-destructive/10 px-2 py-1.5 text-xs text-destructive">
            {error}
          </p>
        ) : null}
        {/* Batch Actions */}
        {suggestions.length > 0 && (
          <div className="flex items-center justify-between rounded-md bg-muted/30 px-3 py-2">
            <div className="flex items-center gap-2">
              <span className="text-xs text-muted-foreground">
                已选 {selectedIds.size} / {suggestions.length} 条
              </span>
              <Button
                type="button"
                size="sm"
                variant="ghost"
                className="h-6 text-xs"
                onClick={handleSelectAll}
              >
                全选
              </Button>
              <Button
                type="button"
                size="sm"
                variant="ghost"
                className="h-6 text-xs"
                onClick={handleDeselectAll}
              >
                取消全选
              </Button>
            </div>
            <div className="flex gap-1.5">
              <Button
                type="button"
                size="sm"
                variant="outline"
                onClick={handleAcceptSelected}
                disabled={selectedIds.size === 0 || applying}
              >
                {applying ? (
                  <Loader2 className="mr-1 h-3.5 w-3.5 animate-spin" />
                ) : (
                  <Check className="mr-1 h-3.5 w-3.5" />
                )}
                接受选中
              </Button>
              <Button
                type="button"
                size="sm"
                variant="outline"
                onClick={handleRejectSelected}
                disabled={selectedIds.size === 0}
              >
                <X className="mr-1 h-3.5 w-3.5" />
                拒绝选中
              </Button>
            </div>
          </div>
        )}

        {/* Suggestions List */}
        {suggestions.length > 0 ? (
          <div className="space-y-2">
            {suggestions.map((suggestion) => {
              const config = SUGGESTION_TYPE_CONFIG[suggestion.suggestion_type];
              const Icon = config.icon;
              const isExpanded = expandedIds.has(suggestion.id);
              const isSelected = selectedIds.has(suggestion.id);

              return (
                <div
                  key={suggestion.id}
                  className={cn(
                    "overflow-hidden rounded-md border border-border/60",
                    isSelected && "border-primary/50 bg-primary/5",
                  )}
                >
                  {/* Suggestion header */}
                  <div className="flex items-start gap-2 bg-muted/30 px-3 py-2">
                    <input
                      type="checkbox"
                      checked={isSelected}
                      onChange={() => toggleSelect(suggestion.id)}
                      className="mt-1 h-3.5 w-3.5"
                    />
                    <button
                      type="button"
                      className="flex min-w-0 flex-1 items-start gap-2 text-left"
                      onClick={() => toggleExpand(suggestion.id)}
                    >
                      {isExpanded ? (
                        <ChevronDown className="mt-0.5 h-3.5 w-3.5 shrink-0 text-muted-foreground" />
                      ) : (
                        <ChevronRight className="mt-0.5 h-3.5 w-3.5 shrink-0 text-muted-foreground" />
                      )}
                      <div className="min-w-0 flex-1">
                        <div className="flex items-center gap-2">
                          <Badge
                            variant="outline"
                            className={cn("text-xs", config.className)}
                          >
                            <Icon className="mr-1 h-3 w-3" />
                            {config.label}
                          </Badge>
                          <span className="truncate text-xs font-medium">
                            {suggestion.target_path}
                          </span>
                        </div>
                        <div className="mt-1 text-xs text-muted-foreground">
                          {suggestion.reason}
                        </div>
                      </div>
                    </button>
                    <div className="flex shrink-0 items-center gap-1">
                      <Badge variant="outline" className="text-xs">
                        {Math.round(suggestion.confidence * 100)}%
                      </Badge>
                    </div>
                  </div>

                  {/* Expanded details */}
                  {isExpanded && (
                    <div className="space-y-2 border-t border-border/40 px-3 py-2">
                      {suggestion.current_value && (
                        <div>
                          <div className="text-xs font-medium text-muted-foreground">
                            当前值
                          </div>
                          <div className="rounded bg-muted/50 px-2 py-1 text-xs">
                            {suggestion.current_value}
                          </div>
                        </div>
                      )}
                      <div>
                        <div className="text-xs font-medium text-muted-foreground">
                          建议值
                        </div>
                        <div className="rounded bg-green-500/5 px-2 py-1 text-xs">
                          {suggestion.suggested_value}
                        </div>
                      </div>
                      <div className="flex items-center justify-between text-xs text-muted-foreground">
                        <span>来源: {suggestion.source}</span>
                        <span>文件: {suggestion.target_path}</span>
                      </div>
                    </div>
                  )}
                </div>
              );
            })}
          </div>
        ) : (
          <div className="py-8 text-center text-xs text-muted-foreground">
            {loading ? (
              <div className="flex items-center justify-center gap-2">
                <Loader2 className="h-4 w-4 animate-spin" />
                <span>正在分析笔记库...</span>
              </div>
            ) : (
              <div>
                <p>点击上方按钮开始分析笔记库</p>
                <p className="mt-1 text-muted-foreground/60">
                  支持全面审计、标题优化、标签建议等
                </p>
              </div>
            )}
          </div>
        )}
      </CardContent>
    </Card>
  );
}
