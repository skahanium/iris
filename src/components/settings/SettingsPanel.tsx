import { useCallback, useEffect, useState } from "react";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { ScrollArea } from "@/components/ui/scroll-area";
import {
  BING_SEARCH_CREDENTIAL_SERVICE,
  llmCredentialService,
} from "@/lib/credentials";
import {
  credentialDelete,
  credentialHas,
  credentialSet,
  settingsGet,
  settingsSet,
} from "@/lib/ipc";

interface SettingsPanelProps {
  open: boolean;
  onClose: () => void;
  provider: string;
}

export function SettingsPanel({ open, onClose, provider }: SettingsPanelProps) {
  const [llmKeyInput, setLlmKeyInput] = useState("");
  const [bingKeyInput, setBingKeyInput] = useState("");
  const [bingKeyConfigured, setBingKeyConfigured] = useState(false);
  const [customBaseUrl, setCustomBaseUrl] = useState("");

  const refreshBingKeyStatus = useCallback(async () => {
    const has = await credentialHas(BING_SEARCH_CREDENTIAL_SERVICE);
    setBingKeyConfigured(has);
  }, []);

  useEffect(() => {
    if (!open) return;
    void refreshBingKeyStatus();
    void settingsGet<string>("llm_custom_base_url").then((v) => {
      if (v) setCustomBaseUrl(v);
    });
  }, [open, refreshBingKeyStatus]);

  if (!open) return null;

  const saveLlmApiKey = async () => {
    if (!llmKeyInput.trim()) return;
    await credentialSet(llmCredentialService(provider), llmKeyInput.trim());
    setLlmKeyInput("");
  };

  const saveBingApiKey = async () => {
    if (!bingKeyInput.trim()) return;
    await credentialSet(BING_SEARCH_CREDENTIAL_SERVICE, bingKeyInput.trim());
    setBingKeyInput("");
    await refreshBingKeyStatus();
  };

  const clearBingApiKey = async () => {
    await credentialDelete(BING_SEARCH_CREDENTIAL_SERVICE);
    setBingKeyInput("");
    await refreshBingKeyStatus();
  };

  const saveBaseUrl = async () => {
    if (customBaseUrl.trim()) {
      await settingsSet("llm_custom_base_url", customBaseUrl.trim());
    } else {
      await settingsSet("llm_custom_base_url", null);
    }
  };

  return (
    <div className="fixed inset-y-0 right-0 z-50 flex w-80 flex-col border-l border-border bg-panel shadow-xl">
      <div className="flex items-center justify-between border-b border-border px-3 py-2">
        <span className="text-sm font-medium">设置</span>
        <Button type="button" size="sm" variant="ghost" onClick={onClose}>
          Esc
        </Button>
      </div>

      <ScrollArea className="flex-1">
        <div className="space-y-4 p-3">
          <div>
            <label className="mb-1 block text-xs font-medium">
              LLM API Key
            </label>
            <p className="mb-1.5 text-xs text-muted-foreground">
              当前提供商：{provider}。Key 存入系统凭据管理器，不落盘。
            </p>
            <div className="flex gap-2">
              <Input
                type="password"
                placeholder="输入 API Key…"
                value={llmKeyInput}
                onChange={(e) => setLlmKeyInput(e.target.value)}
              />
              <Button
                type="button"
                size="sm"
                onClick={() => void saveLlmApiKey()}
              >
                保存
              </Button>
            </div>
          </div>

          <div>
            <label className="mb-1 block text-xs font-medium">
              联网搜索 API Key
            </label>
            <p className="mb-1.5 text-xs text-muted-foreground">
              Bing Web Search v7。未配置时降级为 DuckDuckGo。
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

          <div>
            <label className="mb-1 block text-xs font-medium">
              自定义 API Base URL
            </label>
            <p className="mb-1.5 text-xs text-muted-foreground">
              仅对「自定义」提供商生效。例如 http://localhost:11434/v1
            </p>
            <div className="flex gap-2">
              <Input
                placeholder="https://api.example.com/v1"
                value={customBaseUrl}
                onChange={(e) => setCustomBaseUrl(e.target.value)}
                onBlur={() => void saveBaseUrl()}
              />
            </div>
          </div>
        </div>
      </ScrollArea>
    </div>
  );
}
