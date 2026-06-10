import { useCallback, useEffect, useState } from "react";
import {
  BookMarked,
  Brain,
  ClipboardList,
  Plus,
  ShieldCheck,
  Trash2,
  ToggleLeft,
  ToggleRight,
  type LucideIcon,
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

function formatProfileEntry(entry: ProfileEntry): string {
  if (entry.key !== "ai_prompt_profile") return ruleText(entry.value);
  if (!entry.value || typeof entry.value !== "object") {
    return ruleText(entry.value);
  }

  const value = entry.value as Record<string, unknown>;
  const lines: string[] = [];
  const displayName = value.display_name;
  const language = value.language;
  const writingStyle = value.writing_style;
  const persona = value.persona;
  const customRules = Array.isArray(value.custom_rules)
    ? value.custom_rules.length
    : 0;

  if (typeof displayName === "string" && displayName.trim()) {
    lines.push(`称呼：${displayName}`);
  }
  if (typeof language === "string" && language.trim()) {
    lines.push(`语言：${language}`);
  }
  if (typeof writingStyle === "string" && writingStyle.trim()) {
    lines.push(`写作风格：${writingStyle}`);
  }
  if (typeof persona === "string" && persona.trim()) {
    lines.push(`人格：${persona}`);
  }
  lines.push(`自定义规则：${customRules} 条`);

  return lines.join("\n");
}

interface AiRulesPanelProps {
  compact?: boolean;
}

function RulesSummaryTile({
  icon: Icon,
  label,
  value,
}: {
  icon: LucideIcon;
  label: string;
  value: string;
}) {
  return (
    <div className="rounded-md border border-border/60 bg-surface-inset/25 px-3 py-2.5">
      <div className="flex items-center gap-2 text-muted-foreground">
        <Icon className="h-3.5 w-3.5" />
        <span className="text-[11px] font-medium">{label}</span>
      </div>
      <p className="mt-1.5 text-sm font-semibold text-foreground">{value}</p>
    </div>
  );
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
      try {
        await profileDelete({ key });
        void refresh();
      } catch (e) {
        setError(invokeErrorMessage(e));
      }
    },
    [refresh],
  );

  const labelFor = (key: string) =>
    RULE_KEYS.find((r) => r.key === key)?.label ?? key;

  const activeEntries = entries.filter((entry) => entry.is_active);
  const inactiveEntries = entries.length - activeEntries.length;
  const manualEntries = entries.filter((entry) =>
    entry.source.includes("user"),
  ).length;

  return (
    <div
      data-testid="ai-rules-workbench"
      className={
        compact
          ? "grid min-h-[520px] gap-3 overflow-hidden p-0 xl:grid-cols-[minmax(320px,0.85fr)_minmax(0,1.35fr)]"
          : "grid min-h-0 flex-1 gap-3 overflow-hidden p-3 xl:grid-cols-[minmax(320px,0.85fr)_minmax(0,1.35fr)]"
      }
    >
      <div className="min-h-0 space-y-3 overflow-y-auto pr-1">
        {!compact ? (
          <div>
            <div className="mb-1 flex items-center gap-2">
              <BookMarked className="h-4 w-4 text-primary" />
              <span className="text-sm font-medium">规则中心</span>
            </div>
            <p className="text-xs text-muted-foreground">
              仅在你确认后保存；按场景注入工作流，不污染知识库检索。
            </p>
          </div>
        ) : null}

        <div
          className="grid gap-2 sm:grid-cols-3 xl:grid-cols-1"
          data-testid="ai-rules-summary-grid"
        >
          <RulesSummaryTile
            icon={ClipboardList}
            label="启用规则"
            value={`${activeEntries.length} 条`}
          />
          <RulesSummaryTile
            icon={Brain}
            label="用户记忆"
            value={`${manualEntries} 条`}
          />
          <RulesSummaryTile
            icon={ShieldCheck}
            label="停用记录"
            value={`${inactiveEntries} 条`}
          />
        </div>

        <Card className="shrink-0 border-border/60 bg-background">
          <CardHeader className="p-3 pb-2">
            <div className="flex items-center justify-between gap-2">
              <span className="text-sm font-semibold">添加规则</span>
              <Badge variant="outline" className="text-[10px]">
                user_manual
              </Badge>
            </div>
            <p className="text-xs text-muted-foreground">
              {RULE_KEYS.find((r) => r.key === newKey)?.hint}
            </p>
          </CardHeader>
          <CardContent className="space-y-2 p-3 pt-0">
            <select
              className="h-9 w-full rounded-md border border-border bg-background px-2 text-xs"
              value={newKey}
              onChange={(e) => setNewKey(e.target.value)}
            >
              {RULE_KEYS.map((r) => (
                <option key={r.key} value={r.key}>
                  {r.label}
                </option>
              ))}
            </select>
            <textarea
              className="min-h-[128px] w-full resize-none rounded-md border border-border bg-background px-3 py-2 text-xs leading-relaxed"
              placeholder="用自然语言描述规则，例如：引用必须标注来源..."
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
          <p className="rounded-md border border-destructive/25 bg-destructive/5 px-3 py-2 text-xs text-destructive">
            {error}
          </p>
        ) : null}
      </div>

      <div className="flex min-h-0 flex-col rounded-md border border-border/60 bg-background">
        <div className="flex shrink-0 items-center justify-between gap-3 border-b border-border/60 px-3 py-2.5">
          <div className="flex items-center gap-2">
            <BookMarked className="h-4 w-4 text-muted-foreground" />
            <span className="text-sm font-semibold text-foreground">
              规则与记忆
            </span>
          </div>
          <button
            type="button"
            className="flex items-center gap-1 rounded-md px-2 py-1 text-xs text-muted-foreground hover:bg-muted hover:text-foreground"
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

        <div className="min-h-0 flex-1 space-y-2 overflow-y-auto p-3">
          {entries.length === 0 ? (
            <div className="flex h-full min-h-[220px] items-center justify-center rounded-md border border-dashed border-border/70 bg-surface-inset/20 px-6 text-center">
              <p className="max-w-sm text-xs leading-relaxed text-muted-foreground">
                暂无规则。可以在对话中说“以后都这样”由 AI
                提议，或在左侧手动添加。
              </p>
            </div>
          ) : (
            entries.map((entry) => (
              <div
                key={entry.key}
                className={
                  entry.is_active
                    ? "rounded-md border border-border/60 bg-surface-inset/20 px-3 py-2.5"
                    : "rounded-md border border-border/50 bg-muted/20 px-3 py-2.5 opacity-60"
                }
              >
                <div className="flex items-start justify-between gap-3">
                  <div className="min-w-0 flex-1">
                    <div className="flex flex-wrap items-center gap-1.5">
                      <Badge variant="outline" className="text-[10px]">
                        {labelFor(entry.key)}
                      </Badge>
                      <Badge variant="secondary" className="text-[10px]">
                        {entry.source}
                      </Badge>
                      {!entry.is_active ? (
                        <Badge variant="outline" className="text-[10px]">
                          已停用
                        </Badge>
                      ) : null}
                    </div>
                    <p className="mt-2 whitespace-pre-wrap text-xs leading-relaxed text-foreground">
                      {formatProfileEntry(entry)}
                    </p>
                    <p className="mt-2 text-[10px] text-muted-foreground">
                      置信度 {Math.round(entry.confidence * 100)}% ·{" "}
                      {entry.updated_at}
                    </p>
                  </div>
                  <div className="flex shrink-0 gap-1">
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
                </div>
              </div>
            ))
          )}
        </div>
      </div>
    </div>
  );
}
