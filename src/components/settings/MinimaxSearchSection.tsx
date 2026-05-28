import { useCallback, useEffect, useState } from "react";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { isTauri } from "@tauri-apps/api/core";

import {
  invokeErrorMessage,
  MINIMAX_CREDENTIAL_SERVICE,
} from "@/lib/credentials";
import {
  credentialDelete,
  credentialHas,
  credentialSet,
} from "@/lib/ipc";
import {
  minimaxConfigGet,
  minimaxConfigSet,
  minimaxConfigTest,
  notifyLlmConfigChanged,
} from "@/lib/llm-ipc";

type WebSearchBackendOption = "auto" | "minimax" | "duckduckgo";

interface MinimaxSearchSectionProps {
  open: boolean;
}

export function MinimaxSearchSection({ open }: MinimaxSearchSectionProps) {
  const [configured, setConfigured] = useState(false);
  const [apiHost, setApiHost] = useState("https://api.minimaxi.com");
  const [backend, setBackend] = useState<WebSearchBackendOption>("auto");
  const [keyInput, setKeyInput] = useState("");
  const [loading, setLoading] = useState(false);
  const [saving, setSaving] = useState(false);
  const [testing, setTesting] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const [testResult, setTestResult] = useState<{
    ok: boolean;
    message: string;
  } | null>(null);

  const load = useCallback(async () => {
    if (!isTauri()) return;
    setLoading(true);
    setMessage(null);
    try {
      const res = await minimaxConfigGet();
      setApiHost(res.minimaxApiHost);
      setBackend(res.webSearchBackend as WebSearchBackendOption);
      setConfigured(res.minimaxConfigured);
      setKeyInput("");
    } catch (err) {
      setMessage(invokeErrorMessage(err));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    if (open) void load();
  }, [open, load]);

  const saveKey = async () => {
    if (!keyInput.trim()) return;
    setSaving(true);
    setMessage(null);
    setTestResult(null);
    try {
      await credentialSet(MINIMAX_CREDENTIAL_SERVICE, keyInput.trim());
      setConfigured(true);
      setKeyInput("");
      notifyLlmConfigChanged();
      setMessage("MiniMax Key 已保存到系统凭据库");
    } catch (err) {
      setMessage(invokeErrorMessage(err));
    } finally {
      setSaving(false);
    }
  };

  const clearKey = async () => {
    setSaving(true);
    setMessage(null);
    setTestResult(null);
    try {
      await credentialDelete(MINIMAX_CREDENTIAL_SERVICE);
      setConfigured(false);
      notifyLlmConfigChanged();
      setMessage("已清除 MiniMax Key");
    } catch (err) {
      setMessage(invokeErrorMessage(err));
    } finally {
      setSaving(false);
    }
  };

  const savePrefs = async () => {
    setSaving(true);
    setMessage(null);
    try {
      const res = await minimaxConfigSet({
        minimaxApiHost: apiHost.trim(),
        webSearchBackend: backend,
      });
      setApiHost(res.minimaxApiHost);
      setBackend(res.webSearchBackend as WebSearchBackendOption);
      notifyLlmConfigChanged();
      setMessage("检索偏好已保存");
    } catch (err) {
      setMessage(invokeErrorMessage(err));
    } finally {
      setSaving(false);
    }
  };

  const runTest = async () => {
    setTesting(true);
    setTestResult(null);
    setMessage(null);
    try {
      const has = await credentialHas(MINIMAX_CREDENTIAL_SERVICE);
      if (!has && keyInput.trim()) {
        await credentialSet(MINIMAX_CREDENTIAL_SERVICE, keyInput.trim());
        setConfigured(true);
        setKeyInput("");
      }
      if (apiHost.trim()) {
        await minimaxConfigSet({ minimaxApiHost: apiHost.trim() });
      }
      const res = await minimaxConfigTest();
      setTestResult(res);
      notifyLlmConfigChanged();
    } catch (err) {
      setTestResult({ ok: false, message: invokeErrorMessage(err) });
    } finally {
      setTesting(false);
    }
  };

  if (!isTauri()) {
    return null;
  }

  return (
    <section className="space-y-3 border-t border-border/60 pt-4">
      <div>
        <h3 className="text-sm font-medium">MiniMax 联网检索</h3>
        <p className="mt-1 text-xs text-muted-foreground">
          Token Plan Key 仅用于底栏「联网」时的网页摘要；对话主模型仍为
          DeepSeek，见上方 AI 连接。
        </p>
      </div>

      <div className="space-y-2">
        <label className="text-xs font-medium">MiniMax Token Plan API Key</label>
        <Input
          type="password"
          autoComplete="off"
          placeholder={configured ? "已配置（输入新 Key 可覆盖）" : "粘贴 Key"}
          value={keyInput}
          onChange={(e) => setKeyInput(e.target.value)}
          disabled={loading || saving}
        />
        <div className="flex flex-wrap gap-2">
          <Button
            type="button"
            size="sm"
            disabled={saving || !keyInput.trim()}
            onClick={() => void saveKey()}
          >
            保存 Key
          </Button>
          {configured ? (
            <Button
              type="button"
              size="sm"
              variant="outline"
              disabled={saving}
              onClick={() => void clearKey()}
            >
              清除 Key
            </Button>
          ) : null}
          <Button
            type="button"
            size="sm"
            variant="secondary"
            disabled={testing || saving}
            onClick={() => void runTest()}
          >
            {testing ? "测试中…" : "测试连接"}
          </Button>
        </div>
      </div>

      <div className="space-y-2">
        <label className="text-xs font-medium">API Host（国内默认）</label>
        <Input
          value={apiHost}
          onChange={(e) => setApiHost(e.target.value)}
          disabled={loading || saving}
          spellCheck={false}
        />
      </div>

      <div className="space-y-2">
        <label className="text-xs font-medium">检索后端</label>
        <Select
          value={backend}
          onValueChange={(v) => setBackend(v as WebSearchBackendOption)}
          disabled={loading || saving}
        >
          <SelectTrigger className="h-8 text-xs">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="auto">自动（MiniMax 优先，失败降级 DuckDuckGo）</SelectItem>
            <SelectItem value="minimax">仅 MiniMax</SelectItem>
            <SelectItem value="duckduckgo">仅 DuckDuckGo</SelectItem>
          </SelectContent>
        </Select>
      </div>

      <Button
        type="button"
        size="sm"
        variant="outline"
        disabled={saving || loading}
        onClick={() => void savePrefs()}
      >
        保存检索设置
      </Button>

      {testResult ? (
        <p
          className={
            testResult.ok ? "text-xs text-emerald-600" : "text-xs text-destructive"
          }
        >
          {testResult.message}
        </p>
      ) : null}
      {message ? (
        <p className="text-xs text-muted-foreground">{message}</p>
      ) : null}
    </section>
  );
}
