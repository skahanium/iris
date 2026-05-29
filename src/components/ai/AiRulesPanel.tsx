import { useCallback, useEffect, useState } from "react";
import {
  BookMarked,
  Plus,
  Trash2,
  ToggleLeft,
  ToggleRight,
} from "lucide-react";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader } from "@/components/ui/card";
import { invokeErrorMessage } from "@/lib/credentials";
import {
  profileDeactivate,
  profileDelete,
  profileGet,
  profileList,
  profileSetRule,
} from "@/lib/ipc";

const RULE_KEYS: { key: string; label: string; hint: string }[] = [
  {
    key: "custom_rules",
    label: "自定义规则",
    hint: "通用写作与回答约束",
  },
  {
    key: "writing_style",
    label: "写作风格",
    hint: "语气、篇幅、结构偏好",
  },
  {
    key: "citation_habits",
    label: "引用习惯",
    hint: "引用格式与依据要求",
  },
  {
    key: "tool_preferences",
    label: "工具偏好",
    hint: "联网、检索范围等",
  },
  {
    key: "agent_behavior",
    label: "Agent 行为",
    hint: "自主程度与确认习惯",
  },
];

interface ProfileEntry {
  key: string;
  value: unknown;
  source: string;
  confidence: number;
  is_active: boolean;
  updated_at: string;
}

function ruleText(value: unknown): string {
  if (typeof value === "string") return value;
  if (value && typeof value === "object" && "description" in value) {
    const d = (value as { description?: unknown }).description;
    if (typeof d === "string") return d;
  }
  return JSON.stringify(value, null, 2);
}

interface AiRulesPanelProps {
  /** 设置页内嵌时隐藏大标题、收紧边距 */
  compact?: boolean;
}

export function AiRulesPanel({ compact = false }: AiRulesPanelProps) {
  const [entries, setEntries] = useState<ProfileEntry[]>([]);
  const [showInactive, setShowInactive] = useState(false);
  const [newKey, setNewKey] = useState("custom_rules");
  const [newText, setNewText] = useState("");
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    try {
      const items = await profileList({ include_inactive: showInactive });
      setEntries(items as unknown as ProfileEntry[]);
    } catch (e) {
      setError(invokeErrorMessage(e));
    }
  }, [showInactive]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const handleAdd = useCallback(async () => {
    const text = newText.trim();
    if (!text || !newKey) return;
    setError(null);
    try {
      await profileSetRule({
        key: newKey,
        description: text,
        source: "user_manual",
      });
      setNewText("");
      void refresh();
    } catch (e) {
      setError(invokeErrorMessage(e));
    }
  }, [newKey, newText, refresh]);

  const handleToggle = useCallback(
    async (entry: ProfileEntry) => {
      try {
        if (entry.is_active) {
          await profileDeactivate({ key: entry.key });
        } else {
          const full = await profileGet({ key: entry.key });
          if (full) {
            await profileSetRule({
              key: entry.key,
              description: ruleText(full.value),
              source: "user_reactivate",
            });
          }
        }
        void refresh();
      } catch (e) {
        setError(invokeErrorMessage(e));
      }
    },
    [refresh],
  );

  const handleDelete = useCallback(
    async (key: string) => {
      if (!window.confirm("永久删除此规则？")) return;
      await profileDelete({ key });
      void refresh();
    },
    [refresh],
  );

  const labelFor = (key: string) =>
    RULE_KEYS.find((r) => r.key === key)?.label ?? key;

  return (
    <div
      className={
        compact
          ? "flex max-h-[360px] min-h-0 flex-col overflow-hidden p-2"
          : "flex min-h-0 flex-1 flex-col overflow-hidden p-3"
      }
    >
      {!compact ? (
        <div className="mb-2 shrink-0">
          <div className="mb-1 flex items-center gap-2">
            <BookMarked className="h-4 w-4 text-primary" />
            <span className="text-sm font-medium">规则中心</span>
          </div>
          <p className="text-xs text-muted-foreground">
            仅在你确认后保存；按场景注入工作流，不污染知识库检索。
          </p>
        </div>
      ) : null}

      <div className="mb-2 flex shrink-0 items-center gap-2">
        <button
          type="button"
          className="flex items-center gap-1 text-xs text-muted-foreground"
          onClick={() => setShowInactive(!showInactive)}
        >
          {showInactive ? (
            <ToggleRight className="h-4 w-4 text-primary" />
          ) : (
            <ToggleLeft className="h-4 w-4" />
          )}
          显示已停用
        </button>
      </div>

      <Card className="mb-2 shrink-0 border-border/60">
        <CardHeader className="p-2 pb-1">
          <span className="text-xs font-medium">添加规则</span>
        </CardHeader>
        <CardContent className="space-y-2 p-2 pt-0">
          <select
            className="w-full rounded-md border border-border bg-background px-2 py-1 text-xs"
            value={newKey}
            onChange={(e) => setNewKey(e.target.value)}
          >
            {RULE_KEYS.map((r) => (
              <option key={r.key} value={r.key}>
                {r.label}
              </option>
            ))}
          </select>
          <p className="text-[10px] text-muted-foreground">
            {RULE_KEYS.find((r) => r.key === newKey)?.hint}
          </p>
          <textarea
            className="min-h-[72px] w-full resize-none rounded-md border border-border bg-background px-2 py-1.5 text-xs"
            placeholder="用自然语言描述规则，例如：引用必须标注来源…"
            value={newText}
            onChange={(e) => setNewText(e.target.value)}
          />
          <Button
            type="button"
            size="sm"
            className="w-full"
            disabled={!newText.trim()}
            onClick={() => void handleAdd()}
          >
            <Plus className="mr-1 h-3.5 w-3.5" />
            保存规则
          </Button>
        </CardContent>
      </Card>

      {error ? (
        <p className="mb-2 shrink-0 text-xs text-destructive">{error}</p>
      ) : null}

      <div className="min-h-0 flex-1 space-y-2 overflow-y-auto">
        {entries.length === 0 ? (
          <p className="text-center text-xs text-muted-foreground">
            暂无规则。可在对话中说「以后都这样」由 AI 提议，或在此手动添加。
          </p>
        ) : (
          entries.map((entry) => (
            <Card
              key={entry.key}
              className={!entry.is_active ? "opacity-50" : ""}
            >
              <CardHeader className="flex flex-row items-center gap-2 p-2 pb-0">
                <Badge variant="outline" className="text-[10px]">
                  {labelFor(entry.key)}
                </Badge>
                <Badge variant="secondary" className="text-[10px]">
                  {entry.source}
                </Badge>
              </CardHeader>
              <CardContent className="p-2 pt-1">
                <p className="whitespace-pre-wrap text-xs">
                  {ruleText(entry.value)}
                </p>
                <div className="mt-2 flex gap-1">
                  <Button
                    type="button"
                    variant="ghost"
                    size="icon"
                    className="h-7 w-7"
                    title={entry.is_active ? "停用" : "启用"}
                    onClick={() => void handleToggle(entry)}
                  >
                    {entry.is_active ? (
                      <ToggleRight className="h-3.5 w-3.5 text-primary" />
                    ) : (
                      <ToggleLeft className="h-3.5 w-3.5" />
                    )}
                  </Button>
                  <Button
                    type="button"
                    variant="ghost"
                    size="icon"
                    className="h-7 w-7 text-destructive"
                    onClick={() => void handleDelete(entry.key)}
                  >
                    <Trash2 className="h-3.5 w-3.5" />
                  </Button>
                </div>
              </CardContent>
            </Card>
          ))
        )}
      </div>
    </div>
  );
}
