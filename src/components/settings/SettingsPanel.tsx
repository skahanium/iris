import { Moon, Sun } from "lucide-react";

import { LlmRoutingSection } from "@/components/settings/LlmRoutingSection";
import { MinimaxSearchSection } from "@/components/settings/MinimaxSearchSection";
import { ProfileManager } from "@/components/ai/ProfileManager";
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
        <div className="space-y-5 px-4 py-4">
          <div>
            <label className="mb-1.5 block text-xs font-medium">外观</label>
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
          </div>

          <LlmRoutingSection open={open} />
          <MinimaxSearchSection open={open} />

          {/* AI 记忆与规则 */}
          <div>
            <label className="mb-1.5 block text-xs font-medium">AI 记忆与规则</label>
            <p className="mb-2 text-xs text-muted-foreground">
              AI 会在对话中学习并请求确认保存规则。规则可在下方查看、停用或删除。
            </p>
            <ProfileManager />
          </div>
        </div>
      </ScrollArea>
    </IrisOverlay>
  );
}
