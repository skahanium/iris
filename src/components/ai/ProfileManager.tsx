import { useState, useCallback, useEffect } from "react";
import { Settings2, Plus, Trash2, ToggleLeft, ToggleRight } from "lucide-react";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader } from "@/components/ui/card";
import { ScrollArea } from "@/components/ui/scroll-area";
import {
  profileList,
  profileGet,
  profileSetRule,
  profileDeactivate,
  profileDelete,
} from "@/lib/ipc";

// ─── Types ───────────────────────────────────────────────

interface ProfileEntry {
  key: string;
  value: unknown;
  source: string;
  confidence: number;
  is_active: boolean;
  updated_at: string;
}

// ─── Key Labels ──────────────────────────────────────────

const PROFILE_KEY_LABELS: Record<string, string> = {
  custom_rules: "自定义规则",
  writing_style: "写作风格",
  citation_habits: "引用习惯",
  tool_preferences: "工具偏好",
  model_preferences: "模型偏好",
};

// ─── Component ───────────────────────────────────────────

export function ProfileManager() {
  const [entries, setEntries] = useState<ProfileEntry[]>([]);
  const [showInactive, setShowInactive] = useState(false);
  const [showAdd, setShowAdd] = useState(false);
  const [newKey, setNewKey] = useState("");
  const [newValue, setNewValue] = useState("");

  const refresh = useCallback(async () => {
    try {
      const items = await profileList({
        include_inactive: showInactive,
      });
      setEntries(items as unknown as ProfileEntry[]);
    } catch {
      // ignore
    }
  }, [showInactive]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const handleToggle = useCallback(
    async (entry: ProfileEntry) => {
      if (entry.is_active) {
        await profileDeactivate({ key: entry.key });
      } else {
        const full = await profileGet({ key: entry.key });
        const text =
          typeof full?.value === "object" &&
          full.value !== null &&
          "description" in full.value &&
          typeof (full.value as { description?: unknown }).description ===
            "string"
            ? (full.value as { description: string }).description
            : JSON.stringify(full?.value ?? "");
        await profileSetRule({
          key: entry.key,
          description: text,
          source: "user_reactivate",
        });
      }
      void refresh();
    },
    [refresh],
  );

  const handleDelete = useCallback(
    async (key: string) => {
      await profileDelete({ key });
      void refresh();
    },
    [refresh],
  );

  const handleAdd = useCallback(async () => {
    const text = newValue.trim();
    if (!newKey.trim() || !text) return;
    try {
      if (newValue.trimStart().startsWith("{")) {
        const parsed = JSON.parse(newValue) as { description?: string };
        const desc =
          typeof parsed.description === "string" ? parsed.description : newValue;
        await profileSetRule({
          key: newKey.trim(),
          description: desc,
          source: "user_manual",
        });
      } else {
        await profileSetRule({
          key: newKey.trim(),
          description: text,
          source: "user_manual",
        });
      }
      setNewKey("");
      setNewValue("");
      setShowAdd(false);
      void refresh();
    } catch {
      alert("JSON 格式无效，或直接输入纯文本规则");
    }
  }, [newKey, newValue, refresh]);

  return (
    <div className="flex h-full flex-col">
      {/* Header */}
      <div className="border-b border-border p-3">
        <div className="mb-2 flex items-center gap-2">
          <Settings2 className="h-4 w-4 text-primary" />
          <span className="text-sm font-medium">AI 记忆与规则</span>
        </div>

        <div className="flex items-center gap-2">
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

          <div className="flex-1" />

          <Button
            variant="outline"
            size="sm"
            className="h-7 text-xs"
            onClick={() => setShowAdd(!showAdd)}
          >
            <Plus className="mr-1 h-3 w-3" />
            添加规则
          </Button>
        </div>
      </div>

      {/* Add form */}
      {showAdd && (
        <div className="space-y-2 border-b border-border p-3">
          <select
            value={newKey}
            onChange={(e) => setNewKey(e.target.value)}
            className="w-full rounded-md border border-input bg-background px-2 py-1.5 text-xs"
          >
            <option value="">选择规则类型…</option>
            {Object.entries(PROFILE_KEY_LABELS).map(([key, label]) => (
              <option key={key} value={key}>
                {label}
              </option>
            ))}
            <option value="custom">自定义…</option>
          </select>

          {newKey === "custom" && (
            <input
              type="text"
              placeholder="自定义 key"
              className="w-full rounded-md border border-input bg-background px-2 py-1.5 text-xs"
              onChange={(e) => setNewKey(e.target.value)}
            />
          )}

          <textarea
            value={newValue}
            onChange={(e) => setNewValue(e.target.value)}
            placeholder="规则内容 (JSON 格式)"
            className="w-full rounded-md border border-input bg-background px-2 py-1.5 font-mono text-xs"
            rows={3}
          />

          <div className="flex gap-2">
            <Button
              size="sm"
              className="h-7 text-xs"
              onClick={() => void handleAdd()}
            >
              保存
            </Button>
            <Button
              variant="ghost"
              size="sm"
              className="h-7 text-xs"
              onClick={() => setShowAdd(false)}
            >
              取消
            </Button>
          </div>
        </div>
      )}

      {/* Entries list */}
      <ScrollArea className="flex-1">
        {entries.length === 0 ? (
          <div className="flex h-full flex-col items-center justify-center text-muted-foreground">
            <Settings2 className="mb-2 h-8 w-8 opacity-30" />
            <p className="text-sm">暂无规则或偏好</p>
            <p className="mt-1 text-xs">AI 会在对话中学习并请求确认保存规则</p>
          </div>
        ) : (
          <div className="space-y-2 p-3">
            {entries.map((entry) => (
              <Card
                key={entry.key}
                className={!entry.is_active ? "opacity-50" : ""}
              >
                <CardHeader className="p-2 pb-1">
                  <div className="flex items-center gap-2">
                    <Badge variant="outline" className="text-[10px]">
                      {PROFILE_KEY_LABELS[entry.key] ?? entry.key}
                    </Badge>
                    <Badge
                      variant="secondary"
                      className="text-[10px]"
                      title={`来源: ${entry.source}`}
                    >
                      {entry.source}
                    </Badge>
                    <span className="ml-auto text-[10px] text-muted-foreground">
                      置信度: {Math.round(entry.confidence * 100)}%
                    </span>
                  </div>
                </CardHeader>
                <CardContent className="p-2 pt-0">
                  <div className="whitespace-pre-wrap rounded-md bg-muted p-2 font-mono text-xs">
                    {typeof entry.value === "string"
                      ? entry.value
                      : JSON.stringify(entry.value, null, 2)}
                  </div>
                  <div className="mt-1.5 flex items-center gap-1">
                    <Button
                      variant="ghost"
                      size="icon"
                      className="h-6 w-6"
                      title={entry.is_active ? "停用" : "启用"}
                      onClick={() => void handleToggle(entry)}
                    >
                      {entry.is_active ? (
                        <ToggleRight className="h-3 w-3 text-primary" />
                      ) : (
                        <ToggleLeft className="h-3 w-3" />
                      )}
                    </Button>
                    <Button
                      variant="ghost"
                      size="icon"
                      className="h-6 w-6 text-destructive"
                      title="永久删除"
                      onClick={() => void handleDelete(entry.key)}
                    >
                      <Trash2 className="h-3 w-3" />
                    </Button>
                    <span className="ml-auto text-[10px] text-muted-foreground">
                      {new Date(entry.updated_at).toLocaleDateString()}
                    </span>
                  </div>
                </CardContent>
              </Card>
            ))}
          </div>
        )}
      </ScrollArea>
    </div>
  );
}
