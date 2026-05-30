import { Moon, Sun } from "lucide-react";

import { LlmRoutingSection } from "@/components/settings/LlmRoutingSection";
import { MinimaxSearchSection } from "@/components/settings/MinimaxSearchSection";
import { AiRulesPanel } from "@/components/ai/AiRulesPanel";
import { AssistantIdentitySection } from "@/components/settings/AssistantIdentitySection";
import { PromptProfileSection } from "@/components/settings/PromptProfileSection";
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
  return (
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
            <AssistantIdentitySection />
            <div className="mt-4 border-t border-border/60 pt-4">
              <PromptProfileSection />
            </div>
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
        </div>
      </ScrollArea>
    </IrisOverlay>
  );
}
