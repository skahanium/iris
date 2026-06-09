import { Moon, Sun, Puzzle, Sparkles } from "lucide-react";
import { useState } from "react";

import { LlmRoutingSection } from "@/components/settings/LlmRoutingSection";
import { MinimaxSearchSection } from "@/components/settings/MinimaxSearchSection";
import { PersonaSettingsPanel } from "@/components/settings/PersonaSettingsPanel";
import { AiRulesPanel } from "@/components/ai/AiRulesPanel";
import { SkillsPanel } from "@/components/ai/SkillsPanel";
import { Button } from "@/components/ui/button";
import { IrisOverlay } from "@/components/ui/iris-overlay";
import { ScrollArea } from "@/components/ui/scroll-area";

interface SettingsPanelProps {
  open: boolean;
  onClose: () => void;
  theme: "dark" | "light";
  onThemeChange: (theme: "dark" | "light") => void;
}

export function SettingsPanel({
  open,
  onClose,
  theme,
  onThemeChange,
}: SettingsPanelProps) {
  const [skillsOpen, setSkillsOpen] = useState(false);
  const [personaOpen, setPersonaOpen] = useState(false);

  return (
    <>
      <IrisOverlay open={open} onClose={onClose} title="设置" size="command">
        <ScrollArea className="flex-1">
          <div className="space-y-6 px-4 py-4">
            <section>
              <h3 className="mb-2 text-xs font-medium text-foreground">外观</h3>
              <div className="flex gap-2">
                <Button
                  type="button"
                  size="sm"
                  variant={theme === "dark" ? "default" : "outline"}
                  className="gap-1.5"
                  onClick={() => onThemeChange("dark")}
                >
                  <Moon className="h-3.5 w-3.5" />
                  暗色
                </Button>
                <Button
                  type="button"
                  size="sm"
                  variant={theme === "light" ? "default" : "outline"}
                  className="gap-1.5"
                  onClick={() => onThemeChange("light")}
                >
                  <Sun className="h-3.5 w-3.5" />
                  亮色
                </Button>
              </div>
            </section>

            <section data-testid="settings-section-ai-assistant">
              <h3 className="mb-2 text-xs font-medium text-foreground">
                AI 助手
              </h3>
              <p className="mb-2 text-xs text-muted-foreground">
                配置侧栏称呼、头像，以及注入模型的人格与写作风格。
              </p>
              <Button
                type="button"
                size="sm"
                variant="outline"
                className="gap-1.5"
                data-testid="open-persona-settings"
                onClick={() => setPersonaOpen(true)}
              >
                <Sparkles className="h-3.5 w-3.5" />
                打开人格配置
              </Button>
            </section>

            <section data-testid="settings-section-skills">
              <h3 className="mb-2 text-xs font-medium text-foreground">
                Skills 扩展
              </h3>
              <p className="mb-2 text-xs text-muted-foreground">
                安装和管理 Agent Skills，扩展 AI 助手的能力。
              </p>
              <Button
                type="button"
                size="sm"
                variant="outline"
                className="gap-1.5"
                onClick={() => setSkillsOpen(true)}
              >
                <Puzzle className="h-3.5 w-3.5" />
                管理 Skills
              </Button>
            </section>

            <section>
              <h3 className="mb-2 text-xs font-medium text-foreground">
                模型与联网
              </h3>
              <div className="space-y-5">
                <LlmRoutingSection open={open} />
                <MinimaxSearchSection open={open} />
              </div>
            </section>

            <section>
              <h3 className="mb-2 text-xs font-medium text-foreground">
                AI 记忆与规则
              </h3>
              <p className="mb-2 text-xs text-muted-foreground">
                对话中确认的规则会写入此处，并影响后续助手行为。
              </p>
              <div className="max-h-[360px] overflow-hidden rounded-md border border-border">
                <AiRulesPanel compact />
              </div>
            </section>

            <section data-testid="settings-section-about">
              <h3 className="mb-2 text-xs font-medium text-foreground">
                关于 Iris
              </h3>
              <div className="rounded-md border border-border/70 bg-surface-inset/40 px-3 py-2 text-xs leading-5 text-muted-foreground">
                <div className="font-medium text-foreground">Iris</div>
                <div>版本 1.0.0</div>
                <div>Copyright (C) 2026 Iris Contributors</div>
                <div>Licensed under GNU Affero General Public License v3.0</div>
                <div>
                  License: <span className="font-mono">LICENSE</span>
                  <span className="px-1 text-muted-foreground/60">·</span>
                  Source:{" "}
                  <span className="font-mono">
                    https://github.com/skahanium/iris
                  </span>
                </div>
              </div>
            </section>
          </div>
        </ScrollArea>
      </IrisOverlay>
      <PersonaSettingsPanel
        open={personaOpen}
        onClose={() => setPersonaOpen(false)}
      />
      <SkillsPanel open={skillsOpen} onClose={() => setSkillsOpen(false)} />
    </>
  );
}
