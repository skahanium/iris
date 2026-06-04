import { useCallback, useEffect, useState } from "react";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { invokeErrorMessage } from "@/lib/credentials";
import {
  promptProfileGet,
  promptProfilePresets,
  promptProfileSet,
  type PromptProfileDto,
} from "@/lib/ipc";

export function PromptProfileSection() {
  const [persona, setPersona] = useState("");
  const [writingStyle, setWritingStyle] = useState("");
  const [language, setLanguage] = useState("zh-CN");
  const [rulesText, setRulesText] = useState("");
  const [saved, setSaved] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [presets, setPresets] = useState<
    { label: string; profile: PromptProfileDto }[]
  >([]);

  const load = useCallback(async () => {
    try {
      const p = await promptProfileGet();
      setPersona(p.persona);
      setWritingStyle(p.writing_style);
      setLanguage(p.language);
      setRulesText((p.custom_rules ?? []).join("\n"));
    } catch (e) {
      setError(invokeErrorMessage(e));
    }
  }, []);

  useEffect(() => {
    void load();
    void promptProfilePresets()
      .then(setPresets)
      .catch(() => setPresets([]));
  }, [load]);

  const applyPreset = (profile: PromptProfileDto) => {
    setPersona(profile.persona);
    setWritingStyle(profile.writing_style);
    setLanguage(profile.language);
    setRulesText((profile.custom_rules ?? []).join("\n"));
  };

  const save = async () => {
    setError(null);
    try {
      await promptProfileSet({
        persona,
        writing_style: writingStyle,
        language,
        custom_rules: rulesText
          .split("\n")
          .map((l) => l.trim())
          .filter(Boolean),
      });
      setSaved(true);
      setTimeout(() => setSaved(false), 2000);
    } catch (e) {
      setError(invokeErrorMessage(e));
    }
  };

  return (
    <div className="space-y-3" data-testid="prompt-profile-section">
      <p className="text-xs text-muted-foreground">
        自定义助手人格与写作风格，将注入到 AI 环境提示中。
      </p>
      {presets.length > 0 ? (
        <div className="flex flex-wrap gap-2">
          {presets.map((p) => (
            <Button
              key={p.label}
              type="button"
              variant="outline"
              size="sm"
              className="h-7 text-xs"
              onClick={() => applyPreset(p.profile)}
            >
              {p.label}
            </Button>
          ))}
        </div>
      ) : null}
      <div className="space-y-1.5">
        <span className="text-xs font-medium">人格描述</span>
        <Textarea
          className="min-h-[60px] text-xs"
          value={persona}
          onChange={(e) => setPersona(e.target.value)}
          placeholder="例如：严谨、简洁、偏学术表达"
        />
      </div>
      <div className="space-y-1.5">
        <span className="text-xs font-medium">写作风格</span>
        <Input
          className="h-8 text-xs"
          value={writingStyle}
          onChange={(e) => setWritingStyle(e.target.value)}
        />
      </div>
      <div className="space-y-1.5">
        <span className="text-xs font-medium">回答语言</span>
        <Input
          className="h-8 text-xs"
          value={language}
          onChange={(e) => setLanguage(e.target.value)}
        />
      </div>
      <div className="space-y-1.5">
        <span className="text-xs font-medium">自定义规则（每行一条）</span>
        <Textarea
          className="min-h-[80px] text-xs"
          value={rulesText}
          onChange={(e) => setRulesText(e.target.value)}
        />
      </div>
      {error ? <p className="text-xs text-destructive">{error}</p> : null}
      <Button type="button" size="sm" onClick={() => void save()}>
        {saved ? "已保存" : "保存人格配置"}
      </Button>
    </div>
  );
}
