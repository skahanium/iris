import { useEffect, useState } from "react";

import { AssistantAvatar } from "@/components/ai/AssistantAvatar";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { useAssistantIdentity } from "@/hooks/useAssistantIdentity";
import {
  DEFAULT_ASSISTANT_IDENTITY,
  sanitizeAvatarEmoji,
  sanitizeDisplayName,
  type AssistantIdentity,
} from "@/lib/assistant-identity";

const EMOJI_PRESETS = ["✨", "📚", "🦉", "🖋️", "🔍", "🧠", "🌿", "⚖️"] as const;

/** 仅在设置面板中编辑；侧栏只读展示 */
export function AssistantIdentitySection() {
  const { identity, setIdentity } = useAssistantIdentity();
  const [draft, setDraft] = useState<AssistantIdentity>(identity);

  useEffect(() => {
    setDraft(identity);
  }, [identity]);

  const commit = () => {
    setIdentity({
      displayName: sanitizeDisplayName(draft.displayName),
      avatarEmoji: sanitizeAvatarEmoji(draft.avatarEmoji),
    });
  };

  const reset = () => {
    setDraft({ ...DEFAULT_ASSISTANT_IDENTITY });
    setIdentity({ ...DEFAULT_ASSISTANT_IDENTITY });
  };

  return (
    <div className="space-y-3" data-testid="settings-assistant-identity">
      <p className="text-xs text-muted-foreground">
        自定义右侧 AI 侧栏的显示名称与头像，仅保存在本机。
      </p>

      <div className="flex items-center gap-3 rounded-md border border-border bg-surface-inset/30 p-3">
        <AssistantAvatar identity={draft} />
        <div className="min-w-0 flex-1 space-y-2">
          <div>
            <label
              htmlFor="assistant-display-name"
              className="mb-1 block text-[11px] text-muted-foreground"
            >
              称呼
            </label>
            <Input
              id="assistant-display-name"
              value={draft.displayName}
              maxLength={24}
              placeholder="例如：小鸢、文献助手"
              onChange={(e) =>
                setDraft((prev) => ({
                  ...prev,
                  displayName: e.target.value,
                }))
              }
              onBlur={commit}
              onKeyDown={(e) => {
                if (e.key === "Enter") commit();
              }}
            />
          </div>
          <div>
            <label
              htmlFor="assistant-avatar-emoji"
              className="mb-1 block text-[11px] text-muted-foreground"
            >
              头像（emoji，可选）
            </label>
            <Input
              id="assistant-avatar-emoji"
              value={draft.avatarEmoji ?? ""}
              maxLength={8}
              placeholder="留空则使用称呼首字"
              className="text-base"
              onChange={(e) =>
                setDraft((prev) => ({
                  ...prev,
                  avatarEmoji: sanitizeAvatarEmoji(e.target.value),
                }))
              }
              onBlur={commit}
            />
          </div>
        </div>
      </div>

      <div className="flex flex-wrap gap-1.5">
        {EMOJI_PRESETS.map((emoji) => (
          <Button
            key={emoji}
            type="button"
            size="sm"
            variant={draft.avatarEmoji === emoji ? "default" : "outline"}
            className="h-8 w-8 px-0 text-base"
            aria-label={`头像 ${emoji}`}
            onClick={() => {
              const next = { ...draft, avatarEmoji: emoji };
              setDraft(next);
              setIdentity(next);
            }}
          >
            {emoji}
          </Button>
        ))}
        <Button
          type="button"
          size="sm"
          variant="ghost"
          className="h-8 text-xs"
          onClick={() => {
            const next = { ...draft, avatarEmoji: null };
            setDraft(next);
            setIdentity(next);
          }}
        >
          使用称呼首字
        </Button>
      </div>

      <div className="flex gap-2">
        <Button type="button" size="sm" onClick={commit}>
          保存
        </Button>
        <Button type="button" size="sm" variant="outline" onClick={reset}>
          恢复默认
        </Button>
      </div>
    </div>
  );
}
