import { Moon, Sun } from "lucide-react";
import { useCallback, useEffect, useState } from "react";

import { LlmRoutingSection } from "@/components/settings/LlmRoutingSection";
import { Button } from "@/components/ui/button";
import { IrisOverlay } from "@/components/ui/iris-overlay";
import { Input } from "@/components/ui/input";
import { ScrollArea } from "@/components/ui/scroll-area";
import {
  BING_SEARCH_CREDENTIAL_SERVICE,
  invokeErrorMessage,
} from "@/lib/credentials";
import {
  credentialDelete,
  credentialHas,
  credentialSet,
} from "@/lib/ipc";
import { notifyLlmConfigChanged } from "@/lib/llm-ipc";

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
  const [bingKeyInput, setBingKeyInput] = useState("");
  const [bingKeyConfigured, setBingKeyConfigured] = useState(false);

  const refreshBingKeyStatus = useCallback(async () => {
    const has = await credentialHas(BING_SEARCH_CREDENTIAL_SERVICE);
    setBingKeyConfigured(has);
  }, []);

  useEffect(() => {
    if (!open) return;
    void refreshBingKeyStatus();
  }, [open, refreshBingKeyStatus]);

  const saveBingApiKey = async () => {
    if (!bingKeyInput.trim()) return;
    try {
      await credentialSet(BING_SEARCH_CREDENTIAL_SERVICE, bingKeyInput.trim());
      setBingKeyInput("");
      await refreshBingKeyStatus();
      notifyLlmConfigChanged();
    } catch (err) {
      console.error("Bing Key 保存失败:", invokeErrorMessage(err));
    }
  };

  const clearBingApiKey = async () => {
    await credentialDelete(BING_SEARCH_CREDENTIAL_SERVICE);
    setBingKeyInput("");
    await refreshBingKeyStatus();
    notifyLlmConfigChanged();
  };

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

          <div>
            <label className="mb-1 block text-xs font-medium">
              联网搜索 API Key
            </label>
            <p className="mb-1.5 text-xs text-muted-foreground">
              Bing Web Search v7。未配置时降级为 DuckDuckGo；底栏「搜索 API」圆点反映此状态。
            </p>
            <div className="flex gap-2">
              <Input
                type="password"
                placeholder="Bing API Key…"
                value={bingKeyInput}
                onChange={(e) => setBingKeyInput(e.target.value)}
              />
              <Button
                type="button"
                size="sm"
                onClick={() => void saveBingApiKey()}
              >
                保存
              </Button>
            </div>
            {bingKeyConfigured && (
              <Button
                type="button"
                size="sm"
                variant="ghost"
                className="mt-1.5 h-7 px-2 text-xs"
                onClick={() => void clearBingApiKey()}
              >
                清除 Bing Key
              </Button>
            )}
          </div>
        </div>
      </ScrollArea>
    </IrisOverlay>
  );
}
