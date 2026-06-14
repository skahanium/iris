import { useCallback, useEffect, useState } from "react";

import { AssistantAvatar } from "@/components/ai/AssistantAvatar";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { IrisOverlay } from "@/components/ui/iris-overlay";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Textarea } from "@/components/ui/textarea";
import { invokeErrorMessage } from "@/lib/credentials";
import {
  promptProfileGet,
  promptProfilePresets,
  type PromptProfileDto,
} from "@/lib/ipc";
import {
  DEFAULT_PROMPT_PROFILE,
  normalizePromptProfile,
  profileToAvatarIdentity,
  sanitizeAvatarEmoji,
  sanitizeDisplayName,
} from "@/lib/prompt-profile";
import { usePromptProfile } from "@/hooks/usePromptProfile";

const EMOJI_PRESETS = ["✨", "📚", "🦉", "🖋️", "🔍", "🧠", "🌿", "⚖️"] as const;

interface PersonaSettingsPanelProps {
  open: boolean;
  onClose: () => void;
}

export function PersonaSettingsBody({ open }: { open: boolean }) {
  const { saveProfile } = usePromptProfile();
  const [draft, setDraft] = useState<PromptProfileDto>(DEFAULT_PROMPT_PROFILE);
  const [rulesText, setRulesText] = useState("");
  const [presets, setPresets] = useState<
    { label: string; profile: PromptProfileDto }[]
  >([]);
  const [saved, setSaved] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

  const loadDraft = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const profile = normalizePromptProfile(await promptProfileGet());
      setDraft(profile);
      setRulesText((profile.custom_rules ?? []).join("\n"));
    } catch (e) {
      setError(invokeErrorMessage(e));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    if (!open) return;
    void loadDraft();
    void promptProfilePresets()
      .then((items) =>
        setPresets(
          items.map((item) => ({
            label: item.label,
            profile: normalizePromptProfile(item.profile),
          })),
        ),
      )
      .catch(() => setPresets([]));
  }, [loadDraft, open]);

  const applyPreset = (profile: PromptProfileDto) => {
    const normalized = normalizePromptProfile(profile);
    setDraft((prev) => ({
      ...normalized,
      display_name: prev.display_name,
      avatar_emoji: normalized.avatar_emoji ?? prev.avatar_emoji,
    }));
    setRulesText((normalized.custom_rules ?? []).join("\n"));
  };

  const handleSave = async () => {
    setError(null);
    try {
      await saveProfile({
        ...draft,
        display_name: sanitizeDisplayName(draft.display_name),
        avatar_emoji: sanitizeAvatarEmoji(draft.avatar_emoji),
        custom_rules: rulesText
          .split("\n")
          .map((line) => line.trim())
          .filter(Boolean),
      });
      setSaved(true);
      setTimeout(() => setSaved(false), 2000);
    } catch (e) {
      setError(invokeErrorMessage(e));
    }
  };

  const handleReset = () => {
    setDraft({ ...DEFAULT_PROMPT_PROFILE });
    setRulesText("");
  };

  const avatarIdentity = profileToAvatarIdentity(draft);

  return (
    <>
      <div className="task-overlay-filter shrink-0 border-b border-border/60 px-4 py-3">
        <p className="text-xs text-muted-foreground">
          称呼与头像显示在 AI 侧栏；人格描述与规则将注入模型 system prompt。
        </p>
      </div>
      <ScrollArea className="task-overlay-results flex-1">
        <div
          className="space-y-6 px-4 py-4"
          data-testid="persona-settings-panel"
        >
          {loading ? (
            <p className="text-xs text-muted-foreground">加载中…</p>
          ) : null}

          <section className="space-y-3">
            <h3 className="text-xs font-medium text-foreground">外观</h3>
            <div className="flex items-center gap-3 rounded-md border border-border bg-surface-inset/30 p-3">
              <AssistantAvatar identity={avatarIdentity} />
              <div className="min-w-0 flex-1 space-y-2">
                <div>
                  <label
                    htmlFor="persona-display-name"
                    className="mb-1 block text-[11px] text-muted-foreground"
                  >
                    称呼
                  </label>
                  <Input
                    id="persona-display-name"
                    className="h-8 text-xs"
                    value={draft.display_name}
                    maxLength={24}
                    placeholder="例如：砚、小鸢"
                    onChange={(e) =>
                      setDraft((prev) => ({
                        ...prev,
                        display_name: e.target.value,
                      }))
                    }
                  />
                </div>
                <div>
                  <label
                    htmlFor="persona-avatar-emoji"
                    className="mb-1 block text-[11px] text-muted-foreground"
                  >
                    头像（emoji，可选）
                  </label>
                  <Input
                    id="persona-avatar-emoji"
                    className="h-8 text-base"
                    value={draft.avatar_emoji ?? ""}
                    maxLength={8}
                    placeholder="留空则使用称呼首字"
                    onChange={(e) =>
                      setDraft((prev) => ({
                        ...prev,
                        avatar_emoji: sanitizeAvatarEmoji(e.target.value),
                      }))
                    }
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
                  variant={draft.avatar_emoji === emoji ? "default" : "outline"}
                  className="h-8 w-8 px-0 text-base"
                  aria-label={`头像 ${emoji}`}
                  onClick={() =>
                    setDraft((prev) => ({ ...prev, avatar_emoji: emoji }))
                  }
                >
                  {emoji}
                </Button>
              ))}
              <Button
                type="button"
                size="sm"
                variant="ghost"
                className="h-8 text-xs"
                onClick={() =>
                  setDraft((prev) => ({ ...prev, avatar_emoji: null }))
                }
              >
                使用称呼首字
              </Button>
            </div>
          </section>

          {presets.length > 0 ? (
            <section className="space-y-2">
              <h3 className="text-xs font-medium text-foreground">人格预设</h3>
              <div className="flex flex-wrap gap-2">
                {presets.map((preset) => (
                  <Button
                    key={preset.label}
                    type="button"
                    variant="outline"
                    size="sm"
                    className="h-8 text-xs"
                    onClick={() => applyPreset(preset.profile)}
                  >
                    {preset.label}
                  </Button>
                ))}
              </div>
            </section>
          ) : null}

          <section className="space-y-3">
            <h3 className="text-xs font-medium text-foreground">行为人格</h3>
            <div className="space-y-1.5">
              <span className="text-xs font-medium text-muted-foreground">
                人格描述
              </span>
              <Textarea
                className="min-h-[72px] text-xs"
                value={draft.persona}
                onChange={(e) =>
                  setDraft((prev) => ({ ...prev, persona: e.target.value }))
                }
                placeholder="留空则使用默认「砚」身份；填写后将覆盖默认行为描述"
              />
            </div>
            <div className="space-y-1.5">
              <span className="text-xs font-medium text-muted-foreground">
                写作风格
              </span>
              <Input
                className="h-8 text-xs"
                value={draft.writing_style}
                onChange={(e) =>
                  setDraft((prev) => ({
                    ...prev,
                    writing_style: e.target.value,
                  }))
                }
              />
            </div>
            <div className="space-y-1.5">
              <span className="text-xs font-medium text-muted-foreground">
                回答语言
              </span>
              <Input
                className="h-8 text-xs"
                value={draft.language}
                onChange={(e) =>
                  setDraft((prev) => ({ ...prev, language: e.target.value }))
                }
              />
            </div>
          </section>

          <section className="space-y-1.5">
            <span className="text-xs font-medium text-foreground">
              自定义规则（每行一条）
            </span>
            <Textarea
              className="min-h-[88px] text-xs"
              value={rulesText}
              onChange={(e) => setRulesText(e.target.value)}
            />
          </section>

          {error ? <p className="text-xs text-destructive">{error}</p> : null}

          <div className="flex gap-2 pb-2">
            <Button type="button" size="sm" onClick={() => void handleSave()}>
              {saved ? "已保存" : "保存人格配置"}
            </Button>
            <Button
              type="button"
              size="sm"
              variant="outline"
              onClick={handleReset}
            >
              恢复默认
            </Button>
          </div>
        </div>
      </ScrollArea>
    </>
  );
}

export function PersonaSettingsPanel({
  open,
  onClose,
}: PersonaSettingsPanelProps) {
  return (
    <IrisOverlay open={open} onClose={onClose} title="人格配置" size="command">
      <PersonaSettingsBody open={open} />
    </IrisOverlay>
  );
}
